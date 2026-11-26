use jsonrpsee::types::{error::ErrorCode, ErrorObjectOwned};
use log::error;
use ring_service_core::{error::CarriedFault, redact::redacted, service_faults, ServiceError};
use thiserror::Error;
use zolana_ring_client::RingError;

use crate::{enrollment_applier::EnrollmentRefusal, record_update::RecordUpdateRefusal};

#[derive(Debug, Error)]
pub enum WriterError {
    #[error(transparent)]
    Service(#[from] ServiceError),
    #[error(transparent)]
    RecordUpdate(#[from] RecordUpdateRefusal),
    #[error("indexer records lag the storage registry")]
    StaleRecords,
    #[error("storage update failed because {0}")]
    StorageUpdate(Box<RingError>),
    #[error(transparent)]
    Enrollment(#[from] EnrollmentRefusal),
    #[error("{0}")]
    PrivateWrites(Box<dyn CarriedFault>),
}

service_faults!(WriterError);

impl From<Box<dyn CarriedFault>> for WriterError {
    fn from(fault: Box<dyn CarriedFault>) -> Self {
        Self::PrivateWrites(fault)
    }
}

/// A client sees its own request errors, and only a generic message for a server fault.
impl From<&WriterError> for ErrorObjectOwned {
    fn from(failure: &WriterError) -> Self {
        use WriterError::*;
        match failure {
            Service(fault) => fault.into(),
            PrivateWrites(fault) => fault.rpc_error(),
            RecordUpdate(refusal) => {
                ErrorObjectOwned::owned(refusal.code(), refusal.to_string(), None::<()>)
            }
            Enrollment(refusal) => {
                ErrorObjectOwned::owned(*refusal as i32, refusal.to_string(), None::<()>)
            }
            StaleRecords | StorageUpdate(_) => {
                error!("storage update failed {}", redacted(failure));
                ErrorObjectOwned::owned(
                    ErrorCode::InternalError.code(),
                    "record writer is unavailable",
                    None::<()>,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_update_refusal_codes_are_stable() {
        use RecordUpdateRefusal::*;
        for refusal in [
            Disabled,
            EntryCount,
            InvalidEntry,
            Signature,
            Expired,
            UnknownRing,
            RegistryNotServed,
            Unauthorized,
            UnknownUpdate,
            WrongCluster,
            ReaderGrantsInDatabase,
            InvalidAwaitedLists,
            NotPolicyAuthority,
            ListApprovalBatch,
            UnknownListApproval,
            ListApprovalWindow,
            RecordsNotCreated,
            InvalidLimits,
            LimitsWithoutRecord,
            RingSaltNotCommitted,
            UnregisteredOwnerUnmatched,
        ] {
            let code = match refusal {
                Disabled => -32140,
                EntryCount => -32141,
                InvalidEntry => -32142,
                Signature => -32143,
                Expired => -32144,
                UnknownRing => -32145,
                RegistryNotServed => -32146,
                Unauthorized => -32147,
                UnknownUpdate => -32149,
                WrongCluster => -32190,
                ReaderGrantsInDatabase => -32230,
                InvalidAwaitedLists => -32240,
                NotPolicyAuthority => -32241,
                ListApprovalBatch => -32242,
                UnknownListApproval => -32244,
                ListApprovalWindow => -32245,
                RecordsNotCreated => -32260,
                InvalidLimits => -32276,
                LimitsWithoutRecord => -32277,
                RingSaltNotCommitted => -32279,
                UnregisteredOwnerUnmatched => -32280,
            };
            assert_eq!(refusal.code(), code);
        }
    }

    #[test]
    fn enrollment_refusal_codes_are_stable() {
        use EnrollmentRefusal::*;
        for refusal in [
            NotServed,
            RingMissing,
            InvalidWrite,
            RegisterKeyProof,
            Full,
            Budget,
            RingsFull,
            InFlight,
        ] {
            let code = match refusal {
                NotServed => -32180,
                RingMissing => -32181,
                InvalidWrite => -32182,
                RegisterKeyProof => -32183,
                Full => -32184,
                Budget => -32185,
                RingsFull => -32186,
                InFlight => -32187,
            };
            assert_eq!(
                ErrorObjectOwned::from(&WriterError::Enrollment(refusal)).code(),
                code
            );
        }
    }
}
