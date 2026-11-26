//! `WalletScan` against an in-memory indexer.

use std::sync::Mutex;

use crate::ZolanaServices;
use custom_ring_interface::{RecordForm, RecordKind};
use solana_address::Address;
use solana_signature::Signature;
use spr_client::{
    rpc::GetShieldedTransactionsByNullifiersResponse, AsyncProverClient, ClientError,
    Context as Page, GetEncryptedUtxosByTagsResponse, GetShieldedTransactionsByTagsResponse,
    HistoryFilters, OutputContext, OutputSlot, PaginationToken, RingHistoryOptions,
    ShieldedTransaction, WitnessReader,
};
use spr_keypair::{ShieldedAddress, ShieldedKeypair};
use spr_transaction::{
    utxo::blinding::derive_transact_output_blinding, Data, Mint, Utxo, WalletUtxo,
};
use zolana_ring_keys::bootstrap_slot_tag;
use zolana_ring_keys::encode_ring_body_output;
use zolana_ring_keys::DeliveredSlot;
use zolana_ring_keys::OutputRecord;
use zolana_ring_transaction::{record_key, RecipientBody};

use super::{
    fixtures::{keypair, Member, RING},
    *,
};

#[path = "../../../../sdk-libs/client/tests/common/history_page.rs"]
mod history_page;

use history_page::EventsPage;

const TREE: u16 = 2;
const SALT: [u8; 16] = [7; 16];
const SEED: [u8; 32] = [0x0b; 32];
const CAUGHT_UP: Page = Page {
    block_time: 0,
    slot: u64::MAX,
};

/// Serves what the asked tags or nullifiers reach, in one page.
#[derive(Default)]
struct Ledger {
    transactions: Vec<ShieldedTransaction>,
    queried: Mutex<Vec<[u8; 32]>>,
    mints: Vec<Mint>,
}

fn slot_secret(
    tx: &spr_keypair::ViewingKey,
    owner: &ShieldedAddress,
    first_nullifier: &[u8; 32],
    output_index: u32,
) -> zolana_ring_keys::SlotSecret {
    DeliveredSlot {
        agreement: &tx.ecdh(&owner.viewing_pubkey).unwrap(),
        tx_viewing_pk: tx.pubkey().as_bytes(),
        recipient_viewing_pk: owner.viewing_pubkey.as_bytes(),
        salt: &SALT,
        output_index,
        first_nullifier,
    }
    .secret()
    .unwrap()
}

/// The ring body of `sealed` at slot 0, its record alone.
fn ring_body(
    sealed: &Sealed<'_>,
    owner: &ShieldedAddress,
    blinding: [u8; 32],
    secret: &zolana_ring_keys::SlotSecret,
) -> Vec<u8> {
    let record = OutputRecord {
        kind: RecordKind::Ring,
        amount: sealed.amount,
        owner_key: owner.signing_pubkey.confidential_view_tag().unwrap(),
        mint: sealed.asset.asset.to_bytes(),
        blinding,
        key: record_key(RecordKind::Ring, owner.nullifier_pubkey),
        ..OutputRecord::default()
    };
    let ciphertext = RecipientBody {
        record: &record,
        form: RecordForm::default(),
        payload: None,
    }
    .seal(secret)
    .unwrap();
    encode_ring_body_output(&ciphertext, &[]).unwrap()
}

/// One ring output, sealed at slot 0 as either rail seals it.
struct Sealed<'a> {
    sender: &'a ShieldedKeypair,
    owner: &'a ShieldedKeypair,
    first_nullifier: [u8; 32],
    asset: Mint,
    amount: u64,
    leaf: u64,
}

impl Ledger {
    /// A payment from `sealed.sender` to `sealed.owner`, its ring body at output 0.
    fn payment(&mut self, sealed: Sealed<'_>) -> [u8; 32] {
        let owner = sealed.owner.shielded_address().unwrap();
        let tx = sealed
            .sender
            .viewing_key
            .get_transaction_viewing_key(&sealed.first_nullifier)
            .unwrap();
        let blinding = derive_transact_output_blinding(&sealed.first_nullifier, &SEED, 0).unwrap();
        let secret = slot_secret(&tx, &owner, &sealed.first_nullifier, 0);
        let payload = ring_body(&sealed, &owner, blinding, &secret);
        let hash = Utxo {
            owner: owner.signing_pubkey,
            asset: sealed.asset,
            amount: sealed.amount,
            blinding,
            ring_program_id: Some(RING),
            data: Data::default(),
        }
        .hash(&owner.nullifier_pubkey, &[0; 32], &[0; 32], TREE)
        .unwrap();
        self.transactions.push(ShieldedTransaction {
            slot: 5,
            tx_signature: Signature::from([sealed.leaf as u8 + 1; 64]),
            event_index: Some(0),
            tx_viewing_pk: Some(tx.pubkey()),
            salt: Some(SALT),
            output_slots: vec![OutputSlot {
                view_tag: bootstrap_slot_tag(owner.viewing_pubkey.u()),
                output_context: OutputContext {
                    hash,
                    tree_id: TREE,
                    leaf_index: sealed.leaf,
                },
                payload,
            }],
            messages: Vec::new(),
            nullifiers: vec![sealed.first_nullifier],
            proofless: false,
            ring_config: None,
            ring_program_id: Some(RING),
        });
        hash
    }

    /// A ring spend of `nullifier` with no output of the wallet.
    fn spend(&mut self, nullifier: [u8; 32]) {
        self.transactions.push(ShieldedTransaction {
            slot: 6,
            tx_signature: Signature::from([0xee; 64]),
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
    }

    fn client(self) -> Zolana<NoRpc, Self> {
        ZolanaServices {
            rpc: NoRpc,
            indexer: self,
            prover: AsyncProverClient::local(),
            ring_prover: AsyncProverClient::local(),
        }
        .connect()
    }
}

impl Ledger {
    fn page(
        &self,
        filters: HistoryFilters,
        matches: impl Fn(&ShieldedTransaction) -> bool,
    ) -> GetShieldedTransactionsByTagsResponse {
        EventsPage {
            context: CAUGHT_UP,
            output_tree_id: Some(TREE),
            rows: self
                .transactions
                .iter()
                .filter(|tx| filters.slot.contains(tx.slot) && matches(tx))
                .cloned()
                .collect(),
            more: false,
        }
        .response()
    }
}

impl WitnessReader for Ledger {}

impl Rpc for Ledger {
    fn get_shielded_transactions_by_ring(
        &self,
        options: RingHistoryOptions,
        _config: Option<IndexerRpcConfig>,
    ) -> Result<GetShieldedTransactionsByTagsResponse, ClientError> {
        Ok(self.page(options.filters, |tx| {
            tx.ring_program_id == Some(options.ring_program_id)
        }))
    }

    fn get_shielded_transactions_by_tags(
        &self,
        tags: Vec<[u8; 32]>,
        _pagination_token: Option<PaginationToken>,
        _limit: Option<u32>,
        filters: HistoryFilters,
        _config: Option<IndexerRpcConfig>,
    ) -> Result<GetShieldedTransactionsByTagsResponse, ClientError> {
        self.queried.lock().unwrap().extend(&tags);
        Ok(self.page(filters, |tx| {
            tx.output_slots
                .iter()
                .any(|slot| tags.contains(&slot.view_tag))
        }))
    }

    fn get_encrypted_utxos_by_tags(
        &self,
        _tags: Vec<[u8; 32]>,
        _pagination_token: Option<PaginationToken>,
        _limit: Option<u32>,
        _filters: HistoryFilters,
        _config: Option<IndexerRpcConfig>,
    ) -> Result<GetEncryptedUtxosByTagsResponse, ClientError> {
        Ok(GetEncryptedUtxosByTagsResponse {
            context: CAUGHT_UP,
            output_tree_id: Some(TREE),
            data: Vec::new(),
            pagination_token: None,
        })
    }

    fn get_shielded_transactions_by_nullifiers(
        &self,
        nullifiers: Vec<[u8; 32]>,
        _pagination_token: Option<PaginationToken>,
        _limit: Option<u32>,
        filters: HistoryFilters,
        _config: Option<IndexerRpcConfig>,
    ) -> Result<GetShieldedTransactionsByNullifiersResponse, ClientError> {
        Ok(self.page(filters, |tx| {
            tx.nullifiers.iter().any(|spent| nullifiers.contains(spent))
        }))
    }
}

struct NoRpc;

impl Rpc for NoRpc {}

impl Member {
    fn scan(&self, zolana: &Zolana<NoRpc, Ledger>) -> Vec<[u8; 32]> {
        self.scan_notes(zolana)
            .spendable
            .utxos()
            .map(|utxo| utxo.utxo_hash)
            .collect()
    }

    /// Registers nothing up front, the chain's assets name the ledger's mints.
    fn scan_notes(&self, zolana: &Zolana<NoRpc, Ledger>) -> RingWalletNotes {
        let assets = RingAssets::new([]).unwrap();
        let keys = zolana_ring_keys::RotatingRingKeys::new(&self.keys).unwrap();
        ScanAt {
            sync: RingWalletSync::new(&keys, &assets)
                .with_indexer_config(IndexerRpcConfig::at_slot(9))
                .with_chain_assets(&|| Ok(RingAssets::new(zolana.indexer().mints.clone())?)),
            store: &self.store,
            watch: None,
        }
        .fetch(zolana)
        .unwrap()
    }

    fn payment_from_2(&self, ledger: &mut Ledger, asset: Mint) -> [u8; 32] {
        ledger.payment(Sealed {
            sender: &keypair(2),
            owner: &self.keys,
            first_nullifier: [0x11; 32],
            asset,
            amount: 70,
            leaf: 10,
        })
    }
}

#[test]
fn one_sync_finds_a_ring_payment_and_the_next_drops_it_once_spent() {
    let wallet = Member::new(1, "spent-payment");
    let ring_notes = |ledger: Ledger| -> Vec<WalletUtxo> {
        let notes = wallet.scan_notes(&ledger.client());
        let ring = notes.spendable.ring_balances().remove(&RING);
        ring.into_iter()
            .flat_map(|balances| balances.assets)
            .flat_map(|balance| balance.utxos)
            .collect()
    };
    let mut ledger = Ledger::default();
    let paid = wallet.payment_from_2(&mut ledger, Mint::SOL);
    let found = ring_notes(ledger);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].utxo_hash, paid);
    let mut ledger = Ledger::default();
    wallet.payment_from_2(&mut ledger, Mint::SOL);
    ledger.spend(found[0].nullifier);
    assert!(ring_notes(ledger).is_empty());
}

#[test]
fn a_wallet_finds_a_payment_under_its_bootstrap_tag_and_resumes_past_it() {
    let wallet = Member::new(1, "bootstrap-scan");
    let mut ledger = Ledger::default();
    let paid = wallet.payment_from_2(&mut ledger, Mint::SOL);
    let zolana = ledger.client();
    assert_eq!(wallet.scan(&zolana), [paid]);
    let stored = wallet.store.load().unwrap();
    assert_eq!(stored.history().current().tag_position(), Some(u64::MAX));
    assert!(stored.history().current().ring_positions().is_empty());
    assert_eq!(
        wallet.scan(&zolana),
        [paid],
        "the ledger answers past its position"
    );
}

#[test]
fn a_note_in_a_mint_registered_after_the_registry_read_is_kept() {
    let wallet = Member::new(1, "late-mint");
    let mint = Mint {
        asset: Address::new_from_array([0x6d; 32]),
        asset_id: 9,
    };
    let mut ledger = Ledger {
        mints: vec![mint],
        ..Ledger::default()
    };
    let paid = wallet.payment_from_2(&mut ledger, mint);
    let zolana = ledger.client();
    assert_eq!(wallet.scan(&zolana), [paid]);
    assert!(wallet.store.load().unwrap().pending_notes().is_empty());
    assert_eq!(wallet.scan(&zolana), [paid], "a second scan");
}

#[test]
fn a_note_waits_pending_until_its_mint_registers() {
    let wallet = Member::new(1, "pending-mint");
    let mint = Mint {
        asset: Address::new_from_array([0x6d; 32]),
        asset_id: 9,
    };
    let chain = |mints| {
        let mut ledger = Ledger {
            mints,
            ..Ledger::default()
        };
        let paid = wallet.payment_from_2(&mut ledger, mint);
        (paid, ledger.client())
    };
    let (paid, unregistered) = chain(Vec::new());
    let synced = wallet.scan_notes(&unregistered);
    assert_eq!(synced.spendable.utxos().count(), 0);
    assert!(synced.spendable.unknown_mints.contains(&mint.asset));
    let pending = wallet.store.load().unwrap().pending_notes().to_vec();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].utxo_hash(), paid);

    let (_, registered) = chain(vec![mint]);
    assert_eq!(wallet.scan(&registered), [paid]);
    assert!(wallet.store.load().unwrap().pending_notes().is_empty());
}
