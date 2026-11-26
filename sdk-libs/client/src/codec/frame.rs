//! The writer of a transfer frame and its SPP expansion, the bytes `RingTransactData::parse` reads back.

use custom_ring_interface::{
    extension_flag, key_catalog::TransferShape, stream::framed_slot_tag, CustomRingError,
    FrameExtensions, FrameHeader, InputTrees, RingOutput, RingPolicy, RingTransactData, SlotBody,
    RING_TRANSACT_ACCOUNT_PREFIX_LEN,
};
use spr_interface::instruction::{
    instruction_data::transact::CacheAccess, ExternalDataPreimage, InterfaceTransfer,
    TransactIxData, TransactIxDataRef, TransactProof, TreeContext,
};
use wincode::{containers, len::FixIntLen, Serialize};

/// The runtime's ceiling on CPI instruction data, the expanded SPP call must fit it.
pub const MAX_CPI_DATA_LEN: usize = 10 * 1024;

/// One public leg's committed accounts, the asset account then the user account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settlement<'a> {
    pub asset_account: &'a [u8; 32],
    pub user_account: &'a [u8; 32],
}

pub trait FrameEncode {
    /// The bytes `parse` reads back, refusing what it refuses.
    fn encode(&self) -> Result<Vec<u8>, CustomRingError>;

    /// For catalog sizing, `parse` refuses a `shape` off the catalog.
    fn encode_under(&self, shape: TransferShape) -> Result<Vec<u8>, CustomRingError>;
}

impl FrameEncode for RingTransactData<'_> {
    fn encode(&self) -> Result<Vec<u8>, CustomRingError> {
        self.encode_under(self.rail.key_shape(&FrameHeader::of(self)?)?)
    }

    fn encode_under(&self, shape: TransferShape) -> Result<Vec<u8>, CustomRingError> {
        let flags = self.flags();
        let header = FrameHeader::of(self)?;
        let stream = self.stream_under(shape);
        self.validate_under(stream)?;
        let interface_transfers =
            <containers::Vec<InterfaceTransfer, FixIntLen<u8>> as Serialize>::serialize(
                &self.interface_transfers,
            )
            .map_err(|_| CustomRingError::InvalidInstructionData)?;
        let mut out = Vec::new();
        out.push(flags.0);
        out.extend(header.bytes()?);
        out.push(self.approval.bits());
        out.extend_from_slice(bytemuck::bytes_of(self.proof));
        out.extend_from_slice(&self.tracking_revision.0);
        if let Some(revision) = self.own_revision {
            out.extend_from_slice(&revision.0);
        }
        if let Some(revision) = self.policy.and_then(|policy| policy.curator_revision) {
            out.extend_from_slice(&revision.0);
        }
        out.extend_from_slice(&self.expiry_unix_ts.to_le_bytes());
        out.extend_from_slice(self.tx_viewing_u);
        out.extend_from_slice(self.salt);
        if !self.extensions.is_empty() {
            write_extensions(&self.extensions, &mut out)?;
        }
        out.extend_from_slice(&interface_transfers);
        out.extend_from_slice(self.spl_withdrawal_owners.as_flattened());
        push_bitmap(&mut out, self.outputs.iter().map(RingOutput::is_exit));
        for output in &self.outputs {
            if self.has_data {
                let len = u16::try_from(output.extra_len())
                    .map_err(|_| CustomRingError::OutputBodyLength)?;
                out.extend_from_slice(&len.to_le_bytes());
            }
            out.extend_from_slice(output.hash);
            match output.body {
                SlotBody::Exit(body) => {
                    out.extend_from_slice(&output.owner_tag);
                    out.extend_from_slice(output.record);
                    out.extend_from_slice(body);
                }
                SlotBody::Recipient(body) => {
                    out.extend_from_slice(&framed_slot_tag(&output.owner_tag));
                    out.extend_from_slice(body);
                }
            }
        }
        if let Some(replies) = self.replies {
            out.extend_from_slice(&replies.byte_len().to_le_bytes());
            out.extend_from_slice(replies.region());
        }
        if let Some(sender) = self.sender {
            out.extend_from_slice(sender.tag);
            out.extend_from_slice(sender.body);
        }
        out.extend_from_slice(self.envelope);
        out.extend_from_slice(self.recipient_keys);
        out.extend_from_slice(self.private_tx_hash);
        if let Some(anonymous) = &self.anonymous {
            out.extend_from_slice(anonymous.commitment);
            out.extend_from_slice(anonymous.commitment_pok);
            if let Some(tag) = anonymous.default_owner_tag {
                out.extend_from_slice(tag);
            }
        }
        if let Some(owner) = &self.in_proof_owner {
            out.extend_from_slice(owner.owner);
            out.extend_from_slice(owner.commitment);
            out.extend_from_slice(owner.commitment_pok);
        }
        out.extend_from_slice(bytemuck::bytes_of(self.spp_proof));
        out.extend_from_slice(self.nullifiers.as_flattened());
        match self.input_trees {
            InputTrees::One(context) => push_tree_context(&mut out, context)?,
            InputTrees::Two {
                first,
                second,
                second_inputs,
            } => {
                push_bitmap(
                    &mut out,
                    (0..self.nullifiers.len()).map(|input| (second_inputs >> input) & 1 == 1),
                );
                push_tree_context(&mut out, first)?;
                push_tree_context(&mut out, second)?;
            }
        }
        Ok(out)
    }
}

pub trait SppExpansion {
    /// The SPP data the program forwards, `b` decompressed for SPP's verifier.
    fn expand(&self, ring: &RingPolicy) -> Result<TransactIxData, CustomRingError>;

    fn spp_instruction_data(&self, ring: &RingPolicy) -> Result<Vec<u8>, CustomRingError>;

    /// The tag and the expansion under `proof` in one buffer, the program's heap holds no second copy.
    fn spp_instruction_data_under(
        &self,
        ring: &RingPolicy,
        proof: TransactProof,
    ) -> Result<Vec<u8>, CustomRingError>;

    /// The external data hash the SPP proof binds, computable before either proof exists.
    fn external_data_hash(
        &self,
        ring: &RingPolicy,
        settlements: &[Settlement<'_>],
    ) -> Result<[u8; 32], CustomRingError>;

    /// The expansion under `proof`, a client sizes the transaction with a zeroed one.
    fn transact_ix_data(
        &self,
        ring: &RingPolicy,
        proof: TransactProof,
    ) -> Result<TransactIxData, CustomRingError>;

    /// The run SPP reads as owner signers, between the nullifier PDAs and the settlement groups.
    fn owner_signers<'s, T>(
        &self,
        spp: &'s [T],
        settlements: usize,
    ) -> Result<&'s [T], CustomRingError>;
}

impl SppExpansion for RingTransactData<'_> {
    fn expand(&self, ring: &RingPolicy) -> Result<TransactIxData, CustomRingError> {
        self.transact_ix_data(ring, self.spp_proof_points()?)
    }

    fn spp_instruction_data(&self, ring: &RingPolicy) -> Result<Vec<u8>, CustomRingError> {
        self.spp_instruction_data_under(ring, self.spp_proof_points()?)
    }

    fn spp_instruction_data_under(
        &self,
        ring: &RingPolicy,
        proof: TransactProof,
    ) -> Result<Vec<u8>, CustomRingError> {
        self.validate_under(self.audit_stream()?)?;
        self.spp_instruction_data_validated(ring, proof)
    }

    fn external_data_hash(
        &self,
        ring: &RingPolicy,
        settlements: &[Settlement<'_>],
    ) -> Result<[u8; 32], CustomRingError> {
        let data = self.spp_instruction_data_under(ring, TransactProof::zeroed())?;
        let (tag, expanded) = data
            .split_first()
            .ok_or(CustomRingError::InvalidInstructionData)?;
        // 1. SPP's own parser fixes the prefix boundary, the bytes before `private_tx_hash`.
        let (_, prefix) = TransactIxDataRef::parse_with_external_data_prefix(expanded)
            .map_err(|_| CustomRingError::InvalidInstructionData)?;
        let tag = [*tag];
        let mut preimage = ExternalDataPreimage::new(&tag, prefix);
        // 2. The caller must supply settlements in transfer order. Each pair binds asset then user after the prefix.
        // Inline owner tags are already in the prefix.
        for settlement in settlements {
            preimage.push_settlement(settlement.asset_account, settlement.user_account)?;
        }
        Ok(preimage.finish()?)
    }

    fn transact_ix_data(
        &self,
        ring: &RingPolicy,
        proof: TransactProof,
    ) -> Result<TransactIxData, CustomRingError> {
        self.validate_under(self.audit_stream()?)?;
        self.transact_ix_data_validated(ring, proof)
    }

    fn owner_signers<'s, T>(
        &self,
        spp: &'s [T],
        settlements: usize,
    ) -> Result<&'s [T], CustomRingError> {
        let start =
            RING_TRANSACT_ACCOUNT_PREFIX_LEN + self.input_trees.count() + self.nullifiers.len();
        settlements
            .checked_add(self.cache_accounts())
            .and_then(|tail| spp.len().checked_sub(tail))
            .and_then(|end| spp.get(start..end))
            .ok_or(CustomRingError::InvalidSettlementAccounts)
    }
}

fn write_extensions(
    extensions: &FrameExtensions<'_>,
    out: &mut Vec<u8>,
) -> Result<(), CustomRingError> {
    let flag = |set: bool, bit: u8| if set { bit } else { 0 };
    out.push(
        flag(extensions.data_hash.is_some(), extension_flag::DATA_HASH)
            | flag(
                extensions.ring_data_hash.is_some(),
                extension_flag::RING_DATA_HASH,
            )
            | flag(!extensions.messages.is_empty(), extension_flag::MESSAGES)
            | flag(extensions.cache.is_some(), extension_flag::CACHE)
            | flag(
                extensions.approval_totals.is_some(),
                extension_flag::APPROVAL_TOTALS,
            ),
    );
    if let Some(hash) = extensions.data_hash {
        out.extend_from_slice(hash);
    }
    if let Some(hash) = extensions.ring_data_hash {
        out.extend_from_slice(hash);
    }
    if !extensions.messages.is_empty() {
        out.push(
            u8::try_from(extensions.messages.len())
                .map_err(|_| CustomRingError::RingTransactLength)?,
        );
        for message in &extensions.messages {
            let len =
                u16::try_from(message.data.len()).map_err(|_| CustomRingError::OutputBodyLength)?;
            out.extend_from_slice(message.view_tag);
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(message.data);
        }
    }
    if let Some(cache) = &extensions.cache {
        write_cache_access(cache, out);
    }
    if let Some(totals) = extensions.approval_totals {
        out.extend_from_slice(totals);
    }
    Ok(())
}

fn write_cache_access(cache: &CacheAccess, out: &mut Vec<u8>) {
    out.extend_from_slice(&cache.read_bitmap.to_le_bytes());
    for write in &cache.write_slots {
        out.extend_from_slice(&[write.output, write.slot]);
    }
}

fn push_bitmap(out: &mut Vec<u8>, bits: impl ExactSizeIterator<Item = bool>) {
    let mut bytes = vec![0u8; bits.len().div_ceil(8)];
    for (index, _) in bits.enumerate().filter(|(_, set)| *set) {
        bytes[index / 8] |= 1 << (index % 8);
    }
    out.extend_from_slice(&bytes);
}

fn push_tree_context(out: &mut Vec<u8>, context: TreeContext) -> Result<(), CustomRingError> {
    out.extend(
        <TreeContext as Serialize>::serialize(&context)
            .map_err(|_| CustomRingError::InvalidInstructionData)?,
    );
    Ok(())
}
