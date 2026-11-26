use std::sync::Mutex;

use sea_orm::{ConnectionTrait, Statement};
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use zolana_ring_policy::AssetLimits;

use super::cosign_window_tests::clear_usage;
use super::*;
use crate::cosign::{ApproveClass, CoSignDecision, CoSignPolicy, CoSignRequest};

const RING: Address = Address::new_from_array([9; 32]);
const ASSET: Address = Address::new_from_array([3; 32]);
const CAP: u64 = 100;
const NOTE: [u8; 32] = [0xa1; 32];
const OTHER_NOTE: [u8; 32] = [0xb2; 32];
const PADDING: [u8; 32] = [0xc3; 32];

/// The frame every message carries and what the ring history holds of it.
struct Chain {
    frame: Mutex<TransferRead>,
    history: Mutex<SpendRead>,
    cap: Mutex<u64>,
}

impl AssetLimitSource for Chain {
    async fn current_slot(&self) -> Result<u64, RingRpcError> {
        Ok(1_000)
    }

    async fn asset_policies(
        &self,
        _ring: Address,
        assets: &[Address],
    ) -> Result<Vec<Option<MintPolicy>>, RingRpcError> {
        let policy = MintPolicy {
            limits: AssetLimits {
                volume_cap: *self.cap.lock().expect("cap"),
                volume_window_slots: 10_000,
                ..AssetLimits::EMPTY
            },
            ..MintPolicy::EMPTY
        };
        Ok(vec![Some(policy); assets.len()])
    }

    async fn co_signer_scope(
        &self,
        _ring: Address,
        _signer: Address,
    ) -> Result<CoSignScope, RingRpcError> {
        Ok(CoSignScope::ALL)
    }

    async fn transfer_flows(
        &self,
        _ring: Address,
        _frame: FrameSource<'_>,
    ) -> Result<FrameRead, RingRpcError> {
        Ok(FrameRead::Transfer(
            self.frame.lock().expect("frame").clone(),
        ))
    }

    async fn spend_read(
        &self,
        _ring: Address,
        _nullifiers: &[[u8; 32]],
    ) -> Result<SpendRead, RingRpcError> {
        Ok(*self.history.lock().expect("history"))
    }
}

struct Wallet {
    co_signer: LimitedCoSigner<Chain>,
    store: Store,
}

fn transfer(owner: u8, amount: u64, nullifiers: &[[u8; 32]]) -> TransferRead {
    TransferRead {
        flows: vec![OwnerFlow {
            owner: Member::from_bytes([owner; 32]).expect("member"),
            flow: AssetFlow {
                asset: ASSET,
                debit: amount.into(),
                charged_outflow: amount.into(),
                deposited: 0,
                withdrawn: 0,
            },
        }],
        spends_note: true,
        approval_bits: None,
        co_signers: Vec::new(),
        nullifiers: nullifiers.to_vec(),
    }
}

const fn tip(slot: u64, block_height: u64) -> FinalizedTip {
    FinalizedTip { slot, block_height }
}

impl Wallet {
    async fn open(url: &str) -> Self {
        let store = Store::open(url, crate::database::STORE_CONNECTIONS)
            .await
            .expect("store");
        let co_signer = LimitedCoSignerInit {
            signer: Arc::new(Keypair::new_from_array([5; 32])),
            limits: Chain {
                frame: Mutex::new(transfer(7, CAP, &[NOTE])),
                history: Mutex::new(SpendRead {
                    spent: false,
                    history_through: 0,
                }),
                cap: Mutex::new(CAP),
            },
            store: Arc::new(store.clone()),
            approval: CoSignerApproval::new([ApproveClass::Transfers]),
        }
        .build();
        Self { co_signer, store }
    }

    fn carry(&self, frame: TransferRead) {
        *self.co_signer.rule.limits.frame.lock().expect("frame") = frame;
    }

    fn history(&self, spent: bool, history_through: u64) {
        *self.co_signer.rule.limits.history.lock().expect("history") = SpendRead {
            spent,
            history_through,
        };
    }

    /// One message per blockhash, valid up to `last_valid_block_height`.
    async fn send(
        &self,
        blockhash: u8,
        last_valid_block_height: u64,
    ) -> Result<CoSignDecision, RingRpcError> {
        self.co_signer
            .decide(&CoSignRequest {
                ring: RING,
                message: VersionedMessage::Legacy(Message {
                    recent_blockhash: solana_hash::Hash::new_from_array([blockhash; 32]),
                    ..Message::default()
                }),
                flows: CoSignFlows::Frame,
                approved_transfer: None,
                sender: Sender::Wallet {
                    last_valid_block_height,
                },
            })
            .await
    }

    async fn signs(&self, blockhash: u8, last_valid_block_height: u64) {
        let decision = self.send(blockhash, last_valid_block_height).await;
        assert!(
            matches!(decision, Ok(CoSignDecision::Signed(_))),
            "{decision:?}"
        );
    }

    async fn sweep(&self, tip: FinalizedTip) {
        self.co_signer.sweep(tip).await.expect("sweep");
    }

    async fn count(&self, sql: &str) -> i64 {
        let connection = self.store.connection();
        connection
            .query_one(Statement::from_string(
                connection.get_database_backend(),
                format!("SELECT COUNT(*) AS n FROM {sql}"),
            ))
            .await
            .expect("count")
            .expect("row")
            .try_get("", "n")
            .expect("n")
    }

    /// `[held, confirmed, released, tracked sends, tracked inputs]`.
    async fn rows(&self) -> [i64; 5] {
        [
            self.count("cosign_usage WHERE state='held'").await,
            self.count("cosign_usage WHERE state='confirmed'").await,
            self.count("cosign_usage WHERE state='released'").await,
            self.count("cosign_wallet_sends").await,
            self.count("cosign_wallet_inputs").await,
        ]
    }

    /// Another note's transfer of `amount`, refused while the cap is held.
    async fn other_transfer(&self, amount: u64) -> Result<CoSignDecision, RingRpcError> {
        self.carry(transfer(8, amount, &[OTHER_NOTE]));
        self.send(200, 10_000).await
    }
}

#[tokio::test]
async fn an_unlanded_send_is_released_once_the_history_covers_its_expiry() {
    let wallet = Wallet::open("sqlite::memory:").await;
    wallet.signs(1, 500).await;

    // 1. At the last valid height the message can still land.
    wallet.history(false, u64::MAX);
    wallet.sweep(tip(900, 500)).await;
    wallet.sweep(tip(901, 500)).await;
    assert_eq!(wallet.rows().await, [1, 0, 0, 1, 1]);
    assert_eq!(
        wallet.other_transfer(1).await.expect("decision"),
        CoSignDecision::Refused(CoSignRefusal::OverLimit)
    );

    // 2. Past it the first finalized slot is the expiry, a history one slot short proves nothing.
    wallet.history(false, 909);
    wallet.sweep(tip(910, 501)).await;
    wallet.sweep(tip(950, 540)).await;
    assert_eq!(wallet.rows().await, [1, 0, 0, 1, 1]);

    // 3. A history through the expiry slot shows no spend, the cap is free again.
    wallet.history(false, 910);
    wallet.sweep(tip(951, 541)).await;
    assert_eq!(wallet.rows().await, [0, 0, 1, 0, 0]);
    assert!(matches!(
        wallet.other_transfer(CAP).await,
        Ok(CoSignDecision::Signed(_))
    ));

    // 4. The released message never reserves again.
    wallet.carry(transfer(7, CAP, &[NOTE]));
    assert!(matches!(
        wallet.send(1, 500).await,
        Err(RingRpcError::CoSign(CoSignError::ReleasedReservation))
    ));
}

#[tokio::test]
async fn a_landed_send_is_confirmed_and_counts_past_its_expiry() {
    let wallet = Wallet::open("sqlite::memory:").await;
    wallet.signs(1, 500).await;
    // The expiry is read first, the history shows the spend only later.
    wallet.history(false, 0);
    wallet.sweep(tip(910, 501)).await;
    wallet.history(true, u64::MAX);
    wallet.sweep(tip(911, 502)).await;
    assert_eq!(wallet.rows().await, [0, 1, 0, 0, 0]);
    wallet.history(false, u64::MAX);
    wallet.sweep(tip(5_000, 4_000)).await;
    assert_eq!(wallet.rows().await, [0, 1, 0, 0, 0]);
    assert_eq!(
        wallet.other_transfer(1).await.expect("decision"),
        CoSignDecision::Refused(CoSignRefusal::OverLimit)
    );
}

#[tokio::test]
async fn one_transfer_under_a_second_blockhash_adds_no_usage() {
    let wallet = Wallet::open("sqlite::memory:").await;
    wallet.carry(transfer(7, 60, &[PADDING, NOTE]));
    wallet.signs(1, 700).await;
    // 1. The newer message takes the reservation, the first can land up to its own height.
    wallet.signs(2, 600).await;
    assert_eq!(wallet.rows().await, [1, 0, 1, 1, 2]);
    assert_eq!(
        wallet
            .count("cosign_wallet_sends WHERE last_valid_block_height=700")
            .await,
        1
    );

    // 2. A second proof of the note, with other padding or another amount, could land beside it.
    for reproved in [
        transfer(7, 60, &[NOTE, OTHER_NOTE]),
        transfer(7, 60, &[NOTE]),
        transfer(7, 59, &[PADDING, NOTE]),
        transfer(8, 60, &[PADDING, NOTE]),
    ] {
        wallet.carry(reproved);
        assert!(matches!(
            wallet.send(3, 800).await,
            Err(RingRpcError::CoSign(CoSignError::InputReserved))
        ));
    }
    assert_eq!(wallet.rows().await, [1, 0, 1, 1, 2]);

    // 3. One reservation of 60 is held, 40 more fits and 41 does not.
    assert_eq!(
        wallet.other_transfer(41).await.expect("decision"),
        CoSignDecision::Refused(CoSignRefusal::OverLimit)
    );
    assert!(matches!(
        wallet.other_transfer(40).await,
        Ok(CoSignDecision::Signed(_))
    ));
}

#[tokio::test]
async fn a_refused_request_holds_nothing() {
    let wallet = Wallet::open("sqlite::memory:").await;
    wallet.carry(transfer(7, CAP + 1, &[NOTE]));
    assert_eq!(
        wallet.send(1, 500).await.expect("decision"),
        CoSignDecision::Refused(CoSignRefusal::OverLimit)
    );
    assert_eq!(wallet.rows().await, [0, 0, 0, 0, 0]);

    // A takeover the lowered cap refuses leaves the earlier message its reservation.
    wallet.carry(transfer(7, 60, &[NOTE]));
    wallet.signs(2, 500).await;
    *wallet.co_signer.rule.limits.cap.lock().expect("cap") = 59;
    assert_eq!(
        wallet.send(3, 600).await.expect("decision"),
        CoSignDecision::Refused(CoSignRefusal::OverLimit)
    );
    assert_eq!(wallet.rows().await, [1, 0, 0, 1, 1]);
    assert_eq!(
        wallet
            .count("cosign_wallet_sends WHERE last_valid_block_height=500")
            .await,
        1
    );
}

async fn assert_concurrent_messages_of_one_transfer_hold_one_reservation(url: &str) {
    let wallets = [
        Arc::new(Wallet::open(url).await),
        Arc::new(Wallet::open(url).await),
    ];
    let mut sends = tokio::task::JoinSet::new();
    for blockhash in 0..24u8 {
        let wallet = wallets[usize::from(blockhash % 2)].clone();
        sends.spawn(async move { wallet.send(blockhash, 500 + u64::from(blockhash)).await });
    }
    while let Some(sent) = sends.join_next().await {
        let decision = sent.expect("task");
        assert!(
            matches!(decision, Ok(CoSignDecision::Signed(_))),
            "{decision:?}"
        );
    }
    let [held, confirmed, released, tracked, inputs] = wallets[0].rows().await;
    assert_eq!([held, confirmed, released], [1, 0, 23]);
    assert_eq!([tracked, inputs], [1, 1]);
    assert_eq!(
        wallets[0]
            .count("cosign_wallet_sends WHERE last_valid_block_height=523")
            .await,
        1,
        "the reservation outlives the newest message"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_messages_of_one_transfer_hold_one_reservation_on_sqlite() {
    let path = std::env::temp_dir().join(format!("cosign-sends-{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    assert_concurrent_messages_of_one_transfer_hold_one_reservation(&format!(
        "sqlite://{}?mode=rwc",
        path.display()
    ))
    .await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs RING_RPC_TEST_POSTGRES_URL"]
async fn concurrent_messages_of_one_transfer_hold_one_reservation_on_postgres() {
    let url = std::env::var("RING_RPC_TEST_POSTGRES_URL").expect("postgres url");
    let store = Store::open(&url, 1).await.expect("store");
    clear_usage(&store).await;
    assert_concurrent_messages_of_one_transfer_hold_one_reservation(&url).await;
}
