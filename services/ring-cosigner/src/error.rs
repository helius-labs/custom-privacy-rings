use jsonrpsee::types::{error::ErrorCode, ErrorObjectOwned};
use log::error;
use thiserror::Error;

pub use ring_audit::RingAuditError;
pub use ring_service_core::ServiceError;
use ring_service_core::{service_faults, upstream::ScopeUnreadable};

use crate::cosign::CoSignError;

#[derive(Debug, Error)]
pub enum RingCosignerError {
    #[error(transparent)]
    Service(#[from] ServiceError),
    #[error(transparent)]
    Audit(RingAuditError),
    #[error("co-signing needs a service store")]
    CoSignerStoreMissing,
    #[error(transparent)]
    CoSign(#[from] CoSignError),
}

service_faults!(RingCosignerError);

#[cfg(any(test, feature = "test-fixtures"))]
impl RingCosignerError {
    pub fn codes() -> Vec<i32> {
        CoSignError::ALL
            .into_iter()
            .map(CoSignError::code)
            .collect()
    }
}

/// A core fault an audit call raised stays `Service`.
impl From<RingAuditError> for RingCosignerError {
    fn from(error: RingAuditError) -> Self {
        match error {
            RingAuditError::Service(fault) => Self::Service(fault),
            area => Self::Audit(area),
        }
    }
}

impl From<ScopeUnreadable> for RingCosignerError {
    fn from(unreadable: ScopeUnreadable) -> Self {
        Self::CoSign(unreadable.into())
    }
}

/// A client sees its own request errors, and only a generic message for a server fault.
impl From<&RingCosignerError> for ErrorObjectOwned {
    fn from(failure: &RingCosignerError) -> Self {
        use RingCosignerError::*;
        match failure {
            Service(fault) => fault.into(),
            Audit(fault) => fault.into(),
            CoSign(inner) => (*inner).into(),
            CoSignerStoreMissing => {
                error!("{failure}");
                ErrorObjectOwned::owned(
                    ErrorCode::InternalError.code(),
                    "audit service is unavailable",
                    None::<()>,
                )
            }
        }
    }
}

impl From<RingCosignerError> for ErrorObjectOwned {
    fn from(failure: RingCosignerError) -> Self {
        (&failure).into()
    }
}
