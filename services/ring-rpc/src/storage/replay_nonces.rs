use std::sync::atomic::{AtomicU64, Ordering};

use sea_orm::{ConnectionTrait, DbErr, Statement, Value};

use crate::{authorize::AUTH_SKEW, replay::ReplayCheck};

pub(crate) struct NonceBook<'a, C> {
    pub conn: &'a C,
}

impl<C: ConnectionTrait> NonceBook<'_, C> {
    /// False when any process sharing the store already spent the nonce.
    pub async fn spend(&self, check: &ReplayCheck, last_pruned: &AtomicU64) -> Result<bool, DbErr> {
        if claim_prune(last_pruned, check.now) {
            // A replica whose clock lags by up to one skew still finds the nonce.
            let expired = check.now.saturating_sub(2 * AUTH_SKEW.as_secs());
            let prune = "DELETE FROM replay_nonces WHERE signed_at<$1";
            self.conn
                .execute(self.statement(prune, vec![seconds_value(expired)?]))
                .await?;
        }
        let insert = "INSERT INTO replay_nonces(ring,nonce,signed_at) VALUES($1,$2,$3) \
                      ON CONFLICT(ring,nonce) DO NOTHING";
        let values = vec![
            Value::from(check.ring.as_ref().to_vec()),
            Value::from(check.nonce.to_vec()),
            seconds_value(check.timestamp)?,
        ];
        let inserted = self.conn.execute(self.statement(insert, values)).await?;
        Ok(inserted.rows_affected() == 1)
    }

    fn statement(&self, sql: &str, values: Vec<Value>) -> Statement {
        Statement::from_sql_and_values(self.conn.get_database_backend(), sql, values)
    }
}

/// Zero means unpruned.
fn claim_prune(last_pruned: &AtomicU64, now: u64) -> bool {
    let interval = AUTH_SKEW.as_secs();
    let stamped = now.saturating_add(1);
    let mut previous = last_pruned.load(Ordering::Relaxed);
    loop {
        let due = previous == 0 || now.abs_diff(previous - 1) >= interval;
        if !due {
            return false;
        }
        match last_pruned.compare_exchange_weak(
            previous,
            stamped,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => return true,
            Err(actual) => previous = actual,
        }
    }
}

fn seconds_value(seconds: u64) -> Result<Value, DbErr> {
    i64::try_from(seconds)
        .map(Value::from)
        .map_err(|_| DbErr::Custom("nonce timestamp exceeds the column range".into()))
}

#[cfg(test)]
mod tests {
    use solana_address::Address;

    use super::*;
    use crate::database::Store;

    const RING: Address = Address::new_from_array([9; 32]);

    fn check(nonce: u8, timestamp: u64, now: u64) -> ReplayCheck {
        ReplayCheck {
            ring: RING,
            nonce: [nonce; 32],
            timestamp,
            now,
        }
    }

    #[tokio::test]
    async fn expired_nonce_pruning_runs_at_most_once_per_skew_window() {
        let store = Store::open("sqlite::memory:", 1).await.expect("store");
        let book = NonceBook {
            conn: store.connection(),
        };
        let last_pruned = AtomicU64::new(0);
        let skew = AUTH_SKEW.as_secs();

        assert!(book
            .spend(&check(1, 1_000, 1_000), &last_pruned)
            .await
            .expect("first"));

        let stale_at = 1_000u64;
        let within = 1_000 + skew - 1;
        store
            .connection()
            .execute(Statement::from_sql_and_values(
                store.connection().get_database_backend(),
                "INSERT INTO replay_nonces(ring,nonce,signed_at) VALUES($1,$2,$3)",
                vec![
                    Value::from(RING.as_ref().to_vec()),
                    Value::from([99u8; 32].to_vec()),
                    Value::from(100i64),
                ],
            ))
            .await
            .expect("stale");

        assert!(book
            .spend(&check(2, within, within), &last_pruned)
            .await
            .expect("within"));
        let count_within: i64 = store
            .connection()
            .query_one(Statement::from_string(
                store.connection().get_database_backend(),
                "SELECT COUNT(*) AS count FROM replay_nonces WHERE signed_at=100",
            ))
            .await
            .expect("count")
            .expect("row")
            .try_get("", "count")
            .expect("value");
        assert_eq!(count_within, 1, "prune must be throttled inside AUTH_SKEW");

        let after = stale_at + skew;
        assert!(book
            .spend(&check(3, after, after), &last_pruned)
            .await
            .expect("after"));
        let count_after: i64 = store
            .connection()
            .query_one(Statement::from_string(
                store.connection().get_database_backend(),
                "SELECT COUNT(*) AS count FROM replay_nonces WHERE signed_at=100",
            ))
            .await
            .expect("count")
            .expect("row")
            .try_get("", "count")
            .expect("value");
        assert_eq!(count_after, 0, "prune runs again after AUTH_SKEW");
    }
}
