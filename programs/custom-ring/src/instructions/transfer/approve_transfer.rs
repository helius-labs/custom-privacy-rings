//! Lists a co-signer's approval of one transfer call among the transaction's instructions.
//!
//! The instruction writes nothing. A transfer reads the approval from the instructions sysvar, and
//! the sysvar lists top-level instructions alone, each with the signer flags of the transaction.
//!
//! Read after `program-libs/interface/src/instructions/transfer_approval.rs`. Read before `instructions/accounts/loader.rs`.

use custom_ring_interface::{CoSigner, CustomRingError, RingPolicy, TransferApproval};
use pinocchio::{AccountView, Address, ProgramResult};
use spr_account_checks::AccountIterator;

use crate::instructions::loader::load;

const TOP_LEVEL_STACK_HEIGHT: u64 = 1;

#[inline(never)]
pub fn process_approve_transfer_ix(
    program_id: &Address,
    accounts: &mut [AccountView],
    data: &[u8],
) -> ProgramResult {
    // 1. A call from another program appears in no instruction list, a transfer would never read it.
    if stack_height() != TOP_LEVEL_STACK_HEIGHT {
        return Err(CustomRingError::ApprovalNotTopLevel.into());
    }
    TransferApproval::parse_body(data).ok_or(CustomRingError::InvalidInstructionData)?;
    let mut iter = AccountIterator::new(accounts);
    let ring = iter.next_account("ring")?;
    let record = iter.next_account("co_signer_record")?;
    let co_signer = iter.next_account("co_signer")?;

    // 2. The record's own signer approves under the ring of the program.
    load::<RingPolicy>(program_id, ring)?;
    let record = load::<CoSigner>(program_id, record)?;
    if !co_signer.is_signer() {
        return Err(CustomRingError::MissingCoSigner.into());
    }
    if co_signer.address() != &record.signer {
        return Err(CustomRingError::UnauthorizedCoSigner.into());
    }
    Ok(())
}

/// Zero off chain, no host call is at the top level.
fn stack_height() -> u64 {
    #[cfg(target_os = "solana")]
    // The syscall takes no argument and reads no program memory.
    unsafe {
        pinocchio::syscalls::sol_get_stack_height()
    }
    #[cfg(not(target_os = "solana"))]
    0
}
