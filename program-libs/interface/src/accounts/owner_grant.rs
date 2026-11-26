//! Owner signatures authorize reader grants within one tracking registry namespace.
//!
//! Read after `program-libs/interface/src/accounts/tracking.rs`.
//! Read before `programs/storage/src/instructions/owner_grant.rs`.

use bytemuck::{Pod, Zeroable};
use solana_address::Address;

use crate::{
    storage::{storage_account, StorageError},
    storage_instruction::OwnerGrantIxData,
    ReaderKeyBytes, READER_KEY_ED25519, READER_KEY_P256, STORAGE_PROGRAM_ID,
};

/// A program owner's on-chain consent to one reader, a PDA cannot sign off chain, the address names both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct OwnerGrant {
    pub discriminator: u8,
    /// Zero revokes.
    pub active: u8,
}

storage_account!(OwnerGrant, 5, b"owner_grant", |_: &Self| true);

impl OwnerGrant {
    /// One account per owner per reader, the key splits because a seed holds 32 bytes.
    pub fn seeds<'a>(
        registry: &'a Address,
        owner: &'a Address,
        reader: &'a ReaderKeyBytes,
    ) -> [&'a [u8]; 5] {
        let (scheme, key) = reader.split_at(2);
        [Self::SEED, registry.as_ref(), owner.as_ref(), scheme, key]
    }

    pub fn find_address(
        registry: &Address,
        owner: &Address,
        reader: &ReaderKeyBytes,
    ) -> (Address, u8) {
        Address::find_program_address(&Self::seeds(registry, owner, reader), &STORAGE_PROGRAM_ID)
    }

    pub const fn is_active(&self) -> bool {
        self.active == 1
    }
}

impl OwnerGrantIxData {
    /// An Ed25519 key's trailing byte is in the seeds, one grant address per reader.
    pub fn check(&self) -> Result<(), StorageError> {
        let canonical = match self.reader[0] {
            READER_KEY_ED25519 => self.reader[33] == 0,
            READER_KEY_P256 => matches!(self.reader[1], 0x02 | 0x03),
            _ => false,
        };
        if !canonical {
            return Err(StorageError::InvalidReaderKey);
        }
        if self.active > 1 {
            return Err(StorageError::InvalidInstructionData);
        }
        Ok(())
    }
}

const _: () = assert!(OwnerGrant::SIZE == 2);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p256_reader_key_requires_compressed_sec1_prefix() {
        for prefix in [0x02, 0x03] {
            let mut reader = [7; 34];
            reader[0] = READER_KEY_P256;
            reader[1] = prefix;
            assert_eq!(OwnerGrantIxData { reader, active: 1 }.check(), Ok(()));
        }
        for bad_prefix in [0x00, 0x01, 0x04, 0x06, 0xff] {
            let mut reader = [7; 34];
            reader[0] = READER_KEY_P256;
            reader[1] = bad_prefix;
            assert_eq!(
                OwnerGrantIxData { reader, active: 1 }.check(),
                Err(StorageError::InvalidReaderKey),
                "bad_prefix={bad_prefix:#x}"
            );
        }
    }
}
