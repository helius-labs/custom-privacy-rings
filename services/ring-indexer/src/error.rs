use jsonrpsee::types::{error::ErrorCode, ErrorObjectOwned};
use thiserror::Error;
use zolana_ring_indexer::{redact::redacted, rpc::RingRpcError};

#[derive(Error, Clone, Debug, PartialEq, Eq)]
pub enum RingApiError {
    #[error("member registry is out of sync ({0})")]
    OutOfSync(String),
    #[error("member registry root changed")]
    RootChanged,
    #[error("member registry page has an index gap or inconsistent leaf")]
    PageGap,
    #[error("invalid request ({0})")]
    InvalidRequest(String),
    #[error("internal error ({0})")]
    Internal(String),
}

impl From<sea_orm::DbErr> for RingApiError {
    fn from(error: sea_orm::DbErr) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<RingApiError> for ErrorObjectOwned {
    fn from(error: RingApiError) -> Self {
        let refusal = match &error {
            RingApiError::OutOfSync(_) => RingRpcError::RegistryOutOfSync,
            RingApiError::RootChanged => RingRpcError::RegistryRootChanged,
            RingApiError::PageGap => RingRpcError::RegistryPageGap,
            RingApiError::InvalidRequest(_) => {
                return Self::owned(
                    ErrorCode::InvalidRequest.code(),
                    error.to_string(),
                    None::<()>,
                )
            }
            RingApiError::Internal(reason) => {
                log::error!("ring lookup failed ({})", redacted(reason));
                return Self::owned(
                    ErrorCode::InternalError.code(),
                    "internal server error",
                    None::<()>,
                );
            }
        };
        Self::owned(refusal.code(), error.to_string(), None::<()>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_internal_fault_reaches_the_client_without_its_reason() {
        let reason = "connect postgres://admin:secret@db.internal:5432/ring?api-key=f00d";
        let internal = ErrorObjectOwned::from(RingApiError::Internal(reason.to_owned()));
        assert_eq!(internal.code(), ErrorCode::InternalError.code());
        assert_eq!(internal.message(), "internal server error");
    }
}
