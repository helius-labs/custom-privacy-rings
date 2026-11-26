//! Ring bodies, sender bundles and reply metadata, each under its own label over a CRDS slot secret.
//!
//! A ring body is the slot's proven record and its unproven payload, one stream. The proof binds the
//! record, its data hashes included, to the slot's opening and its ciphertext to the statement. The
//! payload holds the application data alone and stays the sender's word.

use custom_ring_interface::{RecordForm, RecordKind, SENDER_BUNDLE_LEN};
use solana_address::Address;
use spr_hasher::primitives::is_canonical_x25519_public_key;
use spr_interface::ZERO_NULLIFIER_PUBKEY;
use spr_keypair::{ShieldedAddress, X25519Keypair, X25519Pubkey};
use spr_transaction::{
    derive_output_blinding_seed, utxo::derive_transact_output_blinding, Blinding, Data, DataRecord,
    Mint, SppProofOutputUtxo, Utxo,
};
use zeroize::Zeroizing;
use zolana_ring_keys::symmetric_apply_audit;
use zolana_ring_keys::OutputRecord;
use zolana_ring_keys::RecipientBodyForms;
use zolana_ring_keys::RecipientPayload;
use zolana_ring_keys::RecordKey;
use zolana_ring_keys::SenderBundle;
use zolana_ring_keys::SlotSecret;
use zolana_ring_keys::RECIPIENT_BODY_LABEL;
use zolana_ring_keys::RECORD_ZERO_KEY_BIT;
use zolana_ring_keys::REPLY_LABEL;
use zolana_ring_keys::SENDER_BUNDLE_LABEL;

use crate::{RingAssets, RingRailError};

/// `viewing_u || owner_key || nullifier_pk`.
const REPLY_HEAD_LEN: usize = 3 * 32;

/// Opt in per frame, the statement binds its bytes and proves nothing about its meaning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyMetadata {
    pub viewing_key: X25519Pubkey,
    pub owner_key: [u8; 32],
    pub nullifier_pk: [u8; 32],
    pub data: Data,
}

/// A decrypted reply, its sender claim stays unverified until the wallet confirms it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyClaim(ReplyMetadata);

impl ReplyClaim {
    pub fn claimed(&self) -> &ReplyMetadata {
        &self.0
    }
}

impl ReplyMetadata {
    /// The head and the wincode of the data, no padding.
    pub(crate) fn encoded_len(&self) -> Result<usize, RingRailError> {
        let data = wincode::serialized_size(&self.data)
            .map_err(spr_transaction::TransactionError::from)?;
        usize::try_from(data)
            .map(|data| REPLY_HEAD_LEN + data)
            .map_err(|_| RingRailError::ReplyMetadataLength {
                reply_len: usize::MAX,
            })
    }

    fn encode(&self, reply_len: usize) -> Result<Zeroizing<Vec<u8>>, RingRailError> {
        self.data.validate()?;
        let data = Zeroizing::new(
            wincode::serialize(&self.data).map_err(spr_transaction::TransactionError::from)?,
        );
        if reply_len < REPLY_HEAD_LEN + data.len() {
            return Err(RingRailError::ReplyMetadataLength { reply_len });
        }
        let mut bytes = Zeroizing::new(Vec::with_capacity(reply_len));
        bytes.extend_from_slice(&self.viewing_key.u());
        bytes.extend_from_slice(&self.owner_key);
        bytes.extend_from_slice(&self.nullifier_pk);
        bytes.extend_from_slice(&data);
        bytes.resize(reply_len, 0);
        Ok(bytes)
    }

    /// A nonzero padding byte or a zero or non-canonical `viewing_u` opens nothing, as enrollment refuses it.
    fn decode(bytes: &[u8]) -> Option<Self> {
        let (viewing_u, rest) = bytes.split_first_chunk::<32>()?;
        if *viewing_u == [0; 32] || !is_canonical_x25519_public_key(viewing_u) {
            return None;
        }
        let (owner_key, rest) = rest.split_first_chunk::<32>()?;
        let (nullifier_pk, rest) = rest.split_first_chunk::<32>()?;
        let data: Data = wincode::deserialize(rest).ok()?;
        data.validate().ok()?;
        let used = usize::try_from(wincode::serialized_size(&data).ok()?).ok()?;
        if rest.get(used..)?.iter().any(|byte| *byte != 0) {
            return None;
        }
        Some(Self {
            viewing_key: X25519Pubkey::from_u(*viewing_u).ok()?,
            owner_key: *owner_key,
            nullifier_pk: *nullifier_pk,
            data,
        })
    }
}

/// A body its reader decrypted, the record still unchecked against the commitment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedRecipientBody {
    pub record: OutputRecord,
    pub payload: PayloadRead,
}

/// The unproven bytes past the record, a malformed one leaves the proven record whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PayloadRead {
    /// A frame without data.
    Absent,
    Opened(RecipientPayload),
    Malformed,
}

/// The key a record of `kind` names for `nullifier_pk`, a ring output's nonzero key stays unpublished.
pub fn record_key(kind: RecordKind, nullifier_pk: [u8; 32]) -> RecordKey {
    match kind {
        RecordKind::Exit => carried_key(nullifier_pk),
        _ if nullifier_pk == ZERO_NULLIFIER_PUBKEY => RecordKey::Zero,
        RecordKind::Ring | RecordKind::Dummy => RecordKey::Unpublished,
    }
}

/// The key of an exit or merge record, the zero key named by its bit alone.
pub fn carried_key(nullifier_pk: [u8; 32]) -> RecordKey {
    match nullifier_pk == ZERO_NULLIFIER_PUBKEY {
        true => RecordKey::Zero,
        false => RecordKey::Carried(nullifier_pk),
    }
}

/// The nullifier key a reader rebuilds `record` under, `None` on a ring output under a nonzero key.
///
/// Readers take a ring output's nonzero key from the recipient's own address or the owner's
/// tracking leaf, never from the record.
pub fn published_nullifier_pk(record: &OutputRecord) -> Option<[u8; 32]> {
    match record.key {
        RecordKey::Zero => Some(ZERO_NULLIFIER_PUBKEY),
        RecordKey::Carried(key) => Some(key),
        RecordKey::Unpublished => None,
    }
}

/// One slot's record under the frame's form, then the payload padded to the frame's width.
#[must_use]
#[derive(Clone, Copy, Debug)]
pub struct RecipientBody<'a> {
    pub record: &'a OutputRecord,
    pub form: RecordForm,
    /// The payload and the frame's width, absent on an exit and on a frame without data.
    pub payload: Option<(&'a RecipientPayload, usize)>,
}

impl RecipientBody<'_> {
    /// One slot secret keys one body.
    pub fn seal(self, slot_secret: &SlotSecret) -> Result<Vec<u8>, RingRailError> {
        let mut plaintext = Zeroizing::new(self.record.published(self.form));
        if let Some((payload, width)) = self.payload {
            plaintext.extend_from_slice(&payload.encode_padded(width)?);
        }
        let mut ciphertext = plaintext.to_vec();
        symmetric_apply_audit(
            slot_secret.as_bytes(),
            RECIPIENT_BODY_LABEL,
            &mut ciphertext,
        )?;
        Ok(ciphertext)
    }
}

/// The cipher is unauthenticated, only the commitment a reader rebuilds proves ownership.
pub fn open_recipient_body(
    ciphertext: &[u8],
    slot_secret: &SlotSecret,
    form: RecordForm,
) -> Result<OpenedRecipientBody, RingRailError> {
    let mut plaintext = Zeroizing::new(ciphertext.to_vec());
    symmetric_apply_audit(slot_secret.as_bytes(), RECIPIENT_BODY_LABEL, &mut plaintext)?;
    let kind = match plaintext.first() {
        Some(kind) if kind & !RECORD_ZERO_KEY_BIT == RecordKind::Exit as u8 => RecordKind::Exit,
        _ => RecordKind::Ring,
    };
    let (record, payload) = plaintext
        .split_at_checked(form.published_len(kind))
        .ok_or(custom_ring_interface::CustomRingError::RingTransactLength)?;
    Ok(OpenedRecipientBody {
        record: OutputRecord::decode(record, form)?,
        payload: match payload.is_empty() {
            true => PayloadRead::Absent,
            false => RecipientPayload::decode(payload)
                .map_or(PayloadRead::Malformed, PayloadRead::Opened),
        },
    })
}

/// The application data `output` sends beside its record.
pub fn recipient_payload(output: &SppProofOutputUtxo) -> RecipientPayload {
    RecipientPayload {
        data: output.data.utxo_data().unwrap_or_default().to_vec(),
    }
}

/// `slot_secret` sits at index `m`, past every body's index.
pub fn seal_sender_bundle(
    bundle: &SenderBundle,
    slot_secret: &SlotSecret,
) -> Result<Vec<u8>, RingRailError> {
    let mut ciphertext = bundle.encode().to_vec();
    symmetric_apply_audit(slot_secret.as_bytes(), SENDER_BUNDLE_LABEL, &mut ciphertext)?;
    Ok(ciphertext)
}

pub fn open_sender_bundle(
    ciphertext: &[u8],
    slot_secret: &SlotSecret,
) -> Result<SenderBundle, RingRailError> {
    if ciphertext.len() != SENDER_BUNDLE_LEN {
        return Err(RingRailError::SenderBundleLength {
            len: ciphertext.len(),
        });
    }
    let mut plaintext = Zeroizing::new(ciphertext.to_vec());
    symmetric_apply_audit(slot_secret.as_bytes(), SENDER_BUNDLE_LABEL, &mut plaintext)?;
    Ok(SenderBundle::decode(&plaintext)?)
}

/// Shares slot `i`'s secret, its own label keeps the body's counter from restarting.
pub fn seal_reply(
    reply: &ReplyMetadata,
    reply_len: usize,
    slot_secret: &SlotSecret,
) -> Result<Vec<u8>, RingRailError> {
    let mut ciphertext = reply.encode(reply_len)?.to_vec();
    symmetric_apply_audit(slot_secret.as_bytes(), REPLY_LABEL, &mut ciphertext)?;
    Ok(ciphertext)
}

/// `None` on a reply that never decodes, a hidden dummy reads as random bytes.
pub fn open_reply(
    ciphertext: &[u8],
    slot_secret: &SlotSecret,
) -> Result<Option<ReplyClaim>, RingRailError> {
    let mut plaintext = Zeroizing::new(ciphertext.to_vec());
    symmetric_apply_audit(slot_secret.as_bytes(), REPLY_LABEL, &mut plaintext)?;
    Ok(ReplyMetadata::decode(&plaintext).map(ReplyClaim))
}

/// One published anonymous output, rebuilt from a body and the reader's keys.
#[derive(Clone, Copy, Debug)]
pub struct AnonymousOutput<'a> {
    pub owner: &'a ShieldedAddress,
    pub assets: &'a RingAssets,
    pub ring: Address,
    pub tree_id: u16,
    pub utxo_hash: &'a [u8; 32],
}

/// A note its reader rebuilt, a match establishes ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedNote {
    pub utxo: Utxo,
    /// The reader's own key or the zero key, the owner spends the note once that key is its registered one.
    pub nullifier_pk: [u8; 32],
    pub data_hash: Option<[u8; 32]>,
    pub ring_data_hash: Option<[u8; 32]>,
}

/// A body whose rebuilt commitment names the reader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenedBody {
    Note(OpenedNote),
    /// The commitment rebuilds under a mint the registry does not name, `utxo` holds its asset id at zero.
    UnknownMint(OpenedNote),
}

/// `TXOB(first_nullifier, TXOS(first_nullifier, blinding_seed), position)` per output.
pub struct OutputBlindings {
    first_nullifier: [u8; 32],
    output_seed: Zeroizing<[u8; 32]>,
}

impl AnonymousOutput<'_> {
    /// Opens a body with no frame at hand, each form its length admits tried against the commitment.
    pub fn open(
        &self,
        ciphertext: &[u8],
        slot_secret: &SlotSecret,
    ) -> Result<Option<OpenedBody>, RingRailError> {
        let mut without_data = None;
        for form in RecordForm::of_recipient_body(ciphertext.len()) {
            let Ok(body) = open_recipient_body(ciphertext, slot_secret, form) else {
                continue;
            };
            // A form whose payload decodes wins, a malformed one still yields the note.
            match (self.open_body(&body)?, &body.payload) {
                (Some(opened), PayloadRead::Malformed) => {
                    without_data = without_data.or(Some(opened));
                }
                (Some(opened), _) => return Ok(Some(opened)),
                (None, _) => {}
            }
        }
        Ok(without_data)
    }

    /// `None` unless the record names the reader as a ring output's owner and the commitment rebuilds
    /// from the record's proven hashes under the reader's own key, or the zero key the record's kind names.
    pub fn open_body(
        &self,
        body: &OpenedRecipientBody,
    ) -> Result<Option<OpenedBody>, RingRailError> {
        let record = &body.record;
        if record.kind != RecordKind::Ring
            || record.owner_key != self.owner.signing_pubkey.confidential_view_tag()?
        {
            return Ok(None);
        }
        // No registry lookup, an owner not yet registered reads its note at once.
        let nullifier_pk = published_nullifier_pk(record).unwrap_or(self.owner.nullifier_pubkey);
        let mint = Address::new_from_array(record.mint);
        let asset = self.assets.by_mint(&mint);
        let published = |hash: [u8; 32]| Some(hash).filter(|hash| *hash != [0; 32]);
        let data_hash = published(record.data_hash);
        let ring_data_hash = published(record.ring_data_hash);
        // Data under a zero hash binds to no commitment, a lost payload leaves the note without it.
        let data = match (&body.payload, data_hash) {
            (PayloadRead::Opened(payload), Some(_)) => {
                Data::new(vec![DataRecord::UtxoData(payload.data.clone())])
            }
            _ => Data::default(),
        };
        let note = |asset: Mint| Utxo {
            owner: self.owner.signing_pubkey,
            asset,
            amount: record.amount,
            blinding: record.blinding,
            ring_program_id: Some(self.ring),
            data: data.clone(),
        };
        // The commitment reads the mint alone, the id only names a registered one.
        let probe = note(asset.unwrap_or(Mint {
            asset: mint,
            asset_id: 0,
        }));
        let rebuilds = probe
            .hash(
                &nullifier_pk,
                &record.data_hash,
                &record.ring_data_hash,
                self.tree_id,
            )
            .is_ok_and(|hash| hash == *self.utxo_hash);
        if !rebuilds {
            return Ok(None);
        }
        let opened = |utxo| OpenedNote {
            utxo,
            nullifier_pk,
            data_hash,
            ring_data_hash,
        };
        Ok(Some(match asset {
            Some(asset) => OpenedBody::Note(opened(note(asset))),
            None => OpenedBody::UnknownMint(opened(probe.clone())),
        }))
    }
}

impl OutputBlindings {
    /// `blinding_seed` is the rail's `CRBS(tx_viewing_sk, salt)`.
    pub fn new(
        first_nullifier: &[u8; 32],
        blinding_seed: &[u8; 32],
    ) -> Result<Self, RingRailError> {
        Ok(Self {
            first_nullifier: *first_nullifier,
            output_seed: Zeroizing::new(derive_output_blinding_seed(
                first_nullifier,
                blinding_seed,
            )?),
        })
    }

    pub fn published(first_nullifier: &[u8; 32], output_seed: &[u8; 32]) -> Self {
        Self {
            first_nullifier: *first_nullifier,
            output_seed: Zeroizing::new(*output_seed),
        }
    }

    pub fn at(&self, position: u32) -> Result<Blinding, RingRailError> {
        Ok(derive_transact_output_blinding(
            &self.first_nullifier,
            &self.output_seed,
            position,
        )?)
    }

    /// The seed the sender bundle publishes, every change blinding derives from it.
    pub fn output_seed(&self) -> &[u8; 32] {
        &self.output_seed
    }
}

/// False for a low-order transaction key, every clamped secret refuses the same keys.
pub fn is_contributory(tx_viewing_pk: &X25519Pubkey) -> bool {
    X25519Keypair::from_bytes(&[0; 32])
        .ecdh(tx_viewing_pk)
        .is_ok()
}
