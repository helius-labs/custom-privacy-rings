//! The writer of the ed25519 precompile instruction `Ed25519Signatures` reads.

use custom_ring_interface::owner_signature::{
    Ed25519Signatures, SignatureOffsets, HEADER_LEN, THIS_INSTRUCTION,
};
use solana_address::Address;

/// One owner's signature over one write's message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnerSignature<'a> {
    pub owner: &'a Address,
    pub signature: &'a [u8; 64],
    pub message: &'a [u8],
}

pub trait SignatureInstruction {
    /// The precompile data with every descriptor pointing into itself, `None` past a `u16` offset.
    fn encode(signatures: &[OwnerSignature<'_>]) -> Option<Vec<u8>>;
}

impl SignatureInstruction for Ed25519Signatures<'_> {
    fn encode(signatures: &[OwnerSignature<'_>]) -> Option<Vec<u8>> {
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
