//! The anonymous rail's slots, the sender's change leading at output 0 and one record body per
//! non-exit slot.

use custom_ring_interface::{stream::slot_tag, MIN_REPLY_LEN};
use rand::{rngs::OsRng, RngCore};
use spr_interface::instruction::instruction_data::transact::TransactOutput;
use spr_keypair::{viewing_key::ViewTag, ShieldedAddress, X25519Pubkey};
use spr_transaction::{
    instructions::transact::SppProofInputs, Mint, SppProofOutputUtxo, TransactionError,
};
use zolana_ring_keys::bootstrap_slot_tag;

use crate::{recipient_delivery::seal_reply, OutputBlindings, ReplyMetadata, RingRailError};

#[derive(Clone, Debug)]
struct Payment {
    recipient: X25519Pubkey,
    tag: ViewTag,
    reply: Option<ReplyMetadata>,
}

/// The payments of one anonymous transfer in declared order.
#[must_use]
#[derive(Clone, Debug)]
pub struct AnonymousPlan {
    payments: Vec<Payment>,
    /// One per change slot in slot order, a change slot past them takes random bytes.
    change_tags: Vec<ViewTag>,
    change_asset: Mint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnonymousSlot {
    /// A live output at the sender's exact address, its record sealed to the sender.
    Change,
    Payment {
        recipient: X25519Pubkey,
        tag: ViewTag,
    },
    Exit,
    Dummy,
}

impl AnonymousPlan {
    pub fn new() -> Self {
        Self {
            payments: Vec::new(),
            change_tags: Vec::new(),
            change_asset: Mint::SOL,
        }
    }

    /// A relay's plan, the sealed outputs already publish the wallet's tags.
    pub fn from_recipients(recipients: impl IntoIterator<Item = X25519Pubkey>) -> Self {
        recipients.into_iter().fold(Self::new(), Self::with_payment)
    }

    /// Under the recipient's bootstrap tag until a later call replaces it.
    pub fn with_payment(mut self, recipient: X25519Pubkey) -> Self {
        self.payments.push(Payment {
            recipient,
            tag: bootstrap_slot_tag(recipient.u()),
            reply: None,
        });
        self
    }

    /// Tags the latest payment, at the width a ring slot publishes.
    pub fn with_tag(mut self, tag: ViewTag) -> Self {
        if let Some(payment) = self.payments.last_mut() {
            payment.tag = slot_tag(tag);
        }
        self
    }

    /// No tag finds the latest payment, a note its recipient spends in the same frame does.
    pub fn with_random_tag(mut self) -> Self {
        if let Some(payment) = self.payments.last_mut() {
            payment.tag = random_slot_tag();
        }
        self
    }

    /// Attaches a reply to the latest payment, no other path sets one.
    pub fn with_reply(mut self, reply: ReplyMetadata) -> Self {
        if let Some(payment) = self.payments.last_mut() {
            payment.reply = Some(reply);
        }
        self
    }

    /// A tag one change slot publishes, each call names the next change slot.
    pub fn with_sender_tag(mut self, tag: ViewTag) -> Self {
        self.change_tags.push(slot_tag(tag));
        self
    }

    /// The asset of the zero change a frame leads with when nothing returns, SOL unless set.
    pub fn with_change_asset(mut self, asset: Mint) -> Self {
        self.change_asset = asset;
        self
    }

    pub fn change_asset(&self) -> Mint {
        self.change_asset
    }

    pub fn recipients(&self) -> impl Iterator<Item = X25519Pubkey> + '_ {
        self.payments.iter().map(|payment| payment.recipient)
    }

    /// One length per frame, a slot without a reply carries random bytes of it.
    pub fn reply_len(&self) -> Result<Option<usize>, RingRailError> {
        self.payments
            .iter()
            .filter_map(|payment| payment.reply.as_ref())
            .map(ReplyMetadata::encoded_len)
            .try_fold(None, |longest: Option<usize>, len| {
                Ok(Some(longest.unwrap_or(MIN_REPLY_LEN).max(len?)))
            })
    }

    /// Change leads, every other ring output takes the next payment in declared order.
    pub fn slots(
        &self,
        outputs: &[SppProofOutputUtxo],
        sender: &ShieldedAddress,
    ) -> Result<Vec<AnonymousSlot>, RingRailError> {
        let published = published_outputs(outputs);
        let change = leading_change(published, sender);
        let mut payments = self.payments.iter();
        let mut slots = Vec::with_capacity(published.len());
        for (slot_index, output) in published.iter().enumerate() {
            let slot = match output.owner_address {
                _ if slot_index < change => AnonymousSlot::Change,
                None => AnonymousSlot::Dummy,
                Some(_) if is_change(output, sender) => {
                    return Err(RingRailError::AnonymousChange { slot_index });
                }
                Some(owner) if output.ring_program_id.is_none() => {
                    if owner.signing_pubkey == sender.signing_pubkey {
                        return Err(RingRailError::RingSenderExit { slot_index });
                    }
                    AnonymousSlot::Exit
                }
                Some(owner) => match payments.next() {
                    Some(payment) if payment.recipient == owner.viewing_pubkey => {
                        AnonymousSlot::Payment {
                            recipient: payment.recipient,
                            tag: payment.tag,
                        }
                    }
                    Some(_) => return Err(RingRailError::RingPaymentSlot { slot_index }),
                    None => return Err(RingRailError::UnplannedRingOutput { slot_index }),
                },
            };
            slots.push(slot);
        }
        match payments.next() {
            Some(_) => Err(RingRailError::RingPaymentSlot {
                slot_index: published.len(),
            }),
            None => Ok(slots),
        }
    }

    /// The tag each slot publishes, `None` on an exit since its owner tag is no view tag.
    /// A frame publishes each tag once and random bytes on a slot that would repeat one, a match
    /// returns every slot of the transaction and equal tags would show two slots share an owner.
    pub(crate) fn tags(&self, slots: &[AnonymousSlot]) -> Vec<Option<ViewTag>> {
        let mut published: Vec<ViewTag> = Vec::new();
        let mut once = |tag: ViewTag| match published.contains(&tag) {
            true => random_slot_tag(),
            false => {
                published.push(tag);
                tag
            }
        };
        let mut change_tags = self.change_tags.iter().copied();
        slots
            .iter()
            .map(|slot| match slot {
                AnonymousSlot::Exit => None,
                AnonymousSlot::Change => {
                    Some(change_tags.next().map_or_else(random_slot_tag, &mut once))
                }
                AnonymousSlot::Dummy => Some(random_slot_tag()),
                AnonymousSlot::Payment { tag, .. } => Some(once(*tag)),
            })
            .collect()
    }

    fn slot_replies(&self, slots: &[AnonymousSlot]) -> Vec<Option<&ReplyMetadata>> {
        let mut payments = self.payments.iter();
        slots
            .iter()
            .map(|slot| match slot {
                AnonymousSlot::Payment { .. } => {
                    payments.next().and_then(|payment| payment.reply.as_ref())
                }
                _ => None,
            })
            .collect()
    }
}

impl Default for AnonymousPlan {
    fn default() -> Self {
        Self::new()
    }
}

impl AnonymousSlot {
    pub const fn is_change(&self) -> bool {
        matches!(self, Self::Change)
    }
}

/// The added change leads, SPL ahead of SOL, then each dataless output `declared` to the sender.
pub(crate) fn lead_change(
    inputs: &mut SppProofInputs,
    sender: &ShieldedAddress,
    declared: &[SppProofOutputUtxo],
) -> Result<(), RingRailError> {
    let live =
        |outputs: &[SppProofOutputUtxo]| outputs.iter().filter(|output| !output.is_dummy()).count();
    let appended = inputs
        .output_utxos
        .iter()
        .zip(declared)
        .all(|(sealed, declared)| note_of(sealed) == note_of(declared));
    // SPP appends its change today, a builder that leads with it keeps the same frame.
    let first = match appended {
        true => declared.len(),
        false => 0,
    };
    let added = first..first + live(&inputs.output_utxos).saturating_sub(live(declared));
    let rank = |index: usize| {
        let output = &inputs.output_utxos[index];
        match added.contains(&index) {
            true if output.asset != Mint::SOL => 0,
            true => 1,
            false if is_change(output, sender) => 2,
            false => 3,
        }
    };
    let mut order: Vec<usize> = (0..inputs.output_utxos.len()).collect();
    order.sort_by_key(|index| rank(*index));
    let first_nullifier = inputs.first_nullifier()?;
    let blindings = OutputBlindings::new(&first_nullifier, &inputs.blinding_seed)?;
    inputs.output_utxos = order
        .iter()
        .zip(0u32..)
        .map(|(index, slot_index)| {
            Ok(SppProofOutputUtxo {
                blinding: blindings.at(slot_index)?,
                ..inputs.output_utxos[*index].clone()
            })
        })
        .collect::<Result<_, RingRailError>>()?;
    // Change never sits in the omitted compact suffix, the published prefix permutes within itself.
    let external = &mut inputs.external_data;
    let published = order
        .iter()
        .filter(|index| **index < external.outputs.len());
    external.resolved_owner_tags = published
        .clone()
        .map(|index| external.resolved_owner_tags[*index])
        .collect();
    external.outputs = published
        .zip(&inputs.output_utxos)
        .map(|(index, output)| {
            Ok(TransactOutput {
                utxo_hash: output.hash(inputs.output_tree_id)?,
                ..external.outputs[*index].clone()
            })
        })
        .collect::<Result<_, RingRailError>>()?;
    Ok(())
}

pub(crate) struct AnonymousSeal<'a> {
    pub rail: &'a crate::RingOutputRail,
    pub first_nullifier: [u8; 32],
    pub outputs: &'a [SppProofOutputUtxo],
    pub slots: &'a [AnonymousSlot],
    pub plan: &'a AnonymousPlan,
    pub form: crate::BodyForm,
}

/// One non-exit slot's bytes, its reply empty without reply metadata.
pub(crate) struct SealedBody {
    pub body: Vec<u8>,
    pub reply: Vec<u8>,
}

impl AnonymousSeal<'_> {
    /// The record body of a tagged slot sealed at its output position, an exit carries none.
    pub fn body(&self, slot_index: usize) -> Result<SealedBody, RingRailError> {
        let reply_len = self.plan.reply_len()?;
        if self.slots[slot_index] == AnonymousSlot::Exit {
            return Err(RingRailError::RingOutputFraming { slot_index });
        }
        let output = &self.outputs[slot_index];
        let body = self
            .rail
            .seal_body(output, &self.first_nullifier, slot_index, self.form)?;
        let replies = self.plan.slot_replies(self.slots);
        let reply = match (self.slots[slot_index], replies[slot_index], reply_len) {
            (AnonymousSlot::Payment { recipient, .. }, Some(reply), Some(reply_len)) => {
                let index =
                    u32::try_from(slot_index).map_err(|_| TransactionError::TooManyOutputs)?;
                let secret = self
                    .rail
                    .slot_secret(&recipient, &self.first_nullifier, index)?;
                seal_reply(reply, reply_len, &secret)?
            }
            _ => random_bytes(reply_len.unwrap_or(0)),
        };
        Ok(SealedBody { body, reply })
    }
}

/// The outputs the frame publishes, compact padding is an omitted suffix.
pub fn published_outputs(outputs: &[SppProofOutputUtxo]) -> &[SppProofOutputUtxo] {
    let omitted = outputs
        .iter()
        .rev()
        .take_while(|output| output.is_compact())
        .count();
    &outputs[..outputs.len() - omitted]
}

/// Uniform over the published bytes, as a sender tag and a request tag are.
fn random_slot_tag() -> ViewTag {
    let mut tag = [0u8; 32];
    OsRng.fill_bytes(&mut tag);
    slot_tag(tag)
}

fn random_bytes(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

fn leading_change(outputs: &[SppProofOutputUtxo], sender: &ShieldedAddress) -> usize {
    outputs
        .iter()
        .take_while(|output| is_change(output, sender))
        .count()
}

fn note_of(output: &SppProofOutputUtxo) -> (Option<ShieldedAddress>, Mint, u64) {
    (output.owner_address, output.asset, output.amount)
}

/// A change carries no data, a data note to the sender takes a payment slot.
pub(crate) fn is_change(output: &SppProofOutputUtxo, sender: &ShieldedAddress) -> bool {
    let carries_data = [output.data_hash, output.ring_data_hash]
        .into_iter()
        .flatten()
        .any(|hash| hash != [0; 32]);
    output.owner_address == Some(*sender) && output.ring_program_id.is_some() && !carries_data
}
