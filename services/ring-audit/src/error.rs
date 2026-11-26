use jsonrpsee::types::{error::ErrorCode, ErrorObjectOwned};
use log::error;
use spr_transaction::TransactionError;
use thiserror::Error;
use zolana_ring_client::rpc::{
    AUDITED_UTXOS_LIMIT, WALLET_SNAPSHOT_CAPACITY, WALLET_SNAPSHOT_EXPIRED,
};
use zolana_ring_policy::MemberError;

pub use ring_service_core::ServiceError;
use ring_service_core::{error::CarriedFault, service_faults};

use crate::tracking::TrackingRefusal;

#[derive(Debug, Error)]
pub enum RingAuditError {
    #[error(transparent)]
    Service(#[from] ServiceError),
    #[error("ring_program_id is required when keys are derived per ring")]
    RingRequired,
    #[error("ring is not served by the local auditor key")]
    RingNotServed,
    #[error("audit page is invalid")]
    InvalidPage,
    #[error("a UTXO read names 1 to {AUDITED_UTXOS_LIMIT} UTXO hashes")]
    InvalidUtxoRequest,
    #[error("the cursor is no position of the read")]
    InvalidCursor,
    #[error("wallet snapshot expired, restart without a cursor")]
    WalletSnapshotExpired,
    #[error("wallet snapshot capacity is exhausted")]
    WalletSnapshotCapacity,
    #[error("asset registry is unusable because {0}")]
    Assets(#[from] TransactionError),
    #[error(transparent)]
    Grant(#[from] GrantRefusal),
    #[error("grant identity failed because {0}")]
    Member(#[from] MemberError),
    #[error("stored grant row does not open under the grant rows key")]
    InvalidGrantRow,
    #[error("database reader grants need a service store")]
    GrantStoreMissing,
    #[error("database reader grants need a grant rows key")]
    GrantRowsKeyMissing,
    #[error(transparent)]
    Tracking(#[from] TrackingRefusal),
}

ring_service_core::listed_refusals! {
    #[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
    #[repr(i32)]
    pub enum GrantRefusal {
        #[error("ring has no initialized records registry")]
        RecordsUnavailable = -32130,
        #[error("service keeps reader grants in the storage tree")]
        GrantsInTree = -32131,
        #[error("read method and wallet scope disagree")]
        WalletScope = -32270,
    }
}

impl GrantRefusal {
    pub(crate) const fn code(self) -> i32 {
        self as i32
    }
}

service_faults!(RingAuditError);

#[cfg(any(test, feature = "test-fixtures"))]
impl RingAuditError {
    pub fn codes() -> Vec<i32> {
        GrantRefusal::ALL
            .into_iter()
            .map(GrantRefusal::code)
            .chain([WALLET_SNAPSHOT_EXPIRED, WALLET_SNAPSHOT_CAPACITY])
            .collect()
    }
}

/// A client sees its own request errors, and only a generic message for a server fault.
impl From<&RingAuditError> for ErrorObjectOwned {
    fn from(failure: &RingAuditError) -> Self {
        use RingAuditError::*;
        let (code, message) = match failure {
            Service(fault) => return fault.into(),
            RingRequired | RingNotServed | InvalidPage | InvalidUtxoRequest | InvalidCursor => {
                let message = failure.to_string();
                return ErrorObjectOwned::owned(
                    ErrorCode::InvalidRequest.code(),
                    message,
                    None::<()>,
                );
            }
            WalletSnapshotExpired => {
                return ErrorObjectOwned::owned(
                    WALLET_SNAPSHOT_EXPIRED,
                    failure.to_string(),
                    None::<()>,
                );
            }
            WalletSnapshotCapacity => {
                return ErrorObjectOwned::owned(
                    WALLET_SNAPSHOT_CAPACITY,
                    failure.to_string(),
                    None::<()>,
                );
            }
            Grant(refusal) => {
                return ErrorObjectOwned::owned(refusal.code(), refusal.to_string(), None::<()>);
            }
            Assets(inner) => {
                error!("asset registry is unusable {inner}");
                (ErrorCode::InternalError, "upstream request failed")
            }
            Member(inner) => {
                error!("grant identity failed {inner}");
                (ErrorCode::InternalError, "audit service is unavailable")
            }
            InvalidGrantRow | GrantStoreMissing | GrantRowsKeyMissing => {
                error!("{failure}");
                (ErrorCode::InternalError, "audit service is unavailable")
            }
            Tracking(TrackingRefusal::Incomplete) => {
                let message = failure.to_string();
                return ErrorObjectOwned::owned(
                    ErrorCode::ServerIsBusy.code(),
                    message,
                    None::<()>,
                );
            }
            Tracking(inner) => {
                error!("tracking discovery refused {inner}");
                (ErrorCode::InternalError, "tracking discovery failed")
            }
        };
        ErrorObjectOwned::owned(code.code(), message, None::<()>)
    }
}

impl From<RingAuditError> for ErrorObjectOwned {
    fn from(failure: RingAuditError) -> Self {
        (&failure).into()
    }
}

impl CarriedFault for RingAuditError {
    fn rpc_error(&self) -> ErrorObjectOwned {
        self.into()
    }
}

impl From<RingAuditError> for Box<dyn CarriedFault> {
    fn from(failure: RingAuditError) -> Self {
        Box::new(failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_refusal_codes_are_stable() {
        use GrantRefusal::*;
        for refusal in GrantRefusal::ALL {
            let code = match refusal {
                RecordsUnavailable => -32130,
                GrantsInTree => -32131,
                WalletScope => -32270,
            };
            assert_eq!(
                ErrorObjectOwned::from(RingAuditError::Grant(refusal)).code(),
                code
            );
        }
    }
}
