//! Enrolls an owner's keys in a ring's tracking registry, or rotates its viewing key, by appending one entry to the registry's queue.
//!
//! An SPP sender names any nullifier key on an output, and SPP never sees the secret behind it. A ring
//! needs its auditor able to compute each nullifier and needs an owner's salted lists to bind. Enrollment
//! proves both secrets are sealed to the auditor, once, before any proof can read the row.
//!
//! Read after `programs/storage/src/lib.rs`, `program-libs/interface/src/accounts/tracking.rs`. Read
//! before `prover/circuits/registry/register_key.go`, `programs/storage/src/instructions/apply_enrollments.rs`.
//!
//! Guarantees:
//! 1. The owner signs, a program signs its PDA through CPI. The instruction moves no lamports and takes no payer.
//! 2. An entry with a proof verifies `RegisterKey` with the ring id, member and auditor key the program supplies, a proof for another ring, member or auditor fails. Only such an entry inserts a row at apply.
//! 3. The program computes the entry from the signer and the data it checked, and the queue stores its hash at the next sequence.
//! 4. The registry loads read only and the queue is its own account, so enrollment takes no write lock a transfer meets.
//! 5. The tree and every sealed secret rebuild from the `Enrolled` and `EnrollmentsApplied` events alone.
//!
//! Relies on:
//! 1. The ring pins the registry by address. The registry's ring id and auditor key are its creator's word.
//! 2. An entry grants nothing until an apply places it under a root, because every check reads the root.

use custom_ring_interface::{
    storage::StorageError,
    storage_instruction::{EnrollRequest, EnrolledEvent, StorageEvent},
    tracking::TrackingEntry,
    RegisterKeyCheck,
};
use pinocchio::{AccountView, Address, ProgramResult};

use crate::{
    emit_event,
    loader::{
        check_signer, check_storage_program, load_enrollment_queue_mut, load_tracking_registry,
    },
};

pub fn process_enroll_ix(
    program_id: &Address,
    accounts: &mut [AccountView],
    data: &[u8],
) -> ProgramResult {
    let EnrollRequest { ix, proof } = EnrollRequest::parse(data)?;
    let [owner_account, registry_account, queue_account, program_account, ..] = accounts else {
        return Err(StorageError::MissingAccounts.into());
    };
    // 1. The owner's signature authorizes the entry and names its member.
    check_signer(owner_account)?;
    check_storage_program(program_id, program_account)?;
    ix.check()?;
    // 2. The queue must name the registry, an entry in another registry's queue would apply there.
    let registry = load_tracking_registry(program_id, registry_account)?;
    let mut queue =
        load_enrollment_queue_mut(program_id, registry_account.address(), queue_account)?;
    // 3. A proof seals the secrets to the registry's auditor and marks the entry proven.
    // An entry without one is a viewing key rotation, it moves a present row's viewing hash alone.
    if let Some(proof) = proof {
        RegisterKeyCheck {
            ring_id: &registry.ring_id,
            auditor_pubkey: &registry.auditor_pubkey,
            owner: owner_account.address(),
            ix,
            proof,
        }
        .verify()?;
    }
    // 4. A full queue refuses with nothing written.
    let proven = proof.is_some();
    let entry = TrackingEntry::from_enroll(owner_account.address(), ix, proven)?;
    let sequence = queue.push(entry.hash().map_err(|_| StorageError::HashingFailed)?)?;
    // 5. The event repeats the values the queued hash binds, after every check passed.
    emit_event(
        program_id,
        &StorageEvent::Enrolled(EnrolledEvent {
            registry: *registry_account.address(),
            owner: *owner_account.address(),
            sequence: sequence.to_le_bytes(),
            proven: u8::from(proven),
            record: *ix,
        }),
    )
}
