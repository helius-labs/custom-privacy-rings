//! The compute unit limit a client sets for each ring instruction.

use custom_ring_interface::InstructionTag;

const ACCOUNT_UNITS: u32 = 50_000;
/// One policy hash plus a curator registry load.
const POLICY_UNITS: u32 = 150_000;
/// A ring proof, and for most the SPP CPI with its own proof.
pub const PROOF_UNITS: u32 = 1_400_000;

pub const fn compute_unit_limit(tag: InstructionTag) -> u32 {
    match tag {
        InstructionTag::Transfer => PROOF_UNITS,
        InstructionTag::Merge => PROOF_UNITS,
        InstructionTag::UpdateAuthority => ACCOUNT_UNITS,
        InstructionTag::SetPaused => ACCOUNT_UNITS,
        InstructionTag::SetCoSigner => ACCOUNT_UNITS,
        InstructionTag::ClearCoSigner => ACCOUNT_UNITS,
        InstructionTag::SetDelegate => ACCOUNT_UNITS,
        InstructionTag::ApproveTransfer => ACCOUNT_UNITS,
        InstructionTag::AuthorityTransfer => PROOF_UNITS,
        InstructionTag::Initialize => 3 * ACCOUNT_UNITS + POLICY_UNITS,
        InstructionTag::UpdatePolicy => POLICY_UNITS,
        InstructionTag::EnrollBatch => PROOF_UNITS,
    }
}
