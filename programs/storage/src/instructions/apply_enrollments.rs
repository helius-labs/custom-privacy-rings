//! Applies a batch of owner-signed writes to a ring's tracking tree.
//!
//! The registry's authority, the ring's `ring_auth` acting for its `EnrollmentWriter`, is the one writer.
//! It holds no owner's key, so the precompile instruction ahead of the ring's carries each write's
//! owner ed25519 signature, and a first enrollment carries its `RegisterKey` proof. The writer chooses
//! the signed writes that enter and their order, nothing inside one.
//!
//! Read after `program-libs/interface/src/accounts/tracking.rs`, `program-libs/interface/src/proof/owner_signature.rs`.
//! Read before `prover/circuits/registry/apply_enrollments.go`.
//!
//! Guarantees:
//! 1. Every applied entry is one its owner signed over the registry, and every inserting entry carries a verified `RegisterKey` seal.
//! 2. The new root and next index follow from the current ones by exactly the batch's entries in write order, and the next index stays within `TRACKING_CAPACITY`.
//! 3. The outgoing root keeps its full grace. An apply that would evict an unexpired root refuses.
//! 4. An `EnrollmentsApplied` event carrying every write follows only a verified proof.
//!
//! Relies on:
//! 1. `ApplyEnrollmentsCircuit` keeps the tree sorted, inserts only proven entries of absent owners and moves only the viewing hash of a present owner.
//! 2. The ring signs as authority only for its `EnrollmentWriter` and pins the registry by address.
//! 3. The runtime refuses a transaction whose precompile signature fails.

use custom_ring_interface::{
    chain_link,
    key_catalog::RingCircuit,
    storage::StorageError,
    storage_instruction::{
        ApplyEnrollmentsRequest, EnrollmentChange, EnrollmentsAppliedEvent, StorageEvent,
    },
    tracking::{tracking_chain_start, TrackingRootUpdate},
    verifying_key, ApplyEnrollmentsStatement, RegisterKeyCheck, RingProof,
};
use pinocchio::{
    error::ProgramError,
    sysvars::{clock::Clock, Sysvar},
    AccountView, Address, ProgramResult,
};

use crate::{
    emit_event,
    loader::{
        check_signer, check_storage_program, load_tracking_registry_mut, with_owner_signatures,
    },
};

pub fn process_apply_enrollments_ix(
    program_id: &Address,
    accounts: &mut [AccountView],
    data: &[u8],
) -> ProgramResult {
    let request = ApplyEnrollmentsRequest::parse(data)?;
    let ix = request.ix;
    let [authority_account, registry_account, sysvar_account, program_account, ..] = accounts
    else {
        return Err(StorageError::MissingAccounts.into());
    };
    // 1. Only the registry's authority writes, the ring signs as it for its `EnrollmentWriter` alone.
    check_signer(authority_account)?;
    check_storage_program(program_id, program_account)?;
    let registry_address = *registry_account.address();
    let mut registry = load_tracking_registry_mut(program_id, registry_account)?;
    if authority_account.address() != &registry.authority {
        return Err(StorageError::Unauthorized.into());
    }
    // 2. Signature `i` is write `i`'s owner over the registry and its entry hash, a first enrollment also proves its seal.
    // The chain folds the same entry hashes, so the proof applies exactly what the owners signed.
    let chain_start = tracking_chain_start(&registry_address, ix.revision)?;
    let chain_end = with_owner_signatures(sysvar_account, request.writes.len(), |signatures| {
        request.writes.iter().enumerate().try_fold(
            chain_start,
            |chain, (index, write)| -> Result<[u8; 32], ProgramError> {
                write.check()?;
                let signed = write.signed(&registry_address)?;
                signatures.check(index, write.owner, &signed.message)?;
                if let EnrollmentChange::Enroll { ix: record, proof } = write.change {
                    RegisterKeyCheck {
                        ring_id: &registry.ring_id,
                        auditor_pubkey: &registry.auditor_pubkey,
                        owner: write.owner,
                        ix: record,
                        proof,
                    }
                    .verify()?;
                }
                Ok(chain_link(&chain, &signed.entry_hash)
                    .map_err(|_| StorageError::HashingFailed)?)
            },
        )
    })?;
    // 3. The statement's old values come from the account, and a stale revision fails ahead of the proof.
    let count = u64::from(ix.count);
    let (old_root, old_next_index) = (registry.roots[0].hash, registry.next_index());
    registry.update(TrackingRootUpdate {
        revision: ix.revision,
        slot: Clock::get()?.slot,
        new_root: ix.new_root,
        new_next_index: ix.new_next_index(),
    })?;
    // 4. A failed proof reverts the root.
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
        key: RingCircuit::apply_enrollments(count)
            .and_then(verifying_key)
            .ok_or(StorageError::UnsupportedBatch)?,
    }
    .verify()
    .map_err(|_| StorageError::ProofVerificationFailed)?;
    // 5. The event repeats the checked writes, the tree and every sealed secret rebuild from events alone.
    let header = EnrollmentsAppliedEvent {
        registry: registry_address,
        revision: registry.newest().revision,
        new_root: ix.new_root,
        new_next_index: ix.new_next_index,
        count: ix.count,
    };
    emit_event(
        program_id,
        &StorageEvent::EnrollmentsApplied {
            header,
            writes: request.writes,
        },
    )
}
