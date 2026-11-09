//! Applies queued enrollments to a ring's tracking tree.
//!
//! Enrollment never waits on an authority. Anyone may apply, so the program chains the entry hashes the
//! queue stored at `Enroll` and the applier chooses only how many. A stale proof fails and the entries
//! stay queued for the next applier.
//!
//! Read after `programs/storage/src/instructions/enroll.rs`, `program-libs/interface/src/accounts/tracking.rs`.
//! Read before `prover/circuits/registry/apply_enrollments.go`.
//!
//! Guarantees:
//! 1. The new root and next index follow from the current ones by exactly the next `count` queued entries in sequence order, and the next index stays within `QUAD_CAPACITY`.
//! 2. Each entry applies once, `head` moves past every entry the chain absorbed.
//! 3. The outgoing root keeps its full grace. An apply that would evict an unexpired root refuses, and the entries stay queued.
//!
//! Relies on:
//! 1. `ApplyEnrollmentsCircuit` keeps the tree sorted, inserts only proven entries of absent owners and moves only the viewing hash of a present owner.
//! 2. The ring pins the registry by address.

use custom_ring_interface::{
    chain_link,
    key_catalog::RingCircuit,
    storage::StorageError,
    storage_instruction::ApplyEnrollmentsIxData,
    tracking::{tracking_chain_start, TrackingRootUpdate},
    verifying_key, ApplyEnrollmentsStatement, RingProof,
};
use pinocchio::{
    sysvars::{clock::Clock, Sysvar},
    AccountView, Address, ProgramResult,
};

use crate::{
    loader::{load_enrollment_queue_mut, load_tracking_registry_mut},
    parse,
};

pub fn process_apply_enrollments_ix(
    program_id: &Address,
    accounts: &mut [AccountView],
    data: &[u8],
) -> ProgramResult {
    let ix: &ApplyEnrollmentsIxData = parse(data)?;
    let [registry_account, queue_account, ..] = accounts else {
        return Err(StorageError::MissingAccounts.into());
    };
    // 1. One to `APPLY_BATCH` entries, and the chain starts from the registry and revision the proof names.
    ApplyEnrollmentsIxData::check_count(ix.count)?;
    let count = u64::from(ix.count);
    let registry_address = *registry_account.address();
    let chain_start = tracking_chain_start(&registry_address, ix.revision)?;
    // 2. The chain folds the stored hashes from `head`, so the applier cannot skip, reorder or substitute an entry.
    let mut queue = load_enrollment_queue_mut(program_id, &registry_address, queue_account)?;
    let chain_end = queue
        .take(count)?
        .try_fold(chain_start, |chain, entry_hash| {
            chain_link(&chain, entry_hash).map_err(|_| StorageError::HashingFailed)
        })?;
    // 3. The statement's old values come from the account, and a stale revision fails ahead of the proof.
    let mut registry = load_tracking_registry_mut(program_id, registry_account)?;
    let (old_root, old_next_index) = (registry.roots[0].hash, registry.next_index());
    registry.update(TrackingRootUpdate {
        revision: ix.revision,
        slot: Clock::get()?.slot,
        new_root: ix.new_root,
        new_next_index: ix.new_next_index(),
    })?;
    // 4. A failed proof reverts the root and `head`.
    let statement = ApplyEnrollmentsStatement {
        old_root,
        new_root: ix.new_root,
        old_next_index,
        new_next_index: ix.new_next_index(),
        chain_start,
        chain_end,
        count,
    };
    RingProof {
        proof: &ix.proof,
        statement: statement.hash().map_err(|_| StorageError::HashingFailed)?,
        key: verifying_key(RingCircuit::apply_enrollments(count))
            .ok_or(StorageError::UnsupportedBatch)?,
    }
    .verify()
    .map_err(|_| StorageError::ProofVerificationFailed)?;
    Ok(())
}
