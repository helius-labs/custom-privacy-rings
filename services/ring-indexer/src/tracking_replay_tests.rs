use bytemuck::Zeroable;
use custom_ring_interface::{
    storage::Revision,
    storage_instruction::{
        tag, ApplyEnrollmentsIxData, EnrollIxData, EnrollRequest, InitializeTrackingIxData,
    },
    tracking::{
        tracking_chain_start, EnrollmentQueue, TrackingEntry, TrackingRegistry,
        ENROLLMENT_QUEUE_CAPACITY, TRACKING_EMPTY_ROOT,
    },
    CustomRingProof, STORAGE_PROGRAM_ID, VIEWING_PUBKEY_TAG,
};
use sea_orm::TransactionTrait;
use solana_address::Address;
use solana_signature::Signature;
use spr_hasher::primitives::hash_bytes;
use spr_indexer_api::Hash;
use zolana_ring_indexer::tracking::{
    GetEnrollmentQueueRequest, GetTrackingApplyProofInputsRequest, GetTrackingOpeningsRequest,
    GetTrackingRecordPoolRequest, GetTrackingRecordsRequest, GetTrackingRecordsResponse,
    RingTrackingOpening,
};
use zolana_ring_member_registry::{RecordTree, TrackingTree};

use super::{account, block, replay_indexer, rpc, Chain};
use crate::{
    api,
    block::{BlockInfo, BlockMetadata, Instruction, InstructionGroup, TransactionInfo},
    error::RingApiError,
    indexer::Indexer,
    storage::{self, QueueBounds, RingCursor, TrackingState, TrackingStore},
    tests::{database, field},
};

const OWNER: Address = Address::new_from_array([51; 32]);
const NEWEST_SLOT: u64 = 5_000;

struct Ledger {
    indexer: Indexer,
    cursor: RingCursor,
    chain: std::sync::Arc<std::sync::Mutex<Chain>>,
    handle: jsonrpsee::server::ServerHandle,
    registry: Address,
    queue: Address,
}

/// Passes `EnrollIxData::check`, `seal` varies the ciphertext and `viewing` the key.
fn ix_data(seal: u8, viewing: u8) -> EnrollIxData {
    EnrollIxData {
        nullifier_pk: field(1),
        list_tag: field(2),
        viewing_u: field(viewing),
        eph_u: field(4),
        ciphertext: [seal; 64],
    }
}

fn instruction(accounts: Vec<Address>, data: Vec<u8>) -> Instruction {
    Instruction {
        program_id: STORAGE_PROGRAM_ID,
        data,
        accounts,
        stack_height: Some(1),
    }
}

fn entry(owner: Address, ix: &EnrollIxData, proven: bool) -> TrackingEntry {
    TrackingEntry::from_enroll(&owner, ix, proven).unwrap()
}

impl Ledger {
    async fn new() -> Self {
        let creator = Address::new_from_array([50; 32]);
        let ring_id = field(9);
        let (registry, _) = TrackingRegistry::find_address(&creator, &ring_id);
        let (queue, _) = EnrollmentQueue::find_address(&registry);
        let mut auditor_pubkey = [9; 33];
        auditor_pubkey[0] = VIEWING_PUBKEY_TAG;
        let data = InitializeTrackingIxData {
            ring_id,
            auditor_pubkey,
        };
        let chain = std::sync::Arc::new(std::sync::Mutex::new(Chain {
            blocks: [(NEWEST_SLOT, block(NEWEST_SLOT, vec![]))].into(),
            ..Default::default()
        }));
        let (rpc, handle) = rpc(chain.clone()).await;
        let mut ledger = Self {
            indexer: replay_indexer(std::sync::Arc::new(database().await), rpc),
            cursor: RingCursor::new(0),
            chain,
            handle,
            registry,
            queue,
        };
        let initialize = instruction(
            vec![creator, creator, registry, queue],
            [&[tag::INITIALIZE_TRACKING][..], bytemuck::bytes_of(&data)].concat(),
        );
        ledger.land(1, initialize).await.unwrap();
        ledger
    }

    fn enroll(&self, owner: Address, ix: &EnrollIxData, proven: bool) -> Instruction {
        let proof = CustomRingProof::zeroed();
        let request = EnrollRequest {
            ix,
            proof: proven.then_some(&proof),
        };
        instruction(
            vec![owner, self.registry, self.queue],
            [&[tag::ENROLL][..], &request.encode()].concat(),
        )
    }

    async fn land(&mut self, slot: u64, instruction: Instruction) -> anyhow::Result<()> {
        self.land_all(slot, vec![instruction]).await
    }

    async fn land_all(&mut self, slot: u64, instructions: Vec<Instruction>) -> anyhow::Result<()> {
        let transactions = instructions
            .into_iter()
            .map(|instruction| settled(instruction, vec![]))
            .collect();
        self.land_transactions(slot, transactions).await
    }

    async fn land_transactions(
        &mut self,
        slot: u64,
        transactions: Vec<TransactionInfo>,
    ) -> anyhow::Result<()> {
        let block = BlockInfo {
            metadata: BlockMetadata {
                slot,
                blockhash: Hash(field(slot as u8)),
                ..Default::default()
            },
            transactions,
        };
        let tx = self.indexer.db.begin().await?;
        self.indexer
            .apply_block(&tx, &block, &mut self.cursor)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The apply an honest applier proves at the projected revision, consuming `entries` from the head.
    async fn apply(&self, tree: &mut TrackingTree, entries: &[TrackingEntry]) -> Instruction {
        let revision: Revision = self.state().await.account.newest().revision;
        let chain_start = tracking_chain_start(&self.registry, revision).unwrap();
        let applied = tree.clone().apply(chain_start, entries).unwrap();
        *tree = applied.tree;
        self.apply_data(ApplyEnrollmentsIxData {
            revision,
            proof: Zeroable::zeroed(),
            new_root: applied.proof.new_root,
            new_next_index: applied.proof.new_next_index.to_le_bytes(),
            count: entries.len() as u8,
        })
    }

    fn apply_data(&self, data: ApplyEnrollmentsIxData) -> Instruction {
        instruction(
            vec![self.registry, self.queue],
            [&[tag::APPLY_ENROLLMENTS][..], bytemuck::bytes_of(&data)].concat(),
        )
    }

    /// The chain confirms the projection and its queue bounds at `slot`.
    async fn serve(&mut self, slot: u64) {
        let state = self.state().await;
        let bounds = self.bounds().await;
        self.hold_queue(bounds);
        {
            let mut chain = self.chain.lock().unwrap();
            chain.blocks.insert(slot, block(slot, vec![]));
            chain.accounts.insert(
                self.registry.to_string(),
                account(STORAGE_PROGRAM_ID, bytemuck::bytes_of(&*state.account)),
            );
        }
        self.cursor.resume(self.indexer.db.as_ref()).await.unwrap();
    }

    fn hold_queue(&self, bounds: QueueBounds) {
        let mut queue = EnrollmentQueue::new(self.registry);
        queue.head = bounds.head.to_le_bytes();
        queue.tail = bounds.tail.to_le_bytes();
        self.chain.lock().unwrap().accounts.insert(
            self.queue.to_string(),
            account(STORAGE_PROGRAM_ID, bytemuck::bytes_of(&queue)),
        );
    }

    fn store(&self) -> TrackingStore<'_, sea_orm::DatabaseConnection> {
        TrackingStore::new(self.indexer.db.as_ref(), self.registry.to_bytes())
    }

    async fn state(&self) -> TrackingState {
        self.store().state().await.unwrap().unwrap()
    }

    async fn bounds(&self) -> QueueBounds {
        self.store().queue_bounds().await.unwrap()
    }

    async fn fault(&self) -> String {
        self.state().await.fault.expect("the projection stopped")
    }

    /// The owner's opening under the newest root, served as a sender reads it.
    async fn opening(&mut self, slot: u64, owner: Address) -> RingTrackingOpening {
        self.serve(slot).await;
        let member = entry(owner, &ix_data(5, 3), true).member;
        let root = self.state().await.account.newest().hash;
        let response = api::tracking_openings(
            self.indexer.db.as_ref(),
            self.indexer.rpc.as_ref(),
            GetTrackingOpeningsRequest {
                registry: self.registry.into(),
                members: vec![Hash(member)],
                expected_root: Hash(root),
            },
        )
        .await
        .unwrap();
        response.openings.into_iter().next().unwrap()
    }

    /// Rolls the newest block back as a fork does.
    async fn rewind(&mut self) {
        let tx = self.indexer.db.begin().await.unwrap();
        storage::rollback(&tx, &mut self.cursor).await.unwrap();
        tx.commit().await.unwrap();
    }

    async fn stop(self) {
        self.handle.stop().unwrap();
        self.handle.stopped().await;
    }
}

fn settled(
    outer_instruction: Instruction,
    inner_instructions: Vec<Instruction>,
) -> TransactionInfo {
    TransactionInfo {
        signature: Signature::from([1; 64]),
        error: None,
        instruction_groups: vec![InstructionGroup {
            outer_instruction,
            inner_instructions,
        }],
    }
}

#[tokio::test]
async fn enrollments_queue_in_block_order_and_move_no_root() {
    let mut ledger = Ledger::new().await;
    let before = ledger.state().await;
    let first = ix_data(5, 3);
    let rotation = ix_data(5, 6);
    ledger
        .land_all(
            2,
            vec![
                ledger.enroll(OWNER, &first, true),
                ledger.enroll(OWNER, &rotation, false),
            ],
        )
        .await
        .unwrap();
    assert_eq!(ledger.state().await, before);
    assert_eq!(ledger.bounds().await, QueueBounds { head: 0, tail: 2 });
    let queued = ledger.store().queued(0, 8).await.unwrap();
    let shape: Vec<_> = queued
        .iter()
        .map(|row| (row.sequence, row.proven, row.ix))
        .collect();
    assert_eq!(shape, [(0, true, first), (1, false, rotation)]);
    ledger.stop().await;
}

/// A rotation ahead of the first enrollment changes nothing, the proven entry inserts the leaf, a later
/// rotation moves only its viewing key and the seal stays the inserting entry's.
#[tokio::test]
async fn a_late_registration_inserts_once_and_later_rotations_move_only_the_viewing_key() {
    let mut ledger = Ledger::new().await;
    let early = ix_data(5, 6);
    let first = ix_data(5, 3);
    ledger
        .land(2, ledger.enroll(OWNER, &early, false))
        .await
        .unwrap();
    ledger
        .land(3, ledger.enroll(OWNER, &first, true))
        .await
        .unwrap();
    // 1. Queued entries grant nothing, the owner stays absent under the root.
    assert!(!ledger.opening(3, OWNER).await.present);
    let mut tree = TrackingTree::new().unwrap();
    let queued = [entry(OWNER, &early, false), entry(OWNER, &first, true)];
    let apply = ledger.apply(&mut tree, &queued).await;
    ledger.land(4, apply).await.unwrap();
    let state = ledger.state().await;
    assert_eq!(state.fault, None);
    assert_eq!(state.account.newest().hash, tree.root().unwrap());
    assert_eq!(state.account.next_index(), 2);
    assert_eq!(ledger.bounds().await, QueueBounds { head: 2, tail: 2 });
    // 2. The leaf holds the proven entry's key, its seal and its viewing key.
    let inserted = ledger.opening(4, OWNER).await;
    assert!(inserted.present);
    let seal = inserted.record.seal.clone().unwrap();
    assert_eq!(seal.viewing_u.0, field(3));
    assert_eq!(seal.ciphertext.0, [5; 64]);
    assert_eq!(seal.eph_pk.0[1..], field(4));
    assert_eq!(seal.owner, OWNER.into());
    assert_eq!(
        inserted.record.viewing_hash,
        Hash(hash_bytes(&field(3)).unwrap())
    );
    // 3. A rotation with other sealed fields moves only the viewing key, the seal stays the first one.
    let rotated = ix_data(8, 7);
    ledger
        .land(5, ledger.enroll(OWNER, &rotated, false))
        .await
        .unwrap();
    let apply = ledger
        .apply(&mut tree, &[entry(OWNER, &rotated, false)])
        .await;
    ledger.land(6, apply).await.unwrap();
    assert_eq!(ledger.state().await.fault, None);
    let updated = ledger.opening(6, OWNER).await;
    assert_eq!(updated.record.nullifier_pk, inserted.record.nullifier_pk);
    assert_eq!(updated.record.ct_hash, inserted.record.ct_hash);
    assert_eq!(
        updated.record.viewing_hash,
        Hash(hash_bytes(&field(7)).unwrap())
    );
    let seal = updated.record.seal.unwrap();
    assert_eq!(seal.viewing_u.0, field(7));
    assert_eq!(seal.ciphertext.0, [5; 64]);
    ledger.stop().await;
}

/// The queue applies in order, a second proven entry of one owner is no refusal and updates its key.
#[tokio::test]
async fn a_second_proven_enrollment_applies_as_a_viewing_key_update() {
    let mut ledger = Ledger::new().await;
    let (first, second) = (ix_data(5, 3), ix_data(8, 7));
    ledger
        .land_all(
            2,
            vec![
                ledger.enroll(OWNER, &first, true),
                ledger.enroll(OWNER, &second, true),
            ],
        )
        .await
        .unwrap();
    let mut tree = TrackingTree::new().unwrap();
    let entries = [entry(OWNER, &first, true), entry(OWNER, &second, true)];
    let apply = ledger.apply(&mut tree, &entries).await;
    ledger.land(3, apply).await.unwrap();
    assert_eq!(ledger.state().await.fault, None);
    let leaf = ledger.opening(3, OWNER).await.record;
    assert_eq!(leaf.ct_hash, Hash(entries[0].ct_hash));
    assert_eq!(leaf.viewing_hash, Hash(entries[1].viewing_hash));
    assert_eq!(leaf.seal.unwrap().ciphertext.0, [5; 64]);
    ledger.stop().await;
}

#[tokio::test]
async fn a_root_off_the_queued_entries_stops_the_projection_and_consumes_nothing() {
    let mut ledger = Ledger::new().await;
    let first = ix_data(5, 3);
    ledger
        .land(2, ledger.enroll(OWNER, &first, true))
        .await
        .unwrap();
    let forged = TrackingEntry {
        ct_hash: field(8),
        ..entry(OWNER, &first, true)
    };
    let mut tree = TrackingTree::new().unwrap();
    let apply = ledger.apply(&mut tree, &[forged]).await;
    ledger.land(3, apply).await.unwrap();
    let fault = ledger.fault().await;
    assert!(fault.contains("applied root mismatch"), "{fault}");
    assert_eq!(ledger.bounds().await, QueueBounds { head: 0, tail: 1 });
    ledger.stop().await;
}

/// Each refusal the program raises means the projection drifted from a landed transaction.
#[tokio::test]
async fn an_apply_the_program_refuses_stops_the_projection() {
    for (count, reason) in [
        (0, "apply refused"),
        (2, "more entries than the queue holds"),
    ] {
        let mut ledger = Ledger::new().await;
        ledger
            .land(2, ledger.enroll(OWNER, &ix_data(5, 3), true))
            .await
            .unwrap();
        let apply = ledger.apply_data(ApplyEnrollmentsIxData {
            revision: Revision::default(),
            proof: Zeroable::zeroed(),
            new_root: field(8),
            new_next_index: 2u64.to_le_bytes(),
            count,
        });
        ledger.land(3, apply).await.unwrap();
        let fault = ledger.fault().await;
        assert!(fault.contains(reason), "{fault}");
        ledger.stop().await;
    }
}

#[tokio::test]
async fn an_enrollment_the_program_refuses_stops_the_projection() {
    let mut ledger = Ledger::new().await;
    let zero_viewing_key = ix_data(5, 0);
    ledger
        .land(2, ledger.enroll(OWNER, &zero_viewing_key, true))
        .await
        .unwrap();
    let fault = ledger.fault().await;
    assert!(fault.contains("enrollment refused"), "{fault}");
    ledger.stop().await;
}

#[tokio::test]
async fn an_enrollment_past_a_full_queue_stops_the_projection() {
    let mut ledger = Ledger::new().await;
    let enroll = ledger.enroll(OWNER, &ix_data(5, 3), false);
    let full = vec![enroll; ENROLLMENT_QUEUE_CAPACITY + 1];
    ledger.land_all(2, full).await.unwrap();
    let fault = ledger.fault().await;
    assert!(fault.contains("queue is full"), "{fault}");
    ledger.stop().await;
}

#[tokio::test]
async fn a_rolled_back_apply_restores_the_head_and_replays_to_the_same_root() {
    let mut ledger = Ledger::new().await;
    let first = ix_data(5, 3);
    ledger
        .land(2, ledger.enroll(OWNER, &first, true))
        .await
        .unwrap();
    let mut tree = TrackingTree::new().unwrap();
    let apply = ledger.apply(&mut tree, &[entry(OWNER, &first, true)]).await;
    ledger.land(3, apply.clone()).await.unwrap();
    let applied = ledger.state().await;
    ledger.rewind().await;
    assert_eq!(
        ledger.state().await.account.newest().hash,
        TRACKING_EMPTY_ROOT
    );
    assert_eq!(ledger.bounds().await, QueueBounds { head: 0, tail: 1 });
    ledger.land(3, apply).await.unwrap();
    assert_eq!(ledger.state().await, applied);
    ledger.rewind().await;
    ledger.rewind().await;
    assert_eq!(ledger.bounds().await, QueueBounds::default());
    ledger.stop().await;
}

#[tokio::test]
async fn the_enrollment_queue_pages_from_the_head_under_the_queue_account() {
    let mut ledger = Ledger::new().await;
    let owners = [OWNER, Address::new_from_array([52; 32])];
    let enrolls = owners
        .iter()
        .map(|owner| ledger.enroll(*owner, &ix_data(5, 3), true))
        .collect();
    ledger.land_all(2, enrolls).await.unwrap();
    ledger.serve(2).await;
    let request = GetEnrollmentQueueRequest {
        registry: ledger.registry.into(),
        after: None,
        limit: 1,
    };
    let read = |request| {
        api::enrollment_queue(
            ledger.indexer.db.as_ref(),
            ledger.indexer.rpc.as_ref(),
            request,
        )
    };
    let page = read(request.clone()).await.unwrap();
    assert_eq!((page.head, page.tail), (0, 2));
    let [row] = page.entries.as_slice() else {
        panic!("one entry per page, {:?}", page.entries);
    };
    assert_eq!(
        (row.sequence, row.owner, row.proven),
        (0, OWNER.into(), true)
    );
    let next = read(GetEnrollmentQueueRequest {
        after: Some(0),
        ..request.clone()
    })
    .await
    .unwrap();
    assert_eq!(next.entries[0].owner, owners[1].into());
    let unbounded = GetEnrollmentQueueRequest {
        limit: 0,
        ..request.clone()
    };
    assert!(matches!(
        read(unbounded).await,
        Err(RingApiError::InvalidRequest(_))
    ));
    ledger.hold_queue(QueueBounds { head: 0, tail: 3 });
    assert!(matches!(
        read(request).await,
        Err(RingApiError::RootChanged)
    ));
    ledger.stop().await;
}

#[tokio::test]
async fn apply_proof_inputs_consume_the_head_and_reach_the_root_the_program_records() {
    let mut ledger = Ledger::new().await;
    let first = ix_data(5, 3);
    ledger
        .land(2, ledger.enroll(OWNER, &first, true))
        .await
        .unwrap();
    ledger.serve(2).await;
    let (db, rpc) = (ledger.indexer.db.clone(), ledger.indexer.rpc.clone());
    let registry = ledger.registry;
    let request = |count| GetTrackingApplyProofInputsRequest {
        registry: registry.into(),
        count,
    };
    for count in [0, 2] {
        assert!(matches!(
            api::tracking_apply_proof_inputs(db.as_ref(), rpc.as_ref(), request(count)).await,
            Err(RingApiError::InvalidRequest(_))
        ));
    }
    let inputs = api::tracking_apply_proof_inputs(db.as_ref(), rpc.as_ref(), request(1))
        .await
        .unwrap();
    let mut tree = TrackingTree::new().unwrap();
    let apply = ledger.apply(&mut tree, &[entry(OWNER, &first, true)]).await;
    assert_eq!(inputs.old_root, Hash(TRACKING_EMPTY_ROOT));
    assert_eq!(inputs.new_root, Hash(tree.root().unwrap()));
    let [write] = inputs.writes.as_slice() else {
        panic!("one entry writes once");
    };
    assert_eq!(
        (write.sequence, write.proven, write.present),
        (0, true, false)
    );
    ledger.land(3, apply).await.unwrap();
    ledger.serve(3).await;
    assert!(matches!(
        api::tracking_apply_proof_inputs(db.as_ref(), rpc.as_ref(), request(1)).await,
        Err(RingApiError::InvalidRequest(_))
    ));
    ledger.stop().await;
}

#[tokio::test]
async fn every_tracking_read_serves_the_leaf_key_and_seal() {
    let mut ledger = Ledger::new().await;
    let first = ix_data(5, 3);
    ledger
        .land(2, ledger.enroll(OWNER, &first, true))
        .await
        .unwrap();
    let mut tree = TrackingTree::new().unwrap();
    let apply = ledger.apply(&mut tree, &[entry(OWNER, &first, true)]).await;
    ledger.land(3, apply).await.unwrap();
    let opened = ledger.opening(3, OWNER).await;
    let (db, rpc) = (ledger.indexer.db.as_ref(), ledger.indexer.rpc.as_ref());
    let registry = ledger.registry.into();
    let root = Hash(tree.root().unwrap());
    let page = api::tracking_records(
        db,
        rpc,
        GetTrackingRecordsRequest {
            registry,
            expected_root: root.clone(),
            start_index: 1,
            limit: 16,
        },
    )
    .await
    .unwrap();
    assert_eq!(page.records[0].record, opened.record);
    let pool = api::tracking_record_pool(
        db,
        rpc,
        GetTrackingRecordPoolRequest {
            registry,
            members: vec![opened.record.member.clone()],
            expected_root: root,
            openings: 4,
        },
    )
    .await
    .unwrap();
    assert_eq!(pool.records[0].record, opened.record);
    ledger.stop().await;
}

#[tokio::test]
async fn a_registry_off_its_canonical_address_is_not_indexed() {
    let mut ledger = Ledger::new().await;
    let forged = Address::new_from_array([70; 32]);
    let data = InitializeTrackingIxData {
        ring_id: field(9),
        auditor_pubkey: [9; 33],
    };
    let creator = Address::new_from_array([50; 32]);
    let initialize = instruction(
        vec![creator, creator, forged],
        [&[tag::INITIALIZE_TRACKING][..], bytemuck::bytes_of(&data)].concat(),
    );
    ledger.land(2, initialize).await.unwrap();
    let store: TrackingStore<'_, _> =
        TrackingStore::new(ledger.indexer.db.as_ref(), forged.to_bytes());
    assert!(store.state().await.unwrap().is_none());
    ledger.stop().await;
}

/// An inner `Enroll` queues as an outer one does, a failed transaction queues nothing.
#[tokio::test]
async fn a_program_owner_enrolls_through_its_own_cross_program_call_and_a_failure_queues_nothing() {
    let mut ledger = Ledger::new().await;
    let enroll = ledger.enroll(OWNER, &ix_data(5, 3), true);
    let failed = TransactionInfo {
        error: Some("custom program error".into()),
        ..settled(enroll.clone(), vec![])
    };
    let caller = Instruction {
        program_id: Address::new_from_array([61; 32]),
        data: vec![1],
        accounts: vec![],
        stack_height: Some(1),
    };
    ledger
        .land_transactions(2, vec![failed, settled(caller, vec![enroll])])
        .await
        .unwrap();
    assert_eq!(ledger.bounds().await, QueueBounds { head: 0, tail: 1 });
    ledger.stop().await;
}

/// Serves the scan straight from the indexer's own `getTrackingRecords` handler.
struct Endpoint<'a>(&'a Ledger);

impl zolana_ring_rpc::TrackingSource for Endpoint<'_> {
    async fn root(
        &self,
        _ring: Address,
        _finality: zolana_ring_rpc::Finality,
    ) -> Result<Option<zolana_ring_rpc::TrackingRoot>, spr_client::ClientError> {
        let newest = self.0.state().await.account.newest();
        Ok(Some(zolana_ring_rpc::TrackingRoot {
            registry: self.0.registry,
            root: newest.hash,
            revision: newest.revision.value(),
            next_index: self.0.state().await.account.next_index(),
        }))
    }

    async fn records(
        &self,
        request: GetTrackingRecordsRequest,
    ) -> Result<GetTrackingRecordsResponse, zolana_ring_client::RingIndexerError> {
        let indexer = &self.0.indexer;
        api::tracking_records(indexer.db.as_ref(), indexer.rpc.as_ref(), request)
            .await
            .map_err(|error| spr_client::ClientError::Rpc(error.to_string()).into())
    }
}

/// A first enrollment of the key `seed` derives, sealed to `auditor`.
fn sealed(seed: u8, auditor: &spr_keypair::X25519Pubkey) -> EnrollIxData {
    let key = spr_keypair::NullifierKey::from_secret([seed; 31]);
    let mut list_salt = [seed; 32];
    list_salt[0] = 0;
    let sealed = zolana_ring_client::NullifierKeyEnvelope::new(&key, &list_salt, auditor)
        .unwrap()
        .sealed;
    EnrollIxData {
        nullifier_pk: key.pubkey().unwrap(),
        list_tag: custom_ring_interface::domain::list_tag(&list_salt).unwrap(),
        viewing_u: field(seed),
        eph_u: sealed.eph_pk.u(),
        ciphertext: sealed.ciphertext,
    }
}

#[tokio::test]
async fn a_fresh_auditor_scans_the_indexer_endpoint_from_the_first_leaf_and_holds_every_owner() {
    let mut ledger = Ledger::new().await;
    let auditor = spr_keypair::X25519Keypair::new();
    let owners = [
        Address::new_from_array([61; 32]),
        Address::new_from_array([62; 32]),
    ];
    let mut entries = Vec::new();
    for (slot, owner) in (2..).zip(owners) {
        let ix = sealed(slot as u8, &auditor.pubkey());
        ledger
            .land(slot, ledger.enroll(owner, &ix, true))
            .await
            .unwrap();
        entries.push(entry(owner, &ix, true));
    }
    let mut tree = TrackingTree::new().unwrap();
    let apply = ledger.apply(&mut tree, &entries).await;
    ledger.land(4, apply).await.unwrap();
    ledger.serve(4).await;

    let held = zolana_ring_rpc::tracking_fixture::fresh_scan(
        Address::new_from_array([60; 32]),
        &auditor,
        &Endpoint(&ledger),
        &owners,
    )
    .await
    .unwrap();
    for (owner, held) in owners.iter().zip(&held) {
        let member = zolana_ring_policy::Member::owner_tag(owner.as_array()).unwrap();
        assert_eq!(held.map(|held| held.member()), Some(*member.as_bytes()));
    }
    ledger.stop().await;
}
