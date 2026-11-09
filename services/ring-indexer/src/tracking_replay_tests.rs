use bytemuck::Zeroable;
use custom_ring_interface::{
    storage_instruction::{
        tag, EnrollIxData, EnrollmentChange, EnrollmentWrite, EnrollmentsAppliedEvent,
        InitializeTrackingIxData, StorageEvent,
    },
    tracking::{
        tracking_chain_start, viewing_hash, TrackingEntry, TrackingRegistry, TrackingRootUpdate,
        TRACKING_EMPTY_ROOT,
    },
    CustomRingProof, STORAGE_PROGRAM_ID, VIEWING_PUBKEY_TAG,
};
use sea_orm::TransactionTrait;
use solana_address::Address;
use solana_signature::Signature;
use spr_indexer_api::Hash;
use zolana_ring_indexer::tracking::{
    GetTrackingApplyProofInputsRequest, GetTrackingOpeningsRequest, GetTrackingRecordPoolRequest,
    GetTrackingRecordsRequest, GetTrackingRecordsResponse, RingTrackingEntryView,
    RingTrackingOpening,
};
use zolana_ring_member_registry::{RecordTree, TrackingTree};

use super::{account, block, replay_indexer, rpc, Chain};
use crate::{
    api,
    block::{BlockInfo, BlockMetadata, Instruction, InstructionGroup, TransactionInfo},
    error::RingApiError,
    indexer::Indexer,
    storage::{self, RingCursor, TrackingState, TrackingStore},
    tests::{database, field},
};

const OWNER: Address = Address::new_from_array([51; 32]);
const WRITER: Address = Address::new_from_array([49; 32]);
const NEWEST_SLOT: u64 = 5_000;

struct Ledger {
    indexer: Indexer,
    cursor: RingCursor,
    chain: std::sync::Arc<std::sync::Mutex<Chain>>,
    handle: jsonrpsee::server::ServerHandle,
    registry: Address,
}

/// Zero, storage verified the proof before the event the projection reads.
static PROOF: std::sync::LazyLock<CustomRingProof> =
    std::sync::LazyLock::new(CustomRingProof::zeroed);

/// One write as the test owns it, borrowed into an `EnrollmentWrite` for the codec.
#[derive(Clone, Copy)]
enum Write {
    Enroll(Address, EnrollIxData),
    Rotate {
        owner: Address,
        viewing_u: [u8; 32],
        prior_viewing_u: [u8; 32],
        prior_seq: [u8; 8],
    },
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

fn instruction(
    program_id: Address,
    accounts: Vec<Address>,
    data: Vec<u8>,
    height: u32,
) -> Instruction {
    Instruction {
        program_id,
        data,
        accounts,
        stack_height: Some(height),
    }
}

impl Write {
    fn borrowed(&self) -> EnrollmentWrite<'_> {
        match self {
            Self::Enroll(owner, ix) => EnrollmentWrite {
                owner,
                change: EnrollmentChange::Enroll { ix, proof: &PROOF },
            },
            Self::Rotate {
                owner,
                viewing_u,
                prior_viewing_u,
                prior_seq,
            } => EnrollmentWrite {
                owner,
                change: EnrollmentChange::Rotate {
                    viewing_u,
                    prior_viewing_u,
                    prior_seq,
                },
            },
        }
    }

    fn entry(&self) -> TrackingEntry {
        self.borrowed().entry().unwrap()
    }
}

impl Ledger {
    async fn new() -> Self {
        let creator = Address::new_from_array([50; 32]);
        let ring_id = field(9);
        let (registry, _) = TrackingRegistry::find_address(&creator, &ring_id);
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
        };
        let initialize = instruction(
            STORAGE_PROGRAM_ID,
            vec![creator, creator, registry, Address::default()],
            [&[tag::INITIALIZE_TRACKING][..], bytemuck::bytes_of(&data)].concat(),
            1,
        );
        ledger
            .land_transactions(1, vec![settled(initialize, vec![])])
            .await
            .unwrap();
        ledger
    }

    /// The storage `ApplyEnrollments` an honest writer proves at the projected revision, with the event it emits.
    async fn apply(&self, tree: &mut TrackingTree, writes: &[Write]) -> (Instruction, Instruction) {
        let state = self.state().await;
        let revision = state.account.newest().revision;
        let chain_start = tracking_chain_start(&self.registry, revision).unwrap();
        let entries: Vec<_> = writes.iter().map(Write::entry).collect();
        let applied = tree.clone().apply(chain_start, &entries).unwrap();
        *tree = applied.tree;
        let mut next = *state.account;
        next.update(TrackingRootUpdate {
            revision,
            slot: 0,
            new_root: applied.proof.new_root,
            new_next_index: applied.proof.new_next_index,
        })
        .unwrap();
        self.applying(
            writes,
            EnrollmentsAppliedEvent {
                registry: self.registry,
                revision: next.newest().revision,
                new_root: applied.proof.new_root,
                new_next_index: applied.proof.new_next_index.to_le_bytes(),
                count: writes.len() as u8,
            },
        )
    }

    /// The parent and its event for `header`, nothing checked.
    fn applying(
        &self,
        writes: &[Write],
        header: EnrollmentsAppliedEvent,
    ) -> (Instruction, Instruction) {
        let borrowed: Vec<_> = writes.iter().map(Write::borrowed).collect();
        let parent = instruction(
            STORAGE_PROGRAM_ID,
            vec![WRITER, self.registry],
            vec![tag::APPLY_ENROLLMENTS],
            1,
        );
        let event = StorageEvent::EnrollmentsApplied {
            header,
            writes: borrowed,
        };
        let event = instruction(STORAGE_PROGRAM_ID, vec![], event.encode(), 2);
        (parent, event)
    }

    async fn land_apply(&mut self, slot: u64, (parent, event): (Instruction, Instruction)) {
        self.land_transactions(slot, vec![settled(parent, vec![event])])
            .await
            .unwrap();
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

    /// The chain confirms the projection at `slot`.
    async fn serve(&mut self, slot: u64) {
        let state = self.state().await;
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

    fn store(&self) -> TrackingStore<'_, sea_orm::DatabaseConnection> {
        TrackingStore::new(self.indexer.db.as_ref(), self.registry.to_bytes())
    }

    async fn state(&self) -> TrackingState {
        self.store().state().await.unwrap().unwrap()
    }

    async fn fault(&self) -> String {
        self.state().await.fault.expect("the projection stopped")
    }

    /// The owner's opening under the newest root, served as a sender reads it.
    async fn opening(&mut self, slot: u64, owner: Address) -> RingTrackingOpening {
        self.serve(slot).await;
        let member = Write::Rotate {
            owner,
            viewing_u: field(3),
            prior_viewing_u: [0; 32],
            prior_seq: [0; 8],
        }
        .entry()
        .member;
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

/// A rotation ahead of the first enrollment changes nothing, the Enroll inserts the leaf, a later
/// rotation moves only its viewing key and the seal stays the inserting write's.
#[tokio::test]
async fn a_late_registration_inserts_once_and_later_rotations_move_only_the_viewing_key() {
    let mut ledger = Ledger::new().await;
    assert!(!ledger.opening(1, OWNER).await.present);
    let mut tree = TrackingTree::new().unwrap();
    let batch = [
        Write::Rotate {
            owner: OWNER,
            viewing_u: field(6),
            prior_viewing_u: [0; 32],
            prior_seq: [0; 8],
        },
        Write::Enroll(OWNER, ix_data(5, 3)),
    ];
    let apply = ledger.apply(&mut tree, &batch).await;
    ledger.land_apply(2, apply).await;
    let state = ledger.state().await;
    assert_eq!(state.fault, None);
    assert_eq!(state.account.newest().hash, tree.root().unwrap());
    assert_eq!(state.account.next_index(), 2);
    let inserted = ledger.opening(2, OWNER).await;
    assert!(inserted.present);
    let seal = inserted.record.seal.clone().unwrap();
    assert_eq!(seal.viewing_u.0, field(3));
    assert_eq!(seal.seq, 0);
    assert_eq!(seal.ciphertext.0, [5; 64]);
    assert_eq!(seal.eph_pk.0[1..], field(4));
    assert_eq!(seal.owner, OWNER.into());
    let apply = ledger
        .apply(
            &mut tree,
            &[Write::Rotate {
                owner: OWNER,
                viewing_u: field(7),
                prior_viewing_u: field(3),
                prior_seq: [0; 8],
            }],
        )
        .await;
    ledger.land_apply(3, apply).await;
    assert_eq!(ledger.state().await.fault, None);
    let updated = ledger.opening(3, OWNER).await;
    assert_eq!(updated.record.ct_hash, inserted.record.ct_hash);
    assert_eq!(
        updated.record.viewing_hash,
        Hash(viewing_hash(&field(7), 1).unwrap())
    );
    let seal = updated.record.seal.unwrap();
    assert_eq!(seal.viewing_u.0, field(7));
    assert_eq!(seal.seq, 1);
    assert_eq!(seal.ciphertext.0, [5; 64]);
    // A returning key takes the next counter, its leaf hash differs from its first.
    let apply = ledger
        .apply(
            &mut tree,
            &[Write::Rotate {
                owner: OWNER,
                viewing_u: field(3),
                prior_viewing_u: field(7),
                prior_seq: 1u64.to_le_bytes(),
            }],
        )
        .await;
    ledger.land_apply(4, apply).await;
    assert_eq!(ledger.state().await.fault, None);
    let returned = ledger.opening(4, OWNER).await;
    assert_eq!(
        returned.record.viewing_hash,
        Hash(viewing_hash(&field(3), 2).unwrap())
    );
    assert_ne!(returned.record.viewing_hash, inserted.record.viewing_hash);
    assert_eq!(returned.record.seal.unwrap().seq, 2);
    ledger.stop().await;
}

/// The rule the circuit and the projection share refuses a repeated Enroll and a Rotate from a stale key.
#[test]
fn a_repeated_enrollment_and_a_stale_rotation_do_not_apply() {
    let first = Write::Enroll(OWNER, ix_data(5, 3)).entry();
    for second in [
        Write::Enroll(OWNER, ix_data(8, 7)),
        Write::Rotate {
            owner: OWNER,
            viewing_u: field(7),
            prior_viewing_u: field(6),
            prior_seq: [0; 8],
        },
    ] {
        let applied = TrackingTree::new()
            .unwrap()
            .apply([0; 32], &[first, second.entry()]);
        assert!(applied.is_err());
    }
}

#[tokio::test]
async fn an_event_off_the_projected_root_or_revision_stops_the_projection() {
    for (forge, reason) in [
        (0, "applied root mismatch"),
        (1, "another revision"),
        (2, "registry"),
    ] {
        let mut ledger = Ledger::new().await;
        let writes = [Write::Enroll(OWNER, ix_data(5, 3))];
        let (parent, event) = ledger
            .apply(&mut TrackingTree::new().unwrap(), &writes)
            .await;
        let StorageEvent::EnrollmentsApplied { mut header, .. } =
            StorageEvent::decode(&event.data).unwrap();
        match forge {
            0 => header.new_root = field(8),
            1 => header.revision.0[0] ^= 1,
            _ => header.registry = Address::new_from_array([70; 32]),
        }
        let (_, forged) = ledger.applying(&writes, header);
        ledger.land_apply(2, (parent, forged)).await;
        let stopped = match forge {
            2 => TrackingStore::new(ledger.indexer.db.as_ref(), [70; 32])
                .state()
                .await
                .unwrap()
                .is_none(),
            _ => ledger.fault().await.contains(reason),
        };
        assert!(stopped, "forge {forge}");
        assert_eq!(
            ledger.state().await.account.newest().hash,
            TRACKING_EMPTY_ROOT
        );
        ledger.stop().await;
    }
}

/// `tag::EMIT_EVENT` bytes count only as the direct child of a storage `ApplyEnrollments`.
#[tokio::test]
async fn an_event_under_another_parent_or_in_a_failed_transaction_is_ignored() {
    let mut ledger = Ledger::new().await;
    let writes = [Write::Enroll(OWNER, ix_data(5, 3))];
    let (parent, event) = ledger
        .apply(&mut TrackingTree::new().unwrap(), &writes)
        .await;
    let other_program = Instruction {
        program_id: Address::new_from_array([61; 32]),
        ..parent.clone()
    };
    let other_tag = Instruction {
        data: vec![tag::UPDATE],
        ..parent.clone()
    };
    let outer_event = Instruction {
        stack_height: Some(1),
        ..event.clone()
    };
    let failed = TransactionInfo {
        error: Some("custom program error".into()),
        ..settled(parent, vec![event.clone()])
    };
    ledger
        .land_transactions(
            2,
            vec![
                settled(other_program, vec![event.clone()]),
                settled(other_tag, vec![event]),
                settled(outer_event, vec![]),
                failed,
            ],
        )
        .await
        .unwrap();
    let state = ledger.state().await;
    assert_eq!(state.fault, None);
    assert_eq!(state.account.newest().hash, TRACKING_EMPTY_ROOT);
    ledger.stop().await;
}

#[tokio::test]
async fn a_rolled_back_batch_replays_to_the_same_root() {
    let mut ledger = Ledger::new().await;
    let mut tree = TrackingTree::new().unwrap();
    let apply = ledger
        .apply(&mut tree, &[Write::Enroll(OWNER, ix_data(5, 3))])
        .await;
    ledger.land_apply(2, apply.clone()).await;
    let applied = ledger.state().await;
    ledger.rewind().await;
    assert_eq!(
        ledger.state().await.account.newest().hash,
        TRACKING_EMPTY_ROOT
    );
    ledger.land_apply(2, apply).await;
    assert_eq!(ledger.state().await, applied);
    ledger.stop().await;
}

#[tokio::test]
async fn apply_proof_inputs_take_the_writer_entries_and_reach_the_root_the_program_records() {
    let mut ledger = Ledger::new().await;
    ledger.serve(1).await;
    let (db, rpc) = (ledger.indexer.db.clone(), ledger.indexer.rpc.clone());
    let registry = ledger.registry;
    let write = Write::Enroll(OWNER, ix_data(5, 3));
    let view = |entry: TrackingEntry| RingTrackingEntryView {
        member: Hash(entry.member),
        nullifier_pk: Hash(entry.nullifier_pk),
        ct_hash: Hash(entry.ct_hash),
        list_tag: Hash(entry.list_tag),
        viewing_hash: Hash(entry.viewing_hash),
        prior_viewing_hash: Hash(entry.prior_viewing_hash),
        proven: entry.proven,
    };
    let request = |entries: Vec<RingTrackingEntryView>| GetTrackingApplyProofInputsRequest {
        registry: registry.into(),
        entries,
    };
    for count in [0, 6] {
        assert!(matches!(
            api::tracking_apply_proof_inputs(
                db.as_ref(),
                rpc.as_ref(),
                request(vec![view(write.entry()); count])
            )
            .await,
            Err(RingApiError::InvalidRequest(_))
        ));
    }
    let inputs = api::tracking_apply_proof_inputs(
        db.as_ref(),
        rpc.as_ref(),
        request(vec![view(write.entry())]),
    )
    .await
    .unwrap();
    let mut tree = TrackingTree::new().unwrap();
    let apply = ledger.apply(&mut tree, &[write]).await;
    assert_eq!(inputs.old_root, Hash(TRACKING_EMPTY_ROOT));
    assert_eq!(inputs.new_root, Hash(tree.root().unwrap()));
    let [served] = inputs.writes.as_slice() else {
        panic!("one entry writes once");
    };
    assert_eq!((served.proven, served.present), (true, false));
    ledger.land_apply(2, apply).await;
    ledger.stop().await;
}

#[tokio::test]
async fn every_tracking_read_serves_the_leaf_key_and_seal() {
    let mut ledger = Ledger::new().await;
    let mut tree = TrackingTree::new().unwrap();
    let apply = ledger
        .apply(&mut tree, &[Write::Enroll(OWNER, ix_data(5, 3))])
        .await;
    ledger.land_apply(2, apply).await;
    let opened = ledger.opening(2, OWNER).await;
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
        STORAGE_PROGRAM_ID,
        vec![creator, creator, forged],
        [&[tag::INITIALIZE_TRACKING][..], bytemuck::bytes_of(&data)].concat(),
        1,
    );
    ledger
        .land_transactions(2, vec![settled(initialize, vec![])])
        .await
        .unwrap();
    let store: TrackingStore<'_, _> =
        TrackingStore::new(ledger.indexer.db.as_ref(), forged.to_bytes());
    assert!(store.state().await.unwrap().is_none());
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
    let writes: Vec<_> = (2..)
        .zip(owners)
        .map(|(seed, owner)| Write::Enroll(owner, sealed(seed, &auditor.pubkey())))
        .collect();
    let mut tree = TrackingTree::new().unwrap();
    let apply = ledger.apply(&mut tree, &writes).await;
    ledger.land_apply(2, apply).await;
    ledger.serve(2).await;

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
