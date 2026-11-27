//! The auditor's merge transitions, applied once per confirmed merge and resolved to a fixed point.

use std::collections::{HashMap, HashSet};

use solana_address::Address;
use solana_signature::Signature;
use spr_hasher::primitives::hash_bytes;

use crate::{
    types::{CreatedUtxo, KnownUtxo, UtxoOwner},
    AuditError,
};

/// `(tx_signature, event_index)`, one confirmed merge.
pub type MergeId = (Signature, u16);

/// One confirmed merge, the published count never reads as a count of real inputs.
/// Built only by `confirmed_merge` once its record rebuilds `output_commitment`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfirmedMerge {
    pub(crate) slot: u64,
    pub(crate) tx_signature: Signature,
    pub(crate) event_index: u16,
    pub(crate) output_commitment: [u8; 32],
    pub(crate) tree_id: u16,
    pub(crate) leaf_index: u64,
    pub(crate) first_nullifier: [u8; 32],
    pub(crate) ring_data_hash: [u8; 32],
    pub(crate) disclosed: MergeRecordOutput,
}

#[derive(Clone, PartialEq, Eq)]
pub struct MergeRecordOutput {
    pub owner_key: [u8; 32],
    pub nullifier_pk: [u8; 32],
    pub mint: Address,
    pub amount: u64,
    /// The output's blinding, the record's seed is dropped once it derives.
    pub blinding: [u8; 32],
    /// Published nullifiers no seed derivation names as padding.
    pub real_nullifiers: Vec<[u8; 32]>,
}

zolana_ring_keys::redacted_debug!(MergeRecordOutput {
    shown: [owner_key, nullifier_pk, mint, amount, real_nullifiers],
    hidden: [blinding],
});

/// The fields a merge output inherits, an audit record and no wallet opening.
#[derive(Clone, PartialEq, Eq)]
pub struct AuditedMergeOutput {
    pub(crate) commitment: [u8; 32],
    pub(crate) slot: u64,
    pub(crate) tx_signature: Signature,
    pub(crate) event_index: u16,
    pub(crate) tree_id: u16,
    pub(crate) leaf_index: u64,
    pub(crate) first_nullifier: [u8; 32],
    pub(crate) ring_data_hash: [u8; 32],
    pub(crate) owner_key: [u8; 32],
    pub(crate) owner: UtxoOwner,
    pub(crate) asset: [u8; 32],
    pub(crate) mint: Address,
    pub(crate) amount: u64,
    pub(crate) real_nullifiers: Vec<[u8; 32]>,
    pub(crate) blinding: [u8; 32],
}

zolana_ring_keys::redacted_debug!(AuditedMergeOutput {
    shown: [
        commitment,
        slot,
        tx_signature,
        event_index,
        tree_id,
        leaf_index,
        first_nullifier,
        ring_data_hash,
        owner_key,
        owner,
        asset,
        mint,
        amount,
        real_nullifiers
    ],
    hidden: [blinding],
});

/// Keeps one state per ring.
#[derive(Clone, Debug, Default)]
pub struct MergeLedger {
    applied: HashSet<MergeId>,
    outputs: HashMap<[u8; 32], AuditedMergeOutput>,
}

impl MergeLedger {
    /// A repeated merge changes nothing and returns `None`, a failed one stays unapplied.
    pub fn apply(
        &mut self,
        merge: ConfirmedMerge,
    ) -> Result<Option<AuditedMergeOutput>, AuditError> {
        let id = (merge.tx_signature, merge.event_index);
        if self.applied.contains(&id) {
            return Ok(None);
        }
        let output = merge.output()?;
        self.applied.insert(id);
        self.outputs.insert(output.commitment, output.clone());
        Ok(Some(output))
    }

    pub fn output(&self, commitment: &[u8; 32]) -> Option<&AuditedMergeOutput> {
        self.outputs.get(commitment)
    }

    pub fn outputs(&self) -> impl Iterator<Item = &AuditedMergeOutput> {
        self.outputs.values()
    }
}

impl AuditedMergeOutput {
    pub fn commitment(&self) -> [u8; 32] {
        self.commitment
    }

    /// The record's opening of the output, owned by whichever key the record names.
    pub fn opening(&self) -> Result<KnownUtxo, AuditError> {
        Ok(KnownUtxo {
            owner_key: self.owner_key,
            owner: self.owner,
            asset: self.asset,
            mint: Some(self.mint),
            amount: self.amount,
            blinding: self.blinding,
            data_hash: Some([0; 32]),
            ring_data_hash: Some(self.ring_data_hash),
            tree_id: self.tree_id,
        })
    }

    pub fn created_utxo(&self) -> Result<CreatedUtxo, AuditError> {
        Ok(CreatedUtxo {
            commitment: self.commitment,
            slot_index: 0,
            leaf_index: self.leaf_index,
            utxo: self.opening()?,
            ring_note: true,
        })
    }
}

impl ConfirmedMerge {
    pub fn output_commitment(&self) -> [u8; 32] {
        self.output_commitment
    }

    pub fn disclosed(&self) -> &MergeRecordOutput {
        &self.disclosed
    }

    pub fn output(&self) -> Result<AuditedMergeOutput, AuditError> {
        let record = &self.disclosed;
        Ok(AuditedMergeOutput {
            commitment: self.output_commitment,
            slot: self.slot,
            tx_signature: self.tx_signature,
            event_index: self.event_index,
            tree_id: self.tree_id,
            leaf_index: self.leaf_index,
            first_nullifier: self.first_nullifier,
            ring_data_hash: self.ring_data_hash,
            owner_key: record.owner_key,
            owner: UtxoOwner::Keyed {
                nullifier_pk: record.nullifier_pk,
            },
            asset: hash_bytes(record.mint.as_array())?,
            mint: record.mint,
            amount: record.amount,
            real_nullifiers: record.real_nullifiers.clone(),
            blinding: record.blinding,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_merge_resolves_from_its_record_and_applies_once() {
        let mint = Address::new_from_array([3; 32]);
        let merge = ConfirmedMerge {
            slot: 1,
            tx_signature: Signature::from([1; 64]),
            event_index: 0,
            output_commitment: [20; 32],
            tree_id: 1,
            leaf_index: 1,
            first_nullifier: [1; 32],
            ring_data_hash: [0; 32],
            disclosed: MergeRecordOutput {
                owner_key: [1; 32],
                nullifier_pk: [2; 32],
                mint,
                amount: 9,
                blinding: [0; 32],
                real_nullifiers: vec![[8; 32]],
            },
        };
        let mut ledger = MergeLedger::default();
        assert!(ledger.apply(merge.clone()).expect("applied").is_some());
        assert!(ledger.apply(merge).expect("repeated").is_none());
        let output = ledger.output(&[20; 32]).expect("resolved");
        assert_eq!((output.amount, output.mint), (9, mint));
        assert_eq!(output.asset, hash_bytes(mint.as_array()).unwrap());
        assert_eq!(output.real_nullifiers, [[8; 32]]);
    }
}
