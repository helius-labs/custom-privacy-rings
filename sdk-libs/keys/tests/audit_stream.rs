//! Pins the audit vectors the Go circuits read.

use custom_ring_interface::tracking::ZERO_NULLIFIER_PUBKEY;
use custom_ring_interface::{
    ciphertext_digest, disclosure::SealedKeys, key_catalog::TRANSFER_SHAPES, record_chain,
    AuditKey, AuditStreamLayout, AuditorMessage, CustomRingError, RecordForm, RecordKind,
    RingOutput, SlotBody, ENTRY_RECORD_LEN, EXIT_RECORD_LEN, RING_RECORD_LEN, SEALED_KEY_LEN,
    SENDER_BUNDLE_LEN, STREAM_BLOCK_LEN, TRANSFER_RECORD_LEN,
};
use serde::Deserialize;
use serde_json::{json, Value};
use spr_hasher::{primitives::hash_bytes, Hasher, Poseidon};
use zolana_ring_keys::decode_recipient_keys;
use zolana_ring_keys::encode_recipient_keys;
use zolana_ring_keys::AuditEnvelope;
use zolana_ring_keys::AuditEvidence;
use zolana_ring_keys::AuditorMessageRead;
use zolana_ring_keys::EntryRecord;
use zolana_ring_keys::OutputRecord;
use zolana_ring_keys::RecipientPayload;
use zolana_ring_keys::RecordKey;
use zolana_ring_keys::SealedLayout;
use zolana_ring_keys::SenderBundle;
use zolana_ring_keys::RECORD_ZERO_KEY_BIT;

const VECTORS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../test-vectors/audit/ring-disclosure-ranges.json"
);

fn layout(key: AuditKey, inputs: usize, outputs: usize, capacity: usize) -> AuditStreamLayout {
    AuditStreamLayout {
        key,
        inputs,
        outputs,
        capacity,
        vouched: false,
        records: RecordForm::default(),
    }
}

fn capacity_blocks(layout: AuditStreamLayout) -> usize {
    layout.key.envelope_blocks() + (layout.capacity + 1) * SEALED_KEY_LEN / STREAM_BLOCK_LEN
}

fn vouched(layout: AuditStreamLayout) -> AuditStreamLayout {
    AuditStreamLayout {
        vouched: true,
        ..layout
    }
}

fn key_vector(key: AuditKey, outputs: usize) -> Value {
    let bare = layout(key, 1, outputs, outputs);
    json!({
        "class": key.class(),
        "outputs": outputs,
        "entry": key.entry,
        "envelope_blocks": key.envelope_blocks(),
        "first_key_counter": key.first_key_counter(),
        "capacity_blocks": capacity_blocks(bare),
        "key_start_counters": (0..outputs)
            .map(|slot| bare.recipient_key(slot).start_counter)
            .collect::<Vec<_>>(),
        "envelope_len_by_input_count": (1..=key.class())
            .map(|inputs| layout(key, inputs, outputs, outputs).envelope().len)
            .collect::<Vec<_>>(),
    })
}

fn compute_vector() -> Value {
    json!({
        "description": "Auditor stream ranges for each transfer key at full output capacity, the envelope then one sealed recipient key per slot.",
        "keys": TRANSFER_SHAPES
            .iter()
            .map(|shape| key_vector(shape.audit_key(), usize::from(shape.outputs)))
            .collect::<Vec<_>>(),
    })
}

#[test]
fn the_ranges_are_the_shared_vector() {
    let committed: Value =
        serde_json::from_str(&std::fs::read_to_string(VECTORS).expect("vector file"))
            .expect("vector json");
    assert_eq!(committed, compute_vector());
}

#[test]
#[ignore = "rewrites test-vectors/audit/ring-disclosure-ranges.json"]
fn write_ring_disclosure_ranges_vector() {
    let json = serde_json::to_string_pretty(&compute_vector()).expect("json");
    std::fs::write(VECTORS, json + "\n").expect("write vector file");
}

#[test]
fn the_contract_counts_each_key() {
    let plain = AuditKey {
        class: 24,
        entry: false,
    };
    assert_eq!(layout(plain, 3, 2, 2).envelope().len, 33);
    assert_eq!(plain.first_key_counter(), 5);
    let entry = AuditKey {
        class: 8,
        entry: true,
    };
    assert_eq!(
        layout(entry, 2, 4, 4).envelope().len,
        41 + 2 * ENTRY_RECORD_LEN
    );
    assert_eq!(
        entry.envelope_blocks(),
        (41 + 8 * ENTRY_RECORD_LEN).div_ceil(STREAM_BLOCK_LEN)
    );
    assert_eq!(layout(entry, 2, 3, 4).keys_len(), 3 * SEALED_KEY_LEN);
}

#[test]
fn no_counter_serves_two_regions_and_every_region_holds_its_bytes() {
    for shape in TRANSFER_SHAPES {
        let key = shape.audit_key();
        let full = layout(
            key,
            key.class(),
            usize::from(shape.outputs),
            usize::from(shape.outputs),
        );
        let envelope = full.envelope();
        assert!(envelope.len <= envelope.max_blocks * STREAM_BLOCK_LEN);
        let mut next = envelope.start_counter + envelope.max_blocks as u32;
        for slot in 0..full.capacity {
            let region = full.recipient_key(slot);
            assert_eq!(region.start_counter, next, "{shape:?}");
            assert!(region.len <= region.max_blocks * STREAM_BLOCK_LEN);
            next += region.max_blocks as u32;
        }
        let voucher = vouched(full).voucher();
        assert_eq!(voucher.start_counter, next, "{shape:?}");
        assert_eq!(voucher.len, voucher.max_blocks * STREAM_BLOCK_LEN);
        next += voucher.max_blocks as u32;
        assert_eq!(
            next as usize - envelope.start_counter as usize,
            capacity_blocks(full)
        );
    }
}

#[test]
fn counts_off_the_key_are_refused() {
    let key = AuditKey {
        class: 2,
        entry: false,
    };
    assert!(layout(key, 1, 2, 2).checked().is_ok());
    for refused in [
        layout(key, 0, 2, 2),
        layout(key, 3, 2, 2),
        layout(key, 1, 0, 2),
        layout(key, 1, 3, 2),
    ] {
        assert_eq!(refused.checked(), Err(CustomRingError::RingTransactLength));
    }
}

/// A frame with fewer slots transmits fewer keys, every published key keeps its counter.
#[test]
fn a_compact_frame_keeps_every_key_counter() {
    let key = AuditKey {
        class: 24,
        entry: false,
    };
    let (full, compact) = (layout(key, 2, 4, 4), layout(key, 2, 2, 4));
    for slot in 0..2 {
        assert_eq!(full.recipient_key(slot), compact.recipient_key(slot));
    }
    assert_eq!(compact.recipient_key(2).len, 0);
    assert_eq!(compact.sealed_len(), 33 + 2 * SEALED_KEY_LEN);
}

#[test]
fn a_vouched_frame_adds_one_key_and_moves_no_counter() {
    let key = AuditKey {
        class: 24,
        entry: false,
    };
    let (plain, compact) = (layout(key, 2, 4, 4), layout(key, 2, 2, 4));
    assert_eq!(plain.voucher().len, 0);
    assert_eq!(plain.keys_len(), 4 * SEALED_KEY_LEN);
    for unvouched in [plain, compact] {
        let frame = vouched(unvouched);
        assert_eq!(frame.keys_len(), unvouched.keys_len() + SEALED_KEY_LEN);
        assert_eq!(frame.envelope(), unvouched.envelope());
        for slot in 0..4 {
            assert_eq!(frame.recipient_key(slot), unvouched.recipient_key(slot));
        }
        assert_eq!(
            frame.voucher().start_counter,
            plain.recipient_key(3).start_counter + 2
        );
    }
}

const PLAIN: RecordForm = RecordForm {
    data: false,
    ring_data: false,
};
const DATA: RecordForm = RecordForm {
    data: true,
    ring_data: false,
};
const RING_DATA: RecordForm = RecordForm {
    data: false,
    ring_data: true,
};
const BOTH: RecordForm = RecordForm {
    data: true,
    ring_data: true,
};

fn record() -> OutputRecord {
    OutputRecord {
        kind: RecordKind::Ring,
        amount: 0x0102_0304_0506_0708,
        owner_key: [0xa1; 32],
        mint: [0xa2; 32],
        blinding: [0xa4; 32],
        key: RecordKey::Unpublished,
        data_hash: [0; 32],
        ring_data_hash: [0; 32],
    }
}

fn exit_record(key: RecordKey) -> OutputRecord {
    OutputRecord {
        kind: RecordKind::Exit,
        key,
        ..record()
    }
}

/// A record whose hashes `form` publishes, zero where it publishes none.
fn hashed(record: OutputRecord, form: RecordForm) -> OutputRecord {
    let ring_data = form.ring_data && record.kind != RecordKind::Exit;
    OutputRecord {
        data_hash: [0xd1; 32].map(|byte| byte * u8::from(form.data)),
        ring_data_hash: [0xd2; 32].map(|byte| byte * u8::from(ring_data)),
        ..record
    }
}

#[test]
fn the_record_lays_out_its_contract_bytes() {
    let exit = hashed(exit_record(RecordKey::Carried([0xa3; 32])), DATA).encode(DATA);
    assert_eq!(exit.len(), TRANSFER_RECORD_LEN);
    assert_eq!(exit[0], 2);
    assert_eq!(exit[1..9], 0x0102_0304_0506_0708u64.to_be_bytes());
    assert_eq!(exit[9..41], [0xa1; 32]);
    assert_eq!(exit[41..73], [0xa2; 32]);
    assert_eq!(exit[73..RING_RECORD_LEN], [0xa4; 32]);
    assert_eq!(exit[RING_RECORD_LEN..EXIT_RECORD_LEN], [0xa3; 32]);
    assert_eq!(exit[EXIT_RECORD_LEN..], [0xd1; 32]);
    // A ring slot leads with the data hash only under the data bit.
    let ring = hashed(record(), BOTH).encode(BOTH);
    assert_eq!(ring[RING_RECORD_LEN..EXIT_RECORD_LEN], [0xd1; 32]);
    assert_eq!(ring[EXIT_RECORD_LEN..], [0xd2; 32]);
    let ring = hashed(record(), RING_DATA).encode(RING_DATA);
    assert_eq!(ring[RING_RECORD_LEN..EXIT_RECORD_LEN], [0xd2; 32]);
}

/// One length per kind under each form, a record of any other length is refused.
#[test]
fn every_form_reads_back_at_its_one_length() {
    for (form, ring_len, exit_len) in [
        (PLAIN, 105, 137),
        (DATA, 137, 169),
        (RING_DATA, 137, 137),
        (BOTH, 169, 169),
    ] {
        for (read, len) in [
            (hashed(record(), form), ring_len),
            (
                hashed(
                    OutputRecord {
                        key: RecordKey::Zero,
                        ..record()
                    },
                    form,
                ),
                ring_len,
            ),
            (
                hashed(exit_record(RecordKey::Carried([0xa3; 32])), form),
                exit_len,
            ),
            (hashed(exit_record(RecordKey::Zero), form), exit_len),
            (OutputRecord::default(), ring_len),
        ] {
            let published = read.published(form);
            assert_eq!(published.len(), len, "{form:?} {:?}", read.kind);
            assert_eq!(OutputRecord::decode(&published, form).unwrap(), read);
            for other in [&published[..len - 1], &[&published[..], &[0]].concat()] {
                assert_eq!(
                    OutputRecord::decode(other, form),
                    Err(CustomRingError::RingTransactLength)
                );
            }
        }
    }
}

/// The kind names a zero key, every other key of a non-exit slot stays unpublished.
#[test]
fn the_zero_key_bit_names_the_zero_key() {
    let zero = OutputRecord {
        key: RecordKey::Zero,
        ..record()
    };
    assert_eq!(
        zero.encode(PLAIN)[0],
        RecordKind::Ring as u8 | RECORD_ZERO_KEY_BIT
    );
    assert_eq!(
        exit_record(RecordKey::Carried(ZERO_NULLIFIER_PUBKEY)).encode(PLAIN),
        exit_record(RecordKey::Zero).encode(PLAIN)
    );
    assert_eq!(record().encode(PLAIN)[0], RecordKind::Ring as u8);
}

#[test]
fn a_record_off_its_form_is_refused() {
    let mut unknown = record().published(PLAIN);
    unknown[0] = 3;
    let mut dummy = [0; RING_RECORD_LEN];
    assert_eq!(
        OutputRecord::decode(&dummy, PLAIN).unwrap(),
        OutputRecord::default()
    );
    dummy[73] = 1;
    let mut zero_dummy = [0; RING_RECORD_LEN];
    zero_dummy[0] = RECORD_ZERO_KEY_BIT;
    let exit = exit_record(RecordKey::Carried([0xa3; 32])).published(PLAIN);
    let mut forged_bit = exit.clone();
    forged_bit[0] |= RECORD_ZERO_KEY_BIT;
    let mut dropped_bit = exit_record(RecordKey::Zero).published(PLAIN);
    dropped_bit[0] &= !RECORD_ZERO_KEY_BIT;
    for (name, refused) in [
        ("an unknown kind", &unknown[..]),
        ("a dummy carrying a byte", &dummy[..]),
        ("a dummy naming the zero key", &zero_dummy[..]),
        ("a zero key bit on a carried key", &forged_bit[..]),
        ("a carried zero key without its bit", &dropped_bit[..]),
    ] {
        assert_eq!(
            OutputRecord::decode(refused, PLAIN),
            Err(CustomRingError::RingTransactLength),
            "{name}"
        );
    }
    let mut hashed_dummy = [0; TRANSFER_RECORD_LEN];
    hashed_dummy[TRANSFER_RECORD_LEN - 1] = 1;
    assert_eq!(
        OutputRecord::decode(&hashed_dummy, BOTH),
        Err(CustomRingError::RingTransactLength)
    );
}

fn entry_key() -> AuditKey {
    AuditKey {
        class: 4,
        entry: true,
    }
}

fn entry_record() -> EntryRecord {
    EntryRecord {
        owner_key: [1; 32],
        nullifier_pk: [2; 32],
        amount: 3,
        mint: [4; 32],
        input_commitment: [5; 32],
    }
}

/// R counts notes, default bits and records follow note order.
#[test]
fn envelopes_read_back_by_note_order() {
    let plain = AuditEnvelope {
        tx_viewing_secret: [7; 32],
        real_inputs: 2,
        evidence: AuditEvidence::None,
    };
    let plain_layout = layout(
        AuditKey {
            class: 24,
            entry: false,
        },
        3,
        2,
        2,
    );
    assert_eq!(plain.encode().len(), plain_layout.envelope().len);
    assert_eq!(
        AuditEnvelope::decode(&plain.encode(), plain_layout).unwrap(),
        plain
    );
    let entry = AuditEnvelope {
        tx_viewing_secret: [7; 32],
        real_inputs: 2,
        evidence: AuditEvidence::Entry {
            default_note_bits: 0b01,
            inputs: vec![
                entry_record(),
                EntryRecord::default(),
                EntryRecord::default(),
            ],
        },
    };
    let entry_layout = layout(entry_key(), 3, 2, 4);
    assert_eq!(entry.encode().len(), entry_layout.envelope().len);
    assert_eq!(
        AuditEnvelope::decode(&entry.encode(), entry_layout).unwrap(),
        entry
    );
    let refuse = |envelope: AuditEnvelope, layout: AuditStreamLayout| {
        assert_eq!(
            AuditEnvelope::decode(&envelope.encode(), layout),
            Err(CustomRingError::RingTransactLength)
        );
    };
    refuse(
        AuditEnvelope {
            real_inputs: 4,
            ..plain.clone()
        },
        plain_layout,
    );
    let with_bits = |bits, inputs: Vec<EntryRecord>| AuditEnvelope {
        evidence: AuditEvidence::Entry {
            default_note_bits: bits,
            inputs,
        },
        ..entry.clone()
    };
    refuse(
        with_bits(
            0b100,
            vec![
                EntryRecord::default(),
                EntryRecord::default(),
                entry_record(),
            ],
        ),
        entry_layout,
    );
    refuse(
        with_bits(
            0b01,
            vec![entry_record(), entry_record(), EntryRecord::default()],
        ),
        entry_layout,
    );
}

#[test]
fn recipient_keys_read_back_per_published_slot() {
    let keys = [[1; 32], [2; 32]];
    let key = AuditKey {
        class: 2,
        entry: false,
    };
    let encoded = encode_recipient_keys(&keys);
    assert_eq!(encoded.len(), layout(key, 1, 2, 4).keys_len());
    assert_eq!(
        decode_recipient_keys(&encoded, layout(key, 1, 2, 4)).unwrap(),
        keys
    );
    assert!(decode_recipient_keys(&encoded, layout(key, 1, 1, 4)).is_err());
}

/// An exit's record travels in the auditor message, every other record opens its body.
#[test]
fn the_auditor_message_cuts_by_the_layout_and_the_exits() {
    let key = AuditKey {
        class: 24,
        entry: false,
    };
    let stream = layout(key, 2, 2, 2);
    let mut data = vec![4; 33];
    data.extend([0x11; 33]);
    data.extend([0x22; 2 * SEALED_KEY_LEN]);
    data.extend([0x33; EXIT_RECORD_LEN]);
    let message = AuditorMessage::parse(&data, stream, 1).unwrap();
    assert_eq!(message.envelope, &[0x11; 33]);
    assert_eq!(message.recipient_keys, &[0x22; 64]);
    assert_eq!(message.exit_record(0), Some(&[0x33; EXIT_RECORD_LEN][..]));
    assert_eq!(message.exit_record(1), None);
    assert!(AuditorMessage::parse(&data, stream, 0).is_none());
    assert!(AuditorMessage::parse(&data, stream, 2).is_none());
    // A data frame's exit record carries its data hash.
    let data_stream = AuditStreamLayout {
        records: DATA,
        ..stream
    };
    assert!(AuditorMessage::parse(&data, data_stream, 1).is_none());
    data.extend([0x33; 32]);
    let message = AuditorMessage::parse(&data, data_stream, 1).unwrap();
    assert_eq!(
        message.exit_record(0),
        Some(&[0x33; TRANSFER_RECORD_LEN][..])
    );
    data[0] = 2;
    assert!(AuditorMessage::parse(&data, stream, 1).is_none());
}

#[test]
fn the_auditor_message_holds_the_voucher_key_only_on_a_vouched_layout() {
    let key = AuditKey {
        class: 24,
        entry: false,
    };
    let stream = layout(key, 2, 2, 2);
    let mut data = vec![4; 33];
    data.extend([0x11; 33]);
    data.extend([0x22; 2 * SEALED_KEY_LEN]);
    let split = |data: &[u8], stream: AuditStreamLayout| {
        let message = AuditorMessage::parse(data, stream, 0)?;
        let keys = stream.split_keys(message.recipient_keys)?;
        Some((keys.slots.to_vec(), keys.voucher.copied()))
    };
    assert_eq!(split(&data, stream), Some((vec![0x22; 64], None)));
    assert_eq!(split(&data, vouched(stream)), None);
    data.extend([0x44; SEALED_KEY_LEN]);
    assert_eq!(split(&data, stream), None);
    assert_eq!(
        split(&data, vouched(stream)),
        Some((vec![0x22; 64], Some([0x44; SEALED_KEY_LEN])))
    );
}

/// The payload carries the application data outside every proof, its padding stays zero.
#[test]
fn the_recipient_payload_reads_back_at_one_width() {
    let payload = RecipientPayload {
        data: vec![1, 2, 3],
    };
    let padded = payload.encode_padded(64).unwrap();
    assert_eq!(padded.len(), 64);
    assert_eq!(RecipientPayload::decode(&padded).unwrap(), payload);
    let mut tampered = padded.clone();
    tampered[63] = 1;
    assert!(RecipientPayload::decode(&tampered).is_err());
    assert_eq!(
        payload.encode_padded(payload.encoded_len() - 1),
        Err(CustomRingError::OutputBodyLength)
    );
    let empty = RecipientPayload::default();
    assert_eq!(
        empty.encode_padded(1),
        Err(CustomRingError::OutputBodyLength)
    );
    assert_eq!(
        RecipientPayload::decode(&empty.encode_padded(2).unwrap()).unwrap(),
        empty
    );
}

#[test]
fn the_sender_bundle_holds_the_output_seed_alone() {
    let bundle = SenderBundle {
        output_seed: [1; 32],
    };
    let encoded = bundle.encode();
    assert_eq!(encoded.len(), SENDER_BUNDLE_LEN);
    assert_eq!(SenderBundle::decode(&encoded).unwrap(), bundle);
    assert!(SenderBundle::decode(&[encoded.as_slice(), &[0; 32]].concat()).is_err());
}

#[derive(Deserialize)]
struct StreamVector {
    tx_viewing_sk: String,
    cases: Vec<StreamCase>,
}

#[derive(Deserialize)]
struct StreamCase {
    name: String,
    layout: u8,
    input_class: usize,
    outputs: usize,
    output_count: usize,
    input_count: usize,
    envelope_plaintext: String,
    envelope_ciphertext: String,
    recipient_keys: Vec<String>,
    recipient_keys_ciphertext: String,
    records: Vec<String>,
    record_ciphertexts: Vec<String>,
    envelope_digest: String,
    record_chain: String,
}

fn unhex(value: &str) -> Vec<u8> {
    hex::decode(value).unwrap()
}

#[test]
fn the_solved_go_stream_reads_back_under_the_rust_layout() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test-vectors/audit/audit-stream.json"
    );
    let vector: StreamVector =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        vector
            .cases
            .iter()
            .map(|case| case.name.as_str())
            .collect::<Vec<_>>(),
        [
            "plain_with_exit_and_dummy",
            "compact",
            "data",
            "entry",
            "no_spend",
            "entry_default_first",
            "vouched",
        ]
    );
    for case in &vector.cases {
        let layout = AuditStreamLayout {
            vouched: case.layout & 1 != 0,
            records: RecordForm {
                data: case.layout & 4 != 0,
                ring_data: case.layout & 8 != 0,
            },
            ..layout(
                AuditKey {
                    class: case.input_class,
                    entry: case.layout & 2 != 0,
                },
                case.input_count,
                case.output_count,
                case.outputs,
            )
        }
        .checked()
        .unwrap();
        let plaintext = unhex(&case.envelope_plaintext);
        let envelope = AuditEnvelope::decode(&plaintext, layout).unwrap();
        assert_eq!(
            hex::encode(envelope.tx_viewing_secret),
            vector.tx_viewing_sk
        );
        assert_eq!(envelope.encode(), plaintext, "{}", case.name);
        let keys: Vec<[u8; 32]> = case
            .recipient_keys
            .iter()
            .map(|key| unhex(key).try_into().unwrap())
            .collect();
        assert_eq!(keys.len(), case.output_count, "{}", case.name);
        let (sealed_envelope, sealed_keys) = (
            unhex(&case.envelope_ciphertext),
            unhex(&case.recipient_keys_ciphertext),
        );
        assert_eq!(
            sealed_envelope.len(),
            layout.envelope().len,
            "{}",
            case.name
        );
        assert_eq!(sealed_keys.len(), layout.keys_len(), "{}", case.name);
        let SealedKeys {
            slots: slot_keys,
            voucher,
        } = layout.split_keys(&sealed_keys).unwrap();
        assert_eq!(voucher.is_some(), case.name == "vouched");
        let digest = Poseidon::hashv(&[
            &ciphertext_digest(&sealed_envelope).unwrap(),
            &ciphertext_digest(slot_keys).unwrap(),
            &voucher.map_or([0; 32], |voucher| hash_bytes(voucher).unwrap()),
        ])
        .unwrap();
        assert_eq!(hex::encode(digest), case.envelope_digest, "{}", case.name);
        let ciphertexts: Vec<Vec<u8>> = case
            .record_ciphertexts
            .iter()
            .map(|ciphertext| unhex(ciphertext))
            .collect();
        let mut outputs = Vec::new();
        for (record, ciphertext) in case.records.iter().zip(&ciphertexts) {
            let bytes = unhex(record);
            let decoded = OutputRecord::decode(&bytes, layout.records).unwrap();
            assert_eq!(decoded.published(layout.records), bytes);
            outputs.push(RingOutput {
                hash: &[0; 32],
                owner_tag: [0; 32],
                body: match decoded.kind {
                    RecordKind::Exit => SlotBody::Exit(&[]),
                    RecordKind::Ring | RecordKind::Dummy => SlotBody::Recipient(ciphertext),
                },
                record: ciphertext,
            });
        }
        assert_eq!(
            hex::encode(record_chain(&outputs, case.outputs, layout.records).unwrap()),
            case.record_chain,
            "{}",
            case.name
        );
    }
}

#[test]
fn a_ring_body_opens_with_its_record() {
    use zolana_ring_keys::encode_ring_body_output;
    use zolana_ring_keys::ring_body_output;
    let body = [record().published(PLAIN), vec![0x55; 5]].concat();
    let data = encode_ring_body_output(&body, &[0x66; 97]).unwrap();
    let (read, reply) = ring_body_output(&data).unwrap();
    assert_eq!(read, &body[..]);
    assert_eq!(reply, &[0x66; 97]);
    assert_eq!(
        encode_ring_body_output(&body[..RING_RECORD_LEN - 1], &[]),
        Err(CustomRingError::OutputBodyLength)
    );
}
