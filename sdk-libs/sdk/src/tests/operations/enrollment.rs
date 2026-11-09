use custom_ring_interface::RingMode;
use spr_hasher::primitives::hash_bytes;
use spr_keypair::{ShieldedKeypair, X25519Keypair};

use super::*;
use crate::tests::support::{
    keypair, owner_record, ring, Chain, Registry, RingSettings, ServedLeaf, TRACKING,
};
use crate::MemberRecords;

#[tokio::test]
async fn member_records_come_back_in_member_order() {
    let registered = keypair(1);
    let stranger = keypair(2);
    let client = Chain::ring(RingSettings {
        has_policy: true,
        ..RingSettings::new(RingMode::Confidential)
    })
    .with_registry(
        Registry::new(&[(&registered, 1)]),
        &crate::tests::pipeline_rules(),
    )
    .client();
    let reads = MemberRecords {
        ring: ring(),
        members: &[owner_record(&stranger), owner_record(&registered)],
    }
    .read(client.zolana())
    .await
    .unwrap();
    assert!(reads[0].record().is_none());
    assert_eq!(reads[1].record().unwrap().lists, 1);
}

const SALT: [u8; 32] = [8; 32];

fn held(owner: &ShieldedKeypair) -> ServedLeaf {
    ServedLeaf::sealed(owner.pubkey(), &owner.nullifier_key, &SALT)
}

async fn read(chain: Chain, owner: Address) -> Option<EnrollIxData> {
    EnrollmentRead {
        tracking_registry: TRACKING,
        owner,
    }
    .read(chain.client().zolana())
    .await
    .unwrap()
}

#[tokio::test]
async fn an_enrollment_reads_the_leaf_and_nothing_without_one() {
    let owner = keypair(70);
    let leaf = held(&owner);
    let chain = || Chain::ring(RingSettings::new(RingMode::Confidential));
    assert_eq!(read(chain(), owner.pubkey()).await, None);
    let applied = chain().with_tracking_leaves(&[leaf]);
    assert_eq!(read(applied, owner.pubkey()).await, Some(leaf.enroll()));
}

fn enroll(owner: &ShieldedKeypair, viewing_u: [u8; 32]) -> Enroll<'_> {
    Enroll {
        ring: ring(),
        owner: owner.pubkey(),
        nullifier_key: &owner.nullifier_key,
        viewing_pubkey: spr_keypair::X25519Pubkey::from_u(viewing_u).unwrap(),
        list_salt: SALT,
    }
}

/// A leaf fixed the key, tag and seal, only the viewing key moves.
#[tokio::test]
async fn an_enrolled_owner_proves_a_rotation_and_another_key_is_refused() {
    let owner = keypair(72);
    let leaf = held(&owner);
    let client = Chain::ring(RingSettings::new(RingMode::Confidential))
        .with_tracking_leaves(&[leaf])
        .client();
    let viewing_u = X25519Keypair::new().pubkey().u();
    let rotation = enroll(&owner, viewing_u).prove(&client).await.unwrap();
    let prior_viewing_hash = hash_bytes(&leaf.viewing_u).unwrap();
    assert_eq!(
        rotation.change,
        OwnedChange::Rotate {
            viewing_u,
            prior_viewing_hash
        }
    );
    assert_eq!(rotation.tracking_registry, TRACKING);
    let stranger = keypair(73);
    let other_key = ServedLeaf::sealed(owner.pubkey(), &stranger.nullifier_key, &SALT);
    let client = Chain::ring(RingSettings::new(RingMode::Confidential))
        .with_tracking_leaves(&[other_key])
        .client();
    assert!(matches!(
        enroll(&owner, viewing_u).prove(&client).await,
        Err(RingError::AlreadyEnrolled { owner: refused }) if refused == owner.pubkey()
    ));
}

/// The request carries the owner's signature over the registry, a flipped byte fails it.
#[test]
fn only_the_owner_signs_its_write_and_the_signature_binds_every_byte() {
    let owner = keypair(74);
    let enrollment = OwnerEnrollment {
        ring: ring(),
        tracking_registry: TRACKING,
        owner: owner.pubkey(),
        change: OwnedChange::Rotate {
            viewing_u: X25519Keypair::new().pubkey().u(),
            prior_viewing_hash: [9; 32],
        },
    };
    assert!(matches!(
        enrollment.sign(&keypair(75)),
        Err(RingError::EnrollmentSigner { owner: refused }) if refused == owner.pubkey()
    ));
    let request = enrollment.sign(&owner).unwrap();
    assert!(request.verify(&TRACKING).is_ok());
    assert!(request.verify(&Address::new_unique()).is_err());
    let mut flipped = request.clone();
    let last = flipped.write.0.len() - 1;
    flipped.write.0[last] ^= 1;
    assert!(flipped.verify(&TRACKING).is_err());
}
