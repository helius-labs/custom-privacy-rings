//! The ring merge request around its compressed SPP proof point.

#[path = "common/spp_point.rs"]
mod common;

use std::ops::Range;

use custom_ring_interface::{
    storage::Revision, CustomRingError, CustomRingMergeIxDataRef, CustomRingProof,
    PlainGroth16Proof, RecordKind, MERGE_ENVELOPE_LEN, MERGE_RECORD_LEN,
};
use solana_bn254::compression::prelude::alt_bn128_g2_compress_be;
use spr_interface::instruction::{
    instruction_data::{
        merge_ring::{MergeRingIxData, MergeRingIxDataRef},
        merge_transact::{MergeOutputEncryption, MergeProof, MergeTransactIxData},
    },
    tag::RING_MERGE_TRANSACT,
};
use zolana_ring_client::CustomRingMergeIxData;
use zolana_ring_client::SealedMergeRecord;
use zolana_ring_keys::MergeEnvelope;
use zolana_ring_keys::MergeRecord;
use zolana_ring_keys::OutputRecord;
use zolana_ring_keys::RecordKey;
use zolana_ring_keys::MERGE_OUTPUT_LABEL;
use zolana_ring_keys::MERGE_RECORD_LABEL;

const FRAME_VECTOR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../test-vectors/merge/merge_frame.hex"
);

fn merge_body() -> MergeTransactIxData {
    MergeTransactIxData {
        expiry_unix_ts: 42,
        proof: MergeProof {
            a: [0x21; 32],
            b: common::spp_b(),
            c: [0x22; 32],
        },
        output_utxo_hash: [1; 32],
        output_encryption: MergeOutputEncryption::PoseidonCtrV1 {
            salt: [2; 16],
            ciphertext: [[3; 32]; 3],
        },
        private_tx_hash: [4; 32],
        nullifiers: vec![[5; 32]],
        utxo_tree_root_index: 0,
        nullifier_tree_root_index: 0,
        cache_slot: None,
    }
}

fn request() -> CustomRingMergeIxData {
    CustomRingMergeIxData {
        tracking_revision: Revision::from(3),
        output_ring_data_hash: [8; 32],
        sealed: SealedMergeRecord {
            proof: CustomRingProof {
                groth16: PlainGroth16Proof {
                    proof_a: [9; 32],
                    proof_b: [10; 64],
                    proof_c: [11; 32],
                },
                commitment: [12; 32],
                commitment_pok: [13; 32],
            },
            eph_pk: [4; 33],
            envelope: [15; MERGE_ENVELOPE_LEN],
            record: [14; MERGE_RECORD_LEN],
        },
        merge: merge_body(),
    }
}

#[test]
fn the_frame_is_the_shared_vector() {
    let committed = std::fs::read_to_string(FRAME_VECTOR).expect("vector file");
    assert_eq!(
        committed.trim(),
        hex::encode(request().serialize().unwrap())
    );
}

#[test]
#[ignore = "rewrites test-vectors/merge/merge_frame.hex"]
fn write_merge_frame_vector() {
    let line = hex::encode(request().serialize().unwrap()) + "\n";
    std::fs::write(FRAME_VECTOR, line).expect("write vector file");
}

#[test]
fn the_view_rebuilds_the_exact_spp_body_from_the_compressed_point() {
    let request = request();
    let bytes = request.serialize().unwrap();
    let spp_body = request.merge.serialize().unwrap();
    let view = CustomRingMergeIxDataRef::from_bytes(&bytes).unwrap();
    assert_eq!(view.tracking_revision, Revision::from(3));
    assert_eq!(view.output_ring_data_hash, &[8; 32]);
    assert_eq!(view.spp_merge_body().unwrap(), spp_body);
    assert_eq!(
        bytes.len() + 64,
        8 + 32
            + CustomRingProof::SIZE
            + 33
            + MERGE_ENVELOPE_LEN
            + MERGE_RECORD_LEN
            + spp_body.len()
    );
    assert_eq!(view.proof, &request.sealed.proof);
    assert_eq!(view.eph_pk, &request.sealed.eph_pk);
    assert_eq!(view.envelope, &request.sealed.envelope);
    assert_eq!(view.record, &request.sealed.record);
}

/// The forwarded bytes are SPP's own `merge_ring` data.
#[test]
fn the_forward_parses_as_merge_ring() {
    let request = request();
    let bytes = request.serialize().unwrap();
    let view = CustomRingMergeIxDataRef::from_bytes(&bytes).unwrap();
    let forwarded = view.forwarded(&view.spp_merge_body().unwrap());
    assert_eq!(forwarded[0], RING_MERGE_TRANSACT);
    assert_eq!(
        MergeRingIxData::deserialize(&forwarded[1..]).unwrap(),
        MergeRingIxData {
            output_ring_data_hash: request.output_ring_data_hash,
            merge: request.merge.clone(),
        }
    );
    MergeRingIxDataRef::from_bytes(&forwarded[1..]).unwrap();
}

/// The merge record is a record head, its key and the seed, under its own label.
#[test]
fn the_merge_record_is_a_transfer_record_and_its_seed() {
    let merged = MergeRecord {
        record: OutputRecord {
            kind: RecordKind::Ring,
            amount: 12,
            owner_key: [0xa1; 32],
            mint: [0xa2; 32],
            blinding: [0xa5; 32],
            key: RecordKey::Carried([0xa3; 32]),
            ..OutputRecord::default()
        },
        seed: [0xa4; 32],
    };
    let encoded = merged.encode();
    assert_eq!(encoded[0], RecordKind::Ring as u8);
    assert_eq!(encoded[73..105], [0xa5; 32]);
    assert_eq!(encoded[105..137], [0xa3; 32]);
    assert_eq!(encoded[137..], [0xa4; 32]);
    assert_eq!(MergeRecord::decode(&encoded).unwrap(), merged);
    assert!(MergeRecord::decode(&encoded[..168]).is_err());
    let envelope = MergeEnvelope {
        eph_sk: [1; 32],
        recipient_u: [2; 32],
    };
    assert_eq!(MergeEnvelope::decode(&envelope.encode()).unwrap(), envelope);
    assert_eq!(MERGE_RECORD_LABEL, b"CRING/mrg");
    assert_eq!(MERGE_OUTPUT_LABEL, b"CRING/mro");
}

fn compressed_b(bytes: &[u8]) -> Range<usize> {
    let b = alt_bn128_g2_compress_be(&common::spp_b()).unwrap();
    let start = bytes
        .windows(b.len())
        .position(|window| window == b)
        .expect("the request carries b compressed");
    start..start + b.len()
}

#[test]
fn a_compressed_point_off_the_curve_is_refused() {
    let mut bytes = request().serialize().unwrap();
    let b = compressed_b(&bytes);
    bytes[b].fill(0xff);
    let view = CustomRingMergeIxDataRef::from_bytes(&bytes).unwrap();
    assert_eq!(
        view.spp_merge_body(),
        Err(CustomRingError::InvalidSppProofPoint)
    );
}

#[test]
fn framing_errors_are_refused_before_or_by_the_spp_parser() {
    let bytes = request().serialize().unwrap();
    let truncated = &bytes[..compressed_b(&bytes).end - 1];
    for refused in [truncated, &bytes[..39]] {
        assert_eq!(
            CustomRingMergeIxDataRef::from_bytes(refused).err(),
            Some(CustomRingError::InvalidInstructionData)
        );
    }
}
