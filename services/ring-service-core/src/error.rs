use jsonrpsee::types::{error::ErrorCode, ErrorObjectOwned};
use log::error;
use spr_client::ClientError;
use thiserror::Error;
use zolana_ring_client::{
    rpc::KeyFileError, AuditError, RingIndexerError, RingRpcError as RingRefusal,
};

use crate::{authorize::Unauthorized, redact::redacted};

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("operator signing failed")]
    Custody(#[from] zolana_operator_custody::CustodyError),
    #[error("request is unauthorized because {0}")]
    Unauthorized(#[from] Unauthorized),
    #[error("key derivation failed because {0}")]
    Derivation(#[from] spr_keypair::KeypairError),
    #[error("auditor key is unusable because {0}")]
    AuditorKey(#[from] KeyFileError),
    #[error(transparent)]
    Upstream(#[from] ClientError),
    #[error(transparent)]
    Refused(RingRefusal),
    #[error(transparent)]
    Origin(#[from] AuditError),
    #[error("indexer returned data outside the audit bounds")]
    InvalidIndexerResponse,
    #[error("audit service state is unavailable")]
    StateUnavailable,
    #[error("audit service is busy")]
    Busy,
    #[error("service store failed because {0}")]
    Store(#[from] sea_orm::DbErr),
    #[error("the cluster refused the transaction because {reason}")]
    SendRefused { reason: String },
}

const SEND_REFUSED: i32 = -32113;

#[cfg(any(test, feature = "test-fixtures"))]
impl ServiceError {
    pub fn codes() -> Vec<i32> {
        vec![SEND_REFUSED]
    }
}

/// A fault of an upper area, a lower area carries it to the RPC edge unread.
pub trait CarriedFault: std::error::Error + Send + Sync + 'static {
    fn rpc_error(&self) -> ErrorObjectOwned;
}

#[doc(hidden)]
pub mod fault_sources {
    pub use sea_orm::DbErr;
    pub use spr_client::ClientError;
    pub use spr_keypair::KeypairError;
    pub use zolana_operator_custody::CustodyError;
    pub use zolana_ring_client::{rpc::KeyFileError, AuditError, RingIndexerError};

    pub use crate::authorize::Unauthorized;
}

/// `?` lifts each core fault source into an area error through its `Service` variant.
#[macro_export]
macro_rules! service_faults {
    ($area:ty) => {
        $crate::service_faults!(
            $area: CustodyError,
            Unauthorized,
            KeypairError,
            KeyFileError,
            ClientError,
            AuditError,
            DbErr,
            RingIndexerError
        );
    };
    ($area:ty: $($source:ident),+) => {$(
        impl From<$crate::error::fault_sources::$source> for $area {
            fn from(error: $crate::error::fault_sources::$source) -> Self {
                Self::Service(error.into())
            }
        }
    )+};
}

impl From<RingIndexerError> for ServiceError {
    fn from(error: RingIndexerError) -> Self {
        match error {
            RingIndexerError::Refused(refused) => Self::Refused(refused),
            RingIndexerError::Client(error) => Self::Upstream(error),
        }
    }
}

/// A client sees its own request errors, and only a generic message for a server fault.
impl From<&ServiceError> for ErrorObjectOwned {
    fn from(failure: &ServiceError) -> Self {
        use ServiceError::*;
        let (code, message) = match failure {
            Unauthorized(_) => {
                let message = failure.to_string();
                return ErrorObjectOwned::owned(
                    ErrorCode::InvalidRequest.code(),
                    message,
                    None::<()>,
                );
            }
            SendRefused { .. } => {
                return ErrorObjectOwned::owned(SEND_REFUSED, failure.to_string(), None::<()>);
            }
            Busy => {
                let message = failure.to_string();
                return ErrorObjectOwned::owned(
                    ErrorCode::ServerIsBusy.code(),
                    message,
                    None::<()>,
                );
            }
            Upstream(inner) => {
                error!("upstream request failed {}", redacted(inner));
                (ErrorCode::InternalError, "upstream request failed")
            }
            Refused(inner) => {
                error!("indexer refused the registry read {inner}");
                (ErrorCode::InternalError, "upstream request failed")
            }
            Origin(AuditError::MergeAuditWorkBudget) => (
                ErrorCode::InternalError,
                "audit scan exceeded its per-request work budget",
            ),
            Origin(inner) => {
                error!("transaction origin lookup failed {}", redacted(inner));
                (ErrorCode::InternalError, "upstream request failed")
            }
            InvalidIndexerResponse => {
                error!("indexer returned data outside the audit bounds");
                (ErrorCode::InternalError, "indexer response is invalid")
            }
            Custody(inner) => {
                error!("operator signing failed {}", redacted(inner));
                (ErrorCode::InternalError, "operator signing failed")
            }
            Derivation(_) | AuditorKey(_) => {
                error!("auditor key derivation failed");
                (ErrorCode::InternalError, "auditor key is unavailable")
            }
            StateUnavailable => {
                error!("audit service state is unavailable");
                (ErrorCode::InternalError, "audit service is unavailable")
            }
            Store(inner) => {
                error!("service store failed {}", redacted(inner));
                (ErrorCode::InternalError, "audit service is unavailable")
            }
        };
        ErrorObjectOwned::owned(code.code(), message, None::<()>)
    }
}

impl From<ServiceError> for ErrorObjectOwned {
    fn from(failure: ServiceError) -> Self {
        (&failure).into()
    }
}

#[cfg(test)]
mod tests {
    use solana_address::Address;
    use zolana_operator_custody::CustodyError;

    use super::*;

    #[test]
    fn a_custody_failure_reaches_the_client_as_a_generic_internal_error() {
        const DETAIL: &str =
            "AccessDeniedException for arn:aws:kms:us-east-1:123456789012:key/1a2b3c4d";
        let failures = [
            CustodyError::Service(std::io::Error::other(DETAIL).into()),
            CustodyError::AddressMismatch,
            CustodyError::UnrequiredSigner(Address::new_from_array([7; 32])),
        ];
        for failure in failures {
            let detail = failure.to_string();
            let object = ErrorObjectOwned::from(ServiceError::Custody(failure));
            assert_eq!(object.code(), ErrorCode::InternalError.code());
            assert_eq!(object.message(), "operator signing failed");
            assert!(object.data().is_none());
            assert!(!object.message().contains(&detail), "{detail}");
        }
    }

    #[test]
    fn a_logged_upstream_error_drops_the_api_key() {
        let upstream = ClientError::Rpc(
            "request to https://rpc.example/?api-key=f00dcafe&x=1 failed".to_owned(),
        );
        let logged = redacted(&ServiceError::Upstream(upstream));
        assert!(!logged.contains("f00dcafe"), "{logged}");
        assert!(logged.contains("api-key="), "{logged}");
    }

    #[test]
    fn exhausted_merge_scan_has_an_explicit_rpc_error() {
        let response =
            ErrorObjectOwned::from(ServiceError::Origin(AuditError::MergeAuditWorkBudget));
        assert_eq!(response.code(), ErrorCode::InternalError.code());
        assert_eq!(
            response.message(),
            "audit scan exceeded its per-request work budget"
        );
    }
}
