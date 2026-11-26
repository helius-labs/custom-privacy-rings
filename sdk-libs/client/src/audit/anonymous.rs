//! The every-site read of a ring's history, each body tried under every held key with no use of its tag.

#[path = "anonymous_recovery.rs"]
mod recovery;
pub use recovery::RecoveryStatus;
pub(crate) use recovery::{RecoverySession, RecoveryUpdate};
use zolana_ring_keys::RecipientBodyForms;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use custom_ring_interface::{AuditStreamLayout, RecordForm, RecordKind, RingMode};
use solana_address::Address;
use solana_signature::Signature;
use spr_client::{AsyncRpc, IndexerRpcConfig};
use spr_keypair::{PublicKey, ShieldedAddress, ViewingKey, X25519Pubkey};
use spr_transaction::{
    instructions::merge::MergeOutputPlaintext, DecryptionResult, DeriveRequest, Mint, ShieldedKeys,
    ShieldedTransaction, TransactionError, TransactionKeyRequest, WalletUtxo,
};
use zolana_ring_keys::ring_body_output;
use zolana_ring_keys::SlotSecret;
use zolana_ring_keys::{RingDeliveryRequest, RingKeys};
use zolana_ring_transaction::{
    is_contributory, open_recipient_body, open_reply, AnonymousOutput, OpenedBody, OpenedNote,
    ReplyClaim, RingAssets,
};

use crate::history::HistoryPosition;
use crate::{
    anonymous_wallet::{AnonymousWallet, PendingNote, WalletRevision},
    confidential::is_ring_merge,
    decrypt::SealedRecipientKeys,
    delivered::{self, RingDeliveryContext},
    error::RowDisposition,
    exactly_one_each,
    merge_audit::RingMergeBody,
    origin::TransactionOrigin,
    tags::IndexerReads,
    viewing_history::{ViewingHistory, ViewingKeyEntry},
    AnonymousError, AuditError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnonymousStream {
    Merge {
        first_nullifier: [u8; 32],
    },
    /// Found by trying the slot with no use of its tag.
    FullScan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnonymousRingUtxo {
    pub utxo_hash: [u8; 32],
    pub viewing_pubkey: X25519Pubkey,
    pub stream: AnonymousStream,
}

/// One payment of the wallet's own transfer, its body opened with the transaction key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SentPayment {
    pub tx_signature: Signature,
    pub utxo_hash: [u8; 32],
    pub position: u32,
    pub recipient_viewing_pubkey: X25519Pubkey,
    /// The held address that rebuilt the hash, `None` under `SentCheck::Unverified`.
    pub owner_pubkey: Option<PublicKey>,
    pub amount: u64,
    pub check: SentCheck,
}

/// A decrypted reply, an unauthenticated claim about who paid, never a verified sender.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SenderClaim {
    pub utxo_hash: [u8; 32],
    pub viewing_pubkey: X25519Pubkey,
    pub claim: ReplyClaim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SentCheck {
    /// A held recipient address rebuilt the published hash under `mint`.
    Verified { mint: Mint },
    /// No held address names the owner and viewing key, or the body rebuilds no hash.
    Unverified,
}

/// One own transaction's audit stream layout and its ring's auditor key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SenderAuditFrame {
    pub layout: AuditStreamLayout,
    pub auditor_pk: X25519Pubkey,
}

/// A sender reads its payments' keys from the auditor stream its transaction key sealed.
#[async_trait]
pub trait SenderAuditFrames: Sync {
    /// `None` for a transaction of a ring the source does not serve.
    async fn frame(
        &self,
        transaction: &ShieldedTransaction,
    ) -> Result<Option<SenderAuditFrame>, AuditError>;
}

/// One ring's frames, each layout parsed from the instruction the origin confirms.
#[must_use]
pub struct RingSenderAudit<'a, O> {
    pub origin: &'a O,
    pub ring: Address,
    pub mode: RingMode,
    pub auditor_pk: X25519Pubkey,
}

#[async_trait]
impl<O: TransactionOrigin> SenderAuditFrames for RingSenderAudit<'_, O> {
    async fn frame(
        &self,
        transaction: &ShieldedTransaction,
    ) -> Result<Option<SenderAuditFrame>, AuditError> {
        if transaction.ring_program_id != Some(self.ring) {
            return Ok(None);
        }
        let event_index = transaction
            .event_index
            .ok_or(AuditError::MissingEventIndex)?;
        let origin = self
            .origin
            .origin(transaction.tx_signature, event_index, self.ring)
            .await?;
        Ok(Some(SenderAuditFrame {
            layout: origin.audit_frame(self.mode)?,
            auditor_pk: self.auditor_pk,
        }))
    }
}

/// Bodies no held key rebuilt, an owner's note under a bad body opens only through an auditor read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AuditorRecovery {
    pub unopened_count: u64,
    /// Commitments, foreign notes and dummies among them.
    pub unopened: Vec<[u8; 32]>,
}

/// Only [`crate::AnonymousWallet::absorb`] stores what it covers.
pub struct AnonymousScan {
    expected_revision: Option<WalletRevision>,
    recovery: RecoveryUpdate,
    retried: BTreeSet<[u8; 32]>,
    target_slot: Option<u64>,
    starting_history: ViewingHistory,
    history: ViewingHistory,
    decrypted: DecryptionResult,
    pending: Vec<PendingNote>,
    sender_claims: Vec<SenderClaim>,
    pub utxos: Vec<AnonymousRingUtxo>,
    /// Bodies that rebuilt under no mint of `assets`, each among the pending notes.
    pub unmatched: Vec<[u8; 32]>,
    /// The wallet's payments in its own transfers, a body that fails to open left out.
    pub sent: Vec<SentPayment>,
    /// Filled by a full scan alone.
    pub auditor_recovery: AuditorRecovery,
}

impl AnonymousScan {
    pub fn status(&self) -> RecoveryStatus {
        match &self.recovery {
            RecoveryUpdate::Paused(_) => RecoveryStatus::Paused,
            _ => RecoveryStatus::Complete,
        }
    }

    pub fn target_slot(&self) -> Option<u64> {
        self.target_slot
    }

    pub(crate) fn expected_revision(&self) -> Option<WalletRevision> {
        self.expected_revision
    }

    pub(crate) fn take_retried(&mut self) -> BTreeSet<[u8; 32]> {
        std::mem::take(&mut self.retried)
    }

    pub(crate) fn take_recovery(&mut self) -> RecoveryUpdate {
        std::mem::take(&mut self.recovery)
    }

    pub fn history(&self) -> &ViewingHistory {
        &self.history
    }

    pub fn decrypted(&self) -> &DecryptionResult {
        &self.decrypted
    }

    /// Notes verified under a mint `assets` does not name, none spendable.
    pub fn pending_notes(&self) -> &[PendingNote] {
        &self.pending
    }

    /// Replies on the wallet's notes, unauthenticated.
    pub fn sender_claims(&self) -> &[SenderClaim] {
        &self.sender_claims
    }

    pub(crate) fn starts_from(&self, history: &ViewingHistory) -> bool {
        self.starting_history == *history
    }

    pub(crate) fn into_stored(
        self,
    ) -> (
        ViewingHistory,
        DecryptionResult,
        Vec<PendingNote>,
        Vec<SenderClaim>,
    ) {
        (
            self.history,
            self.decrypted,
            self.pending,
            self.sender_claims,
        )
    }
}

/// Reads a ring's whole history and opens each body under the held viewing keys, no tag selects a body.
#[must_use]
pub struct AnonymousSync<'a, K: ?Sized> {
    keys: &'a K,
    assets: &'a RingAssets,
    config: Option<IndexerRpcConfig>,
    from: ScanFrom,
    full_ring: Option<Address>,
    page_budget: usize,
    recipients: &'a [ShieldedAddress],
    sender_audit: Option<&'a dyn SenderAuditFrames>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanFrom {
    Watermark,
    Start,
}

impl<'a, K: ShieldedKeys + RingKeys + ?Sized> AnonymousSync<'a, K> {
    pub fn new(keys: &'a K, assets: &'a RingAssets) -> Self {
        Self {
            keys,
            assets,
            config: None,
            from: ScanFrom::Watermark,
            full_ring: None,
            page_budget: 32,
            recipients: &[],
            sender_audit: None,
        }
    }

    pub fn with_indexer_config(mut self, config: IndexerRpcConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Every ring from its start, for a wallet that keeps no notes between syncs.
    pub fn with_full_rescan(mut self) -> Self {
        self.from = ScanFrom::Start;
        self
    }

    /// Without it a scan records no sent payment, change still opens.
    pub fn with_sender_audit(mut self, frames: &'a dyn SenderAuditFrames) -> Self {
        self.sender_audit = Some(frames);
        self
    }

    /// The recipients' addresses the wallet holds, a sent payment verifies only against one.
    pub fn with_recipient_addresses(mut self, recipients: &'a [ShieldedAddress]) -> Self {
        self.recipients = recipients;
        self
    }

    /// An entry whose key the holder lacks stays as stored, a failed page moves nothing.
    pub async fn fetch<I: AsyncRpc + ?Sized>(
        &self,
        indexer: &I,
        history: &ViewingHistory,
    ) -> Result<AnonymousScan, AnonymousError> {
        let wallet = AnonymousWallet::new(history.clone());
        let mut scan = self.recover_wallet(indexer, &wallet).await?;
        scan.expected_revision = None;
        Ok(scan)
    }

    /// Reads from the stored history of `wallet` with its notes held, their nullifiers find the merges and spends
    /// of stored notes.
    pub async fn fetch_wallet<I: AsyncRpc + ?Sized>(
        self,
        indexer: &I,
        wallet: &AnonymousWallet,
    ) -> Result<AnonymousScan, AnonymousError> {
        self.recover_wallet(indexer, wallet).await
    }

    pub async fn full_scan<I: AsyncRpc + ?Sized>(
        &self,
        indexer: &I,
        history: &ViewingHistory,
        ring: Address,
    ) -> Result<AnonymousScan, AnonymousError> {
        let wallet = AnonymousWallet::new(history.clone());
        let sync = Self {
            full_ring: Some(ring),
            ..*self
        };
        let mut scan = sync.recover_wallet(indexer, &wallet).await?;
        scan.expected_revision = None;
        Ok(scan)
    }

    fn scan<'s, 'r, I: AsyncRpc + ?Sized>(
        &'s self,
        indexer: &'r I,
        history: &ViewingHistory,
    ) -> Result<Scan<'s, 'a, 'r, I, K>, AnonymousError> {
        // A held key with no entry would read nothing and report nothing missed.
        if self
            .keys
            .viewing_public_keys()
            .iter()
            .any(|key| history.entry(key).is_none())
        {
            return Err(AnonymousError::HeldKeyNotInHistory);
        }
        Ok(Scan {
            sync: self,
            address: self.keys.address()?,
            reads: IndexerReads::configured(indexer, self.config),
            starting_history: history.clone(),
            history: history.clone(),
            found: Vec::new(),
            held_back: Vec::new(),
            found_hashes: HashSet::new(),
            owned_nullifiers: HashSet::new(),
            pending_assigned: Vec::new(),
            resolved_assets: HashMap::new(),
            minimum_slot: None,
            utxos: Vec::new(),
            decrypted: DecryptionResult::default(),
            unmatched: Vec::new(),
            sent: Vec::new(),
            sender_claims: Vec::new(),
        })
    }
}

/// One history query, every entry listed shares its position and pays one read.
struct RingRead {
    ring: Address,
    position: HistoryPosition,
    entries: Vec<usize>,
}

struct Scan<'s, 'a, 'r, I: ?Sized, K: ?Sized> {
    sync: &'s AnonymousSync<'a, K>,
    address: ShieldedAddress,
    reads: IndexerReads<'r, I>,
    starting_history: ViewingHistory,
    history: ViewingHistory,
    /// Notes whose rebuilt hash matched, before their nullifiers.
    found: Vec<WalletUtxo>,
    /// The same under a mint `assets` does not name.
    held_back: Vec<WalletUtxo>,
    /// Kept after `found` moves into `decrypted`.
    found_hashes: HashSet<[u8; 32]>,
    owned_nullifiers: HashSet<[u8; 32]>,
    pending_assigned: Vec<WalletUtxo>,
    resolved_assets: HashMap<u64, Mint>,
    minimum_slot: Option<u64>,
    utxos: Vec<AnonymousRingUtxo>,
    decrypted: DecryptionResult,
    unmatched: Vec<[u8; 32]>,
    sent: Vec<SentPayment>,
    sender_claims: Vec<SenderClaim>,
}

struct Body<'t> {
    tx: &'t ShieldedTransaction,
    position: usize,
    context: RingDeliveryContext,
    found: AnonymousStream,
}

impl<'t> Body<'t> {
    fn of(tx: &'t ShieldedTransaction, position: usize, found: AnonymousStream) -> Option<Self> {
        ring_body_output(&tx.output_slots.get(position)?.payload)?;
        Some(Self {
            tx,
            position,
            context: RingDeliveryContext::of(tx)?,
            found,
        })
    }

    fn every(tx: &'t ShieldedTransaction) -> Vec<Self> {
        (0..tx.output_slots.len())
            .filter_map(|position| Self::of(tx, position, AnonymousStream::FullScan))
            .collect()
    }
}

impl<I: AsyncRpc + ?Sized, K: ShieldedKeys + RingKeys + ?Sized> Scan<'_, '_, '_, I, K> {
    fn held_entries(&self) -> Vec<usize> {
        let held = self.sync.keys.viewing_public_keys();
        self.history
            .entries()
            .iter()
            .enumerate()
            .filter(|(_, entry)| held.contains(&entry.viewing_pubkey()))
            .map(|(index, _)| index)
            .collect()
    }

    /// Entries at one position of one ring share a read, a key added later is its own query.
    fn ring_reads(&self, held: &[usize]) -> Vec<RingRead> {
        let rescan = self.sync.from == ScanFrom::Start;
        let mut reads: BTreeMap<(Address, Option<u64>), Vec<usize>> = BTreeMap::new();
        for index in held {
            let entry = &self.history.entries()[*index];
            for (ring, after_slot) in entry.ring_positions() {
                let after_slot = after_slot.filter(|_| !rescan);
                reads.entry((*ring, after_slot)).or_default().push(*index);
            }
        }
        reads
            .into_iter()
            .map(|((ring, after_slot), entries)| RingRead {
                ring,
                position: after_slot
                    .map_or_else(HistoryPosition::default, HistoryPosition::after_slot),
                entries,
            })
            .collect()
    }

    /// Each body's slot secret, one agreement per transaction inside the holder.
    fn body_secrets<'t>(
        &self,
        viewing_pubkey: X25519Pubkey,
        bodies: Vec<Body<'t>>,
    ) -> Result<Vec<(Body<'t>, SlotSecret)>, AnonymousError> {
        if bodies.is_empty() {
            return Ok(Vec::new());
        }
        let requests = bodies
            .iter()
            .map(|body| {
                Ok(RingDeliveryRequest {
                    viewing_pubkey,
                    tx_viewing_pk: body.context.tx_viewing_pk,
                    salt: body.context.salt,
                    output_index: u32::try_from(body.position)
                        .map_err(|_| TransactionError::TooManyOutputs)?,
                    first_nullifier: body.context.first_nullifier,
                })
            })
            .collect::<Result<Vec<RingDeliveryRequest>, AnonymousError>>()?;
        let secrets = exactly_one_each(
            self.sync.keys.ring_delivery_secrets(&requests)?,
            requests.len(),
        )?;
        bodies
            .into_iter()
            .zip(secrets)
            .map(|(body, secret)| Ok((body, SlotSecret::new(secret)?)))
            .collect()
    }

    /// `None` when another key sent the transaction.
    fn own_transaction_key(
        &self,
        viewing_pubkey: X25519Pubkey,
        context: &RingDeliveryContext,
    ) -> Result<Option<ViewingKey>, AnonymousError> {
        let derived =
            crate::exactly_one(self.sync.keys.transaction_keys(&[TransactionKeyRequest {
                viewing_pubkey,
                first_nullifier: context.first_nullifier,
            }])?)?;
        Ok(Some(derived).filter(|key| key.pubkey() == context.tx_viewing_pk))
    }

    /// Only a rebuilt commitment proves the note the wallet's, every other body stays silent.
    fn open_recipient(
        &mut self,
        index: usize,
        body: &Body<'_>,
        position: usize,
        secret: &SlotSecret,
    ) -> Result<(), AnonymousError> {
        let slot = &body.tx.output_slots[position];
        let Some((ciphertext, reply)) = ring_body_output(&slot.payload) else {
            return Ok(());
        };
        let opened = self
            .output_at(body, &self.address, position)
            .open(ciphertext, secret);
        match opened {
            Ok(Some(OpenedBody::Note(note))) => {
                self.accept(index, body.tx, body.found, position, note)?;
            }
            Ok(Some(OpenedBody::UnknownMint(note))) => {
                self.hold_back(body.tx, position, note)?;
            }
            Ok(None) | Err(_) => return Ok(()),
        }
        if let Some(claim) = open_reply(reply, secret).ok().flatten() {
            let viewing_pubkey = self.history.entries()[index].viewing_pubkey();
            self.sender_claims.push(SenderClaim {
                utxo_hash: slot.output_context.hash,
                viewing_pubkey,
                claim,
            });
        }
        Ok(())
    }

    /// Only the wallet's own transaction key reopens the auditor stream it sealed.
    async fn list_sent(
        &mut self,
        index: usize,
        tx: &ShieldedTransaction,
    ) -> Result<(), AnonymousError> {
        let (Some(frames), Some(context)) = (self.sync.sender_audit, RingDeliveryContext::of(tx))
        else {
            return Ok(());
        };
        let viewing_pubkey = self.history.entries()[index].viewing_pubkey();
        let Some(tx_viewing_key) = self.own_transaction_key(viewing_pubkey, &context)? else {
            return Ok(());
        };
        let keys = frames.frame(tx).await.and_then(|frame| {
            frame
                .map(|frame| {
                    SealedRecipientKeys {
                        transaction: tx,
                        layout: frame.layout,
                        auditor_pk: &frame.auditor_pk,
                    }
                    .open(&tx_viewing_key)
                })
                .transpose()
        });
        let keys = match keys {
            Ok(Some(keys)) => keys,
            Err(error) if error.disposition() == RowDisposition::Retry => {
                return Err(AnonymousError::SenderAudit(Box::new(error)))
            }
            // A frame the stream does not reopen lists no payment.
            Ok(None) | Err(_) => return Ok(()),
        };
        // A slot sealed to a held key is the wallet's own change.
        let held = self.sync.keys.viewing_public_keys();
        for (position, key) in keys.into_iter().map(|key| key.u()).enumerate() {
            let Ok(recipient) = X25519Pubkey::from_u(key) else {
                continue;
            };
            if key == [0; 32] || !is_contributory(&recipient) || held.contains(&recipient) {
                continue;
            }
            // An exit carries no ring body.
            let Some(body) = Body::of(tx, position, AnonymousStream::FullScan) else {
                continue;
            };
            if let Some(sent) = self.sent_payment(&body, position, &recipient, &tx_viewing_key) {
                self.sent.push(sent);
            }
        }
        Ok(())
    }

    fn sent_payment(
        &self,
        body: &Body<'_>,
        position: usize,
        recipient: &X25519Pubkey,
        tx_viewing_key: &ViewingKey,
    ) -> Option<SentPayment> {
        let slot = &body.tx.output_slots[position];
        let (ciphertext, _) = ring_body_output(&slot.payload)?;
        let slot_index = u32::try_from(position).ok()?;
        let agreement = zeroize::Zeroizing::new(tx_viewing_key.ecdh(recipient).ok()?);
        let secret = zolana_ring_keys::DeliveredSlot {
            agreement: &agreement,
            tx_viewing_pk: body.context.tx_viewing_pk.as_bytes(),
            recipient_viewing_pk: recipient.as_bytes(),
            salt: &body.context.salt,
            output_index: slot_index,
            first_nullifier: &body.context.first_nullifier,
        }
        .secret()
        .ok()?;
        // The amount sits in the record's head, the same bytes under every form.
        let plain = RecordForm::of_recipient_body(ciphertext.len())
            .find_map(|form| open_recipient_body(ciphertext, &secret, form).ok())?;
        // A dummy's key is in the stream too, its proven record names no note.
        if plain.record.kind != RecordKind::Ring {
            return None;
        }
        let verified = self
            .sync
            .recipients
            .iter()
            // A held address verifies only under its own key, `open_body` rebuilds with it.
            .filter(|address| address.viewing_pubkey == *recipient)
            .find_map(|address| {
                match self
                    .output_at(body, address, position)
                    .open(ciphertext, &secret)
                    .ok()
                    .flatten()?
                {
                    OpenedBody::Note(note) => Some((address.signing_pubkey, note.utxo.asset)),
                    OpenedBody::UnknownMint(_) => None,
                }
            });
        let (owner_pubkey, check) = match verified {
            Some((owner, mint)) => (Some(owner), SentCheck::Verified { mint }),
            None => (None, SentCheck::Unverified),
        };
        Some(SentPayment {
            tx_signature: body.tx.tx_signature,
            utxo_hash: slot.output_context.hash,
            position: slot_index,
            recipient_viewing_pubkey: *recipient,
            owner_pubkey,
            amount: plain.record.amount,
            check,
        })
    }

    fn output_at<'b>(
        &'b self,
        body: &'b Body<'_>,
        owner: &'b ShieldedAddress,
        position: usize,
    ) -> AnonymousOutput<'b> {
        let context = &body.tx.output_slots[position].output_context;
        AnonymousOutput {
            owner,
            assets: self.sync.assets,
            ring: body.context.ring,
            tree_id: context.tree_id,
            utxo_hash: &context.hash,
        }
    }

    /// Keeps a rebuilt note once, under the stream that found it.
    fn accept(
        &mut self,
        index: usize,
        tx: &ShieldedTransaction,
        stream: AnonymousStream,
        position: usize,
        note: OpenedNote,
    ) -> Result<(), AnonymousError> {
        let viewing_pubkey = self.history.entries()[index].viewing_pubkey();
        let utxo = published_note(tx, position, note)?;
        self.keep(utxo, viewing_pubkey, stream);
        Ok(())
    }

    /// Verified under a mint `assets` does not name, it waits among the pending notes.
    fn hold_back(
        &mut self,
        tx: &ShieldedTransaction,
        position: usize,
        note: OpenedNote,
    ) -> Result<(), AnonymousError> {
        let utxo = published_note(tx, position, note)?;
        self.hold_back_note(utxo);
        Ok(())
    }

    fn hold_back_note(&mut self, utxo: WalletUtxo) {
        if self.found_hashes.insert(utxo.utxo_hash) {
            self.unmatched.push(utxo.utxo_hash);
            self.held_back.push(utxo);
        }
    }

    fn is_found(&self, utxo_hash: &[u8; 32]) -> bool {
        self.found_hashes.contains(utxo_hash)
    }

    fn keep(&mut self, utxo: WalletUtxo, viewing_pubkey: X25519Pubkey, stream: AnonymousStream) {
        if !self.found_hashes.insert(utxo.utxo_hash) {
            return;
        }
        self.utxos.push(AnonymousRingUtxo {
            utxo_hash: utxo.utxo_hash,
            viewing_pubkey,
            stream,
        });
        self.found.push(utxo);
    }

    /// Keeps each nonzero key merge output the wallet's keys rebuild, except a note the caller passed.
    fn open_ring_merges(&mut self, rows: Vec<ShieldedTransaction>) -> Result<(), AnonymousError> {
        let viewing_pubkey = self.address.viewing_pubkey;
        for tx in &rows {
            let Some(merge) = OwnerMerge::read(tx) else {
                continue;
            };
            if self.is_found(merge.commitment()) {
                continue;
            }
            let first_nullifier = merge.first_nullifier;
            let derived = self.sync.keys.derive(&[
                DeriveRequest::MergeOutputKey { first_nullifier },
                DeriveRequest::MergeOutputBlinding { first_nullifier },
            ])?;
            let [key, blinding] = <[[u8; 32]; 2]>::try_from(derived).map_err(|derived| {
                TransactionError::IncompleteDerivation {
                    got: derived.len(),
                    want: 2,
                }
            })?;
            let Some(mut note) = merge.rebuild(MergeKeys {
                owner: self.address.signing_pubkey,
                nullifier_pk: self.address.nullifier_pubkey,
                key,
                blinding,
            })?
            else {
                continue;
            };
            match self.sync.assets.by_mint(&note.utxo.asset.asset) {
                Some(asset) => {
                    note.utxo.asset = asset;
                    let utxo = published_note(tx, 0, note)?;
                    self.keep(
                        utxo,
                        viewing_pubkey,
                        AnonymousStream::Merge { first_nullifier },
                    );
                }
                None => self.hold_back(tx, 0, note)?,
            }
        }
        Ok(())
    }

    /// A held back note carries its nullifier, an absorb stores it with the position past its row.
    fn finish<S: ShieldedKeys + ?Sized>(self, keys: &S) -> Result<AnonymousScan, AnonymousError> {
        let Scan {
            starting_history,
            history,
            found,
            held_back,
            utxos,
            mut decrypted,
            unmatched,
            sent,
            sender_claims,
            owned_nullifiers,
            pending_assigned,
            ..
        } = self;
        assign_nullifiers(keys, &mut decrypted, found)?;
        let mut held = DecryptionResult::default();
        assign_nullifiers(keys, &mut held, held_back)?;
        held.utxos.extend(pending_assigned);
        delivered::sort_by_publication(&mut decrypted.utxos);
        delivered::sort_by_publication(&mut held.utxos);
        // A read row spends other owners' notes too, only the wallet's own spends reach the stored wallet.
        let own: HashSet<[u8; 32]> = decrypted
            .utxos
            .iter()
            .chain(&held.utxos)
            .map(|note| note.nullifier)
            .chain(owned_nullifiers)
            .collect();
        decrypted
            .spent_nullifiers
            .retain(|nullifier| own.contains(nullifier));
        Ok(AnonymousScan {
            expected_revision: None,
            recovery: RecoveryUpdate::Keep,
            retried: BTreeSet::new(),
            target_slot: None,
            starting_history,
            history,
            decrypted,
            pending: held.utxos.into_iter().map(PendingNote::new).collect(),
            sender_claims,
            utxos,
            unmatched,
            sent,
            auditor_recovery: AuditorRecovery::default(),
        })
    }

    fn entry_mut(&mut self, index: usize) -> &mut ViewingKeyEntry {
        &mut self.history.entries_mut()[index]
    }
}

/// Nothing of it is authentic until its commitment rebuilds.
pub(crate) struct OwnerMerge<'t> {
    pub(crate) tx: &'t ShieldedTransaction,
    ring: Address,
    first_nullifier: [u8; 32],
    body: RingMergeBody,
}

struct MergeKeys {
    owner: PublicKey,
    nullifier_pk: [u8; 32],
    key: [u8; 32],
    blinding: [u8; 32],
}

impl<'t> OwnerMerge<'t> {
    /// The single output of a ring merge under any view tag.
    fn read(tx: &'t ShieldedTransaction) -> Option<Self> {
        let [slot] = tx.output_slots.as_slice() else {
            return None;
        };
        if !is_ring_merge(tx) {
            return None;
        }
        Some(Self {
            tx,
            ring: tx.ring_program_id?,
            first_nullifier: *tx.nullifiers.first()?,
            body: RingMergeBody::of(slot)?,
        })
    }

    fn commitment(&self) -> &'t [u8; 32] {
        &self.tx.output_slots[0].output_context.hash
    }

    /// `None` under another wallet's secrets, the cipher is unauthenticated and only the commitment decides.
    fn rebuild(&self, keys: MergeKeys) -> Result<Option<OpenedNote>, AnonymousError> {
        let Ok(plaintext) = MergeOutputPlaintext::decrypt(&self.body.encrypted, &keys.key) else {
            return Ok(None);
        };
        let utxo = plaintext.into_utxo(
            keys.owner,
            Mint::new(plaintext.mint, 0),
            keys.blinding,
            Some(self.ring),
        );
        let hash = utxo.hash(
            &keys.nullifier_pk,
            &[0; 32],
            &self.body.ring_data_hash,
            self.tx.output_slots[0].output_context.tree_id,
        )?;
        Ok((hash == *self.commitment()).then_some(OpenedNote {
            utxo,
            nullifier_pk: keys.nullifier_pk,
            data_hash: None,
            ring_data_hash: Some(self.body.ring_data_hash),
        }))
    }
}

pub(crate) fn published_note(
    tx: &ShieldedTransaction,
    position: usize,
    note: OpenedNote,
) -> Result<WalletUtxo, AnonymousError> {
    let context = &tx.output_slots[position].output_context;
    Ok(WalletUtxo {
        utxo: note.utxo,
        nullifier_pubkey: note.nullifier_pk,
        utxo_hash: context.hash,
        nullifier: [0; 32],
        tx_viewing_key: None,
        data_hash: note.data_hash,
        ring_data_hash: note.ring_data_hash,
        tree_id: context.tree_id,
        leaf_index: context.leaf_index,
        latest_tree_id: None,
        slot: tx.slot,
        tx_signature: tx.tx_signature,
        slot_index: u32::try_from(position).map_err(|_| TransactionError::TooManyOutputs)?,
    })
}

/// Derives the nullifiers of `found` and adds them to `decrypted` in publication order.
fn assign_nullifiers<K: ShieldedKeys + ?Sized>(
    keys: &K,
    decrypted: &mut DecryptionResult,
    mut found: Vec<WalletUtxo>,
) -> Result<(), AnonymousError> {
    if found.is_empty() {
        return Ok(());
    }
    delivered::assign_nullifiers(keys, &mut found)?;
    decrypted.utxos.extend(found);
    Ok(())
}
