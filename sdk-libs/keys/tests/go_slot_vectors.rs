//! The key holder derives the slot secret the Go witnesses seal under.

use serde::Deserialize;
use spr_keypair::{ShieldedKeypair, SigningKey, ViewingKey};
use zolana_ring_keys::{RingDeliveryRequest, RingKeys, RotatingRingKeys};

#[derive(Deserialize)]
struct Bodies {
    tx_viewing_sk: String,
    recipient_viewing_sk: String,
    recipient_keys: Vec<String>,
    salt: String,
    first_nullifier: String,
    body_index: u32,
    body_slot_secret: String,
}

fn array<const N: usize>(value: &str) -> [u8; N] {
    hex::decode(value).unwrap().try_into().unwrap()
}

#[test]
fn a_retired_key_answers_the_slot_secret_of_its_body() {
    let vector: Bodies = serde_json::from_str(include_str!(
        "../../../test-vectors/audit/anonymous_bodies.json"
    ))
    .unwrap();
    let recipient = ViewingKey::from_canonical_bytes(&array(&vector.recipient_viewing_sk)).unwrap();
    let viewing_pubkey = recipient.pubkey();
    assert_eq!(
        viewing_pubkey.u(),
        array::<32>(&vector.recipient_keys[vector.body_index as usize])
    );
    let identity = ShieldedKeypair::from_keypair(SigningKey::from_ed25519_bytes(&[7; 32])).unwrap();
    let keys = RotatingRingKeys::new(&identity)
        .unwrap()
        .with_retired(vec![recipient])
        .unwrap();
    let request = |output_index| RingDeliveryRequest {
        viewing_pubkey,
        tx_viewing_pk: ViewingKey::from_canonical_bytes(&array(&vector.tx_viewing_sk))
            .unwrap()
            .pubkey(),
        salt: array(&vector.salt),
        output_index,
        first_nullifier: array(&vector.first_nullifier),
    };
    let secrets = keys
        .ring_delivery_secrets(&[request(vector.body_index), request(vector.body_index + 1)])
        .unwrap();
    assert_eq!(hex::encode(secrets[0]), vector.body_slot_secret);
    assert_ne!(secrets[1], secrets[0]);
}
