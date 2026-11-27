//! The hidden owner signs inside the AnonymousRingEddsa proof.

use custom_ring_interface::{
    anonymous_exit::{TokenAccountControl, TokenAccountState},
    key_catalog::{RecordPools, TransferSpend},
    RingMode, RingRail,
};
use solana_account::Account;
use solana_address::Address;
use spr_client::{AsyncRpc, Shape};
use spr_interface::{pda, SPL_TOKEN_2022_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID};
use spr_keypair::{viewing_key::ViewTag, NullifierKey, PublicKey, ShieldedAddress, SigningKey};
use spr_transaction::{
    instructions::transact::{
        ConfidentialTransaction, SettlementTransfer, SppProofInputs, SppProofOutputUtxo,
    },
    Mint, WalletUtxo,
};
use zolana_ring_keys::{RingAddress, RingPaymentRequest};
use zolana_ring_transaction::{AnonymousPlan, ClassifiedInput, ReplyMetadata, RingDelivery};

use super::{
    canonical_shape,
    owner::{CustomRingTransferInput, Owner},
    public_owners::check_zero_key_inputs,
    sealed,
    selection::{payment_total, widest, Candidate, Funding},
    spends_default_ring, transfer_key, CoOwnerKey, OutputSeal, OwnerSource, Rail, RailKind,
    RingSender, RingTransfer, SealedOutputs, Spend, Staged, TransferAuthority,
};
use crate::{
    client::{RingClient, RingIndexerBackend},
    rpc::SubjectListSalt,
    viewing_history::TransferTags,
    AnonymousExitDestination, CustomRing, CustomRingConfig, RingError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnonymousPayment {
    pub to: RingAddress,
    pub asset: Mint,
    pub amount: u64,
    /// An unauthenticated claim to its recipient, [`Self::with_reply`] opts in.
    pub reply: Option<ReplyMetadata>,
    pub data: NoteData,
    /// The tag of the request it pays, the recipient's bootstrap tag without one.
    pub request_tag: Option<ViewTag>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RingPayment {
    pub to: RingAddress,
    pub amount: u64,
    /// An unauthenticated claim to its recipient, [`Self::with_reply`] opts in.
    pub reply: Option<ReplyMetadata>,
    pub data: NoteData,
    /// The tag of the request it pays, the recipient's bootstrap tag without one.
    pub request_tag: Option<ViewTag>,
}

/// What a paid note commits beside its amount, the recipient's body carries it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteData {
    pub ring_data_hash: Option<[u8; 32]>,
    /// SPP holds a nonzero hash to an owner who signs, the hidden owner or a public signer.
    pub application: Option<ApplicationData>,
}

/// SPP commits `hash` unchecked, a reader recomputes it from `bytes`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationData {
    pub hash: [u8; 32],
    pub bytes: Vec<u8>,
}

impl AnonymousPayment {
    #[must_use = "use the updated payment"]
    pub fn with_reply(mut self, reply: ReplyMetadata) -> Self {
        self.reply = Some(reply);
        self
    }

    #[must_use = "use the updated payment"]
    pub fn with_data(mut self, data: NoteData) -> Self {
        self.data = data;
        self
    }

    /// A payment to the tags' own key takes the sender tag, `None` reads as the recipient's bootstrap tag.
    pub(crate) fn tag(&self, tags: Option<&TransferTags>) -> Option<ViewTag> {
        tags.filter(|tags| tags.viewing_pubkey() == self.to.recipient.viewing_pubkey)
            .map(TransferTags::sender)
            .or(self.request_tag)
    }

    pub(super) fn add_to(
        &self,
        transaction: &mut ConfidentialTransaction,
    ) -> Result<(), RingError> {
        if self.to.recipient.signing_pubkey.is_zero() {
            return Err(spr_transaction::TransactionError::ZeroRecipientOwner.into());
        }
        let mut output = SppProofOutputUtxo {
            owner_address: Some(self.to.recipient),
            asset: self.asset,
            amount: self.amount,
            ring_program_id: Some(self.to.ring_program_id),
            ring_data_hash: self.data.ring_data_hash,
            ..Default::default()
        };
        if let Some(application) = &self.data.application {
            output = output.with_utxo_data(application.bytes.clone(), application.hash);
        }
        transaction.add_output_utxo(output)?;
        Ok(())
    }
}

/// A public co-owner, it signs the transaction and one prover holds its secret.
#[derive(Clone, Copy)]
pub struct CoOwner<'a> {
    pub owner: Address,
    pub nullifier_key: &'a NullifierKey,
}

impl RingPayment {
    pub fn new(to: RingAddress, amount: u64) -> Self {
        Self {
            to,
            amount,
            reply: None,
            data: NoteData::default(),
            request_tag: None,
        }
    }

    #[must_use = "use the updated payment"]
    pub fn with_reply(mut self, reply: ReplyMetadata) -> Self {
        self.reply = Some(reply);
        self
    }

    #[must_use = "use the updated payment"]
    pub fn with_data(mut self, data: NoteData) -> Self {
        self.data = data;
        self
    }

    /// The request's amount when it names one.
    pub fn to_request(request: &RingPaymentRequest, amount: u64) -> Self {
        Self {
            to: request.address.clone(),
            amount: request.amount.unwrap_or(amount),
            data: NoteData::default(),
            reply: None,
            request_tag: request.tag,
        }
    }

    pub fn in_asset(&self, asset: Mint) -> AnonymousPayment {
        AnonymousPayment {
            to: self.to.clone(),
            asset,
            amount: self.amount,
            reply: self.reply.clone(),
            data: self.data.clone(),
            request_tag: self.request_tag,
        }
    }
}

#[must_use = "select the inputs of the move"]
pub struct AnonymousMove<'a> {
    pub ring: CustomRing,
    pub sender: &'a dyn RingSender,
    pub nullifier_key: &'a NullifierKey,
    pub signer: &'a SigningKey,
    pub payer: Address,
    /// The sender's UTXOs on any tree.
    pub utxos: &'a [WalletUtxo],
    pub asset: Mint,
    /// The change returns to the sender.
    pub payments: Vec<RingPayment>,
    pub entry: EntrySpend,
    /// Read only when the ring demands a co-signer.
    pub co_signer: Option<Address>,
    /// The wallet's sender tag. Change takes random bytes without it, and an entry spend is refused.
    pub tags: Option<&'a TransferTags>,
}

/// A default-ring spend publishes the hidden owner's key, a move takes one only when allowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntrySpend {
    Refuse,
    Allow,
}

/// What a public party or a zero-key spend may reveal about the hidden owner.
///
/// The proof refuses the hidden owner only in the published owner run. A payer, funder, extra
/// signer, exit destination or zero-key spend of the owner settles and names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OwnerExposure {
    #[default]
    Refused,
    Accepted,
}

impl OwnerExposure {
    pub(super) const fn of(owner_exposed: bool) -> Self {
        if owner_exposed {
            Self::Accepted
        } else {
            Self::Refused
        }
    }

    pub(super) fn guard(self, owner: HiddenOwner) -> Option<HiddenOwner> {
        match self {
            Self::Refused => Some(owner),
            Self::Accepted => None,
        }
    }
}

/// The owner's spends on an anonymous ring, no transaction signer names the owner by default.
pub struct Anonymous<'a> {
    pub(super) owner: Owner<'a>,
    pub(super) signer: OwnerSource<'a>,
    pub(super) exposure: OwnerExposure,
    /// In declared order, the order of their ring outputs.
    pub(super) payments: Vec<AnonymousPayment>,
    tags: Option<TransferTags>,
    pub(super) exit_token_owner: Option<Address>,
    co_owners: Vec<CoOwner<'a>>,
    read_cache: Option<ReadCache>,
}

/// Notes SPP reads from a cache account instead of proving their inclusion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadCache {
    pub cache: Address,
    pub notes: Vec<CachedNote>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CachedNote {
    pub nullifier: [u8; 32],
    pub slot: u8,
}

pub type AnonymousTransfer<'a> = RingTransfer<Anonymous<'a>>;

impl sealed::Sealed for Anonymous<'_> {}

impl Rail for Anonymous<'_> {
    fn kind(&self) -> RailKind<'_> {
        RailKind::Anonymous(self)
    }
}

impl<'a> RingTransfer<Owner<'a>> {
    /// The hidden Ed25519 owner an anonymous ring proves, it must be the sender's signing key.
    pub fn with_anonymous_signer(self, signer: &'a SigningKey) -> RingTransfer<Anonymous<'a>> {
        self.with_anonymous_owner(OwnerSource::Ready(signer.into()))
    }

    pub(crate) fn with_anonymous_owner(
        self,
        signer: OwnerSource<'a>,
    ) -> RingTransfer<Anonymous<'a>> {
        RingTransfer {
            ring: self.ring,
            co_signers: self.co_signers,
            priority_fee: self.priority_fee,
            rail: Anonymous {
                owner: self.rail,
                signer,
                exposure: OwnerExposure::Refused,
                payments: Vec::new(),
                tags: None,
                exit_token_owner: None,
                co_owners: Vec::new(),
                read_cache: None,
            },
            list_salts: self.list_salts,
            ring_salt: self.ring_salt,
            approval: self.approval,
            seal_row: self.seal_row,
            program: self.program,
        }
    }
}

impl<'a> RingTransfer<Anonymous<'a>> {
    /// A public co-owner of later inputs, it signs the transaction and its nullifier key stays with the prover.
    #[must_use = "use the updated transfer"]
    pub fn with_co_owner(mut self, co_owner: CoOwner<'a>) -> Self {
        self.rail.co_owners.push(co_owner);
        self
    }

    /// Named notes spend from `cache`, SPP checks each slot against the frame's selector.
    #[must_use = "use the updated transfer"]
    pub fn with_read_cache(mut self, read_cache: ReadCache) -> Self {
        self.rail.read_cache = Some(read_cache);
        self
    }

    /// Every ring output owner other than the sender is paid here, before the transaction is padded.
    pub fn with_payment(mut self, payment: AnonymousPayment) -> Result<Self, RingError> {
        check_payment_ring(&payment, self.ring)?;
        payment.add_to(&mut self.rail.owner.transaction)?;
        self.rail.payments.push(payment);
        Ok(self)
    }

    /// The wallet's sender tag, the first slot sealed to its key publishes it.
    #[must_use = "use the updated transfer"]
    pub fn with_tags(mut self, tags: TransferTags) -> Self {
        self.rail.tags = Some(tags);
        self
    }

    /// Unset, staging pads to the smallest supported shape.
    pub fn with_shape(mut self, shape: Shape) -> Result<Self, RingError> {
        if !shape.is_supported() {
            return Err(spr_transaction::TransactionError::UnsupportedShape {
                n_in: shape.n_inputs(),
                n_out: shape.n_outputs(),
            }
            .into());
        }
        self.rail.owner.shape = Some(shape);
        Ok(self)
    }

    /// Admits the parties and zero-key spends [`OwnerExposure`] names, a co-signer stays refused.
    #[must_use = "use the updated transfer"]
    pub fn with_owner_exposure(mut self, exposure: OwnerExposure) -> Self {
        self.rail.exposure = exposure;
        self
    }

    /// A missing exit token account must be the associated token account of `owner`.
    #[must_use = "use the updated transfer"]
    pub fn with_exit_token_owner(mut self, owner: Address) -> Self {
        self.rail.exit_token_owner = Some(owner);
        self
    }

    pub(crate) fn check_public_signers(
        &self,
        signers: impl IntoIterator<Item = Address>,
    ) -> Result<(), RingError> {
        let owner = HiddenOwner::of(&self.rail.owner.sender.address()?.signing_pubkey)?;
        match self.rail.exposure.guard(owner) {
            Some(owner) => owner.check_signers(signers),
            None => Ok(()),
        }
    }
}

impl<'a> AnonymousMove<'a> {
    /// As [`super::DelegateMove::select`], ring UTXOs before an entry spend.
    pub async fn select<P: AsyncRpc, I: RingIndexerBackend>(
        self,
        client: &RingClient<P, I>,
    ) -> Result<AnonymousTransfer<'a>, RingError> {
        let amount = payment_total(self.payments.iter().map(|payment| payment.amount))?;
        let payments = self.payments.len();
        let owner = self.sender.address()?.signing_pubkey;
        let ring = Some(self.ring.program_id());
        let widest_spend = |key: AnonymousKey| widest(payments + 1, |shape| key.serves(shape));
        let max_inputs = widest_spend(AnonymousKey::Commitment).ok_or(RingError::ShapeUnkeyed {
            rail: RingRail::Anonymous,
        })?;
        let funding = Funding {
            utxos: self.utxos,
            asset: self.asset.asset,
            amount,
            max_inputs,
        };
        let mut attempts = vec![funding
            .select(|utxo| utxo.utxo.owner == owner && utxo.utxo.ring_program_id == ring)
            .and_then(|candidate| self.attempt(AnonymousKey::Commitment, &candidate))];
        if let EntrySpend::Allow = self.entry {
            if let Some(max_inputs) = widest_spend(AnonymousKey::Entry) {
                let entry = Funding {
                    max_inputs,
                    ..funding
                }
                .select(|utxo| utxo.utxo.owner == owner && utxo.is_default_ring_spendable());
                attempts.push(
                    entry.and_then(|candidate| self.attempt(AnonymousKey::Entry, &candidate)),
                );
            }
        }
        funding.first_fit(attempts, client).await
    }

    fn attempt(
        &self,
        key: AnonymousKey,
        candidate: &Candidate,
    ) -> Result<AnonymousTransfer<'a>, RingError> {
        let shape = self.shape(key, candidate).ok_or(RingError::ShapeUnkeyed {
            rail: RingRail::Anonymous,
        })?;
        let transaction = ConfidentialTransaction::new_with_ring(
            candidate.utxos.clone(),
            self.payer,
            self.ring.program_id(),
        )?;
        let transfer = RingTransfer {
            co_signers: self.co_signer.into_iter().collect(),
            ..RingTransfer::<Owner<'a>>::new(CustomRingTransferInput {
                ring: self.ring,
                sender: self.sender,
                nullifier_key: Some(self.nullifier_key),
                transaction,
            })
        }
        .with_anonymous_signer(self.signer);
        let transfer = match self.tags {
            Some(tags) => transfer.with_tags(tags.clone()),
            None => transfer,
        };
        self.payments
            .iter()
            .try_fold(transfer, |transfer, payment| {
                transfer.with_payment(payment.in_asset(self.asset))
            })?
            .with_shape(shape)
    }

    /// Output 0 is always the sender change, a zero one when nothing returns.
    fn shape(&self, key: AnonymousKey, candidate: &Candidate) -> Option<Shape> {
        let sender_change = usize::from(!candidate.change);
        candidate.shape(self.payments.len() + sender_change, |shape| {
            key.serves(shape)
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AnonymousKey {
    Commitment,
    Entry,
}

impl AnonymousKey {
    pub fn serves(self, shape: Shape) -> bool {
        super::keyed(RingRail::Anonymous, shape, self == Self::Entry)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HiddenOwner(pub Address);

pub(crate) struct AnonymousSpend<'a> {
    pub token_owner: Option<Address>,
    pub signer: OwnerSource<'a>,
}

impl HiddenOwner {
    pub(crate) fn of(key: &PublicKey) -> Result<Self, RingError> {
        Ok(Self(Address::new_from_array(key.as_ed25519()?)))
    }

    pub(crate) fn check_signers(
        self,
        signers: impl IntoIterator<Item = Address>,
    ) -> Result<(), RingError> {
        if signers.into_iter().any(|signer| signer == self.0) {
            Err(RingError::AnonymousPublicSignerLinksOwner)
        } else {
            Ok(())
        }
    }
}

impl Anonymous<'_> {
    pub(super) async fn stage<R: AsyncRpc + ?Sized>(
        &self,
        client: &R,
        config: &CustomRingConfig,
    ) -> Result<Staged<'_>, RingError> {
        if config.mode != RingMode::Anonymous {
            return Err(RingError::NotAnonymousRing);
        }
        let keys = self.owner.sender;
        let sender = keys.address()?;
        if self.signer.public_key() != sender.signing_pubkey {
            return Err(RingError::AnonymousSignerMismatch);
        }
        let guarded = self
            .exposure
            .guard(HiddenOwner::of(&sender.signing_pubkey)?);
        self.owner.check_settlement_accounts()?;
        if let Some(owner) = guarded {
            owner.check_signers([self.owner.transaction.payer()])?;
            self.owner.check_deposit_funders(owner)?;
        }
        let transfers = self.owner.transaction.interface_transfers()?;
        let exits = AnonymousExits {
            transfers: &transfers,
            hidden_owner: guarded.map(|owner| owner.0),
            named_owner: self.exit_token_owner,
        }
        .owners(client)
        .await?;
        let transaction = self.owner.transaction.clone();
        let shape = match self.owner.shape {
            Some(shape) => shape,
            None => canonical_shape(&transaction, &sender)?,
        };
        let SealedOutputs { rail, proof_inputs } = OutputSeal {
            keys,
            sender: &sender,
            auditor: config.auditor_pubkey,
            delivery: RingDelivery::Anonymous(Box::new(
                TaggedPayments {
                    transaction: &transaction,
                    sender: &sender,
                    payments: &self.payments,
                    tags: self.tags.as_ref(),
                }
                .plan()?,
            )),
            shape,
            program_salt: None,
        }
        .seal(transaction)?;
        let proof_inputs = self.cached(proof_inputs)?;
        check_anonymous_inputs(&proof_inputs, &sender, self.exposure)?;
        let owner_signers = public_co_owners(&proof_inputs, &sender)?;
        // A signer owning no input authorizes its data output alone, it spends with no key, and an address slot spends nothing.
        let spends = |owner: &&Address| {
            proof_inputs.input_utxos.iter().any(|input| {
                input.is_note()
                    && input.utxo.owner.confidential_view_tag().ok() == Some(owner.to_bytes())
            })
        };
        let co_owners = owner_signers
            .iter()
            .filter(spends)
            .map(|owner| {
                let held = self
                    .co_owners
                    .iter()
                    .find(|co_owner| co_owner.owner == *owner)
                    .ok_or(RingError::MissingCoOwnerKey { owner: *owner })?;
                CoOwnerKey::of(held)
            })
            .collect::<Result<_, RingError>>()?;
        Ok(Staged {
            proof_inputs,
            rail,
            sender,
            authority: TransferAuthority {
                owner: self.owner.owner_key,
                co_owners,
            },
            owner_signers,
            interface_transfer_accounts: self.owner.interface_transfer_accounts.clone(),
            spend: Spend::Anonymous(Box::new(AnonymousSpend {
                token_owner: exits.token_owner,
                signer: self.signer,
            })),
            presealed: None,
            list_salts: Vec::new(),
        })
    }
}

impl Anonymous<'_> {
    /// After sealing, a slot's cache source changes no blinding and no output.
    fn cached(&self, mut inputs: SppProofInputs) -> Result<SppProofInputs, RingError> {
        let Some(read_cache) = &self.read_cache else {
            return Ok(inputs);
        };
        for note in &read_cache.notes {
            let input = inputs
                .input_utxos
                .iter_mut()
                .find(|input| input.nullifier() == note.nullifier)
                .ok_or(RingError::CachedInputUnknown)?;
            *input = input.clone().with_cache_slot(note.slot)?;
        }
        Ok(inputs.with_read_cache(read_cache.cache))
    }
}

pub(super) fn check_payment_ring(
    payment: &AnonymousPayment,
    ring: CustomRing,
) -> Result<(), RingError> {
    if payment.to.ring_program_id == ring.program_id() {
        Ok(())
    } else {
        Err(RingError::ForeignRingAddress {
            address: payment.to.ring_program_id,
            ring: ring.program_id(),
        })
    }
}

pub(super) fn payment_salts(
    payments: &[AnonymousPayment],
) -> Result<Vec<SubjectListSalt>, RingError> {
    payments
        .iter()
        .map(|payment| {
            Ok(SubjectListSalt {
                member: payment
                    .to
                    .recipient
                    .signing_pubkey
                    .owner_proof_input_hash()?,
                list_salt: payment.to.list_salt,
            })
        })
        .collect()
}

pub(super) struct TaggedPayments<'a> {
    pub transaction: &'a ConfidentialTransaction,
    pub sender: &'a ShieldedAddress,
    pub payments: &'a [AnonymousPayment],
    pub tags: Option<&'a TransferTags>,
}

impl TaggedPayments<'_> {
    /// Without the wallet's sender tag change takes random bytes, the ring note the wallet spends
    /// leads its ring sync to the change.
    pub fn plan(self) -> Result<AnonymousPlan, RingError> {
        let Self {
            transaction,
            sender,
            payments,
            tags,
        } = self;
        // 1. An entry spends the sender's default-ring notes and no ring note of its own. A core
        // scan that reads their spend ahead of the ring sync hands the transaction to no later sync,
        // and a co-owner's ring note leads only its own sync, so only the sender tag finds the
        // change. A deposit spends a dummy and returns no change.
        let mut own_notes = transaction
            .inputs()
            .iter()
            .filter(|input| input.is_note() && input.utxo.owner == sender.signing_pubkey)
            .peekable();
        let entry = own_notes.peek().is_some()
            && own_notes.all(|input| input.utxo.ring_program_id.is_none());
        // 2. Change takes the sender tag only under the key the tags derive from, a deposit's
        // fresh sender holds another key and its change takes a random tag.
        let sender_tags = tags.filter(|tags| tags.viewing_pubkey() == sender.viewing_pubkey);
        if entry && sender_tags.is_none() {
            return Err(RingError::SenderTagRequired);
        }
        let plan = match sender_tags {
            Some(tags) => AnonymousPlan::new().with_sender_tag(tags.sender()),
            None => AnonymousPlan::new(),
        };
        // 3. The send call alone attaches a reply, no default path does.
        Ok(payments.iter().fold(plan, |plan, payment| {
            let plan = plan.with_payment(payment.to.recipient.viewing_pubkey);
            let plan = match payment.tag(tags) {
                Some(tag) => plan.with_tag(tag),
                None => plan,
            };
            match &payment.reply {
                Some(reply) => plan.with_reply(reply.clone()),
                None => plan,
            }
        }))
    }
}

/// The hidden owner's note is ring input 0, a frame of only public owners takes the confidential
/// family.
pub(super) fn check_anonymous_inputs(
    inputs: &SppProofInputs,
    sender: &ShieldedAddress,
    exposure: OwnerExposure,
) -> Result<(), RingError> {
    let hidden_leads = inputs
        .input_utxos
        .iter()
        .find(|input| input.is_note())
        .is_some_and(|input| input.utxo.owner == sender.signing_pubkey);
    if !hidden_leads {
        return Err(RingError::HiddenOwnerMissing);
    }
    // A cached write needs a public writer signing beside the hidden owner, and no party cell or
    // signer class names one. A cached read and an address slot name no signer. The statement binds
    // an address through the private transaction hash, and SPP reads a cached note under the selector.
    if inputs
        .output_utxos
        .iter()
        .any(|output| output.cache_slot.is_some())
    {
        return Err(RingError::InvalidAnonymousSlot);
    }
    if exposure == OwnerExposure::Refused {
        check_zero_key_inputs(&inputs.input_utxos, &sender.signing_pubkey)?;
    }
    check_entry_inputs(inputs, sender)?;
    check_audit_key(inputs)
}

/// The owner run past the hidden owner, the owners of its later inputs in first-occurrence order.
pub(super) fn public_co_owners(
    inputs: &SppProofInputs,
    sender: &ShieldedAddress,
) -> Result<Vec<Address>, RingError> {
    let hidden = HiddenOwner::of(&sender.signing_pubkey)?.0;
    Ok(inputs
        .owner_signer_pubkeys()?
        .into_iter()
        .filter(|owner| *owner != hidden)
        .collect())
}

/// A default-ring spend past the entry class must split, any other spend needs a keyed shape.
fn check_audit_key(inputs: &SppProofInputs) -> Result<(), RingError> {
    let default_ring = spends_default_ring(&inputs.input_utxos);
    let published = inputs
        .input_utxos
        .iter()
        .filter(|input| !input.is_compact())
        .count();
    let keyed = transfer_key(TransferSpend {
        rail: RingRail::Anonymous,
        inputs: published,
        outputs: inputs.output_utxos.len(),
        spends_default_ring: default_ring,
        legs: 0,
        owners: 0,
        pools: RecordPools::default(),
        wide: false,
    })
    .is_some();
    match (keyed, default_ring) {
        (true, _) => Ok(()),
        (false, true) => Err(RingError::DefaultRingSpendTooWide { inputs: published }),
        (false, false) => Err(RingError::ShapeUnkeyed {
            rail: RingRail::Anonymous,
        }),
    }
}

/// SPP publishes a hidden owner spending its default-ring note, an address slot carries no ring and spends none.
fn check_entry_inputs(inputs: &SppProofInputs, sender: &ShieldedAddress) -> Result<(), RingError> {
    let live = || {
        inputs
            .input_utxos
            .iter()
            .enumerate()
            .filter(|(_, input)| input.is_note() && input.utxo.owner == sender.signing_pubkey)
    };
    let holds_default = live().any(|(_, input)| input.utxo.ring_program_id.is_none());
    match live().find(|(_, input)| holds_default && input.utxo.ring_program_id.is_some()) {
        Some((index, _)) => Err(RingError::EntryRingInput { index }),
        None => Ok(()),
    }
}

pub(super) struct AnonymousExits<'a> {
    pub transfers: &'a [SettlementTransfer],
    /// `None` once the owner accepts exposure, no exit is then checked against it.
    pub hidden_owner: Option<Address>,
    pub named_owner: Option<Address>,
}

pub(super) struct ExitOwners {
    pub token_owner: Option<Address>,
}

struct SplExit {
    mint: Address,
    token_account: Address,
}

impl AnonymousExits<'_> {
    /// Each exit destination reads apart, a missing token account takes the named owner.
    pub(super) async fn owners<R: AsyncRpc + ?Sized>(
        self,
        client: &R,
    ) -> Result<ExitOwners, RingError> {
        self.check_exits(client).await?;
        Ok(ExitOwners {
            token_owner: self.named_owner,
        })
    }

    async fn check_exits<R: AsyncRpc + ?Sized>(&self, client: &R) -> Result<(), RingError> {
        let withdrawals = self
            .transfers
            .iter()
            .filter(|transfer| !transfer.is_deposit());
        if self.named_owner.is_some()
            && !withdrawals
                .clone()
                .any(|transfer| matches!(transfer, SettlementTransfer::Spl { .. }))
        {
            return Err(RingError::AnonymousExitTokenOwnerWithoutSplExit);
        }
        for transfer in withdrawals {
            match *transfer {
                SettlementTransfer::Sol {
                    user_sol_account: recipient,
                    ..
                } => self.check_sol_exit(client, recipient).await?,
                SettlementTransfer::Spl {
                    mint,
                    user_spl_token: token_account,
                    ..
                } => {
                    let owner = self
                        .spl_exit_owner(
                            client,
                            SplExit {
                                mint,
                                token_account,
                            },
                        )
                        .await?;
                    if let Some(named) = self.named_owner.filter(|named| *named != owner) {
                        return Err(RingError::AnonymousExitNamedOwnerMismatch {
                            token_account,
                            named,
                            owner,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    async fn check_sol_exit<R: AsyncRpc + ?Sized>(
        &self,
        client: &R,
        recipient: Address,
    ) -> Result<(), RingError> {
        if self.hidden_owner == Some(recipient) {
            return Err(RingError::AnonymousExitLinksOwner(recipient));
        }
        // SPP reads a missing account as a system account without data.
        let Some(account) = client.get_account(recipient).await? else {
            return Ok(());
        };
        AnonymousExitDestination::Sol {
            recipient: &recipient,
            owner: &account.owner,
            data_len: account.data.len(),
        }
        .named_key()?;
        Ok(())
    }

    async fn spl_exit_owner<R: AsyncRpc + ?Sized>(
        &self,
        client: &R,
        exit: SplExit,
    ) -> Result<Address, RingError> {
        let SplExit {
            mint,
            token_account,
        } = exit;
        let invalid = || RingError::AnonymousExitTokenAccountInvalid(token_account);
        let (control, token_program) = match client.get_account(token_account).await? {
            Some(account) => (
                token_account_state(&account, &mint)
                    .ok_or_else(invalid)?
                    .control,
                account.owner,
            ),
            // The associated token program creates it with the named owner and no other authority.
            None => {
                let owner = self.named_owner.ok_or_else(invalid)?;
                let token_program = client
                    .get_account(mint)
                    .await?
                    .map(|mint| mint.owner)
                    .filter(is_token_program)
                    .ok_or_else(invalid)?;
                if pda::associated_token_address_with_program(&owner, &mint, &token_program)
                    != token_account
                {
                    return Err(invalid());
                }
                (
                    TokenAccountControl::SoleOwner(owner.to_bytes()),
                    token_program,
                )
            }
        };
        let owner = Address::new_from_array(*control.owner());
        if let Some(hidden) = self.hidden_owner {
            // An associated token account stays derivable from its first owner.
            let associated =
                pda::associated_token_address_with_program(&hidden, &mint, &token_program);
            if [token_account, owner].contains(&hidden) || associated == token_account {
                return Err(RingError::AnonymousExitLinksOwner(token_account));
            }
        }
        AnonymousExitDestination::Spl {
            token_account: &token_account,
            control: &control,
        }
        .named_key()?;
        Ok(owner)
    }
}

fn token_account_state(account: &Account, mint: &Address) -> Option<TokenAccountState> {
    if !is_token_program(&account.owner) {
        return None;
    }
    TokenAccountState::unpack(&account.data)
        .ok()
        .filter(|state| state.mint == mint.to_bytes())
}

fn is_token_program(program: &Address) -> bool {
    [SPL_TOKEN_PROGRAM_ID, SPL_TOKEN_2022_PROGRAM_ID].contains(program.as_array())
}
