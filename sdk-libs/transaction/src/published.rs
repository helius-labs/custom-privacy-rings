//! The slots a sealed ring transfer publishes, in the compact form the ring program expands. A
//! non-exit slot's record leads its body, an exit's record goes to the auditor message.

use custom_ring_interface::{RingOutput, SlotBody, TRANSFER_RECORD_LEN};
use spr_event::confidential_encrypted_output_body;
use spr_interface::instruction::instruction_data::transact::{OwnerTag, TransactOutput};
use spr_keypair::ShieldedAddress;
use spr_transaction::{instructions::transact::SppProofInputs, SppProofOutputUtxo};
use zolana_ring_keys::ring_body_output;

use crate::{
    anonymous_rail::published_outputs, record_of, AnonymousSlot, BodyForm, ExitSlot, RingDelivery,
    RingOutputRail, RingRailError, Slot,
};

#[derive(Clone, Copy, Debug)]
pub struct PublishedSlot<'a> {
    pub output: &'a SppProofOutputUtxo,
    pub hash: &'a [u8; 32],
    pub owner_tag: &'a [u8; 32],
    pub body: SlotBody<'a>,
    /// The record ciphertext the statement binds, zero padded past `record_len`.
    record: [u8; TRANSFER_RECORD_LEN],
    record_len: usize,
    /// Set on the anonymous rail.
    pub anonymous: Option<AnonymousSlot>,
    /// The sealed reply beside a recipient body, empty without reply metadata.
    pub reply: &'a [u8],
}

impl<'a> PublishedSlot<'a> {
    /// [`SlotBody::record_len`] bytes under the frame's form.
    pub fn record(&self) -> &[u8] {
        &self.record[..self.record_len]
    }

    pub fn wire(&self) -> RingOutput<'_> {
        RingOutput {
            hash: self.hash,
            owner_tag: *self.owner_tag,
            body: self.body,
            record: self.record(),
        }
    }
}

impl RingOutputRail {
    /// Every published slot in order, compact padding omitted, each expanding to exactly the output the SPP proof binds.
    pub fn published_slots<'a>(
        &self,
        inputs: &'a SppProofInputs,
        sender: &ShieldedAddress,
    ) -> Result<Vec<PublishedSlot<'a>>, RingRailError> {
        let outputs = published_outputs(&inputs.output_utxos);
        let published = &inputs.external_data.outputs;
        if published.len() != outputs.len() {
            return Err(RingRailError::RingOutputFraming {
                slot_index: outputs.len().min(published.len()),
            });
        }
        let anonymous = match &self.delivery {
            RingDelivery::Anonymous(plan) => Some(plan.slots(outputs, sender)?),
            RingDelivery::Delivered => None,
        };
        let first_nullifier = inputs.first_nullifier()?;
        let form = BodyForm::of(outputs)?.records;
        outputs
            .iter()
            .zip(published)
            .enumerate()
            .map(|(index, (output, published))| {
                let anonymous = anonymous.as_ref().map(|slots| slots[index]);
                let (body, reply) = match anonymous {
                    Some(slot) => Self::anonymous_body(slot, published, index)?,
                    None => (self.delivered_body(output, published, index)?, &[][..]),
                };
                let (hash, owner_tag) = inline(published, index)?;
                let exit_record;
                let published_record = match body {
                    SlotBody::Recipient(body) => record_of(body, form, index)?,
                    SlotBody::Exit(_) => {
                        exit_record =
                            self.exit_record(output, &first_nullifier, ExitSlot { index, form })?;
                        &exit_record[..]
                    }
                };
                let record_len = body.record_len(form);
                if published_record.len() != record_len {
                    return Err(RingRailError::RingOutputFraming { slot_index: index });
                }
                let mut record = [0; TRANSFER_RECORD_LEN];
                record[..record_len].copy_from_slice(published_record);
                let slot = PublishedSlot {
                    output,
                    hash,
                    owner_tag,
                    body,
                    record,
                    record_len,
                    anonymous,
                    reply,
                };
                if slot.wire().transact_output(reply)? != *published {
                    return Err(RingRailError::RingOutputFraming { slot_index: index });
                }
                Ok(slot)
            })
            .collect()
    }

    /// The body kind each anonymous slot publishes, any other is refused.
    /// The tag is not checked, the sealing wallet chose it and a relay's plan does not carry it.
    fn anonymous_body<'a>(
        slot: AnonymousSlot,
        published: &'a TransactOutput,
        slot_index: usize,
    ) -> Result<(SlotBody<'a>, &'a [u8]), RingRailError> {
        let framing = || RingRailError::RingOutputFraming { slot_index };
        let data = published.data.as_deref();
        if slot == AnonymousSlot::Exit {
            let payload = data
                .and_then(confidential_encrypted_output_body)
                .ok_or_else(framing)?;
            return Ok((SlotBody::Exit(payload), &[]));
        }
        let (body, reply) = data.and_then(ring_body_output).ok_or_else(framing)?;
        Ok((SlotBody::Recipient(body), reply))
    }

    fn delivered_body<'a>(
        &self,
        output: &SppProofOutputUtxo,
        published: &'a TransactOutput,
        slot_index: usize,
    ) -> Result<SlotBody<'a>, RingRailError> {
        let framing = || RingRailError::RingOutputFraming { slot_index };
        let data = published.data.as_deref().ok_or_else(framing)?;
        Ok(match self.slot(output) {
            Slot::Exit => {
                SlotBody::Exit(confidential_encrypted_output_body(data).ok_or_else(framing)?)
            }
            Slot::Ring { .. } | Slot::Dummy => {
                SlotBody::Recipient(ring_body_output(data).ok_or_else(framing)?.0)
            }
        })
    }
}

fn inline(
    published: &TransactOutput,
    slot_index: usize,
) -> Result<(&[u8; 32], &[u8; 32]), RingRailError> {
    match &published.owner_tag {
        OwnerTag::Inline(owner_tag) => Ok((&published.utxo_hash, owner_tag)),
        _ => Err(RingRailError::RingOutputFraming { slot_index }),
    }
}
