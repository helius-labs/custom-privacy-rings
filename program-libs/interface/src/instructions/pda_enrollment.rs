use solana_address::Address;

use crate::{
    storage::StorageError,
    storage_instruction::{ApplyEnrollmentsRequest, EnrollmentWrite},
};

pub const ENROLL_PDA: &[u8; 8] = b"RINGENRL";

#[derive(Clone, Copy)]
pub struct EnrollmentOwner<'a> {
    pub owner: &'a Address,
    pub ring_authority: &'a Address,
}

impl EnrollmentOwner<'_> {
    pub fn parse<'a>(&self, payload: &'a [u8]) -> Result<EnrollmentWrite<'a>, StorageError> {
        let body = payload
            .strip_prefix(&[crate::tag::ENROLL_BATCH])
            .ok_or(StorageError::InvalidInstructionData)?;
        let batch = ApplyEnrollmentsRequest::parse(body)?;
        let [write] = batch.writes.as_slice() else {
            return Err(StorageError::UnsupportedBatch);
        };
        if write.owner != self.owner {
            return Err(StorageError::OwnerSignatureMismatch);
        }
        write.check_pda_owner(self.ring_authority)?;
        write.check()?;
        Ok(*write)
    }
}

#[cfg(feature = "enrollment-cpi")]
pub use cpi::{PdaEnrollment, VaultEnrollmentPolicy};

#[cfg(feature = "enrollment-cpi")]
mod cpi {
    use pinocchio::{
        cpi::{invoke_signed, Signer},
        error::ProgramError,
        instruction::{InstructionAccount, InstructionView},
        AccountView, Address, ProgramResult,
    };

    use super::{EnrollmentOwner, EnrollmentWrite, ENROLL_PDA};
    use crate::{storage::StorageError, storage_instruction::EnrollmentChange, STORAGE_PROGRAM_ID};

    pub struct VaultEnrollmentPolicy<'a> {
        pub manager: &'a Address,
        pub vault: &'a Address,
        pub ring: &'a Address,
        pub tracking_registry: &'a Address,
    }

    pub struct PdaEnrollment<'a> {
        manager: &'a AccountView,
        writer: &'a AccountView,
        policy: &'a AccountView,
        ring_auth: &'a AccountView,
        tracking: &'a AccountView,
        vault: &'a AccountView,
        storage: &'a AccountView,
        ring: &'a AccountView,
        payload: &'a [u8],
        write: EnrollmentWrite<'a>,
    }

    impl<'a> PdaEnrollment<'a> {
        pub fn parse(accounts: &'a [AccountView], data: &'a [u8]) -> Result<Self, ProgramError> {
            let [manager, writer, policy, ring_auth, tracking, vault, storage, ring] = accounts
            else {
                return Err(StorageError::MissingAccounts.into());
            };
            if !manager.is_signer() || !writer.is_signer() || manager.address() == writer.address()
            {
                return Err(StorageError::MissingSignature.into());
            }
            if storage.address() != &STORAGE_PROGRAM_ID {
                return Err(StorageError::InvalidStorageProgram.into());
            }
            let payload = data
                .strip_prefix(ENROLL_PDA)
                .ok_or(StorageError::InvalidInstructionData)?;
            let write = EnrollmentOwner {
                owner: vault.address(),
                ring_authority: ring_auth.address(),
            }
            .parse(payload)?;
            Ok(Self {
                manager,
                writer,
                policy,
                ring_auth,
                tracking,
                vault,
                storage,
                ring,
                payload,
                write,
            })
        }

        pub fn vault(&self) -> &AccountView {
            self.vault
        }

        pub fn change(&self) -> EnrollmentChange<'a> {
            self.write.change
        }

        pub fn invoke_signed(
            self,
            policy: VaultEnrollmentPolicy<'_>,
            signer: Signer<'_, '_>,
        ) -> ProgramResult {
            if self.manager.address() != policy.manager {
                return Err(StorageError::Unauthorized.into());
            }
            if self.vault.address() != policy.vault {
                return Err(StorageError::OwnerSignatureMismatch.into());
            }
            if self.ring.address() != policy.ring
                || self.tracking.address() != policy.tracking_registry
            {
                return Err(StorageError::InvalidTrackingRegistry.into());
            }
            let accounts = [
                InstructionAccount::readonly_signer(self.writer.address()),
                InstructionAccount::readonly(self.policy.address()),
                InstructionAccount::writable(self.ring_auth.address()),
                InstructionAccount::writable(self.tracking.address()),
                InstructionAccount::readonly_signer(self.vault.address()),
                InstructionAccount::readonly(self.storage.address()),
            ];
            let instruction = InstructionView {
                program_id: policy.ring,
                data: self.payload,
                accounts: &accounts,
            };
            invoke_signed(
                &instruction,
                &[
                    self.writer,
                    self.policy,
                    self.ring_auth,
                    self.tracking,
                    self.vault,
                    self.storage,
                ],
                &[signer],
            )
        }
    }
}
