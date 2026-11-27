use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use zolana_ring_client::TrackingRegistryHost;

use custom_ring_interface::{
    storage::{RegistryId, Revision, StorageRegistry},
    storage_instruction::{tag, InitializeIxData, InitializeTrackingIxData},
    tracking::TrackingRegistry,
    MEMBER_REGISTRY_EMPTY_ROOT,
};
use sea_orm::{Database, DatabaseConnection, TransactionTrait};
use sea_orm_migration::MigratorTrait;
use solana_address::Address;
use solana_signature::Signature;
use spr_indexer_api::Hash;
use zolana_ring_indexer::rpc::{GetStorageMemberOpeningsRequest, GetStorageMemberRecordsRequest};

use crate::{
    api,
    block::{BlockInfo, BlockMetadata, Instruction, InstructionGroup, TransactionInfo},
    error::RingApiError,
    indexer::{
        block_batch, from_storage, ApplyError, Indexer, Readiness, RegistryCall, StartSlot,
        MIN_BLOCK_BATCH,
    },
    migration::Migrator,
    rpc::SolanaRpc,
    storage::{
        self, RingCursor, StorageRoot, StorageState, StorageStore, TrackingState, TrackingStore,
    },
    storage_registry::StorageCall,
    tracking_registry::TrackingCall,
};

pub(crate) fn field(value: u8) -> [u8; 32] {
    let mut field = [0; 32];
    field[31] = value;
    field
}

pub(crate) async fn database() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    db
}

pub(crate) async fn fixture() -> (DatabaseConnection, StorageState, RingCursor) {
    let db = database().await;
    let cursor = RingCursor::new(0);
    let state = create_registry(&db, [11; 32]).await;
    storage::save_cursor(&db, &cursor).await.unwrap();
    (db, state, cursor)
}

/// A registry as `Initialize` leaves it.
pub(crate) async fn create_registry(db: &DatabaseConnection, registry: [u8; 32]) -> StorageState {
    let state = StorageState {
        registry,
        account: Box::new(StorageRegistry::new(
            Address::new_from_array([3; 32]),
            &InitializeIxData {
                registry_id: RegistryId::default(),
                grace_slots: [0; 8],
                ring_id: [0; 32],
                tracking_registry: Address::default(),
            },
        )),
        fault: None,
    };
    let tx = db.begin().await.unwrap();
    <StorageCall as RegistryCall<StorageRegistry>>::create(
        &StorageStore::new(&tx, registry),
        &state,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    state
}

pub(crate) fn indexer(db: &DatabaseConnection, start: Option<StartSlot>) -> Indexer {
    Indexer {
        db: Arc::new(db.clone()),
        rpc: Arc::new(SolanaRpc::new("http://127.0.0.1:1".into())),
        start,
        block_fetches: 4,
        readiness: Readiness::default(),
    }
}

fn openings_request(state: &StorageState, root: [u8; 32]) -> GetStorageMemberOpeningsRequest {
    GetStorageMemberOpeningsRequest {
        registry: Address::new_from_array(state.registry).into(),
        members: vec![Hash(field(4))],
        expected_root: Hash(root),
    }
}

#[tokio::test]
async fn member_page_reports_a_missing_leaf_instead_of_skipping_it() {
    let (db, mut state, mut cursor) = fixture().await;
    state.account.next_index = 2u64.to_le_bytes();
    state.account.roots[0].revision = Revision::from(1);
    let store = StorageStore::new(&db, state.registry);
    store.save_state(&state).await.unwrap();
    store
        .save_root(&StorageRoot::current(&*state.account))
        .await
        .unwrap();
    cursor.resume(&db).await.unwrap();
    let request = GetStorageMemberRecordsRequest {
        registry: Address::new_from_array(state.registry).into(),
        expected_root: Hash(MEMBER_REGISTRY_EMPTY_ROOT),
        start_index: 1,
        limit: 1,
    };
    let rpc = indexer(&db, None).rpc;
    assert_eq!(
        api::storage_member_records(&db, rpc.as_ref(), request).await,
        Err(RingApiError::PageGap)
    );
}

#[tokio::test]
async fn reads_fail_closed_before_rpc_while_catching_up_or_off_the_history() {
    let (db, state, mut cursor) = fixture().await;
    let rpc = indexer(&db, None).rpc;
    let read = |request| api::storage_member_openings(&db, rpc.as_ref(), request);
    let request = openings_request(&state, MEMBER_REGISTRY_EMPTY_ROOT);
    let catching_up = RingApiError::OutOfSync("indexer is catching up or recovering".into());
    assert_eq!(read(request.clone()).await, Err(catching_up));
    cursor.resume(&db).await.unwrap();
    let off_history = openings_request(&state, field(9));
    assert_eq!(read(off_history).await, Err(RingApiError::RootChanged));
    // A held root passes every projection check and stops at the chain confirmation.
    let unconfirmed = RingApiError::OutOfSync("cursor has no canonical block".into());
    assert_eq!(read(request).await, Err(unconfirmed));
}

#[tokio::test]
async fn block_batches_cross_skipped_pages_without_skipping_live_blocks() {
    let server = jsonrpsee::server::ServerBuilder::default()
        .build("127.0.0.1:0")
        .await
        .unwrap();
    let address = server.local_addr().unwrap();
    let mut module = jsonrpsee::RpcModule::new(());
    module
        .register_method("getBlocks", |params, _, _| {
            let (start, end, commitment): (u64, u64, serde_json::Value) = params.parse()?;
            assert_eq!(commitment["commitment"], "confirmed");
            Ok::<_, jsonrpsee::types::ErrorObjectOwned>(
                (5000..=5040)
                    .filter(|slot| (start..=end).contains(slot))
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap();
    let handle = server.start(module);
    let rpc = SolanaRpc::new(format!("http://{address}"));
    let first = block_batch(&rpc, 1..=5100, MIN_BLOCK_BATCH).await.unwrap();
    assert_eq!(first.slots, (5000..=5031).collect::<Vec<_>>());
    assert_eq!(first.scanned_slot, 5031);
    let second = block_batch(&rpc, 5032..=5100, MIN_BLOCK_BATCH)
        .await
        .unwrap();
    assert_eq!(second.slots, (5032..=5040).collect::<Vec<_>>());
    assert_eq!(second.scanned_slot, 5100);
    handle.stop().unwrap();
    handle.stopped().await;
}

#[tokio::test]
async fn an_explicit_start_slot_must_match_the_stored_cursor() {
    let (db, _, _) = fixture().await;
    assert!(indexer(&db, Some(StartSlot::Slot(6)))
        .cursor()
        .await
        .is_err());
    for start in [
        Some(StartSlot::Slot(1)),
        Some(StartSlot::Slot(0)),
        Some(StartSlot::Latest),
        None,
    ] {
        let cursor = indexer(&db, start).cursor().await.unwrap();
        assert_eq!(cursor.start_slot, 0, "{start:?}");
    }
}

#[tokio::test]
async fn an_empty_index_needs_a_start_slot() {
    let db = database().await;
    assert!(indexer(&db, None).cursor().await.is_err());
    assert!(storage::cursor(&db).await.unwrap().is_none());
    let cursor = indexer(&db, Some(StartSlot::Slot(10)))
        .cursor()
        .await
        .unwrap();
    assert_eq!((cursor.start_slot, cursor.scanned_slot), (9, 9));
    assert_eq!(
        indexer(&db, None).cursor().await.unwrap().start_slot,
        cursor.start_slot
    );
}

#[test]
fn start_slots_parse_a_slot_or_latest() {
    assert_eq!("latest".parse(), Ok(StartSlot::Latest));
    assert_eq!("42".parse(), Ok(StartSlot::Slot(42)));
    assert!("soon".parse::<StartSlot>().is_err());
}

#[test]
fn the_announcement_signs_with_a_pinned_kms_key_alone() {
    use clap::{error::ErrorKind, Parser};
    use zolana_operator_custody::{KeyArn, KmsEndpoint};

    const KEY_ARN: &str =
        "arn:aws:kms:us-east-1:123456789012:key/1a2b3c4d-0000-4000-8000-000000000000";
    let service = Address::new_from_array([7; 32]);
    let service_text = service.to_string();
    let parse = |flags: &[&str]| {
        let base = [
            "ring-indexer",
            "--db-url",
            "sqlite::memory:",
            "--photon-url",
            "http://127.0.0.1:8784",
            "--photon-registry-url",
            "http://127.0.0.1:8790",
            "--public-url",
            "http://127.0.0.1:8786",
        ];
        crate::Args::try_parse_from(base.iter().chain(flags))
    };

    let seed = parse(&["--service-key-file", "service.seed"]).unwrap_err();
    assert_eq!(seed.kind(), ErrorKind::UnknownArgument);
    let unpinned = parse(&["--service-key-arn", KEY_ARN]).unwrap_err();
    assert_eq!(unpinned.kind(), ErrorKind::MissingRequiredArgument);

    let pinned = parse(&[
        "--service-key-arn",
        KEY_ARN,
        "--service-address",
        &service_text,
        "--localnet-kms",
        "127.0.0.1:4566",
    ])
    .unwrap();
    assert_eq!(
        pinned.service_key_arn,
        Some(KEY_ARN.parse::<KeyArn>().unwrap())
    );
    assert_eq!(pinned.service_address, Some(service));
    assert_eq!(
        pinned.kms.endpoint(),
        Ok(KmsEndpoint::Localnet("127.0.0.1:4566".parse().unwrap()))
    );
}

#[tokio::test]
async fn storage_shaped_instructions_of_other_programs_cost_no_queries() {
    let mut db = database().await;
    let statements = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&statements);
    db.set_metric_callback(move |_| {
        counted.fetch_add(1, Ordering::Relaxed);
    });
    create_registry(&db, [11; 32]).await;
    let foreign = Address::new_from_array([0x55; 32]);
    let storage_tags = [
        tag::INITIALIZE,
        tag::UPDATE,
        tag::UPDATE_AUTHORITY,
        tag::APPLY_ENROLLMENTS,
        tag::INITIALIZE_TRACKING,
        tag::EMIT_EVENT,
    ];
    let shaped = storage_tags
        .into_iter()
        .map(|tag| TransactionInfo {
            signature: Signature::from([1; 64]),
            error: None,
            instruction_groups: vec![InstructionGroup {
                outer_instruction: Instruction {
                    program_id: foreign,
                    data: vec![tag, 1, 2, 3],
                    accounts: vec![],
                    stack_height: Some(1),
                },
                inner_instructions: vec![],
            }],
        })
        .collect();
    let indexer = indexer(&db, None);
    let mut cursor = RingCursor::new(0);
    let mut costs = Vec::new();
    for (slot, transactions) in [(3, Vec::new()), (4, shaped)] {
        let block = BlockInfo {
            metadata: BlockMetadata {
                slot,
                blockhash: Hash(field(slot as u8)),
                ..Default::default()
            },
            transactions,
        };
        let before = statements.load(Ordering::Relaxed);
        let tx = db.begin().await.unwrap();
        indexer.apply_block(&tx, &block, &mut cursor).await.unwrap();
        tx.commit().await.unwrap();
        costs.push(statements.load(Ordering::Relaxed) - before);
    }
    assert_eq!(costs[0], costs[1]);
}

#[tokio::test]
async fn a_storage_call_under_another_program_moves_no_registry() {
    let db = database().await;
    let registry = [11; 32];
    let before = create_registry(&db, registry).await;
    let takeover = TransactionInfo {
        signature: Signature::from([1; 64]),
        error: None,
        instruction_groups: vec![InstructionGroup {
            outer_instruction: Instruction {
                program_id: Address::new_from_array([0x55; 32]),
                data: vec![tag::UPDATE_AUTHORITY],
                accounts: vec![
                    before.account.authority,
                    Address::new_from_array([0x66; 32]),
                    Address::new_from_array(registry),
                ],
                stack_height: Some(1),
            },
            inner_instructions: vec![],
        }],
    };
    let block = BlockInfo {
        metadata: BlockMetadata {
            slot: 3,
            blockhash: Hash(field(3)),
            ..Default::default()
        },
        transactions: vec![takeover],
    };

    let tx = db.begin().await.unwrap();
    indexer(&db, None)
        .apply_block(&tx, &block, &mut RingCursor::new(0))
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let after = StorageStore::new(&db, registry).state().await.unwrap();
    assert_eq!(after, Some(before));
}

#[test]
fn a_stored_value_the_codecs_refuse_faults_its_ring_and_a_database_failure_retries() {
    let refused = ring_service_core::sql::integer(u64::MAX).unwrap_err();
    assert!(matches!(
        from_storage(anyhow::Error::from(refused).context("load")),
        ApplyError::Fault(_)
    ));
    let failed = sea_orm::DbErr::RecordNotFound("row".into());
    assert!(matches!(
        from_storage(anyhow::Error::from(failed).context("load")),
        ApplyError::Retry(_)
    ));
}

#[test]
fn ring_errors_keep_their_wire_codes() {
    for (error, code) in [
        (RingApiError::OutOfSync(String::new()), -32070),
        (RingApiError::RootChanged, -32071),
        (RingApiError::PageGap, -32075),
        (RingApiError::InvalidRequest(String::new()), -32600),
        (RingApiError::Internal(String::new()), -32603),
    ] {
        assert_eq!(jsonrpsee::types::ErrorObjectOwned::from(error).code(), code);
    }
}

#[tokio::test]
#[ignore = "needs RING_INDEXER_TEST_POSTGRES_URL"]
async fn postgres_migrates_both_ways_and_serves_the_ring_queries() {
    let url = std::env::var("RING_INDEXER_TEST_POSTGRES_URL").unwrap();
    let db = Database::connect(url).await.unwrap();
    Migrator::reset(&db).await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    Migrator::down(&db, None).await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    let state = create_registry(&db, [11; 32]).await;
    let store = StorageStore::new(&db, state.registry);
    assert_eq!(store.state().await.unwrap(), Some(state));
    let low = store.tree().low_record(&field(4), 0).await.unwrap();
    assert_eq!(low.index, 0);
    let mut cursor = RingCursor::new(0);
    cursor.resume(&db).await.unwrap();
    let tx = storage::begin_read(&db).await.unwrap();
    assert!(storage::cursor(&tx).await.unwrap().unwrap().is_ready());
    tx.commit().await.unwrap();
    storage::prune(&db, 1).await.unwrap();
    let tracking = TrackingStore::new(&db, [12; 32]);
    let creator = InitializeTrackingIxData {
        ring_id: field(9),
        auditor_pubkey: [9; 33],
    };
    <TrackingCall as RegistryCall<TrackingRegistry>>::create(
        &tracking,
        &TrackingState {
            registry: [12; 32],
            account: Box::new(TrackingRegistry::new(
                Address::new_from_array([3; 32]),
                &creator,
            )),
            fault: None,
        },
    )
    .await
    .unwrap();
    assert!(tracking.state().await.unwrap().is_some());
    let low = tracking.tree().low_record(&field(4), 0).await.unwrap();
    assert_eq!(low.index, 0);
    Migrator::reset(&db).await.unwrap();
}
