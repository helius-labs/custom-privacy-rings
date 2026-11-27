use std::collections::HashSet;

use crate::client_rate::RateLimit;
use jsonrpsee::types::ErrorObjectOwned;
use ring_audit::RingAuditError;
use ring_cosigner::RingCosignerError;
use ring_relay::RingRelayError;
use ring_service_core::{error::RefusalCode, ServiceError};
use ring_writer::{
    enrollment_applier::EnrollmentRefusal, record_update::RecordUpdateRefusal, WriterError,
};

/// Every refusal code the service answers with, in code order.
pub(crate) fn every_code() -> Vec<RefusalCode> {
    let mut codes = [
        ServiceError::codes(),
        WriterError::codes(),
        RingAuditError::codes(),
        RingCosignerError::codes(),
        RingRelayError::codes(),
        RateLimit::codes(),
    ]
    .concat();
    codes.sort_by_key(|refusal| std::cmp::Reverse(refusal.code));
    codes
}

#[test]
fn every_rpc_error_code_is_unique_across_the_crates() {
    let mut seen = HashSet::new();
    let repeated: Vec<i32> = every_code()
        .into_iter()
        .map(|refusal| refusal.code)
        .filter(|code| !seen.insert(*code))
        .collect();
    assert!(repeated.is_empty(), "{repeated:?}");
}

fn edge(error: impl Into<RingRelayError>) -> (i32, String) {
    let object = ErrorObjectOwned::from(error.into());
    (object.code(), object.message().to_owned())
}

#[test]
fn every_edge_error_keeps_its_code_and_message() {
    use ring_audit::{error::GrantRefusal, tracking::TrackingRefusal};
    use ring_cosigner::cosign::CoSignError;
    use ring_relay::{authority_transfer::AuthorityTransferError, relay::RelayError};
    use ring_service_core::authorize::Unauthorized;
    use zolana_operator_custody::CustodyError;
    use zolana_ring_client::{
        rpc::{KeyFileError, RelayProofInputsError, TokenAccountSetupError},
        AuditError, SealRow,
    };

    const INVALID: i32 = -32600;
    const INTERNAL: i32 = -32603;
    const BUSY: i32 = -32009;
    for refusal in CoSignError::ALL {
        let wrapped = RingCosignerError::CoSign(refusal);
        assert_eq!(edge(wrapped), (refusal as i32, refusal.to_string()));
    }
    for refusal in AuthorityTransferError::ALL {
        let wrapped = RingRelayError::AuthorityTransfer(refusal);
        assert_eq!(edge(wrapped), (refusal as i32, refusal.to_string()));
    }
    for refusal in RecordUpdateRefusal::ALL {
        assert_eq!(edge(refusal), (refusal as i32, refusal.to_string()));
    }
    for refusal in EnrollmentRefusal::ALL {
        assert_eq!(edge(refusal), (refusal as i32, refusal.to_string()));
    }
    for refusal in GrantRefusal::ALL {
        let wrapped = RingAuditError::Grant(refusal);
        assert_eq!(edge(wrapped), (refusal as i32, refusal.to_string()));
    }
    let relay = [
        (RelayError::RingNotServed, -32100),
        (RelayError::QuoteFeePayer, -32101),
        (RelayError::QuoteExpired, -32102),
        (RelayError::QuoteWindow, -32103),
        (RelayProofInputsError::NullifierKey.into(), -32104),
        (RelayError::RingMismatch, -32105),
        (RelayError::RailMismatch, -32106),
        (RelayError::OwnerSignature, -32107),
        (RelayError::Proof("no route".to_owned()), -32108),
        (RelayError::CoSignRefused, -32109),
        (RelayError::UnknownRelay, -32110),
        (RelayError::UnexpectedSigner, -32111),
        (RelayError::SignerSignature, -32112),
        (TokenAccountSetupError::Repeated { index: 1 }.into(), -32200),
        (RelayError::DepositorIsFeePayer, -32201),
        (RelayError::Budget, -32203),
        (RelayError::SignerBudget, -32204),
        (RelayError::DelegateSignature, -32206),
        (RelayError::Allowance, -32207),
        (RelayError::LiveAttempt, -32208),
        (RelayError::DepositorSignature, -32209),
        (RelayError::BudgetUnset, -32210),
        (
            RelayError::Reseal {
                row: SealRow::Narrow,
            },
            -32211,
        ),
        (RelayError::Reseal { row: SealRow::Wide }, -32212),
        (
            RelayError::Oversized {
                reason: "too wide".to_owned(),
            },
            -32216,
        ),
    ];
    for (refusal, code) in relay {
        let message = refusal.to_string();
        assert_eq!(edge(RingRelayError::Relay(refusal)), (code, message));
    }
    let unauthorized = [
        Unauthorized::StaleTimestamp,
        Unauthorized::UnknownReaderKey,
        Unauthorized::PasskeyNeedsAssertion,
        Unauthorized::UnexpectedAssertion,
        Unauthorized::BadSignature,
        Unauthorized::NotAnAssertion,
        Unauthorized::ChallengeMismatch,
        Unauthorized::OriginNotAllowed,
        Unauthorized::CrossOriginAssertion,
        Unauthorized::RelyingPartyMismatch,
        Unauthorized::UserVerificationMissing,
        Unauthorized::NoConfig,
        Unauthorized::AuditorKeyMismatch,
        Unauthorized::NotGranted,
        Unauthorized::InvalidNonce,
        Unauthorized::Replay,
        Unauthorized::ClusterMismatch,
        Unauthorized::NotRingAuthority,
        Unauthorized::NotPolicyAuthority,
    ];
    for refusal in unauthorized {
        let message = format!("request is unauthorized because {refusal}");
        assert_eq!(edge(ServiceError::from(refusal)), (INVALID, message));
    }
    let fixed: Vec<(RingRelayError, i32, &str)> = vec![
        (
            ServiceError::SendRefused {
                reason: "blockhash".to_owned(),
            }
            .into(),
            -32113,
            "the cluster refused the transaction because blockhash",
        ),
        (ServiceError::Busy.into(), BUSY, "audit service is busy"),
        (
            ServiceError::Upstream(spr_client::ClientError::Rpc("down".to_owned())).into(),
            INTERNAL,
            "upstream request failed",
        ),
        (
            ServiceError::from(zolana_ring_client::RingIndexerError::Refused(
                zolana_ring_client::RingRpcError::RegistryOutOfSync,
            ))
            .into(),
            INTERNAL,
            "upstream request failed",
        ),
        (
            ServiceError::Origin(AuditError::MergeAuditWorkBudget).into(),
            INTERNAL,
            "audit scan exceeded its per-request work budget",
        ),
        (
            ServiceError::Origin(AuditError::InvalidHistoryPage).into(),
            INTERNAL,
            "upstream request failed",
        ),
        (
            ServiceError::InvalidIndexerResponse.into(),
            INTERNAL,
            "indexer response is invalid",
        ),
        (
            ServiceError::Custody(CustodyError::KeyArn).into(),
            INTERNAL,
            "operator signing failed",
        ),
        (
            ServiceError::Derivation(spr_keypair::KeypairError::InvalidPublicKey).into(),
            INTERNAL,
            "auditor key is unavailable",
        ),
        (
            ServiceError::AuditorKey(KeyFileError::ZeroAuditorKey).into(),
            INTERNAL,
            "auditor key is unavailable",
        ),
        (
            ServiceError::StateUnavailable.into(),
            INTERNAL,
            "audit service is unavailable",
        ),
        (
            ServiceError::Store(sea_orm::DbErr::Custom("gone".to_owned())).into(),
            INTERNAL,
            "audit service is unavailable",
        ),
        (
            RingAuditError::RingRequired.into(),
            INVALID,
            "ring_program_id is required when keys are derived per ring",
        ),
        (
            RingAuditError::RingNotServed.into(),
            INVALID,
            "ring is not served by the local auditor key",
        ),
        (
            RingAuditError::InvalidPage.into(),
            INVALID,
            "audit page is invalid",
        ),
        (
            RingAuditError::InvalidUtxoRequest.into(),
            INVALID,
            "a UTXO read names 1 to 100 UTXO hashes",
        ),
        (
            RingAuditError::InvalidCursor.into(),
            INVALID,
            "the cursor is no position of the read",
        ),
        (
            RingAuditError::WalletSnapshotExpired.into(),
            -32172,
            "wallet snapshot expired, restart without a cursor",
        ),
        (
            RingAuditError::WalletSnapshotCapacity.into(),
            -32173,
            "wallet snapshot capacity is exhausted",
        ),
        (
            RingAuditError::Assets(spr_transaction::TransactionError::TooManyOutputs).into(),
            INTERNAL,
            "upstream request failed",
        ),
        (
            RingAuditError::Member(zolana_ring_policy::MemberError::Zero).into(),
            INTERNAL,
            "audit service is unavailable",
        ),
        (
            RingAuditError::InvalidGrantRow.into(),
            INTERNAL,
            "audit service is unavailable",
        ),
        (
            RingAuditError::GrantStoreMissing.into(),
            INTERNAL,
            "audit service is unavailable",
        ),
        (
            RingAuditError::GrantRowsKeyMissing.into(),
            INTERNAL,
            "audit service is unavailable",
        ),
        (
            RingAuditError::Tracking(TrackingRefusal::Incomplete).into(),
            BUSY,
            "tracking discovery has not reached the root's next index",
        ),
        (
            RingAuditError::Tracking(TrackingRefusal::Opening).into(),
            INTERNAL,
            "tracking discovery failed",
        ),
        (
            WriterError::StaleRecords.into(),
            INTERNAL,
            "record writer is unavailable",
        ),
        (
            WriterError::StorageUpdate(Box::new(zolana_ring_client::RingError::Member(
                zolana_ring_policy::MemberError::Zero,
            )))
            .into(),
            INTERNAL,
            "record writer is unavailable",
        ),
        (
            WriterError::PrivateWrites(Box::new(RingAuditError::InvalidPage)).into(),
            INVALID,
            "audit page is invalid",
        ),
        (
            RingCosignerError::CoSignerStoreMissing.into(),
            INTERNAL,
            "audit service is unavailable",
        ),
        (
            RingRelayError::MergeSettingsUnavailable,
            -32170,
            "merge settings need a service store",
        ),
        (
            RingRelayError::StaleMergeSettings,
            -32171,
            "stored merge settings are signed at the same time or later",
        ),
        (
            RingRelayError::RelayProverBusy,
            BUSY,
            "relay prover is busy",
        ),
    ];
    for (error, code, message) in fixed {
        assert_eq!(edge(error), (code, message.to_owned()));
    }
}
