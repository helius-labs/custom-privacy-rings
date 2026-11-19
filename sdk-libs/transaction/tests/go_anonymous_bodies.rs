//! Reads `test-vectors/audit/anonymous_bodies.json`, the record ciphertext of a witness the anonymous key solves.

use custom_ring_interface::{RecordForm, RecordKind, RING_RECORD_LEN};
use serde::Deserialize;
use zolana_ring_keys::OutputRecord;
use zolana_ring_keys::RecordKey;
use zolana_ring_keys::SlotSecret;
use zolana_ring_transaction::{open_recipient_body, PayloadRead, RecipientBody};

#[derive(Deserialize)]
struct Vector {
    body_owner_key: String,
    body_blinding: String,
    body_amount: u64,
    body_mint: String,
    body_slot_secret: String,
    body: String,
}

fn array<const N: usize>(value: &str) -> [u8; N] {
    hex::decode(value).unwrap().try_into().unwrap()
}

#[test]
fn the_sealer_and_the_reader_agree_with_the_proven_ciphertext() {
    let vector: Vector = serde_json::from_str(include_str!(
        "../../../test-vectors/audit/anonymous_bodies.json"
    ))
    .unwrap();
    let record = OutputRecord {
        kind: RecordKind::Ring,
        amount: vector.body_amount,
        owner_key: array(&vector.body_owner_key),
        mint: array(&vector.body_mint),
        blinding: array(&vector.body_blinding),
        key: RecordKey::Unpublished,
        ..OutputRecord::default()
    };
    let secret = SlotSecret::new(array(&vector.body_slot_secret)).unwrap();
    let sealed = RecipientBody {
        record: &record,
        form: RecordForm::default(),
        payload: None,
    }
    .seal(&secret)
    .unwrap();
    assert_eq!(hex::encode(sealed), vector.body);
    let ciphertext: [u8; RING_RECORD_LEN] = array(&vector.body);
    let opened = open_recipient_body(&ciphertext, &secret, RecordForm::default()).unwrap();
    assert_eq!(
        (opened.record, opened.payload),
        (record, PayloadRead::Absent)
    );
}
