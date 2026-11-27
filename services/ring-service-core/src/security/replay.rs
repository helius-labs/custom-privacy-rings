use std::{future::Future, sync::atomic::AtomicU64};

use solana_address::Address;

use crate::{
    authorize::{Authorized, AuthorizedNonce, PendingNonce, Unauthorized, AUTH_SKEW},
    clock::Clock,
    database::Store,
    error::ServiceError,
    replay_nonces::NonceBook,
};

/// Every replica that shares the store refuses a nonce one of them accepted.
pub struct ReplayGuard {
    store: Store,
    clock: Clock,
    last_pruned: AtomicU64,
}

#[must_use]
pub(crate) struct ReplayCheck {
    pub(crate) ring: Address,
    pub(crate) nonce: [u8; 32],
    pub(crate) timestamp: u64,
    pub(crate) now: u64,
}

impl ReplayGuard {
    pub fn new(store: Store, clock: Clock) -> Self {
        Self {
            store,
            clock,
            last_pruned: AtomicU64::new(0),
        }
    }

    /// A refused authorization spends no nonce.
    pub async fn authorize_once<T, E>(
        &self,
        pending: PendingNonce,
        authorize: impl Future<Output = Result<T, E>>,
    ) -> Result<T, E>
    where
        E: From<ServiceError>,
    {
        let Authorized { value, nonce } = pending.authorize(authorize).await?;
        self.accept(nonce).await?;
        Ok(value)
    }

    /// A timestamp that left the skew while the request ran is refused.
    pub async fn accept(&self, authorized: AuthorizedNonce) -> Result<(), ServiceError> {
        let check = authorized.into_check(self.clock.now()?);
        check.fresh()?;
        let book = NonceBook {
            conn: self.store.connection(),
        };
        if book.spend(&check, &self.last_pruned).await? {
            Ok(())
        } else {
            Err(Unauthorized::Replay.into())
        }
    }
}

impl ReplayCheck {
    fn fresh(&self) -> Result<(), Unauthorized> {
        if self.now.abs_diff(self.timestamp) > AUTH_SKEW.as_secs() {
            return Err(Unauthorized::StaleTimestamp);
        }
        Ok(())
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
pub async fn spent_nonces(store: &Store) -> Result<i64, sea_orm::DbErr> {
    crate::sql::Query::new("SELECT COUNT(*) AS rows FROM replay_nonces", Vec::new())
        .one(store.connection())
        .await?
        .ok_or_else(|| sea_orm::DbErr::Custom("count returned no row".to_owned()))?
        .try_get("", "rows")
}

#[cfg(test)]
mod tests {
    use super::*;

    const RING: Address = Address::new_from_array([1; 32]);

    async fn authorized(ring: Address, nonce: u8, timestamp: u64) -> AuthorizedNonce {
        PendingNonce {
            ring,
            nonce: [nonce; 32],
            timestamp,
        }
        .authorize(async { Ok::<_, Unauthorized>(()) })
        .await
        .expect("authorized")
        .nonce
    }

    #[tokio::test]
    async fn every_guard_refuses_a_replay_and_a_stale_timestamp_per_ring() {
        let skew = AUTH_SKEW.as_secs();
        let other = Address::new_from_array([2; 32]);
        let store = Store::in_memory().await.expect("store");
        let guard = |now| ReplayGuard::new(store.clone(), Clock::Fixed(now));
        guard(1_000)
            .accept(authorized(RING, 7, 1_000).await)
            .await
            .expect("first use");
        assert!(matches!(
            guard(1_000 + skew)
                .accept(authorized(RING, 7, 1_000).await)
                .await,
            Err(ServiceError::Unauthorized(Unauthorized::Replay))
        ));
        guard(1_000)
            .accept(authorized(other, 7, 1_000).await)
            .await
            .expect("another ring");
        assert!(matches!(
            guard(1_000 + skew + 1)
                .accept(authorized(RING, 8, 1_000).await)
                .await,
            Err(ServiceError::Unauthorized(Unauthorized::StaleTimestamp))
        ));
    }

    #[tokio::test]
    async fn a_refused_authorization_spends_no_nonce() {
        let store = Store::in_memory().await.expect("store");
        let guard = ReplayGuard::new(store.clone(), Clock::Fixed(1_000));
        let pending = || PendingNonce {
            ring: RING,
            nonce: [7; 32],
            timestamp: 1_000,
        };
        let refused = guard
            .authorize_once(pending(), async {
                Err::<(), ServiceError>(Unauthorized::NotGranted.into())
            })
            .await;
        assert!(matches!(
            refused,
            Err(ServiceError::Unauthorized(Unauthorized::NotGranted))
        ));
        assert_eq!(spent_nonces(&store).await.expect("rows"), 0);
        guard
            .authorize_once(pending(), async { Ok::<_, ServiceError>(()) })
            .await
            .expect("first use");
        assert!(matches!(
            guard
                .authorize_once(pending(), async { Ok::<_, ServiceError>(()) })
                .await,
            Err(ServiceError::Unauthorized(Unauthorized::Replay))
        ));
    }
}
