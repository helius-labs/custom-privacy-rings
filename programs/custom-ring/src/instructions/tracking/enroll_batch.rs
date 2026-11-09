//! Forwards a batch of owner-signed enrollment writes to the ring's tracking registry.
//!
//! Storage accepts tracking writes only from the registry's authority, `ring_auth`. The ring signs a tracking
//! write as `ring_auth` only for the key its `EnrollmentWriter` role names, and storage checks every owner
//! signature and seal itself, so the role decides who submits and never what an owner signed.
//!
//! Read after `instructions/accounts/loader.rs`, `instructions/runtime/instruction_runtime.rs`. Read before
//! `programs/storage/src/instructions/apply_enrollments.rs`.

use custom_ring_interface::{
    storage_instruction::tag as storage_tag, CustomRingError, RingRole, STORAGE_PROGRAM_ID,
};
use pinocchio::{AccountView, Address, ProgramResult};
use spr_account_checks::AccountIterator;

use crate::instructions::{instruction_runtime::RingAuth, loader::RoleHolder};

#[inline(never)]
pub fn process_enroll_batch_ix(
    program_id: &Address,
    accounts: &mut [AccountView],
    data: &[u8],
) -> ProgramResult {
    let mut iter = AccountIterator::new(accounts);
    let writer = iter.next_signer("writer")?;
    let ring = iter.next_account("ring")?;
    let ring_auth = iter.next_mut("ring_auth")?;
    let tracking = iter.next_mut("tracking_registry")?;
    let instructions_sysvar = iter.next_account("instructions_sysvar")?;
    let storage_program = iter.next_account("storage_program")?;

    // 1. Only the enrollment writer submits, into the registry the ring pinned at creation.
    let policy = RoleHolder {
        program_id,
        role: RingRole::EnrollmentWriter,
        signer: writer,
    }
    .load(ring)?;
    if tracking.address() != &policy.tracking {
        return Err(CustomRingError::InvalidTrackingRegistry.into());
    }
    drop(policy);
    // 2. Storage parses, verifies and applies the forwarded data unchanged under the `ring_auth` signature.
    let placed = RingAuth::derive(program_id)?.check_account(ring_auth)?;
    let storage_data = [&[storage_tag::APPLY_ENROLLMENTS][..], data].concat();
    placed.invoke_program(
        &STORAGE_PROGRAM_ID,
        &[
            &*ring_auth,
            &*tracking,
            &*instructions_sysvar,
            &*storage_program,
        ],
        &storage_data,
    )
}
