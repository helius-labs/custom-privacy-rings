//! Compact ring frames expand into canonical SPP bytes for settlement and external data hashing.
//!
//! Read after `program-libs/interface/src/instructions/transact.rs` and `program-libs/interface/src/codec/output.rs`.
//! Read before `programs/custom-ring/src/instructions/transfer/processor.rs`.

use spr_interface::instruction::{
    InputUtxo, MessageData, OwnerTag, TransactIxData, TransactOutput, TransactProof,
};

use super::{invalid, ParsedTransact, RingOutput, RingTransactData, SlotBody};
use crate::{
    error::CustomRingError, output::OutputPayload, tagged_x25519, AuditorMessage, RingPolicy,
};

impl ParsedTransact<'_> {
    pub fn spp_instruction_data(&self, ring: &RingPolicy) -> Result<Vec<u8>, CustomRingError> {
        self.data
            .spp_instruction_data_validated(ring, self.data.spp_proof_points()?)
    }
}

impl<'a> RingOutput<'a> {
    pub fn transact_output(&self, reply: &[u8]) -> Result<TransactOutput, CustomRingError> {
        Ok(TransactOutput {
            utxo_hash: *self.hash,
            owner_tag: OwnerTag::Inline(self.owner_tag),
            data: Some(self.payload(reply).encode()?),
        })
    }

    fn payload<'reply>(&self, reply: &'reply [u8]) -> OutputPayload<'reply>
    where
        'a: 'reply,
    {
        match self.body {
            SlotBody::Recipient(body) => OutputPayload::Recipient { body, reply },
            SlotBody::Exit(body) => OutputPayload::Confidential(body),
        }
    }
}

impl RingTransactData<'_> {
    pub fn auditor_message(&self) -> Vec<u8> {
        AuditorMessage::data(self.tx_viewing_u, self.auditor_ciphertexts())
    }

    pub fn spp_proof_points(&self) -> Result<TransactProof, CustomRingError> {
        let b = groth16_solana::decompression::decompress_g2(&self.spp_proof.proof_b)
            .map_err(|_| CustomRingError::InvalidSppProofPoint)?;
        Ok(TransactProof {
            a: self.spp_proof.proof_a,
            b,
            c: self.spp_proof.proof_c,
        })
    }

    pub fn spp_instruction_data_validated(
        &self,
        ring: &RingPolicy,
        proof: TransactProof,
    ) -> Result<Vec<u8>, CustomRingError> {
        let expanded = self.transact_ix_data_validated(ring, proof)?;
        let len = wincode::serialized_size(&expanded).map_err(invalid)?;
        let mut data = Vec::with_capacity(usize::try_from(len).map_err(invalid)?.saturating_add(1));
        data.push(self.rail.spp_tag());
        wincode::serialize_into(&mut data, &expanded).map_err(invalid)?;
        Ok(data)
    }

    pub fn transact_ix_data_validated(
        &self,
        ring: &RingPolicy,
        proof: TransactProof,
    ) -> Result<TransactIxData, CustomRingError> {
        // The auditor's `u` without its tag, the program takes it from the account and a frame names no reader.
        let view_tag = auditor_tag(ring);
        Ok(TransactIxData {
            expiry_unix_ts: u64::from(self.expiry_unix_ts),
            tx_viewing_pk: tagged_x25519(self.tx_viewing_u),
            salt: *self.salt,
            interface_transfers: self.interface_transfers.clone(),
            data_hash: self.extensions.data_hash.copied(),
            ring_data_hash: self.extensions.ring_data_hash.copied(),
            outputs: {
                let mut outputs = Vec::with_capacity(self.outputs.len());
                for (output, reply) in self.outputs.iter().zip(self.slot_replies()) {
                    outputs.push(output.transact_output(reply)?);
                }
                outputs
            },
            // The auditor message stays last.
            messages: self
                .extensions
                .messages
                .iter()
                .map(|message| MessageData {
                    view_tag: *message.view_tag,
                    data: message.data.to_vec(),
                })
                .chain([MessageData {
                    view_tag,
                    data: self.auditor_message(),
                }])
                .collect(),
            private_tx_hash: *self.private_tx_hash,
            circuit: self.circuit(),
            proof,
            inputs: self.input_utxos().collect(),
            tree_contexts: self.input_trees.contexts(),
        })
    }

    fn input_utxos(&self) -> impl Iterator<Item = InputUtxo> + '_ {
        self.nullifiers
            .iter()
            .enumerate()
            .map(|(input, nullifier)| InputUtxo {
                nullifier_hash: *nullifier,
                tree_index: self.input_trees.tree_index(input),
            })
    }

    /// The envelope, the recipient keys, then each exit's record in slot order, the records a scheme 3 body cannot carry.
    fn auditor_ciphertexts(&self) -> impl Iterator<Item = &[u8]> + Clone {
        let exits = self
            .outputs
            .iter()
            .filter(|output| output.is_exit())
            .map(|output| output.record);
        [self.envelope, self.recipient_keys]
            .into_iter()
            .chain(exits)
    }
}

fn auditor_tag(ring: &RingPolicy) -> [u8; 32] {
    core::array::from_fn(|index| ring.auditor_pubkey[index + 1])
}
