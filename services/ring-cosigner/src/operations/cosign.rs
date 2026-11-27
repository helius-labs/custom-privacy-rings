use std::collections::BTreeSet;

use async_trait::async_trait;
use clap::ValueEnum;
use custom_ring_interface::CoSignScope;
use jsonrpsee::types::ErrorObjectOwned;
use ring_service_core::{landing::FinalizedTip, upstream::ScopeUnreadable};
use sha2::{Digest, Sha256};
use solana_address::Address;
use solana_message::VersionedMessage;
use solana_signature::Signature;
use thiserror::Error;
use zolana_ring_policy::Member;

use crate::{cosigner::ApprovedCall, error::RingCosignerError};

/// One class of the five a ring's `CoSigner` records hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum ApproveClass {
    Outflow,
    Withdrawal,
    Transfers,
    Deposits,
    Withdrawals,
}

/// Explicit, an empty set refuses every operation whatever its limits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoSignerApproval(BTreeSet<ApproveClass>);

pub struct CoSignRequest {
    pub ring: Address,
    pub message: VersionedMessage,
    pub flows: CoSignFlows,
    /// The call a program makes under the message's `ApproveTransfer`.
    pub approved_transfer: Option<ApprovedCall>,
    pub sender: Sender,
}

/// Who sends the signed message and so who learns its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sender {
    /// The relay settles the reservation of its own send.
    Relay,
    /// The sweep settles from the ring history, no block past the height lands the message.
    Wallet { last_valid_block_height: u64 },
}

#[derive(Clone, Copy)]
pub(crate) struct FrameSource<'a> {
    pub(crate) message: &'a VersionedMessage,
    pub(crate) approved_transfer: Option<&'a ApprovedCall>,
}

/// A caller names no transfer debit, the co-signer reads each from the frame it signs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoSignFlows {
    Frame,
    /// A deposit's public leg, or nothing on a merge.
    Published(Vec<OwnerFlow>),
}

/// One reservation row, each owner and asset once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerFlow {
    pub owner: Member,
    pub flow: AssetFlow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetFlow {
    pub asset: Address,
    /// The owner's net outflow from `zolana_ring_policy::owner_debits`.
    pub debit: u128,
    /// `debit` plus the owner's charge, compared with owner caps and `cosign_above`.
    pub charged_outflow: u128,
    /// At most `charged_outflow`, and at most what the frame moves out of the ring of `asset`.
    pub exiting: u128,
    /// A sum of u64 legs, past `u64::MAX` while the net stays a u64 magnitude.
    pub deposited: u128,
    pub withdrawn: u128,
}

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoSignDecision {
    /// Over `CoSignRequest::message`.
    Signed(Signature),
    Refused(CoSignRefusal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoSignRefusal {
    Disabled,
    OverLimit,
    /// No class the operation demands sits in the explicit approval.
    NotApproved,
    Frozen,
    Blocked,
    NotAllowed,
}

ring_service_core::listed_refusals! {
    #[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
    #[repr(i32)]
    pub enum CoSignError {
        #[error("co-signing is disabled")]
        Disabled = -32160,
        #[error("the transaction exceeds a co-signer limit")]
        OverLimit = -32161,
        #[error("the co-sign flows name an owner and asset twice")]
        RepeatedAsset = -32162,
        #[error("the asset limits or an input opening are unreadable")]
        LimitsUnavailable = -32163,
        #[error("the message holds anything but one ring transfer the co-signer can read")]
        MessageScope = -32164,
        #[error("the co-signer approves no class the operation demands")]
        NotApproved = -32165,
        #[error("the co-sign reservation is already released")]
        ReleasedReservation = -32166,
        #[error("a sender or an asset of the transaction is frozen")]
        Frozen = -32167,
        #[error("a party or an asset of the transaction is blocked")]
        Blocked = -32168,
        #[error("a party or an asset of the transaction is not on the allow list")]
        NotAllowed = -32169,
        #[error("the list entry names no usable subject")]
        ListSubject = -32175,
        #[error("the simulation does not make the approved transfer call exactly once")]
        ApprovedCallNotOnce = -32176,
        #[error("the message names no blockhash the cluster holds")]
        StaleBlockhash = -32177,
        #[error("another transfer holds a funding account or note until it lands or expires")]
        InputReserved = -32178,
        #[error("a required signer other than the co-signer has not signed the transaction")]
        MissingSignature = -32179,
        #[error("the co-sign request names an asset with no record")]
        UnrecordedAsset = -32296,
    }
}

/// The digest of the co-signer and its serialized message, a reservation key no signer call derives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ticket([u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoSignSettlement {
    Confirmed(Ticket),
    Failed(Ticket),
}

impl ApproveClass {
    const fn scope(self) -> CoSignScope {
        match self {
            Self::Outflow => CoSignScope::APPROVE_OUTFLOW,
            Self::Withdrawal => CoSignScope::APPROVE_WITHDRAWAL,
            Self::Transfers => CoSignScope::TRANSFERS,
            Self::Deposits => CoSignScope::DEPOSITS,
            Self::Withdrawals => CoSignScope::WITHDRAWALS,
        }
    }
}

impl CoSignerApproval {
    pub fn new(classes: impl IntoIterator<Item = ApproveClass>) -> Self {
        Self(classes.into_iter().collect())
    }

    pub(crate) fn scope(&self) -> CoSignScope {
        self.0
            .iter()
            .fold(CoSignScope::NONE, |scope, class| scope | class.scope())
    }
}

impl Ticket {
    pub fn of(signer: &Address, message: &VersionedMessage) -> Self {
        Self(
            Sha256::new()
                .chain_update(signer)
                .chain_update(message.serialize())
                .finalize()
                .into(),
        )
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub(crate) const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl CoSignError {
    pub const fn code(self) -> i32 {
        self as i32
    }
}

impl From<ScopeUnreadable> for CoSignError {
    fn from(_: ScopeUnreadable) -> Self {
        Self::LimitsUnavailable
    }
}

impl From<CoSignRefusal> for CoSignError {
    fn from(refusal: CoSignRefusal) -> Self {
        match refusal {
            CoSignRefusal::Disabled => Self::Disabled,
            CoSignRefusal::OverLimit => Self::OverLimit,
            CoSignRefusal::NotApproved => Self::NotApproved,
            CoSignRefusal::Frozen => Self::Frozen,
            CoSignRefusal::Blocked => Self::Blocked,
            CoSignRefusal::NotAllowed => Self::NotAllowed,
        }
    }
}

impl From<CoSignError> for ErrorObjectOwned {
    fn from(error: CoSignError) -> Self {
        Self::owned(error.code(), error.to_string(), None::<()>)
    }
}

#[async_trait]
pub trait CoSignPolicy: Send + Sync {
    /// `None` when the policy never signs.
    fn signer(&self) -> Option<Address>;

    async fn decide(&self, request: &CoSignRequest) -> Result<CoSignDecision, RingCosignerError>;

    /// Keeps or frees the usage `decide` reserved for the transaction.
    async fn settle(&self, settlement: CoSignSettlement) -> Result<(), RingCosignerError>;

    /// Settles one bounded page of the reservations a wallet sends itself.
    async fn sweep(&self, tip: FinalizedTip) -> Result<(), RingCosignerError>;
}

pub struct RefusingCoSigner;

#[async_trait]
impl CoSignPolicy for RefusingCoSigner {
    fn signer(&self) -> Option<Address> {
        None
    }

    async fn decide(&self, _request: &CoSignRequest) -> Result<CoSignDecision, RingCosignerError> {
        Ok(CoSignDecision::Refused(CoSignRefusal::Disabled))
    }

    async fn settle(&self, _settlement: CoSignSettlement) -> Result<(), RingCosignerError> {
        Ok(())
    }

    async fn sweep(&self, _tip: FinalizedTip) -> Result<(), RingCosignerError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosign_error_codes_are_stable() {
        let codes = [
            -32160, -32161, -32162, -32163, -32164, -32165, -32166, -32167, -32168, -32169, -32175,
            -32176, -32177, -32178, -32179, -32296,
        ];
        for (error, code) in CoSignError::ALL.into_iter().zip(codes) {
            assert_eq!(ErrorObjectOwned::from(error).code(), code);
        }
    }
}
