use jsonrpsee::types::{error::ErrorCode, ErrorObjectOwned};
use log::error;
use spr_transaction::TransactionError;
use thiserror::Error;
use zolana_ring_client::rpc::{
    AUDITED_UTXOS_LIMIT, WALLET_SNAPSHOT_CAPACITY, WALLET_SNAPSHOT_EXPIRED,
};
use zolana_ring_policy::MemberError;

pub use ring_service_core::ServiceError;
use ring_service_core::{
    error::CarriedFault, redact::redacted, service_faults, upstream::ScopeUnreadable,
};
use ring_writer::{
    enrollment_applier::EnrollmentRefusal, record_update::RecordUpdateRefusal, WriterError,
};

use crate::{
    authority_transfer::AuthorityTransferError, cosign::CoSignError, relay::RelayError,
    tracking::TrackingRefusal,
};

#[derive(Debug, Error)]
pub enum RingRpcError {
    #[error(transparent)]
    Service(#[from] ServiceError),
    #[error(transparent)]
    Writer(WriterError),
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
    #[error("relay prover is busy")]
    RelayProverBusy,
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
    #[error("co-signing needs a service store")]
    CoSignerStoreMissing,
    #[error(transparent)]
    Relay(#[from] RelayError),
    #[error(transparent)]
    AuthorityTransfer(#[from] AuthorityTransferError),
    #[error(transparent)]
    CoSign(#[from] CoSignError),
    #[error("merge settings need a service store")]
    MergeSettingsUnavailable,
    #[error("stored merge settings are signed at the same time or later")]
    StaleMergeSettings,
    #[error(transparent)]
    Tracking(#[from] TrackingRefusal),
    #[error("merge build failed because {0}")]
    MergeBuild(Box<zolana_ring_client::RingError>),
}

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

impl GrantRefusal {
    pub const fn code(self) -> i32 {
        self as i32
    }
}

const MERGE_SETTINGS_UNAVAILABLE: i32 = -32170;
const STALE_MERGE_SETTINGS: i32 = -32171;

service_faults!(RingRpcError);

/// A core fault a writer call raised stays `Service`.
impl From<WriterError> for RingRpcError {
    fn from(error: WriterError) -> Self {
        match error {
            WriterError::Service(fault) => Self::Service(fault),
            area => Self::Writer(area),
        }
    }
}

impl From<RecordUpdateRefusal> for RingRpcError {
    fn from(refusal: RecordUpdateRefusal) -> Self {
        Self::Writer(refusal.into())
    }
}

impl From<EnrollmentRefusal> for RingRpcError {
    fn from(refusal: EnrollmentRefusal) -> Self {
        Self::Writer(refusal.into())
    }
}

impl From<ScopeUnreadable> for RingRpcError {
    fn from(unreadable: ScopeUnreadable) -> Self {
        Self::CoSign(unreadable.into())
    }
}

/// A client sees its own request errors, and only a generic message for a server fault.
impl From<&RingRpcError> for ErrorObjectOwned {
    fn from(failure: &RingRpcError) -> Self {
        use RingRpcError::*;
        let (code, message) = match failure {
            Service(fault) => return fault.into(),
            Writer(fault) => return fault.into(),
            Relay(relay) => return relay.into(),
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
            MergeSettingsUnavailable => {
                let message = failure.to_string();
                return ErrorObjectOwned::owned(MERGE_SETTINGS_UNAVAILABLE, message, None::<()>);
            }
            StaleMergeSettings => {
                let message = failure.to_string();
                return ErrorObjectOwned::owned(STALE_MERGE_SETTINGS, message, None::<()>);
            }
            Grant(refusal) => {
                return ErrorObjectOwned::owned(refusal.code(), refusal.to_string(), None::<()>);
            }
            RelayProverBusy => {
                let message = failure.to_string();
                return ErrorObjectOwned::owned(
                    ErrorCode::ServerIsBusy.code(),
                    message,
                    None::<()>,
                );
            }
            Assets(inner) => {
                error!("asset registry is unusable {inner}");
                (ErrorCode::InternalError, "upstream request failed")
            }
            Member(inner) => {
                error!("grant identity failed {inner}");
                (ErrorCode::InternalError, "audit service is unavailable")
            }
            InvalidGrantRow | GrantStoreMissing | GrantRowsKeyMissing | CoSignerStoreMissing => {
                error!("{failure}");
                (ErrorCode::InternalError, "audit service is unavailable")
            }
            CoSign(inner) => return (*inner).into(),
            AuthorityTransfer(refusal) => return (*refusal).into(),
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
            MergeBuild(_) => {
                error!("managed merge failed {}", redacted(failure));
                (ErrorCode::InternalError, "managed merge failed")
            }
        };
        ErrorObjectOwned::owned(code.code(), message, None::<()>)
    }
}

impl From<RingRpcError> for ErrorObjectOwned {
    fn from(failure: RingRpcError) -> Self {
        (&failure).into()
    }
}

impl CarriedFault for RingRpcError {
    fn rpc_error(&self) -> ErrorObjectOwned {
        self.into()
    }
}

impl From<RingRpcError> for Box<dyn CarriedFault> {
    fn from(failure: RingRpcError) -> Self {
        Box::new(failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_refusal_codes_are_stable() {
        use GrantRefusal::*;
        for refusal in [RecordsUnavailable, GrantsInTree, WalletScope] {
            let code = match refusal {
                RecordsUnavailable => -32130,
                GrantsInTree => -32131,
                WalletScope => -32270,
            };
            assert_eq!(
                ErrorObjectOwned::from(RingRpcError::Grant(refusal)).code(),
                code
            );
        }
    }
}
