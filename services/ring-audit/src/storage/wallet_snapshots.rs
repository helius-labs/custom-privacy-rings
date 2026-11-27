use std::num::NonZeroUsize;

use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DatabaseTransaction, IsolationLevel,
    TransactionTrait, Value,
};
use serde::{Deserialize, Serialize};
use solana_address::Address;
use zolana_ring_client::rpc::{WalletBalance, WalletEnrollment};

use crate::{
    error::{RingAuditError, ServiceError},
    owner_notes::{snapshot_history_through, OwnerNotesPage, OWNER_ROW_BATCH},
    spent_index::{read_revision, read_snapshot, NoteSeal},
    utxos::{Balances, OwnerPage, OwnerRange, WalletCursor, WalletCursorScope, WalletStatusQuery},
};
use ring_service_core::sql;

const SNAPSHOT_TTL_SECONDS: u64 = 300;

#[derive(Clone, Copy, Debug)]
pub struct WalletSnapshotLimits {
    pub store: NonZeroUsize,
    pub principal_wallet: NonZeroUsize,
}

impl Default for WalletSnapshotLimits {
    fn default() -> Self {
        Self {
            store: const { NonZeroUsize::new(1024).expect("nonzero snapshot limit") },
            principal_wallet: const { NonZeroUsize::new(2).expect("nonzero wallet snapshot limit") },
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SnapshotMetadata {
    principal: Vec<u8>,
    wallet: [u8; 32],
    pub expires_at: u64,
    pub through: u64,
    revision: Option<i64>,
    pub enrollment: WalletEnrollment,
    pub balances: Vec<WalletBalance>,
}

pub(crate) struct SnapshotPage {
    pub page: OwnerPage,
    pub metadata: SnapshotMetadata,
    pub id: Option<[u8; 32]>,
}

pub struct WalletSnapshotRelease {
    pub ring: Address,
    pub wallet: Address,
    pub principal: [u8; 34],
    pub cursor: WalletCursor,
    pub now: u64,
}

pub(crate) struct WalletSnapshots<'a> {
    pub db: &'a DatabaseConnection,
    pub seal: &'a NoteSeal,
    pub limits: WalletSnapshotLimits,
}

pub(crate) struct WalletSnapshotCleanup<'a> {
    pub db: &'a DatabaseConnection,
    pub now: u64,
}

impl SnapshotMetadata {
    fn new(query: WalletStatusQuery, through: u64) -> Result<Self, RingAuditError> {
        Ok(Self {
            principal: query.principal.to_vec(),
            wallet: query.wallet.to_bytes(),
            expires_at: query
                .now
                .checked_add(SNAPSHOT_TTL_SECONDS)
                .ok_or(RingAuditError::Service(ServiceError::StateUnavailable))?,
            through,
            revision: None,
            enrollment: query.enrollment,
            balances: Vec::new(),
        })
    }

    fn authorize(&self, principal: &[u8; 34], wallet: &Address) -> Result<(), RingAuditError> {
        if self.principal != principal.as_slice() || self.wallet != wallet.to_bytes() {
            return Err(RingAuditError::InvalidCursor);
        }
        Ok(())
    }
}

impl WalletSnapshotRelease {
    pub(crate) fn validate(&self) -> Result<(), RingAuditError> {
        self.cursor.validate_scope(WalletCursorScope {
            ring: self.ring,
            wallet: self.wallet,
        })
    }
}

impl WalletSnapshots<'_> {
    pub(crate) async fn read(
        &self,
        query: WalletStatusQuery,
    ) -> Result<SnapshotPage, RingAuditError> {
        let cursor = query.after.ok_or(RingAuditError::InvalidCursor)?;
        let transaction = read_snapshot(self.db).await?;
        let metadata = self
            .metadata(
                &transaction,
                SnapshotLookup {
                    id: cursor.snapshot,
                    ring: query.ring,
                },
            )
            .await?
            .ok_or(RingAuditError::WalletSnapshotExpired)?;
        metadata.authorize(&query.principal, &query.wallet)?;
        if metadata.expires_at <= query.now {
            return Err(RingAuditError::WalletSnapshotExpired);
        }
        let member = zolana_ring_policy::Member::owner_tag(query.wallet.as_array())
            .map_err(|_| RingAuditError::InvalidCursor)?;
        let range = OwnerRange {
            member: *member.as_bytes(),
            after: Some(cursor.after),
            limit: query.limit,
        };
        let fetched = i64::try_from(query.limit.saturating_add(1))
            .map_err(|_| RingAuditError::InvalidPage)?;
        let rows = sql::Query::new(
            "SELECT note_tag,sealed,spent_slot FROM wallet_snapshot_notes WHERE \
             snapshot_id=$1 AND note_tag>$2 ORDER BY note_tag LIMIT $3",
            vec![
                sql::bytes(&cursor.snapshot),
                sql::bytes(&cursor.after),
                fetched.into(),
            ],
        )
        .all(&transaction)
        .await?;
        let reader = OwnerNotesPage {
            ring: query.ring,
            seal: self.seal,
            range,
        };
        let page = OwnerPage {
            more: rows.len() > query.limit,
            notes: rows
                .iter()
                .take(query.limit)
                .map(|row| reader.open(row, &range.member))
                .collect::<Result<_, _>>()?,
        };
        transaction.commit().await?;
        Ok(SnapshotPage {
            page,
            metadata,
            id: Some(cursor.snapshot),
        })
    }

    pub(crate) async fn capture(
        &self,
        query: WalletStatusQuery,
    ) -> Result<SnapshotPage, RingAuditError> {
        let member = zolana_ring_policy::Member::owner_tag(query.wallet.as_array())
            .map_err(|_| RingAuditError::InvalidCursor)?;
        let range = OwnerRange {
            member: *member.as_bytes(),
            after: None,
            limit: query.limit,
        };
        let reader = OwnerNotesPage {
            ring: query.ring,
            seal: self.seal,
            range,
        };
        let initial = read_snapshot(self.db).await?;
        let page = reader.read(&initial).await?;
        if !page.more {
            let through = snapshot_history_through(&initial, query.ring).await?;
            let mut metadata = SnapshotMetadata::new(query, through)?;
            metadata.revision = Some(read_revision(&initial, &query.ring).await?);
            let mut balances = Balances::default();
            for note in &page.notes {
                balances.add(note, through);
            }
            metadata.balances = balances.into_values();
            initial.commit().await?;
            return Ok(SnapshotPage {
                page,
                metadata,
                id: None,
            });
        }
        initial.commit().await?;
        let transaction = match self.db.get_database_backend() {
            DatabaseBackend::Postgres => {
                let transaction = self
                    .db
                    .begin_with_config(Some(IsolationLevel::RepeatableRead), None)
                    .await?;
                // Before the snapshot starts, a row update waited on inside it fails to serialize.
                transaction
                    .execute_unprepared("LOCK TABLE wallet_snapshot_admission IN EXCLUSIVE MODE")
                    .await?;
                transaction
            }
            DatabaseBackend::Sqlite | DatabaseBackend::MySql => {
                let transaction = self.db.begin().await?;
                let admitted = transaction
                    .execute_unprepared("UPDATE wallet_snapshot_admission SET id=id WHERE id=1")
                    .await?;
                if admitted.rows_affected() != 1 {
                    return Err(RingAuditError::Service(ServiceError::StateUnavailable));
                }
                transaction
            }
        };
        let page = reader.read(&transaction).await?;
        let through = snapshot_history_through(&transaction, query.ring).await?;
        let mut metadata = SnapshotMetadata::new(query, through)?;
        metadata.revision = Some(read_revision(&transaction, &query.ring).await?);
        let mut balances = Balances::default();
        if !page.more {
            for note in &page.notes {
                balances.add(note, through);
            }
            metadata.balances = balances.into_values();
            transaction.commit().await?;
            return Ok(SnapshotPage {
                page,
                metadata,
                id: None,
            });
        }
        let scope = self
            .seal
            .tags
            .snapshot_scope(&query.principal, &query.wallet)?;
        self.admit(
            &transaction,
            SnapshotAdmission {
                ring: query.ring,
                scope,
            },
        )
        .await?;
        let id: [u8; 32] = rand::random();
        sql::Query::new(
            "INSERT INTO wallet_snapshots(id,ring,scope_tag,expires_at,sealed) \
             VALUES($1,$2,$3,$4,$5)",
            vec![
                sql::bytes(&id),
                sql::bytes(query.ring.as_ref()),
                sql::bytes(&scope),
                sql::integer(metadata.expires_at)?,
                Vec::<u8>::new().into(),
            ],
        )
        .execute(&transaction)
        .await?;
        let owner_tag = self.seal.tags.owner_tag(&range.member)?;
        let mut batch = OwnerNotesPage {
            range: OwnerRange {
                limit: OWNER_ROW_BATCH,
                ..range
            },
            ..reader
        };
        loop {
            let captured = batch.read(&transaction).await?;
            for note in &captured.notes {
                balances.add(note, through);
            }
            let Some(last) = captured.notes.last() else {
                break;
            };
            let mut values: Vec<Value> = vec![
                sql::bytes(&id),
                sql::bytes(query.ring.as_ref()),
                sql::bytes(&owner_tag),
                sql::bytes(&last.key),
            ];
            let after_clause = match batch.range.after {
                Some(after) => {
                    values.push(sql::bytes(&after));
                    " AND note_tag>$5"
                }
                None => "",
            };
            sql::Query::new(
                format!(
                    "INSERT INTO wallet_snapshot_notes(snapshot_id,note_tag,sealed,spent_slot) \
                     SELECT $1,note_tag,sealed,spent_slot FROM owner_notes WHERE ring=$2 AND \
                     owner_tag=$3 AND note_tag<=$4{after_clause}"
                ),
                values,
            )
            .execute(&transaction)
            .await?;
            if !captured.more {
                break;
            }
            batch.range.after = Some(last.key);
        }
        metadata.balances = balances.into_values();
        let body = serde_json::to_vec(&metadata)
            .map_err(|_| RingAuditError::Service(ServiceError::StateUnavailable))?;
        let key = self.seal.tags.snapshot_metadata(&id)?;
        let sealed = self.seal.seal(&key, &body)?;
        sql::Query::new(
            "UPDATE wallet_snapshots SET sealed=$2 WHERE id=$1",
            vec![sql::bytes(&id), sealed.into()],
        )
        .execute(&transaction)
        .await?;
        transaction.commit().await?;
        Ok(SnapshotPage {
            page,
            metadata,
            id: Some(id),
        })
    }

    pub(crate) async fn release(
        &self,
        release: WalletSnapshotRelease,
    ) -> Result<(), RingAuditError> {
        release.validate()?;
        let transaction = self.db.begin().await?;
        let metadata = self
            .metadata(
                &transaction,
                SnapshotLookup {
                    id: release.cursor.snapshot,
                    ring: release.ring,
                },
            )
            .await?;
        if let Some(metadata) = metadata {
            metadata.authorize(&release.principal, &release.wallet)?;
            sql::Query::new(
                "UPDATE wallet_snapshots SET expires_at=0 WHERE id=$1 AND ring=$2",
                vec![
                    sql::bytes(&release.cursor.snapshot),
                    sql::bytes(release.ring.as_ref()),
                ],
            )
            .execute(&transaction)
            .await?;
        }
        transaction.commit().await?;
        WalletSnapshotCleanup {
            db: self.db,
            now: release.now,
        }
        .run()
        .await?;
        Ok(())
    }

    async fn metadata(
        &self,
        db: &impl ConnectionTrait,
        lookup: SnapshotLookup,
    ) -> Result<Option<SnapshotMetadata>, RingAuditError> {
        let row = sql::Query::new(
            "SELECT expires_at,sealed FROM wallet_snapshots WHERE id=$1 AND ring=$2",
            vec![sql::bytes(&lookup.id), sql::bytes(lookup.ring.as_ref())],
        )
        .one(db)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let stored_expiry = row.try_get::<i64>("", "expires_at")?;
        let key = self.seal.tags.snapshot_metadata(&lookup.id)?;
        let body = self
            .seal
            .open(&key, &row.try_get::<Vec<u8>>("", "sealed")?)?;
        let mut metadata: SnapshotMetadata = serde_json::from_slice(&body)
            .map_err(|_| RingAuditError::Service(ServiceError::StateUnavailable))?;
        if stored_expiry == 0 {
            metadata.expires_at = 0;
        } else if u64::try_from(stored_expiry).ok() != Some(metadata.expires_at) {
            return Err(RingAuditError::Service(ServiceError::StateUnavailable));
        }
        Ok(Some(metadata))
    }

    async fn admit(
        &self,
        db: &DatabaseTransaction,
        admission: SnapshotAdmission,
    ) -> Result<(), RingAuditError> {
        let row = sql::Query::new(
            "SELECT COUNT(*) AS total, COALESCE(SUM(CASE WHEN ring=$1 AND scope_tag=$2 \
             THEN 1 ELSE 0 END),0) AS scoped FROM wallet_snapshots",
            vec![
                sql::bytes(admission.ring.as_ref()),
                sql::bytes(&admission.scope),
            ],
        )
        .one(db)
        .await?
        .ok_or(RingAuditError::Service(ServiceError::StateUnavailable))?;
        let total = row.try_get::<i64>("", "total")?;
        let scoped = row.try_get::<i64>("", "scoped")?;
        if usize::try_from(total)
            .map_err(|_| RingAuditError::Service(ServiceError::StateUnavailable))?
            >= self.limits.store.get()
            || usize::try_from(scoped)
                .map_err(|_| RingAuditError::Service(ServiceError::StateUnavailable))?
                >= self.limits.principal_wallet.get()
        {
            return Err(RingAuditError::WalletSnapshotCapacity);
        }
        Ok(())
    }
}

struct SnapshotLookup {
    id: [u8; 32],
    ring: Address,
}

struct SnapshotAdmission {
    ring: Address,
    scope: [u8; 32],
}

impl WalletSnapshotCleanup<'_> {
    pub(crate) async fn run(self) -> Result<usize, RingAuditError> {
        let transaction = self.db.begin().await?;
        let row = sql::Query::new(
            "SELECT id FROM wallet_snapshots WHERE expires_at<=$1 ORDER BY expires_at,id LIMIT 1",
            vec![sql::integer(self.now)?],
        )
        .one(&transaction)
        .await?;
        let Some(row) = row else {
            transaction.commit().await?;
            return Ok(0);
        };
        let id = row.try_get::<Vec<u8>>("", "id")?;
        let deleted = sql::Query::new(
            "DELETE FROM wallet_snapshot_notes WHERE snapshot_id=$1 AND note_tag IN \
             (SELECT note_tag FROM wallet_snapshot_notes WHERE snapshot_id=$1 ORDER BY \
             note_tag LIMIT $2)",
            vec![id.clone().into(), sql::count(OWNER_ROW_BATCH)?],
        )
        .execute(&transaction)
        .await?;
        sql::Query::new(
            "DELETE FROM wallet_snapshots WHERE id=$1 AND NOT EXISTS (SELECT 1 FROM \
             wallet_snapshot_notes WHERE snapshot_id=$1)",
            vec![id.into()],
        )
        .execute(&transaction)
        .await?;
        transaction.commit().await?;
        usize::try_from(deleted)
            .map_err(|_| RingAuditError::Service(ServiceError::StateUnavailable))
    }
}
