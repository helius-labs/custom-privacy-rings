use solana_address::Address;
use spr_indexer_api::Hash;
use zolana_ring_client::rpc::GetListSaltResponse;
use zolana_ring_policy::{Member, MemberError};

use crate::{
    CustomRing, OwnerListSalts, OwnerSaltCheck, PrivateKeyKind, RecordUpdateTarget, RingError,
    REDACTED,
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
    for salt in [LIST_SALT, RING_SALT] {
        assert!(!rendered.contains(&format!("{salt:?}")), "{rendered}");
    }

    let empty = OwnerListSalts::from_response(&GetListSaltResponse {
        list_salt: None,
        ring_salt: None,
    });
    assert!(empty.is_empty());
    assert_eq!(empty.primary_salt(), None);

    let check = OwnerSaltCheck {
        ring: target().ring,
        owner: &OWNER,
        list_salt: &LIST_SALT,
    };
    let check_debug = format!("{check:?}");
    assert!(check_debug.contains(REDACTED), "{check_debug}");
    assert!(
        !check_debug.contains(&format!("{LIST_SALT:?}")),
        "{check_debug}"
    );
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
