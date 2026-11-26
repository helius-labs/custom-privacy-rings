//! The `Merge` data and its statement.
//!
//! A frame carries SPP's merge body with `proof.b` compressed, and the program forwards it decompressed.
//! SPP publishes no auditor message for a merge, so the auditor envelope and the record sealed to the
//! owner's viewing key travel in the ring instruction alone and only the statement binds them.
//!
//! Read after `program-libs/interface/src/proof/statement.rs`. Read before
//! `programs/custom-ring/src/instructions/settlement/merge.rs`, `prover/circuits/merge.go`.

use crate::{
    bytes::Bytes, error::CustomRingError, storage::Revision, CustomRingProof, MERGE_ENVELOPE_LEN,
    MERGE_RECORD_LEN, TAGGED_X25519_KEY_LEN,
};
use spr_hasher::{
    hash_chain::create_hash_chain_from_slice,
    primitives::{hash_bytes, pack_be, right_align},
    HasherError,
};
use spr_interface::instruction::{
    instruction_data::merge_transact::MergeOutputEncryption, tag::RING_MERGE_TRANSACT,
};

pub const MERGE_PUBLIC_INPUT_ELEMENTS: usize = 16;
/// `expiry_unix_ts u64 || proof.a` open the SPP merge body ahead of `proof.b`.
pub const MERGE_B_OFFSET: usize = core::mem::size_of::<u64>() + 32;

/// Borrowed view of the tag 57 request.
pub struct CustomRingMergeIxDataRef<'a> {
    pub tracking_revision: Revision,
    pub output_ring_data_hash: &'a [u8; 32],
    pub proof: &'a CustomRingProof,
    pub eph_pk: &'a [u8; TAGGED_X25519_KEY_LEN],
    pub envelope: &'a [u8; MERGE_ENVELOPE_LEN],
    pub record: &'a [u8; MERGE_RECORD_LEN],
    body: SppMergeBody<'a>,
}

impl<'a> CustomRingMergeIxDataRef<'a> {
    pub fn from_bytes(data: &'a [u8]) -> Result<Self, CustomRingError> {
        let mut bytes = Bytes::new(data, CustomRingError::InvalidInstructionData);
        Ok(Self {
            tracking_revision: *bytes.pod()?,
            output_ring_data_hash: bytes.array()?,
            proof: bytes.pod()?,
            eph_pk: bytes.array()?,
            envelope: bytes.array()?,
            record: bytes.array()?,
            body: SppMergeBody::read(bytes)?,
        })
    }

    /// The SPP merge body the program forwards, `proof.b` decompressed, SPP's parser reads the rest.
    pub fn spp_merge_body(&self) -> Result<Vec<u8>, CustomRingError> {
        self.body.decompressed()
    }

    /// `merge_ring` data over `spp_body`.
    pub fn forwarded(&self, spp_body: &[u8]) -> Vec<u8> {
        [
            &[RING_MERGE_TRANSACT][..],
            self.output_ring_data_hash,
            spp_body,
        ]
        .concat()
    }
}

/// An SPP merge body around its compressed `proof.b`.
pub(crate) struct SppMergeBody<'a> {
    head: &'a [u8; MERGE_B_OFFSET],
    compressed_b: &'a [u8; 64],
    tail: &'a [u8],
}

impl<'a> SppMergeBody<'a> {
    pub(crate) fn read(mut bytes: Bytes<'a>) -> Result<Self, CustomRingError> {
        Ok(Self {
            head: bytes.array()?,
            compressed_b: bytes.array()?,
            tail: bytes.rest(),
        })
    }

    pub(crate) fn decompressed(&self) -> Result<Vec<u8>, CustomRingError> {
        let b = groth16_solana::decompression::decompress_g2(self.compressed_b)
            .map_err(|_| CustomRingError::InvalidSppProofPoint)?;
        Ok([&self.head[..], &b, self.tail].concat())
    }
}

/// The merge statement, `prover/circuits/merge.go`.
pub struct MergePublicInput<'a> {
    pub output_utxo_hash: &'a [u8; 32],
    pub first_nullifier: &'a [u8; 32],
    /// `hash_bytes` of the ring program id, the value the merged notes commit.
    pub ring_id: &'a [u8; 32],
    pub auditor_pk: &'a [u8; TAGGED_X25519_KEY_LEN],
    pub eph_pk: &'a [u8; TAGGED_X25519_KEY_LEN],
    pub envelope: &'a [u8; MERGE_ENVELOPE_LEN],
    pub record: &'a [u8; MERGE_RECORD_LEN],
    /// The payer's Solana owner identity.
    pub payer: &'a [u8; 32],
    pub tracking_root: &'a [u8; 32],
    /// SPP publishes it unproven, the ring proof alone binds it to the output.
    pub output_encryption: &'a MergeOutputEncryption,
}

impl MergePublicInput<'_> {
    pub fn hash(&self) -> Result<[u8; 32], HasherError> {
        create_hash_chain_from_slice(&self.elements()?)
    }

    pub fn elements(&self) -> Result<[[u8; 32]; MERGE_PUBLIC_INPUT_ELEMENTS], HasherError> {
        let [auditor_lo, auditor_hi] = pack_be(self.auditor_pk);
        let [eph_lo, eph_hi] = pack_be(self.eph_pk);
        let MergeOutputEncryption::PoseidonCtrV1 { salt, ciphertext } = self.output_encryption;
        let [amount, mint_lo, mint_hi] = *ciphertext;
        Ok([
            crate::domain::field(crate::domain::MERGE_STATEMENT),
            *self.output_utxo_hash,
            *self.first_nullifier,
            *self.ring_id,
            auditor_lo,
            auditor_hi,
            eph_lo,
            eph_hi,
            hash_bytes(self.envelope)?,
            hash_bytes(self.record)?,
            *self.payer,
            *self.tracking_root,
            right_align(salt),
            amount,
            mint_lo,
            mint_hi,
        ])
    }
}
