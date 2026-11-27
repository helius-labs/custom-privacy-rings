//! The custom ring auditor client, free of test utilities so the end-to-end test runs its path.
//!
//! Feature `operations` adds the instruction builders, proofs and submission, a blocking caller
//! wraps an operation in `zolana.block_on(op.prove(zolana.as_async()))`.

#[cfg(test)]
extern crate self as zolana_ring_client;

#[cfg(feature = "operations")]
#[path = "state/ring_accounts.rs"]
mod account;
#[cfg(feature = "operations")]
#[path = "transfer/address_ceiling.rs"]
mod address_ceiling;
#[cfg(feature = "operations")]
#[path = "admin/roles.rs"]
mod admin;
#[path = "audit/anonymous.rs"]
mod anonymous;
#[path = "state/anonymous_wallet.rs"]
mod anonymous_wallet;
#[cfg(feature = "operations")]
#[path = "transport/ring_client.rs"]
mod client;
#[cfg(feature = "operations")]
#[path = "transfer/co_signer.rs"]
mod co_signer;
#[path = "codec/compute_units.rs"]
mod compute_units;
#[path = "audit/confidential.rs"]
mod confidential;
#[path = "crypto/decrypt/mod.rs"]
mod decrypt;
#[cfg(feature = "operations")]
#[path = "transfer/delegate_source.rs"]
mod delegate_source;
#[path = "audit/delivered_utxos.rs"]
mod delivered;
#[cfg(feature = "operations")]
#[path = "transfer/deposit.rs"]
mod deposit;
#[path = "crypto/disclosure/mod.rs"]
mod disclosure;
#[path = "crypto/encryption/mod.rs"]
mod encryption;
#[cfg(feature = "operations")]
#[path = "enrollment/owner.rs"]
mod enrollment;
mod error;
#[path = "state/exposed_notes.rs"]
mod exposed_notes;
#[path = "codec/frame.rs"]
mod frame;
#[cfg(feature = "operations")]
#[path = "transport/heap.rs"]
mod heap;
#[path = "transport/history.rs"]
pub mod history;
#[path = "transport/ring_indexer.rs"]
mod indexer;
#[path = "codec/key_catalog.rs"]
mod key_catalog;
#[cfg(feature = "operations")]
#[path = "transfer/limits.rs"]
mod limits;
#[cfg(feature = "operations")]
#[path = "merge/builder.rs"]
mod merge;
#[path = "merge/audit.rs"]
mod merge_audit;
#[path = "codec/merge_data.rs"]
mod merge_data;
#[path = "state/merge_ledger.rs"]
mod merge_ledger;
#[path = "codec/origin/mod.rs"]
mod origin;
#[path = "codec/owner_signature.rs"]
mod owner_signature;
#[cfg(feature = "operations")]
#[path = "transfer/pipeline/mod.rs"]
mod pipeline;
#[path = "codec/program_data.rs"]
mod program_data;
#[cfg(feature = "operations")]
#[path = "transport/projection.rs"]
mod projection;
#[cfg(feature = "operations")]
#[path = "proof/prover.rs"]
mod prover;
#[cfg(feature = "operations")]
#[path = "enrollment/publish_viewing_rotation.rs"]
mod publish_viewing_rotation;
#[path = "security/reader.rs"]
mod reader;
#[path = "records/openings.rs"]
pub mod records;
#[path = "audit/recover.rs"]
mod recover;
#[cfg(feature = "operations")]
#[path = "records/registry.rs"]
mod registry_records;
#[cfg(feature = "operations")]
#[path = "transport/ring_relay.rs"]
mod relay;
#[cfg(feature = "operations")]
#[path = "transfer/relay_message.rs"]
mod relay_message;
#[cfg(feature = "operations")]
#[path = "proof/request.rs"]
mod request;
#[path = "records/resolver.rs"]
pub mod resolver;
#[cfg(feature = "operations")]
mod ring_error;
#[path = "codec/ring_keys.rs"]
mod ring_keys;
#[cfg(feature = "rpc")]
#[path = "rpc/mod.rs"]
pub mod rpc;
#[path = "audit/scan.rs"]
mod scan;
#[cfg(feature = "operations")]
#[path = "transfer/utxo_selection.rs"]
mod select;
#[cfg(feature = "operations")]
#[path = "transport/signature_history.rs"]
mod signature_history;
#[cfg(feature = "operations")]
#[path = "records/storage.rs"]
mod storage;
#[path = "codec/storage_accounts.rs"]
mod storage_accounts;
#[path = "codec/storage_data.rs"]
mod storage_data;
#[cfg(feature = "operations")]
#[path = "transport/submission.rs"]
mod submission;
#[path = "transport/tags.rs"]
mod tags;
#[cfg(all(test, feature = "operations"))]
mod tests;
#[path = "codec/transfer_accounts.rs"]
mod transfer_accounts;
#[cfg(feature = "operations")]
#[path = "state/pool_trees.rs"]
mod trees;
mod types;
#[cfg(feature = "operations")]
#[path = "enrollment/vault.rs"]
mod vault_enrollment;
#[path = "state/viewing_history.rs"]
mod viewing_history;
#[path = "enrollment/viewing_rotation.rs"]
mod viewing_rotation;
#[path = "codec/vouch.rs"]
mod vouch;
#[cfg(feature = "operations")]
#[path = "transfer/wallet_payment.rs"]
mod wallet_payment;
#[path = "audit/wallet_sync.rs"]
mod wallet_sync;

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
pub use crate::origin::{ConfirmedRingOrigins, FinalizedRpcOrigin, ORIGIN_TRANSACTION_CONFIG};
pub use crate::owner_signature::{OwnerSignature, SignatureInstruction};
pub use crate::storage_accounts::{OwnerGrantHost, StorageRegistryAddress, TrackingRegistryHost};
pub use crate::storage_data::{
    AuthorityWriteEntry, EnrollmentRequestEncode, ListWriteData, StorageEventDecode,
    StorageUpdateCodec,
};
pub use crate::transfer_accounts::{
    located_layout, AnonymousExitDestination, AnonymousExitRefusal, ApprovalEncode, CoSignerPairs,
    KnownPrefix, ListRegistry, PolicySources, WithdrawalLeg, SYSTEM_PROGRAM_ID,
};
pub use crate::viewing_history::{
    AdvancedTag, HistoryUpdate, TransferTags, ViewingHistory, ViewingKeyEntry, TAG_WINDOW,
};
pub use crate::vouch::VouchMessage;
pub use crate::{
    confidential::{RingConfidentialNotes, RingConfidentialScan, RingFullScan},
    decrypt::{FramedRow, TransactionAudit},
    disclosure::{AuditEnvelope, AuditStreamSeal, OpenedAudit, SealedAuditStream},
    encryption::{
        auditor_view_tag, MergeRecordSeal, NullifierKeyEnvelope, SealedMergeOpening,
        SealedNullifierKey,
    },
    error::{AnonymousError, AuditError, AuditFailureKind, RowDisposition},
    exposed_notes::{
        mint_balances, note_state, verify_ring_owned, MintBalance, NoteState, RingOwnedUtxos,
    },
    indexer::{AsyncRingIndexer, RingIndexer, RingIndexerError},
    merge_audit::{confirmed_merge, MergeAudit, MergeAuditPage},
    merge_ledger::{AuditedMergeOutput, ConfirmedMerge, MergeLedger, MergeRecordOutput},
    origin::{
        ring_event_origin_in, ConfirmedEvent, ConfirmedEventKind, ConfirmedMergeDisclosure,
        FinalizedTransactionOrigin, RingOrigin, RingWithdrawal, TransactionOrigin,
    },
    reader::{Ed25519ReaderKey, P256ReaderKey, ReaderKey, ReaderKeyError},
    records::{
        member_record, owner_record_key, path_of, storage_apply_proof_inputs, MemberOpeningRead,
        StorageTip,
    },
    recover::{
        MemberRecovery, RecoveredNotes, RecoveryProgress, RecoveryStep, RingRecovery, SourceMember,
    },
    resolver::{RegisteredKeys, ResolveError, TrackingLeaf, TrackingLeafCheck},
    scan::{RingScan, ScanPage, SkippedRow},
    types::{
        AuditedEntryInput, AuditedOutput, AuditedTransaction, AuditedUtxo, CreatedUtxo, KnownUtxo,
        OutputOpening, OutputPayloadError, OutputPayloadErrorKind, RingOwnerTag, UtxoOwner,
    },
};
pub use program_data::{program_data, ProgramData};
pub use viewing_rotation::ViewingRotation;
pub use wallet_sync::{
    RingWalletNotes, RingWalletScanned, RingWalletSources, RingWalletSync, RingWalletSynced,
};
pub use zolana_ring_indexer::rpc::RingRpcError;

#[cfg(feature = "operations")]
pub use custom_ring_interface::{
    tag, CoSignScope, CustomRingProof, InstructionTag, RingKind, RingMode,
};
#[cfg(feature = "operations")]
pub use spr_program::instruction::{DepositAsset, DepositSplAccounts};

pub(crate) use crate::exposed_notes::is_zero_key;
#[cfg(feature = "operations")]
pub use crate::prover::secret_json_body;
#[cfg(feature = "operations")]
pub(crate) use crate::resolver::{RecipientResolver, ResolvedRecipient};
#[cfg(feature = "operations")]
pub use crate::{
    account::{
        CustomRing, CustomRingCoSigner, CustomRingConfig, PinnedPolicy, RingRegistration,
        RingStatus,
    },
    address_ceiling::{
        AddressCeiling, AddressCeilingError, CeilingDecision, Fit, FitError, FitSigners,
        MergeGroup, SetupEffect, SpentInput, TransactionFit,
    },
    admin::{
        ClearCoSigner, InitialPolicy, Initialize, PolicyTable, PolicyUpdate, SetCoSigner,
        SetDelegate, SourceOwner, UpdateAuthority, UpdatePolicy,
    },
    client::{RingClient, RingIndexerBackend},
    delegate_source::{
        AllowedNullifiers, DelegateSourceError, ResolvedSource, SourceEnrollment, SourceSpends,
        TrackingEnrollment,
    },
    deposit::{
        DepositOutput, DepositRecipient, PreparedRingDeposit, RingDeposit, RingDepositBatch,
        RingDepositBatchReceipt, RingDepositReceipt,
    },
    enrollment::{
        Enroll, EnrolledLeaf, EnrollmentRead, EnrollmentService, OwnedChange, OwnerEnrollment,
    },
    merge::{
        CustomRingMerge, CustomRingMergeInstruction, FittedMerge, MergeProofInput, MergeRelay,
        PreparedCustomRingMerge, ProvenCustomRingMerge, RingMergeOperation, MAX_RING_MERGE_INPUTS,
        MERGE_DEFAULT_INPUT_COUNT,
    },
    pipeline::{
        entry_shape_keyed, shape_keyed, sole_mint, transfer_key, Anonymous, AnonymousMove,
        AnonymousPayment, AnonymousTransfer, ApplicationData, CachedNote, CoOwner,
        CustomRingTransfer, CustomRingTransferInput, DelegateCandidates, DelegateFunding,
        DelegateMint, DelegateMintAccount, DelegateMove, DelegateOutput, DelegatePayment,
        DelegateSource, DelegateTransfer, DelegateTransferInput, DelegateWithdrawal, Delegated,
        EntrySpend, NoteData, Owner, OwnerExposure, ProgramCommitment, ProgramPlan,
        ProvenRingTransfer, PublicOwnerTransfer, PublicOwners, Rail, RailKind, ReadCache, Relayed,
        RelayedTransferInput, RingPayment, RingSender, RingTransfer, SealRow, TransferKey,
        UnsignedRelay, VouchSigner,
    },
    publish_viewing_rotation::RotateViewingKey,
    registry_records::{
        AuthorityRecordUpdate, MemberRecords, OwnerListSalts, OwnerListTags, OwnerListTagsRead,
        PrivateKeyKind, PrivateListMember, RecordUpdateTarget, SaltedOwnerCheck,
        WalletRecordUpdate,
    },
    relay::{
        DelegateRequester, PendingSignature, PreparedAuthorityTransfer, PreparedRelay, RelaySent,
        RequestedAuthorityTransfer, RingRelay,
    },
    relay_message::{
        ApprovalLimit, ApprovalOpening, CoSignerApproval, CoSignerRule, ExactApproval,
        FunderApproval, LegDirection, MessageMismatch, OutputTotalsOpening, OwnerApproval,
        PublicLeg, PublicTotalsOpening, RelayFunderTerms, RelayTransferTerms, SlotCommitment,
        SlotOpening, TokenAccountCreate, VerifiedMessage,
    },
    ring_error::RingError,
    select::{select_merge_where, select_spend_where},
    signature_history::SignatureHistory,
    storage::{
        EnrollBatch, InitializeStorage, PrivateStorageWrite, ProvenEnrollBatch, ProvenPrivateWrite,
        RecordsCreation, SaltedOwnerWrite, StorageApplyProof, UpdateStorage,
    },
    submission::{
        live_registry, LocalSigners, RingOperation, RingSubmission, SettledSubmission,
        SubmissionSigners, SubmissionStatus,
    },
    trees::{tree_id, written_tree_fees},
    wallet_payment::{AnonymousRingTransfers, OwnerRingTransfers, RingProving},
};
#[cfg(feature = "operations")]
pub use vault_enrollment::{EnrollVault, VaultEnrollmentMessage};

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
