//! `AnonymousSync` and the tag sync against an in-memory indexer, every transfer sealed by the anonymous rail.

#[path = "common/history_page.rs"]
pub mod history_page;
#[path = "common/page_window.rs"]
mod page_window;

use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
};

use async_trait::async_trait;
use custom_ring_interface::{
    AuditKey, AuditStreamLayout, AuditorMessage, RecordForm, RecordKind, MIN_REPLY_LEN,
    RING_RECORD_LEN,
};
use solana_address::Address;
use solana_signature::Signature;
use spr_client::{
    rpc::GetShieldedTransactionsByNullifiersResponse, AsyncRpc, ClientError, Context,
    GetShieldedTransactionsByTagsResponse, HistoryFilters, IndexerRpcConfig, OutputContext,
    OutputSlot, PaginationToken, RingHistoryOptions, ShieldedTransaction,
};
use spr_event::{encode_merge_output_data, MergeOutputDataRef, MessageData};
use spr_interface::ZERO_NULLIFIER_PUBKEY;
use spr_keypair::{
    random_salt, viewing_key::ViewTag, ShieldedAddress, ShieldedKeypair, SigningKey, ViewingKey,
    X25519Pubkey,
};
use spr_transaction::{
    instructions::{
        merge::{merge_output_blinding, merge_output_key, MergeOutputPlaintext},
        transact::ConfidentialTransaction,
    },
    verify_spendable, Data, DecryptRequest, DecryptionResult, DeriveRequest, Mint, ShieldedKeys,
    TransactionError, TransactionKeyRequest, Utxo, WalletUtxo,
};
use zolana_ring_client::{
    auditor_view_tag, note_state, verify_ring_owned, AnonymousError, AnonymousScan,
    AnonymousStream, AnonymousSync, AnonymousWallet, AuditError, AuditStreamSeal, NoteState,
    PendingClaim, RingConfidentialScan, RingNoteCodec, SenderAuditFrame, SenderAuditFrames,
    SentCheck, ViewingHistory, TAG_WINDOW,
};
use zolana_ring_keys::encode_ring_body_output;
use zolana_ring_keys::AuditEvidence;
use zolana_ring_keys::DeliveredSlot;
use zolana_ring_keys::OutputRecord;
use zolana_ring_keys::RecipientPayload;
use zolana_ring_keys::SlotSecret;
use zolana_ring_keys::{
    bootstrap_slot_tag, RingDeliveryRequest, RingKeyError, RingKeys, RotatingRingKeys,
    ViewTagRequest, ViewTagStream,
};
use zolana_ring_transaction::{
    published_outputs, record_key, seal_reply, AnonymousPlan, BodyForm, ExitSlot, RecipientBody,
    ReplyMetadata, RingAssets, RingDelivery, RingOutputRail,
};

use history_page::EventsPage;
use page_window::{page_window, MockPageWindow};

const RING: Address = Address::new_from_array([4; 32]);
const TREE: u16 = 2;
const USDC: Mint = Mint {
    asset: Address::new_from_array([6; 32]),
    asset_id: 3,
};
const RESPONSE_TOO_LARGE: &str = "JSON-RPC error from ring: code=Some(-32008) message=\"large\"";

fn keypair(seed: u8) -> ShieldedKeypair {
    ShieldedKeypair::from_keypair(SigningKey::from_ed25519_bytes(&[seed; 32])).unwrap()
}

#[derive(Default)]
struct Indexer {
    transactions: Mutex<Vec<ShieldedTransaction>>,
    next_leaf: Mutex<u64>,
    tip: Mutex<u64>,
    /// Transactions per ring page below the asked limit.
    page_cap: Mutex<Option<usize>>,
    /// Ring pages asked above the limit answer `-32008`.
    response_limit: Mutex<Option<u32>>,
    limits_asked: Mutex<Vec<u32>>,
    ring_queries: Mutex<Vec<RingHistoryOptions>>,
    nullifier_queries: Mutex<Vec<Vec<[u8; 32]>>>,
    /// The ring page at the index fails.
    failing_page: Mutex<Option<usize>>,
    ring_pages: AtomicUsize,
}

impl Indexer {
    fn publish(&self, transaction: ShieldedTransaction) {
        let mut tip = self.tip.lock().unwrap();
        *tip = (*tip).max(transaction.slot);
        self.transactions.lock().unwrap().push(transaction);
    }

    /// Leaves in append order, every published output takes the next.
    fn leaves(&self, count: usize) -> u64 {
        let mut next = self.next_leaf.lock().unwrap();
        let first = *next;
        *next += count as u64;
        first
    }

    fn context(&self) -> Context {
        Context {
            block_time: 0,
            slot: *self.tip.lock().unwrap(),
        }
    }

    fn require_context(&self, config: Option<IndexerRpcConfig>) -> Result<(), ClientError> {
        let indexed = self.context().slot;
        if config
            .and_then(|config| config.require_slot)
            .is_some_and(|required| indexed < required)
        {
            return Err(ClientError::IndexerMinimumContextSlot { indexed });
        }
        Ok(())
    }

    fn take_ring_pages(&self) -> usize {
        self.ring_pages.swap(0, Ordering::Relaxed)
    }

    fn page(
        &self,
        filters: HistoryFilters,
        matches: impl Fn(&ShieldedTransaction) -> bool,
    ) -> GetShieldedTransactionsByTagsResponse {
        EventsPage {
            context: self.context(),
            output_tree_id: Some(TREE),
            rows: self
                .transactions
                .lock()
                .unwrap()
                .iter()
                .filter(|tx| filters.slot.contains(tx.slot) && matches(tx))
                .cloned()
                .collect(),
            more: false,
        }
        .response()
    }
}

#[async_trait]
impl AsyncRpc for Indexer {
    async fn get_shielded_transactions_by_tags(
        &self,
        tags: Vec<[u8; 32]>,
        _pagination_token: Option<PaginationToken>,
        _limit: Option<u32>,
        filters: HistoryFilters,
        config: Option<IndexerRpcConfig>,
    ) -> Result<GetShieldedTransactionsByTagsResponse, ClientError> {
        self.require_context(config)?;
        Ok(self.page(filters, |tx| {
            tx.output_slots
                .iter()
                .any(|slot| tags.contains(&slot.view_tag))
        }))
    }

    async fn get_shielded_transactions_by_ring(
        &self,
        options: RingHistoryOptions,
        config: Option<IndexerRpcConfig>,
    ) -> Result<GetShieldedTransactionsByTagsResponse, ClientError> {
        self.require_context(config)?;
        self.ring_queries.lock().unwrap().push(options.clone());
        let limit = options.limit.unwrap();
        self.limits_asked.lock().unwrap().push(limit);
        if self
            .response_limit
            .lock()
            .unwrap()
            .is_some_and(|cap| limit > cap)
        {
            return Err(ClientError::Indexer(RESPONSE_TOO_LARGE.to_owned()));
        }
        let page = self.ring_pages.fetch_add(1, Ordering::Relaxed);
        if *self.failing_page.lock().unwrap() == Some(page) {
            return Err(ClientError::Indexer("a failed page".to_owned()));
        }
        let rows: Vec<ShieldedTransaction> = self
            .transactions
            .lock()
            .unwrap()
            .iter()
            .filter(|tx| {
                tx.ring_program_id == Some(options.ring_program_id)
                    && options.filters.slot.contains(tx.slot)
            })
            .cloned()
            .collect();
        let cap = self
            .page_cap
            .lock()
            .unwrap()
            .map_or(limit as usize, |cap| cap.min(limit as usize));
        Ok(page_window(
            EventsPage {
                context: self.context(),
                output_tree_id: Some(TREE),
                rows,
                more: false,
            }
            .response(),
            MockPageWindow {
                token: options.pagination_token,
                limit: cap,
            },
        ))
    }

    async fn get_shielded_transactions_by_nullifiers(
        &self,
        nullifiers: Vec<[u8; 32]>,
        pagination_token: Option<PaginationToken>,
        limit: Option<u32>,
        filters: HistoryFilters,
        config: Option<IndexerRpcConfig>,
    ) -> Result<GetShieldedTransactionsByNullifiersResponse, ClientError> {
        self.require_context(config)?;
        self.nullifier_queries
            .lock()
            .unwrap()
            .push(nullifiers.clone());
        Ok(page_window(
            self.page(filters, |tx| {
                tx.nullifiers
                    .iter()
                    .any(|nullifier| nullifiers.contains(nullifier))
            }),
            MockPageWindow {
                token: pagination_token,
                limit: limit.unwrap_or(spr_indexer_api::PAGE_LIMIT as u32) as usize,
            },
        ))
    }
}

struct RecoveryHistory<'a>(&'a Indexer);

#[async_trait]
impl AsyncRpc for RecoveryHistory<'_> {
    async fn get_shielded_transactions_by_ring(
        &self,
        options: RingHistoryOptions,
        config: Option<IndexerRpcConfig>,
    ) -> Result<GetShieldedTransactionsByTagsResponse, ClientError> {
        self.0
            .get_shielded_transactions_by_ring(options, config)
            .await
    }
    async fn get_shielded_transactions_by_nullifiers(
        &self,
        nullifiers: Vec<[u8; 32]>,
        pagination_token: Option<PaginationToken>,
        limit: Option<u32>,
        filters: HistoryFilters,
        config: Option<IndexerRpcConfig>,
    ) -> Result<GetShieldedTransactionsByNullifiersResponse, ClientError> {
        self.0
            .get_shielded_transactions_by_nullifiers(
                nullifiers,
                pagination_token,
                limit,
                filters,
                config,
            )
            .await
    }
}

struct Wallet {
    keys: ShieldedKeypair,
    /// Notes it spends, never published, the sync of another wallet never reads them.
    funding: Vec<WalletUtxo>,
    replies: bool,
    /// On the change of each frame it seals, random bytes without one.
    sender_tag: Option<ViewTag>,
}

struct Pay<'a> {
    to: &'a ShieldedAddress,
    amount: u64,
    /// The tag of the request it pays, the recipient's bootstrap tag without one.
    tag: Option<ViewTag>,
}

impl Wallet {
    fn new(seed: u8) -> Self {
        let keys = keypair(seed);
        let funding = (0..8)
            .map(|index| funding_utxo(&keys, seed, index))
            .collect();
        Self {
            keys,
            funding,
            replies: true,
            sender_tag: None,
        }
    }

    fn address(&self) -> ShieldedAddress {
        self.keys.shielded_address().unwrap()
    }

    /// Seals one anonymous transfer from the next funding note and publishes it.
    fn pay(&mut self, indexer: &Indexer, payments: &[Pay<'_>]) -> ShieldedTransaction {
        let tx = self.seal(indexer, payments);
        indexer.publish(tx.clone());
        tx
    }

    /// Seals one anonymous transfer, its leaves taken, without publishing it.
    fn seal(&mut self, indexer: &Indexer, payments: &[Pay<'_>]) -> ShieldedTransaction {
        let first_nullifier = self.funding[0].nullifier;
        let tx_key = self
            .keys
            .viewing_key
            .get_transaction_viewing_key(&first_nullifier)
            .unwrap();
        self.seal_under(indexer, payments, tx_key)
    }

    /// `tx_key` of the wallet's own derivation, or a relayer's.
    fn seal_under(
        &mut self,
        indexer: &Indexer,
        payments: &[Pay<'_>],
        tx_key: ViewingKey,
    ) -> ShieldedTransaction {
        let reply = ReplyMetadata {
            viewing_key: self.keys.viewing_pubkey(),
            owner_key: self.address().solana_address().unwrap().to_bytes(),
            nullifier_pk: self.address().nullifier_pubkey,
            data: Data::default(),
        };
        // A payment to the wallet itself is change, the plan lists other recipients alone.
        let own = self.address();
        let plan = match self.sender_tag {
            Some(tag) => AnonymousPlan::new().with_sender_tag(tag),
            None => AnonymousPlan::new(),
        };
        let plan = payments
            .iter()
            .filter(|pay| *pay.to != own)
            .fold(plan, |plan, pay| {
                let plan = plan.with_payment(pay.to.viewing_pubkey);
                let plan = match pay.tag {
                    Some(tag) => plan.with_tag(tag),
                    None => plan,
                };
                if self.replies {
                    plan.with_reply(reply.clone())
                } else {
                    plan
                }
            });
        let input = self.funding.remove(0);
        let asset = input.utxo.asset;
        let mut transaction = ConfidentialTransaction::new_with_ring(
            vec![input],
            self.address().solana_address().unwrap(),
            RING,
        )
        .unwrap()
        .with_output_tree_id(TREE)
        .unwrap();
        for pay in payments {
            transaction
                .transfer_with_ring(pay.to, asset, pay.amount, Some(RING))
                .unwrap();
        }
        let rail =
            RingOutputRail::new(tx_key, audit_key(), RingDelivery::Anonymous(Box::new(plan)));
        let proof = rail.encrypt(transaction, &self.address()).unwrap();
        indexed(indexer, &rail, &proof)
    }

    async fn scan_from(&self, indexer: &Indexer, history: &ViewingHistory) -> AnonymousScan {
        scan_from(indexer, &self.keys, history).await
    }

    async fn recover(&self, indexer: &Indexer) -> AnonymousScan {
        restore(indexer, &self.keys, &fresh(self)).await
    }
}

/// The indexed row of a sealed frame, its leaves taken.
fn indexed(
    indexer: &Indexer,
    rail: &RingOutputRail,
    proof: &spr_transaction::instructions::transact::SppProofInputs,
) -> ShieldedTransaction {
    let external = &proof.external_data;
    let leaf = indexer.leaves(external.outputs.len());
    ShieldedTransaction {
        slot: leaf + 1,
        tx_signature: Signature::from(signature(leaf)),
        event_index: Some(0),
        tx_viewing_pk: Some(rail.tx_viewing_key().pubkey()),
        salt: Some(rail.salt()),
        output_slots: external
            .outputs
            .iter()
            .zip(&external.resolved_owner_tags)
            .zip(leaf..)
            .map(|((output, tag), leaf_index)| OutputSlot {
                view_tag: *tag,
                output_context: OutputContext {
                    hash: output.utxo_hash,
                    tree_id: TREE,
                    leaf_index,
                },
                payload: output.data.clone().unwrap_or_default(),
            })
            .collect(),
        messages: external
            .messages
            .iter()
            .cloned()
            .chain([auditor_message(rail, proof)])
            .collect(),
        nullifiers: proof
            .input_utxos
            .iter()
            .map(|input| input.nullifier())
            .collect(),
        proofless: false,
        ring_config: None,
        ring_program_id: Some(RING),
    }
}

/// A key of the frame's own counts, the layout [`TestFrames`] names for every row.
fn test_layout(inputs: usize, outputs: usize) -> AuditStreamLayout {
    AuditStreamLayout {
        key: AuditKey {
            class: inputs,
            entry: false,
        },
        inputs,
        outputs,
        capacity: outputs,
        vouched: false,
        records: Default::default(),
    }
}

/// The last message, the auditor stream every recipient key travels in.
fn auditor_message(
    rail: &RingOutputRail,
    proof: &spr_transaction::instructions::transact::SppProofInputs,
) -> MessageData {
    let outputs = published_outputs(&proof.output_utxos);
    let layout = test_layout(proof.input_utxos.len(), outputs.len());
    let sealed = AuditStreamSeal {
        layout,
        tx_viewing_key: rail.tx_viewing_key(),
        salt: &rail.salt(),
        real_inputs: u8::try_from(proof.input_utxos.len()).unwrap(),
        evidence: AuditEvidence::None,
        recipient_keys: rail
            .recipient_keys(outputs, outputs.len())
            .unwrap()
            .iter()
            .map(X25519Pubkey::u)
            .collect(),
        voucher: None,
    }
    .seal(&audit_key())
    .unwrap();
    let first_nullifier = proof.input_utxos[0].nullifier();
    let exits: Vec<Vec<u8>> = outputs
        .iter()
        .enumerate()
        .filter(|(_, output)| output.ring_program_id.is_none() && !output.is_dummy())
        .map(|(index, output)| {
            let form = BodyForm::of(outputs).unwrap().records;
            rail.exit_record(output, &first_nullifier, ExitSlot { index, form })
                .unwrap()
        })
        .collect();
    let ciphertexts = [&sealed.envelope[..], &sealed.keys[..]]
        .into_iter()
        .chain(exits.iter().map(Vec::as_slice));
    MessageData {
        view_tag: auditor_view_tag(&audit_key()),
        data: AuditorMessage::data(&sealed.eph_u, ciphertexts),
    }
}

/// The ring's auditor and each row's layout, a sender reads its payments' keys through them.
struct TestFrames;

#[async_trait]
impl SenderAuditFrames for TestFrames {
    async fn frame(
        &self,
        transaction: &ShieldedTransaction,
    ) -> Result<Option<SenderAuditFrame>, AuditError> {
        Ok(Some(SenderAuditFrame {
            layout: test_layout(transaction.nullifiers.len(), transaction.output_slots.len()),
            auditor_pk: audit_key(),
        }))
    }
}

/// A data note's proven hashes and its application data.
struct NoteData {
    data_hash: [u8; 32],
    ring_data_hash: [u8; 32],
    application: Vec<u8>,
}

/// A ring record naming `owner`, sealed under `secret`, a data note's with both hashes and its payload at its own width.
fn ring_body(
    owner: &ShieldedAddress,
    note: (u64, [u8; 32], [u8; 32]),
    data: Option<NoteData>,
    secret: &SlotSecret,
) -> Vec<u8> {
    let (amount, mint, blinding) = note;
    let form = RecordForm {
        data: data.is_some(),
        ring_data: data.is_some(),
    };
    let (data_hash, ring_data_hash, payload) = match data {
        Some(data) => (
            data.data_hash,
            data.ring_data_hash,
            Some(RecipientPayload {
                data: data.application,
            }),
        ),
        None => ([0; 32], [0; 32], None),
    };
    RecipientBody {
        record: &OutputRecord {
            kind: RecordKind::Ring,
            amount,
            owner_key: owner.signing_pubkey.confidential_view_tag().unwrap(),
            mint,
            blinding,
            key: record_key(RecordKind::Ring, owner.nullifier_pubkey),
            data_hash,
            ring_data_hash,
        },
        form,
        payload: payload
            .as_ref()
            .map(|payload| (payload, payload.encoded_len())),
    }
    .seal(secret)
    .unwrap()
}

/// The ring auditor every rail here seals exits and capacity slots to.
fn audit_key() -> X25519Pubkey {
    spr_keypair::X25519Keypair::from_bytes(&[0x33; 32]).pubkey()
}

fn assets() -> RingAssets {
    RingAssets::new([USDC]).unwrap()
}

fn signature(leaf: u64) -> [u8; 64] {
    let mut bytes = [0x5a; 64];
    bytes[..8].copy_from_slice(&leaf.to_be_bytes());
    bytes
}

/// A held ring note of `owner`, its nullifier canonical as every real one is.
fn funding_utxo(owner: &ShieldedKeypair, seed: u8, index: u8) -> WalletUtxo {
    let utxo = Utxo {
        owner: owner.signing_pubkey(),
        asset: Mint::SOL,
        amount: 100,
        blinding: field(seed, index),
        ring_program_id: Some(RING),
        data: Data::default(),
    };
    rehashed(
        owner,
        WalletUtxo {
            nullifier: [0; 32],
            utxo,
            nullifier_pubkey: owner.shielded_address().unwrap().nullifier_pubkey,
            utxo_hash: [0; 32],
            tx_viewing_key: None,
            data_hash: None,
            ring_data_hash: None,
            tree_id: TREE,
            leaf_index: u64::from(index),
            latest_tree_id: None,
            slot: 0,
            tx_signature: Signature::from([seed; 64]),
            slot_index: 0,
        },
    )
}

/// `note` with its hash and nullifier rebuilt over its fields.
fn rehashed(owner: &ShieldedKeypair, mut note: WalletUtxo) -> WalletUtxo {
    note.utxo_hash = note
        .utxo
        .hash(&note.nullifier_pubkey, &[0; 32], &[0; 32], TREE)
        .unwrap();
    note.nullifier = owner
        .nullifier(&note.utxo_hash, &note.utxo.blinding)
        .unwrap();
    note
}

/// A canonical field value distinct per pair.
fn field(seed: u8, index: u8) -> [u8; 32] {
    let mut value = [0x11; 32];
    value[30] = seed;
    value[31] = index;
    value
}

fn stream(scan: &AnonymousScan, utxo_hash: [u8; 32]) -> AnonymousStream {
    scan.utxos
        .iter()
        .find(|utxo| utxo.utxo_hash == utxo_hash)
        .map(|utxo| utxo.stream)
        .unwrap_or_else(|| panic!("note {utxo_hash:?} not found"))
}

fn found(scan: &AnonymousScan) -> Vec<[u8; 32]> {
    scan.utxos.iter().map(|utxo| utxo.utxo_hash).collect()
}

/// The hash of the output at `position`.
fn output(tx: &ShieldedTransaction, position: usize) -> [u8; 32] {
    tx.output_slots[position].output_context.hash
}

/// A wallet that never synced, reading `RING` from its start.
fn fresh(wallet: &Wallet) -> ViewingHistory {
    ViewingHistory::new(wallet.keys.viewing_pubkey(), 0).with_ring(RING, None)
}

/// The keys and history of a wallet rebuilt from its seed and its retained rotated keys alone.
fn wiped(seed: u8, retained: &[ViewingKey]) -> (RotatingRingKeys, ViewingHistory) {
    let identity = keypair(seed);
    let mut keys = RotatingRingKeys::new(&identity).unwrap();
    let mut history = ViewingHistory::new(identity.viewing_pubkey(), 0);
    for (created_at, key) in (1..).zip(retained) {
        keys = keys.rotate(key.clone()).unwrap();
        history = history.with_rotated_key(key.pubkey(), created_at).unwrap();
    }
    (keys, history.with_ring(RING, None))
}

async fn scan_from<K: ShieldedKeys + RingKeys>(
    indexer: &Indexer,
    keys: &K,
    history: &ViewingHistory,
) -> AnonymousScan {
    AnonymousSync::new(keys, &assets())
        .with_sender_audit(&TestFrames)
        .fetch(indexer, history)
        .await
        .unwrap()
}

async fn restore<K: ShieldedKeys + RingKeys>(
    indexer: &Indexer,
    keys: &K,
    history: &ViewingHistory,
) -> AnonymousScan {
    AnonymousSync::new(keys, &assets())
        .with_sender_audit(&TestFrames)
        .full_scan(&RecoveryHistory(indexer), history, RING)
        .await
        .unwrap()
}

fn unspent(keys: &RotatingRingKeys, scan: &AnonymousScan) -> Vec<[u8; 32]> {
    let mut hashes: Vec<[u8; 32]> = verify_spendable(keys, scan.decrypted())
        .unwrap()
        .utxos()
        .map(|utxo| utxo.utxo_hash)
        .collect();
    hashes.sort_unstable();
    hashes
}

fn pay<'a>(to: &'a ShieldedAddress, amount: u64) -> Pay<'a> {
    Pay {
        to,
        amount,
        tag: None,
    }
}

fn after_slot(history: &ViewingHistory) -> Option<u64> {
    history.current().ring_positions()[&RING]
}

async fn tag_sync(indexer: &Indexer, keys: &ShieldedKeypair, wallet: &mut AnonymousWallet) {
    let commit = RingConfidentialScan::new(keys, &assets())
        .sync(indexer, wallet, &[])
        .await
        .unwrap();
    wallet.absorb(commit).unwrap();
}

fn held(wallet: &AnonymousWallet) -> Vec<[u8; 32]> {
    wallet.notes().iter().map(|note| note.utxo_hash).collect()
}

/// The wallet holds none of the notes it spends here, only its sender tag finds the change.
#[tokio::test]
async fn one_tag_sync_finds_a_bootstrap_payment_a_request_payment_and_the_change_and_reads_no_ring_history(
) {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let mut bob = Wallet::new(2);
    let carol = Wallet::new(3);
    let mut stored = AnonymousWallet::new(ViewingHistory::new(bob.keys.viewing_pubkey(), 0));
    let request = stored.history().request_tag(&bob.keys).unwrap();
    stored.update_history(request.history).unwrap();
    let selected = stored.history().select(&bob.keys).unwrap();
    stored.update_history(selected.history).unwrap();
    bob.sender_tag = Some(selected.tag.sender());
    let sent = bob.pay(&indexer, &[pay(&carol.address(), 20)]);
    let to_bootstrap = alice.pay(&indexer, &[pay(&bob.address(), 30)]);
    let to_request = Pay {
        to: &bob.address(),
        amount: 7,
        tag: Some(request.tag),
    };
    let to_request = alice.pay(&indexer, &[to_request]);
    tag_sync(&indexer, &bob.keys, &mut stored).await;
    assert_eq!(
        held(&stored),
        [
            output(&sent, 0),
            output(&to_bootstrap, 1),
            output(&to_request, 1)
        ]
    );
    assert_eq!(indexer.take_ring_pages(), 0);
}

/// A wallet restored from its seed starts each counter at zero.
#[tokio::test]
async fn change_and_a_payment_under_the_highest_listed_tags_move_both_counters_past_them() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let mut bob = Wallet::new(2);
    let mut stored = AnonymousWallet::new(ViewingHistory::new(bob.keys.viewing_pubkey(), 0));
    let [sender, request] = [ViewTagStream::Sender, ViewTagStream::Request].map(|stream| {
        bob.keys
            .view_tags(&[ViewTagRequest {
                viewing_pubkey: bob.keys.viewing_pubkey(),
                stream,
                indices: TAG_WINDOW - 1..TAG_WINDOW,
            }])
            .unwrap()[0][0]
    });
    bob.sender_tag = Some(sender);
    let sent = bob.pay(&indexer, &[pay(&alice.address(), 20)]);
    let to_request = Pay {
        to: &bob.address(),
        amount: 7,
        tag: Some(request),
    };
    let paid = alice.pay(&indexer, &[to_request]);
    tag_sync(&indexer, &bob.keys, &mut stored).await;
    assert_eq!(held(&stored), [output(&sent, 0), output(&paid, 1)]);
    let current = stored.history().current();
    assert_eq!(
        (current.tx_count(), current.request_count()),
        (TAG_WINDOW, TAG_WINDOW)
    );
}

/// The ring sync opens every body of a row its caller hands it, no tag selects one.
#[tokio::test]
async fn change_under_no_tag_is_found_from_the_row_of_its_own_transaction() {
    let indexer = Indexer::default();
    let mut bob = Wallet::new(2);
    let carol = Wallet::new(3);
    let mut stored = AnonymousWallet::new(ViewingHistory::new(bob.keys.viewing_pubkey(), 0));
    let sent = bob.pay(&indexer, &[pay(&carol.address(), 20)]);
    tag_sync(&indexer, &bob.keys, &mut stored).await;
    assert!(held(&stored).is_empty(), "no listed tag names the frame");
    let commit = RingConfidentialScan::new(&bob.keys, &assets())
        .sync(&indexer, &stored, core::slice::from_ref(&sent))
        .await
        .unwrap();
    stored.absorb(commit).unwrap();
    assert_eq!(held(&stored), [output(&sent, 0)]);
}

/// A seed holds no request counter, a restore lists `TAG_WINDOW` requests past the last one it found.
#[tokio::test]
async fn a_stored_request_counter_finds_a_late_payment_and_a_seed_only_restore_stops_at_64_unpaid_requests_in_a_row(
) {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let unsynced = || AnonymousWallet::new(ViewingHistory::new(bob.keys.viewing_pubkey(), 0));
    let mut stored = unsynced();
    let mut requests = Vec::new();
    for _ in 0..200 {
        let minted = stored.history().request_tag(&bob.keys).unwrap();
        stored.update_history(minted.history).unwrap();
        requests.push(minted.tag);
    }
    let to_request = Pay {
        to: &bob.address(),
        amount: 30,
        tag: Some(requests[150]),
    };
    let paid = alice.pay(&indexer, &[to_request]);
    tag_sync(&indexer, &bob.keys, &mut stored).await;
    assert_eq!(held(&stored), [output(&paid, 1)]);

    let mut restored = unsynced();
    tag_sync(&indexer, &bob.keys, &mut restored).await;
    assert!(held(&restored).is_empty());
    let counted = restored.history().clone().with_request_count(200);
    restored.update_history(counted).unwrap();
    tag_sync(&indexer, &bob.keys, &mut restored).await;
    assert_eq!(held(&restored), [output(&paid, 1)]);
    assert_eq!(indexer.take_ring_pages(), 0);
}

#[tokio::test]
async fn a_body_sealed_to_another_key_is_skipped_and_the_scan_goes_on() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let carol = Wallet::new(3);
    let to_carol = alice.pay(&indexer, &[pay(&carol.address(), 30)]);
    let to_bob = alice.pay(&indexer, &[pay(&bob.address(), 10)]);
    assert_eq!(
        found(&bob.scan_from(&indexer, &fresh(&bob)).await),
        [output(&to_bob, 1)]
    );
    assert_eq!(
        found(&carol.scan_from(&indexer, &fresh(&carol)).await),
        [output(&to_carol, 1)]
    );
}

/// The production holder across a rotation and a restart, the old key stays readable.
#[tokio::test]
async fn a_rotated_wallet_finds_notes_under_both_keys_through_a_restart_and_a_recovery() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let identity = keypair(2);
    let old_address = identity.shielded_address().unwrap();
    let to_old = alice.pay(&indexer, &[pay(&old_address, 30)]);
    let keys = RotatingRingKeys::new(&identity).unwrap();
    let history = ViewingHistory::new(old_address.viewing_pubkey, 0).with_ring(RING, None);
    let mut wallet = AnonymousWallet::new(history);
    let scan = AnonymousSync::new(&keys, &assets())
        .fetch(&indexer, wallet.history())
        .await
        .unwrap();
    assert_eq!(found(&scan), [output(&to_old, 1)]);
    wallet.absorb(scan).unwrap();

    let next = ViewingKey::new();
    let keys = keys.rotate(next.clone()).unwrap();
    let new_address = keys.address().unwrap();
    assert_eq!(new_address.signing_pubkey, old_address.signing_pubkey);
    assert_ne!(new_address.viewing_pubkey, old_address.viewing_pubkey);
    let rotated = wallet
        .history()
        .clone()
        .with_rotated_key(new_address.viewing_pubkey, 1)
        .unwrap();
    wallet.update_history(rotated).unwrap();
    let to_new = alice.pay(&indexer, &[pay(&new_address, 10)]);
    let to_old_again = alice.pay(&indexer, &[pay(&old_address, 7)]);

    let stored = wallet.to_json();
    let mut current = keypair(2);
    current.viewing_key = next;
    let restarted_keys = RotatingRingKeys::new(&current)
        .unwrap()
        .with_retired(vec![identity.viewing_key.clone()])
        .unwrap();
    let mut restarted = AnonymousWallet::from_json(&stored).unwrap();
    let scan = AnonymousSync::new(&restarted_keys, &assets())
        .fetch(&indexer, restarted.history())
        .await
        .unwrap();
    restarted.absorb(scan).unwrap();
    let mut expected = [
        output(&to_old, 1),
        output(&to_new, 1),
        output(&to_old_again, 1),
    ];
    expected.sort_unstable();
    let mut held: Vec<[u8; 32]> = restarted
        .notes()
        .iter()
        .map(|note| note.utxo_hash)
        .collect();
    held.sort_unstable();
    assert_eq!(held, expected);

    let recovered = AnonymousSync::new(&restarted_keys, &assets())
        .full_scan(&indexer, restarted.history(), RING)
        .await
        .unwrap();
    let mut notes = found(&recovered);
    notes.sort_unstable();
    assert_eq!(notes, expected);
    let unspent = verify_spendable(&restarted_keys, recovered.decrypted()).unwrap();
    assert_eq!(unspent.utxos().count(), expected.len());
}

#[tokio::test]
async fn a_ring_watched_after_rotation_reads_payments_to_the_retired_key() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let identity = keypair(2);
    let old_address = identity.shielded_address().unwrap();
    let keys = RotatingRingKeys::new(&identity)
        .unwrap()
        .rotate(ViewingKey::new())
        .unwrap();
    let history = ViewingHistory::new(old_address.viewing_pubkey, 0)
        .with_rotated_key(keys.address().unwrap().viewing_pubkey, 1)
        .unwrap()
        .with_ring(RING, None);
    let payment = alice.pay(&indexer, &[pay(&old_address, 30)]);
    let scan = AnonymousSync::new(&keys, &assets())
        .fetch(&indexer, &history)
        .await
        .unwrap();
    assert_eq!(found(&scan), [output(&payment, 1)]);
    assert!(scan
        .history()
        .entries()
        .iter()
        .all(|entry| entry.ring_positions()[&RING] == Some(payment.slot)));
}

/// A key added after a sync reads the ring's whole history under its own query, the stored key reads on.
#[tokio::test]
async fn an_imported_key_finds_the_notes_below_the_wallets_position() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let identity = keypair(2);
    let imported_key = ViewingKey::new();
    let old_address = ShieldedAddress {
        viewing_pubkey: imported_key.pubkey(),
        ..identity.shielded_address().unwrap()
    };
    let old_note = alice.pay(&indexer, &[pay(&old_address, 30)]);
    let current = identity.viewing_pubkey();
    let alone = RotatingRingKeys::new(&identity).unwrap();
    let synced = AnonymousSync::new(&alone, &assets())
        .fetch(
            &indexer,
            &ViewingHistory::new(current, 5).with_ring(RING, None),
        )
        .await
        .unwrap();
    assert!(found(&synced).is_empty(), "the holder lacks the old key");
    let position = after_slot(synced.history());
    assert_eq!(position, Some(old_note.slot));
    let imported = synced
        .history()
        .clone()
        .with_imported_key(imported_key.pubkey(), 1)
        .unwrap();
    assert_eq!(
        after_slot(&imported),
        position,
        "the current key keeps its position"
    );
    indexer.take_ring_pages();
    let keys = RotatingRingKeys::new(&identity)
        .unwrap()
        .with_retired(vec![imported_key.clone()])
        .unwrap();
    let scan = AnonymousSync::new(&keys, &assets())
        .fetch(&indexer, &imported)
        .await
        .unwrap();
    assert_eq!(found(&scan), [output(&old_note, 1)]);
    assert_eq!(
        indexer.take_ring_pages(),
        3,
        "the target probe and one query per position"
    );
    let retired = scan.history().entry(&imported_key.pubkey()).unwrap();
    assert_eq!(retired.ring_positions()[&RING], Some(old_note.slot));
}

/// Pages stop inside one slot, a run that fails there stores nothing and the next finds every row once.
#[tokio::test]
async fn a_restart_inside_a_split_slot_skips_nothing_and_repeats_nothing() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let split: Vec<ShieldedTransaction> = [10, 11, 12]
        .map(|amount| {
            let mut paid = alice.seal(&indexer, &[pay(&bob.address(), amount)]);
            paid.slot = 5;
            indexer.publish(paid.clone());
            paid
        })
        .into();
    *indexer.page_cap.lock().unwrap() = Some(1);
    *indexer.failing_page.lock().unwrap() = Some(1);
    let wallet = AnonymousWallet::new(fresh(&bob));
    let failed = AnonymousSync::new(&bob.keys, &assets())
        .fetch(&indexer, wallet.history())
        .await;
    assert!(failed.is_err(), "a failed page ends the read");

    *indexer.failing_page.lock().unwrap() = None;
    indexer.take_ring_pages();
    let restarted = AnonymousWallet::from_json(&wallet.to_json()).unwrap();
    let scan = bob.scan_from(&indexer, restarted.history()).await;
    assert_eq!(
        indexer.take_ring_pages(),
        4,
        "the target probe and a token on every page ahead of more"
    );
    assert_eq!(scan.utxos.len(), split.len());
    for paid in &split {
        stream(&scan, output(paid, 1));
    }
    assert_eq!(after_slot(scan.history()), Some(5));

    let later = alice.pay(&indexer, &[pay(&bob.address(), 13)]);
    let next = bob.scan_from(&indexer, scan.history()).await;
    assert_eq!(found(&next), [output(&later, 1)]);
    assert_eq!(after_slot(next.history()), Some(later.slot));
}

/// A crash on either side of the one replace skips no note, a scan is stored only through `absorb`.
#[tokio::test]
async fn a_crash_on_either_side_of_the_one_write_skips_no_funds() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let paid = alice.pay(&indexer, &[pay(&bob.address(), 30)]);
    let mut wallet = AnonymousWallet::new(fresh(&bob));
    let before = wallet.to_json();
    let scan = bob.scan_from(&indexer, wallet.history()).await;
    assert!(after_slot(scan.history()).is_some());

    // The position alone is refused, its notes unwritten.
    assert!(matches!(
        wallet.update_history(scan.history().clone()),
        Err(AnonymousError::ScanProgressMoved)
    ));

    // The crash before the replace.
    let restarted = AnonymousWallet::from_json(&before).unwrap();
    assert_eq!(after_slot(restarted.history()), None);
    let replay = bob.scan_from(&indexer, restarted.history()).await;
    assert_eq!(found(&replay), [output(&paid, 1)]);

    // The crash after it.
    wallet.absorb(scan).unwrap();
    let after = wallet.to_json();
    let mut restarted = AnonymousWallet::from_json(&after).unwrap();
    assert_eq!(restarted, wallet);
    assert_eq!(restarted.notes()[0].utxo_hash, output(&paid, 1));
    assert!(bob
        .scan_from(&indexer, restarted.history())
        .await
        .utxos
        .is_empty());

    let before = restarted.clone();
    assert!(matches!(
        restarted.absorb(replay),
        Err(AnonymousError::ScanHistoryChanged)
    ));
    assert_eq!(restarted, before);
    assert_eq!(restarted.notes().len(), 1);
}

#[tokio::test]
async fn a_scan_cannot_overwrite_a_later_tag_or_key() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    alice.pay(&indexer, &[pay(&bob.address(), 30)]);
    let history = fresh(&bob);
    let updates = [
        history.request_tag(&bob.keys).unwrap().history,
        history
            .clone()
            .with_rotated_key(ViewingKey::new().pubkey(), 1)
            .unwrap(),
    ];
    for updated in updates {
        let mut wallet = AnonymousWallet::new(history.clone());
        let scan = bob.scan_from(&indexer, wallet.history()).await;
        wallet.update_history(updated).unwrap();
        let before = wallet.clone();
        assert!(matches!(
            wallet.absorb(scan),
            Err(AnonymousError::ScanHistoryChanged)
        ));
        assert_eq!(wallet, before);
    }
}

#[tokio::test]
async fn a_tag_counter_update_keeps_the_stored_progress() {
    let bob = Wallet::new(2);
    let mut wallet = AnonymousWallet::new(fresh(&bob));
    let advanced = wallet.history().request_tag(&bob.keys).unwrap().history;
    wallet.update_history(advanced.clone()).unwrap();
    assert_eq!(*wallet.history(), advanced);
    assert_eq!(wallet.history().current().request_count(), 1);
}

#[tokio::test]
async fn a_page_too_large_halves_the_limit_and_loses_no_row() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let paid = [
        alice.pay(&indexer, &[pay(&bob.address(), 10)]),
        alice.pay(&indexer, &[pay(&bob.address(), 11)]),
    ];
    *indexer.response_limit.lock().unwrap() = Some(1);
    *indexer.page_cap.lock().unwrap() = Some(1);
    let scan = bob.scan_from(&indexer, &fresh(&bob)).await;
    assert_eq!(found(&scan), paid.each_ref().map(|paid| output(paid, 1)));
    let asked = indexer.limits_asked.lock().unwrap().clone();
    assert_eq!(asked[..2], [1000, 500]);
    assert_eq!(asked.iter().filter(|limit| **limit == 1).count(), 3);
    assert_eq!(after_slot(scan.history()), Some(paid[1].slot));
}

#[tokio::test]
async fn a_page_too_large_at_limit_one_fails_the_read_and_moves_nothing() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    alice.pay(&indexer, &[pay(&bob.address(), 10)]);
    *indexer.response_limit.lock().unwrap() = Some(0);
    let refused = AnonymousSync::new(&bob.keys, &assets())
        .fetch(&indexer, &fresh(&bob))
        .await;
    assert!(matches!(
        refused,
        Err(AnonymousError::Client(ClientError::Indexer(_)))
    ));
    assert_eq!(indexer.limits_asked.lock().unwrap().last(), Some(&1));
}

#[tokio::test]
async fn a_repeated_row_of_one_transaction_refuses_the_page() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let paid = alice.pay(&indexer, &[pay(&bob.address(), 10)]);
    indexer.publish(paid);
    let refused = AnonymousSync::new(&bob.keys, &assets())
        .fetch(&indexer, &fresh(&bob))
        .await;
    assert!(matches!(refused, Err(AnonymousError::InvalidHistoryPage)));
}

/// A relayer's transaction key is not the wallet's, the wallet lists a payment only under the key it derives.
#[tokio::test]
async fn a_relayed_proof_under_another_transaction_key_keeps_its_change_and_lists_no_payment() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let relayed = alice.seal_under(&indexer, &[pay(&bob.address(), 30)], ViewingKey::new());
    indexer.publish(relayed.clone());
    let own = alice.pay(&indexer, &[pay(&bob.address(), 20)]);
    let scan = AnonymousSync::new(&alice.keys, &assets())
        .with_sender_audit(&TestFrames)
        .fetch(&indexer, &fresh(&alice))
        .await
        .unwrap();
    assert_eq!(found(&scan), [output(&relayed, 0), output(&own, 0)]);
    assert_eq!(scan.sent.len(), 1, "a sent row needs the wallet's own key");
    assert_eq!(scan.sent[0].tx_signature, own.tx_signature);
}

/// A payment to itself and the change are two SPL change outputs, each opened from its own record.
#[tokio::test]
async fn a_sender_recovers_two_spl_change_outputs_after_a_wipe() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let mut note = funding_utxo(&alice.keys, 1, 9);
    note.utxo.asset = USDC;
    note.utxo.amount = 40;
    alice.funding.insert(0, rehashed(&alice.keys, note));
    let paid = alice.pay(&indexer, &[pay(&alice.address(), 25)]);
    assert!(paid.output_slots[..2]
        .iter()
        .all(|slot| zolana_ring_keys::ring_body_output(&slot.payload).is_some()));
    let (keys, history) = wiped(1, &[]);
    let restored = restore(&indexer, &keys, &history).await;
    let mut amounts: Vec<(Mint, u64)> = restored
        .decrypted()
        .utxos
        .iter()
        .map(|note| (note.utxo.asset, note.utxo.amount))
        .collect();
    amounts.sort_unstable_by_key(|(_, amount)| *amount);
    assert_eq!(amounts, [(USDC, 15), (USDC, 25)]);
    assert_eq!(unspent(&keys, &restored), {
        let mut both = [output(&paid, 0), output(&paid, 1)];
        both.sort_unstable();
        both
    });
}

#[tokio::test]
async fn a_sender_recovers_its_change_after_a_wipe() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let paid = alice.pay(&indexer, &[pay(&bob.address(), 30)]);
    let scan = alice.recover(&indexer).await;
    assert_eq!(found(&scan), [output(&paid, 0)]);
    assert_eq!(stream(&scan, output(&paid, 0)), AnonymousStream::FullScan);
    assert_eq!(scan.sent.len(), 1);
    assert_eq!(after_slot(scan.history()), Some(paid.slot));
}

/// A full spend leads with a zero change, the sender recovers it beside its payment.
#[tokio::test]
async fn a_sender_that_keeps_nothing_recovers_its_zero_change_and_payment_row_after_a_wipe() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let paid = alice.pay(&indexer, &[pay(&bob.address(), 100)]);
    let scan = alice.recover(&indexer).await;
    assert_eq!(found(&scan), [output(&paid, 0)]);
    assert_eq!(scan.sent.len(), 1);
    assert_eq!(scan.sent[0].amount, 100);
    assert_eq!(scan.sent[0].utxo_hash, output(&paid, 1));
}

/// The second transfer spends the first change, the wiped sender restores the chain from its seed.
#[tokio::test]
async fn a_wiped_sender_restores_its_change_chain_and_payment_rows_from_the_seed() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let (bob, carol) = (Wallet::new(2), Wallet::new(3));
    let first = alice.pay(&indexer, &[pay(&bob.address(), 30)]);
    let change = alice
        .scan_from(&indexer, &fresh(&alice))
        .await
        .decrypted()
        .utxos[0]
        .clone();
    alice.funding.insert(0, change);
    let second = alice.pay(&indexer, &[pay(&carol.address(), 20)]);

    let (keys, history) = wiped(1, &[]);
    let restored = restore(&indexer, &keys, &history).await;
    assert_eq!(found(&restored), [output(&first, 0), output(&second, 0)]);
    assert_eq!(unspent(&keys, &restored), [output(&second, 0)]);
    let sent: Vec<u64> = restored.sent.iter().map(|row| row.amount).collect();
    assert_eq!(sent, [30, 20]);
    let positioned = scan_from(&indexer, &keys, &history).await;
    assert_eq!(unspent(&keys, &positioned), [output(&second, 0)]);
}

#[tokio::test]
async fn a_wiped_wallet_restores_a_merge_of_its_change_and_a_payment_from_the_seed() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let mut bob = Wallet::new(2);
    let carol = Wallet::new(3);
    alice.pay(&indexer, &[pay(&bob.address(), 30)]);
    let paid = bob.pay(&indexer, &[pay(&carol.address(), 40)]);
    let held = bob.scan_from(&indexer, &fresh(&bob)).await;
    let nullifiers: Vec<[u8; 32]> = held
        .decrypted()
        .utxos
        .iter()
        .map(|utxo| utxo.nullifier)
        .collect();
    assert_eq!(nullifiers.len(), 2, "the payment and the change");
    let merged = publish_merge(&indexer, &bob, nullifiers, 90);

    let (keys, history) = wiped(2, &[]);
    let restored = restore(&indexer, &keys, &history).await;
    assert!(found(&restored).contains(&output(&paid, 0)));
    assert_eq!(unspent(&keys, &restored), [merged]);
    let positioned = scan_from(&indexer, &keys, &history).await;
    assert_eq!(unspent(&keys, &positioned), [merged]);
}

/// The seed derives no rotated key, a note under a lost one stays closed to the wallet.
#[tokio::test]
async fn a_wiped_rotated_wallet_restores_every_note_only_with_its_retained_key() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let mut bob = Wallet::new(2);
    let carol = Wallet::new(3);
    let to_old = alice.pay(&indexer, &[pay(&bob.address(), 30)]);
    let next = ViewingKey::new();
    bob.keys.viewing_key = next.clone();
    let to_new = alice.pay(&indexer, &[pay(&bob.address(), 10)]);
    let sent = bob.pay(&indexer, &[pay(&carol.address(), 40)]);
    let mut held = [output(&to_old, 1), output(&to_new, 1), output(&sent, 0)];
    held.sort_unstable();

    let (keys, history) = wiped(2, &[next]);
    let restored = restore(&indexer, &keys, &history).await;
    assert_eq!(unspent(&keys, &restored), held);
    assert_eq!(restored.sent.len(), 1);

    let (seed_only, history) = wiped(2, &[]);
    let restored = restore(&indexer, &seed_only, &history).await;
    assert_eq!(found(&restored), [output(&to_old, 1)]);
    assert!(restored
        .auditor_recovery
        .unopened
        .contains(&output(&to_new, 1)));
}

#[tokio::test]
async fn the_change_lists_every_payment_and_a_held_address_verifies_its_row() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2).address();
    let carol = Wallet::new(3).address();
    let paid = alice.pay(&indexer, &[pay(&bob, 30), pay(&carol, 20)]);
    let forged = ShieldedAddress {
        nullifier_pubkey: Wallet::new(4).address().nullifier_pubkey,
        ..carol
    };
    let held = [bob, forged];
    let scan = AnonymousSync::new(&alice.keys, &assets())
        .with_sender_audit(&TestFrames)
        .with_recipient_addresses(&held)
        .fetch(&indexer, &fresh(&alice))
        .await
        .unwrap();
    let rows: Vec<_> = scan
        .sent
        .iter()
        .map(|row| {
            (
                row.position,
                row.recipient_viewing_pubkey,
                row.owner_pubkey,
                row.amount,
                row.check,
                row.utxo_hash,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                1,
                bob.viewing_pubkey,
                Some(bob.signing_pubkey),
                30,
                SentCheck::Verified { mint: Mint::SOL },
                output(&paid, 1),
            ),
            (
                2,
                carol.viewing_pubkey,
                None,
                20,
                SentCheck::Unverified,
                output(&paid, 2),
            ),
        ]
    );
    assert!(scan
        .sent
        .iter()
        .all(|row| row.tx_signature == paid.tx_signature));
    let unheld = alice.scan_from(&indexer, &fresh(&alice)).await;
    assert!(unheld
        .sent
        .iter()
        .all(|row| row.check == SentCheck::Unverified));
    assert_eq!(unheld.sent.len(), 2);
}

/// A listed payment whose body opens to nothing leaves no row, the sync and the change go on.
#[tokio::test]
async fn a_sent_body_that_fails_to_open_leaves_no_row() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2).address();
    let mut paid = alice.seal(&indexer, &[pay(&bob, 30)]);
    // A ring body cut short, behind the encoding tag, the length and the scheme.
    let short_body = RING_RECORD_LEN - 1;
    let payload = &mut paid.output_slots[1].payload;
    payload.truncate(6 + short_body);
    payload[1..5].copy_from_slice(&u32::try_from(1 + short_body).unwrap().to_le_bytes());
    indexer.publish(paid.clone());
    let scan = alice.scan_from(&indexer, &fresh(&alice)).await;
    assert!(scan.sent.is_empty());
    assert_eq!(stream(&scan, output(&paid, 0)), AnonymousStream::FullScan);
}

/// A griefing note costs one stored row, the next open reads past it and registering spends it.
#[tokio::test]
async fn an_unknown_mint_keeps_the_position_moving_and_its_note_unspendable_until_registered() {
    let indexer = Indexer::default();
    let bob = Wallet::new(2);
    let sender = keypair(3);
    let reply = ReplyMetadata {
        viewing_key: sender.viewing_pubkey(),
        owner_key: sender.signing_pubkey().as_ed25519().unwrap(),
        nullifier_pk: sender.nullifier_key.pubkey().unwrap(),
        data: Data::default(),
    };
    let unregistered = Mint {
        asset: Address::new_from_array([0xee; 32]),
        asset_id: 9,
    };
    let tx_key = ViewingKey::new();
    let salt = random_salt();
    let first_nullifier = field(9, 6);
    let view = bob.keys.viewing_pubkey();
    let secret = DeliveredSlot {
        agreement: &tx_key.ecdh(&view).unwrap(),
        tx_viewing_pk: tx_key.pubkey().as_bytes(),
        recipient_viewing_pk: view.as_bytes(),
        salt: &salt,
        output_index: 0,
        first_nullifier: &first_nullifier,
    }
    .secret()
    .unwrap();
    let blinding = field(9, 5);
    let hash = Utxo {
        owner: bob.address().signing_pubkey,
        asset: unregistered,
        amount: 5,
        blinding,
        ring_program_id: Some(RING),
        data: Data::default(),
    }
    .hash(&bob.address().nullifier_pubkey, &[0; 32], &[0; 32], TREE)
    .unwrap();
    let body = ring_body(
        &bob.address(),
        (5, unregistered.asset.to_bytes(), blinding),
        None,
        &secret,
    );
    let mut transaction = ShieldedTransaction {
        slot: 3,
        tx_signature: Signature::from(signature(3)),
        event_index: Some(0),
        tx_viewing_pk: Some(tx_key.pubkey()),
        salt: Some(salt),
        output_slots: vec![OutputSlot {
            view_tag: bootstrap_slot_tag(view.u()),
            output_context: OutputContext {
                hash,
                tree_id: TREE,
                leaf_index: 0,
            },
            payload: encode_ring_body_output(
                &body,
                &seal_reply(&reply, MIN_REPLY_LEN, &secret).unwrap(),
            )
            .unwrap(),
        }],
        messages: Vec::new(),
        nullifiers: vec![first_nullifier],
        proofless: false,
        ring_config: None,
        ring_program_id: Some(RING),
    };
    indexer.publish(transaction.clone());
    transaction.slot = 4;
    transaction.tx_signature = Signature::from(signature(4));
    transaction.output_slots[0].output_context.hash = field(8, 8);
    indexer.publish(transaction);
    let mut wallet = AnonymousWallet::new(fresh(&bob));
    let scan = bob.scan_from(&indexer, wallet.history()).await;
    assert_eq!(scan.unmatched, [hash]);
    assert_eq!(after_slot(scan.history()), Some(4), "the position moves");
    assert!(scan.utxos.is_empty());
    let [claim] = scan.sender_claims() else {
        panic!("one claim from the matching commitment");
    };
    assert_eq!(claim.utxo_hash, hash);
    assert_eq!(claim.claim.claimed(), &reply);
    wallet.absorb(scan).unwrap();

    let mut wallet = AnonymousWallet::from_json(&wallet.to_json()).unwrap();
    let [pending] = wallet.pending_notes() else {
        panic!("one pending note");
    };
    assert_eq!(
        (pending.mint(), pending.amount(), pending.utxo_hash()),
        (unregistered.asset, 5, hash)
    );
    let [claim] = wallet.pending_claims() else {
        panic!("one stored claim");
    };
    assert_eq!(claim.utxo_hash, hash);
    assert_eq!(claim.claimed_viewing_key, reply.viewing_key);
    assert_eq!(claim.claimed_owner_key, reply.owner_key);
    assert_eq!(claim.claimed_nullifier_pk, reply.nullifier_pk);
    assert!(wallet.notes().is_empty());
    assert_eq!(
        verify_spendable(&bob.keys, &wallet.decrypted())
            .unwrap()
            .utxos()
            .count(),
        0,
        "nothing to spend before the mint registers"
    );

    let next = bob.scan_from(&indexer, wallet.history()).await;
    assert!(
        next.utxos.is_empty() && next.unmatched.is_empty(),
        "no row reads again"
    );
    wallet.absorb(next).unwrap();
    assert_eq!(wallet.pending_notes().len(), 1);

    wallet.resolve_pending(&assets());
    assert_eq!(wallet.pending_notes().len(), 1, "still unregistered");

    indexer.take_ring_pages();
    wallet.resolve_pending(&RingAssets::new([unregistered]).unwrap());
    assert_eq!(indexer.take_ring_pages(), 0, "resolving reads no history");
    assert!(wallet.pending_notes().is_empty());
    let spendable = verify_spendable(&bob.keys, &wallet.decrypted()).unwrap();
    let notes: Vec<_> = spendable.utxos().collect();
    let [note] = notes.as_slice() else {
        panic!("one spendable note");
    };
    assert_eq!(note.utxo_hash, hash);
    assert_eq!(note.utxo.asset, unregistered);
    assert_ne!(note.nullifier, [0; 32]);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HolderCall {
    Decrypt,
    DeliverySecrets,
    MergeOutputKey,
}

/// A remote holder whose first call of one kind fails.
struct FlakyHolder {
    keys: ShieldedKeypair,
    failing: HolderCall,
    failed: AtomicBool,
}

impl FlakyHolder {
    fn new(seed: u8, failing: HolderCall) -> Self {
        Self {
            keys: keypair(seed),
            failing,
            failed: AtomicBool::new(false),
        }
    }

    fn fails(&self, call: HolderCall) -> bool {
        call == self.failing && !self.failed.swap(true, Ordering::Relaxed)
    }
}

impl ShieldedKeys for FlakyHolder {
    fn address(&self) -> Result<ShieldedAddress, TransactionError> {
        self.keys.address()
    }

    fn viewing_public_keys(&self) -> Vec<X25519Pubkey> {
        self.keys.viewing_public_keys()
    }

    fn decrypt(&self, requests: &[DecryptRequest<'_>]) -> Result<Vec<Vec<u8>>, TransactionError> {
        if self.fails(HolderCall::Decrypt) {
            return Err(TransactionError::IncompleteDecryption { got: 0, want: 1 });
        }
        self.keys.decrypt(requests)
    }

    fn derive(&self, requests: &[DeriveRequest]) -> Result<Vec<[u8; 32]>, TransactionError> {
        let asks_merge_key = requests
            .iter()
            .any(|request| matches!(request, DeriveRequest::MergeOutputKey { .. }));
        if asks_merge_key && self.fails(HolderCall::MergeOutputKey) {
            return Err(TransactionError::IncompleteDerivation { got: 0, want: 1 });
        }
        self.keys.derive(requests)
    }

    fn transaction_keys(
        &self,
        requests: &[TransactionKeyRequest],
    ) -> Result<Vec<ViewingKey>, TransactionError> {
        self.keys.transaction_keys(requests)
    }
}

impl RingKeys for FlakyHolder {
    fn ring_delivery_secrets(
        &self,
        requests: &[RingDeliveryRequest],
    ) -> Result<Vec<[u8; 32]>, RingKeyError> {
        if self.fails(HolderCall::DeliverySecrets) {
            return Err(RingKeyError::UnknownViewingKey);
        }
        self.keys.ring_delivery_secrets(requests)
    }

    fn view_tags(&self, requests: &[ViewTagRequest]) -> Result<Vec<Vec<ViewTag>>, RingKeyError> {
        self.keys.view_tags(requests)
    }
}

#[tokio::test]
async fn a_holder_failure_in_recovery_fails_the_read_and_the_retry_finds_the_note() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let paid = alice.pay(&indexer, &[pay(&bob.address(), 30)]);
    let holder = FlakyHolder::new(2, HolderCall::DeliverySecrets);
    let history = fresh(&bob);
    assert!(AnonymousSync::new(&holder, &assets())
        .full_scan(&indexer, &history, RING)
        .await
        .is_err());
    let scan = AnonymousSync::new(&holder, &assets())
        .full_scan(&indexer, &history, RING)
        .await
        .unwrap();
    assert_eq!(found(&scan), [output(&paid, 1)]);
}

/// A merge of notes the wallet never opened names only unknown nullifiers, recovery still rebuilds it.
#[tokio::test]
async fn recovery_opens_a_merge_whose_inputs_the_wallet_never_saw() {
    let indexer = Indexer::default();
    let bob = Wallet::new(2);
    let first = [0x11; 32];
    let merged_hash = publish_merge(&indexer, &bob, vec![first], 100);
    let scan = AnonymousSync::new(&bob.keys, &assets())
        .full_scan(&indexer, &fresh(&bob), RING)
        .await
        .unwrap();
    assert_eq!(found(&scan), [merged_hash]);
    assert_eq!(
        scan.utxos[0].stream,
        AnonymousStream::Merge {
            first_nullifier: first
        }
    );
    let positioned = bob.scan_from(&indexer, &fresh(&bob)).await;
    assert!(positioned.utxos.is_empty());
}

/// A holder failure on the first page's merge ends the read before a second page.
#[tokio::test]
async fn an_every_site_recovery_opens_each_page_before_the_next_read() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let (bob, carol) = (Wallet::new(2), Wallet::new(3));
    publish_merge(&indexer, &bob, vec![[0x11; 32]], 100);
    for _ in 0..3 {
        alice.pay(&indexer, &[pay(&carol.address(), 10)]);
    }
    *indexer.page_cap.lock().unwrap() = Some(1);
    let holder = FlakyHolder::new(2, HolderCall::MergeOutputKey);
    indexer.take_ring_pages();
    assert!(AnonymousSync::new(&holder, &assets())
        .full_scan(&indexer, &fresh(&bob), RING)
        .await
        .is_err());
    assert_eq!(indexer.take_ring_pages(), 2);
    let queries = indexer.ring_queries.lock().unwrap();
    assert_eq!(queries.len(), 2);
    assert_eq!(queries[0].filters.slot.lte, None);
    assert_eq!(queries[1].filters.slot.lte, Some(indexer.context().slot));
    assert!(queries.iter().all(|query| query.pagination_token.is_none()));
}

/// Ring rows also spend notes of other owners, the stored wallet keeps only the spends of its own notes.
#[tokio::test]
async fn the_stored_wallet_keeps_only_its_own_nullifiers() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let (bob, carol) = (Wallet::new(2), Wallet::new(3));
    alice.pay(&indexer, &[pay(&bob.address(), 30)]);
    for _ in 0..3 {
        alice.pay(&indexer, &[pay(&carol.address(), 10)]);
    }
    let mut wallet = AnonymousWallet::new(fresh(&bob));
    wallet
        .absorb(bob.scan_from(&indexer, wallet.history()).await)
        .unwrap();
    assert!(wallet.decrypted().spent_nullifiers.is_empty());
    let note = wallet.notes()[0].clone();
    publish_merge(&indexer, &bob, vec![note.nullifier], 30);
    alice.pay(&indexer, &[pay(&carol.address(), 10)]);
    let scan = AnonymousSync::new(&bob.keys, &assets())
        .fetch_wallet(&indexer, &wallet)
        .await
        .unwrap();
    wallet.absorb(scan).unwrap();
    let own = HashSet::from([note.nullifier]);
    assert_eq!(wallet.decrypted().spent_nullifiers, own);
    let mut restored = AnonymousWallet::new(fresh(&bob));
    restored.absorb(bob.recover(&indexer).await).unwrap();
    assert_eq!(restored.decrypted().spent_nullifiers, own);
    let queried: HashSet<_> = indexer
        .nullifier_queries
        .lock()
        .unwrap()
        .iter()
        .flatten()
        .copied()
        .collect();
    let owned: HashSet<_> = restored.notes().iter().map(|note| note.nullifier).collect();
    assert_eq!(queried, owned);
}

/// Another device of the wallet spends a stored note, the next sync marks it spent with no held notes passed.
#[tokio::test]
async fn a_stored_note_spent_elsewhere_is_spent_after_a_wallet_sync() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let carol = Wallet::new(3);
    let bob = Wallet::new(2);
    alice.pay(&indexer, &[pay(&bob.address(), 30)]);
    let mut wallet = AnonymousWallet::new(fresh(&bob));
    wallet
        .absorb(bob.scan_from(&indexer, wallet.history()).await)
        .unwrap();
    let note = wallet.notes()[0].clone();
    let mut other_device = Wallet::new(2);
    other_device.funding = vec![note.clone()];
    other_device.pay(&indexer, &[pay(&carol.address(), 30)]);
    let scan = AnonymousSync::new(&bob.keys, &assets())
        .fetch_wallet(&indexer, &wallet)
        .await
        .unwrap();
    wallet.absorb(scan).unwrap();
    assert_eq!(wallet.note_state(&note), NoteState::Spent);
}

#[test]
fn a_ring_added_after_a_rotation_reaches_every_held_key() {
    let old = ViewingKey::new().pubkey();
    let new = ViewingKey::new().pubkey();
    let history = ViewingHistory::new(old, 0)
        .with_rotated_key(new, 1)
        .unwrap()
        .with_ring(RING, None);
    for key in [old, new] {
        assert!(history
            .entry(&key)
            .unwrap()
            .ring_positions()
            .contains_key(&RING));
    }
}

/// `Spent` wins over the zero key, and a spent note leaves the exposed list and the balance.
#[tokio::test]
async fn a_spent_zero_key_note_leaves_the_exposed_list() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let unkeyed = ShieldedAddress {
        nullifier_pubkey: ZERO_NULLIFIER_PUBKEY,
        ..bob.address()
    };
    let paid = alice.pay(&indexer, &[pay(&unkeyed, 30)]);
    let scan = bob.scan_from(&indexer, &fresh(&bob)).await;
    assert_eq!(found(&scan), [output(&paid, 1)]);
    let mut wallet = AnonymousWallet::new(fresh(&bob));
    wallet.absorb(scan).unwrap();
    let note = wallet.notes()[0].clone();
    assert_eq!(wallet.note_state(&note), NoteState::ExposedIfSpent);
    let balance = wallet.balances()[&Mint::SOL.asset];
    assert_eq!((balance.spendable, balance.exposed_if_spent), (0, 30));
    let owned = verify_ring_owned(&bob.keys, &wallet.decrypted()).unwrap();
    assert!(owned.owned.utxos.is_empty());
    assert_eq!(owned.exposed_if_spent.len(), 1);
    assert_eq!(owned.exposed_if_spent[0].nullifier, note.nullifier);

    let spent = DecryptionResult {
        spent_nullifiers: [note.nullifier].into_iter().collect(),
        ..wallet.decrypted()
    };
    let owned = verify_ring_owned(&bob.keys, &spent).unwrap();
    assert!(owned.exposed_if_spent.is_empty());
    assert!(owned.spent_exposed.contains(&note.nullifier));
    assert_eq!(
        note_state(&note, |nullifier| spent
            .spent_nullifiers
            .contains(nullifier)),
        NoteState::Spent
    );
}

#[tokio::test]
async fn a_merge_of_the_wallets_notes_is_found_by_recovery_and_marks_them_spent() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    for amount in [70, 30] {
        alice.pay(&indexer, &[pay(&bob.address(), amount)]);
    }
    let mut wallet = AnonymousWallet::new(fresh(&bob));
    wallet
        .absorb(bob.scan_from(&indexer, wallet.history()).await)
        .unwrap();
    let nullifiers: Vec<[u8; 32]> = wallet.notes().iter().map(|utxo| utxo.nullifier).collect();
    let first = nullifiers[0];
    let merged_hash = publish_merge(&indexer, &bob, nullifiers, 100);
    let next = AnonymousSync::new(&bob.keys, &assets())
        .fetch_wallet(&indexer, &wallet)
        .await
        .unwrap();
    assert_eq!(
        stream(&next, merged_hash),
        AnonymousStream::Merge {
            first_nullifier: first
        }
    );
    let recovered = bob.recover(&indexer).await;
    assert_eq!(
        stream(&recovered, merged_hash),
        AnonymousStream::Merge {
            first_nullifier: first
        }
    );
    let unspent: Vec<[u8; 32]> = verify_spendable(&bob.keys, recovered.decrypted())
        .unwrap()
        .utxos()
        .map(|utxo| utxo.utxo_hash)
        .collect();
    assert_eq!(unspent, [merged_hash]);
}

#[tokio::test]
async fn a_reply_stays_an_unverified_claim_until_the_wallet_confirms_it() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let first = alice.pay(&indexer, &[pay(&bob.address(), 10)]);
    let sender = alice.keys.viewing_pubkey();
    let unconfirmed = bob.scan_from(&indexer, &fresh(&bob)).await;
    let [claim] = unconfirmed.sender_claims() else {
        panic!("one claim on the note found");
    };
    assert_eq!(claim.utxo_hash, output(&first, 1));
    assert_eq!(claim.claim.claimed().viewing_key, sender);
    let mut wallet = AnonymousWallet::new(fresh(&bob));
    assert!(matches!(
        wallet.confirm_claim(sender),
        Err(AnonymousError::UnknownSenderClaim)
    ));
    wallet.absorb(unconfirmed).unwrap();
    let restarted = AnonymousWallet::from_json(&wallet.to_json()).unwrap();
    assert_eq!(restarted.pending_claims().len(), 1);
    assert_eq!(restarted.pending_claims()[0].claimed_viewing_key, sender);
    let mut confirmed = restarted;
    confirmed.confirm_claim(sender).unwrap();
    assert!(confirmed.pending_claims().is_empty());
}

/// A send carries no reply unless it opts in, the recipient learns no sender.
#[tokio::test]
async fn a_payment_without_a_reply_names_no_sender() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    alice.replies = false;
    let bob = Wallet::new(2);
    let paid = alice.pay(&indexer, &[pay(&bob.address(), 10)]);
    assert_eq!(
        paid.output_slots[1].payload.len(),
        8 + RING_RECORD_LEN,
        "no reply bytes follow the body"
    );
    let scan = bob.scan_from(&indexer, &fresh(&bob)).await;
    assert_eq!(scan.utxos.len(), 1);
    assert!(scan.sender_claims().is_empty());
}

#[test]
fn a_wallet_document_round_trips_and_refuses_an_unknown_field() {
    let bob = Wallet::new(2);
    let rotated = fresh(&bob)
        .with_rotated_key(keypair(4).viewing_pubkey(), 5)
        .unwrap();
    let wallet = AnonymousWallet::new(rotated);
    let json = wallet.to_json();
    assert_eq!(AnonymousWallet::from_json(&json).unwrap(), wallet);
    let other = json.replacen('{', r#"{"other":6,"#, 1);
    assert!(matches!(
        AnonymousWallet::from_json(&other),
        Err(AnonymousError::WalletDocument(_))
    ));
    assert!(matches!(
        AnonymousWallet::from_json(r#"{"viewingHistory":[]}"#),
        Err(AnonymousError::WalletDocument(_))
    ));
    let mut repeated: serde_json::Value = serde_json::from_str(&json).unwrap();
    let key = repeated["history"]["viewingHistory"][0]["viewingPubkey"].clone();
    repeated["history"]["viewingHistory"][1]["viewingPubkey"] = key.clone();
    let Err(AnonymousError::WalletDocument(reason)) =
        AnonymousWallet::from_json(&repeated.to_string())
    else {
        panic!("a repeated key is refused as a wallet document fault");
    };
    assert!(!reason.contains(key.as_str().unwrap()), "{reason}");
}

#[test]
fn a_bare_history_is_refused() {
    let bob = Wallet::new(2);
    let wallet = AnonymousWallet::new(fresh(&bob));
    let document: serde_json::Value = serde_json::from_str(&wallet.to_json()).unwrap();
    assert!(matches!(
        AnonymousWallet::from_json(&document["history"].to_string()),
        Err(AnonymousError::WalletDocument(_))
    ));
}

#[tokio::test]
async fn a_held_key_with_no_history_entry_refuses_the_scan() {
    let indexer = Indexer::default();
    let identity = keypair(2);
    let keys = RotatingRingKeys::new(&identity)
        .unwrap()
        .with_retired(vec![ViewingKey::new()])
        .unwrap();
    let history = ViewingHistory::new(identity.viewing_pubkey(), 0).with_ring(RING, None);
    assert!(matches!(
        AnonymousSync::new(&keys, &assets())
            .fetch(&indexer, &history)
            .await,
        Err(AnonymousError::HeldKeyNotInHistory)
    ));
}

/// One delivered payment of 5 to `owner` at slot 1, its mint named by the body alone.
fn unknown_mint_payment(owner: &Wallet, mint: Mint, blinding: [u8; 32]) -> ShieldedTransaction {
    let tx_key = ViewingKey::new();
    let salt = random_salt();
    let first_nullifier = field(9, blinding[31]);
    let view = owner.keys.viewing_pubkey();
    let secret = DeliveredSlot {
        agreement: &tx_key.ecdh(&view).unwrap(),
        tx_viewing_pk: tx_key.pubkey().as_bytes(),
        recipient_viewing_pk: view.as_bytes(),
        salt: &salt,
        output_index: 0,
        first_nullifier: &first_nullifier,
    }
    .secret()
    .unwrap();
    let address = owner.address();
    let hash = Utxo {
        owner: address.signing_pubkey,
        asset: mint,
        amount: 5,
        blinding,
        ring_program_id: Some(RING),
        data: Data::default(),
    }
    .hash(&address.nullifier_pubkey, &[0; 32], &[0; 32], TREE)
    .unwrap();
    let body = ring_body(
        &address,
        (5, mint.asset.to_bytes(), blinding),
        None,
        &secret,
    );
    ShieldedTransaction {
        slot: 1,
        tx_signature: Signature::from(signature(1)),
        event_index: Some(0),
        tx_viewing_pk: Some(tx_key.pubkey()),
        salt: Some(salt),
        output_slots: vec![OutputSlot {
            view_tag: bootstrap_slot_tag(view.u()),
            output_context: OutputContext {
                hash,
                tree_id: TREE,
                leaf_index: 0,
            },
            payload: encode_ring_body_output(&body, &[]).unwrap(),
        }],
        messages: Vec::new(),
        nullifiers: vec![first_nullifier],
        proofless: false,
        ring_config: None,
        ring_program_id: Some(RING),
    }
}

/// A merge of `nullifiers` into one SOL note of `owner` under its registered nullifier key.
fn publish_merge(
    indexer: &Indexer,
    owner: &Wallet,
    nullifiers: Vec<[u8; 32]>,
    amount: u64,
) -> [u8; 32] {
    let first = nullifiers[0];
    let address = owner.address();
    let nullifier_key = &owner.keys.nullifier_key;
    let merged = Utxo {
        owner: address.signing_pubkey,
        asset: Mint::SOL,
        amount,
        blinding: merge_output_blinding(nullifier_key, &first).unwrap(),
        ring_program_id: Some(RING),
        data: Data::default(),
    };
    let hash = merged
        .hash(&address.nullifier_pubkey, &[0; 32], &[0; 32], TREE)
        .unwrap();
    let salt = [8; 16];
    let ciphertext = MergeOutputPlaintext {
        amount,
        mint: Mint::SOL.asset,
    }
    .encrypt(&merge_output_key(nullifier_key, &first).unwrap(), &salt)
    .unwrap();
    let leaf = indexer.leaves(1);
    indexer.publish(ShieldedTransaction {
        slot: leaf + 1,
        tx_signature: Signature::from(signature(leaf)),
        event_index: Some(0),
        tx_viewing_pk: None,
        salt: None,
        output_slots: vec![OutputSlot {
            view_tag: first,
            output_context: OutputContext {
                hash,
                tree_id: TREE,
                leaf_index: leaf,
            },
            payload: encode_merge_output_data(MergeOutputDataRef {
                salt: &salt,
                ciphertext: &ciphertext,
                prefix: &[[0; 32]],
            }),
        }],
        messages: Vec::new(),
        nullifiers,
        proofless: false,
        ring_config: None,
        ring_program_id: Some(RING),
    });
    hash
}

#[tokio::test]
async fn an_extended_recipient_body_rebuilds_its_commitment_with_its_data() {
    let indexer = Indexer::default();
    let bob = Wallet::new(2);
    let tx_key = ViewingKey::new();
    let salt = random_salt();
    let first_nullifier = field(9, 9);
    let bob_view = bob.keys.viewing_pubkey();
    let secret = DeliveredSlot {
        agreement: &tx_key.ecdh(&bob_view).unwrap(),
        tx_viewing_pk: tx_key.pubkey().as_bytes(),
        recipient_viewing_pk: bob_view.as_bytes(),
        salt: &salt,
        output_index: 0,
        first_nullifier: &first_nullifier,
    }
    .secret()
    .unwrap();
    let (blinding, data_hash, ring_data_hash) = (field(9, 7), field(9, 6), field(9, 5));
    let note = Utxo {
        owner: bob.address().signing_pubkey,
        asset: Mint::SOL,
        amount: 5,
        blinding,
        ring_program_id: Some(RING),
        data: Data::default(),
    };
    let hash = note
        .hash(
            &bob.address().nullifier_pubkey,
            &data_hash,
            &ring_data_hash,
            TREE,
        )
        .unwrap();
    let body = ring_body(
        &bob.address(),
        (5, Mint::SOL.asset.to_bytes(), blinding),
        Some(NoteData {
            data_hash,
            ring_data_hash,
            application: b"memo".to_vec(),
        }),
        &secret,
    );
    assert!(body.len() > RING_RECORD_LEN);
    indexer.publish(ShieldedTransaction {
        slot: 1,
        tx_signature: Signature::from([0x78; 64]),
        event_index: Some(0),
        tx_viewing_pk: Some(tx_key.pubkey()),
        salt: Some(salt),
        output_slots: vec![OutputSlot {
            view_tag: bootstrap_slot_tag(bob_view.u()),
            output_context: OutputContext {
                hash,
                tree_id: TREE,
                leaf_index: indexer.leaves(1),
            },
            payload: encode_ring_body_output(&body, &[]).unwrap(),
        }],
        messages: Vec::new(),
        nullifiers: vec![first_nullifier],
        proofless: false,
        ring_config: None,
        ring_program_id: Some(RING),
    });
    let scan = AnonymousSync::new(&bob.keys, &assets())
        .full_scan(&indexer, &fresh(&bob), RING)
        .await
        .unwrap();
    assert_eq!(found(&scan), [hash]);
    let opened = &scan.decrypted().utxos[0];
    assert_eq!(opened.data_hash, Some(data_hash));
    assert_eq!(opened.ring_data_hash, Some(ring_data_hash));
    assert_eq!(
        opened.utxo.data.utxo_data().map(<[u8]>::to_vec),
        Some(b"memo".to_vec())
    );
}

/// Publishes one confidential payment, `published` edits its recipient slot before the indexer serves it.
fn deliver(
    sender: &mut Wallet,
    indexer: &Indexer,
    to: &ShieldedAddress,
    published: impl FnOnce(&mut OutputSlot),
) -> ShieldedTransaction {
    let input = sender.funding.remove(0);
    let mut transaction = ConfidentialTransaction::new_with_ring(
        vec![input],
        sender.address().solana_address().unwrap(),
        RING,
    )
    .unwrap()
    .with_output_tree_id(TREE)
    .unwrap();
    transaction
        .transfer_with_ring(to, Mint::SOL, 30, Some(RING))
        .unwrap();
    let rail = RingOutputRail::new(ViewingKey::new(), audit_key(), RingDelivery::Delivered);
    let proof = rail.encrypt(transaction, &sender.address()).unwrap();
    let external = &proof.external_data;
    let leaf = indexer.leaves(external.outputs.len());
    let position = external
        .resolved_owner_tags
        .iter()
        .position(|tag| *tag == custom_ring_interface::stream::slot_tag(to.viewing_pubkey.u()))
        .unwrap();
    let mut output_slots: Vec<OutputSlot> = external
        .outputs
        .iter()
        .zip(&external.resolved_owner_tags)
        .zip(leaf..)
        .map(|((output, tag), leaf_index)| OutputSlot {
            view_tag: *tag,
            output_context: OutputContext {
                hash: output.utxo_hash,
                tree_id: TREE,
                leaf_index,
            },
            payload: output.data.clone().unwrap_or_default(),
        })
        .collect();
    published(&mut output_slots[position]);
    let tx = ShieldedTransaction {
        slot: leaf + 1,
        tx_signature: Signature::from(signature(leaf)),
        event_index: Some(0),
        tx_viewing_pk: Some(rail.tx_viewing_key().pubkey()),
        salt: Some(rail.salt()),
        output_slots,
        messages: Vec::new(),
        nullifiers: vec![proof.first_nullifier().unwrap()],
        proofless: false,
        ring_config: None,
        ring_program_id: Some(RING),
    };
    indexer.publish(tx.clone());
    tx
}

#[test]
fn a_delivered_note_counts_once_core_rebuilds_its_commitment() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let tx = deliver(&mut alice, &indexer, &bob.address(), |_| {});
    let assets = assets();
    let codec = RingNoteCodec {
        ring: RING,
        keys: &bob.keys,
        assets: &assets,
    };
    let mut decrypted = DecryptionResult::default();
    decrypted
        .extend_with_ring(&bob.keys, &[tx], assets.registry(), &codec)
        .unwrap();
    let spendable = verify_spendable(&bob.keys, &decrypted).unwrap();
    let ring = spendable.ring_balances().remove(&RING).unwrap();
    assert_eq!(ring.get_balance(Mint::SOL.asset).unwrap().amount, 30);
}

#[test]
fn one_codec_opens_an_anonymous_payment_and_the_senders_change_through_core() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let carol = Wallet::new(3);
    let paid = alice.pay(&indexer, &[pay(&bob.address(), 30)]);
    let assets = assets();
    let credited = |wallet: &Wallet| {
        let codec = RingNoteCodec {
            ring: RING,
            keys: &wallet.keys,
            assets: &assets,
        };
        let mut decrypted = DecryptionResult::default();
        decrypted
            .extend_with_ring(
                &wallet.keys,
                core::slice::from_ref(&paid),
                assets.registry(),
                &codec,
            )
            .unwrap();
        let ring = verify_spendable(&wallet.keys, &decrypted)
            .unwrap()
            .ring_balances();
        ring.get(&RING)
            .and_then(|balances| balances.get_balance(Mint::SOL.asset))
            .map(|balance| balance.amount)
    };
    assert_eq!(credited(&bob), Some(30));
    assert_eq!(credited(&alice), Some(70));
    assert_eq!(credited(&carol), None);
}

#[tokio::test]
async fn a_full_scan_finds_a_confidential_note_under_a_wrong_tag() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let mut hash = [0; 32];
    deliver(&mut alice, &indexer, &bob.address(), |slot| {
        slot.view_tag = [0x55; 32];
        hash = slot.output_context.hash;
    });
    let recovered = RingConfidentialScan::new(&bob.keys, &assets())
        .full_scan(&indexer, RING)
        .await
        .unwrap();
    assert_eq!(recovered.owned.owned.utxos.len(), 1);
    assert_eq!(recovered.owned.owned.utxos[0].utxo_hash, hash);
    assert!(!recovered.auditor_recovery.unopened.contains(&hash));
}

#[tokio::test]
async fn a_bad_body_opens_nothing_and_a_foreign_slot_stays_a_miss_under_auditor_recovery() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let carol = Wallet::new(3);
    let mut bad = [0; 32];
    deliver(&mut alice, &indexer, &bob.address(), |slot| {
        *slot.payload.last_mut().unwrap() ^= 1;
        bad = slot.output_context.hash;
    });
    let mut foreign = [0; 32];
    deliver(&mut alice, &indexer, &carol.address(), |slot| {
        foreign = slot.output_context.hash;
    });
    let scan = RingConfidentialScan::new(&bob.keys, &assets())
        .full_scan(&indexer, RING)
        .await
        .unwrap();
    assert!(scan.owned.owned.utxos.is_empty());
    assert!(scan.auditor_recovery.unopened.contains(&bad));
    assert!(scan.auditor_recovery.unopened.contains(&foreign));
}

#[tokio::test]
async fn bounded_recovery_resumes_after_restart_at_its_fixed_target() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    for _ in 0..4 {
        alice.pay(&indexer, &[pay(&bob.address(), 5)]);
    }
    *indexer.page_cap.lock().unwrap() = Some(1);
    let mut wallet = AnonymousWallet::new(fresh(&bob));
    let initial = AnonymousSync::new(&bob.keys, &assets())
        .with_page_budget(1)
        .unwrap()
        .with_full_scan(RING)
        .fetch_wallet(&indexer, &wallet)
        .await
        .unwrap();
    let target = initial.target_slot().unwrap();
    assert_eq!(initial.status(), zolana_ring_client::RecoveryStatus::Paused);
    wallet.absorb(initial).unwrap();
    assert_eq!(
        wallet.recovery_status(),
        zolana_ring_client::RecoveryStatus::Paused
    );
    alice.pay(&indexer, &[pay(&bob.address(), 7)]);
    for _ in 0..32 {
        wallet = AnonymousWallet::from_json(&wallet.to_json()).unwrap();
        let step = AnonymousSync::new(&bob.keys, &assets())
            .with_page_budget(1)
            .unwrap()
            .with_full_scan(RING)
            .fetch_wallet(&indexer, &wallet)
            .await
            .unwrap();
        assert_eq!(step.target_slot(), Some(target));
        let complete = step.status() == zolana_ring_client::RecoveryStatus::Complete;
        wallet.absorb(step).unwrap();
        if complete {
            break;
        }
    }
    assert_eq!(
        wallet.recovery_status(),
        zolana_ring_client::RecoveryStatus::Complete
    );
    assert_eq!(wallet.notes().len(), 4);
    assert_eq!(after_slot(wallet.history()), Some(target));
}

#[tokio::test]
async fn a_resumed_recovery_refuses_another_owner_key_set_ring_mode_or_starting_history() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    for _ in 0..2 {
        alice.pay(&indexer, &[pay(&bob.address(), 5)]);
    }
    *indexer.page_cap.lock().unwrap() = Some(1);
    let assets = assets();
    let (both_keys, history) = wiped(2, &[ViewingKey::new()]);
    let first_key = RotatingRingKeys::new(&keypair(2)).unwrap();
    let step = |keys| {
        AnonymousSync::new(keys, &assets)
            .with_page_budget(1)
            .unwrap()
    };
    let mut wallet = AnonymousWallet::new(history);
    let paused = step(&first_key)
        .with_full_scan(RING)
        .fetch_wallet(&indexer, &wallet)
        .await
        .unwrap();
    assert_eq!(paused.status(), zolana_ring_client::RecoveryStatus::Paused);
    wallet.absorb(paused).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&wallet.to_json()).unwrap();
    let mut other_owner = stored.clone();
    other_owner["recovery"]["scope"]["owner"] = Wallet::new(3).address().to_string().into();
    let mut moved_history = stored.clone();
    moved_history["history"]["viewingHistory"][0]["request"]["next"] = 1.into();
    for edited in [other_owner, moved_history] {
        let edited = AnonymousWallet::from_json(&edited.to_string()).unwrap();
        assert!(matches!(
            step(&first_key)
                .with_full_scan(RING)
                .fetch_wallet(&indexer, &edited)
                .await,
            Err(AnonymousError::RecoveryScopeChanged)
        ));
    }
    for resumed in [
        step(&both_keys).with_full_scan(RING),
        step(&first_key),
        step(&first_key).with_full_scan(Address::new_from_array([5; 32])),
        step(&first_key).with_full_scan(RING).with_full_rescan(),
    ] {
        assert!(matches!(
            resumed.fetch_wallet(&indexer, &wallet).await,
            Err(AnonymousError::RecoveryScopeChanged)
        ));
    }
    assert!(step(&first_key)
        .with_full_scan(RING)
        .fetch_wallet(&indexer, &wallet)
        .await
        .is_ok());
}

#[tokio::test]
async fn a_tag_sync_holds_no_note_whose_published_commitment_was_replaced() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    let mut replaced = alice.seal(&indexer, &[pay(&bob.address(), 30)]);
    replaced.output_slots[1].output_context.hash = field(8, 8);
    indexer.publish(replaced);
    let mut stored = AnonymousWallet::new(ViewingHistory::new(bob.keys.viewing_pubkey(), 0));
    tag_sync(&indexer, &bob.keys, &mut stored).await;
    assert!(held(&stored).is_empty() && stored.pending_notes().is_empty());
    let paid = alice.pay(&indexer, &[pay(&bob.address(), 30)]);
    tag_sync(&indexer, &bob.keys, &mut stored).await;
    assert_eq!(held(&stored), [output(&paid, 1)]);
}

#[tokio::test]
async fn every_site_recovery_opens_no_foreign_or_changed_merge() {
    let indexer = Indexer::default();
    let (bob, carol) = (Wallet::new(2), Wallet::new(3));
    publish_merge(&indexer, &carol, vec![field(3, 1)], 100);
    publish_merge(&indexer, &bob, vec![field(2, 1)], 100);
    indexer
        .transactions
        .lock()
        .unwrap()
        .last_mut()
        .unwrap()
        .output_slots[0]
        .output_context
        .hash = field(8, 8);
    let opened = publish_merge(&indexer, &bob, vec![field(2, 2)], 100);
    let scan = AnonymousSync::new(&bob.keys, &assets())
        .full_scan(&indexer, &fresh(&bob), RING)
        .await
        .unwrap();
    assert_eq!(found(&scan), [opened]);
    assert!(scan.pending_notes().is_empty());
}

#[test]
fn a_pending_claim_prints_no_claimed_key() {
    let claimed_viewing_key = ViewingKey::new().pubkey();
    let printed = format!(
        "{:?}",
        PendingClaim {
            utxo_hash: [7; 32],
            claimed_viewing_key,
            claimed_owner_key: [0xa1; 32],
            claimed_nullifier_pk: [0xb2; 32],
        }
    );
    assert!(printed.contains(&format!("{:?}", [7u8; 32])), "{printed}");
    for hidden in [
        format!("{:?}", claimed_viewing_key.as_bytes()),
        "161".to_owned(),
        "178".to_owned(),
    ] {
        assert!(!printed.contains(&hidden), "{printed}");
    }
}

#[tokio::test]
async fn a_persisted_pending_note_keeps_a_later_spend_after_registration() {
    let indexer = Indexer::default();
    let bob = Wallet::new(2);
    let unknown = Mint::new(Address::new_from_array([0xee; 32]), 9);
    let blinding = field(7, 6);
    let payment = unknown_mint_payment(&bob, unknown, blinding);
    let hash = payment.output_slots[0].output_context.hash;
    let nullifier = bob.keys.nullifier(&hash, &blinding).unwrap();
    indexer.publish(payment);
    let mut wallet = AnonymousWallet::new(fresh(&bob));
    let initial = AnonymousSync::new(&bob.keys, &assets())
        .fetch_wallet(&indexer, &wallet)
        .await
        .unwrap();
    wallet.absorb(initial).unwrap();
    assert_eq!(wallet.pending_notes().len(), 1);
    wallet = AnonymousWallet::from_json(&wallet.to_json()).unwrap();
    indexer.publish(ShieldedTransaction {
        slot: 20,
        tx_signature: Signature::from(signature(20)),
        event_index: Some(0),
        tx_viewing_pk: None,
        salt: None,
        output_slots: Vec::new(),
        messages: Vec::new(),
        nullifiers: vec![nullifier],
        proofless: false,
        ring_config: None,
        ring_program_id: Some(RING),
    });
    let updated = AnonymousSync::new(&bob.keys, &assets())
        .fetch_wallet(&indexer, &wallet)
        .await
        .unwrap();
    wallet.absorb(updated).unwrap();
    wallet.resolve_pending(&RingAssets::new([unknown]).unwrap());
    let note = wallet
        .notes()
        .iter()
        .find(|note| note.utxo_hash == hash)
        .unwrap();
    assert_eq!(wallet.note_state(note), NoteState::Spent);
}

#[tokio::test]
async fn same_slot_spends_before_creation_survive_page_boundaries() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    let bob = Wallet::new(2);
    alice.pay(&indexer, &[pay(&bob.address(), 30)]);
    let initial = bob.scan_from(&indexer, &fresh(&bob)).await;
    let note = initial.decrypted().utxos[0].clone();
    let merged = publish_merge(&indexer, &bob, vec![note.nullifier], 30);
    for row in indexer.transactions.lock().unwrap().iter_mut() {
        row.slot = 10;
        row.tx_signature = if row
            .output_slots
            .iter()
            .any(|slot| slot.output_context.hash == merged)
        {
            Signature::from([1; 64])
        } else {
            Signature::from([2; 64])
        };
    }
    *indexer.tip.lock().unwrap() = 10;
    *indexer.page_cap.lock().unwrap() = Some(1);
    let mut recovered = AnonymousWallet::new(fresh(&bob));
    let scan = AnonymousSync::new(&bob.keys, &assets())
        .with_full_scan(RING)
        .fetch_wallet(&indexer, &recovered)
        .await
        .unwrap();
    recovered.absorb(scan).unwrap();
    assert_eq!(recovered.note_state(&note), NoteState::Spent);
    assert!(recovered
        .notes()
        .iter()
        .any(|note| note.utxo_hash == merged));
}

#[tokio::test]
async fn keyed_unknown_mint_merge_promotes_without_history_replay() {
    let indexer = Indexer::default();
    let bob = Wallet::new(2);
    let first = field(2, 8);
    let unknown = Mint::new(Address::new_from_array([0xed; 32]), 8);
    publish_merge(&indexer, &bob, vec![first], 100);
    let utxo = Utxo {
        owner: bob.address().signing_pubkey,
        asset: unknown,
        amount: 100,
        blinding: merge_output_blinding(&bob.keys.nullifier_key, &first).unwrap(),
        ring_program_id: Some(RING),
        data: Data::default(),
    };
    let hash = utxo
        .hash(&bob.address().nullifier_pubkey, &[0; 32], &[0; 32], TREE)
        .unwrap();
    let salt = [8; 16];
    let ciphertext = MergeOutputPlaintext {
        amount: 100,
        mint: unknown.asset,
    }
    .encrypt(
        &merge_output_key(&bob.keys.nullifier_key, &first).unwrap(),
        &salt,
    )
    .unwrap();
    {
        let mut rows = indexer.transactions.lock().unwrap();
        let row = rows.last_mut().unwrap();
        row.output_slots[0].output_context.hash = hash;
        row.output_slots[0].payload = encode_merge_output_data(MergeOutputDataRef {
            salt: &salt,
            ciphertext: &ciphertext,
            prefix: &[[0; 32]],
        });
    }
    let mut wallet = AnonymousWallet::new(fresh(&bob));
    let scan = AnonymousSync::new(&bob.keys, &assets())
        .with_full_scan(RING)
        .fetch_wallet(&indexer, &wallet)
        .await
        .unwrap();
    assert_eq!(scan.pending_notes().len(), 1);
    assert!(scan.decrypted().utxos.is_empty());
    wallet.absorb(scan).unwrap();
    let before = wallet.history().clone();
    wallet = AnonymousWallet::from_json(&wallet.to_json()).unwrap();
    wallet.resolve_pending(&RingAssets::new([unknown]).unwrap());
    assert_eq!(wallet.notes()[0].utxo_hash, hash);
    assert_eq!(wallet.history(), &before);
}

#[tokio::test]
async fn canonical_scan_refuses_a_stale_wallet_revision() {
    let indexer = Indexer::default();
    let bob = Wallet::new(2);
    let unknown = Mint::new(Address::new_from_array([0xec; 32]), 8);
    indexer.publish(unknown_mint_payment(&bob, unknown, field(4, 3)));
    let mut wallet = AnonymousWallet::new(fresh(&bob));
    let initial = AnonymousSync::new(&bob.keys, &assets())
        .fetch_wallet(&indexer, &wallet)
        .await
        .unwrap();
    wallet.absorb(initial).unwrap();
    assert_eq!(wallet.pending_notes().len(), 1);
    let scan = AnonymousSync::new(&bob.keys, &assets())
        .fetch_wallet(&indexer, &wallet)
        .await
        .unwrap();
    let before = wallet.to_json();
    wallet.resolve_pending(&assets());
    wallet.update_history(wallet.history().clone()).unwrap();
    assert_eq!(wallet.to_json(), before);
    let history = wallet.history().clone();
    wallet.resolve_pending(&RingAssets::new([unknown]).unwrap());
    assert_eq!(wallet.history(), &history);
    assert!(wallet.pending_notes().is_empty());
    assert!(matches!(
        wallet.absorb(scan),
        Err(AnonymousError::ScanHistoryChanged)
    ));
}

#[tokio::test]
async fn recovery_diagnostics_keep_a_bounded_sample_across_restarts() {
    let indexer = Indexer::default();
    let mut alice = Wallet::new(1);
    alice.funding = (0..80)
        .map(|index| funding_utxo(&alice.keys, 1, index))
        .collect();
    let bob = Wallet::new(2);
    let carol = Wallet::new(3);
    for _ in 0..80 {
        alice.pay(&indexer, &[pay(&carol.address(), 1)]);
    }
    *indexer.page_cap.lock().unwrap() = Some(10);
    let mut wallet = AnonymousWallet::new(fresh(&bob));
    let mut count = 0;
    for _ in 0..16 {
        let scan = AnonymousSync::new(&bob.keys, &assets())
            .with_page_budget(2)
            .unwrap()
            .with_full_scan(RING)
            .fetch_wallet(&indexer, &wallet)
            .await
            .unwrap();
        count = scan.auditor_recovery.unopened_count;
        assert!(scan.auditor_recovery.unopened.len() <= 64);
        let complete = scan.status() == zolana_ring_client::RecoveryStatus::Complete;
        wallet.absorb(scan).unwrap();
        wallet = AnonymousWallet::from_json(&wallet.to_json()).unwrap();
        assert!(wallet.notes().is_empty());
        assert!(wallet.decrypted().spent_nullifiers.is_empty());
        if complete {
            break;
        }
    }
    assert_eq!(
        wallet.recovery_status(),
        zolana_ring_client::RecoveryStatus::Complete
    );
    assert!(count >= 80);
}
