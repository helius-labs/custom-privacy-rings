//! Slot secrets of the ring outputs under a wallet's viewing keys.

use solana_address::Address;
use spr_hasher::primitives::is_canonical_bn254_scalar_be;
use spr_keypair::{NullifierKey, ShieldedAddress, X25519Pubkey};
use spr_transaction::{
    DeriveRequest, RingCodec, RingOpening, ShieldedKeys, ShieldedTransaction, WalletUtxo,
};
use zolana_ring_keys::ring_body_output;
use zolana_ring_keys::SlotSecret;
use zolana_ring_keys::{RingDeliveryRequest, RingKeys};
use zolana_ring_transaction::{is_contributory, AnonymousOutput, OpenedBody, RingAssets};

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

/// Serves confidential and anonymous rings, each body tried under every held key.
pub struct RingNoteCodec<'a, K: ?Sized> {
    pub ring: Address,
    pub keys: &'a K,
    pub assets: &'a RingAssets,
}

/// One ring body a held key rebuilt, with the secret its reply opens under.
pub(crate) struct OpenedSlot {
    pub viewing_pubkey: X25519Pubkey,
    pub secret: SlotSecret,
    pub opening: RingOpening,
}

impl<K: ShieldedKeys + RingKeys + ?Sized> RingCodec for RingNoteCodec<'_, K> {
    type Error = AnonymousError;

    fn ring_program_id(&self) -> Address {
        self.ring
    }

    fn open(&self, event: &ShieldedTransaction) -> Result<Vec<RingOpening>, AnonymousError> {
        Ok(self
            .opened(event)?
            .into_iter()
            .map(|slot| slot.opening)
            .collect())
    }
}

impl<K: ShieldedKeys + RingKeys + ?Sized> RingNoteCodec<'_, K> {
    /// No tag selects a body, a rebuilt commitment alone names the owner.
    pub(crate) fn opened(
        &self,
        event: &ShieldedTransaction,
    ) -> Result<Vec<OpenedSlot>, AnonymousError> {
        let Some(context) = RingDeliveryContext::of(event) else {
            return Ok(Vec::new());
        };
        let address = self.keys.address()?;
        let bodies: Vec<u32> = (0u32..)
            .zip(&event.output_slots)
            .filter(|(_, slot)| ring_body_output(&slot.payload).is_some())
            .map(|(position, _)| position)
            .collect();
        // One key's requests run together, the holder agrees once per key.
        let requests: Vec<RingDeliveryRequest> = self
            .keys
            .viewing_public_keys()
            .into_iter()
            .flat_map(|viewing_pubkey| {
                bodies.iter().map(move |&output_index| RingDeliveryRequest {
                    viewing_pubkey,
                    tx_viewing_pk: context.tx_viewing_pk,
                    salt: context.salt,
                    output_index,
                    first_nullifier: context.first_nullifier,
                })
            })
            .collect();
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let secrets =
            exactly_one_each(self.keys.ring_delivery_secrets(&requests)?, requests.len())?;
        let mut opened = Vec::new();
        for (request, secret) in requests.iter().zip(secrets) {
            let candidate = RingDeliveredCandidate {
                tx: event,
                slot_index: request.output_index,
                slot_secret: SlotSecret::new(secret)?,
            };
            let Some(OpenedBody::Note(note) | OpenedBody::UnknownMint(note)) =
                open_recipient_candidate(&address, self.assets, &candidate)?
            else {
                continue;
            };
            opened.push(OpenedSlot {
                viewing_pubkey: request.viewing_pubkey,
                secret: candidate.slot_secret,
                opening: RingOpening {
                    slot_index: request.output_index,
                    nullifier_pubkey: note.nullifier_pk,
                    mint: note.utxo.asset.asset,
                    amount: note.utxo.amount,
                    blinding: note.utxo.blinding,
                    data: note.utxo.data,
                    data_hash: note.data_hash,
                    ring_data_hash: note.ring_data_hash,
                },
            });
        }
        Ok(opened)
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
    Ok(AnonymousOutput {
        ring: context.ring,
        owner: address,
        utxo_hash: &slot.output_context.hash,
        tree_id: slot.output_context.tree_id,
        assets,
    }
    .open(ciphertext, &candidate.slot_secret)
    .unwrap_or(None))
}
