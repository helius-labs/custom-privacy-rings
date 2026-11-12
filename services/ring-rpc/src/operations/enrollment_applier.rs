//! The ring's sole enrollment writer, it holds owner-signed writes and lands them as `EnrollBatch` batches.
//!
//! The writer serves each ring the operator names and each ring its policy role registers. An owner
//! hands over one signed write, a first enrollment with its `RegisterKey` proof or a rotation. The
//! writer admits a write only once its signature, its fields and its proof pass the checks storage
//! runs, so a held write never refuses a batch it rides in. Each ring runs on its own task under the
//! shared bound of `keyed_tasks`, so a ring whose proof stalls holds one slot and never the batch of
//! another ring.
//!
//! Read after `sdk-libs/client/src/rpc/enrollment.rs` and `sdk-libs/sdk/src/operations/storage.rs`.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use custom_ring_interface::{
    compute_unit_limit,
    storage_instruction::{EnrollmentChange, ENROLLMENT_BATCH},
    tracking::{TrackingEntry, TrackingRegistry, TRACKING_ROOT_HISTORY},
    InstructionTag, RegisterKeyCheck, RingAccount, RingPolicy,
};
use custom_ring_sdk::{CustomRing, EnrollBatch, ProvenEnrollBatch};
use log::warn;
use solana_address::Address;
use solana_keypair::Keypair;
use solana_signer::Signer;
use spr_client::{compile_message, sign_transaction, ComputeBudgetConfig};
use thiserror::Error;
use tokio::sync::Mutex as AsyncMutex;
use zolana_ring_client::rpc::{SubmitEnrollmentRequest, SubmitEnrollmentResponse};

use crate::{
    authorize::Unauthorized,
    database::Store,
    enrollment_rings::EnrollmentRingBook,
    error::{redacted, RingRpcError},
    keyed_tasks::{KeyedTasks, MAX_CONCURRENT_KEYED_TASKS},
    landing::{signature_fees, Attempt, ChainOutcome, Landing},
    storage_chain::StorageChain,
};

/// Writes one ring holds ahead of its batches, a full ring refuses the next until a batch lands.
pub const MAX_HELD_ENROLLMENTS: usize = 64;
/// Failed batches a write rides in before the writer drops it.
pub const MAX_ENROLLMENT_ATTEMPTS: u8 = 3;

#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
#[repr(i32)]
pub enum EnrollmentRefusal {
    #[error("enrollments are not written for the ring")]
    NotServed = -32180,
    #[error("the ring holds no initialized policy or tracking registry, or names another writer")]
    RingMissing = -32181,
    #[error("the enrollment write is malformed or its signature does not verify")]
    InvalidWrite = -32182,
    #[error("the RegisterKey proof does not verify for the enrolled key")]
    RegisterKeyProof = -32183,
    #[error(
        "the ring holds {MAX_HELD_ENROLLMENTS} enrollments, submit again after its next batch"
    )]
    Full = -32184,
    #[error("the enrollment writer's fee budget or the ring's share of it is spent, the operator raises it")]
    Budget = -32185,
    #[error("the enrollment writer serves its most registered rings, the operator raises the cap")]
    RingsFull = -32186,
}

/// Applies the held writes of each served ring, the payer is the ring's `EnrollmentWriter`.
pub struct EnrollmentApplier {
    chain: Arc<dyn StorageChain>,
    payer: Keypair,
    /// Served without a registration and never dropped.
    named: Vec<Address>,
    registered: Mutex<HashSet<Address>>,
    max_rings: usize,
    store: Option<Store>,
    budget: EnrollmentBudget,
    held: Mutex<HashMap<Address, Arc<AsyncMutex<RingState>>>>,
}

#[must_use]
pub struct EnrollmentApplierParts {
    pub chain: Arc<dyn StorageChain>,
    pub payer: Keypair,
    pub rings: Vec<Address>,
    pub budget_lamports: u64,
    pub ring_budget_lamports: u64,
    /// Registered rings, the named rings stay outside it.
    pub max_rings: usize,
    /// Keeps the registered rings across a restart.
    pub store: Option<Store>,
}

/// Charged at send over the process life, a batch that never lands keeps its charge.
struct EnrollmentBudget {
    total: u64,
    per_ring: u64,
    spent: Mutex<SpentFees>,
}

#[derive(Default)]
struct SpentFees {
    total: u64,
    rings: HashMap<Address, u64>,
}

/// The writer role a batch pass reads from the ring policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriterRole {
    Held,
    Moved,
}

#[derive(Default)]
struct RingState {
    queued: VecDeque<HeldWrite>,
    in_flight: Option<InFlight>,
}

#[derive(Clone)]
struct HeldWrite {
    request: SubmitEnrollmentRequest,
    owner: [u8; 32],
    /// Computed once at submit from the verified write.
    entry: TrackingEntry,
    attempts: u8,
}

struct InFlight {
    attempt: Attempt,
    writes: Vec<HeldWrite>,
}

/// One ring's roles and tracking registry as its policy pins them.
struct RingTracking {
    policy: Address,
    writer: Address,
    registry: Address,
    account: TrackingRegistry,
}

struct RingPass<'a> {
    chain: &'a dyn StorageChain,
    payer: &'a Keypair,
    budget: &'a EnrollmentBudget,
    ring: CustomRing,
    /// A registered ring ends its pass once its policy names another writer.
    registered: bool,
    state: &'a mut RingState,
}

impl EnrollmentApplierParts {
    /// Loads the registered rings from the store.
    pub async fn applier(self) -> Result<EnrollmentApplier, RingRpcError> {
        let registered = match &self.store {
            Some(store) => {
                EnrollmentRingBook::new(store.connection(), self.payer.pubkey())
                    .rings()
                    .await?
            }
            None => Vec::new(),
        };
        Ok(EnrollmentApplier {
            chain: self.chain,
            payer: self.payer,
            named: self.rings,
            registered: Mutex::new(registered.into_iter().collect()),
            max_rings: self.max_rings,
            store: self.store,
            budget: EnrollmentBudget {
                total: self.budget_lamports,
                per_ring: self.ring_budget_lamports,
                spent: Mutex::default(),
            },
            held: Mutex::default(),
        })
    }
}

impl EnrollmentBudget {
    /// The payer's signature and one precompile signature per write.
    fn batch_fee(writes: usize) -> u64 {
        signature_fees(writes.saturating_add(1))
    }

    fn holds(&self, ring: Address, fee: u64) -> bool {
        self.spent().after(ring, fee, self).is_some()
    }

    fn charge(&self, ring: Address, fee: u64) -> bool {
        let mut spent = self.spent();
        let Some((total, ring_total)) = spent.after(ring, fee, self) else {
            return false;
        };
        spent.total = total;
        spent.rings.insert(ring, ring_total);
        true
    }

    fn spent(&self) -> MutexGuard<'_, SpentFees> {
        self.spent.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl SpentFees {
    /// The total and the ring's spend once `fee` is charged, `None` past either cap.
    fn after(&self, ring: Address, fee: u64, budget: &EnrollmentBudget) -> Option<(u64, u64)> {
        let total = self
            .total
            .checked_add(fee)
            .filter(|total| *total <= budget.total)?;
        let ring_total = self
            .rings
            .get(&ring)
            .copied()
            .unwrap_or(0)
            .checked_add(fee)
            .filter(|spent| *spent <= budget.per_ring)?;
        Some((total, ring_total))
    }
}

impl EnrollmentApplier {
    pub fn payer(&self) -> Address {
        self.payer.pubkey()
    }

    /// A zero budget or ring budget refuses every write.
    pub const fn writes(&self) -> bool {
        self.budget.total > 0 && self.budget.per_ring > 0
    }

    /// Serves the ring once its policy role signed and its policy names the payer as `EnrollmentWriter`.
    pub async fn register(&self, ring: Address, signer: Address) -> Result<(), RingRpcError> {
        let tracking = ring_tracking(self.chain.as_ref(), ring)
            .await?
            .ok_or(EnrollmentRefusal::RingMissing)?;
        if tracking.policy != signer {
            return Err(Unauthorized::NotPolicyAuthority.into());
        }
        if tracking.writer != self.payer.pubkey() {
            return Err(EnrollmentRefusal::RingMissing.into());
        }
        if self.named.contains(&ring) {
            return Ok(());
        }
        {
            let mut registered = self.registered();
            if registered.contains(&ring) {
                return Ok(());
            }
            if registered.len() >= self.max_rings {
                return Err(EnrollmentRefusal::RingsFull.into());
            }
            registered.insert(ring);
        }
        if let Some(book) = self.book() {
            if let Err(error) = book.insert(ring).await {
                self.registered().remove(&ring);
                return Err(error);
            }
        }
        Ok(())
    }

    /// Holds the write once every check storage runs on it passes.
    pub async fn submit(
        &self,
        request: SubmitEnrollmentRequest,
    ) -> Result<SubmitEnrollmentResponse, RingRpcError> {
        let ring = request.ring.0;
        let named = self.named.contains(&ring);
        if !named && !self.registered().contains(&ring) {
            return Err(EnrollmentRefusal::NotServed.into());
        }
        if !self.budget.holds(ring, EnrollmentBudget::batch_fee(1)) {
            return Err(EnrollmentRefusal::Budget.into());
        }
        // 1. The registry the ring pins, its ring id and auditor key bind every proof.
        let tracking = ring_tracking(self.chain.as_ref(), ring)
            .await?
            .ok_or(EnrollmentRefusal::RingMissing)?;
        if !named && tracking.writer != self.payer.pubkey() {
            self.unregister(ring).await;
            return Err(EnrollmentRefusal::RingMissing.into());
        }
        // 2. The write parses exactly, passes the field checks and carries its owner's signature.
        let write = request
            .verify(&tracking.registry)
            .map_err(|_| EnrollmentRefusal::InvalidWrite)?;
        // 3. A first enrollment's proof verifies here, a failure at apply would refuse the whole batch.
        if let EnrollmentChange::Enroll { ix, proof } = write.change {
            RegisterKeyCheck {
                ring_id: &tracking.account.ring_id,
                auditor_pubkey: &tracking.account.auditor_pubkey,
                owner: write.owner,
                ix,
                proof,
            }
            .verify()
            .map_err(|_| EnrollmentRefusal::RegisterKeyProof)?;
        }
        let owner = write.owner.to_bytes();
        let entry = write.entry().map_err(|_| EnrollmentRefusal::InvalidWrite)?;
        let state = self.ring_state(ring);
        let mut state = state.lock().await;
        let held = state.queued.len()
            + state
                .in_flight
                .as_ref()
                .map_or(0, |in_flight| in_flight.writes.len());
        if state.queued.iter().any(|queued| queued.request == request) {
            return Ok(SubmitEnrollmentResponse {});
        }
        if held >= MAX_HELD_ENROLLMENTS {
            return Err(EnrollmentRefusal::Full.into());
        }
        state.queued.push_back(HeldWrite {
            request,
            owner,
            entry,
            attempts: 0,
        });
        Ok(SubmitEnrollmentResponse {})
    }

    /// Every `interval`, the first at once.
    pub async fn run(self: Arc<Self>, interval: Duration) {
        let rings = KeyedTasks::new(MAX_CONCURRENT_KEYED_TASKS);
        let mut ticks = tokio::time::interval(interval);
        loop {
            ticks.tick().await;
            rings.spawn_idle(self.served(), |ring| {
                let applier = self.clone();
                async move { applier.apply_logged(ring).await }
            });
        }
    }

    /// One pass over the served rings in turn, a failing ring never holds up another.
    pub async fn flush(&self) {
        for ring in self.served() {
            self.apply_logged(ring).await;
        }
    }

    fn served(&self) -> Vec<Address> {
        let mut rings = self.named.clone();
        rings.extend(
            self.registered()
                .iter()
                .filter(|ring| !self.named.contains(ring)),
        );
        rings
    }

    fn registered(&self) -> MutexGuard<'_, HashSet<Address>> {
        self.registered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn book(&self) -> Option<EnrollmentRingBook<'_>> {
        self.store
            .as_ref()
            .map(|store| EnrollmentRingBook::new(store.connection(), self.payer.pubkey()))
    }

    /// Drops a registered ring and its held writes, a failed delete drops it again after a restart.
    async fn unregister(&self, ring: Address) {
        if !self.registered().remove(&ring) {
            return;
        }
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&ring);
        warn!("enrollment writer dropped ring {ring}, its policy names another writer");
        let Some(book) = self.book() else {
            return;
        };
        if let Err(error) = book.remove(ring).await {
            warn!(
                "enrollment writer kept the row of ring {ring} because {}",
                redacted(&error)
            );
        }
    }

    async fn apply_logged(&self, ring: Address) {
        let state = self.ring_state(ring);
        // A ring found busy sits out the tick.
        let Ok(mut state) = state.try_lock() else {
            return;
        };
        let pass = RingPass {
            chain: self.chain.as_ref(),
            payer: &self.payer,
            budget: &self.budget,
            ring: CustomRing::new(ring),
            registered: !self.named.contains(&ring),
            state: &mut state,
        };
        let role = pass.run().await;
        drop(state);
        match role {
            Ok(WriterRole::Held) => {}
            Ok(WriterRole::Moved) => self.unregister(ring).await,
            Err(error) => warn!(
                "enrollment batch of ring {ring} failed because {}",
                redacted(&error)
            ),
        }
    }

    fn ring_state(&self, ring: Address) -> Arc<AsyncMutex<RingState>> {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(ring)
            .or_default()
            .clone()
    }
}

impl RingPass<'_> {
    async fn run(mut self) -> Result<WriterRole, RingRpcError> {
        // 1. One batch at a time, a second proof from the same root could not land.
        if let Some(in_flight) = self.state.in_flight.take() {
            match in_flight.attempt.landing(self.chain).await? {
                Landing::Pending => {
                    self.state.in_flight = Some(in_flight);
                    return Ok(WriterRole::Held);
                }
                Landing::Landed(ChainOutcome::Confirmed { .. }) => {}
                Landing::Landed(ChainOutcome::Failed { .. }) | Landing::Expired => {
                    self.requeue(in_flight.writes);
                }
            }
        }
        if self.state.queued.is_empty() {
            return Ok(WriterRole::Held);
        }
        let Some(tracking) = ring_tracking(self.chain, self.ring.program_id()).await? else {
            return Ok(WriterRole::Held);
        };
        if self.registered && tracking.writer != self.payer.pubkey() {
            return Ok(WriterRole::Moved);
        }
        // 2. The program refuses a batch that would evict a root inside its grace, waiting spends no proof.
        let slot = self.chain.slot().await?;
        if slot < tracking.account.roots[TRACKING_ROOT_HISTORY - 1].expires_at() {
            return Ok(WriterRole::Held);
        }
        let writes = self.applicable(tracking.registry).await?;
        if writes.is_empty() {
            return Ok(WriterRole::Held);
        }
        let requests: Vec<_> = writes.iter().map(|held| held.request.clone()).collect();
        // 3. Proving takes long, a batch landed meanwhile moves the root and leaves the proof unsent.
        let proven = self
            .chain
            .prove_enrollments(EnrollBatch {
                ring: self.ring,
                writer: self.payer.pubkey(),
                tracking_registry: tracking.registry,
                writes: &requests,
            })
            .await;
        let proven = match proven {
            Ok(proven) => proven,
            Err(error) => {
                self.requeue(writes);
                return Err(error);
            }
        };
        let current = ring_tracking(self.chain, self.ring.program_id()).await?;
        if current.is_none_or(|current| current.account.newest().revision != proven.revision) {
            self.requeue(writes);
            return Ok(WriterRole::Held);
        }
        self.send(proven, writes).await?;
        Ok(WriterRole::Held)
    }

    async fn send(
        mut self,
        proven: ProvenEnrollBatch,
        writes: Vec<HeldWrite>,
    ) -> Result<(), RingRpcError> {
        if !self.budget.charge(
            self.ring.program_id(),
            EnrollmentBudget::batch_fee(writes.len()),
        ) {
            self.requeue(writes);
            return Err(EnrollmentRefusal::Budget.into());
        }
        let latest = self.chain.latest_blockhash().await?;
        let message = compile_message(
            &self.payer.pubkey(),
            &proven.instructions,
            latest.blockhash,
            ComputeBudgetConfig::new(compute_unit_limit(InstructionTag::EnrollBatch)),
        )?;
        let transaction = sign_transaction(message, &[self.payer])?;
        self.chain.send(&transaction).await?;
        self.state.in_flight = Some(InFlight {
            attempt: Attempt {
                signature: transaction.signatures[0],
                last_valid_block_height: latest.last_valid_block_height,
            },
            writes,
        });
        Ok(())
    }

    /// The oldest writes that apply in order from the current leaves, a stale one drops as one refusal fails the batch.
    async fn applicable(&mut self, registry: Address) -> Result<Vec<HeldWrite>, RingRpcError> {
        let owners: Vec<[u8; 32]> = self.state.queued.iter().map(|held| held.owner).collect();
        let mut current: HashMap<[u8; 32], [u8; 32]> = self
            .chain
            .tracking_leaves(registry, &owners)
            .await?
            .into_iter()
            .map(|(owner, leaf)| (owner, leaf.record.viewing_hash))
            .collect();
        let mut batch = Vec::with_capacity(ENROLLMENT_BATCH);
        while batch.len() < ENROLLMENT_BATCH {
            let Some(held) = self.state.queued.pop_front() else {
                break;
            };
            let applies = match held.entry.proven {
                true => !current.contains_key(&held.owner),
                false => current.get(&held.owner) == Some(&held.entry.prior_viewing_hash),
            };
            if !applies {
                warn!(
                    "enrollment writer dropped a stale or repeated write of {}",
                    Address::new_from_array(held.owner)
                );
                continue;
            }
            current.insert(held.owner, held.entry.viewing_hash);
            batch.push(held);
        }
        Ok(batch)
    }

    /// Back to the front in their order, a write past its attempts drops so it never holds the ring.
    fn requeue(&mut self, writes: Vec<HeldWrite>) {
        for mut held in writes.into_iter().rev() {
            held.attempts += 1;
            if held.attempts < MAX_ENROLLMENT_ATTEMPTS {
                self.state.queued.push_front(held);
            }
        }
    }
}

async fn ring_tracking(
    chain: &dyn StorageChain,
    ring: Address,
) -> Result<Option<RingTracking>, RingRpcError> {
    let Some(policy) = chain
        .account(CustomRing::new(ring).policy_pda())
        .await?
        .and_then(|account| RingPolicy::decode(&account.owner, &ring, &account.data))
    else {
        return Ok(None);
    };
    let account = chain
        .account(policy.tracking)
        .await?
        .and_then(|account| TrackingRegistry::decode(&account.owner, &account.data));
    Ok(account.map(|account| RingTracking {
        policy: policy.roles.policy,
        writer: policy.roles.enrollment_writer,
        registry: policy.tracking,
        account,
    }))
}

#[cfg(test)]
#[path = "enrollment_applier_tests.rs"]
mod enrollment_applier_tests;
