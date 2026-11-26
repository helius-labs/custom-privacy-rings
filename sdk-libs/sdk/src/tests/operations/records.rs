use custom_ring_interface::domain;
use solana_address::Address;
use spr_indexer_api::Hash;
use zolana_ring_client::rpc::GetListSaltResponse;
use zolana_ring_policy::Member;

use crate::{
    OwnerListSalts, OwnerListTags, PrivateKeyKind, PrivateListMember, RingError, REDACTED,
};

const OWNER: Address = Address::new_from_array([0x11; 32]);
const LIST_SALT: [u8; 32] = [0x12; 32];
const RING_SALT: [u8; 32] = [0x2b; 32];
const OTHER_SALT: [u8; 32] = [0x1d; 32];

fn tags(list_tag: Option<[u8; 32]>, ring_list_tag: [u8; 32]) -> OwnerListTags {
    OwnerListTags {
        owner: OWNER,
        list_tag,
        ring_list_tag,
    }
}

fn salts(list_salt: Option<[u8; 32]>, ring_salt: Option<[u8; 32]>) -> OwnerListSalts {
    OwnerListSalts {
        list_salt,
        ring_salt,
    }
}

#[test]
fn owner_list_salts_debug_redacts_both_salts() {
    let both = OwnerListSalts::from_response(&GetListSaltResponse {
        list_salt: Some(Hash(LIST_SALT)),
        ring_salt: Some(Hash(RING_SALT)),
    });
    assert_eq!(both, salts(Some(LIST_SALT), Some(RING_SALT)));
    let rendered = format!("{both:?}");
    assert!(rendered.contains(REDACTED), "{rendered}");
    for salt in [LIST_SALT, RING_SALT] {
        assert!(!rendered.contains(&format!("{salt:?}")), "{rendered}");
    }
}

#[test]
fn a_clear_on_a_ring_with_a_committed_tag_writes_both_keys_or_refuses() {
    let owner_tag = Member::owner_tag(OWNER.as_array()).unwrap();
    let member = *owner_tag.as_bytes();
    let list_tag = domain::list_tag(&LIST_SALT).unwrap();
    let ring_tag = domain::list_tag(&RING_SALT).unwrap();
    let enrolled = tags(Some(list_tag), ring_tag);

    let revoked = salts(Some(LIST_SALT), Some(RING_SALT))
        .revocation_members(&enrolled)
        .unwrap();
    assert_eq!(
        revoked,
        [
            PrivateListMember {
                member: Member::salted(&owner_tag, &LIST_SALT).unwrap(),
                kind: PrivateKeyKind::OwnerSalted,
                owner: OWNER,
            },
            PrivateListMember {
                member: Member::ring_salted(&owner_tag, &RING_SALT).unwrap(),
                kind: PrivateKeyKind::RingSalted,
                owner: OWNER,
            },
        ]
    );
    assert_eq!(revoked[0].unregistered_owner(), None);
    assert_eq!(revoked[1].unregistered_owner(), Some(OWNER));

    for ring_salt in [None, Some(OTHER_SALT), Some(LIST_SALT), Some([0; 32])] {
        assert!(matches!(
            salts(Some(LIST_SALT), ring_salt).revocation_members(&enrolled),
            Err(RingError::RingSaltUnknown)
        ));
    }
    assert!(matches!(
        salts(None, Some(RING_SALT)).revocation_members(&enrolled),
        Err(RingError::ListSaltUnknown { member: refused }) if refused == member
    ));
    for list_salt in [OTHER_SALT, RING_SALT, [0; 32]] {
        assert!(matches!(
            salts(Some(list_salt), Some(RING_SALT)).revocation_members(&enrolled),
            Err(RingError::ListSaltMismatch { member: refused }) if refused == member
        ));
    }
}

#[test]
fn a_clear_on_a_ring_with_no_committed_tag_writes_the_owner_key_alone() {
    let owner_tag = Member::owner_tag(OWNER.as_array()).unwrap();
    let list_tag = domain::list_tag(&LIST_SALT).unwrap();
    let enrolled = tags(Some(list_tag), [0; 32]);
    for ring_salt in [None, Some(RING_SALT)] {
        let revoked = salts(Some(LIST_SALT), ring_salt)
            .revocation_members(&enrolled)
            .unwrap();
        assert_eq!(revoked.len(), 1);
        assert_eq!(
            revoked[0].member,
            Member::salted(&owner_tag, &LIST_SALT).unwrap()
        );
        assert_eq!(revoked[0].unregistered_owner(), None);
    }
    for unenrolled_salts in [
        salts(None, Some(RING_SALT)),
        salts(Some(LIST_SALT), None),
        salts(None, None),
    ] {
        assert!(matches!(
            unenrolled_salts.revocation_members(&tags(None, [0; 32])),
            Err(RingError::RingSaltUnknown)
        ));
    }
}

#[test]
fn a_set_keys_an_enrolled_owner_by_its_salt_and_any_other_by_the_ring_salt() {
    let owner_tag = Member::owner_tag(OWNER.as_array()).unwrap();
    let list_tag = domain::list_tag(&LIST_SALT).unwrap();
    let ring_tag = domain::list_tag(&RING_SALT).unwrap();
    let both = salts(Some(LIST_SALT), Some(RING_SALT));

    let enrolled = both.write_member(&tags(Some(list_tag), ring_tag)).unwrap();
    assert_eq!(
        enrolled.member,
        Member::salted(&owner_tag, &LIST_SALT).unwrap()
    );
    assert_eq!(enrolled.unregistered_owner(), None);
    assert_eq!(
        salts(Some(LIST_SALT), None)
            .write_member(&tags(Some(list_tag), ring_tag))
            .unwrap(),
        enrolled
    );

    let unenrolled = tags(None, ring_tag);
    let listed = both.write_member(&unenrolled).unwrap();
    assert_eq!(
        listed.member,
        Member::ring_salted(&owner_tag, &RING_SALT).unwrap()
    );
    assert_eq!(listed.unregistered_owner(), Some(OWNER));
    assert_eq!(
        both.revocation_members(&unenrolled).unwrap(),
        [listed],
        "an owner with no row reads the ring key alone"
    );
    // A service naming the ring salt as the owner's salt keys no entry a proof reads.
    for hostile in [salts(Some(RING_SALT), None), salts(None, Some(OTHER_SALT))] {
        assert!(matches!(
            hostile.write_member(&unenrolled),
            Err(RingError::RingSaltUnknown)
        ));
    }
    assert!(matches!(
        salts(Some(RING_SALT), Some(RING_SALT)).write_member(&tags(Some(list_tag), ring_tag)),
        Err(RingError::ListSaltMismatch { .. })
    ));
}
