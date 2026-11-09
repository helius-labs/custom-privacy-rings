use custom_ring_interface::{tracking::EnrollmentQueue, STORAGE_PROGRAM_ID};
use sha2::{Digest, Sha256};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use wincode::{SchemaRead, SchemaWrite};

use super::RelayProofInputsError;

/// `Enroll` data as the wallet built it, its proof proven and its owner's signature still due.
#[derive(Clone, Debug, PartialEq, Eq, SchemaWrite, SchemaRead)]
pub struct RelayEnrollInputs {
    pub ring_program_id: Address,
    pub owner: Address,
    pub tracking_registry: Address,
    /// Tag 6 first, the program holds every field to the owner's signature and proof.
    pub instruction_data: Vec<u8>,
}

/// `Enroll` moves no lamports, so it names no payer.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageEnroll {
    pub owner: Address,
    pub tracking_registry: Address,
}

const ENROLL_REQUEST_DOMAIN: &[u8] = b"zolana/ring-rpc-relay-enroll";

/// The ceiling a relayed enrollment asks and its owner checks.
pub const ENROLL_COMPUTE_UNITS: u32 = 400_000;

impl RelayEnrollInputs {
    /// The owner's signature over it spends the relay's budget, it names no quote so a retry keeps it.
    pub fn request_digest(&self) -> [u8; 32] {
        Sha256::new()
            .chain_update(ENROLL_REQUEST_DOMAIN)
            .chain_update(self.ring_program_id.as_ref())
            .chain_update(self.owner.as_ref())
            .chain_update(self.tracking_registry.as_ref())
            .chain_update(&self.instruction_data)
            .finalize()
            .into()
    }

    pub fn instruction(&self) -> Instruction {
        StorageEnroll {
            owner: self.owner,
            tracking_registry: self.tracking_registry,
        }
        .instruction(self.instruction_data.clone())
    }

    pub fn encode(&self) -> Result<Vec<u8>, RelayProofInputsError> {
        Ok(wincode::serialize(self)?)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RelayProofInputsError> {
        Ok(wincode::deserialize_exact(bytes)?)
    }
}

impl StorageEnroll {
    /// `data` starts with `tag::ENROLL`.
    pub fn instruction(self, data: Vec<u8>) -> Instruction {
        let (queue, _) = EnrollmentQueue::find_address(&self.tracking_registry);
        Instruction {
            program_id: STORAGE_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new_readonly(self.owner, true),
                AccountMeta::new_readonly(self.tracking_registry, false),
                AccountMeta::new(queue, false),
            ],
            data,
        }
    }
}
