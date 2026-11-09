//! Applies each named ring's queued enrollments to its tracking tree, one batch per root.
//!
//! Each ring runs on its own task under the shared bound of `keyed_tasks`, so a ring whose proof stalls
//! holds one slot and never the apply of another ring.
//!
//! The batch size comes from the queue account's `head` and `tail` on chain. The program consumes
//! entries from `head` in order and the ring indexer only proves them, so an indexer serving other
//! entries yields a proof the program refuses and never a granted owner.
//!
//! Read after `services/ring-indexer/src/rpc/tracking.rs` and `sdk-libs/sdk/src/operations/storage.rs`.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use custom_ring_interface::{
    member_registry::APPLY_BATCH,
    tracking::{EnrollmentQueue, TrackingRegistry, TRACKING_ROOT_HISTORY},
    RingAccount, RingPolicy,
};
use custom_ring_sdk::CustomRing;
use log::warn;
use solana_address::Address;
use solana_keypair::Keypair;
use solana_signer::Signer;
use spr_client::{compile_message, sign_transaction, ComputeBudgetConfig};
use tokio::sync::Mutex as AsyncMutex;

use crate::{
    error::{redacted, RingRpcError},
    keyed_tasks::{KeyedTasks, MAX_CONCURRENT_KEYED_TASKS},
    landing::{Attempt, Landing},
    record_writer::UPDATE_COMPUTE_UNITS,
    storage_chain::{ProvedEnrollments, StorageChain},
};

/// Applies the queued enrollments of each named ring, the payer holds no ring role.
pub struct EnrollmentApplier {
    applier: Arc<Applier>,
    rings: Vec<Address>,
}

#[must_use]
pub struct EnrollmentApplierParts {
    pub chain: Arc<dyn StorageChain>,
    pub payer: Keypair,
    pub rings: Vec<Address>,
}

struct Applier {
    chain: Arc<dyn StorageChain>,
    payer: Keypair,
    registries: Mutex<HashMap<Address, Arc<AsyncMutex<RegistryState>>>>,
}

#[derive(Default)]
struct RegistryState {
    attempt: Option<Attempt>,
}

struct RegistryPass<'a> {
    chain: &'a dyn StorageChain,
    payer: &'a Keypair,
    registry: Address,
    state: &'a mut RegistryState,
}

impl EnrollmentApplierParts {
    pub fn applier(self) -> EnrollmentApplier {
        EnrollmentApplier {
            applier: Arc::new(Applier {
                chain: self.chain,
                payer: self.payer,
                registries: Mutex::default(),
            }),
            rings: self.rings,
        }
    }
}

impl EnrollmentApplier {
    pub fn payer(&self) -> Address {
        self.applier.payer.pubkey()
    }

    /// Every `interval`, the first at once.
    pub async fn run(self, interval: Duration) {
        let rings = KeyedTasks::new(MAX_CONCURRENT_KEYED_TASKS);
        let mut ticks = tokio::time::interval(interval);
        loop {
            ticks.tick().await;
            self.spawn_idle(&rings);
        }
    }

    /// One pass over the named rings in turn, a failing ring never holds up another.
    pub async fn flush(&self) {
        for ring in &self.rings {
            self.applier.apply_logged(*ring).await;
        }
    }

    /// Starts a pass for each ring with none running and returns at once.
    fn spawn_idle(&self, rings: &KeyedTasks<Address>) {
        rings.spawn_idle(self.rings.iter().copied(), |ring| {
            let applier = self.applier.clone();
            async move { applier.apply_logged(ring).await }
        });
    }
}

impl Applier {
    async fn apply_logged(&self, ring: Address) {
        if let Err(error) = self.apply_ring(ring).await {
            warn!(
                "enrollment apply of ring {ring} failed because {}",
                redacted(&error)
            );
        }
    }

    async fn apply_ring(&self, ring: Address) -> Result<(), RingRpcError> {
        let account = self
            .chain
            .account(CustomRing::new(ring).policy_pda())
            .await?;
        let Some(policy) =
            account.and_then(|account| RingPolicy::decode(&account.owner, &ring, &account.data))
        else {
            return Ok(());
        };
        // Rings pinning one registry share its state, a ring finding it busy sits out the tick.
        let Ok(mut state) = self.registry_state(policy.tracking).try_lock_owned() else {
            return Ok(());
        };
        RegistryPass {
            chain: self.chain.as_ref(),
            payer: &self.payer,
            registry: policy.tracking,
            state: &mut state,
        }
        .run()
        .await
    }

    fn registry_state(&self, registry: Address) -> Arc<AsyncMutex<RegistryState>> {
        self.registries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(registry)
            .or_default()
            .clone()
    }
}

impl RegistryPass<'_> {
    async fn run(self) -> Result<(), RingRpcError> {
        // 1. One apply at a time, a second proof from the same root could not land.
        if let Some(attempt) = self.state.attempt {
            if attempt.landing(self.chain).await? == Landing::Pending {
                return Ok(());
            }
            self.state.attempt = None;
        }
        let slot = self.chain.slot().await?;
        let Some(tracking) = self.tracking().await? else {
            return Ok(());
        };
        // 2. The program refuses an apply that would evict a root inside its grace, waiting spends no proof.
        if slot < tracking.roots[TRACKING_ROOT_HISTORY - 1].expires_at() {
            return Ok(());
        }
        let count = self.queued().await?.min(APPLY_BATCH as u64);
        if count == 0 {
            return Ok(());
        }
        // 3. Proving takes long, an apply landed meanwhile moves the root and leaves the proof unsent.
        let proved = self.chain.prove_enrollments(self.registry, count).await?;
        let current = self.tracking().await?;
        if current.is_none_or(|current| current.newest().revision != proved.revision) {
            return Ok(());
        }
        self.send(proved).await
    }

    /// Entries no apply consumed, read from the queue account and never from the indexer.
    async fn queued(&self) -> Result<u64, RingRpcError> {
        let (address, _) = EnrollmentQueue::find_address(&self.registry);
        Ok(self
            .chain
            .account(address)
            .await?
            .and_then(|account| EnrollmentQueue::decode(&account.owner, &account.data))
            .map_or(0, |queue| queue.pending()))
    }

    async fn send(self, proved: ProvedEnrollments) -> Result<(), RingRpcError> {
        let latest = self.chain.latest_blockhash().await?;
        let message = compile_message(
            &self.payer.pubkey(),
            &[proved.instruction],
            latest.blockhash,
            ComputeBudgetConfig::new(UPDATE_COMPUTE_UNITS),
        )?;
        let transaction = sign_transaction(message, &[self.payer])?;
        self.chain.send(&transaction).await?;
        self.state.attempt = Some(Attempt {
            signature: transaction.signatures[0],
            last_valid_block_height: latest.last_valid_block_height,
        });
        Ok(())
    }

    async fn tracking(&self) -> Result<Option<TrackingRegistry>, RingRpcError> {
        Ok(self
            .chain
            .account(self.registry)
            .await?
            .and_then(|account| TrackingRegistry::decode(&account.owner, &account.data)))
    }
}

#[cfg(test)]
#[path = "enrollment_applier_tests.rs"]
mod enrollment_applier_tests;
