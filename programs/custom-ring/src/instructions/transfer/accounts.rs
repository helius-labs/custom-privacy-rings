//! Places the accounts of a transfer and reads every statement value that only an account holds.
//!
//! SPP reads its list by position, and the ring program prepends its own slots. A value read from
//! the wrong slot would verify the ring proof against accounts SPP does not settle.
//!
//! `TransferLayout` in `program-libs/interface/src/instructions/transfer_layout.rs` names the account windows.
//!
//! On return these facts hold:
//!
//! 1. Each root comes from the registry the ring pins, at the revision the frame names.
//! 2. `ring_auth` sits only in SPP's `ring_config` slot, and the payer signs.
//! 3. A signed record covers every class the ring configured and the operation demands, and every proven approval bit.
//! 4. Every funder of a deposit leg signs, and a delegate frame has no deposit leg.
//! 5. On the anonymous frame the co-signers fill the party cells.
//!
//! The ring proof finishes the co-signer check. `hiddenOwner` in `prover/circuits/transfer/transfer.go` holds
//! the hidden owner apart from each nonzero party cell and from the owner run, so no hidden owner approves
//! its own outflow. The payer, a funder or the exit may name the hidden owner, the wallet decides. SPP
//! validates each settlement account.

use custom_ring_interface::{
    anonymous_party_identities, cosign::CoSignRequirement, CustomRingError, ParsedTransact,
    PublicSlots, RailAuthorization, RingPolicy, RootRead, SignerSet, TransferLayout,
    TransferPrefix, TransferRoots, ANONYMOUS_PARTIES,
};
use pinocchio::{error::ProgramError, AccountView, Address};
use spr_hasher::primitives::solana_owner_identity;

use crate::instructions::{
    instruction_runtime::{PlacedRingAuth, RingAuth},
    loader::{
        check_funding_signer, load_mint_delegate_asset, CoSigners, RegistrySlots, RingRegistries,
        TransferCall,
    },
    public_legs::public_slots,
};

pub(super) struct TransactAccounts {
    pub roots: TransferRoots,
    /// The in-proof owner's identity, zero otherwise.
    pub sender: [u8; 32],
    pub delegate_asset: [u8; 32],
    /// SPP's owner signer run, default on the authority rails.
    pub signers: SignerSet,
    /// The co-signers' identities, zeros off the anonymous frame.
    pub party_identities: [[u8; 32]; ANONYMOUS_PARTIES],
    pub public_slots: PublicSlots,
    /// Empty without own records and on the authority rails, no rule screens them.
    pub withdrawals: Vec<custom_ring_interface::PolicyWithdrawal>,
    /// Index of the first forwarded SPP account.
    pub spp: usize,
    pub ring_auth: PlacedRingAuth,
}

#[must_use]
pub(super) struct TransactCheck<'a> {
    pub program_id: &'a Address,
    pub ring: &'a RingPolicy,
    pub transfer: &'a ParsedTransact<'a>,
    /// From the tag, an approval digests these bytes.
    pub instruction_data: &'a [u8],
    pub slot: u64,
}

/// The prefix windows as the live accounts fix them.
struct LivePrefix<'a> {
    call: TransferCall<'a>,
    ring: &'a RingPolicy,
    read: RootRead,
    registries: Option<RingRegistries<'a>>,
    co_signers: CoSigners,
    delegate_asset: [u8; 32],
}

impl TransferPrefix for LivePrefix<'_> {
    type Error = ProgramError;

    fn co_signer_pairs(
        &mut self,
        first: usize,
        requirement: Option<CoSignRequirement>,
    ) -> Result<usize, ProgramError> {
        // An unbound registry refuses ahead of every window past it.
        let mut pinned = self
            .call
            .accounts
            .get(TransferLayout::RING + 1..first)
            .unwrap_or_default()
            .iter();
        let slots = RegistrySlots {
            ring: self.ring,
            read: self.read,
        };
        self.registries =
            Some(slots.load(|| pinned.next().ok_or(ProgramError::NotEnoughAccountKeys))?);
        self.co_signers = CoSigners::verify(&self.call, first, requirement)?;
        Ok(self.co_signers.as_slice().len())
    }

    fn delegate_mint(&mut self, signer: usize) -> Result<bool, ProgramError> {
        let accounts = self.call.accounts;
        let delegate_signer = accounts
            .get(signer)
            .filter(|signer| signer.is_signer())
            .ok_or(CustomRingError::UnauthorizedDelegate)?;
        if self.ring.delegate() == Some(delegate_signer.address()) {
            return Ok(false);
        }
        self.delegate_asset = load_mint_delegate_asset(
            accounts
                .get(signer + 1)
                .ok_or(ProgramError::NotEnoughAccountKeys)?,
            delegate_signer.address(),
        )
        .map_err(|error| match self.ring.delegate() {
            None => CustomRingError::DelegateDisabled,
            Some(_) => error,
        })?;
        Ok(true)
    }
}

impl TransactCheck<'_> {
    #[inline(never)]
    pub fn validate(self, accounts: &[AccountView]) -> Result<TransactAccounts, ProgramError> {
        let (ring, data) = (self.ring, self.transfer.data());
        let delegate_rail = data.rail().is_delegate();
        // 1. The frame names only revisions, the registries the ring pins serve each root. A delegate
        // reads no policy root, a restrictive update cannot fail its proof.
        let read = RootRead::transfer(ring, delegate_rail);
        // 2. The legs and the frame's spend and approval bits set the co-sign demand, step 5 of the run
        // proves those bits. The pairs and the delegate slot size the ring's prefix, every window
        // below reads from the one layout SPP's list follows.
        let mut prefix = LivePrefix {
            call: TransferCall {
                program_id: self.program_id,
                ring: accounts
                    .first()
                    .ok_or(ProgramError::NotEnoughAccountKeys)?
                    .address(),
                data: self.instruction_data,
                accounts,
            },
            ring,
            read,
            registries: None,
            co_signers: CoSigners::default(),
            delegate_asset: [0; 32],
        };
        let layout = TransferLayout::new(data, ring, accounts.len(), &mut prefix)?;
        let LivePrefix {
            registries: Some(registries),
            co_signers,
            delegate_asset,
            ..
        } = prefix
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        let bound = data.roots(ring, &registries.roots(self.slot))?;
        // 3. The settlement groups end SPP's list ahead of the cache accounts, SPP reads the same window.
        // The public slots, the funders, the withdrawals and the exit all read from it.
        let window = &accounts[layout.settlement.clone()];
        let public_slots = public_slots(&data.interface_transfers, window)?;
        // 4. `ring_auth` signs the forwarded call, its one slot keeps that signature on `ring_config`.
        let ring_auth = RingAuth::derive(self.program_id)?
            .check_slot(&accounts[layout.spp..], layout.ring_config() - layout.spp)?;
        check_funding_signer(accounts, layout.payer())?;
        let slots = self.transfer.audit_stream().key.signer_slots();
        // 5. A deposit leg would bring an unlisted asset in on the delegate rail.
        if delegate_rail
            && data
                .interface_transfers
                .iter()
                .any(|transfer| transfer.is_deposit())
        {
            return Err(CustomRingError::DelegatePublicLeg.into());
        }
        // 6. A funder signs for its own deposit leg, and SPP moves only that leg's amount under the
        // signature.
        for funder in layout
            .legs(&data.interface_transfers)
            .filter_map(|leg| leg.funder)
        {
            check_funding_signer(accounts, funder)?;
        }
        let owners = &accounts[layout.owners.clone()];
        // 7. Each rail names who authorizes the spend, and the statement binds that identity.
        let (sender, signers) = match data.authorization {
            RailAuthorization::Delegate | RailAuthorization::AnonymousDelegate => {
                // SPP's authority rail admits no owner signer, the delegate slot is the move's one
                // authorization beside governance's rail flag in `RingConfig`.
                if !owners.is_empty() {
                    return Err(CustomRingError::InvalidSettlementAccounts.into());
                }
                ([0; 32], SignerSet::default())
            }
            owner_signed => {
                check_owner_run(owners, &co_signers)?;
                let sender = match owner_signed {
                    // The in-proof owner signs inside SPP's anonymous rail proof, the frame names its key.
                    RailAuthorization::ConfidentialInProof(owner) => {
                        let owner = Address::new_from_array(owner.owner);
                        if co_signers.contains(&owner) {
                            return Err(CustomRingError::CoSignerIsOwner.into());
                        }
                        solana_owner_identity(owner.as_array()).map_err(CustomRingError::from)?
                    }
                    // The proof holds each published owner's live input to the run, an empty run spends nothing.
                    RailAuthorization::ConfidentialSigner | RailAuthorization::AnonymousPublic
                        if !data.no_spend && owners.is_empty() =>
                    {
                        return Err(CustomRingError::OwnerSignerCount.into());
                    }
                    _ => [0; 32],
                };
                (sender, signer_set(owners, slots)?)
            }
        };

        // 8. The rules screen each withdrawal's owner and mint as the live accounts hold them, the frame's
        // owner list must match each token account. The statement binds the result as `withdrawal_hash`.
        let withdrawals = match read {
            RootRead::None => Vec::new(),
            RootRead::Own | RootRead::Policy => custom_ring_interface::policy_withdrawals(
                &data.interface_transfers,
                &window
                    .iter()
                    .map(crate::instructions::loader::LoadedSettlement)
                    .collect::<Vec<_>>(),
                data.spl_withdrawal_owners,
            )?,
        };
        // 9. The co-signers hold the party cells, a zero cell reads as absent.
        let party_identities = if data.rail().anonymous_frame() {
            anonymous_party_identities(co_signers.as_slice())?
        } else {
            [[0; 32]; ANONYMOUS_PARTIES]
        };

        Ok(TransactAccounts {
            roots: bound,
            sender,
            delegate_asset,
            signers,
            party_identities,
            public_slots,
            withdrawals,
            spp: layout.spp,
            ring_auth,
        })
    }
}

/// Every entry signs and SPP counts its run up to the first non-signer. A signing settlement account
/// would lengthen SPP's count and fail its exact settlement length, so both programs read one run.
fn check_owner_run(owners: &[AccountView], co_signers: &CoSigners) -> Result<(), CustomRingError> {
    if owners.iter().any(|owner| !owner.is_signer()) {
        return Err(CustomRingError::MissingOwnerSigner);
    }
    // An owner approving its own spend would add no second party.
    if owners
        .iter()
        .any(|owner| co_signers.contains(owner.address()))
    {
        return Err(CustomRingError::CoSignerIsOwner);
    }
    Ok(())
}

fn signer_set(owners: &[AccountView], slots: usize) -> Result<SignerSet, CustomRingError> {
    SignerSet::new(owners.iter().map(|owner| owner.address().as_array()), slots)
}
