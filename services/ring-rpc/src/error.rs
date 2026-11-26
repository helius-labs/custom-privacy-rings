use std::{borrow::Cow, fmt::Display};

use jsonrpsee::types::{error::ErrorCode, ErrorObjectOwned};
use log::error;
use spr_client::ClientError;
use spr_transaction::TransactionError;
use thiserror::Error;
use zolana_ring_client::{
    rpc::{KeyFileError, AUDITED_UTXOS_LIMIT, WALLET_SNAPSHOT_CAPACITY, WALLET_SNAPSHOT_EXPIRED},
    AuditError, RingIndexerError, RingRpcError as RingRefusal,
};
use zolana_ring_policy::MemberError;

use crate::{
    authority_transfer::AuthorityTransferError, authorize::Unauthorized, cosign::CoSignError,
    enrollment_applier::EnrollmentRefusal, record_update::RecordUpdateRefusal, relay::RelayError,
    tracking::TrackingRefusal,
};

#[derive(Debug, Error)]
pub enum RingRpcError {
    #[error("operator signing failed")]
    Custody(#[from] zolana_operator_custody::CustodyError),
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
    #[error("asset registry is unusable because {0}")]
    Assets(#[from] TransactionError),
    #[error(transparent)]
    Origin(#[from] AuditError),
    #[error("indexer returned data outside the audit bounds")]
    InvalidIndexerResponse,
    #[error("audit service state is unavailable")]
    StateUnavailable,
    #[error("audit service is busy")]
    Busy,
    #[error("relay prover is busy")]
    RelayProverBusy,
    #[error("service store failed because {0}")]
    Store(#[from] sea_orm::DbErr),
    #[error(transparent)]
    RecordUpdate(#[from] RecordUpdateRefusal),
    #[error("indexer records lag the storage registry")]
    StaleRecords,
    #[error("storage update failed because {0}")]
    StorageUpdate(Box<custom_ring_sdk::RingError>),
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
    #[error(transparent)]
    Enrollment(#[from] EnrollmentRefusal),
    #[error("merge build failed because {0}")]
    MergeBuild(Box<custom_ring_sdk::RingError>),
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

impl From<RingIndexerError> for RingRpcError {
    fn from(error: RingIndexerError) -> Self {
        match error {
            RingIndexerError::Refused(refused) => Self::Refused(refused),
            RingIndexerError::Client(error) => Self::Upstream(error),
        }
    }
}

pub(crate) const API_KEY_PARAMETER: &str = "api-key=";
const SECRET_PARAMETERS: [&str; 4] = [API_KEY_PARAMETER, "token=", "key=", "password="];

/// Upstream errors quote request URLs, a log never carries their API key.
pub(crate) fn redacted(error: &impl Display) -> String {
    redact_secrets(&error.to_string()).into_owned()
}

fn redact_secrets(text: &str) -> Cow<'_, str> {
    match redact_url_credentials(text) {
        Cow::Borrowed(text) => redact_parameters(text),
        Cow::Owned(masked) => Cow::Owned(redact_parameters(&masked).into_owned()),
    }
}

fn redact_url_credentials(text: &str) -> Cow<'_, str> {
    if !text.contains("://") || !text.contains('@') {
        return Cow::Borrowed(text);
    }
    let mut redacted = String::with_capacity(text.len());
    let mut start = 0;
    let mut changed = false;
    while let Some(found) = text.get(start..).and_then(|rest| rest.find("://")) {
        let authority_start = start + found + 3;
        let rest = text.get(authority_start..).unwrap_or_default();
        let authority_len = rest
            .find(|c: char| {
                matches!(c, '/' | '?' | '#' | '"' | '\'' | ')' | ']' | '>') || c.is_whitespace()
            })
            .unwrap_or(rest.len());
        let authority = rest.get(..authority_len).unwrap_or_default();
        if let Some(at) = authority.rfind('@') {
            redacted.push_str(text.get(start..authority_start).unwrap_or_default());
            redacted.push_str("redacted");
            start = authority_start + at;
            changed = true;
        } else {
            let next = authority_start + authority_len;
            redacted.push_str(text.get(start..next).unwrap_or_default());
            start = next;
        }
    }
    if !changed {
        return Cow::Borrowed(text);
    }
    redacted.push_str(text.get(start..).unwrap_or_default());
    Cow::Owned(redacted)
}

#[derive(Clone, Copy)]
struct SecretParameter {
    offset: usize,
    len: usize,
}

fn redact_parameters(text: &str) -> Cow<'_, str> {
    let lowercase = text.to_ascii_lowercase();
    if !SECRET_PARAMETERS
        .iter()
        .any(|parameter| lowercase.contains(parameter))
    {
        return Cow::Borrowed(text);
    }
    let mut redacted = String::with_capacity(text.len());
    let mut start = 0;
    let mut changed = false;
    while let Some(found) = next_secret_parameter(&lowercase, start) {
        let mut value_start = found.offset + found.len;
        let mut rest = text.get(value_start..).unwrap_or_default();
        let value_len = match rest.chars().next().filter(|c| matches!(c, '"' | '\'')) {
            Some(quote) => {
                value_start += 1;
                rest = text.get(value_start..).unwrap_or_default();
                rest.find(quote).unwrap_or(rest.len())
            }
            None => rest
                .find(|c: char| {
                    matches!(c, '&' | '"' | '\'' | '#' | ')' | ']' | '>') || c.is_whitespace()
                })
                .unwrap_or(rest.len()),
        };
        redacted.push_str(text.get(start..value_start).unwrap_or_default());
        redacted.push_str("redacted");
        start = value_start + value_len;
        changed = true;
    }
    if !changed {
        return Cow::Borrowed(text);
    }
    redacted.push_str(text.get(start..).unwrap_or_default());
    Cow::Owned(redacted)
}

fn next_secret_parameter(lowercase: &str, start: usize) -> Option<SecretParameter> {
    let bytes = lowercase.as_bytes();
    let mut best: Option<SecretParameter> = None;
    for parameter in SECRET_PARAMETERS {
        let mut cursor = start;
        while let Some(found) = lowercase
            .get(cursor..)
            .and_then(|rest| rest.find(parameter))
        {
            let offset = cursor + found;
            let boundary = offset == 0
                || bytes.get(offset - 1).is_some_and(|byte| {
                    !(byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-')
                });
            if boundary {
                if best.is_none_or(|current| {
                    offset < current.offset
                        || (offset == current.offset && parameter.len() > current.len)
                }) {
                    best = Some(SecretParameter {
                        offset,
                        len: parameter.len(),
                    });
                }
                break;
            }
            cursor = offset + 1;
        }
    }
    best
}

/// A client sees its own request errors, and only a generic message for a server fault.
impl From<RingRpcError> for ErrorObjectOwned {
    fn from(failure: RingRpcError) -> Self {
        use RingRpcError::*;
        let (code, message) = match &failure {
            Relay(relay) => return relay.into(),
            RingRequired | RingNotServed | InvalidPage | InvalidUtxoRequest | InvalidCursor
            | Unauthorized(_) => {
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
            Busy | RelayProverBusy => {
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
            Assets(inner) => {
                error!("asset registry is unusable {inner}");
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
            RecordUpdate(refusal) => {
                return ErrorObjectOwned::owned(refusal.code(), refusal.to_string(), None::<()>);
            }
            Enrollment(refusal) => {
                return ErrorObjectOwned::owned(*refusal as i32, refusal.to_string(), None::<()>);
            }
            StaleRecords | StorageUpdate(_) => {
                error!("storage update failed {}", redacted(&failure));
                (ErrorCode::InternalError, "record writer is unavailable")
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
                error!("managed merge failed {}", redacted(&failure));
                (ErrorCode::InternalError, "managed merge failed")
            }
        };
        ErrorObjectOwned::owned(code.code(), message, None::<()>)
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
            let object = ErrorObjectOwned::from(RingRpcError::Custody(failure));
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
        let logged = redacted(&RingRpcError::Upstream(upstream));
        assert!(!logged.contains("f00dcafe"), "{logged}");
        assert!(logged.contains("api-key="), "{logged}");
    }

    #[test]
    fn redacted_masks_url_credentials_and_query_secrets_without_mangling_pubkey() {
        let db = sea_orm::DbErr::Custom(
            "connect postgres://admin:p@ssword123@db.internal:5432/ring?token=tok_secret&key=k_secret&pubkey=KeepMe"
                .to_owned(),
        );
        let logged = redacted(&RingRpcError::Store(db));
        assert!(!logged.contains("admin"), "{logged}");
        assert!(!logged.contains("p@ssword123"), "{logged}");
        assert!(!logged.contains("tok_secret"), "{logged}");
        assert!(!logged.contains("k_secret"), "{logged}");
        assert!(
            logged.contains("postgres://redacted@db.internal:5432/ring"),
            "{logged}"
        );
        assert!(logged.contains("token=redacted"), "{logged}");
        assert!(logged.contains("key=redacted"), "{logged}");
        assert!(logged.contains("pubkey=KeepMe"), "{logged}");

        let dsn =
            sea_orm::DbErr::Custom("host=db user=app password=dbsecret dbname=ring".to_owned());
        let logged_dsn = redacted(&RingRpcError::Store(dsn));
        assert!(!logged_dsn.contains("dbsecret"), "{logged_dsn}");
        assert!(logged_dsn.contains("password=redacted"), "{logged_dsn}");

        let punctuated = sea_orm::DbErr::Custom(
            "host=db password=my!secret@123$:*^&token='p@ss!word' dbname=ring".to_owned(),
        );
        let logged_punctuated = redacted(&RingRpcError::Store(punctuated));
        assert!(
            !logged_punctuated.contains("my!secret@123"),
            "{logged_punctuated}"
        );
        assert!(
            !logged_punctuated.contains("p@ss!word"),
            "{logged_punctuated}"
        );
        assert!(
            logged_punctuated.contains("password=redacted&token='redacted'"),
            "{logged_punctuated}"
        );
    }

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
                ErrorObjectOwned::from(RingRpcError::Enrollment(refusal)).code(),
                code
            );
        }
    }

    #[test]
    fn exhausted_merge_scan_has_an_explicit_rpc_error() {
        let response =
            ErrorObjectOwned::from(RingRpcError::Origin(AuditError::MergeAuditWorkBudget));
        assert_eq!(response.code(), ErrorCode::InternalError.code());
        assert_eq!(
            response.message(),
            "audit scan exceeded its per-request work budget"
        );
    }
}
