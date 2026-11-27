use jsonrpsee::types::{error::ErrorCode, ErrorObjectOwned};
use spr_transaction::TransactionError;
use thiserror::Error;

use ring_audit::tracking::TrackingRefusal;
pub use ring_audit::RingAuditError;
pub use ring_cosigner::cosign::CoSignError;
pub use ring_cosigner::RingCosignerError;
pub use ring_service_core::ServiceError;
use ring_service_core::{service_faults, upstream::ScopeUnreadable};
use ring_writer::{
    enrollment_applier::EnrollmentRefusal, record_update::RecordUpdateRefusal, WriterError,
};

use crate::{authority_transfer::AuthorityTransferError, relay::RelayError};

#[derive(Debug, Error)]
pub enum RingRelayError {
    #[error(transparent)]
    Service(#[from] ServiceError),
    #[error(transparent)]
    Writer(WriterError),
    #[error(transparent)]
    Audit(RingAuditError),
    #[error(transparent)]
    Cosigner(RingCosignerError),
    #[error("relay prover is busy")]
    RelayProverBusy,
    #[error(transparent)]
    Relay(#[from] RelayError),
    #[error(transparent)]
    AuthorityTransfer(#[from] AuthorityTransferError),
    #[error("merge settings need a service store")]
    MergeSettingsUnavailable,
    #[error("stored merge settings are signed at the same time or later")]
    StaleMergeSettings,
}

const MERGE_SETTINGS_UNAVAILABLE: i32 = -32170;
const STALE_MERGE_SETTINGS: i32 = -32171;

service_faults!(RingRelayError);

#[cfg(any(test, feature = "test-fixtures"))]
impl RingRelayError {
    pub fn codes() -> Vec<ring_service_core::error::RefusalCode> {
        use ring_service_core::error::RefusalCode;
        let own = [
            RefusalCode {
                code: MERGE_SETTINGS_UNAVAILABLE,
                name: "RingRelayError::MergeSettingsUnavailable",
            },
            RefusalCode {
                code: STALE_MERGE_SETTINGS,
                name: "RingRelayError::StaleMergeSettings",
            },
        ];
        RelayError::codes()
            .into_iter()
            .chain(AuthorityTransferError::codes())
            .chain(own)
            .collect()
    }
}

/// A core fault a writer call raised stays `Service`.
impl From<WriterError> for RingRelayError {
    fn from(error: WriterError) -> Self {
        match error {
            WriterError::Service(fault) => Self::Service(fault),
            area => Self::Writer(area),
        }
    }
}

/// A core fault an audit call raised stays `Service`.
impl From<RingAuditError> for RingRelayError {
    fn from(error: RingAuditError) -> Self {
        match error {
            RingAuditError::Service(fault) => Self::Service(fault),
            area => Self::Audit(area),
        }
    }
}

/// A core or audit fault a co-sign call raised stays `Service` or `Audit`.
impl From<RingCosignerError> for RingRelayError {
    fn from(error: RingCosignerError) -> Self {
        match error {
            RingCosignerError::Service(fault) => Self::Service(fault),
            RingCosignerError::Audit(fault) => Self::Audit(fault),
            area => Self::Cosigner(area),
        }
    }
}

impl From<RecordUpdateRefusal> for RingRelayError {
    fn from(refusal: RecordUpdateRefusal) -> Self {
        Self::Writer(refusal.into())
    }
}

impl From<EnrollmentRefusal> for RingRelayError {
    fn from(refusal: EnrollmentRefusal) -> Self {
        Self::Writer(refusal.into())
    }
}

impl From<TrackingRefusal> for RingRelayError {
    fn from(refusal: TrackingRefusal) -> Self {
        Self::Audit(refusal.into())
    }
}

impl From<TransactionError> for RingRelayError {
    fn from(error: TransactionError) -> Self {
        Self::Audit(error.into())
    }
}

impl From<CoSignError> for RingRelayError {
    fn from(error: CoSignError) -> Self {
        Self::Cosigner(error.into())
    }
}

impl From<ScopeUnreadable> for RingRelayError {
    fn from(unreadable: ScopeUnreadable) -> Self {
        Self::Cosigner(unreadable.into())
    }
}

/// A client sees its own request errors, and only a generic message for a server fault.
impl From<&RingRelayError> for ErrorObjectOwned {
    fn from(failure: &RingRelayError) -> Self {
        use RingRelayError::*;
        match failure {
            Service(fault) => fault.into(),
            Writer(fault) => fault.into(),
            Audit(fault) => fault.into(),
            Cosigner(fault) => fault.into(),
            Relay(relay) => relay.into(),
            AuthorityTransfer(refusal) => (*refusal).into(),
            MergeSettingsUnavailable => {
                ErrorObjectOwned::owned(MERGE_SETTINGS_UNAVAILABLE, failure.to_string(), None::<()>)
            }
            StaleMergeSettings => {
                ErrorObjectOwned::owned(STALE_MERGE_SETTINGS, failure.to_string(), None::<()>)
            }
            RelayProverBusy => ErrorObjectOwned::owned(
                ErrorCode::ServerIsBusy.code(),
                failure.to_string(),
                None::<()>,
            ),
        }
    }
}

impl From<RingRelayError> for ErrorObjectOwned {
    fn from(failure: RingRelayError) -> Self {
        (&failure).into()
    }
}
