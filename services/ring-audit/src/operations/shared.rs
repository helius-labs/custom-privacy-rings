use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use solana_address::Address;
use spr_client::ClientError;
use spr_keypair::X25519Keypair;
use tokio::sync::Mutex as AsyncMutex;
use zolana_ring_client::ReaderKey;
use zolana_ring_transaction::RingAssets;

use ring_service_core::{
    authorize::Unauthorized,
    clock::Clock,
    database::ServiceStore,
    keys::{AuditorKeyDerivation, GrantRowsSecret, KeySource},
    origins::Origins,
    replay::ReplayGuard,
    upstream::TransactionSource,
};
use ring_writer::record_update::ReaderGrantStore;
use zolana_ring_indexer::redact::redacted;

use crate::{
    audit::{AuditService, AuditServiceInit, ASSET_REFRESH_INTERVAL},
    error::RingAuditError,
    grant_rows::{GrantRowKey, GrantRows},
    grants::{GrantQuery, Grants, ReadScope, TreeGrants},
    spent_index::NoteSeal,
    utxos::{AuditStore, Backing, MAX_CACHED_ENTRIES},
    wallet_snapshots::{WalletSnapshotCleanup, WalletSnapshotLimits},
};

pub struct Shared<S> {
    pub source: S,
    /// Captured at boot, every authority signature and derived key binds to it.
    pub genesis_hash: [u8; 32],
    pub service: Address,
    keys: KeySource,
    pub clock: Clock,
    assets: AsyncMutex<AssetCache>,
    pub origins: Origins,
    pub replay: ReplayGuard,
    pub(crate) active_readers: Mutex<HashSet<(Address, ReaderKey)>>,
    /// One per ring an authorized read or the backfill opened.
    stores: Mutex<HashMap<Address, Arc<AuditStore>>>,
    snapshot_limits: WalletSnapshotLimits,
    pub grants: Grants,
    pub database: ServiceStore,
}

#[must_use]
pub struct SharedInit<S> {
    pub source: S,
    pub genesis_hash: [u8; 32],
    pub service: Address,
    pub keys: KeySource,
    pub clock: Clock,
    pub assets: Option<RingAssets>,
    pub origins: Origins,
    pub store: ServiceStore,
    pub snapshot_limits: WalletSnapshotLimits,
    pub reader_grants: ReaderGrantStore,
    pub grant_rows: Option<GrantRowsSecret>,
}

struct AssetCache {
    registry: RingAssets,
    refresh: RefreshState,
}

enum RefreshState {
    Never,
    Succeeded(tokio::time::Instant),
    Failed(tokio::time::Instant),
}

impl<S> SharedInit<S> {
    pub fn build(self) -> Result<Shared<S>, RingAuditError> {
        let grants = match self.reader_grants {
            ReaderGrantStore::Tree => Grants::Tree,
            ReaderGrantStore::Database => Grants::Database(Box::new(GrantRows::new(
                self.store
                    .durable()
                    .cloned()
                    .ok_or(RingAuditError::GrantStoreMissing)?,
                GrantRowKey::derive(
                    self.grant_rows
                        .as_ref()
                        .ok_or(RingAuditError::GrantRowsKeyMissing)?
                        .as_bytes(),
                )?,
            ))),
        };
        let assets = match self.assets {
            Some(assets) => assets,
            None => RingAssets::new([])?,
        };
        Ok(Shared {
            source: self.source,
            genesis_hash: self.genesis_hash,
            service: self.service,
            keys: self.keys,
            clock: self.clock,
            assets: AsyncMutex::new(AssetCache {
                registry: assets,
                refresh: RefreshState::Never,
            }),
            origins: self.origins,
            replay: ReplayGuard::new(self.store.store().clone(), self.clock),
            active_readers: Mutex::new(HashSet::new()),
            stores: Mutex::new(HashMap::new()),
            snapshot_limits: self.snapshot_limits,
            grants,
            database: self.store,
        })
    }
}

impl<S> Shared<S> {
    pub const fn keys(&self) -> &KeySource {
        &self.keys
    }

    pub fn store(&self, ring: Address) -> Arc<AuditStore> {
        let mut stores = self.stores.lock().unwrap_or_else(PoisonError::into_inner);
        stores
            .entry(ring)
            .or_insert_with(|| {
                Arc::new(
                    AuditStore::new(ring, self.backing(ring))
                        .with_snapshot_limits(self.snapshot_limits),
                )
            })
            .clone()
    }

    fn backing(&self, ring: Address) -> Backing {
        let seal = self
            .auditor(Some(ring))
            .and_then(|(ring, auditor)| Ok(NoteSeal::derive(&auditor, ring)?));
        match seal {
            Ok(seal) => Backing::Database {
                store: self.database.store().clone(),
                ring,
                seal: Box::new(seal),
                capacity: MAX_CACHED_ENTRIES,
            },
            Err(error) => {
                log::error!(
                    "the projection of ring {ring} is unusable because {}",
                    redacted(&error)
                );
                Backing::Unavailable
            }
        }
    }

    pub fn store_rings(&self) -> Vec<Address> {
        let stores = self.stores.lock().unwrap_or_else(PoisonError::into_inner);
        stores.keys().copied().collect()
    }

    pub fn service(
        self: &Arc<Self>,
        ring: Option<Address>,
    ) -> Result<AuditService<S>, RingAuditError> {
        let (ring, auditor) = self.auditor(ring)?;
        Ok(AuditServiceInit {
            ring,
            auditor,
            shared: self.clone(),
        }
        .build())
    }

    /// `None` names the one ring a local key serves.
    fn auditor(&self, ring: Option<Address>) -> Result<(Address, X25519Keypair), RingAuditError> {
        match &self.keys {
            KeySource::Local {
                ring: served,
                auditor,
            } => {
                self.validate_ring(ring.unwrap_or(*served))?;
                Ok((*served, auditor.clone()))
            }
            KeySource::Derived(root) => {
                let ring = ring.ok_or(RingAuditError::RingRequired)?;
                let auditor = AuditorKeyDerivation {
                    root,
                    genesis_hash: &self.genesis_hash,
                    ring,
                }
                .derive()?;
                Ok((ring, auditor))
            }
        }
    }

    pub(crate) fn validate_ring(&self, ring: Address) -> Result<(), RingAuditError> {
        match &self.keys {
            KeySource::Local { ring: served, .. } if ring != *served => {
                Err(RingAuditError::RingNotServed)
            }
            _ => Ok(()),
        }
    }

    pub async fn cleanup_wallet_snapshots(&self) -> Result<(), RingAuditError> {
        let cleanup = WalletSnapshotCleanup {
            db: self.database.store().connection(),
            now: self.clock.now()?,
        };
        while cleanup.run().await? {}
        Ok(())
    }
}

impl<S: TransactionSource> Shared<S> {
    /// The policy authority, or the upgrade authority before `Initialize`.
    pub async fn require_ring_authority(
        &self,
        ring: Address,
        signer: Address,
    ) -> Result<(), RingAuditError> {
        let (_, auditor) = self.auditor(Some(ring))?;
        let ring_authority = match self.source.ring_config(ring).await? {
            Some(config) => {
                if config.auditor_pubkey != auditor.pubkey() {
                    return Err(Unauthorized::AuditorKeyMismatch.into());
                }
                Some(config.roles().policy)
            }
            None => self.source.upgrade_authority(ring).await?,
        };
        if ring_authority != Some(signer) {
            return Err(Unauthorized::NotRingAuthority.into());
        }
        Ok(())
    }

    pub(crate) async fn read_scope(&self, query: GrantQuery) -> Result<ReadScope, RingAuditError> {
        // A wallet signing for itself holds its own notes, a PDA owner reads through a grant.
        if query.wallet.is_some()
            && matches!(query.reader, ReaderKey::Ed25519(key) if Some(key.address()) == query.wallet)
        {
            return Ok(ReadScope::new(query.wallet));
        }
        if let Some(owner) = query.wallet {
            if self
                .source
                .owner_grant_held(query.ring, owner, query.reader.to_bytes())
                .await?
            {
                return Ok(ReadScope::new(query.wallet));
            }
        }
        // A ring-wide grant covers every query, a wallet grant only a query naming that wallet.
        let granted = match &self.grants {
            Grants::Tree => {
                TreeGrants {
                    source: &self.source,
                }
                .grants(query)
                .await?
            }
            Grants::Database(rows) => rows.grants(query).await?,
        };
        if !granted {
            return Err(Unauthorized::NotGranted.into());
        }
        Ok(ReadScope::new(query.wallet))
    }

    pub async fn cached_assets(&self) -> RingAssets {
        self.assets.lock().await.registry.clone()
    }

    /// A failed read serves the cached registry.
    pub async fn due_assets(&self) -> RingAssets {
        match self.refresh_assets().await {
            Ok(assets) => assets,
            Err(_) => self.cached_assets().await,
        }
    }

    pub(crate) async fn refresh_assets(&self) -> Result<RingAssets, RingAuditError> {
        self.assets_read_within(ASSET_REFRESH_INTERVAL).await
    }

    /// At most one registry read per interval, a failed one blocks retries until it ends.
    async fn assets_read_within(&self, interval: Duration) -> Result<RingAssets, RingAuditError> {
        let mut cache = self.assets.lock().await;
        let refresh_due = match cache.refresh {
            RefreshState::Never => true,
            RefreshState::Succeeded(at) | RefreshState::Failed(at) => at.elapsed() >= interval,
        };
        if refresh_due {
            cache.refresh = RefreshState::Failed(tokio::time::Instant::now());
            cache.registry = self.source.ring_assets().await?;
            cache.refresh = RefreshState::Succeeded(tokio::time::Instant::now());
        } else if matches!(cache.refresh, RefreshState::Failed(_)) {
            return Err(ClientError::Rpc("asset registry is unavailable".to_owned()).into());
        }
        Ok(cache.registry.clone())
    }
}

#[cfg(test)]
mod tests {
    use spr_keypair::X25519Keypair;
    use zolana_ring_client::rpc::WalletEnrollment;

    use super::*;
    use crate::{error::ServiceError, utxos::WalletStatusQuery};
    use ring_service_core::sql;

    async fn shared() -> Shared<()> {
        SharedInit {
            source: (),
            genesis_hash: [1; 32],
            service: Address::default(),
            keys: KeySource::Local {
                ring: Address::new_from_array([8; 32]),
                auditor: X25519Keypair::from_bytes(&[0x41; 32]),
            },
            clock: Clock::Fixed(1_000),
            assets: None,
            origins: Origins::default(),
            store: ServiceStore::internal().await.expect("store"),
            snapshot_limits: WalletSnapshotLimits::default(),
            reader_grants: ReaderGrantStore::Tree,
            grant_rows: None,
        }
        .build()
        .expect("shared")
    }

    #[tokio::test]
    async fn a_ring_whose_seal_does_not_derive_answers_state_unavailable() {
        let other = Address::new_from_array([9; 32]);
        let shared = shared().await;
        let owner = Address::new_from_array([7; 32]);

        let refusal = shared
            .store(other)
            .wallet_status(WalletStatusQuery {
                principal: [1; 34],
                now: 1_000,
                enrollment: WalletEnrollment::NotEnrolled,
                wallet: owner,
                ring: other,
                after: None,
                limit: 10,
            })
            .await;
        assert!(matches!(
            refusal,
            Err(RingAuditError::Service(ServiceError::StateUnavailable))
        ));
    }

    #[tokio::test]
    async fn one_cleanup_removes_every_released_and_expired_snapshot() {
        let shared = shared().await;
        let db = shared.database.store().connection();
        for (seed, expires_at) in [(1_u8, 0_u64), (2, 999), (3, 1_000), (4, 1_001)] {
            sql::Query::new(
                "INSERT INTO wallet_snapshots(id,ring,scope_tag,expires_at,sealed) \
                 VALUES($1,$2,$3,$4,$5)",
                vec![
                    sql::bytes(&[seed; 32]),
                    sql::bytes(&[8; 32]),
                    sql::bytes(&[seed; 32]),
                    sql::integer(expires_at).expect("expiry"),
                    sql::bytes(&[]),
                ],
            )
            .execute(db)
            .await
            .expect("snapshot");
            for note in [1_u8, 2] {
                sql::Query::new(
                    "INSERT INTO wallet_snapshot_notes(snapshot_id,note_tag,sealed) \
                     VALUES($1,$2,$3)",
                    vec![
                        sql::bytes(&[seed; 32]),
                        sql::bytes(&[note; 32]),
                        sql::bytes(&[]),
                    ],
                )
                .execute(db)
                .await
                .expect("note");
            }
        }

        shared.cleanup_wallet_snapshots().await.expect("cleanup");
        let kept: Vec<Vec<u8>> = sql::Query::new("SELECT id FROM wallet_snapshots", Vec::new())
            .all(db)
            .await
            .expect("snapshots")
            .iter()
            .map(|row| row.try_get("", "id").expect("id"))
            .collect();
        assert_eq!(kept, [vec![4; 32]]);
    }
}
