use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

use async_trait::async_trait;
use bytemuck::Zeroable;
use custom_ring_interface::{
    storage::{Revision, StorageRegistry},
    storage_instruction::EnrollIxData,
    tracking::viewing_hash,
    ApplyRecordsIxData, CustomRingProof, RING_POLICY, STORAGE_PROGRAM_ID,
};
use custom_ring_sdk::{OwnedChange, OwnerEnrollment};
use solana_account::Account;
use solana_instruction::Instruction;
use solana_message::VersionedMessage;
use solana_signature::Signature;
use solana_transaction::versioned::VersionedTransaction;
use spr_keypair::X25519Keypair;
use zolana_ring_client::{SealedNullifierKey, TrackingLeaf};
use zolana_ring_indexer::rpc::{
    GetStorageApplyProofInputsRequest, GetStorageApplyProofInputsResponse,
    GetStorageMemberOpeningsRequest, GetStorageMemberOpeningsResponse,
};
use zolana_ring_member_registry::{ApplyProofInputs, TrackingRecord};

use super::*;
use crate::{
    landing::{ChainSubmit, LatestBlockhash, Simulation},
    upstream::RingConfiguration,
};

const RING: Address = Address::new_from_array([1; 32]);
const REGISTRY: Address = Address::new_from_array([2; 32]);
const UNINITIALIZED_RING: Address = Address::new_from_array([11; 32]);
const SLOT: u64 = 1_000;

#[derive(Default)]
struct StubChain {
    slot: std::sync::atomic::AtomicU64,
    accounts: Mutex<HashMap<Address, Account>>,
    proofs: AtomicUsize,
    /// Each proof's registry and write count.
    proved: Mutex<Vec<(Address, usize)>>,
    sent: Mutex<Vec<VersionedTransaction>>,
    outcome: Mutex<Option<ChainOutcome>>,
    during_proof: Mutex<Option<DuringProof>>,
    /// Owner to the viewing hash its leaf holds.
    leaves: Mutex<HashMap<[u8; 32], [u8; 32]>>,
}

type DuringProof = Box<dyn Fn(&StubChain) + Send + Sync>;

fn stored(owner: Address, data: &[u8]) -> Account {
    Account {
        lamports: 1,
        data: data.to_vec(),
        owner,
        executable: false,
        rent_epoch: 0,
    }
}

impl StubChain {
    fn new() -> Arc<Self> {
        let chain = Self::default();
        chain.slot.store(SLOT, Ordering::Relaxed);
        let mut policy = RingPolicy::zeroed();
        policy.discriminator = RING_POLICY;
        policy.tracking = REGISTRY;
        chain.accounts.lock().unwrap().insert(
            CustomRing::new(RING).policy_pda(),
            stored(RING, bytemuck::bytes_of(&policy)),
        );
        chain.tracking(|_| {});
        Arc::new(chain)
    }

    fn tracking(&self, write: impl FnOnce(&mut TrackingRegistry)) {
        let mut accounts = self.accounts.lock().unwrap();
        let account = accounts.entry(REGISTRY).or_insert_with(|| {
            let mut registry = TrackingRegistry::zeroed();
            registry.discriminator = TrackingRegistry::DISCRIMINATOR;
            registry.next_index = 1u64.to_le_bytes();
            stored(STORAGE_PROGRAM_ID, bytemuck::bytes_of(&registry))
        });
        write(bytemuck::from_bytes_mut(&mut account.data));
    }

    /// Each owner holds a leaf at the viewing hash its rotation replaces.
    fn enrolled(&self, seeds: impl IntoIterator<Item = u8>) {
        let mut leaves = self.leaves.lock().unwrap();
        for seed in seeds {
            leaves.insert(owner(seed).pubkey().to_bytes(), prior_viewing_hash(seed));
        }
    }

    fn proved(&self) -> Vec<(Address, usize)> {
        self.proved.lock().unwrap().clone()
    }

    fn landed(&self, outcome: ChainOutcome) {
        *self.outcome.lock().unwrap() = Some(outcome);
    }
}

#[async_trait]
impl ChainSubmit for StubChain {
    async fn account_data(
        &self,
        _address: solana_address::Address,
    ) -> Result<Option<Vec<u8>>, RingRpcError> {
        Ok(None)
    }

    async fn rent_exempt_lamports(&self, _data_len: usize) -> Result<u64, RingRpcError> {
        Ok(0)
    }

    async fn slot(&self) -> Result<u64, RingRpcError> {
        Ok(self.slot.load(Ordering::Relaxed))
    }

    async fn latest_blockhash(&self) -> Result<LatestBlockhash, RingRpcError> {
        Ok(LatestBlockhash {
            blockhash: solana_hash::Hash::new_from_array([7; 32]),
            last_valid_block_height: u64::MAX,
        })
    }

    async fn block_height(&self) -> Result<u64, RingRpcError> {
        Ok(0)
    }

    async fn send(&self, transaction: &VersionedTransaction) -> Result<(), RingRpcError> {
        self.sent.lock().unwrap().push(transaction.clone());
        Ok(())
    }

    async fn outcome(&self, _signature: Signature) -> Result<Option<ChainOutcome>, RingRpcError> {
        Ok(self.outcome.lock().unwrap().take())
    }

    async fn simulate(&self, _message: &VersionedMessage) -> Result<Simulation, RingRpcError> {
        unreachable!("an applier sends without a simulation")
    }
}

#[async_trait]
impl StorageChain for StubChain {
    async fn account(&self, address: Address) -> Result<Option<Account>, RingRpcError> {
        Ok(self.accounts.lock().unwrap().get(&address).cloned())
    }

    async fn ring_config(&self, _ring: Address) -> Result<Option<RingConfiguration>, RingRpcError> {
        unreachable!("an applier reads no ring role")
    }

    async fn member_openings(
        &self,
        _request: GetStorageMemberOpeningsRequest,
    ) -> Result<GetStorageMemberOpeningsResponse, RingRpcError> {
        unreachable!("an applier reads no policy leaf")
    }

    async fn apply_inputs(
        &self,
        _request: GetStorageApplyProofInputsRequest,
    ) -> Result<GetStorageApplyProofInputsResponse, RingRpcError> {
        unreachable!("an applier writes no policy leaf")
    }

    async fn prove_apply(
        &self,
        _inputs: &ApplyProofInputs,
        _registry: &StorageRegistry,
    ) -> Result<ApplyRecordsIxData, RingRpcError> {
        unreachable!("an applier writes no policy leaf")
    }

    async fn prove_enrollments(
        &self,
        batch: EnrollBatch<'_>,
    ) -> Result<ProvenEnrollBatch, RingRpcError> {
        self.proofs.fetch_add(1, Ordering::Relaxed);
        self.proved
            .lock()
            .unwrap()
            .push((batch.tracking_registry, batch.writes.len()));
        let revision = self
            .accounts
            .lock()
            .unwrap()
            .get(&batch.tracking_registry)
            .and_then(|account| TrackingRegistry::decode(&account.owner, &account.data))
            .expect("registry")
            .newest()
            .revision;
        if let Some(act) = self.during_proof.lock().unwrap().take() {
            act(self);
        }
        let instruction = |program_id| Instruction {
            program_id,
            accounts: Vec::new(),
            data: Vec::new(),
        };
        Ok(ProvenEnrollBatch {
            instructions: [instruction(STORAGE_PROGRAM_ID), instruction(RING)],
            revision,
        })
    }

    async fn tracking_leaves(
        &self,
        registry: Address,
        owners: &[[u8; 32]],
    ) -> Result<HashMap<[u8; 32], TrackingLeaf>, RingRpcError> {
        assert_eq!(registry, REGISTRY);
        let leaves = self.leaves.lock().unwrap();
        Ok(owners
            .iter()
            .filter_map(|owner| {
                let viewing_hash = *leaves.get(owner)?;
                let leaf = TrackingLeaf {
                    record: TrackingRecord {
                        viewing_hash,
                        ..TrackingRecord::default()
                    },
                    owner: Address::new_from_array(*owner),
                    viewing_u: [0; 32],
                    seq: 0,
                    seal: SealedNullifierKey {
                        eph_pk: X25519Keypair::from_bytes(&[1; 32]).pubkey(),
                        ciphertext: [0; 64],
                    },
                };
                Some((*owner, leaf))
            })
            .collect())
    }
}

fn applier(chain: &Arc<StubChain>) -> EnrollmentApplier {
    EnrollmentApplierParts {
        chain: chain.clone(),
        payer: Keypair::new_from_array([9; 32]),
        rings: vec![RING, UNINITIALIZED_RING],
    }
    .applier()
}

fn owner(seed: u8) -> Keypair {
    Keypair::new_from_array([seed; 32])
}

fn enrollment(owner: &Keypair, change: OwnedChange) -> OwnerEnrollment {
    OwnerEnrollment {
        ring: CustomRing::new(RING),
        tracking_registry: REGISTRY,
        owner: owner.pubkey(),
        change,
    }
}

/// A rotation needs no proof, the owner's signature alone authorizes it.
fn rotation(seed: u8) -> SubmitEnrollmentRequest {
    let viewing_u = X25519Keypair::from_bytes(&[seed; 32]).pubkey().u();
    let owner = owner(seed);
    let change = OwnedChange::Rotate {
        viewing_u,
        prior_viewing_u: [seed; 32],
        prior_seq: [0; 8],
    };
    enrollment(&owner, change).sign(&owner).expect("signed")
}

/// The viewing hash the owner's leaf holds before the rotation, a field element as every leaf hash is.
fn prior_viewing_hash(seed: u8) -> [u8; 32] {
    viewing_hash(&[seed; 32], 0).expect("viewing hash")
}

fn refusal(result: Result<SubmitEnrollmentResponse, RingRpcError>) -> EnrollmentRefusal {
    match result {
        Err(RingRpcError::Enrollment(refusal)) => refusal,
        other => panic!("an enrollment refusal, not {other:?}"),
    }
}

/// Settles the sent batch as failed, its writes requeue for the next pass.
fn refused(chain: &StubChain) {
    chain.landed(ChainOutcome::Failed {
        reason: String::new(),
    });
}

#[tokio::test]
async fn a_write_the_program_would_refuse_is_never_held() {
    let chain = StubChain::new();
    let applier = applier(&chain);
    let unserved = SubmitEnrollmentRequest {
        ring: Address::new_from_array([12; 32]).into(),
        ..rotation(3)
    };
    assert_eq!(
        refusal(applier.submit(unserved).await),
        EnrollmentRefusal::NotServed
    );
    let uninitialized = SubmitEnrollmentRequest {
        ring: UNINITIALIZED_RING.into(),
        ..rotation(3)
    };
    assert_eq!(
        refusal(applier.submit(uninitialized).await),
        EnrollmentRefusal::RingMissing
    );
    let mut truncated = rotation(3);
    truncated.write.0.pop();
    assert_eq!(
        refusal(applier.submit(truncated).await),
        EnrollmentRefusal::InvalidWrite
    );
    let forged = SubmitEnrollmentRequest {
        signature: rotation(4).signature,
        ..rotation(3)
    };
    assert_eq!(
        refusal(applier.submit(forged).await),
        EnrollmentRefusal::InvalidWrite
    );
    let owner = owner(5);
    let mut ix = EnrollIxData::zeroed();
    ix.viewing_u[31] = 9;
    ix.nullifier_pk[31] = 1;
    let unproven = enrollment(
        &owner,
        OwnedChange::Enroll {
            ix,
            proof: Box::new(CustomRingProof::zeroed()),
        },
    )
    .sign(&owner)
    .expect("signed");
    assert_eq!(
        refusal(applier.submit(unproven).await),
        EnrollmentRefusal::RegisterKeyProof
    );
    applier.flush().await;
    assert!(chain.proved().is_empty());
}

#[tokio::test]
async fn a_full_ring_refuses_and_a_resubmission_is_held_once() {
    let chain = StubChain::new();
    let applier = applier(&chain);
    applier.submit(rotation(0)).await.expect("held");
    applier.submit(rotation(0)).await.expect("held once");
    for seed in 1..MAX_HELD_ENROLLMENTS as u8 {
        applier.submit(rotation(seed)).await.expect("held");
    }
    assert_eq!(
        refusal(applier.submit(rotation(MAX_HELD_ENROLLMENTS as u8)).await),
        EnrollmentRefusal::Full
    );
}

#[tokio::test]
async fn a_pass_proves_one_batch_under_the_payer_alone_and_waits_for_its_landing() {
    let chain = StubChain::new();
    let applier = applier(&chain);
    chain.enrolled(0..ENROLLMENT_BATCH as u8 + 2);
    for seed in 0..ENROLLMENT_BATCH as u8 + 2 {
        applier.submit(rotation(seed)).await.expect("held");
    }

    applier.flush().await;
    assert_eq!(chain.proved(), [(REGISTRY, ENROLLMENT_BATCH)]);
    let sent = chain.sent.lock().unwrap().clone();
    let [transaction] = sent.as_slice() else {
        panic!("one batch, {sent:?}");
    };
    assert_eq!(transaction.signatures.len(), 1);
    assert_eq!(
        transaction.message.static_account_keys()[0],
        applier.payer()
    );

    applier.flush().await;
    assert_eq!(chain.proofs.load(Ordering::Relaxed), 1);

    chain.landed(ChainOutcome::Confirmed { slot: SLOT });
    applier.flush().await;
    assert_eq!(chain.proved()[1], (REGISTRY, 2));
}

#[tokio::test]
async fn a_failed_batch_requeues_its_writes_until_their_attempts_run_out() {
    let chain = StubChain::new();
    chain.enrolled([3]);
    let applier = applier(&chain);
    applier.submit(rotation(3)).await.expect("held");
    applier.flush().await;
    for attempt in 1..MAX_ENROLLMENT_ATTEMPTS {
        refused(&chain);
        applier.flush().await;
        assert_eq!(
            chain.proofs.load(Ordering::Relaxed),
            usize::from(attempt) + 1
        );
    }
    refused(&chain);
    applier.flush().await;
    assert_eq!(
        chain.proofs.load(Ordering::Relaxed),
        usize::from(MAX_ENROLLMENT_ATTEMPTS)
    );
}

#[tokio::test]
async fn a_pass_waits_out_the_grace_of_the_oldest_root() {
    let chain = StubChain::new();
    chain.tracking(|registry| {
        registry.roots[TRACKING_ROOT_HISTORY - 1].expires_at = (SLOT + 1).to_le_bytes()
    });
    chain.enrolled([3]);
    let applier = applier(&chain);
    applier.submit(rotation(3)).await.expect("held");
    applier.flush().await;
    assert!(chain.proved().is_empty());
}

#[tokio::test]
async fn a_root_moved_during_the_proof_sends_nothing() {
    let chain = StubChain::new();
    *chain.during_proof.lock().unwrap() = Some(Box::new(|chain: &StubChain| {
        chain.tracking(|registry| registry.roots[0].revision = Revision::from(1))
    }));
    chain.enrolled([3]);
    let applier = applier(&chain);
    applier.submit(rotation(3)).await.expect("held");

    applier.flush().await;
    assert!(chain.sent.lock().unwrap().is_empty());

    applier.flush().await;
    assert_eq!(chain.sent.lock().unwrap().len(), 1);
}

/// A Rotate of an absent owner or from another viewing hash would fail the whole batch.
#[tokio::test]
async fn a_pass_drops_a_rotation_the_running_leaves_refuse() {
    let chain = StubChain::new();
    chain.enrolled([3]);
    chain
        .leaves
        .lock()
        .unwrap()
        .insert(owner(4).pubkey().to_bytes(), [0x44; 32]);
    let applier = applier(&chain);
    for seed in [3, 4, 5] {
        applier.submit(rotation(seed)).await.expect("held");
    }
    applier.flush().await;
    assert_eq!(chain.proved(), [(REGISTRY, 1)]);
}
