//! The tracking tree's leaf, one indexed tree per ring holding every enrolled key.
//!
//! Read after `program-libs/member-registry/src/tree.rs`, `program-libs/interface/src/accounts/tracking.rs`.
//! Read before `prover/circuits/registry/tracking.go`, `prover/circuits/registry/apply_enrollments.go`.
//!
//! Invariants, `ApplyEnrollmentsCircuit` holds each.
//! 1. A proven entry of an absent owner inserts, an unproven one changes nothing, so no rotation creates a row.
//! 2. An entry of a present owner moves only `viewing_hash`. The nullifier key, ciphertext hash and list tag stay as first inserted, an auditor's opened secrets keep matching them.
//! 3. An insert takes the next empty leaf, so a full tree refuses a new owner and still applies updates.

use custom_ring_interface::{
    chain_link, domain, tracking::TrackingEntry, ApplyEnrollmentsStatement, FIELD_MAX,
};
use spr_hasher::{Hasher, HasherError, Poseidon};

use crate::{
    memory::MemoryTree,
    record::check_member,
    record_pool::RecordPoolOf,
    tree::{ApplyProofInputs, IndexedLeaf, RecordOpening, WritableLeaf, WriteStep},
    QuadPath, RecordTreeError,
};

/// One enrolled owner, a zero `nullifier_pk` marks the sentinel and an unwritten leaf.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrackingRecord {
    pub member: [u8; 32],
    pub next: [u8; 32],
    pub nullifier_pk: [u8; 32],
    pub ct_hash: [u8; 32],
    pub list_tag: [u8; 32],
    /// `hash_bytes(viewing_u)`, an X25519 u can exceed the field and no circuit decides on it.
    pub viewing_hash: [u8; 32],
}

pub type TrackingTree = MemoryTree<TrackingRecord>;
pub type TrackingOpening = RecordOpening<TrackingRecord>;
pub type TrackingPool = RecordPoolOf<TrackingRecord>;

/// One entry's proof input, mirror `EnrollmentWrite` of
/// `prover/circuits/registry/apply_enrollments.go`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackingWriteProofInput {
    pub entry: TrackingEntry,
    /// `low` is the owner's own leaf.
    pub present: bool,
    /// The owner's leaf when present, else the record bracketing it, before the write.
    pub low: TrackingRecord,
    pub low_path: QuadPath,
    /// The `next_index` leaf after the relink on an insert, else index 0 with zero siblings.
    pub slot: QuadPath,
}

impl TrackingRecord {
    /// Leaf 0, every member sorts strictly between it and `FIELD_MAX`.
    pub const SENTINEL: Self = Self {
        member: [0; 32],
        next: FIELD_MAX,
        nullifier_pk: [0; 32],
        ct_hash: [0; 32],
        list_tag: [0; 32],
        viewing_hash: [0; 32],
    };

    /// `Poseidon(CRTK, member, next, nullifier_pk, ct_hash, list_tag, viewing_hash)`, mirror
    /// `prover/circuits/registry/tracking.go`.
    pub fn hash(&self) -> Result<[u8; 32], HasherError> {
        Poseidon::hashv(&[
            &domain::field(domain::TRACKING_LEAF),
            &self.member,
            &self.next,
            &self.nullifier_pk,
            &self.ct_hash,
            &self.list_tag,
            &self.viewing_hash,
        ])
    }

    pub fn brackets(&self, member: &[u8; 32]) -> bool {
        self.member < *member && *member < self.next
    }
}

impl IndexedLeaf for TrackingRecord {
    const SENTINEL: Self = Self::SENTINEL;

    fn member(&self) -> &[u8; 32] {
        &self.member
    }

    fn next(&self) -> &[u8; 32] {
        &self.next
    }

    fn relinked(self, next: [u8; 32]) -> Self {
        Self { next, ..self }
    }

    fn leaf_hash(&self) -> Result<[u8; 32], HasherError> {
        self.hash()
    }

    fn is_blank(&self) -> bool {
        self.nullifier_pk == [0; 32]
    }

    fn blank(member: [u8; 32], next: [u8; 32]) -> Self {
        Self {
            member,
            next,
            ..Self::default()
        }
    }
}

impl WritableLeaf for TrackingRecord {
    type Entry = TrackingEntry;
    type WriteProof = TrackingWriteProofInput;

    fn entry_member(entry: &TrackingEntry) -> &[u8; 32] {
        &entry.member
    }

    fn check_entry(entry: &TrackingEntry) -> Result<(), RecordTreeError> {
        check_member(&entry.member)?;
        // A rotation carries no sealed fields, only a proven entry inserts them.
        if entry.proven && entry.nullifier_pk == [0; 32] {
            return Err(RecordTreeError::TrackingKeyChanged);
        }
        Ok(())
    }

    fn chained(previous: &[u8; 32], entry: &TrackingEntry) -> Result<[u8; 32], HasherError> {
        chain_link(previous, &entry.hash()?)
    }

    /// A queued entry never refuses, a refusal at apply would block every later entry.
    fn after(self, entry: &TrackingEntry) -> Result<Self, RecordTreeError> {
        Ok(match (self.is_blank(), entry.proven) {
            (false, _) => Self {
                viewing_hash: entry.viewing_hash,
                ..self
            },
            (true, true) => Self {
                nullifier_pk: entry.nullifier_pk,
                ct_hash: entry.ct_hash,
                list_tag: entry.list_tag,
                viewing_hash: entry.viewing_hash,
                ..self
            },
            (true, false) => self,
        })
    }

    /// The circuit opens `slot` only on an insert.
    fn write_proof(entry: TrackingEntry, step: WriteStep<Self>) -> TrackingWriteProofInput {
        let insert = !step.present && entry.proven;
        TrackingWriteProofInput {
            entry,
            present: step.present,
            low: step.low,
            low_path: step.low_path,
            slot: match insert {
                true => step.slot,
                false => QuadPath::default(),
            },
        }
    }
}

impl ApplyProofInputs<TrackingWriteProofInput> {
    pub fn statement(&self) -> ApplyEnrollmentsStatement {
        ApplyEnrollmentsStatement {
            old_root: self.old_root,
            new_root: self.new_root,
            old_next_index: self.old_next_index,
            new_next_index: self.new_next_index,
            chain_start: self.chain_start,
            chain_end: self.chain_end,
            count: self.writes.len() as u64,
        }
    }
}
