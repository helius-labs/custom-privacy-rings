//! The ring's sole enrollment writer, it holds owner-signed writes and lands them as `EnrollBatch` batches.
//!
//! An owner hands over one signed write, a first enrollment with its `RegisterKey` proof or a
//! rotation. The writer admits a write only once its signature, its fields and its proof pass the
//! checks storage runs, so a held write never refuses a batch it rides in. Each ring runs on its own
//! task under the shared bound of `keyed_tasks`, so a ring whose proof stalls holds one slot and never
//! the batch of another ring.
//!
//! Read after `sdk-libs/client/src/rpc/enrollment.rs` and `sdk-libs/sdk/src/operations/storage.rs`.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, PoisonError},
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
    error::{redacted, RingRpcError},
    keyed_tasks::{KeyedTasks, MAX_CONCURRENT_KEYED_TASKS},
    landing::{Attempt, ChainOutcome, Landing},
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
    #[error("the ring holds no initialized policy or tracking registry")]
    RingMissing = -32181,
    #[error("the enrollment write is malformed or its signature does not verify")]
    InvalidWrite = -32182,
    #[error("the RegisterKey proof does not verify for the enrolled key")]
    RegisterKeyProof = -32183,
    #[error(
        "the ring holds {MAX_HELD_ENROLLMENTS} enrollments, submit again after its next batch"
    )]
    Full = -32184,
}

/// Applies the held writes of each named ring, the payer is the ring's `EnrollmentWriter`.
pub struct EnrollmentApplier {
    chain: Arc<dyn StorageChain>,
    payer: Keypair,
    rings: Vec<Address>,
    held: Mutex<HashMap<Address, Arc<AsyncMutex<RingState>>>>,
}

#[must_use]
pub struct EnrollmentApplierParts {
    pub chain: Arc<dyn StorageChain>,
    pub payer: Keypair,
    pub rings: Vec<Address>,
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

/// One ring's tracking registry as its policy pins it.
struct RingTracking {
    registry: Address,
    account: TrackingRegistry,
}

struct RingPass<'a> {
    chain: &'a dyn StorageChain,
    payer: &'a Keypair,
    ring: CustomRing,
    state: &'a mut RingState,
}

impl EnrollmentApplierParts {
    pub fn applier(self) -> EnrollmentApplier {
        EnrollmentApplier {
            chain: self.chain,
            payer: self.payer,
            rings: self.rings,
            held: Mutex::default(),
        }
    }
}

impl EnrollmentApplier {
    pub fn payer(&self) -> Address {
        self.payer.pubkey()
    }

    /// Holds the write once every check storage runs on it passes.
    pub async fn submit(
        &self,
        request: SubmitEnrollmentRequest,
    ) -> Result<SubmitEnrollmentResponse, RingRpcError> {
        let ring = request.ring.0;
        if !self.rings.contains(&ring) {
            return Err(EnrollmentRefusal::NotServed.into());
        }
        // 1. The registry the ring pins, its ring id and auditor key bind every proof.
        let tracking = ring_tracking(self.chain.as_ref(), ring)
            .await?
            .ok_or(EnrollmentRefusal::RingMissing)?;
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
            rings.spawn_idle(self.rings.iter().copied(), |ring| {
                let applier = self.clone();
                async move { applier.apply_logged(ring).await }
            });
        }
    }

    /// One pass over the named rings in turn, a failing ring never holds up another.
    pub async fn flush(&self) {
        for ring in &self.rings {
            self.apply_logged(*ring).await;
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
            ring: CustomRing::new(ring),
            state: &mut state,
        };
        if let Err(error) = pass.run().await {
            warn!(
                "enrollment batch of ring {ring} failed because {}",
                redacted(&error)
            );
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
    async fn run(mut self) -> Result<(), RingRpcError> {
        // 1. One batch at a time, a second proof from the same root could not land.
        if let Some(in_flight) = self.state.in_flight.take() {
            match in_flight.attempt.landing(self.chain).await? {
                Landing::Pending => {
                    self.state.in_flight = Some(in_flight);
                    return Ok(());
                }
                Landing::Landed(ChainOutcome::Confirmed { .. }) => {}
                Landing::Landed(ChainOutcome::Failed { .. }) | Landing::Expired => {
                    self.requeue(in_flight.writes);
                }
            }
        }
        if self.state.queued.is_empty() {
            return Ok(());
        }
        let Some(tracking) = ring_tracking(self.chain, self.ring.program_id()).await? else {
            return Ok(());
        };
        // 2. The program refuses a batch that would evict a root inside its grace, waiting spends no proof.
        let slot = self.chain.slot().await?;
        if slot < tracking.account.roots[TRACKING_ROOT_HISTORY - 1].expires_at() {
            return Ok(());
        }
        let writes = self.applicable(tracking.registry).await?;
        if writes.is_empty() {
            return Ok(());
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
            return Ok(());
        }
        self.send(proven, writes).await
    }

    async fn send(
        self,
        proven: ProvenEnrollBatch,
        writes: Vec<HeldWrite>,
    ) -> Result<(), RingRpcError> {
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
        registry: policy.tracking,
        account,
    }))
}

#[cfg(test)]
#[path = "enrollment_applier_tests.rs"]
mod enrollment_applier_tests;
