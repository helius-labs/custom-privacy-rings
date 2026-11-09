use custom_ring_interface::RingMode;
use solana_signer::Signer;
use spr_hasher::primitives::hash_bytes;
use spr_keypair::{ShieldedKeypair, ViewingKey, X25519Keypair};
use zolana_ring_client::ViewingRotation;

use super::*;
use crate::tests::support::{keypair, ring, Chain, RingSettings, ServedLeaf, TRACKING};

fn rotation<'a>(
    identity: &'a ShieldedKeypair,
    keys: &'a RotatingRingKeys,
    history: &'a ViewingHistory,
) -> RotateViewingKey<'a> {
    RotateViewingKey {
        ring: ring(),
        owner: identity.pubkey(),
        keys,
        history,
    }
}

fn chain(leaves: &[ServedLeaf]) -> RingClient<Chain, Chain> {
    Chain::ring(RingSettings::new(RingMode::Confidential))
        .with_tracking_leaves(leaves)
        .client()
}

#[tokio::test]
async fn a_rotation_names_the_new_key_and_a_current_leaf_needs_none() {
    let identity = keypair(3);
    let address = identity.shielded_address().unwrap();
    let leaf = ServedLeaf::of(&address, [0x06; 32]);
    let client = chain(&[leaf]);
    let start = ViewingRotation {
        keys: RotatingRingKeys::new(&identity).unwrap(),
        history: ViewingHistory::new(identity.viewing_pubkey(), 0),
    };
    let ahead = start.keys.clone().rotate(ViewingKey::new()).unwrap();
    assert!(matches!(
        rotation(&identity, &ahead, &start.history)
            .prepare(&client)
            .await,
        Err(RingError::ViewingHistoryNotCurrent)
    ));
    let current = rotation(&identity, &start.keys, &start.history)
        .prepare(&client)
        .await
        .unwrap();
    assert_eq!(current, None);
    let rotated = start.rotate(ViewingKey::new(), 1).unwrap();
    let new = rotated.keys.address().unwrap().viewing_pubkey;
    let prepared = rotation(&identity, &rotated.keys, &rotated.history)
        .prepare(&client)
        .await
        .unwrap()
        .unwrap();
    let prior_viewing_hash = hash_bytes(&leaf.viewing_u).unwrap();
    assert_eq!(
        prepared.change,
        OwnedChange::Rotate {
            viewing_u: new.u(),
            prior_viewing_hash
        }
    );
    assert_eq!(prepared.tracking_registry, TRACKING);
    assert!(prepared.sign(&identity).unwrap().verify(&TRACKING).is_ok());
}

#[tokio::test]
async fn a_rotation_refuses_an_unenrolled_owner_and_a_leaf_key_the_wallet_never_held() {
    let identity = keypair(3);
    let address = identity.shielded_address().unwrap();
    let start = ViewingRotation {
        keys: RotatingRingKeys::new(&identity).unwrap(),
        history: ViewingHistory::new(identity.viewing_pubkey(), 0),
    };
    let prepare = |leaves: Vec<ServedLeaf>| {
        let client = chain(&leaves);
        let (start, identity) = (&start, &identity);
        async move {
            rotation(identity, &start.keys, &start.history)
                .prepare(&client)
                .await
        }
    };
    assert!(matches!(
        prepare(Vec::new()).await,
        Err(RingError::RotationUnenrolled { .. })
    ));
    let foreign = ServedLeaf {
        viewing_u: X25519Keypair::new().pubkey().u(),
        ..ServedLeaf::of(&address, [0x06; 32])
    };
    assert!(matches!(
        prepare(vec![foreign]).await,
        Err(RingError::ViewingKeyNotHeld { .. })
    ));
}
