//! The `Merge` instruction data a client writes, `CustomRingMergeIxDataRef` reads it back.

use custom_ring_interface::{
    merge::MERGE_B_OFFSET, storage::Revision, CustomRingError, CustomRingProof, MERGE_ENVELOPE_LEN,
    MERGE_RECORD_LEN, TAGGED_X25519_KEY_LEN,
};
use spr_interface::instruction::instruction_data::merge_transact::MergeTransactIxData;
use wincode::{SchemaRead, SchemaWrite};

/// A `MergeRecord` of kind `Ring` sealed to the owner's viewing key, and the envelope that opens it for the auditor.
#[derive(Clone, Debug, PartialEq, Eq, SchemaRead, SchemaWrite)]
pub struct SealedMergeRecord {
    pub proof: CustomRingProof,
    pub eph_pk: [u8; TAGGED_X25519_KEY_LEN],
    pub envelope: [u8; MERGE_ENVELOPE_LEN],
    pub record: [u8; MERGE_RECORD_LEN],
}

/// Tag 57, the SPP merge body with `proof.b` compressed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomRingMergeIxData {
    pub tracking_revision: Revision,
    pub output_ring_data_hash: [u8; 32],
    pub sealed: SealedMergeRecord,
    pub merge: MergeTransactIxData,
}

impl CustomRingMergeIxData {
    /// The bytes after the instruction tag.
    pub fn serialize(&self) -> Result<Vec<u8>, CustomRingError> {
        let body = self.merge.serialize().map_err(invalid_data)?;
        let (head, rest) = body
            .split_at_checked(MERGE_B_OFFSET)
            .ok_or(CustomRingError::InvalidInstructionData)?;
        let tail = rest
            .get(self.merge.proof.b.len()..)
            .ok_or(CustomRingError::InvalidInstructionData)?;
        let b = solana_bn254::compression::prelude::alt_bn128_g2_compress_be(&self.merge.proof.b)
            .map_err(|_| CustomRingError::InvalidSppProofPoint)?;
        Ok([
            &self.tracking_revision.0[..],
            &self.output_ring_data_hash,
            &wincode::serialize(&self.sealed).map_err(invalid_data)?,
            head,
            &b,
            tail,
        ]
        .concat())
    }
}

fn invalid_data<E>(_: E) -> CustomRingError {
    CustomRingError::InvalidInstructionData
}
