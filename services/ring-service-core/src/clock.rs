use zolana_ring_client::rpc::unix_now;

use crate::error::ServiceError;

/// Unix seconds for the auth skew rule and the nonce eviction window.
#[derive(Debug, Clone, Copy)]
pub enum Clock {
    System,
    Fixed(u64),
}

impl Clock {
    pub fn now(self) -> Result<u64, ServiceError> {
        match self {
            Self::System => unix_now().map_err(|_| ServiceError::StateUnavailable),
            Self::Fixed(now) => Ok(now),
        }
    }
}
