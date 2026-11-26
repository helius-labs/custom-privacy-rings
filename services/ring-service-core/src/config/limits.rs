use std::{
    collections::HashSet,
    sync::Mutex,
    time::{Duration, Instant},
};

use solana_address::Address;
use zolana_ring_client::ReaderKey;

use crate::error::ServiceError;

pub const MAX_CONCURRENT_READS: usize = 32;
pub const MAX_CONCURRENT_AUTHENTICATIONS: usize = 32;
pub const MAX_REQUEST_UNITS_PER_SECOND: usize = 256;

/// Each lane has its own rate window, and all but `Public` a concurrency cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    Public,
    Authentication,
    /// Held for a whole transfer or merge page.
    Read,
}

pub struct RequestRate(Mutex<RequestWindow>);

struct RequestWindow {
    started_at: Instant,
    accepted: usize,
}

impl Default for RequestRate {
    fn default() -> Self {
        Self(Mutex::new(RequestWindow {
            started_at: Instant::now(),
            accepted: 0,
        }))
    }
}

impl RequestRate {
    pub fn accept(&self, now: Instant) -> Result<(), ServiceError> {
        let mut window = self.0.lock().map_err(|_| ServiceError::StateUnavailable)?;
        if now.duration_since(window.started_at) >= Duration::from_secs(1) {
            *window = RequestWindow {
                started_at: now,
                accepted: 0,
            };
        }
        if window.accepted >= MAX_REQUEST_UNITS_PER_SECOND {
            return Err(ServiceError::Busy);
        }
        window.accepted += 1;
        Ok(())
    }
}

pub struct ReaderPermit<'a> {
    active: &'a Mutex<HashSet<(Address, ReaderKey)>>,
    key: (Address, ReaderKey),
}

impl<'a> ReaderPermit<'a> {
    pub fn acquire(
        active: &'a Mutex<HashSet<(Address, ReaderKey)>>,
        key: (Address, ReaderKey),
    ) -> Result<Self, ServiceError> {
        let mut readers = active.lock().map_err(|_| ServiceError::StateUnavailable)?;
        if !readers.insert(key) {
            return Err(ServiceError::Busy);
        }
        Ok(Self { active, key })
    }
}

impl Drop for ReaderPermit<'_> {
    fn drop(&mut self) {
        if let Ok(mut readers) = self.active.lock() {
            readers.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use solana_keypair::Keypair;
    use solana_signer::Signer;

    use super::*;

    fn reader(byte: u8) -> ReaderKey {
        ReaderKey::ed25519(Keypair::new_from_array([byte; 32]).pubkey()).expect("reader key")
    }

    /// One read at a time per reader and ring, released as soon as the page finishes.
    #[test]
    fn a_reader_holds_one_permit_per_ring_at_a_time() {
        let active = Mutex::new(HashSet::new());
        let ring = Address::new_from_array([1; 32]);
        let other = Address::new_from_array([2; 32]);
        let permit = ReaderPermit::acquire(&active, (ring, reader(7))).expect("first read");

        assert!(matches!(
            ReaderPermit::acquire(&active, (ring, reader(7))),
            Err(ServiceError::Busy)
        ));
        let concurrent = [
            ReaderPermit::acquire(&active, (other, reader(7))).expect("another ring"),
            ReaderPermit::acquire(&active, (ring, reader(8))).expect("another reader"),
        ];

        drop(permit);
        drop(concurrent);
        ReaderPermit::acquire(&active, (ring, reader(7))).expect("released permit");
    }

    #[test]
    fn request_rate_recovers_after_its_window() {
        let rate = RequestRate::default();
        let now = Instant::now();
        for _ in 0..MAX_REQUEST_UNITS_PER_SECOND {
            rate.accept(now).expect("accepted request");
        }
        assert!(matches!(rate.accept(now), Err(ServiceError::Busy)));
        rate.accept(now + Duration::from_secs(1))
            .expect("new window");
    }
}
