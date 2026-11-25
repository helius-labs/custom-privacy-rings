use std::{
    collections::HashMap,
    sync::{atomic::AtomicU64, Mutex},
};

use solana_address::Address;

use crate::{
    authorize::{AuthorizedNonce, Unauthorized, AUTH_SKEW},
    database::Store,
    error::RingRpcError,
    replay_nonces::NonceBook,
};

const MAX_REPLAY_RINGS: usize = 1_024;
const MAX_REPLAY_ENTRIES: usize = 65_536;

pub(crate) enum ReplayGuard {
    Local(LocalNonces),
    Shared {
        store: Store,
        last_pruned: AtomicU64,
    },
}

#[derive(Default)]
pub(crate) struct LocalNonces(Mutex<HashMap<Address, HashMap<[u8; 32], u64>>>);

#[must_use]
pub(crate) struct ReplayCheck {
    pub(crate) ring: Address,
    pub(crate) nonce: [u8; 32],
    pub(crate) timestamp: u64,
    pub(crate) now: u64,
}

impl ReplayGuard {
    pub(crate) fn shared(store: Store) -> Self {
        Self::Shared {
            store,
            last_pruned: AtomicU64::new(0),
        }
    }

    pub(crate) async fn accept(
        &self,
        authorized: AuthorizedNonce,
        now: u64,
    ) -> Result<(), RingRpcError> {
        let check = authorized.into_check(now);
        match self {
            Self::Local(nonces) => Ok(nonces.accept(check)?),
            Self::Shared { store, last_pruned } => {
                check.fresh()?;
                let book = NonceBook {
                    conn: store.connection(),
                };
                match book.spend(&check, last_pruned).await? {
                    true => Ok(()),
                    false => Err(Unauthorized::Replay.into()),
                }
            }
        }
    }
}

impl LocalNonces {
    /// `now` is read at acceptance, a nonce leaves the set only once its timestamp is stale.
    pub(crate) fn accept(&self, check: ReplayCheck) -> Result<(), Unauthorized> {
        check.fresh()?;
        let mut rings = self.0.lock().map_err(|_| Unauthorized::Replay)?;
        if !rings.contains_key(&check.ring) && rings.len() >= MAX_REPLAY_RINGS {
            rings.retain(|_, entries| {
                entries
                    .retain(|_, timestamp| check.now.abs_diff(*timestamp) <= AUTH_SKEW.as_secs());
                !entries.is_empty()
            });
            if rings.len() >= MAX_REPLAY_RINGS {
                return Err(Unauthorized::Replay);
            }
        }
        let accepted = rings.entry(check.ring).or_default();
        accepted.retain(|_, timestamp| check.now.abs_diff(*timestamp) <= AUTH_SKEW.as_secs());
        if accepted.len() >= MAX_REPLAY_ENTRIES
            || accepted.insert(check.nonce, check.timestamp).is_some()
        {
            return Err(Unauthorized::Replay);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn ring_count(&self) -> usize {
        self.0.lock().expect("nonce set").len()
    }
}

impl ReplayCheck {
    fn fresh(&self) -> Result<(), Unauthorized> {
        match self.now.abs_diff(self.timestamp) > AUTH_SKEW.as_secs() {
            true => Err(Unauthorized::StaleTimestamp),
            false => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RING: Address = Address::new_from_array([1; 32]);

    fn check(ring: Address, nonce: u8, timestamp: u64, now: u64) -> ReplayCheck {
        ReplayCheck {
            ring,
            nonce: [nonce; 32],
            timestamp,
            now,
        }
    }

    /// A nonce is reusable only once it falls outside the window, where the
    /// skew rule already refuses its timestamp.
    #[test]
    fn a_nonce_leaves_the_set_when_it_leaves_the_skew_window() {
        let guard = LocalNonces::default();
        let skew = AUTH_SKEW.as_secs();
        guard
            .accept(check(RING, 7, 1_000, 1_000))
            .expect("first use");

        for now in [1_000, 1_000 + skew] {
            assert!(matches!(
                guard.accept(check(RING, 7, 1_000, now)),
                Err(Unauthorized::Replay)
            ));
        }
        assert!(matches!(
            guard.accept(check(RING, 7, 1_000, 1_000 + skew + 1)),
            Err(Unauthorized::StaleTimestamp)
        ));
    }

    /// A replay granted on the window's edge is accepted only after another request evicted its nonce.
    #[test]
    fn a_nonce_evicted_while_its_replay_awaited_is_refused_at_acceptance() {
        let guard = LocalNonces::default();
        let skew = AUTH_SKEW.as_secs();
        guard
            .accept(check(RING, 7, 1_000, 1_000))
            .expect("first use");
        guard
            .accept(check(RING, 8, 1_000 + skew + 1, 1_000 + skew + 1))
            .expect("evicting request");
        assert!(matches!(
            guard.accept(check(RING, 7, 1_000, 1_000 + skew + 1)),
            Err(Unauthorized::StaleTimestamp)
        ));
    }

    /// Nonces are counted per ring, so one reader cannot burn another ring's
    /// budget.
    #[test]
    fn the_nonce_set_is_scoped_to_one_ring() {
        let guard = LocalNonces::default();
        let other = Address::new_from_array([2; 32]);
        guard
            .accept(check(RING, 7, 1_000, 1_000))
            .expect("first ring");
        guard
            .accept(check(other, 7, 1_000, 1_000))
            .expect("second ring");
    }

    /// The entry cap bounds live nonces, not the ring, so eviction reopens a
    /// full set instead of locking the ring out forever.
    #[test]
    fn a_full_nonce_set_reopens_once_its_entries_expire() {
        let guard = LocalNonces::default();
        let now = 1_000;
        {
            let mut rings = guard.0.lock().expect("nonce set");
            let accepted = rings.entry(RING).or_default();
            for index in 0..MAX_REPLAY_ENTRIES {
                let mut nonce = [0u8; 32];
                nonce[..8].copy_from_slice(&(index as u64).to_le_bytes());
                accepted.insert(nonce, now);
            }
        }

        assert!(matches!(
            guard.accept(check(RING, 0xff, now, now)),
            Err(Unauthorized::Replay)
        ));
        let later = now + AUTH_SKEW.as_secs() + 1;
        guard
            .accept(check(RING, 0xff, later, later))
            .expect("expired entries evicted");
    }

    #[test]
    fn ring_cap_evicts_expired_rings_before_refusing_new_rings() {
        let guard = LocalNonces::default();
        let now = 1_000;
        let ring = |index: usize| {
            let mut bytes = [0u8; 32];
            bytes[..8].copy_from_slice(&(index as u64).to_le_bytes());
            Address::new_from_array(bytes)
        };
        for index in 0..MAX_REPLAY_RINGS {
            guard
                .accept(check(ring(index), 1, now, now))
                .expect("within ring cap");
        }
        assert_eq!(guard.ring_count(), MAX_REPLAY_RINGS);
        assert_eq!(
            guard.accept(check(ring(MAX_REPLAY_RINGS), 1, now, now)),
            Err(Unauthorized::Replay)
        );
        assert_eq!(guard.ring_count(), MAX_REPLAY_RINGS);

        let later = now + AUTH_SKEW.as_secs() + 1;
        guard
            .accept(check(ring(MAX_REPLAY_RINGS), 1, later, later))
            .expect("expired rings evicted");
        assert_eq!(guard.ring_count(), 1);
    }
}
