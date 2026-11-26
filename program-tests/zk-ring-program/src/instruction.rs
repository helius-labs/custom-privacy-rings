use core::mem::offset_of;

use custom_ring_interface::{
    transfer_cpi::{RingTransferCpi, TransferFrame},
    RingMode, RingPolicy, RING_POLICY,
};
use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    AccountView, Address, ProgramResult,
};
use spr_hasher::primitives::hash_bytes;
use spr_program::{program::PdaOwner, CompressedProof};

use crate::{
    owner_seeds, tag,
    verifier::{create, update},
};

pub const PROOF_LEN: usize = 32 + 64 + 32;

/// Accounts `[ring_program, transfer..]`, data `tag || proof || transfer` with the transfer from its tag.
pub fn process_instruction(
    program_id: &Address,
    accounts: &mut [AccountView],
    data: &[u8],
) -> ProgramResult {
    let [ring_program, transfer_accounts @ ..] = &*accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    let owner_seeds = owner_seeds(ring_program.address());
    let (owner, bump) = Address::derive_program_address(&owner_seeds, program_id)
        .ok_or(ProgramError::InvalidSeeds)?;
    let (ix_tag, data) = data
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;
    let (proof, transfer) = split_proof(data).ok_or(ProgramError::InvalidInstructionData)?;
    let mode = load_ring_mode(ring_program.address(), transfer_accounts)?;
    let frame = TransferFrame::parse(transfer, mode)?;
    // The program's rule covers private value alone.
    if !frame.interface_transfers().is_empty() {
        return Err(ProgramError::InvalidArgument);
    }
    let owner_hash = *PdaOwner::new(&owner)?.owner_hash();
    let ring = hash_bytes(ring_program.address().as_array())?;
    let private_tx_hash = frame.private_tx_hash();
    match *ix_tag {
        tag::CREATE => create::Verifier {
            proof: &proof,
            public_inputs: create::PublicInputs {
                owner: owner_hash,
                ring,
            },
            private_tx_hash,
            transfers: &[],
        }
        .verify()?,
        tag::UPDATE => update::Verifier {
            proof: &proof,
            public_inputs: update::PublicInputs {
                owner: owner_hash,
                ring,
            },
            private_tx_hash,
            transfers: &[],
        }
        .verify()?,
        _ => return Err(ProgramError::InvalidInstructionData),
    }

    let bump = [bump];
    let seeds = [
        Seed::from(owner_seeds[0]),
        Seed::from(owner_seeds[1]),
        Seed::from(&bump),
    ];
    RingTransferCpi {
        ring_program: ring_program.address(),
        owner: &owner,
        accounts: transfer_accounts,
        data: frame.data(),
    }
    .invoke_signed_parsed(frame, Signer::from(seeds.as_slice()))
}

fn split_proof(data: &[u8]) -> Option<(CompressedProof, &[u8])> {
    let (a, data) = data.split_first_chunk::<32>()?;
    let (b, data) = data.split_first_chunk::<64>()?;
    let (c, transfer) = data.split_first_chunk::<32>()?;
    Some((
        CompressedProof {
            a: *a,
            b: *b,
            c: *c,
        },
        transfer,
    ))
}

/// The ring parses the frame under the mode of the same account.
fn load_ring_mode(
    ring_program: &Address,
    transfer_accounts: &[AccountView],
) -> Result<RingMode, ProgramError> {
    let policy = transfer_accounts
        .first()
        .ok_or(ProgramError::NotEnoughAccountKeys)?;
    if !policy.owned_by(ring_program) {
        return Err(ProgramError::IllegalOwner);
    }
    let data = policy.try_borrow()?;
    if data.len() != size_of::<RingPolicy>() || data[0] != RING_POLICY {
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(RingMode::try_from(data[offset_of!(RingPolicy, mode)])?)
}
