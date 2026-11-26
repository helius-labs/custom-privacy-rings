//! A wallet relays a sealed transfer, deposit or merge, Ring RPC proves, pays and sends it.

#[path = "relay_service/attempts.rs"]
mod attempts;
#[path = "common/relay.rs"]
mod relay_fixture;
#[path = "common/store.rs"]
mod store_fixture;
#[path = "relay_service/transitions.rs"]
mod transitions;
#[path = "common/wide_frame.rs"]
mod wide_frame;

use std::sync::{atomic::Ordering, Arc, LazyLock};

use async_trait::async_trait;
use jsonrpsee::RpcModule;
use sea_orm::{ConnectionTrait, Statement};
use solana_address::Address;
use solana_keypair::Keypair;
use solana_signature::Signature;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
use spr_client::{ClientError, GetShieldedTransactionsByTagsResponse};
use spr_instruction::instruction::{
    CreateAssociatedTokenAccount, TransactInterfaceTransferAccounts, TransactSolTransferAccounts,
    TransactSplWithdrawalAccounts,
};
use spr_interface::SPL_TOKEN_PROGRAM_ID;
use spr_keypair::{NullifierKey, X25519Keypair, X25519Pubkey};
use spr_transaction::{
    instructions::transact::SettlementTransfer, utxo::SppProofInputUtxo, WalletUtxo,
};
use zolana_ring_client::{
    rpc::{
        GetRelayQuoteRequest, GetRelayStatusRequest, PrepareRelayRequest, ProofInputBytes,
        RelayIntent, RelayMergeInputs, RelayOperation, RelayProofInputs, RelayQuote, RelayRail,
        RelayRequestAttestation, RelayStatus, RelayStep, RingChainStatus, ServiceTarget,
        SubmitRelayRequest, TokenAccountSetup, GET_RELAY_QUOTE, GET_RELAY_STATUS, PREPARE_RELAY,
        SUBMIT_RELAY,
    },
    AuditError, RingIndexerError, RingOrigin,
};
use zolana_ring_rpc::{
    rpc_module, ChainOutcome, Hub, MergeAuditRequest, Migrator, RecordsTip, Relay, RelayBudget,
    RelayError, RelayParts, RingConfiguration, RingFrame, RingHistoryPage, RingRpcError,
    ServedRings, ServiceError, ServiceIdentity, Store, TransactionPage, TransactionSource,
    ABANDONED_AFTER_SLOTS, STORE_CONNECTIONS,
};
use zolana_ring_transaction::RingAssets;

use wide_frame::wide_frame;

use relay_fixture::{
    anonymous_rail, deposit_request, owner_wallet, proof_inputs, signed_request, signers_verify,
    RecordingChain, RecordingProver, LAST_VALID_BLOCK_HEIGHT, RENT_LAMPORTS, SLOT, TREE_FEES,
};

const RING: Address = Address::new_from_array([6; 32]);
/// The relay under test runs as this service.
static SERVICE: LazyLock<ServiceTarget> = LazyLock::new(|| ServiceTarget {
    cluster: [61; 32],
    service: service_key().pubkey(),
});

fn service_key() -> Keypair {
    Keypair::new_from_array([62; 32])
}

/// Serves `RING` alone, a relay reads only its configuration.
struct RingSource {
    auditor: X25519Pubkey,
}

fn unused<T>() -> Result<T, ClientError> {
    Err(ClientError::Rpc("unused source".to_owned()))
}

impl TransactionSource for RingSource {
    async fn is_mint_delegate(
        &self,
        _mint: Address,
        _signer: Address,
    ) -> Result<bool, ClientError> {
        Ok(false)
    }

    async fn transactions_by_tag(
        &self,
        _request: TransactionPage,
    ) -> Result<GetShieldedTransactionsByTagsResponse, ClientError> {
        unused()
    }

    async fn ring_frame(
        &self,
        _frame: RingFrame,
    ) -> Result<Vec<spr_transaction::ShieldedTransaction>, ClientError> {
        unused()
    }

    async fn ring_history(
        &self,
        _request: RingHistoryPage,
    ) -> Result<GetShieldedTransactionsByTagsResponse, ClientError> {
        unused()
    }

    async fn merge_audit_page(
        &self,
        _request: MergeAuditRequest<'_>,
    ) -> Result<zolana_ring_client::MergeAuditPage, AuditError> {
        Err(AuditError::InvalidMemberRegistry)
    }

    async fn transaction_origin(
        &self,
        signature: Signature,
        _event_index: u16,
        _ring: Address,
    ) -> Result<RingOrigin, AuditError> {
        Err(AuditError::Unavailable {
            signature,
            message: "unused source".to_owned(),
        })
    }

    async fn ring_config(&self, ring: Address) -> Result<Option<RingConfiguration>, ClientError> {
        Ok((ring == RING).then(|| RingConfiguration {
            auditor_pubkey: self.auditor,
            mode: custom_ring_interface::RingMode::Confidential,
            policy: custom_ring_interface::RingPolicy {
                delegate: ring_delegate().pubkey(),
                ..bytemuck::Zeroable::zeroed()
            },
        }))
    }

    async fn ring_chain_status(&self, _ring: Address) -> Result<RingChainStatus, ClientError> {
        Ok(RingChainStatus::default())
    }

    async fn slot(&self) -> Result<u64, ClientError> {
        unused()
    }

    async fn records_root(&self, _ring: Address) -> Result<RecordsTip, ClientError> {
        Ok(RecordsTip::NoPolicy)
    }

    async fn storage_member_openings(
        &self,
        _request: zolana_ring_indexer::rpc::GetStorageMemberOpeningsRequest,
    ) -> Result<zolana_ring_indexer::rpc::GetStorageMemberOpeningsResponse, RingIndexerError> {
        Ok(unused()?)
    }

    async fn storage_member_records(
        &self,
        _request: zolana_ring_indexer::rpc::GetStorageMemberRecordsRequest,
    ) -> Result<zolana_ring_indexer::rpc::GetStorageMemberRecordsResponse, RingIndexerError> {
        Ok(unused()?)
    }

    async fn upgrade_authority(&self, _program: Address) -> Result<Option<Address>, ClientError> {
        Ok(None)
    }

    async fn health(&self) -> Result<(), ClientError> {
        Ok(())
    }

    async fn genesis_hash(&self) -> Result<[u8; 32], ClientError> {
        Ok(SERVICE.cluster)
    }

    async fn tree_id(&self, _tree: Address) -> Result<u16, zolana_ring_client::RingError> {
        Ok(1)
    }

    async fn ring_assets(&self) -> Result<RingAssets, ClientError> {
        Ok(RingAssets::new([]).expect("assets"))
    }
}

struct OneRing(Address);

#[async_trait]
impl ServedRings for OneRing {
    async fn check(&self, ring: Address) -> Result<(), RingRpcError> {
        match ring == self.0 {
            true => Ok(()),
            false => Err(RelayError::RingNotServed.into()),
        }
    }

    async fn delegate(&self, _ring: Address) -> Result<Option<Address>, RingRpcError> {
        Ok(Some(ring_delegate().pubkey()))
    }
}

fn ring_delegate() -> Keypair {
    Keypair::new_from_array([51; 32])
}

struct Fixture {
    relay: Arc<Relay>,
    module: RpcModule<Arc<Hub<RingSource>>>,
    prover: Arc<RecordingProver>,
    chain: Arc<RecordingChain>,
    fee_payer: Address,
    store: Store,
}

impl Fixture {
    async fn new() -> Self {
        Self::with_budget(FRAME_BUDGET).await
    }

    async fn with_budget(budget: u64) -> Self {
        Self::with_budgets(RelayBudget {
            total_lamports: budget,
            signer_lamports: budget,
        })
        .await
    }

    async fn with_budgets(budget: RelayBudget) -> Self {
        let prover = Arc::new(RecordingProver::default());
        let chain = Arc::new(RecordingChain::default());
        let fee_payer = Keypair::new_from_array([31; 32]);
        let fee_payer_address = fee_payer.pubkey();
        let store = store_fixture::test_store().await;
        let auditor = X25519Keypair::new();
        let source = RingSource {
            auditor: auditor.pubkey(),
        };
        let identity = ServiceIdentity {
            genesis_hash: SERVICE.cluster,
            service: Arc::new(service_key()),
        };
        let hub = Hub::builder(source, identity)
            .with_store(store.clone())
            .with_relay(RelayParts {
                store: store.clone(),
                prover: prover.clone(),
                fee_payer: Arc::new(fee_payer),
                chain: chain.clone(),
            })
            .with_relay_budget(budget)
            .local(RING, auditor)
            .expect("hub");
        let hub = Arc::new(hub);
        Self {
            relay: hub.relay().expect("relay").clone(),
            module: rpc_module(hub).expect("module"),
            prover,
            chain,
            fee_payer: fee_payer_address,
            store,
        }
    }

    /// Lamports the total scope holds outstanding.
    async fn outstanding(&self) -> i64 {
        let connection = self.store.connection();
        connection
            .query_one(Statement::from_string(
                connection.get_database_backend(),
                "SELECT CAST(COALESCE(SUM(lamports), 0) AS BIGINT) AS total FROM relay_reservations"
                    .to_owned(),
            ))
            .await
            .expect("sum")
            .expect("row")
            .try_get("", "total")
            .expect("total")
    }

    async fn row_count(&self) -> i64 {
        let connection = self.store.connection();
        connection
            .query_one(Statement::from_string(
                connection.get_database_backend(),
                "SELECT COUNT(*) AS total FROM relay_reservations".to_owned(),
            ))
            .await
            .expect("count")
            .expect("row")
            .try_get("", "total")
            .expect("total")
    }

    /// Lamports the total counts, reserved and spent for good.
    async fn committed(&self) -> i64 {
        let connection = self.store.connection();
        connection
            .query_one(Statement::from_sql_and_values(
                connection.get_database_backend(),
                "SELECT CAST(COALESCE(SUM(committed), 0) AS BIGINT) AS total FROM relay_reserved \
                 WHERE scope=$1",
                vec![vec![0u8; 32].into()],
            ))
            .await
            .expect("sum")
            .expect("row")
            .try_get("", "total")
            .expect("total")
    }

    async fn prepare(
        &self,
        request: &PrepareRelayRequest,
    ) -> Result<RelayStep, jsonrpsee::MethodsError> {
        let serde_json::Value::Object(params) = serde_json::to_value(request).expect("json") else {
            panic!("request is no object");
        };
        self.module.call(PREPARE_RELAY, params).await
    }

    async fn call<T: serde::de::DeserializeOwned + Clone>(
        &self,
        method: &str,
        request: impl serde::Serialize,
    ) -> T {
        let serde_json::Value::Object(params) = serde_json::to_value(request).expect("json") else {
            panic!("request is no object");
        };
        self.module.call(method, params).await.expect(method)
    }

    async fn quote(&self, operation: RelayOperation) -> RelayQuote {
        self.call(
            GET_RELAY_QUOTE,
            GetRelayQuoteRequest {
                ring: RING.into(),
                operation,
            },
        )
        .await
    }

    fn sent(&self) -> Vec<VersionedTransaction> {
        self.chain.sent()
    }
}

#[tokio::test]
async fn an_anonymous_transfer_relays_with_one_wallet_call() {
    let fixture = Fixture::new().await;
    let quote = fixture.quote(RelayOperation::Anonymous).await;
    let proof_inputs = proof_inputs(RING, anonymous_rail());
    let request = signed_request(quote, &proof_inputs);
    let RelayIntent::Anonymous {
        owner_signature, ..
    } = &request.intent
    else {
        unreachable!("anonymous intent");
    };
    let owner_signature = <[u8; 64]>::from(owner_signature.0);

    let step: RelayStep = fixture.call(PREPARE_RELAY, &request).await;

    let RelayStep::Submitted { relay_id } = step else {
        panic!("an anonymous relay asks the wallet for nothing more, got {step:?}");
    };
    assert_eq!(
        *fixture.prover.requests.lock().expect("requests"),
        [(fixture.fee_payer, Some(owner_signature))]
    );
    let [sent] = fixture.sent().try_into().expect("one sent transaction");
    assert_eq!(sent.message.static_account_keys()[0], fixture.fee_payer);
    assert_eq!(sent.signatures.len(), 1);
    assert!(signers_verify(&sent));
    let status: RelayStatus = fixture
        .call(GET_RELAY_STATUS, GetRelayStatusRequest { relay_id })
        .await;
    assert_eq!(
        status,
        RelayStatus::Confirmed {
            signature: sent.signatures[0].into(),
            slot: SLOT + 1,
        }
    );
}

#[tokio::test]
async fn a_confidential_transfer_relays_with_one_wallet_call() {
    let fixture = Fixture::new().await;
    let quote = fixture.quote(RelayOperation::Confidential).await;
    let request = signed_request(quote, &proof_inputs(RING, RelayRail::Confidential));
    let RelayIntent::Confidential {
        owner_signature, ..
    } = &request.intent
    else {
        unreachable!("confidential intent");
    };
    let owner_signature = <[u8; 64]>::from(owner_signature.0);

    let step: RelayStep = fixture.call(PREPARE_RELAY, &request).await;

    let RelayStep::Submitted { relay_id } = step else {
        panic!("the owner signs inside the proof, got {step:?}");
    };
    assert_eq!(
        *fixture.prover.requests.lock().expect("requests"),
        [(fixture.fee_payer, Some(owner_signature))]
    );
    let [sent] = fixture.sent().try_into().expect("one sent transaction");
    assert_eq!(sent.signatures.len(), 1);
    assert_eq!(sent.message.static_account_keys()[0], fixture.fee_payer);
    assert!(signers_verify(&sent));
    let status: RelayStatus = fixture
        .call(GET_RELAY_STATUS, GetRelayStatusRequest { relay_id })
        .await;
    assert_eq!(
        status,
        RelayStatus::Confirmed {
            signature: sent.signatures[0].into(),
            slot: SLOT + 1,
        }
    );
}

#[tokio::test]
async fn a_deposit_relays_through_the_depositors_signature() {
    let fixture = Fixture::new().await;
    let quote = fixture.quote(RelayOperation::Deposit).await;
    let depositor = Keypair::new_from_array([41; 32]);
    let request = deposit_request(SERVICE.ring(RING), quote, &depositor, 700);

    let step: RelayStep = fixture.call(PREPARE_RELAY, &request).await;

    let RelayStep::SignMessage { relay_id, message } = step else {
        panic!("the depositor signs its funding, got {step:?}");
    };
    assert!(fixture.sent().is_empty());
    let _: RelayStatus = fixture
        .call(
            SUBMIT_RELAY,
            SubmitRelayRequest {
                relay_id: relay_id.clone(),
                signer: depositor.pubkey().into(),
                signature: depositor.sign_message(&message.0).into(),
            },
        )
        .await;
    let [sent] = fixture.sent().try_into().expect("one sent transaction");
    assert_eq!(
        sent.message.static_account_keys()[..2],
        [fixture.fee_payer, depositor.pubkey()]
    );
    assert!(signers_verify(&sent));
    let status: RelayStatus = fixture
        .call(GET_RELAY_STATUS, GetRelayStatusRequest { relay_id })
        .await;
    assert_eq!(
        status,
        RelayStatus::Confirmed {
            signature: sent.signatures[0].into(),
            slot: SLOT + 1,
        }
    );
}

#[tokio::test]
async fn an_unsigned_deposit_reaches_no_prover_and_stores_nothing() {
    let fixture = Fixture::new().await;
    let quote = fixture.quote(RelayOperation::Deposit).await;
    let depositor = Keypair::new_from_array([41; 32]);
    let honest = deposit_request(SERVICE.ring(RING), quote, &depositor, 700);
    let proof_inputs = honest.intent.proof_inputs().clone();
    let inputs = RelayProofInputs::decode(proof_inputs.as_bytes()).expect("decode");
    let signed = |signer: &Keypair, message: &[u8]| PrepareRelayRequest {
        intent: RelayIntent::Deposit {
            proof_inputs: proof_inputs.clone(),
            depositor_signature: signer.sign_message(message).into(),
        },
        ..honest.clone()
    };
    let text = |service: ServiceTarget| {
        RelayRequestAttestation::deposit(
            service.ring(RING),
            depositor.pubkey(),
            &inputs,
            &proof_inputs,
        )
        .message()
    };
    let other_service = ServiceTarget {
        service: Address::new_from_array([63; 32]),
        ..*SERVICE
    };
    let other_cluster = ServiceTarget {
        cluster: [64; 32],
        ..*SERVICE
    };

    for request in [
        signed(&Keypair::new_from_array([43; 32]), &text(*SERVICE)),
        signed(&depositor, &proof_inputs.deposit_request_digest()),
        signed(&depositor, &text(other_service)),
        signed(&depositor, &text(other_cluster)),
    ] {
        let refused = fixture.prepare(&request).await;
        assert!(
            refused_with(&refused, RelayError::DepositorSignature),
            "{refused:?}"
        );
    }
    assert!(fixture.prover.requests.lock().expect("requests").is_empty());
    assert_eq!(fixture.row_count().await, 0);
}

/// The depositor reads its leg and the request digest in the text it signs.
#[test]
fn a_deposit_request_text_names_its_leg_and_digest() {
    let depositor = Keypair::new_from_array([41; 32]);
    let quote = RelayQuote {
        fee_payer: Address::new_from_array([31; 32]).into(),
        valid_until_slot: SLOT,
    };
    let request = deposit_request(SERVICE.ring(RING), quote, &depositor, 700);
    let proof_inputs = request.intent.proof_inputs();
    let inputs = RelayProofInputs::decode(proof_inputs.as_bytes()).expect("decode");
    let text = RelayRequestAttestation::deposit(
        SERVICE.ring(RING),
        depositor.pubkey(),
        &inputs,
        proof_inputs,
    )
    .message();
    let digest = spr_indexer_api::Hash(proof_inputs.deposit_request_digest()).to_base58();
    assert_eq!(
        String::from_utf8(text).expect("text"),
        format!(
            "zolana/ring-rpc-relay-deposit\ncluster: {}\nservice: {}\nring: {RING}\nsigner: {}\ndeposit: 11111111111111111111111111111111 700\nrequest: {digest}",
            spr_indexer_api::Hash(SERVICE.cluster).to_base58(),
            SERVICE.service,
            depositor.pubkey(),
        )
    );
}

/// A deposit frame names one funder that is not the fee payer and rides the deposit rail.
#[tokio::test]
async fn a_deposit_frame_off_the_deposit_rail_or_its_one_funder_is_refused() {
    let fixture = Fixture::new().await;
    let quote = fixture.quote(RelayOperation::Deposit).await;
    let depositor = Keypair::new_from_array([41; 32]);
    let resigned = |inputs: &RelayProofInputs, signer: &Keypair| {
        let proof_inputs = ProofInputBytes::new(inputs.encode().expect("encode"));
        let attestation = RelayRequestAttestation::deposit(
            SERVICE.ring(RING),
            signer.pubkey(),
            inputs,
            &proof_inputs,
        );
        let depositor_signature = signer.sign_message(&attestation.message()).into();
        PrepareRelayRequest {
            intent: RelayIntent::Deposit {
                proof_inputs,
                depositor_signature,
            },
            ..deposit_request(SERVICE.ring(RING), quote.clone(), &depositor, 700)
        }
    };
    let honest = deposit_request(SERVICE.ring(RING), quote.clone(), &depositor, 700);
    let inputs = RelayProofInputs::decode(honest.intent.proof_inputs().as_bytes()).expect("decode");

    let mut confidential =
        RelayProofInputs::decode(honest.intent.proof_inputs().as_bytes()).expect("decode");
    confidential.rail = RelayRail::Confidential;
    let mut two_funders =
        RelayProofInputs::decode(honest.intent.proof_inputs().as_bytes()).expect("decode");
    let second = Keypair::new_from_array([42; 32]);
    two_funders
        .external_data
        .interface_transfers
        .push(SettlementTransfer::Sol {
            is_deposit: true,
            amount: 1,
            user_sol_account: second.pubkey(),
        });
    two_funders
        .interface_transfer_accounts
        .push(TransactInterfaceTransferAccounts::Sol(
            TransactSolTransferAccounts {
                recipient: second.pubkey(),
            },
        ));
    let fee_payer_funds = deposit_request(
        SERVICE.ring(RING),
        quote.clone(),
        &Keypair::new_from_array([31; 32]),
        700,
    );

    for (request, error) in [
        (
            resigned(&confidential, &depositor),
            RelayError::RailMismatch,
        ),
        (
            resigned(&two_funders, &depositor),
            RelayError::DepositorSignature,
        ),
        (fee_payer_funds, RelayError::DepositorIsFeePayer),
        (resigned(&inputs, &second), RelayError::DepositorSignature),
    ] {
        let refused = fixture.prepare(&request).await;
        assert!(refused_with(&refused, error), "{refused:?}");
    }
    assert!(fixture.prover.requests.lock().expect("requests").is_empty());
}

const TOKEN_MINT: Address = Address::new_from_array([0x61; 32]);
const TOKEN_OWNER: Address = Address::new_from_array([0x62; 32]);

/// The rent of the token account it creates, its tree fees and the fee payer's one signature.
const FUNDED_LAMPORTS: u64 = RENT_LAMPORTS + TREE_FEES + 5_000;

/// A confidential withdrawal of `amount` to a token account the relay creates, the one operation the budget funds.
fn funded_transfer(fixture: &Fixture, quote: RelayQuote, amount: u64) -> PrepareRelayRequest {
    let token_program = Address::new_from_array(SPL_TOKEN_PROGRAM_ID);
    let token_account = CreateAssociatedTokenAccount {
        payer: fixture.fee_payer,
        owner: TOKEN_OWNER,
        mint: TOKEN_MINT,
        token_program,
    }
    .address();
    let mut inputs = proof_inputs(RING, RelayRail::Confidential);
    inputs
        .external_data
        .interface_transfers
        .push(SettlementTransfer::Spl {
            mint: TOKEN_MINT,
            is_deposit: false,
            amount,
            user_spl_token: token_account,
        });
    inputs
        .interface_transfer_accounts
        .push(TransactInterfaceTransferAccounts::SplWithdrawal(
            TransactSplWithdrawalAccounts {
                mint: TOKEN_MINT,
                spl_interface: spr_interface::pda::spl_interface(&TOKEN_MINT),
                user_token_account: token_account,
                token_program,
            },
        ));
    let mut request = signed_request(quote, &inputs);
    let RelayIntent::Confidential {
        token_account_setup,
        ..
    } = &mut request.intent
    else {
        panic!("a confidential request");
    };
    token_account_setup.push(TokenAccountSetup {
        mint: TOKEN_MINT.into(),
        owner: TOKEN_OWNER.into(),
    });
    request
}

impl Fixture {
    async fn status(&self, step: RelayStep) -> RelayStatus {
        let (RelayStep::Submitted { relay_id } | RelayStep::SignMessage { relay_id, .. }) = step;
        self.call(GET_RELAY_STATUS, GetRelayStatusRequest { relay_id })
            .await
    }

    fn pending(&self, pending: bool) {
        *self.chain.outcome.lock().expect("outcome") = pending.then_some(None);
    }
}

fn refused_with(refused: &Result<RelayStep, jsonrpsee::MethodsError>, error: RelayError) -> bool {
    matches!(
        refused,
        Err(jsonrpsee::MethodsError::JsonRpc(object)) if object.code() == error.code()
    )
}

/// The landed charge stays in the total and the signer's share returns.
#[tokio::test]
async fn a_funded_relay_holds_its_reservation_until_it_settles() {
    let fixture = Fixture::with_budgets(RelayBudget {
        total_lamports: 2 * FUNDED_LAMPORTS,
        signer_lamports: FUNDED_LAMPORTS,
    })
    .await;
    let quote = fixture.quote(RelayOperation::Confidential).await;
    fixture.pending(true);
    let step = fixture
        .prepare(&funded_transfer(&fixture, quote.clone(), 15))
        .await
        .expect("the relay");
    assert_eq!(fixture.row_count().await, 1);
    assert_eq!(fixture.outstanding().await, FUNDED_LAMPORTS as i64);

    let over_signer = fixture
        .prepare(&funded_transfer(&fixture, quote.clone(), 16))
        .await;
    assert!(
        refused_with(&over_signer, RelayError::SignerBudget),
        "{over_signer:?}"
    );

    fixture.pending(false);
    let status = fixture.status(step).await;
    assert!(
        matches!(status, RelayStatus::Confirmed { .. }),
        "{status:?}"
    );
    assert_eq!(fixture.row_count().await, 0);
    assert_eq!(fixture.outstanding().await, 0);
    assert_eq!(fixture.committed().await, FUNDED_LAMPORTS as i64);
    let next = fixture.prepare(&funded_transfer(&fixture, quote, 17)).await;
    assert!(next.is_ok(), "the share returned, got {next:?}");
}

/// The fee payer pays a fee for every message, a relay with nothing else to fund still spends.
#[tokio::test]
async fn a_relay_with_no_setup_and_no_tree_fee_sends_nothing_at_a_zero_budget() {
    for budget in [
        RelayBudget::default(),
        RelayBudget {
            total_lamports: u64::MAX >> 1,
            signer_lamports: 0,
        },
        RelayBudget {
            total_lamports: 0,
            signer_lamports: u64::MAX >> 1,
        },
    ] {
        let fixture = Fixture::with_budgets(budget).await;
        *fixture.prover.tree_fees.lock().expect("tree fees") = Some(0);
        let quote = fixture.quote(RelayOperation::Confidential).await;
        let request = signed_request(quote, &proof_inputs(RING, RelayRail::Confidential));
        for _ in 0..2 {
            let refused = fixture.prepare(&request).await;
            assert!(
                refused_with(&refused, RelayError::BudgetUnset),
                "{refused:?}"
            );
        }
        assert!(fixture.sent().is_empty());
        assert_eq!(fixture.row_count().await, 0);
        assert_eq!(fixture.committed().await, 0);
    }
}

/// One lamport short of the fee refuses, the fee alone is what a relay with nothing else holds.
#[tokio::test]
async fn a_relay_holds_its_base_and_priority_fee_and_keeps_them_once_landed() {
    const PRIORITY_FEE: u64 = 7_000;
    let fee = 5_000 + PRIORITY_FEE;
    for (budget, fits) in [(fee - 1, false), (fee, true)] {
        let fixture = Fixture::with_budget(budget).await;
        *fixture.prover.tree_fees.lock().expect("tree fees") = Some(0);
        *fixture.prover.priority_fee.lock().expect("priority fee") = Some(PRIORITY_FEE);
        let quote = fixture.quote(RelayOperation::Confidential).await;
        let request = signed_request(quote, &proof_inputs(RING, RelayRail::Confidential));
        fixture.pending(true);
        let prepared = fixture.prepare(&request).await;
        if !fits {
            assert!(refused_with(&prepared, RelayError::Budget), "{prepared:?}");
            assert!(fixture.sent().is_empty());
            assert_eq!(fixture.committed().await, 0);
            continue;
        }
        let step = prepared.expect("relayed");
        assert_eq!(fixture.outstanding().await, fee as i64);
        fixture.pending(false);
        let status = fixture.status(step).await;
        assert!(
            matches!(status, RelayStatus::Confirmed { .. }),
            "{status:?}"
        );
        assert_eq!(fixture.outstanding().await, 0);
        assert_eq!(fixture.committed().await, fee as i64);
    }
}

/// An expired message cost nothing, a reverted one cost its fee and never its rent or tree fees.
#[tokio::test]
async fn a_failed_relay_returns_what_the_chain_did_not_take() {
    for (reverted, kept) in [(false, 0), (true, 5_000)] {
        let fixture = Fixture::with_budget(FUNDED_LAMPORTS).await;
        let quote = fixture.quote(RelayOperation::Confidential).await;
        fixture.pending(true);
        let step = fixture
            .prepare(&funded_transfer(&fixture, quote, 15))
            .await
            .expect("relayed");
        assert_eq!(fixture.outstanding().await, FUNDED_LAMPORTS as i64);
        assert_eq!(fixture.committed().await, FUNDED_LAMPORTS as i64);

        match reverted {
            true => {
                *fixture.chain.outcome.lock().expect("outcome") =
                    Some(Some(ChainOutcome::Failed {
                        reason: "custom program error".to_owned(),
                    }));
            }
            false => fixture
                .chain
                .height
                .store(LAST_VALID_BLOCK_HEIGHT + 1, Ordering::Relaxed),
        }
        let status = fixture.status(step).await;
        assert!(matches!(status, RelayStatus::Failed { .. }), "{status:?}");
        assert_eq!(fixture.row_count().await, 0);
        assert_eq!(fixture.committed().await, kept);
    }
}

/// A retried request names the same relay and is charged once.
#[tokio::test]
async fn a_retried_funded_relay_is_charged_once() {
    let fixture = Fixture::with_budget(FUNDED_LAMPORTS).await;
    let quote = fixture.quote(RelayOperation::Confidential).await;
    fixture.pending(true);
    let request = funded_transfer(&fixture, quote, 15);
    assert!(fixture.prepare(&request).await.is_ok());
    let retried = fixture.prepare(&request).await;
    assert!(retried.is_ok(), "{retried:?}");
    assert_eq!(fixture.outstanding().await, FUNDED_LAMPORTS as i64);
    assert_eq!(fixture.sent().len(), 1);
}

#[tokio::test]
async fn a_merge_relays_with_one_wallet_call() {
    let fixture = Fixture::new().await;
    let quote = fixture.quote(RelayOperation::Merge).await;
    let spends = proof_inputs(RING, RelayRail::Confidential);
    let note = |spend: &SppProofInputUtxo| WalletUtxo {
        utxo: spend.utxo.clone(),
        nullifier_pubkey: spend.nullifier_pubkey,
        utxo_hash: spend.utxo_hash,
        nullifier: spend.nullifier,
        tx_viewing_key: None,
        data_hash: None,
        ring_data_hash: None,
        tree_id: spend.tree_id,
        leaf_index: spend.leaf_index,
        latest_tree_id: None,
        slot: 0,
        tx_signature: Signature::default(),
        slot_index: 0,
    };
    let merge = RelayMergeInputs {
        ring_program_id: RING,
        sender: spends.sender,
        nullifier_key: NullifierKey::from_secret(*spends.nullifier_key.secret()),
        inputs: spends.inputs.iter().map(note).collect(),
    };
    let proof_inputs = ProofInputBytes::new(merge.encode().expect("encode"));
    let text = |service: ServiceTarget| {
        RelayRequestAttestation::merge(service.ring(RING), owner_wallet().pubkey(), &proof_inputs)
            .message()
    };
    let owner_signature = owner_wallet().sign_message(&text(*SERVICE)).into();
    let signed_by = |owner_signature| PrepareRelayRequest {
        ring: RING.into(),
        quote: quote.clone(),
        intent: RelayIntent::Merge {
            proof_inputs: proof_inputs.clone(),
            owner_signature,
        },
        co_signers: Vec::new(),
    };
    let other_service = ServiceTarget {
        service: Address::new_from_array([63; 32]),
        ..*SERVICE
    };

    for refused in [
        Keypair::new_from_array([43; 32]).sign_message(&text(*SERVICE)),
        owner_wallet().sign_message(&proof_inputs.merge_request_digest()),
        owner_wallet().sign_message(&text(other_service)),
    ] {
        let refused = fixture.prepare(&signed_by(refused.into())).await;
        assert!(
            refused_with(&refused, RelayError::OwnerSignature),
            "{refused:?}"
        );
    }
    assert!(fixture.prover.requests.lock().expect("requests").is_empty());

    let step: RelayStep = fixture
        .call(PREPARE_RELAY, &signed_by(owner_signature))
        .await;

    let RelayStep::Submitted { relay_id } = step else {
        panic!("a signed merge sends at once, got {step:?}");
    };
    let [sent] = fixture.sent().try_into().expect("one sent transaction");
    assert_eq!(sent.signatures.len(), 1);
    assert!(signers_verify(&sent));
    let status: RelayStatus = fixture
        .call(GET_RELAY_STATUS, GetRelayStatusRequest { relay_id })
        .await;
    assert_eq!(
        status,
        RelayStatus::Confirmed {
            signature: sent.signatures[0].into(),
            slot: SLOT + 1,
        }
    );
}

/// Past one transaction the frame refuses under its own code, nothing is sent or charged.
#[tokio::test]
async fn a_frame_past_one_transaction_is_refused_before_any_send_or_charge() {
    use custom_ring_interface::tag as ring_tag;
    for frame_tag in [ring_tag::TRANSFER, ring_tag::AUTHORITY_TRANSFER] {
        let fixture = Fixture::with_budget(FRAME_BUDGET).await;
        *fixture.prover.frame.lock().expect("frame") = Some(wide_frame(frame_tag));
        let quote = fixture.quote(RelayOperation::Anonymous).await;
        let request = signed_request(quote, &proof_inputs(RING, anonymous_rail()));
        let refused = fixture.prepare(&request).await;
        let oversized = RelayError::Oversized {
            reason: String::new(),
        };
        assert!(refused_with(&refused, oversized), "{refused:?}");
        assert!(fixture.sent().is_empty());
        assert_eq!(fixture.outstanding().await, 0);
        assert_eq!(fixture.committed().await, 0);
    }
}

const FRAME_BUDGET: u64 = 1_000_000;

/// Landed fees never return, only the operator's budget ends repeated relays.
#[tokio::test]
async fn repeated_landed_relays_stop_at_the_budget() {
    let fixture = Fixture::with_budget(2 * FUNDED_LAMPORTS).await;
    let quote = fixture.quote(RelayOperation::Confidential).await;
    for amount in [15, 16] {
        let step = fixture
            .prepare(&funded_transfer(&fixture, quote.clone(), amount))
            .await
            .expect("within the budget");
        let status = fixture.status(step).await;
        assert!(
            matches!(status, RelayStatus::Confirmed { .. }),
            "{status:?}"
        );
    }
    assert_eq!(fixture.outstanding().await, 0);
    assert_eq!(fixture.committed().await, (2 * FUNDED_LAMPORTS) as i64);
    let refused = fixture.prepare(&funded_transfer(&fixture, quote, 17)).await;
    assert!(refused_with(&refused, RelayError::Budget), "{refused:?}");
}

/// The named key signs what an owner signs on the other rails.
fn delegate_request(
    quote: RelayQuote,
    proof_inputs: &RelayProofInputs,
    signer: &Keypair,
) -> PrepareRelayRequest {
    let digest = proof_inputs
        .spp_proof_inputs(quote.fee_payer.0)
        .message_hash()
        .expect("message hash");
    PrepareRelayRequest {
        ring: proof_inputs.ring_program_id.into(),
        quote,
        intent: RelayIntent::Delegate {
            proof_inputs: ProofInputBytes::new(proof_inputs.encode().expect("encode")),
            delegate_signature: signer.sign_message(&digest).into(),
            token_account_setup: Vec::new(),
        },
        co_signers: Vec::new(),
    }
}

fn delegate_inputs(delegate: Address) -> RelayProofInputs {
    proof_inputs(
        RING,
        RelayRail::Delegate {
            delegate,
            plan: None,
        },
    )
}

#[tokio::test]
async fn a_delegate_frame_relays_under_the_delegates_signature() {
    let fixture = Fixture::with_budget(FRAME_BUDGET).await;
    let delegate = ring_delegate();
    let quote = fixture.quote(RelayOperation::Delegate).await;
    let request = delegate_request(quote, &delegate_inputs(delegate.pubkey()), &delegate);

    let step = fixture.prepare(&request).await.expect("the delegate relay");

    assert!(matches!(step, RelayStep::SignMessage { .. }), "{step:?}");
    assert!(fixture.sent().is_empty());
    assert_eq!(fixture.outstanding().await, TREE_FEES as i64 + 10_000);
    let connection = fixture.store.connection();
    let signer: Vec<u8> = connection
        .query_one(Statement::from_string(
            connection.get_database_backend(),
            "SELECT signer FROM relay_reservations".to_owned(),
        ))
        .await
        .expect("read")
        .expect("reservation")
        .try_get("", "signer")
        .expect("signer");
    assert_eq!(signer, delegate.pubkey().to_bytes());
}

#[tokio::test]
async fn an_unsigned_or_foreign_delegate_request_consumes_nothing() {
    let fixture = Fixture::with_budget(FRAME_BUDGET).await;
    let delegate = ring_delegate();
    let stranger = Keypair::new_from_array([52; 32]);
    let quote = fixture.quote(RelayOperation::Delegate).await;
    for (named, signer) in [
        (delegate.pubkey(), &stranger),
        (stranger.pubkey(), &stranger),
    ] {
        let request = delegate_request(quote.clone(), &delegate_inputs(named), signer);
        let refused = fixture.prepare(&request).await;
        assert!(
            refused_with(&refused, RelayError::DelegateSignature),
            "{refused:?}"
        );
    }
    assert!(fixture.prover.requests.lock().expect("requests").is_empty());
    assert!(fixture.sent().is_empty());
    assert_eq!(fixture.committed().await, 0);
}

/// Tree fees charge the budget even when the token account exists.
#[tokio::test]
async fn token_account_setup_reserves_the_rent_of_a_missing_account() {
    let (mint, token_owner) = (TOKEN_MINT, TOKEN_OWNER);
    for (budget, exists) in [
        (0, false),
        (0, true),
        (FRAME_BUDGET, true),
        (FRAME_BUDGET, false),
    ] {
        let fixture = Fixture::with_budget(budget).await;
        let token_program = Address::new_from_array(SPL_TOKEN_PROGRAM_ID);
        let token_account = CreateAssociatedTokenAccount {
            payer: fixture.fee_payer,
            owner: token_owner,
            mint,
            token_program,
        }
        .address();
        if exists {
            fixture
                .chain
                .accounts
                .lock()
                .expect("accounts")
                .push(token_account);
        }
        let mut inputs = proof_inputs(RING, RelayRail::Confidential);
        inputs
            .external_data
            .interface_transfers
            .push(SettlementTransfer::Spl {
                mint,
                is_deposit: false,
                amount: 15,
                user_spl_token: token_account,
            });
        inputs
            .interface_transfer_accounts
            .push(TransactInterfaceTransferAccounts::SplWithdrawal(
                TransactSplWithdrawalAccounts {
                    mint,
                    spl_interface: spr_interface::pda::spl_interface(&mint),
                    user_token_account: token_account,
                    token_program,
                },
            ));
        let quote = fixture.quote(RelayOperation::Confidential).await;
        let mut request = signed_request(quote, &inputs);
        let RelayIntent::Confidential {
            token_account_setup,
            ..
        } = &mut request.intent
        else {
            panic!("a confidential request");
        };
        token_account_setup.push(TokenAccountSetup {
            mint: mint.into(),
            owner: token_owner.into(),
        });

        let prepared = fixture.prepare(&request).await;

        if budget == 0 {
            assert!(
                refused_with(&prepared, RelayError::BudgetUnset),
                "{prepared:?}"
            );
            assert!(fixture.sent().is_empty());
            continue;
        }
        let RelayStep::Submitted { relay_id } = prepared.expect("relayed") else {
            panic!("the owner signed");
        };
        let _: RelayStatus = fixture
            .call(GET_RELAY_STATUS, GetRelayStatusRequest { relay_id })
            .await;
        assert_eq!(fixture.outstanding().await, 0);
        // A live account takes no create, an owner closing it later finds no rent to spend.
        assert_eq!(
            fixture.sent()[0].message.instructions().len(),
            if exists { 1 } else { 2 }
        );
        let fee = i64::from(fixture.sent()[0].message.header().num_required_signatures) * 5_000;
        let kept = if exists {
            TREE_FEES as i64 + fee
        } else {
            RENT_LAMPORTS as i64 + TREE_FEES as i64 + fee
        };
        assert_eq!(fixture.committed().await, kept);
    }
}
