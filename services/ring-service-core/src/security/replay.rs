use std::sync::atomic::AtomicU64;

use solana_address::Address;

use crate::{
    authorize::{AuthorizedNonce, Unauthorized, AUTH_SKEW},
    database::Store,
    error::ServiceError,
    replay_nonces::NonceBook,
};

/// Every replica that shares the store refuses a nonce one of them accepted.
pub struct ReplayGuard {
    store: Store,
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
    pub fn new(store: Store) -> Self {
        Self {
            store,
            last_pruned: AtomicU64::new(0),
        }
    }

    pub async fn accept(&self, authorized: AuthorizedNonce, now: u64) -> Result<(), ServiceError> {
        let check = authorized.into_check(now);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authorize::PendingNonce;

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
        let guard = ReplayGuard::new(Store::in_memory().await.expect("store"));
        guard
            .accept(authorized(RING, 7, 1_000).await, 1_000)
            .await
            .expect("first use");
        assert!(matches!(
            guard
                .accept(authorized(RING, 7, 1_000).await, 1_000 + skew)
                .await,
            Err(ServiceError::Unauthorized(Unauthorized::Replay))
        ));
        guard
            .accept(authorized(other, 7, 1_000).await, 1_000)
            .await
            .expect("another ring");
        assert!(matches!(
            guard
                .accept(authorized(RING, 8, 1_000).await, 1_000 + skew + 1)
                .await,
            Err(ServiceError::Unauthorized(Unauthorized::StaleTimestamp))
        ));
    }
}
