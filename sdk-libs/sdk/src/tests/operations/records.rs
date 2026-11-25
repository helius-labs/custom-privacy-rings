use solana_address::Address;
use solana_signer::Signer;
use spr_indexer_api::Hash;
use zeroize::{Zeroize, ZeroizeOnDrop};
use zolana_ring_client::rpc::{GetListSaltResponse, RecordUpdateEntry};
use zolana_ring_policy::{ListId, ListSet, Member, MemberError, Writer};

use crate::{
    tests::support::keypair, AuthorityRecordUpdate, CustomRing, OwnerListSalts, OwnerPrivateClear,
    OwnerSaltCheck, PrivateKeyKind, RecordUpdateTarget, RingError, REDACTED,
};

const OWNER: Address = Address::new_from_array([0x11; 32]);
const LIST_SALT: [u8; 32] = [0x12; 32];
const RING_SALT: [u8; 32] = [0x2b; 32];

fn target() -> RecordUpdateTarget {
    RecordUpdateTarget {
        genesis_hash: [0x01; 32],
        ring: CustomRing::new(Address::new_from_array([0x02; 32])),
        registry: Address::new_from_array([0x03; 32]),
        valid_until_slot: 99,
    }
}

#[test]
fn owner_list_salts_preserves_both_salts_and_redacts_debug() {
    let both = OwnerListSalts::from_response(&GetListSaltResponse {
        list_salt: Some(Hash(LIST_SALT)),
        ring_salt: Some(Hash(RING_SALT)),
    });
    assert_eq!(both.list_salt, Some(LIST_SALT));
    assert_eq!(both.ring_salt, Some(RING_SALT));
    assert!(!both.is_empty());
    assert_eq!(both.primary_salt(), Some(&LIST_SALT));

    let rendered = format!("{both:?}");
    assert!(rendered.contains(REDACTED), "{rendered}");
    assert!(!rendered.contains("18"), "{rendered}");
    assert!(!rendered.contains("43"), "{rendered}");

    let empty = OwnerListSalts::from_response(&GetListSaltResponse {
        list_salt: None,
        ring_salt: None,
    });
    assert!(empty.is_empty());
    assert_eq!(empty.primary_salt(), None);

    fn assert_zeroize_on_drop<T: ZeroizeOnDrop>(_: &T) {}
    let mut zeroed = both;
    assert_zeroize_on_drop(&zeroed);
    let clear = OwnerPrivateClear {
        owner: OWNER,
        salts: zeroed.clone(),
        lists: ListSet::single(ListId::PrivateBlock),
    };
    assert_zeroize_on_drop(&clear);
    let clear_debug = format!("{clear:?}");
    assert!(clear_debug.contains(REDACTED), "{clear_debug}");
    assert!(!clear_debug.contains("18"), "{clear_debug}");
    assert!(!clear_debug.contains("43"), "{clear_debug}");
    zeroed.zeroize();
    assert!(zeroed.is_empty());

    let check = OwnerSaltCheck {
        ring: target().ring,
        owner: &OWNER,
        list_salt: &LIST_SALT,
    };
    let check_debug = format!("{check:?}");
    assert!(check_debug.contains(REDACTED), "{check_debug}");
    assert!(!check_debug.contains("18"), "{check_debug}");
}

#[test]
fn revocation_members_returns_every_key_the_circuit_reads() {
    let owner_tag = Member::owner_tag(OWNER.as_array()).unwrap();
    let expected_owner = Member::salted(&owner_tag, &LIST_SALT).unwrap();
    let expected_ring = Member::ring_salted(&owner_tag, &RING_SALT).unwrap();

    let both = OwnerListSalts {
        list_salt: Some(LIST_SALT),
        ring_salt: Some(RING_SALT),
    };
    let primary = both.private_write_member(OWNER).unwrap();
    assert_eq!(primary.member, expected_owner);
    assert_eq!(primary.kind, PrivateKeyKind::OwnerSalted);
    assert_eq!(primary.unregistered_owner(), None);

    let revoked = both.revocation_members(OWNER).unwrap();
    assert_eq!(revoked.len(), 2);
    assert_eq!(revoked[0].member, expected_owner);
    assert_eq!(revoked[0].kind, PrivateKeyKind::OwnerSalted);
    assert_eq!(revoked[0].unregistered_owner(), None);
    assert_eq!(revoked[1].member, expected_ring);
    assert_eq!(revoked[1].kind, PrivateKeyKind::RingSalted);
    assert_eq!(revoked[1].unregistered_owner(), Some(OWNER));

    let owner_only = OwnerListSalts {
        list_salt: Some(LIST_SALT),
        ring_salt: None,
    };
    let revoked_owner = owner_only.revocation_members(OWNER).unwrap();
    assert_eq!(revoked_owner.len(), 1);
    assert_eq!(revoked_owner[0].member, expected_owner);
    assert_eq!(revoked_owner[0].unregistered_owner(), None);

    let ring_only = OwnerListSalts {
        list_salt: None,
        ring_salt: Some(RING_SALT),
    };
    let primary_ring = ring_only.private_write_member(OWNER).unwrap();
    assert_eq!(primary_ring.member, expected_ring);
    assert_eq!(primary_ring.unregistered_owner(), Some(OWNER));
    let revoked_ring = ring_only.revocation_members(OWNER).unwrap();
    assert_eq!(revoked_ring.len(), 1);
    assert_eq!(revoked_ring[0].member, expected_ring);
    assert_eq!(revoked_ring[0].unregistered_owner(), Some(OWNER));
}

#[test]
fn owner_list_salts_refuses_empty_and_zero_salts() {
    let owner_tag = Member::owner_tag(OWNER.as_array()).unwrap();
    let empty = OwnerListSalts::default();
    assert!(matches!(
        empty.private_write_member(OWNER),
        Err(RingError::ListSaltUnknown { member }) if member == *owner_tag.as_bytes()
    ));
    assert!(matches!(
        empty.revocation_members(OWNER),
        Err(RingError::ListSaltUnknown { member }) if member == *owner_tag.as_bytes()
    ));

    for zero in [
        OwnerListSalts {
            list_salt: Some([0; 32]),
            ring_salt: None,
        },
        OwnerListSalts {
            list_salt: None,
            ring_salt: Some([0; 32]),
        },
        OwnerListSalts {
            list_salt: Some(LIST_SALT),
            ring_salt: Some([0; 32]),
        },
        OwnerListSalts {
            list_salt: Some([0; 32]),
            ring_salt: Some(RING_SALT),
        },
    ] {
        assert!(matches!(
            zero.private_write_member(OWNER),
            Err(RingError::Member(MemberError::ZeroSalt))
        ));
        assert!(matches!(
            zero.revocation_members(OWNER),
            Err(RingError::Member(MemberError::ZeroSalt))
        ));
    }
}

#[test]
fn authority_owner_private_clear_clears_all_applicable_keys_and_marks_unregistered_owner() {
    let authority = keypair(9);
    let owner_tag = Member::owner_tag(OWNER.as_array()).unwrap();
    let expected_owner = Member::salted(&owner_tag, &LIST_SALT).unwrap();
    let expected_ring = Member::ring_salted(&owner_tag, &RING_SALT).unwrap();
    let lists = ListSet::of(&[ListId::PrivateAllow, ListId::PrivateBlock]);

    let both = AuthorityRecordUpdate::new(target(), authority.pubkey())
        .with_owner_private_clear(OwnerPrivateClear {
            owner: OWNER,
            salts: OwnerListSalts {
                list_salt: Some(LIST_SALT),
                ring_salt: Some(RING_SALT),
            },
            lists,
        })
        .unwrap()
        .sign(&authority)
        .unwrap();
    both.verify().unwrap();
    assert_eq!(both.unregistered_owners, vec![OWNER.into()]);
    assert_eq!(both.update.entries.len(), 2);
    for (entry, expected) in both
        .update
        .entries
        .iter()
        .zip([expected_owner, expected_ring])
    {
        let RecordUpdateEntry::Write(write) = entry else {
            panic!("write entry");
        };
        assert_eq!(write.member, Hash(*expected.as_bytes()));
        assert_eq!(write.mask, lists.record_bits());
        assert_eq!(write.value, 0);
        assert_eq!(write.mint_policy, None);
    }

    let ring_only = AuthorityRecordUpdate::new(target(), authority.pubkey())
        .with_owner_private_clear(OwnerPrivateClear {
            owner: OWNER,
            salts: OwnerListSalts {
                list_salt: None,
                ring_salt: Some(RING_SALT),
            },
            lists: ListSet::single(ListId::PrivateFrozen),
        })
        .unwrap()
        .sign(&authority)
        .unwrap();
    ring_only.verify().unwrap();
    assert_eq!(ring_only.unregistered_owners, vec![OWNER.into()]);
    assert_eq!(ring_only.update.entries.len(), 1);

    let owner_only = AuthorityRecordUpdate::new(target(), authority.pubkey())
        .with_owner_private_clear(OwnerPrivateClear {
            owner: OWNER,
            salts: OwnerListSalts {
                list_salt: Some(LIST_SALT),
                ring_salt: None,
            },
            lists: ListSet::single(ListId::PrivateAllow),
        })
        .unwrap()
        .sign(&authority)
        .unwrap();
    owner_only.verify().unwrap();
    assert!(owner_only.unregistered_owners.is_empty());
    assert_eq!(owner_only.update.entries.len(), 1);

    let repeated = AuthorityRecordUpdate::new(target(), authority.pubkey())
        .with_owner_private_clear(OwnerPrivateClear {
            owner: OWNER,
            salts: OwnerListSalts {
                list_salt: Some(LIST_SALT),
                ring_salt: Some(RING_SALT),
            },
            lists: ListSet::single(ListId::PrivateAllow),
        })
        .unwrap()
        .with_owner_private_clear(OwnerPrivateClear {
            owner: OWNER,
            salts: OwnerListSalts {
                list_salt: Some(LIST_SALT),
                ring_salt: Some(RING_SALT),
            },
            lists: ListSet::single(ListId::PrivateBlock),
        })
        .unwrap()
        .sign(&authority)
        .unwrap();
    assert_eq!(repeated.unregistered_owners, vec![OWNER.into()]);
}

#[test]
fn authority_owner_private_clear_refuses_invalid_lists_and_salts() {
    let authority = keypair(9);
    let valid_salts = OwnerListSalts {
        list_salt: Some(LIST_SALT),
        ring_salt: Some(RING_SALT),
    };

    assert!(matches!(
        AuthorityRecordUpdate::new(target(), authority.pubkey()).with_owner_private_clear(
            OwnerPrivateClear {
                owner: OWNER,
                salts: valid_salts.clone(),
                lists: ListSet::EMPTY,
            },
        ),
        Err(RingError::EmptyListWrite)
    ));

    assert!(matches!(
        AuthorityRecordUpdate::new(target(), authority.pubkey()).with_owner_private_clear(
            OwnerPrivateClear {
                owner: OWNER,
                salts: valid_salts.clone(),
                lists: ListSet::single(ListId::RingViewing),
            },
        ),
        Err(RingError::ListNotWritable {
            list: ListId::RingViewing,
            writer: Writer::Authority,
        })
    ));

    for list in [
        ListId::Allow,
        ListId::Block,
        ListId::Frozen,
        ListId::Reader,
        ListId::Approval,
    ] {
        assert!(matches!(
            AuthorityRecordUpdate::new(target(), authority.pubkey()).with_owner_private_clear(
                OwnerPrivateClear {
                    owner: OWNER,
                    salts: valid_salts.clone(),
                    lists: ListSet::single(list),
                },
            ),
            Err(RingError::PrivateWritesMismatch)
        ));
    }

    assert!(matches!(
        AuthorityRecordUpdate::new(target(), authority.pubkey()).with_owner_private_clear(
            OwnerPrivateClear {
                owner: OWNER,
                salts: valid_salts,
                lists: ListSet::of(&[ListId::PrivateAllow, ListId::Allow]),
            },
        ),
        Err(RingError::PrivateWritesMismatch)
    ));

    assert!(matches!(
        AuthorityRecordUpdate::new(target(), authority.pubkey()).with_owner_private_clear(
            OwnerPrivateClear {
                owner: OWNER,
                salts: OwnerListSalts::default(),
                lists: ListSet::single(ListId::PrivateAllow),
            },
        ),
        Err(RingError::ListSaltUnknown { .. })
    ));

    assert!(matches!(
        AuthorityRecordUpdate::new(target(), authority.pubkey()).with_owner_private_clear(
            OwnerPrivateClear {
                owner: OWNER,
                salts: OwnerListSalts {
                    list_salt: Some(LIST_SALT),
                    ring_salt: Some([0; 32]),
                },
                lists: ListSet::single(ListId::PrivateAllow),
            },
        ),
        Err(RingError::Member(MemberError::ZeroSalt))
    ));
}
