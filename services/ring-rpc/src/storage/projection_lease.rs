//! One replica writes a ring's projection, every other replica reloads what it committed.
//!
//! A claim takes the lease row when its instance renews it, when a restart of its replica returns,
//! or when its expiry passed, and every new instance moves the fencing token. Every projection
//! transaction first renews the row by replica and token.
//! The row lock orders that write against a concurrent claim, so a writer whose token moved
//! commits nothing. Read before `storage/spent_index.rs`.

use sea_orm::{ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait};
use solana_address::Address;

use ring_service_core::{database::LeaseHolder, sql};

/// Seconds a lease outlives its last claim or commit, a few backfill intervals.
pub(crate) const PROJECTION_LEASE_TTL: u64 = 90;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProjectionLease {
    ring: Address,
    holder: LeaseHolder,
    token: i64,
}

#[must_use]
pub(crate) struct LeaseClaim<'a> {
    pub db: &'a DatabaseConnection,
    pub ring: Address,
    pub holder: LeaseHolder,
    /// Unix seconds.
    pub now: u64,
}

impl LeaseClaim<'_> {
    /// `None` while another replica's lease has not expired.
    pub(crate) async fn claim(self) -> Result<Option<ProjectionLease>, DbErr> {
        let Self {
            db,
            ring,
            holder,
            now,
        } = self;
        let transaction = db.begin().await?;
        // 1. The same instance renews in place. A restart of its replica takes the lease at once and
        //    any other replica only after expiry, both bump the token every older commit names.
        transaction
            .execute(sql::statement(
                &transaction,
                "INSERT INTO projection_leases(ring,holder,instance,token,expires_at) \
                 VALUES($1,$2,$3,1,$4) ON CONFLICT(ring) DO UPDATE SET \
                 token=CASE WHEN projection_leases.holder=excluded.holder \
                 AND projection_leases.instance=excluded.instance \
                 THEN projection_leases.token ELSE projection_leases.token+1 END, \
                 holder=excluded.holder, instance=excluded.instance, \
                 expires_at=excluded.expires_at \
                 WHERE projection_leases.holder=excluded.holder OR projection_leases.expires_at<=$5",
                vec![
                    sql::bytes(ring.as_ref()),
                    sql::bytes(&holder.replica),
                    sql::bytes(&holder.instance),
                    sql::integer(now.saturating_add(PROJECTION_LEASE_TTL))?,
                    sql::integer(now)?,
                ],
            ))
            .await?;
        // 2. The row read back names the holder, a refused update leaves the other holder in it.
        let row = transaction
            .query_one(sql::statement(
                &transaction,
                "SELECT holder,instance,token FROM projection_leases WHERE ring=$1",
                vec![sql::bytes(ring.as_ref())],
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("projection lease row is missing".into()))?;
        transaction.commit().await?;
        let held = row.try_get::<Vec<u8>>("", "holder")? == holder.replica
            && row.try_get::<Vec<u8>>("", "instance")? == holder.instance;
        Ok(held.then_some(ProjectionLease {
            ring,
            holder,
            token: row.try_get("", "token")?,
        }))
    }
}

impl ProjectionLease {
    /// The first write of every projection transaction, it renews the lease or refuses the commit.
    pub(crate) async fn fence(
        &self,
        transaction: &DatabaseTransaction,
        now: u64,
    ) -> Result<(), DbErr> {
        let renewed = transaction
            .execute(sql::statement(
                transaction,
                "UPDATE projection_leases SET expires_at=$1 WHERE ring=$2 AND holder=$3 \
                 AND token=$4 AND expires_at>$5",
                vec![
                    sql::integer(now.saturating_add(PROJECTION_LEASE_TTL))?,
                    sql::bytes(self.ring.as_ref()),
                    sql::bytes(&self.holder.replica),
                    self.token.into(),
                    sql::integer(now)?,
                ],
            ))
            .await?
            .rows_affected();
        if renewed != 1 {
            return Err(DbErr::Custom(
                "the projection lease expired or moved to another replica".into(),
            ));
        }
        Ok(())
    }
}
