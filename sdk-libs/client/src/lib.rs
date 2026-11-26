//! The custom ring auditor client, free of test utilities so the end-to-end test runs its path.

#[path = "operations/anonymous.rs"]
pub mod anonymous;
#[path = "state/anonymous_wallet.rs"]
mod anonymous_wallet;
#[path = "codec/compute_units.rs"]
pub mod compute_units;
#[path = "operations/confidential.rs"]
mod confidential;
#[path = "crypto/decrypt/mod.rs"]
mod decrypt;
#[path = "operations/delivered_utxos.rs"]
pub mod delivered;
#[path = "crypto/disclosure/mod.rs"]
mod disclosure;
#[path = "crypto/encryption/mod.rs"]
mod encryption;
mod error;
#[path = "state/exposed_notes.rs"]
mod exposed_notes;
#[path = "codec/frame.rs"]
pub mod frame;
#[path = "rpc/history.rs"]
pub mod history;
#[path = "rpc/indexer.rs"]
mod indexer;
#[path = "codec/key_catalog.rs"]
pub mod key_catalog;
#[path = "state/known_utxo_index.rs"]
mod known;
#[path = "operations/merge_audit.rs"]
mod merge_audit;
#[path = "codec/merge_data.rs"]
pub mod merge_data;
#[path = "state/merge_ledger.rs"]
mod merge_ledger;
#[path = "codec/origin/mod.rs"]
mod origin;
#[path = "codec/owner_signature.rs"]
pub mod owner_signature;
#[path = "codec/program_data.rs"]
mod program_data;
#[path = "security/reader.rs"]
mod reader;
#[path = "operations/records.rs"]
pub mod records;
#[path = "operations/recover.rs"]
mod recover;
#[path = "operations/resolver.rs"]
pub mod resolver;
#[path = "codec/ring_keys.rs"]
mod ring_keys;
#[cfg(feature = "rpc")]
#[path = "rpc/mod.rs"]
pub mod rpc;
#[path = "operations/scan.rs"]
mod scan;
#[path = "codec/storage_accounts.rs"]
pub mod storage_accounts;
#[path = "codec/storage_data.rs"]
pub mod storage_data;
#[path = "rpc/tags.rs"]
pub mod tags;
#[path = "codec/transfer_accounts.rs"]
pub mod transfer_accounts;
mod types;
#[path = "state/viewing_history.rs"]
pub mod viewing_history;
#[path = "operations/viewing_rotation.rs"]
mod viewing_rotation;
#[path = "codec/vouch.rs"]
pub mod vouch;
#[path = "operations/wallet_sync.rs"]
mod wallet_sync;

pub(crate) const REDACTED: &str = "<redacted>";

pub use crate::anonymous::{
    AnonymousRingUtxo, AnonymousScan, AnonymousStream, AnonymousSync, AuditorRecovery,
    RecoveryStatus, RingSenderAudit, SenderAuditFrame, SenderAuditFrames, SenderClaim, SentCheck,
    SentPayment,
};
pub use crate::anonymous_wallet::{
    AnonymousWallet, LedgerScan, PendingClaim, PendingNote, ScanCommit,
};
pub use crate::compute_units::{compute_unit_limit, PROOF_UNITS};
pub use crate::delivered::RingNoteCodec;
pub use crate::frame::{FrameEncode, Settlement, SppExpansion, MAX_CPI_DATA_LEN};
pub use crate::key_catalog::{
    proving_key_hashes, FittedSpend, KeyWidths, RingKey, RowAuthorization, TransferAddresses,
    TransferKeyRow, RING_KEYS,
};
pub use crate::merge_data::{CustomRingMergeIxData, SealedMergeRecord};
#[cfg(feature = "solana-rpc")]
pub use crate::origin::{confirmed_origin, ConfirmedRingOrigins, ORIGIN_TRANSACTION_CONFIG};
pub use crate::owner_signature::{OwnerSignature, SignatureInstruction};
pub use crate::storage_accounts::{OwnerGrantHost, StorageRegistryAddress, TrackingRegistryHost};
pub use crate::storage_data::{
    AuthorityWriteEntry, EnrollmentRequestEncode, ListWriteData, StorageEventDecode,
    StorageUpdateCodec,
};
pub use crate::tags::{HistoryRows, IndexerReads, TagWindow, TAGS_PER_REQUEST};
pub use crate::transfer_accounts::{
    AnonymousExitDestination, ApprovalEncode, CoSignerPairs, KnownPrefix, ListRegistry,
    PolicySources, WithdrawalLeg, SYSTEM_PROGRAM_ID, TRANSFER_APPROVAL_DATA_LEN,
};
pub use crate::viewing_history::{
    AdvancedTag, StreamCounter, ViewingHistory, ViewingKeyEntry, PENDING_RETENTION,
};
pub use crate::vouch::VouchMessage;
pub use crate::{
    confidential::{RingConfidentialNotes, RingConfidentialScan, RingFullScan},
    decrypt::{FramedRow, SealedRecipientKeys, TransactionAudit},
    disclosure::{AuditEnvelope, AuditStreamSeal, OpenedAudit, SealedAuditStream},
    encryption::{
        auditor_view_tag, open_merge_record, MergeRecordSeal, NullifierKeyEnvelope,
        SealedMergeCiphertexts, SealedMergeOpening, SealedNullifierKey,
    },
    error::{AnonymousError, AuditError, AuditFailureKind, RowDisposition},
    exposed_notes::{
        is_zero_key, mint_balances, note_state, verify_ring_owned, MintBalance, NoteState,
        RingOwnedUtxos,
    },
    indexer::{AsyncRingIndexer, RingIndexer, RingIndexerError},
    known::KnownUtxoIndex,
    merge_audit::{confirmed_merge, MergeAudit, MergeAuditPage},
    merge_ledger::{AuditedMergeOutput, ConfirmedMerge, MergeId, MergeLedger, MergeRecordOutput},
    origin::{
        ring_event_origin_in, ConfirmedEvent, ConfirmedEventKind, ConfirmedMergeDisclosure,
        RingOrigin, RingWithdrawal, TransactionOrigin,
    },
    reader::{Ed25519ReaderKey, P256ReaderKey, ReaderKey, ReaderKeyError},
    records::{
        member_record, owner_record_key, path_of, storage_apply_proof_inputs, MemberOpeningRead,
        RecordPoolCheck, RecordPoolRead, StorageTip,
    },
    recover::{
        MemberRecovery, RecoveredNotes, RecoveryProgress, RecoveryStep, RingRecovery, SourceMember,
    },
    resolver::{
        RecipientResolver, RegisteredKeys, ResolveError, ResolvedRecipient, TrackingLeaf,
        TrackingLeafCheck,
    },
    scan::{AuditedPage, RingScan, RingScanPage, ScanPage, SkippedRow},
    types::{
        AuditedEntryInput, AuditedOutput, AuditedTransaction, AuditedUtxo, CreatedUtxo, KnownUtxo,
        OutputOpening, OutputPayloadError, OutputPayloadErrorKind, RingOwnerTag, UtxoOwner,
    },
};
pub use program_data::{program_data, ProgramData, PROGRAM_DATA_METADATA_LEN};
pub use viewing_rotation::{ViewingRotation, ViewingRotationError};
pub use wallet_sync::{RingWalletNotes, RingWalletScanned, RingWalletSync, RingWalletSynced};
pub use zolana_ring_indexer::rpc::RingRpcError;

/// The answer to a one-item request, a holder that answers more or less is refused.
pub fn exactly_one<T>(answers: Vec<T>) -> Result<T, spr_transaction::TransactionError> {
    let got = answers.len();
    <[T; 1]>::try_from(answers)
        .map(|[answer]| answer)
        .map_err(|_| spr_transaction::TransactionError::IncompleteDerivation { got, want: 1 })
}

/// One answer per request, a holder that answers more or less is refused.
pub(crate) fn exactly_one_each<T>(
    answers: Vec<T>,
    want: usize,
) -> Result<Vec<T>, spr_transaction::TransactionError> {
    match answers.len() {
        got if got == want => Ok(answers),
        got => Err(spr_transaction::TransactionError::IncompleteDerivation { got, want }),
    }
}
