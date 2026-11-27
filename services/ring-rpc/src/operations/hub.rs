use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use solana_address::Address;
use solana_signature::Signature;
use spr_client::{AsyncRpc, ClientError};
use spr_keypair::X25519Keypair;
use thiserror::Error;
use tokio::sync::{watch, Mutex as AsyncMutex, Semaphore, SemaphorePermit};
use zolana_operator_custody::SigningIdentity;
use zolana_ring_client::{
    auditor_view_tag,
    rpc::{
        CoSignListEntry, EditCoSignListRequest, GetCoSignListsRequest, GetCoSignListsResponse,
        GetDelegateNotesRequest, GetDelegateNotesResponse, GetListSaltRequest, GetListSaltResponse,
        KeyMode, PrepareAuthorityTransferRequest, RelayStep, ServiceTarget,
        SetMergeSettingsRequest, SetMergeSettingsResponse,
    },
    PrivateStorageWrite, ProvenPrivateWrite, RingClient, RingError, RingIndexerBackend,
};
use zolana_ring_transaction::RingAssets;

use ring_service_core::{
    authorize::Unauthorized,
    clock::Clock,
    database::ServiceStore,
    error::CarriedFault,
    keyed_tasks::{KeyedTasks, MAX_CONCURRENT_KEYED_TASKS},
    keys::{GrantRowsSecret, KeySource, RootSecret},
    limits::{Lane, RequestRate, MAX_CONCURRENT_AUTHENTICATIONS, MAX_CONCURRENT_READS},
    origins::Origins,
    upstream::TransactionSource,
};
use zolana_ring_indexer::redact::redacted;

use ring_audit::{
    audit::AuditService,
    shared::{Shared, SharedInit},
    utxos::UTXO_BACKFILL_INTERVAL,
    wallet_snapshots::WalletSnapshotLimits,
};
use ring_writer::{
    enrollment_applier::{EnrollmentApplier, EnrollmentRefusal},
    record_update::{ReaderGrantStore, RecordUpdateRefusal},
    record_writer::{PrivateWrites, RecordWriter, SaltedOwner, WriterParts, RECORD_FLUSH_INTERVAL},
    storage_chain::StorageChain,
};

use crate::error::RingRpcError;
use ring_audit::RingAuditError;
use ring_cosigner::{
    cosign::{CoSignPolicy, CoSignerApproval},
    cosign_limits::CoSignPolicyInit,
    cosigner::{co_sign_lists, edit_co_sign_list},
};
use ring_relay::{
    authority_transfer::{
        delegate_notes, list_salt, prepare_authority_transfer, release_delegate_snapshot,
    },
    managed_merge::{set_merge_settings, ManagedMerge, ManagedMergeInit},
    relay::{Relay, RelayParts, SharedRings, RELAY_TRACK_INTERVAL},
    relay_reservations::RelayBudget,
    relay_services::{MergeParts, RelayServices},
};
use ring_service_core::ServiceError;

const READINESS_CACHE_INTERVAL: Duration = Duration::from_secs(1);

pub struct Hub<S> {
    signer: SigningIdentity,
    shared: Arc<Shared<S>>,
    writer: Option<Arc<RecordWriter>>,
    enrollments: Option<Arc<EnrollmentApplier>>,
    co_signer: Arc<dyn CoSignPolicy>,
    relay: Option<Arc<Relay>>,
    merge: Option<MergeParts>,
    lanes: [(RequestRate, Option<Semaphore>); 3],
    readiness: AsyncMutex<ReadinessCache>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadinessStatus {
    Ready,
    Unavailable(&'static str),
}

/// Names the readiness check that failed, which `/ready` reports.
#[derive(Debug, Error)]
pub enum NotReady {
    #[error("upstream probe failed because {0}")]
    Upstream(#[source] ServiceError),
    #[error("the local ring config is unusable because {0}")]
    AuditorKey(#[source] Unauthorized),
    #[error("the cluster genesis hash differs from the one captured at boot")]
    Cluster,
}

impl NotReady {
    pub fn check(&self) -> &'static str {
        match self {
            Self::Upstream(_) => "unavailable upstream",
            Self::AuditorKey(_) => "unavailable auditor key",
            Self::Cluster => "unavailable cluster",
        }
    }
}

#[must_use = "hold the lane permit for the request"]
pub(crate) struct LanePermit<'a> {
    _permit: Option<SemaphorePermit<'a>>,
}

/// Every read and auditor key attestation binds both.
pub struct ServiceIdentity {
    pub genesis_hash: [u8; 32],
    pub service: SigningIdentity,
}

#[must_use]
pub struct HubBuilder<S> {
    source: S,
    identity: ServiceIdentity,
    assets: Option<RingAssets>,
    origins: Origins,
    clock: Clock,
    store: Option<ServiceStore>,
    snapshot_limits: WalletSnapshotLimits,
    storage: Option<(Arc<dyn StorageChain>, SigningIdentity)>,
    reader_grants: ReaderGrantStore,
    grant_rows: Option<GrantRowsSecret>,
    co_signer: Option<SigningIdentity>,
    co_signer_approval: CoSignerApproval,
    relay_budget: RelayBudget,
    relay: Option<RelayParts>,
    merge: Option<MergeParts>,
    enrollments: Option<Arc<EnrollmentApplier>>,
}

struct ReadinessCache {
    checked_at: Option<Instant>,
    status: ReadinessStatus,
    in_flight: Option<watch::Receiver<Option<ReadinessStatus>>>,
}

impl<S: TransactionSource> Hub<S> {
    pub fn builder(source: S, identity: ServiceIdentity) -> HubBuilder<S> {
        HubBuilder {
            source,
            identity,
            assets: None,
            origins: Origins::default(),
            clock: Clock::System,
            store: None,
            snapshot_limits: WalletSnapshotLimits::default(),
            storage: None,
            reader_grants: ReaderGrantStore::Tree,
            grant_rows: None,
            co_signer: None,
            co_signer_approval: CoSignerApproval::default(),
            relay_budget: RelayBudget::default(),
            relay: None,
            merge: None,
            enrollments: None,
        }
    }

    /// The rate unit is spent even when every slot is taken.
    pub(crate) fn admit(&self, lane: Lane) -> Result<LanePermit<'_>, RingRpcError> {
        let index = match lane {
            Lane::Public => 0,
            Lane::Authentication => 1,
            Lane::Read => 2,
        };
        let (rate, slots) = &self.lanes[index];
        rate.accept(Instant::now())?;
        let permit = slots
            .as_ref()
            .map(|slots| {
                slots
                    .try_acquire()
                    .map_err(|_| RingRpcError::Service(ServiceError::Busy))
            })
            .transpose()?;
        Ok(LanePermit { _permit: permit })
    }

    pub fn mode(&self) -> KeyMode {
        match self.shared.keys() {
            KeySource::Local { .. } => KeyMode::Local,
            KeySource::Derived(_) => KeyMode::Derived,
        }
    }

    pub fn service_pubkey(&self) -> Address {
        self.signer.address()
    }

    pub fn record_writer(&self) -> Result<&Arc<RecordWriter>, RingRpcError> {
        self.writer
            .as_ref()
            .ok_or_else(|| RecordUpdateRefusal::Disabled.into())
    }

    pub fn enrollment_applier(&self) -> Result<&Arc<EnrollmentApplier>, RingRpcError> {
        self.enrollments
            .as_ref()
            .ok_or_else(|| EnrollmentRefusal::NotServed.into())
    }

    /// Signs under the asset record limits, refuses every co-signature without a co-signer key.
    pub fn co_signer(&self) -> &Arc<dyn CoSignPolicy> {
        &self.co_signer
    }

    pub(crate) async fn sign_attestation(
        &self,
        attestation: &[u8],
    ) -> Result<Signature, RingRpcError> {
        Ok(self.signer.sign(attestation).await?)
    }

    pub fn origins(&self) -> &Origins {
        &self.shared.origins
    }

    pub fn relay(&self) -> Option<&Arc<Relay>> {
        self.relay.as_ref()
    }

    pub(crate) fn shared(&self) -> &Shared<S> {
        &self.shared
    }

    /// Pays through the relay fee payer, `None` without relay services.
    pub fn managed_merge(self: &Arc<Self>) -> Option<ManagedMerge<S>> {
        let (merge, relay) = self.merge.as_ref().zip(self.relay.as_ref())?;
        Some(
            ManagedMergeInit {
                shared: self.shared.clone(),
                builder: merge.builder.clone(),
                relay: relay.clone(),
            }
            .build(),
        )
    }

    pub fn local_view_tag(&self) -> Option<[u8; 32]> {
        match &self.shared.keys() {
            KeySource::Local { auditor, .. } => Some(auditor_view_tag(&auditor.pubkey())),
            KeySource::Derived(_) => None,
        }
    }

    /// `None` names the one ring a local key serves.
    pub fn service(&self, ring: Option<Address>) -> Result<AuditService<S>, RingAuditError> {
        self.shared.service(ring)
    }

    /// A replay or a request not later than the stored one fails.
    pub async fn set_merge_settings(
        &self,
        request: &SetMergeSettingsRequest,
    ) -> Result<SetMergeSettingsResponse, RingRpcError> {
        Ok(set_merge_settings(&self.shared, request).await?)
    }

    /// The chain names the delegate, only its signature reads a victim's secret and openings.
    pub async fn delegate_notes(
        &self,
        request: &GetDelegateNotesRequest,
    ) -> Result<GetDelegateNotesResponse, RingRpcError> {
        Ok(delegate_notes(&self.shared, request).await?)
    }

    pub async fn release_delegate_snapshot(
        &self,
        request: &GetDelegateNotesRequest,
    ) -> Result<bool, RingRpcError> {
        Ok(release_delegate_snapshot(&self.shared, request).await?)
    }

    /// An owner dodging a block need not hand over an address.
    pub async fn list_salt(
        &self,
        request: &GetListSaltRequest,
    ) -> Result<GetListSaltResponse, RingRpcError> {
        Ok(list_salt(&self.shared, request).await?)
    }

    /// A repeated request returns the stored relay step.
    pub async fn prepare_authority_transfer(
        &self,
        request: &PrepareAuthorityTransferRequest,
    ) -> Result<RelayStep, RingRpcError> {
        Ok(prepare_authority_transfer(&self.shared, self.relay(), request).await?)
    }

    pub async fn edit_co_sign_list(
        &self,
        request: &EditCoSignListRequest,
    ) -> Result<CoSignListEntry, RingRpcError> {
        Ok(edit_co_sign_list(&self.shared, request).await?)
    }

    pub async fn co_sign_lists(
        &self,
        request: &GetCoSignListsRequest,
    ) -> Result<GetCoSignListsResponse, RingRpcError> {
        Ok(co_sign_lists(&self.shared, request).await?)
    }

    /// Every `interval`, the first at once, backfills the local ring and every ring a read opened.
    pub async fn run_utxo_backfill(self: Arc<Self>, interval: Duration)
    where
        S: 'static,
    {
        let rings = KeyedTasks::new(MAX_CONCURRENT_KEYED_TASKS);
        let mut ticks = tokio::time::interval(interval);
        loop {
            ticks.tick().await;
            if let Err(error) = self.shared.cleanup_wallet_snapshots().await {
                log::warn!(
                    "wallet snapshot cleanup failed because {}",
                    redacted(&error)
                );
            }
            rings.spawn_idle(self.backfill_rings(), |ring| {
                let hub = self.clone();
                async move { hub.backfill_ring(ring).await }
            });
        }
    }

    async fn backfill_ring(&self, ring: Address) {
        let backfill = match self.shared.service(Some(ring)) {
            Ok(service) => service.backfill_utxos().await,
            Err(error) => Err(error),
        };
        if let Err(error) = backfill {
            log::warn!(
                "UTXO backfill of ring {ring} failed because {}",
                redacted(&error)
            );
        }
    }

    fn backfill_rings(&self) -> Vec<Address> {
        let mut rings = self.shared.store_rings();
        if let KeySource::Local { ring, .. } = &self.shared.keys() {
            if !rings.contains(ring) {
                rings.push(*ring);
            }
        }
        rings
    }

    /// Local mode holds one key for one ring, so it also checks the config.
    pub async fn probe_upstreams(&self) -> Result<(), NotReady> {
        let upstream = |error: ClientError| NotReady::Upstream(error.into());
        let source = &self.shared.source;
        source.health().await.map_err(upstream)?;
        if source.genesis_hash().await.map_err(upstream)? != self.shared.genesis_hash {
            return Err(NotReady::Cluster);
        }
        if let KeySource::Local { ring, auditor } = &self.shared.keys() {
            let config = source
                .ring_config(*ring)
                .await
                .map_err(upstream)?
                .ok_or(NotReady::AuditorKey(Unauthorized::NoConfig))?;
            if config.auditor_pubkey != auditor.pubkey() {
                return Err(NotReady::AuditorKey(Unauthorized::AuditorKeyMismatch));
            }
        }
        Ok(())
    }

    /// One probe runs at a time in its own task, so a cancelled waiter never restarts it.
    pub(crate) async fn readiness(self: &Arc<Self>) -> ReadinessStatus
    where
        S: 'static,
    {
        let mut receiver = {
            let mut cache = self.readiness.lock().await;
            if cache
                .checked_at
                .is_some_and(|at| at.elapsed() < READINESS_CACHE_INTERVAL)
            {
                return cache.status;
            }
            match cache
                .in_flight
                .clone()
                .filter(|probe| probe.has_changed().is_ok())
            {
                Some(probe) => probe,
                None => {
                    let (sender, probe) = watch::channel(None);
                    cache.in_flight = Some(probe.clone());
                    let hub = self.clone();
                    tokio::spawn(async move {
                        let status = match hub.probe_upstreams().await {
                            Ok(()) => ReadinessStatus::Ready,
                            Err(error) => {
                                log::error!("readiness probe failed because {}", redacted(&error));
                                ReadinessStatus::Unavailable(error.check())
                            }
                        };
                        *hub.readiness.lock().await = ReadinessCache {
                            checked_at: Some(Instant::now()),
                            status,
                            in_flight: None,
                        };
                        let _ = sender.send(Some(status));
                    });
                    probe
                }
            }
        };
        let status = receiver
            .wait_for(Option::is_some)
            .await
            .map(|status| *status);
        match status {
            Ok(status) => status.unwrap_or(ReadinessStatus::Unavailable("unavailable probe")),
            Err(_) => ReadinessStatus::Unavailable("unavailable readiness task"),
        }
    }
}

/// Resolves salted owners from the enrollments the auditor key opens, proves through the ring SDK.
struct HeldPrivateWrites<S, R, I> {
    shared: Arc<Shared<S>>,
    client: RingClient<R, I>,
}

#[async_trait]
impl<S, R, I> PrivateWrites for HeldPrivateWrites<S, R, I>
where
    S: TransactionSource + 'static,
    R: AsyncRpc + Send + Sync + 'static,
    I: RingIndexerBackend + Send + Sync + 'static,
{
    async fn salted_owner(
        &self,
        ring: Address,
        written_key: [u8; 32],
    ) -> Result<Option<SaltedOwner>, Box<dyn CarriedFault>> {
        Ok(self
            .shared
            .service(Some(ring))?
            .salted_owner(&written_key)
            .await?)
    }

    async fn ring_salt(&self, ring: Address) -> Result<[u8; 32], Box<dyn CarriedFault>> {
        let service = self.shared.service(Some(ring))?;
        Ok(*service.ring_salt()?)
    }

    async fn prove(&self, write: PrivateStorageWrite<'_>) -> Result<ProvenPrivateWrite, RingError> {
        write.prove(&self.client).await
    }
}

impl<S: TransactionSource + 'static> Hub<S> {
    /// A record writer without it refuses every salted owner write.
    pub fn serve_private_writes<R, I>(&self, client: RingClient<R, I>) -> Result<(), RingRpcError>
    where
        R: AsyncRpc + Send + Sync + 'static,
        I: RingIndexerBackend + Send + Sync + 'static,
    {
        self.record_writer()?
            .serve_private_writes(Arc::new(HeldPrivateWrites {
                shared: self.shared.clone(),
                client,
            }))
            .map_err(|_| RingRpcError::Service(ServiceError::StateUnavailable))
    }

    /// Every background worker except the managed merge, its caller picks the interval.
    pub fn spawn_workers(self: &Arc<Self>) {
        if let Some(relay) = self.relay() {
            tokio::spawn(relay.clone().run(RELAY_TRACK_INTERVAL));
        }
        tokio::spawn(self.clone().run_utxo_backfill(UTXO_BACKFILL_INTERVAL));
        if let Ok(writer) = self.record_writer() {
            tokio::spawn(writer.clone().run(RECORD_FLUSH_INTERVAL));
        }
    }
}

impl<S: TransactionSource + 'static> HubBuilder<S> {
    #[must_use = "use the updated builder"]
    pub fn with_assets(mut self, assets: RingAssets) -> Self {
        self.assets = Some(assets);
        self
    }

    #[must_use = "use the updated builder"]
    pub fn with_origins(mut self, origins: Origins) -> Self {
        self.origins = origins;
        self
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    #[must_use = "use the updated builder"]
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// Without it the hub keeps its rows in an internal store.
    #[must_use = "use the updated builder"]
    pub fn with_store(mut self, store: ServiceStore) -> Self {
        self.store = Some(store);
        self
    }

    pub fn with_snapshot_limits(mut self, limits: WalletSnapshotLimits) -> Self {
        self.snapshot_limits = limits;
        self
    }

    /// The record writer runs only beside a durable store.
    #[must_use = "use the updated builder"]
    pub fn with_storage_chain(
        mut self,
        chain: Arc<dyn StorageChain>,
        authority: SigningIdentity,
    ) -> Self {
        self.storage = Some((chain, authority));
        self
    }

    /// `Database` needs a durable store and a grant rows key.
    #[must_use = "use the updated builder"]
    pub fn with_reader_grants(mut self, store: ReaderGrantStore) -> Self {
        self.reader_grants = store;
        self
    }

    #[must_use = "use the updated builder"]
    pub fn with_grant_rows_key(mut self, secret: GrantRowsSecret) -> Self {
        self.grant_rows = Some(secret);
        self
    }

    /// The hub refuses to build without a durable store.
    #[must_use = "use the updated builder"]
    pub fn with_co_signer(mut self, signer: SigningIdentity) -> Self {
        self.co_signer = Some(signer);
        self
    }

    /// Without it the co-signer refuses every operation.
    #[must_use = "use the updated builder"]
    pub fn with_co_signer_approval(mut self, approval: CoSignerApproval) -> Self {
        self.co_signer_approval = approval;
        self
    }

    /// Without it the relay sends nothing.
    #[must_use = "use the updated builder"]
    pub const fn with_relay_budget(mut self, budget: RelayBudget) -> Self {
        self.relay_budget = budget;
        self
    }

    /// Serves `submitEnrollment` for the applier's rings.
    #[must_use = "use the updated builder"]
    pub fn with_enrollment_applier(mut self, applier: Arc<EnrollmentApplier>) -> Self {
        self.enrollments = Some(applier);
        self
    }

    #[must_use = "use the updated builder"]
    pub fn with_relay(mut self, relay: RelayParts) -> Self {
        self.relay = Some(relay);
        self
    }

    #[must_use = "use the updated builder"]
    pub fn with_relay_services(mut self, services: RelayServices) -> Self {
        self.relay = Some(services.parts);
        self.merge = Some(services.merge);
        self
    }

    pub async fn local(
        self,
        ring: Address,
        auditor: X25519Keypair,
    ) -> Result<Hub<S>, RingRpcError> {
        self.build(KeySource::Local { ring, auditor }).await
    }

    pub async fn derived(self, root: RootSecret) -> Result<Hub<S>, RingRpcError> {
        self.build(KeySource::Derived(root)).await
    }

    async fn build(self, keys: KeySource) -> Result<Hub<S>, RingRpcError> {
        let ServiceIdentity {
            genesis_hash,
            service: signer,
        } = self.identity;
        let store = match self.store {
            Some(store) => store,
            None => ServiceStore::internal().await?,
        };
        let durable = store.durable().cloned();
        let writer = durable
            .clone()
            .zip(self.storage)
            .map(|(store, (chain, authority))| {
                Arc::new(
                    WriterParts {
                        store,
                        chain,
                        authority,
                        genesis_hash,
                        reader_grants: self.reader_grants,
                    }
                    .writer(),
                )
            });
        let lane = |count| (RequestRate::default(), Some(Semaphore::new(count)));
        let shared = SharedInit {
            source: self.source,
            genesis_hash,
            service: signer.address(),
            keys,
            clock: self.clock,
            assets: self.assets,
            origins: self.origins,
            store,
            snapshot_limits: self.snapshot_limits,
            reader_grants: self.reader_grants,
            grant_rows: self.grant_rows,
        }
        .build()?;
        let shared = Arc::new(shared);
        let co_signer = CoSignPolicyInit {
            signer: self.co_signer,
            shared: shared.clone(),
            store: durable,
            approval: self.co_signer_approval,
        }
        .build()?;
        let relay = self.relay.map(|parts| {
            Arc::new(
                Relay::new(
                    parts,
                    ServiceTarget {
                        cluster: shared.genesis_hash,
                        service: shared.service,
                    },
                )
                .with_budget(self.relay_budget)
                .with_co_signer(co_signer.clone())
                .with_rings(Arc::new(SharedRings(shared.clone()))),
            )
        });
        Ok(Hub {
            signer,
            shared,
            writer,
            enrollments: self.enrollments,
            co_signer,
            relay,
            merge: self.merge,
            lanes: [
                (RequestRate::default(), None),
                lane(MAX_CONCURRENT_AUTHENTICATIONS),
                lane(MAX_CONCURRENT_READS),
            ],
            readiness: AsyncMutex::new(ReadinessCache {
                checked_at: None,
                status: ReadinessStatus::Unavailable("not checked"),
                in_flight: None,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use ring_cosigner::RingCosignerError;
    use ring_relay::role_pins::fixture::RingAccounts;
    use ring_service_core::upstream::{ChainSource, Upstreams};
    use solana_keypair::Keypair;

    use super::*;

    fn builder() -> HubBuilder<ChainSource> {
        let upstreams = Upstreams {
            indexer_url: "http://127.0.0.1:1",
            rpc_url: "http://127.0.0.1:1",
            timeout: Duration::from_secs(1),
        };
        let identity = ServiceIdentity {
            genesis_hash: [1; 32],
            service: Arc::new(Keypair::new_from_array([12; 32])),
        };
        Hub::builder(ChainSource::connect(upstreams).expect("source"), identity)
    }

    fn root() -> RootSecret {
        RootSecret::from_bytes([7; 32]).expect("root")
    }

    /// Only `--store-url` turns on the co-signer, database grants, the record writer and the relay.
    #[tokio::test]
    async fn a_hub_without_a_store_url_turns_on_no_store_gated_part() {
        let co_signer = builder()
            .with_co_signer(Arc::new(Keypair::new_from_array([3; 32])))
            .derived(root())
            .await;
        assert!(matches!(
            co_signer.err(),
            Some(RingRpcError::Cosigner(
                RingCosignerError::CoSignerStoreMissing
            ))
        ));
        let grants = builder()
            .with_reader_grants(ReaderGrantStore::Database)
            .with_grant_rows_key(GrantRowsSecret::from_bytes([5; 32]))
            .derived(root())
            .await;
        assert!(matches!(
            grants.err(),
            Some(RingRpcError::Audit(RingAuditError::GrantStoreMissing))
        ));
        let hub = builder()
            .with_storage_chain(
                Arc::new(RingAccounts(None)),
                Arc::new(Keypair::new_from_array([4; 32])),
            )
            .derived(root())
            .await
            .expect("hub");
        assert!(hub.record_writer().is_err());
        assert!(hub.relay().is_none());
    }
}
