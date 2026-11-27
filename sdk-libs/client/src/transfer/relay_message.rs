//! The message a wallet signs for Ring RPC, checked against the request the wallet made.

use custom_ring_interface::cosign::ApprovalMask;
use custom_ring_interface::stream::slot_tag;
use custom_ring_interface::{
    CustomRingError, InstructionTag, ParsedTransact, RecordKind, RingOutput, RingPolicy, RingRail,
    RingTransactData, SettlementLeg, TransferLayout,
};
use serde::{Deserialize, Serialize};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_message::{v1, VersionedMessage};
use solana_signature::Signature;
use solana_signer::Signer;
use spr_client::{ProofInputUtxo, MAX_LOADED_ACCOUNTS_DATA_SIZE};
use spr_hasher::primitives::right_align;
use spr_indexer_api::{Hash, SerializablePubkey};
use spr_interface::{
    instruction::InterfaceTransfer, pda, tree_slot::tree_id_field, N_PUBLIC_SLOTS,
};
use spr_keypair::{viewing_key::ViewTag, PublicKey, ShieldedAddress, X25519Pubkey};
use spr_program::instruction::{
    CreateAssociatedTokenAccount, TransactInterfaceTransferAccounts, TransactSolTransferAccounts,
    TransactSplWithdrawalAccounts,
};
use spr_transaction::{utxo::DUMMY_DOMAIN, Mint, TransactionError, Utxo, WalletUtxo, SOL_MINT};
use thiserror::Error;
use zolana_ring_keys::bootstrap_slot_tag;
use zolana_ring_keys::DeliveredSlot;
use zolana_ring_keys::OutputRecord;
use zolana_ring_policy::{ApprovalTotals, Member, OutputTotals, OwnerDebit, PublicTotals};
use zolana_ring_transaction::{open_recipient_body, record_key};

use crate::{
    compute_unit_limit,
    delegate_source::{SourceMint, SourceSpends},
    located_layout,
    pipeline::refused_delegate,
    rpc::{AuthorityWithdrawal, RelayId, TxViewingSecret},
    sole_mint, AuditEnvelope, CoSignerPairs, CustomRing, CustomRingConfig, DelegateRequester,
    OpenedAudit, PendingSignature, RequestedAuthorityTransfer, RingError,
};

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum MessageMismatch {
    #[error("the message does not decode as one valid canonical message")]
    Undecodable,
    #[error("the message is not a v1 message")]
    NotV1,
    #[error("the message fee payer is the signer or not the quoted relay fee payer")]
    FeePayer,
    #[error("the message budget is outside the operation's bounds")]
    Budget,
    #[error("the message carries instructions the request did not ask for")]
    Instructions,
    #[error("the message calls a program other than the ring")]
    Program,
    #[error("the message data differs from the request")]
    Data,
    #[error("the message accounts differ from the request")]
    Accounts,
    #[error("the message pays an owner the request does not name")]
    Recipients,
    #[error("the message publishes a transaction key other than the one the request names")]
    TransactionKey,
    #[error("the proven approval bits ask more than the co-signer accepts")]
    ApprovalMaskUnaccepted,
    #[error("the frame carries no approval digest and exact approval was asked")]
    MissingApprovalDigest,
    #[error("the opening does not rebuild the frame's approval digest")]
    FalseApprovalOpening,
    #[error("the opening does not hold one row per output of the key and per public slot")]
    ApprovalOpeningLayout,
    #[error("the opening's blinder is zero and hides no total")]
    ZeroApprovalBlinder,
    #[error("the opening's blinder served an earlier attempt")]
    ReusedApprovalBlinder,
    #[error("the totals pass a limit or name an asset with no limit")]
    ApprovalOverLimit,
    #[error("the message spends a held note the terms do not name")]
    SpendsUnnamedNote,
    #[error("the message spends a note no requested source opening derives")]
    SpendsUnrequestedNote,
    #[error("the change to a source exceeds what the message spends of it in that mint")]
    ChangeExceedsSpend,
    #[error("the terms leave a published output unopened")]
    UnopenedOutput,
    #[error("the public legs differ from the signer's terms")]
    Legs,
    #[error("the request asks for no signature")]
    Unrequested,
    #[error("the signing key is not the key the request names")]
    Signer,
}

/// Built from the wallet's own request, never from Ring RPC's answer.
#[derive(Clone, Debug)]
pub(crate) enum ExpectedMessage {
    Delegate(ExpectedDelegate),
    /// The sealing wallet hands each signer what it reads and signs nothing itself.
    Sealed(SealedTerms),
    PublicOwner(ExpectedPublicOwner),
    Funder(ExpectedFunder),
    CoSigner(ExpectedCoSigner),
}

/// What one public owner agrees to, its own slots and notes alone, the sealing wallet hands it over.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RelayTransferTerms {
    pub ring: SerializablePubkey,
    pub fee_payer: SerializablePubkey,
    pub signer: SerializablePubkey,
    /// One per published slot in slot order, each rebuilds its slot's commitment.
    pub slots: Vec<SlotOpening>,
    /// The signer's notes the transfer spends.
    pub nullifiers: Vec<Hash>,
    /// Every interface transfer leg of the frame in frame order.
    pub legs: Vec<PublicLeg>,
    /// The token accounts the fee payer creates ahead of the ring instruction.
    pub setup: Vec<TokenAccountCreate>,
}

/// What one public funder agrees to, every published output and public leg of the frame it funds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RelayFunderTerms {
    pub ring: SerializablePubkey,
    pub fee_payer: SerializablePubkey,
    pub signer: SerializablePubkey,
    /// One per published slot in slot order.
    pub slots: Vec<SlotCommitment>,
    /// Every interface transfer leg of the frame in frame order.
    pub legs: Vec<PublicLeg>,
    /// The token accounts the fee payer creates ahead of the ring instruction.
    pub setup: Vec<TokenAccountCreate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SlotCommitment {
    pub hash: Hash,
    /// `None` on an exit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<Hash>,
}

/// One interface transfer leg with its settlement group in frame order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PublicLeg {
    pub direction: LegDirection,
    /// `None` on SOL.
    pub mint: Option<SerializablePubkey>,
    pub amount: u64,
    pub accounts: Vec<SerializablePubkey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LegDirection {
    Deposit,
    Withdrawal,
}

/// The preimage of one published output's `utxo_hash`, and the tag its slot publishes.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SlotOpening {
    pub slot: u32,
    pub owner: SerializablePubkey,
    pub nullifier_pk: Hash,
    pub mint: SerializablePubkey,
    pub amount: u64,
    pub blinding: Hash,
    pub tree_id: u16,
    pub data_hash: Hash,
    pub ring_data_hash: Hash,
    /// The tag a non-exit slot publishes, no proof holds it and a slot under another is lost to its recipient's sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<Hash>,
}

zolana_ring_keys::redacted_debug!(SlotOpening {
    shown: [
        slot,
        owner,
        nullifier_pk,
        mint,
        amount,
        tree_id,
        data_hash,
        ring_data_hash,
        tag
    ],
    hidden: [blinding],
});

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TokenAccountCreate {
    pub owner: SerializablePubkey,
    pub mint: SerializablePubkey,
    pub token_program: SerializablePubkey,
}

#[derive(Clone, Debug)]
pub(crate) struct SealedTerms {
    pub owners: Vec<RelayTransferTerms>,
    pub funders: Vec<RelayFunderTerms>,
    pub approval: Option<ApprovalOpening>,
    pub charged_outflows: Vec<OwnerDebit>,
}

/// The totals and blinder a frame's digest commits, no transaction secret and no slot.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ApprovalOpening {
    pub blinder: Hash,
    pub outputs: Vec<OutputTotalsOpening>,
    pub public: Vec<PublicTotalsOpening>,
}

zolana_ring_keys::redacted_debug!(ApprovalOpening {
    shown: [outputs, public],
    hidden: [blinder],
});

/// Amounts are decimal text, a `u128` does not survive every JSON reader.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OutputTotalsOpening {
    pub asset: Hash,
    pub peak: String,
    pub charged_outflow: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PublicTotalsOpening {
    pub asset: Hash,
    pub peak: String,
    pub outflow: String,
    pub gross: String,
}

/// An outside co-signer reads the proven bits or the totals, never a slot.
#[must_use]
#[derive(Clone, Debug)]
pub enum CoSignerRule {
    /// The proven mask alone, no opening and no digest.
    Threshold {
        accepts: ApprovalMask,
    },
    Exact(ExactApproval),
}

#[must_use]
#[derive(Clone)]
pub struct ExactApproval {
    pub opening: ApprovalOpening,
    /// An asset the totals name with no limit here is refused.
    pub limits: Vec<ApprovalLimit>,
    /// The blinders of its earlier attempts, the only record of reuse the SDK has.
    pub seen_blinders: Vec<Hash>,
}

zolana_ring_keys::redacted_debug!(ExactApproval {
    shown: [opening, limits],
    hidden: [seen_blinders],
});

/// `peak` bounds one owner's outflow, `outflow` the sum over owners, `gross` each public row's
/// withdrawals with no deposit subtracted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApprovalLimit {
    pub mint: Address,
    pub peak: u128,
    pub outflow: u128,
    pub gross: u128,
}

/// A co-signer the frame names, what it signs is the rule it holds.
#[must_use]
pub struct CoSignerApproval<'a> {
    pub relay_id: RelayId,
    pub message: Vec<u8>,
    pub ring: Address,
    pub fee_payer: Address,
    pub signer: Address,
    pub config: &'a CustomRingConfig,
    /// The public creates a relay puts ahead of the ring instruction.
    pub setup: Vec<TokenAccountCreate>,
    pub rule: CoSignerRule,
}

#[derive(Clone, Debug)]
pub(crate) struct ExpectedCoSigner {
    ring: Address,
    fee_payer: Address,
    signer: Address,
    policy: Box<RingPolicy>,
    rail: RingRail,
    setup: Vec<Instruction>,
    rule: CoSignerRule,
}

/// One public owner reading its terms against the notes its wallet holds.
#[derive(Clone, Debug)]
pub(crate) struct ExpectedPublicOwner {
    terms: RelayTransferTerms,
    setup: Vec<Instruction>,
    /// From the ring account, a frame read under the wrong rail or policy proves nothing.
    policy: Box<RingPolicy>,
    rail: RingRail,
    known_nullifiers: Vec<[u8; 32]>,
}

/// One public funder reading the frame's outputs and legs against its terms.
#[derive(Clone, Debug)]
pub(crate) struct ExpectedFunder {
    terms: RelayFunderTerms,
    setup: Vec<Instruction>,
    policy: Box<RingPolicy>,
    rail: RingRail,
}

#[derive(Clone, Debug)]
pub(crate) struct ExpectedDelegate {
    pub ring: CustomRing,
    pub fee_payer: Address,
    pub delegate: Address,
    /// From the ring account, a frame read under the wrong rail or auditor proves nothing.
    pub policy: Box<RingPolicy>,
    pub rail: RingRail,
    pub auditor: X25519Pubkey,
    pub tx_viewing_key: TxViewingSecret,
    /// Change returns to an input's owner.
    pub sources: Vec<ExpectedOwner>,
    pub payments: Vec<ExpectedPayment>,
    pub withdrawals: Vec<PublicLeg>,
    pub requester: DelegateRequester,
    pub output_tree_id: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExpectedOwner {
    pub owner_key: [u8; 32],
    pub nullifier_pk: [u8; 32],
    pub viewing_u: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExpectedPayment {
    pub kind: RecordKind,
    pub owner: ExpectedOwner,
    pub mint: Address,
    pub amount: u64,
    /// The published owner tag, an exit's owner key or a payment's slot tag.
    pub tag: ViewTag,
}

/// Only a check builds one.
#[must_use = "sign the verified message"]
#[derive(Debug)]
pub struct VerifiedMessage {
    bytes: Vec<u8>,
    signer: Address,
}

struct Envelope {
    ring: CustomRing,
    program: Address,
    units: u32,
    fee_payer: Address,
    signer: Address,
    /// Ahead of the ring instruction in list order, the relay omits one for a live account.
    setup: Vec<Instruction>,
}

impl ExpectedMessage {
    pub fn check(
        &self,
        message: &[u8],
        signer: Address,
    ) -> Result<VerifiedMessage, MessageMismatch> {
        match self {
            Self::Delegate(delegate) => check_message(delegate, message, signer),
            Self::Sealed(_) => Err(MessageMismatch::Unrequested),
            Self::PublicOwner(owner) => check_message(owner, message, signer),
            Self::Funder(funder) => check_message(funder, message, signer),
            Self::CoSigner(co_signer) => check_message(co_signer, message, signer),
        }
    }

    pub fn terms_for(&self, signer: Address) -> Option<&RelayTransferTerms> {
        match self {
            Self::Sealed(sealed) => sealed.owners.iter().find(|terms| terms.signer.0 == signer),
            Self::PublicOwner(owner) => (owner.terms.signer.0 == signer).then_some(&owner.terms),
            Self::Delegate(_) | Self::Funder(_) | Self::CoSigner(_) => None,
        }
    }

    pub fn funder_terms_for(&self, signer: Address) -> Option<&RelayFunderTerms> {
        match self {
            Self::Sealed(sealed) => sealed.funders.iter().find(|terms| terms.signer.0 == signer),
            Self::Funder(funder) => (funder.terms.signer.0 == signer).then_some(&funder.terms),
            _ => None,
        }
    }

    pub fn approval_opening(&self) -> Option<&ApprovalOpening> {
        match self {
            Self::Sealed(sealed) => sealed.approval.as_ref(),
            _ => None,
        }
    }

    pub fn charged_outflows(&self) -> &[OwnerDebit] {
        match self {
            Self::Sealed(sealed) => &sealed.charged_outflows,
            _ => &[],
        }
    }
}

/// The frame's account list cut by the one layout the program reads it with.
struct DecodedTransfer<'a> {
    accounts: &'a [AccountMeta],
    layout: TransferLayout,
    legs: Vec<SettlementLeg>,
}

impl<'a> DecodedTransfer<'a> {
    fn decode(
        ring: CustomRing,
        policy: &RingPolicy,
        data: &RingTransactData<'_>,
        accounts: &'a [AccountMeta],
    ) -> Result<Self, MessageMismatch> {
        let layout = located_layout(
            data,
            policy,
            &ring.ring_auth_pda(),
            accounts.iter().map(|meta| &meta.pubkey),
        )
        .map_err(|_| MessageMismatch::Accounts)?;
        Ok(Self {
            accounts,
            legs: layout.legs(&data.interface_transfers).collect(),
            layout,
        })
    }

    fn key(&self, index: usize) -> Option<Address> {
        self.accounts.get(index).map(|meta| meta.pubkey)
    }

    fn group(&self, leg: &SettlementLeg) -> &'a [AccountMeta] {
        self.accounts.get(leg.accounts.clone()).unwrap_or_default()
    }
}

impl LegDirection {
    pub(crate) fn of(transfer: InterfaceTransfer) -> Self {
        if transfer.is_deposit() {
            Self::Deposit
        } else {
            Self::Withdrawal
        }
    }
}

impl PublicLeg {
    /// The group SPP reads for the leg, an account group of the other direction is refused.
    pub(crate) fn of(
        direction: LegDirection,
        amount: u64,
        accounts: &TransactInterfaceTransferAccounts,
    ) -> Result<Self, RingError> {
        let (mint, group) = match (direction, accounts) {
            (_, TransactInterfaceTransferAccounts::Sol(sol)) => {
                (None, vec![pda::sol_interface(), sol.recipient])
            }
            (LegDirection::Deposit, TransactInterfaceTransferAccounts::SplDeposit(spl)) => (
                Some(spl.mint),
                vec![
                    spl.mint,
                    spl.spl_interface,
                    spl.token_authority,
                    spl.user_token_account,
                    spl.token_program,
                ],
            ),
            (LegDirection::Withdrawal, TransactInterfaceTransferAccounts::SplWithdrawal(spl)) => (
                Some(spl.mint),
                vec![
                    pda::shielded_pool_cpi_authority(),
                    spl.mint,
                    spl.spl_interface,
                    spl.user_token_account,
                    spl.token_program,
                ],
            ),
            _ => return Err(CustomRingError::InvalidSettlementAccounts.into()),
        };
        Ok(Self {
            direction,
            mint: mint.map(SerializablePubkey),
            amount,
            accounts: group.into_iter().map(SerializablePubkey).collect(),
        })
    }

    /// The mint is read at the kind's own position, a SOL leg names none.
    fn binds(&self, leg: &SettlementLeg, group: &[AccountMeta]) -> bool {
        let mint = leg
            .transfer
            .mint_account_position()
            .and_then(|position| group.get(position))
            .map(|meta| meta.pubkey);
        self.direction == LegDirection::of(leg.transfer)
            && self.amount == leg.transfer.amount()
            && self.mint.map(|mint| mint.0) == mint
            && self
                .accounts
                .iter()
                .map(|account| &account.0)
                .eq(group.iter().map(|meta| &meta.pubkey))
    }
}

/// Each of `legs` equals the term at its place, a leg or a term left over moves funds no terms name.
fn check_legs<'l>(
    terms: &[PublicLeg],
    decoded: &DecodedTransfer<'_>,
    mut legs: impl Iterator<Item = &'l SettlementLeg>,
) -> Result<(), MessageMismatch> {
    let mut terms = terms.iter();
    let bound = legs.all(|leg| {
        terms
            .next()
            .is_some_and(|term| term.binds(leg, decoded.group(leg)))
    });
    if bound && terms.next().is_none() {
        Ok(())
    } else {
        Err(MessageMismatch::Legs)
    }
}

trait ExpectedFrame {
    const TAG: InstructionTag;

    fn rail(&self) -> RingRail;

    fn policy(&self) -> &RingPolicy;

    fn envelope(&self, signer: Address) -> Result<Envelope, MessageMismatch>;

    fn check_frame(
        &self,
        envelope: &Envelope,
        parsed: &ParsedTransact<'_>,
        decoded: &DecodedTransfer<'_>,
    ) -> Result<(), MessageMismatch>;
}

fn check_message<E: ExpectedFrame>(
    expected: &E,
    message: &[u8],
    signer: Address,
) -> Result<VerifiedMessage, MessageMismatch> {
    let envelope = expected.envelope(signer)?;
    let instruction = envelope.open(message)?;
    let parsed = instruction
        .data
        .split_first()
        .filter(|(tag, _)| **tag == E::TAG as u8)
        .and_then(|(_, data)| RingTransactData::parse(data, expected.rail()).ok())
        .ok_or(MessageMismatch::Data)?;
    let decoded = DecodedTransfer::decode(
        envelope.ring,
        expected.policy(),
        parsed.data(),
        &instruction.accounts,
    )?;
    expected.check_frame(&envelope, &parsed, &decoded)?;
    Ok(envelope.verified(message))
}

/// The proof binds each record to its slot under the key the frame publishes, a slot tag is an unproved hint.
pub(crate) fn open_stream(
    parsed: &ParsedTransact<'_>,
    tx_viewing_key: &TxViewingSecret,
    auditor: &X25519Pubkey,
) -> Result<OpenedAudit, MessageMismatch> {
    let data = parsed.data();
    let tx_viewing_key = tx_viewing_key
        .key()
        .map_err(|_| MessageMismatch::TransactionKey)?;
    if *data.tx_viewing_u != tx_viewing_key.pubkey().u() {
        return Err(MessageMismatch::TransactionKey);
    }
    AuditEnvelope {
        eph_u: *data.tx_viewing_u,
        salt: data.salt,
        ciphertext: data.envelope,
    }
    .open_sealed(&tx_viewing_key, auditor, parsed.audit_stream())
    .map_err(|_| MessageMismatch::Data)
}

/// Each slot's record and the key it seals to, a non-exit record leads its body and an exit's sits
/// in the auditor message the frame rebuilds.
fn open_records(
    parsed: &ParsedTransact<'_>,
    tx_viewing_key: &TxViewingSecret,
    opened: &OpenedAudit,
) -> Result<Vec<(OutputRecord, X25519Pubkey)>, MessageMismatch> {
    let data = parsed.data();
    let tx_viewing_key = tx_viewing_key
        .key()
        .map_err(|_| MessageMismatch::TransactionKey)?;
    let keys = opened
        .recipient_keys(data.recipient_keys)
        .map_err(|_| MessageMismatch::Data)?;
    let first_nullifier = data.first_nullifier().map_err(|_| MessageMismatch::Data)?;
    data.outputs
        .iter()
        .zip(keys)
        .zip(0u32..)
        .map(|((output, key), slot)| {
            let secret = DeliveredSlot {
                agreement: &tx_viewing_key
                    .ecdh(&key)
                    .map_err(|_| MessageMismatch::Data)?,
                tx_viewing_pk: tx_viewing_key.pubkey().as_bytes(),
                recipient_viewing_pk: key.as_bytes(),
                salt: data.salt,
                output_index: slot,
                first_nullifier,
            }
            .secret()
            .map_err(|_| MessageMismatch::Data)?;
            let record = open_recipient_body(output.record, &secret, data.record_form())
                .map_err(|_| MessageMismatch::Data)?
                .record;
            Ok((record, key))
        })
        .collect()
}

/// The key signs at `allowed` and is named nowhere else in the frame, any other entry carries an
/// authority its terms never name.
fn check_signer(
    envelope: &Envelope,
    decoded: &DecodedTransfer<'_>,
    allowed: &[usize],
    writable: bool,
) -> Result<(), MessageMismatch> {
    let mut named = decoded
        .accounts
        .iter()
        .enumerate()
        .filter(|(_, meta)| meta.pubkey == envelope.signer);
    let valid = !allowed.is_empty()
        && envelope.reads_only(decoded)
        && named
            .clone()
            .map(|(index, _)| index)
            .eq(allowed.iter().copied())
        && named.all(|(_, meta)| meta.is_signer && (writable || !meta.is_writable));
    if valid {
        Ok(())
    } else {
        Err(MessageMismatch::Accounts)
    }
}

/// One slot alone, a second entry of the key would stand for a signature the message holds once.
fn check_sole_signer(
    envelope: &Envelope,
    decoded: &DecodedTransfer<'_>,
    mut slots: impl Iterator<Item = usize>,
) -> Result<(), MessageMismatch> {
    match (slots.next(), slots.next()) {
        (Some(slot), None) => check_signer(envelope, decoded, &[slot], false),
        _ => Err(MessageMismatch::Accounts),
    }
}

impl ExpectedDelegate {
    pub fn new(requested: &RequestedAuthorityTransfer<'_>) -> Result<Self, RingError> {
        let RequestedAuthorityTransfer {
            request,
            quote,
            config,
            requester,
            output_tree_id,
        } = requested;
        let transfer = &request.transfer;
        match requester {
            DelegateRequester::Ring(allowed) => {
                if config.delegate != Some(request.delegate.0) {
                    return Err(refused_delegate(config));
                }
                allowed.check_sources(transfer.sources.iter().map(|source| &source.0))?;
            }
            DelegateRequester::Mint(mint) => {
                if config.delegate == Some(request.delegate.0) {
                    return Err(RingError::RingDelegateAsMintRequester);
                }
                if sole_mint(transfer.mints()) != Some(*mint) {
                    return Err(CustomRingError::UnauthorizedDelegate.into());
                }
            }
        }
        Ok(Self {
            ring: CustomRing::new(request.ring.0),
            fee_payer: quote.fee_payer.0,
            delegate: request.delegate.0,
            policy: Box::new(*config.account()),
            rail: RingRail::delegate(config.mode),
            auditor: config.auditor_pubkey,
            tx_viewing_key: request.tx_viewing_key.clone(),
            sources: transfer
                .sources
                .iter()
                .map(|source| ExpectedOwner::of(&source.0))
                .collect::<Result<_, RingError>>()?,
            payments: transfer
                .payments
                .iter()
                .map(|payment| (RecordKind::Ring, payment))
                .chain(
                    transfer
                        .exits
                        .iter()
                        .map(|payment| (RecordKind::Exit, payment)),
                )
                .map(|(kind, payment)| {
                    let owner = ExpectedOwner::of(&payment.recipient.0)?;
                    let tag = match kind {
                        RecordKind::Exit => owner.owner_key,
                        _ => payment.tag.as_ref().map_or_else(
                            || bootstrap_slot_tag(owner.viewing_u),
                            |requested| slot_tag(requested.0),
                        ),
                    };
                    Ok(ExpectedPayment {
                        kind,
                        owner,
                        mint: payment.mint.0,
                        amount: payment.amount,
                        tag,
                    })
                })
                .collect::<Result<_, RingError>>()?,
            withdrawals: transfer
                .withdrawals
                .iter()
                .map(withdrawal_leg)
                .collect::<Result<_, _>>()?,
            requester: requester.clone(),
            output_tree_id: *output_tree_id,
        })
    }

    #[cfg(test)]
    pub fn check(&self, message: &[u8]) -> Result<VerifiedMessage, MessageMismatch> {
        check_message(self, message, self.delegate)
    }

    /// A ring output's record publishes no nonzero key, the commitment alone binds the output to
    /// `owner`'s key. The rail carries no data, so both data hashes are zero.
    fn rebuilds(
        &self,
        output: &RingOutput<'_>,
        record: &OutputRecord,
        owner: &ExpectedOwner,
    ) -> bool {
        let ring = (record.kind == RecordKind::Ring).then_some(self.ring.program_id());
        Utxo {
            owner: PublicKey::from_ed25519(&record.owner_key),
            asset: Mint::new(Address::new_from_array(record.mint), 0),
            amount: record.amount,
            blinding: record.blinding,
            ring_program_id: ring,
            data: Default::default(),
        }
        .hash(&owner.nullifier_pk, &[0; 32], &[0; 32], self.output_tree_id)
        .is_ok_and(|hash| hash == *output.hash)
    }

    fn check_moved(
        &self,
        parsed: &ParsedTransact<'_>,
        decoded: &DecodedTransfer<'_>,
    ) -> Result<(), MessageMismatch> {
        let data = parsed.data();
        // 1. The stream opens under the requested keys, and the legs are the requested withdrawals.
        let opened = open_stream(parsed, &self.tx_viewing_key, &self.auditor)?;
        if data.has_data || data.ring_data {
            return Err(MessageMismatch::Data);
        }
        check_legs(&self.withdrawals, decoded, decoded.legs.iter())?;
        // 2. Matching outputs bind no input, every proven spend answers to the requested openings.
        let mut spends = match &self.requester {
            DelegateRequester::Ring(allowed) => Some(
                allowed
                    .check_spends(opened.real_inputs, data.nullifiers)
                    .map_err(|_| MessageMismatch::SpendsUnrequestedNote)?,
            ),
            DelegateRequester::Mint(_) => None,
        };
        // 3. Each live record pays one requested payment or returns change to a requested source,
        // sealed to that owner's viewing key.
        let records = open_records(parsed, &self.tx_viewing_key, &opened)?;
        let anonymous = data.rail().anonymous_frame();
        // An anonymous frame publishes each tag once, a match returns every slot of the transaction.
        let published: Vec<ViewTag> = data
            .outputs
            .iter()
            .filter(|output| !output.is_exit())
            .map(|output| output.owner_tag)
            .collect();
        let mut unpaid = self.payments.clone();
        for (output, (record, key)) in data.outputs.iter().zip(records) {
            if record.kind == RecordKind::Dummy {
                continue;
            }
            let sealed_to = |owner: &ExpectedOwner| {
                (record.kind == RecordKind::Exit || key.u() == owner.viewing_u)
                    && self.rebuilds(output, &record, owner)
            };
            // A payment no published tag finds is lost to its recipient's sync.
            let paid = unpaid.iter().position(|payment| {
                let tagged = if anonymous && payment.kind == RecordKind::Ring {
                    published.contains(&payment.tag)
                } else {
                    payment.tag == output.owner_tag
                };
                payment.matches(&record) && tagged && sealed_to(&payment.owner)
            });
            if let Some(index) = paid {
                unpaid.swap_remove(index);
                continue;
            }
            let source = self
                .sources
                .iter()
                .find(|source| {
                    record.kind == RecordKind::Ring && source.owns(&record) && sealed_to(source)
                })
                .ok_or(MessageMismatch::Recipients)?;
            if let Some(spends) = &mut spends {
                spends.debit(
                    SourceMint {
                        owner_key: source.owner_key,
                        nullifier_pk: source.nullifier_pk,
                        mint: record.mint,
                    },
                    record.amount,
                );
            }
            // An anonymous source's change publishes no tag of its own, a note that source spent finds it.
            if anonymous {
                continue;
            }
            // A confidential source finds its change by the tag.
            if output.owner_tag != bootstrap_slot_tag(source.viewing_u) {
                return Err(MessageMismatch::Recipients);
            }
        }
        if !unpaid.is_empty() {
            return Err(MessageMismatch::Recipients);
        }
        // 4. A mint's permanent delegate holds no notes to weigh the change against.
        spends.as_ref().map_or(Ok(()), SourceSpends::check_change)
    }
}

impl ExpectedFrame for ExpectedDelegate {
    const TAG: InstructionTag = InstructionTag::AuthorityTransfer;

    fn rail(&self) -> RingRail {
        self.rail
    }

    fn policy(&self) -> &RingPolicy {
        &self.policy
    }

    fn envelope(&self, signer: Address) -> Result<Envelope, MessageMismatch> {
        if self.delegate != signer {
            return Err(MessageMismatch::Unrequested);
        }
        Ok(Envelope {
            ring: self.ring,
            program: self.ring.program_id(),
            units: compute_unit_limit(Self::TAG),
            fee_payer: self.fee_payer,
            signer,
            setup: Vec::new(),
        })
    }

    fn check_frame(
        &self,
        envelope: &Envelope,
        parsed: &ParsedTransact<'_>,
        decoded: &DecodedTransfer<'_>,
    ) -> Result<(), MessageMismatch> {
        self.check_moved(parsed, decoded)?;
        check_sole_signer(envelope, decoded, decoded.layout.delegate.into_iter())
    }
}

/// A public owner's approval, its own notes are a required input since its signature is the only gate on them.
#[must_use]
pub struct OwnerApproval<'a> {
    pub relay_id: RelayId,
    pub message: Vec<u8>,
    pub terms: RelayTransferTerms,
    pub config: &'a CustomRingConfig,
    /// Every note the wallet holds, the nullifiers are the wallet's own derivation.
    pub known_notes: &'a [WalletUtxo],
}

impl OwnerApproval<'_> {
    pub fn pending(self) -> PendingSignature {
        PendingSignature::expecting(
            self.relay_id,
            self.message,
            ExpectedMessage::PublicOwner(ExpectedPublicOwner::new(
                self.terms,
                self.config,
                self.known_notes.iter().map(|note| note.nullifier).collect(),
            )),
        )
    }
}

/// A funder's approval, its signature moves the legs it funds and nothing else of its.
#[must_use]
pub struct FunderApproval<'a> {
    pub relay_id: RelayId,
    pub message: Vec<u8>,
    pub terms: RelayFunderTerms,
    pub config: &'a CustomRingConfig,
}

impl FunderApproval<'_> {
    pub fn pending(self) -> PendingSignature {
        PendingSignature::expecting(
            self.relay_id,
            self.message,
            ExpectedMessage::Funder(ExpectedFunder {
                setup: setup_instructions(self.terms.fee_payer.0, &self.terms.setup),
                policy: Box::new(*self.config.account()),
                rail: RingRail::transfer(self.config.mode),
                terms: self.terms,
            }),
        )
    }
}

fn setup_instructions(fee_payer: Address, setup: &[TokenAccountCreate]) -> Vec<Instruction> {
    setup
        .iter()
        .map(|create| {
            CreateAssociatedTokenAccount {
                payer: fee_payer,
                owner: create.owner.0,
                mint: create.mint.0,
                token_program: create.token_program.0,
            }
            .instruction()
        })
        .collect()
}

impl ExpectedPublicOwner {
    pub fn new(
        terms: RelayTransferTerms,
        config: &CustomRingConfig,
        known_nullifiers: Vec<[u8; 32]>,
    ) -> Self {
        Self {
            setup: setup_instructions(terms.fee_payer.0, &terms.setup),
            policy: Box::new(*config.account()),
            rail: RingRail::transfer(config.mode),
            known_nullifiers,
            terms,
        }
    }

    /// SPP reads one signature as approval of every data-bearing output its signer owns, and each
    /// published owner tag of a ring output is zero, so no frame field names those outputs. The
    /// signer reads every published output before it signs.
    fn check_moved(
        &self,
        data: &RingTransactData<'_>,
        decoded: &DecodedTransfer<'_>,
    ) -> Result<(), MessageMismatch> {
        let terms = &self.terms;
        // 1. One opening per published slot in slot order, a missing or repeated slot could hide an
        // output the signature approves.
        let in_order = terms.slots.len() == data.outputs.len()
            && terms
                .slots
                .iter()
                .zip(0u32..)
                .all(|(opening, slot)| opening.slot == slot);
        if !in_order {
            return Err(MessageMismatch::UnopenedOutput);
        }
        // 2. Each opening rebuilds the commitment published at its slot, so the signer reads each
        // owner, amount and data hash the proof commits.
        for (opening, published) in terms.slots.iter().zip(&data.outputs) {
            // An exit commits no ring, a ring id here would refuse every exit.
            let rebuilt = opening.utxo_hash((!published.is_exit()).then_some(terms.ring.0))?;
            let stated = SlotCommitment {
                hash: Hash(rebuilt),
                tag: opening.tag.clone(),
            };
            if !stated.publishes(published) {
                return Err(MessageMismatch::Recipients);
            }
        }
        // 3. A named note the wallet cannot place means no list stood behind the check.
        let named_and_held = terms.nullifiers.iter().all(|nullifier| {
            data.nullifiers.contains(&nullifier.0) && self.known_nullifiers.contains(&nullifier.0)
        });
        if !named_and_held {
            return Err(MessageMismatch::Data);
        }
        let spends_another = data.nullifiers.iter().any(|spent| {
            self.known_nullifiers.contains(spent)
                && !terms.nullifiers.iter().any(|named| named.0 == *spent)
        });
        if spends_another {
            return Err(MessageMismatch::SpendsUnnamedNote);
        }
        // 4. The frame moves public funds through the legs the terms name and no other.
        check_legs(&terms.legs, decoded, decoded.legs.iter())
    }
}

impl SlotOpening {
    /// A zero owner opens a dummy, SPP pays no zero owner and holds a dummy's other fields to zero.
    fn utxo_hash(&self, ring: Option<Address>) -> Result<[u8; 32], MessageMismatch> {
        if self.owner.0 == Address::default() {
            return self.dummy_hash();
        }
        Utxo {
            owner: PublicKey::from_ed25519(self.owner.0.as_array()),
            asset: Mint::new(self.mint.0, 0),
            amount: self.amount,
            blinding: self.blinding.0,
            ring_program_id: ring,
            data: Default::default(),
        }
        .hash(
            &self.nullifier_pk.0,
            &self.data_hash.0,
            &self.ring_data_hash.0,
            self.tree_id,
        )
        .map_err(|_| MessageMismatch::Recipients)
    }

    fn dummy_hash(&self) -> Result<[u8; 32], MessageMismatch> {
        if self.mint.0 != Address::default()
            || self.amount != 0
            || [&self.nullifier_pk, &self.data_hash, &self.ring_data_hash]
                .iter()
                .any(|field| field.0 != [0; 32])
        {
            return Err(MessageMismatch::Recipients);
        }
        ProofInputUtxo {
            domain: right_align(&DUMMY_DOMAIN.to_be_bytes()),
            tree_id: tree_id_field(self.tree_id),
            blinding: self.blinding.0,
            ..ProofInputUtxo::default()
        }
        .hash()
        .map_err(|_| MessageMismatch::Recipients)
    }
}

impl SlotCommitment {
    /// SPP binds an exit's tag to its owner, any other slot publishes the stated tag.
    fn publishes(&self, output: &RingOutput<'_>) -> bool {
        self.hash.0 == *output.hash
            && (output.is_exit() || self.tag.as_ref().map(|tag| tag.0) == Some(output.owner_tag))
    }
}

impl ExpectedFrame for ExpectedPublicOwner {
    const TAG: InstructionTag = InstructionTag::Transfer;

    fn rail(&self) -> RingRail {
        self.rail
    }

    fn policy(&self) -> &RingPolicy {
        &self.policy
    }

    /// Another key stays refused.
    fn envelope(&self, signer: Address) -> Result<Envelope, MessageMismatch> {
        if self.terms.signer.0 != signer
            || (self.terms.slots.is_empty() && self.terms.nullifiers.is_empty())
        {
            return Err(MessageMismatch::Unrequested);
        }
        Ok(Envelope {
            ring: CustomRing::new(self.terms.ring.0),
            program: self.terms.ring.0,
            units: compute_unit_limit(Self::TAG),
            fee_payer: self.terms.fee_payer.0,
            signer,
            setup: self.setup.clone(),
        })
    }

    fn check_frame(
        &self,
        envelope: &Envelope,
        parsed: &ParsedTransact<'_>,
        decoded: &DecodedTransfer<'_>,
    ) -> Result<(), MessageMismatch> {
        self.check_moved(parsed.data(), decoded)?;
        let run = decoded
            .layout
            .owners
            .clone()
            .filter(|&index| decoded.key(index) == Some(envelope.signer));
        check_sole_signer(envelope, decoded, run)
    }
}

impl ExpectedFrame for ExpectedFunder {
    const TAG: InstructionTag = InstructionTag::Transfer;

    fn rail(&self) -> RingRail {
        self.rail
    }

    fn policy(&self) -> &RingPolicy {
        &self.policy
    }

    fn envelope(&self, signer: Address) -> Result<Envelope, MessageMismatch> {
        if self.terms.signer.0 != signer || self.terms.legs.is_empty() {
            return Err(MessageMismatch::Unrequested);
        }
        Ok(Envelope {
            ring: CustomRing::new(self.terms.ring.0),
            program: self.terms.ring.0,
            units: compute_unit_limit(Self::TAG),
            fee_payer: self.terms.fee_payer.0,
            signer,
            setup: self.setup.clone(),
        })
    }

    /// A relayed deposit proves no owner signature, the funder reads every output and leg before it
    /// signs. The signature authorizes every position naming the key, so the check holds both sides.
    fn check_frame(
        &self,
        envelope: &Envelope,
        parsed: &ParsedTransact<'_>,
        decoded: &DecodedTransfer<'_>,
    ) -> Result<(), MessageMismatch> {
        let terms = &self.terms;
        let outputs = &parsed.data().outputs;
        // 1. Each published output is the one the sealing wallet stated at its slot.
        if terms.slots.len() != outputs.len() {
            return Err(MessageMismatch::UnopenedOutput);
        }
        if !terms
            .slots
            .iter()
            .zip(outputs)
            .all(|(slot, output)| slot.publishes(output))
        {
            return Err(MessageMismatch::Recipients);
        }
        // 2. The frame moves public funds through the legs the terms name and no other, a
        // withdrawal has no funder to refuse it.
        check_legs(&terms.legs, decoded, decoded.legs.iter())?;
        // 3. Past the funder slots of its legs the key appears nowhere. A run entry would authorize
        // its data outputs and spent notes under one signature, the sharper authority the terms
        // never name.
        let funded: Vec<&SettlementLeg> = decoded
            .legs
            .iter()
            .filter(|leg| {
                leg.funder
                    .is_some_and(|funder| decoded.key(funder) == Some(envelope.signer))
            })
            .collect();
        let slots: Vec<usize> = funded.iter().filter_map(|leg| leg.funder).collect();
        let sol_funded = funded.iter().any(|leg| !leg.transfer.is_spl());
        check_signer(envelope, decoded, &slots, sol_funded)
    }
}

impl ApprovalOpening {
    pub fn of(totals: &ApprovalTotals, blinder: &[u8; 32]) -> Self {
        Self {
            blinder: Hash(*blinder),
            outputs: totals
                .outputs
                .iter()
                .map(|slot| OutputTotalsOpening {
                    asset: Hash(slot.asset),
                    peak: slot.peak.to_string(),
                    charged_outflow: slot.charged_outflow.to_string(),
                })
                .collect(),
            public: totals
                .public
                .iter()
                .map(|slot| PublicTotalsOpening {
                    asset: Hash(slot.asset),
                    peak: slot.peak.to_string(),
                    outflow: slot.outflow.to_string(),
                    gross: slot.gross.to_string(),
                })
                .collect(),
        }
    }

    fn totals(&self) -> Option<ApprovalTotals> {
        Some(ApprovalTotals {
            outputs: self
                .outputs
                .iter()
                .map(|slot| {
                    Some(OutputTotals {
                        asset: slot.asset.0,
                        peak: slot.peak.parse().ok()?,
                        charged_outflow: slot.charged_outflow.parse().ok()?,
                    })
                })
                .collect::<Option<_>>()?,
            public: self
                .public
                .iter()
                .map(|slot| {
                    Some(PublicTotals {
                        asset: slot.asset.0,
                        peak: slot.peak.parse().ok()?,
                        outflow: slot.outflow.parse().ok()?,
                        gross: slot.gross.parse().ok()?,
                    })
                })
                .collect::<Option<_>>()?,
        })
    }
}

impl CoSignerApproval<'_> {
    pub fn pending(self) -> PendingSignature {
        PendingSignature::expecting(
            self.relay_id,
            self.message,
            ExpectedMessage::CoSigner(ExpectedCoSigner {
                ring: self.ring,
                fee_payer: self.fee_payer,
                signer: self.signer,
                policy: Box::new(*self.config.account()),
                rail: RingRail::transfer(self.config.mode),
                setup: setup_instructions(self.fee_payer, &self.setup),
                rule: self.rule,
            }),
        )
    }
}

impl ExactApproval {
    /// The digest binds the totals, the limits then read them with no slot opened. A ring with no
    /// own registry runs no outflow section and its circuit refuses a nonzero digest, so such a frame
    /// reads as `MissingApprovalDigest` and exact approval stays out of reach there, the proven bits
    /// remain the only rule a co-signer can apply to it.
    fn check(&self, digest: Option<&[u8; 32]>, capacity: usize) -> Result<(), MessageMismatch> {
        let digest = digest.ok_or(MessageMismatch::MissingApprovalDigest)?;
        // 1. The chain hashes a flat run of three fields per output and four per public slot with no
        // length in it, so `4a` more output rows and `3a` fewer public rows rebuild the same digest
        // with every column shifted. Only the key's exact counts make the limits below read a peak
        // as a peak, and they hold ahead of hashing since the rows come from the sealing wallet.
        if self.opening.outputs.len() != capacity || self.opening.public.len() != N_PUBLIC_SLOTS {
            return Err(MessageMismatch::ApprovalOpeningLayout);
        }
        // 2. The circuit accepts any blinder, and a zero one lets anyone test guessed totals against
        // the public digest. Freshness and hiding rest on the reader's refusal alone.
        let blinder = &self.opening.blinder;
        if blinder.0 == [0; 32] {
            return Err(MessageMismatch::ZeroApprovalBlinder);
        }
        if self.seen_blinders.contains(blinder) {
            return Err(MessageMismatch::ReusedApprovalBlinder);
        }
        let totals = self
            .opening
            .totals()
            .ok_or(MessageMismatch::FalseApprovalOpening)?;
        let opens = totals
            .commitment(&blinder.0)
            .is_ok_and(|commitment| commitment == *digest);
        if !opens {
            return Err(MessageMismatch::FalseApprovalOpening);
        }
        let named = totals
            .outputs
            .iter()
            .map(|slot| slot.asset)
            .chain(totals.public.iter().map(|slot| slot.asset))
            .filter(|asset| *asset != [0; 32]);
        for asset in named {
            let limit = self
                .limits
                .iter()
                .find(|limit| {
                    Member::asset(&limit.mint).is_ok_and(|field| *field.as_bytes() == asset)
                })
                .ok_or(MessageMismatch::ApprovalOverLimit)?;
            // A deposit the sender funds offsets its own withdrawal in peak and outflow, a public
            // row's gross alone shows what leaves through it.
            let over_gross = totals
                .public
                .iter()
                .any(|slot| slot.asset == asset && slot.gross > limit.gross);
            if over_gross
                || totals.peak_of(&asset) > limit.peak
                || totals.outflow_of(&asset) > limit.outflow
            {
                return Err(MessageMismatch::ApprovalOverLimit);
            }
        }
        Ok(())
    }
}

impl ExpectedFrame for ExpectedCoSigner {
    const TAG: InstructionTag = InstructionTag::Transfer;

    fn rail(&self) -> RingRail {
        self.rail
    }

    fn policy(&self) -> &RingPolicy {
        &self.policy
    }

    fn envelope(&self, signer: Address) -> Result<Envelope, MessageMismatch> {
        if self.signer != signer {
            return Err(MessageMismatch::Unrequested);
        }
        Ok(Envelope {
            ring: CustomRing::new(self.ring),
            program: self.ring,
            units: compute_unit_limit(Self::TAG),
            fee_payer: self.fee_payer,
            signer,
            setup: self.setup.clone(),
        })
    }

    fn check_frame(
        &self,
        envelope: &Envelope,
        parsed: &ParsedTransact<'_>,
        decoded: &DecodedTransfer<'_>,
    ) -> Result<(), MessageMismatch> {
        let data = parsed.data();
        match &self.rule {
            CoSignerRule::Threshold { accepts } => {
                if data.approval.bits() & !accepts.bits() != 0 {
                    return Err(MessageMismatch::ApprovalMaskUnaccepted);
                }
            }
            CoSignerRule::Exact(exact) => exact.check(
                data.extensions.approval_totals,
                parsed.audit_stream().capacity,
            )?,
        }
        // The key signs the pair of its own record, a slot beside another record approves for it.
        let own = envelope.ring.co_signer_metas(envelope.signer);
        let slots = decoded
            .layout
            .co_signer_pairs()
            .filter(|(record, signer)| decoded.accounts.get(*record..=*signer) == Some(&own[..]))
            .map(|(_, signer)| signer);
        check_sole_signer(envelope, decoded, slots)
    }
}

impl ExpectedOwner {
    fn of(address: &ShieldedAddress) -> Result<Self, RingError> {
        Ok(Self {
            owner_key: address.signing_pubkey.confidential_view_tag()?,
            nullifier_pk: address.nullifier_pubkey,
            viewing_u: address.viewing_pubkey.u(),
        })
    }

    /// A ring output's nonzero key stays unpublished, its record binds the zero key bit alone.
    fn owns(&self, record: &OutputRecord) -> bool {
        record.owner_key == self.owner_key
            && record.key == record_key(record.kind, self.nullifier_pk)
    }
}

impl ExpectedPayment {
    fn matches(&self, record: &OutputRecord) -> bool {
        self.kind == record.kind
            && self.owner.owns(record)
            && record.mint == self.mint.to_bytes()
            && record.amount == self.amount
    }
}

fn withdrawal_leg(withdrawal: &AuthorityWithdrawal) -> Result<PublicLeg, RingError> {
    let mint = withdrawal.mint.0;
    let recipient = withdrawal.recipient.0;
    let accounts = match (mint == SOL_MINT, withdrawal.token_program) {
        (true, None) => {
            TransactInterfaceTransferAccounts::Sol(TransactSolTransferAccounts { recipient })
        }
        (true, Some(program)) => {
            return Err(TransactionError::UnexpectedSolTokenProgram {
                token_program: program.0,
            }
            .into())
        }
        (false, None) => return Err(TransactionError::MissingSplTokenProgram { mint }.into()),
        (false, Some(program)) => {
            TransactInterfaceTransferAccounts::SplWithdrawal(TransactSplWithdrawalAccounts {
                mint,
                spl_interface: pda::spl_interface(&mint),
                user_token_account: pda::associated_token_address_with_program(
                    &recipient, &mint, &program.0,
                ),
                token_program: program.0,
            })
        }
    };
    PublicLeg::of(LegDirection::Withdrawal, withdrawal.amount, &accounts)
}

impl VerifiedMessage {
    pub fn signer(&self) -> Address {
        self.signer
    }

    pub fn sign(&self, signer: &dyn Signer) -> Result<Signature, RingError> {
        if signer.try_pubkey()? != self.signer {
            return Err(MessageMismatch::Signer.into());
        }
        Ok(signer.try_sign_message(&self.bytes)?)
    }
}

impl Envelope {
    /// One canonical v1 message the quoted fee payer pays, holding only the ring instruction.
    fn open(&self, bytes: &[u8]) -> Result<Instruction, MessageMismatch> {
        let message = match wincode::deserialize(bytes) {
            Ok(VersionedMessage::V1(message)) => message,
            Ok(VersionedMessage::Legacy(_) | VersionedMessage::V0(_)) => {
                return Err(MessageMismatch::NotV1)
            }
            Err(_) => return Err(MessageMismatch::Undecodable),
        };
        if message.validate().is_err() || message.serialize() != bytes {
            return Err(MessageMismatch::Undecodable);
        }
        if message.fee_payer() != Some(&self.fee_payer) || self.fee_payer == self.signer {
            return Err(MessageMismatch::FeePayer);
        }
        let config = message.config;
        let within_budget = config
            .compute_unit_limit
            .is_some_and(|units| units <= self.units)
            && config
                .loaded_accounts_data_size_limit
                .is_some_and(|size| size <= MAX_LOADED_ACCOUNTS_DATA_SIZE)
            && config.heap_size.is_none();
        if !within_budget {
            return Err(MessageMismatch::Budget);
        }
        let (compiled, setup) = message
            .instructions
            .split_last()
            .ok_or(MessageMismatch::Instructions)?;
        let key = |index: u8| {
            message
                .account_keys
                .get(usize::from(index))
                .copied()
                .ok_or(MessageMismatch::Undecodable)
        };
        // An omitted create costs the signer nothing, a transfer to a missing account only fails.
        let mut named = self.setup.iter();
        let mut created_atas = Vec::with_capacity(setup.len());
        for compiled in setup {
            let program = key(compiled.program_id_index)?;
            let accounts = compiled
                .accounts
                .iter()
                .map(|&index| key(index))
                .collect::<Result<Vec<_>, _>>()?;
            let matched = named.find(|expected| {
                program == expected.program_id
                    && compiled.data == expected.data
                    && accounts
                        .iter()
                        .eq(expected.accounts.iter().map(|meta| &meta.pubkey))
            });
            let Some(expected) = matched else {
                return Err(MessageMismatch::Instructions);
            };
            let ata = expected
                .accounts
                .get(1)
                .ok_or(MessageMismatch::Instructions)?
                .pubkey;
            created_atas.push(ata);
        }
        if key(compiled.program_id_index)? != self.program {
            return Err(MessageMismatch::Program);
        }
        let accounts = compiled
            .accounts
            .iter()
            .map(|&index| {
                Ok(AccountMeta {
                    pubkey: key(index)?,
                    is_signer: message.is_signer(usize::from(index)),
                    is_writable: requests_write(&message, usize::from(index)),
                })
            })
            .collect::<Result<Vec<_>, MessageMismatch>>()?;
        // Outside the ring frame only the fee payer signs and only a named create's ATA is written.
        for (index, &pubkey) in message.account_keys.iter().enumerate() {
            let in_ring = compiled
                .accounts
                .iter()
                .any(|&account_index| usize::from(account_index) == index);
            if in_ring || pubkey == self.fee_payer {
                continue;
            }
            if message.is_signer(index)
                || (requests_write(&message, index) && !created_atas.contains(&pubkey))
            {
                return Err(MessageMismatch::Accounts);
            }
        }
        Ok(Instruction {
            program_id: self.program,
            accounts,
            data: compiled.data.clone(),
        })
    }

    /// The ring account first, then accounts the program only reads, a message holds one write
    /// role per address across prefix and forwarded.
    fn reads_only(&self, decoded: &DecodedTransfer<'_>) -> bool {
        let (prefix, forwarded) = decoded
            .accounts
            .split_at_checked(decoded.layout.spp)
            .unwrap_or_default();
        let forwarded_write = |meta: &AccountMeta| {
            forwarded
                .iter()
                .any(|spp| spp.pubkey == meta.pubkey && spp.is_writable)
        };
        match prefix.split_first() {
            Some((ring, rest)) => {
                *ring == AccountMeta::new_readonly(self.ring.policy_pda(), false)
                    && rest
                        .iter()
                        .all(|meta| !meta.is_writable || forwarded_write(meta))
            }
            None => false,
        }
    }

    fn verified(&self, bytes: &[u8]) -> VerifiedMessage {
        VerifiedMessage {
            bytes: bytes.to_vec(),
            signer: self.signer,
        }
    }
}

/// The header's write role, the one `compile_message` assigns.
fn requests_write(message: &v1::Message, index: usize) -> bool {
    let header = message.header;
    let signers = usize::from(header.num_required_signatures);
    if index < signers {
        message.is_signer_writable(index)
    } else {
        index
            < message
                .account_keys
                .len()
                .saturating_sub(usize::from(header.num_readonly_unsigned_accounts))
    }
}

#[cfg(test)]
mod tests {
    use solana_message::Hash as Blockhash;
    use spr_client::{compile_message, ComputeBudgetConfig};
    use zolana_ring_keys::REDACTED;

    use super::*;

    const FEE_PAYER: Address = Address::new_from_array([1; 32]);
    const SIGNER: Address = Address::new_from_array([2; 32]);
    const RING: Address = Address::new_from_array([3; 32]);
    const UNITS: u32 = 100_000;

    fn create(owner: u8) -> Instruction {
        CreateAssociatedTokenAccount {
            payer: FEE_PAYER,
            owner: Address::new_from_array([owner; 32]),
            mint: Address::new_from_array([4; 32]),
            token_program: Address::new_from_array([5; 32]),
        }
        .instruction()
    }

    fn opened(setup: Vec<Instruction>) -> Result<Instruction, MessageMismatch> {
        let ring_instruction = Instruction {
            program_id: RING,
            accounts: vec![AccountMeta::new_readonly(SIGNER, true)],
            data: vec![9],
        };
        let instructions: Vec<_> = setup.into_iter().chain([ring_instruction]).collect();
        let message = compile_message(
            &FEE_PAYER,
            &instructions,
            Blockhash::new_from_array([6; 32]),
            ComputeBudgetConfig::new(UNITS),
        )
        .unwrap();
        Envelope {
            ring: CustomRing::new(RING),
            program: RING,
            units: UNITS,
            fee_payer: FEE_PAYER,
            signer: SIGNER,
            setup: vec![create(7), create(8)],
        }
        .open(&message.serialize())
    }

    #[test]
    fn a_message_may_omit_a_named_create_and_nothing_else_changes() {
        assert!(opened(vec![create(7), create(8)]).is_ok());
        assert!(opened(vec![create(8)]).is_ok());
        assert!(opened(Vec::new()).is_ok());
        for refused in [
            vec![create(8), create(7)],
            vec![create(7), create(7)],
            vec![create(9)],
            vec![create(7), create(8), create(8)],
        ] {
            assert_eq!(opened(refused), Err(MessageMismatch::Instructions));
        }
    }

    #[test]
    fn approval_opening_debug_redacts_blinder() {
        let blinder = Hash([0xab; 32]);
        let opening = ApprovalOpening {
            blinder: blinder.clone(),
            outputs: Vec::new(),
            public: Vec::new(),
        };
        let rendered = format!("{opening:?}");
        assert!(rendered.contains(REDACTED), "{rendered}");
        assert!(!rendered.contains(&format!("{blinder:?}")), "{rendered}");
        let seen = Hash([0xcd; 32]);
        let exact = ExactApproval {
            opening,
            limits: Vec::new(),
            seen_blinders: vec![seen.clone()],
        };
        let rendered = format!("{exact:?}");
        assert!(rendered.contains(REDACTED), "{rendered}");
        for secret in [blinder, seen] {
            assert!(!rendered.contains(&format!("{secret:?}")), "{rendered}");
        }
    }

    #[test]
    fn setup_only_accounts_refuse_signers_and_unmatched_writes() {
        let ring_instruction = Instruction {
            program_id: RING,
            accounts: vec![AccountMeta::new_readonly(SIGNER, true)],
            data: vec![9],
        };
        let check = |instructions: Vec<Instruction>| {
            let message = compile_message(
                &FEE_PAYER,
                &instructions,
                Blockhash::new_from_array([6; 32]),
                ComputeBudgetConfig::new(UNITS),
            )
            .unwrap();
            Envelope {
                ring: CustomRing::new(RING),
                program: RING,
                units: UNITS,
                fee_payer: FEE_PAYER,
                signer: SIGNER,
                setup: vec![create(7), create(8)],
            }
            .open(&message.serialize())
        };

        let mut writable_owner = create(7);
        writable_owner.accounts[2].is_writable = true;
        assert_eq!(
            check(vec![writable_owner, ring_instruction.clone()]),
            Err(MessageMismatch::Accounts)
        );

        let mut signer_owner = create(7);
        signer_owner.accounts[2].is_signer = true;
        assert_eq!(
            check(vec![signer_owner, ring_instruction.clone()]),
            Err(MessageMismatch::Accounts)
        );

        let mut signer_ata = create(7);
        signer_ata.accounts[1].is_signer = true;
        assert_eq!(
            check(vec![signer_ata, ring_instruction.clone()]),
            Err(MessageMismatch::Accounts)
        );

        // Omitting create(7) disarms write permission for create(7)'s ATA.
        let omitted_ata = create(7).accounts[1].pubkey;
        let base_msg = compile_message(
            &FEE_PAYER,
            &[create(8), ring_instruction],
            Blockhash::new_from_array([6; 32]),
            ComputeBudgetConfig::new(UNITS),
        )
        .unwrap();
        let VersionedMessage::V1(mut smuggled) = base_msg else {
            panic!("v1 message");
        };
        let insert_at = usize::from(smuggled.header.num_required_signatures);
        smuggled.account_keys.insert(insert_at, omitted_ata);
        for compiled in &mut smuggled.instructions {
            if usize::from(compiled.program_id_index) >= insert_at {
                compiled.program_id_index += 1;
            }
            for index in &mut compiled.accounts {
                if usize::from(*index) >= insert_at {
                    *index += 1;
                }
            }
        }
        assert!(smuggled.validate().is_ok());
        assert_eq!(
            Envelope {
                ring: CustomRing::new(RING),
                program: RING,
                units: UNITS,
                fee_payer: FEE_PAYER,
                signer: SIGNER,
                setup: vec![create(7), create(8)],
            }
            .open(&smuggled.serialize()),
            Err(MessageMismatch::Accounts)
        );
    }
}
