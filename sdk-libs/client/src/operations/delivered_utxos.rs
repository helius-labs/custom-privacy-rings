//! Slot secrets of the ring outputs under a wallet's viewing keys.

use custom_ring_interface::{
    ring_body_output, ring_sender_bundle_output_body,
    stream::{confidential_slot_tag, SlotSecret},
    SENDER_BUNDLE_LEN,
};
use solana_address::Address;
use spr_hasher::primitives::is_canonical_bn254_scalar_be;
use spr_keypair::{NullifierKey, ShieldedAddress, X25519Pubkey};
use spr_transaction::{
    DeriveRequest, RingCodec, RingOpening, ShieldedKeys, ShieldedTransaction, TransactionError,
    WalletUtxo,
};
use zolana_ring_keys::{RingDeliveryRequest, RingKeys, RingTagFrame, RingTagKind, RingTagProbe};
use zolana_ring_transaction::{
    is_contributory, open_recipient_body, AnonymousOutput, OpenedBody, RingAssets,
};

use crate::{exactly_one_each, is_zero_key, AnonymousError};

/// What every delivered slot secret of one ring transaction binds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingDeliveryContext {
    pub ring: Address,
    pub tx_viewing_pk: X25519Pubkey,
    pub salt: [u8; 16],
    pub first_nullifier: [u8; 32],
}

impl RingDeliveryContext {
    /// `None` without a ring, a contributory key, a salt or a canonical first nullifier.
    #[must_use]
    pub fn of(tx: &ShieldedTransaction) -> Option<Self> {
        let context = Self {
            ring: tx.ring_program_id?,
            tx_viewing_pk: tx.tx_viewing_pk?,
            salt: tx.salt?,
            first_nullifier: *tx.nullifiers.first()?,
        };
        (is_contributory(&context.tx_viewing_pk)
            && is_canonical_bn254_scalar_be(&context.first_nullifier))
        .then_some(context)
    }
}

pub(crate) fn assign_nullifiers<K: ShieldedKeys + ?Sized>(
    shielded_keys: &K,
    utxos: &mut [WalletUtxo],
) -> Result<(), AnonymousError> {
    if utxos.is_empty() {
        return Ok(());
    }
    let requests: Vec<_> = utxos
        .iter()
        .map(|input| DeriveRequest::Nullifier {
            utxo_hash: input.utxo_hash,
            blinding: input.utxo.blinding,
        })
        .collect();
    let nullifiers = exactly_one_each(shielded_keys.derive(&requests)?, utxos.len())?;
    let zero_secret = NullifierKey::from_secret([0; 31]);
    for (input, nullifier) in utxos.iter_mut().zip(nullifiers) {
        // A zero-key note spends under secret zero, never the wallet's.
        input.nullifier = match is_zero_key(input) {
            true => zero_secret.nullifier(&input.utxo_hash, &input.utxo.blinding)?,
            false => nullifier,
        };
    }
    Ok(())
}

/// Mirrors spr-transaction's private publication order.
pub(crate) fn sort_by_publication(utxos: &mut [WalletUtxo]) {
    utxos.sort_by_key(|utxo| {
        (
            utxo.slot,
            utxo.tx_signature.as_ref().to_vec(),
            utxo.slot_index,
        )
    });
}

/// Serves confidential and anonymous rings, a zero-tag body opens only as the sender's change.
pub struct RingNoteCodec<'a, K: ?Sized> {
    pub ring: Address,
    pub keys: &'a K,
    pub assets: &'a RingAssets,
}

impl<K: ShieldedKeys + RingKeys + ?Sized> RingCodec for RingNoteCodec<'_, K> {
    type Error = AnonymousError;

    fn ring_program_id(&self) -> Address {
        self.ring
    }

    fn open(&self, event: &ShieldedTransaction) -> Result<Vec<RingOpening>, AnonymousError> {
        let Some(context) = RingDeliveryContext::of(event) else {
            return Ok(Vec::new());
        };
        let address = self.keys.address()?;
        let bodies: Vec<(u32, [u8; 32])> = (0u32..)
            .zip(&event.output_slots)
            .filter(|(_, slot)| ring_body_output(&slot.payload).is_some())
            .map(|(position, slot)| (position, slot.view_tag))
            .collect();
        let bundle = event.messages.iter().find_map(|message| {
            ring_sender_bundle_output_body(&message.data)
                .filter(|bundle| bundle.len() == SENDER_BUNDLE_LEN)
                .map(|_| message.view_tag)
        });
        let bundle_index = u32::try_from(event.output_slots.len())
            .map_err(|_| TransactionError::TooManyOutputs)?;
        // 1. One frame per held key, a confidential body names its key by a public tag.
        let frames: Vec<RingTagFrame> = self
            .keys
            .viewing_public_keys()
            .into_iter()
            .map(|viewing_pubkey| {
                let confidential = confidential_slot_tag(viewing_pubkey.u());
                let tagged =
                    bodies
                        .iter()
                        .filter(|(_, tag)| *tag != [0; 32])
                        .map(|&(index, tag)| {
                            let kind = if tag == confidential {
                                RingTagKind::Confidential
                            } else {
                                RingTagKind::Recipient
                            };
                            RingTagProbe {
                                kind,
                                output_index: index,
                                tag,
                            }
                        });
                let own = bundle.map(|tag| RingTagProbe {
                    kind: RingTagKind::SenderBundle,
                    output_index: bundle_index,
                    tag,
                });
                RingTagFrame {
                    viewing_pubkey,
                    tx_viewing_pk: context.tx_viewing_pk,
                    salt: context.salt,
                    first_nullifier: context.first_nullifier,
                    probes: tagged.chain(own).collect(),
                }
            })
            .filter(|frame| !frame.probes.is_empty())
            .collect();
        if frames.is_empty() {
            return Ok(Vec::new());
        }
        let answers = exactly_one_each(self.keys.ring_tag_matches(&frames)?, frames.len())?;
        let mut secrets = Vec::new();
        let mut change = Vec::new();
        for (frame, answers) in frames.iter().zip(answers) {
            let answers = exactly_one_each(answers, frame.probes.len())?;
            for (probe, secret) in frame.probes.iter().zip(answers) {
                let Some(secret) = secret else {
                    continue;
                };
                if probe.kind != RingTagKind::SenderBundle {
                    secrets.push((probe.output_index, secret));
                    continue;
                }
                // 2. A matched bundle marks an own event, its change carries tag zero.
                let zero_tagged = bodies.iter().filter(|(_, tag)| *tag == [0; 32]);
                change.extend(zero_tagged.map(|&(index, _)| RingDeliveryRequest {
                    viewing_pubkey: frame.viewing_pubkey,
                    tx_viewing_pk: context.tx_viewing_pk,
                    salt: context.salt,
                    output_index: index,
                    first_nullifier: context.first_nullifier,
                }));
            }
        }
        if !change.is_empty() {
            let answers =
                exactly_one_each(self.keys.ring_delivery_secrets(&change)?, change.len())?;
            for (request, secret) in change.iter().zip(answers) {
                secrets.push((request.output_index, SlotSecret::new(secret)?));
            }
        }
        let mut openings = Vec::new();
        for (slot_index, slot_secret) in secrets {
            let candidate = RingDeliveredCandidate {
                tx: event,
                slot_index,
                slot_secret,
            };
            let (OpenedBody::Note(note) | OpenedBody::UnknownMint(note)) =
                match open_recipient_candidate(&address, self.assets, &candidate)? {
                    Some(opened) => opened,
                    None => continue,
                };
            openings.push(RingOpening {
                slot_index,
                nullifier_pubkey: note.nullifier_pk,
                mint: note.utxo.asset.asset,
                amount: note.utxo.amount,
                blinding: note.utxo.blinding,
                data: note.utxo.data,
                data_hash: note.data_hash,
                ring_data_hash: note.ring_data_hash,
            });
        }
        Ok(openings)
    }
}

/// One delivered ring output and the slot secret its recipient agreed for it.
struct RingDeliveredCandidate<'a> {
    tx: &'a ShieldedTransaction,
    slot_index: u32,
    slot_secret: SlotSecret,
}

/// `None` off a ring body output or on a body that rebuilds nothing.
fn open_recipient_candidate(
    address: &ShieldedAddress,
    assets: &RingAssets,
    candidate: &RingDeliveredCandidate<'_>,
) -> Result<Option<OpenedBody>, AnonymousError> {
    let tx = candidate.tx;
    let Some(slot) = tx.output_slots.get(candidate.slot_index as usize) else {
        return Ok(None);
    };
    let (Some((ciphertext, _)), Some(context)) =
        (ring_body_output(&slot.payload), RingDeliveryContext::of(tx))
    else {
        return Ok(None);
    };
    let Ok(body) = open_recipient_body(ciphertext, &candidate.slot_secret) else {
        return Ok(None);
    };
    Ok(AnonymousOutput {
        ring: context.ring,
        owner: address,
        utxo_hash: &slot.output_context.hash,
        tree_id: slot.output_context.tree_id,
        assets,
    }
    .open_body(&body)
    .unwrap_or(None))
}
