use core::fmt::Debug;

use custom_ring_interface::RecordKind;
use solana_address::Address;
use spr_keypair::{ShieldedKeypair, SigningKey};
use zolana_ring_keys::{
    AuditEnvelope, AuditEvidence, MergeEnvelope, MergeRecord, OutputRecord, RecordKey, RingAddress,
    RingPaymentRequest,
};

/// Fills every secret below, its decimal and hex forms appear in no other field.
const SECRET: u8 = 0xd0;

fn assert_redacted(value: &impl Debug) {
    let printed = format!("{value:?}");
    for form in [
        SECRET.to_string(),
        format!("{SECRET:02x}"),
        format!("{SECRET:02X}"),
    ] {
        assert!(!printed.contains(&form), "{printed}");
    }
}

fn record() -> OutputRecord {
    OutputRecord {
        kind: RecordKind::Ring,
        amount: 1,
        owner_key: [1; 32],
        mint: [2; 32],
        blinding: [SECRET; 32],
        key: RecordKey::Carried([3; 32]),
        data_hash: [4; 32],
        ring_data_hash: [5; 32],
    }
}

fn address() -> RingAddress {
    let recipient = ShieldedKeypair::from_keypair(SigningKey::from_ed25519_bytes(&[7; 32]))
        .unwrap()
        .shielded_address()
        .unwrap();
    RingAddress {
        recipient,
        ring_program_id: Address::new_from_array([0x11; 32]),
        list_salt: [SECRET; 32],
    }
}

#[test]
fn an_output_record_prints_no_blinding() {
    assert_redacted(&record());
}

#[test]
fn an_audit_envelope_prints_no_transaction_viewing_secret() {
    assert_redacted(&AuditEnvelope {
        tx_viewing_secret: [SECRET; 32],
        real_inputs: 1,
        evidence: AuditEvidence::None,
    });
}

#[test]
fn a_merge_record_prints_neither_its_seed_nor_its_blinding() {
    assert_redacted(&MergeRecord {
        record: record(),
        seed: [SECRET; 32],
    });
}

#[test]
fn a_merge_envelope_prints_no_ephemeral_secret() {
    assert_redacted(&MergeEnvelope {
        eph_sk: [SECRET; 32],
        recipient_u: [6; 32],
    });
}

#[test]
fn a_payment_request_prints_neither_its_list_salt_nor_its_tag() {
    let request = RingPaymentRequest {
        tag: Some([SECRET; 32]),
        ..RingPaymentRequest::new(address())
    };
    assert_redacted(&request.address);
    assert_redacted(&request);
}
