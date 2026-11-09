//! The ed25519 precompile instruction that carries every owner signature of an enrollment batch.
//!
//! The runtime verifies each signature the precompile names, and refuses the transaction when one
//! does not verify. A program therefore learns that an owner signed a message only by
//! reading the precompile instruction through the instructions sysvar and matching its descriptors to
//! the writes it applies. A descriptor that points into another instruction would let the key or the
//! message come from bytes the program never compares, so every descriptor here points into the
//! precompile instruction itself.
//!
//! Read after `program-libs/interface/src/instructions/storage.rs`. Read before
//! `programs/storage/src/instructions/apply_enrollments.rs`.

use bytemuck::{Pod, Zeroable};
use solana_address::Address;

use crate::storage::StorageError;

/// An instruction index naming the precompile instruction itself.
const THIS_INSTRUCTION: u16 = u16::MAX;
/// `num_signatures u8 || padding u8`.
const HEADER_LEN: usize = 2;

/// One precompile descriptor, every field little endian.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
struct SignatureOffsets {
    signature_offset: [u8; 2],
    signature_instruction_index: [u8; 2],
    public_key_offset: [u8; 2],
    public_key_instruction_index: [u8; 2],
    message_data_offset: [u8; 2],
    message_data_size: [u8; 2],
    message_instruction_index: [u8; 2],
}

/// The descriptors of one precompile instruction and the bytes they point into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ed25519Signatures<'a> {
    data: &'a [u8],
    offsets: &'a [SignatureOffsets],
}

/// One owner's signature over one write's message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnerSignature<'a> {
    pub owner: &'a Address,
    pub signature: &'a [u8; 64],
    pub message: &'a [u8],
}

impl<'a> Ed25519Signatures<'a> {
    /// `InvalidSignatureInstruction` unless the data names exactly `count` signatures.
    pub fn parse(data: &'a [u8], count: usize) -> Result<Self, StorageError> {
        let invalid = StorageError::InvalidSignatureInstruction;
        let ([signatures, _], rest) = data.split_first_chunk::<HEADER_LEN>().ok_or(invalid)?;
        if usize::from(*signatures) != count {
            return Err(invalid);
        }
        let offsets = rest
            .get(..count * core::mem::size_of::<SignatureOffsets>())
            .ok_or(invalid)?;
        Ok(Self {
            data,
            offsets: bytemuck::try_cast_slice(offsets).map_err(|_| invalid)?,
        })
    }

    /// `OwnerSignatureMismatch` unless signature `index` is `owner`'s over exactly `message`, inside the precompile instruction.
    pub fn check(&self, index: usize, owner: &Address, message: &[u8]) -> Result<(), StorageError> {
        let mismatch = StorageError::OwnerSignatureMismatch;
        let offsets = self.offsets.get(index).ok_or(mismatch)?;
        let field = u16::from_le_bytes;
        // 1. Bytes of another instruction would escape the comparisons below.
        if [
            offsets.signature_instruction_index,
            offsets.public_key_instruction_index,
            offsets.message_instruction_index,
        ]
        .into_iter()
        .any(|instruction| field(instruction) != THIS_INSTRUCTION)
        {
            return Err(mismatch);
        }
        // 2. The verified key and message are the write's owner and signed message, byte for byte.
        let bytes = |offset: [u8; 2], len: usize| {
            let start = usize::from(field(offset));
            self.data.get(start..start.checked_add(len)?)
        };
        let key = bytes(offsets.public_key_offset, 32).ok_or(mismatch)?;
        let signed = bytes(
            offsets.message_data_offset,
            usize::from(field(offsets.message_data_size)),
        )
        .ok_or(mismatch)?;
        match key == owner.as_ref() && signed == message {
            true => Ok(()),
            false => Err(mismatch),
        }
    }
}

impl Ed25519Signatures<'_> {
    /// The precompile data with every descriptor pointing into itself, `None` past a `u16` offset.
    #[cfg(not(target_os = "solana"))]
    pub fn encode(signatures: &[OwnerSignature<'_>]) -> Option<Vec<u8>> {
        let descriptors = HEADER_LEN + signatures.len() * core::mem::size_of::<SignatureOffsets>();
        let mut offsets = Vec::with_capacity(signatures.len());
        let mut body = Vec::new();
        for signature in signatures {
            let at = |body: &Vec<u8>| u16::try_from(descriptors + body.len()).ok();
            let signature_offset = at(&body)?;
            body.extend_from_slice(signature.signature);
            let public_key_offset = at(&body)?;
            body.extend_from_slice(signature.owner.as_ref());
            let message_data_offset = at(&body)?;
            body.extend_from_slice(signature.message);
            offsets.push(SignatureOffsets {
                signature_offset: signature_offset.to_le_bytes(),
                signature_instruction_index: THIS_INSTRUCTION.to_le_bytes(),
                public_key_offset: public_key_offset.to_le_bytes(),
                public_key_instruction_index: THIS_INSTRUCTION.to_le_bytes(),
                message_data_offset: message_data_offset.to_le_bytes(),
                message_data_size: u16::try_from(signature.message.len()).ok()?.to_le_bytes(),
                message_instruction_index: THIS_INSTRUCTION.to_le_bytes(),
            });
        }
        let count = u8::try_from(signatures.len()).ok()?;
        Some([&[count, 0][..], bytemuck::cast_slice(&offsets), &body].concat())
    }
}

const _: () = assert!(core::mem::size_of::<SignatureOffsets>() == 14);
