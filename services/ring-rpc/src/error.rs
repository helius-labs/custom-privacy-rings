use jsonrpsee::types::ErrorObjectOwned;
use thiserror::Error;

pub use ring_audit::error::{GrantRefusal, RingAuditError};
pub use ring_cosigner::RingCosignerError;
pub use ring_relay::RingRelayError;
pub use ring_service_core::ServiceError;
use ring_service_core::{error::CarriedFault, service_faults};
use ring_writer::{
    enrollment_applier::EnrollmentRefusal, record_update::RecordUpdateRefusal, WriterError,
};

/// One variant per area, a fault keeps the variant of the area that defines it.
#[derive(Debug, Error)]
pub enum RingRpcError {
    #[error(transparent)]
    Service(#[from] ServiceError),
    #[error(transparent)]
    Writer(WriterError),
    #[error(transparent)]
    Audit(RingAuditError),
    #[error(transparent)]
    Cosigner(RingCosignerError),
    #[error(transparent)]
    Relay(RingRelayError),
}

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

impl From<RingAuditError> for RingRpcError {
    fn from(error: RingAuditError) -> Self {
        match error {
            RingAuditError::Service(fault) => Self::Service(fault),
            area => Self::Audit(area),
        }
    }
}

impl From<RingCosignerError> for RingRpcError {
    fn from(error: RingCosignerError) -> Self {
        match error {
            RingCosignerError::Service(fault) => Self::Service(fault),
            RingCosignerError::Audit(fault) => Self::Audit(fault),
            area => Self::Cosigner(area),
        }
    }
}

impl From<RingRelayError> for RingRpcError {
    fn from(error: RingRelayError) -> Self {
        match error {
            RingRelayError::Service(fault) => Self::Service(fault),
            RingRelayError::Writer(fault) => Self::Writer(fault),
            RingRelayError::Audit(fault) => Self::Audit(fault),
            RingRelayError::Cosigner(fault) => Self::Cosigner(fault),
            area => Self::Relay(area),
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

/// A client sees its own request errors, and only a generic message for a server fault.
impl From<&RingRpcError> for ErrorObjectOwned {
    fn from(failure: &RingRpcError) -> Self {
        use RingRpcError::*;
        match failure {
            Service(fault) => fault.into(),
            Writer(fault) => fault.into(),
            Audit(fault) => fault.into(),
            Cosigner(fault) => fault.into(),
            Relay(fault) => fault.into(),
        }
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
    use std::collections::HashSet;

    use super::*;
    use crate::client_rate::RateLimit;

    #[test]
    fn every_rpc_error_code_is_unique_across_the_crates() {
        let codes = [
            ServiceError::codes(),
            WriterError::codes(),
            RingAuditError::codes(),
            RingCosignerError::codes(),
            RingRelayError::codes(),
            RateLimit::codes(),
        ]
        .concat();
        let mut seen = HashSet::new();
        let repeated: Vec<i32> = codes
            .into_iter()
            .filter(|code| !seen.insert(*code))
            .collect();
        assert!(repeated.is_empty(), "{repeated:?}");
    }
}
