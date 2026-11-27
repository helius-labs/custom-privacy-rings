//! The windows of a `Transfer` or `AuthorityTransfer` account list, one map for the program, its
//! callers and every reader.
//!
//! SPP reads its list by position, and the ring program prepends its own slots. A reader that cut a
//! window at another index would check or sign accounts the program does not read there.
//!
//! The list in order.
//!
//! 1. The ring account.
//! 2. The registries `RootRead` names, own then curator then tracking.
//! 3. One `[record, signer]` or `[record, instructions_sysvar]` pair per co-signer the demand needs, SPP does not receive them.
//! 4. The delegate's signer on the authority rails, then the mint when its permanent delegate signs, SPP receives neither.
//! 5. SPP's list forwarded whole, payer and `ring_config` in its prefix, then input trees, nullifier PDAs, owner run, settlement groups, cache accounts.
//!
//! `test-vectors/proof/transfer-layout.json` pins the windows, `sdk-libs/client/tests/transfer_layout.rs` writes it.

use core::ops::Range;

use spr_interface::instruction::InterfaceTransfer;

use crate::{
    cosign::CoSignRequirement, funder_account_position, CustomRingError, RingPolicy,
    RingTransactData, RootRead, RING_TRANSACT_ACCOUNT_PREFIX_LEN, RING_TRANSACT_RING_CONFIG_INDEX,
    TRANSACT_PAYER_INDEX,
};

/// The two prefix windows no frame byte sizes.
pub trait TransferPrefix {
    type Error: From<CustomRingError>;

    /// The pairs from index `first` whose records cover `requirement`.
    fn co_signer_pairs(
        &mut self,
        first: usize,
        requirement: Option<CoSignRequirement>,
    ) -> Result<usize, Self::Error>;

    /// A signer at `signer` other than the ring's delegate names its mint in the next slot.
    fn delegate_mint(&mut self, signer: usize) -> Result<bool, Self::Error>;
}

/// Indices into the whole list, the ring account at [`TransferLayout::RING`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransferLayout {
    pub registries: Range<usize>,
    /// `[record, signer]` pairs.
    pub co_signers: Range<usize>,
    pub delegate: Option<usize>,
    pub mint: Option<usize>,
    /// The first account SPP receives.
    pub spp: usize,
    pub trees: Range<usize>,
    pub nullifiers: Range<usize>,
    pub owners: Range<usize>,
    pub settlement: Range<usize>,
    pub cache: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementLeg {
    pub transfer: InterfaceTransfer,
    pub accounts: Range<usize>,
    /// The signer a deposit moves funds from.
    pub funder: Option<usize>,
}

impl SettlementLeg {
    pub fn asset_and_user<'a, T>(&self, frame_accounts: &'a [T]) -> Option<[&'a T; 2]> {
        match (self.transfer, frame_accounts.get(self.accounts.clone())?) {
            (
                InterfaceTransfer::SolDeposit { .. } | InterfaceTransfer::SolWithdrawal { .. },
                [asset, user],
            )
            | (InterfaceTransfer::SplDeposit { .. }, [asset, _, _, user, _])
            | (InterfaceTransfer::SplWithdrawal { .. }, [_, asset, _, user, _]) => {
                Some([asset, user])
            }
            _ => None,
        }
    }
}

impl TransferLayout {
    pub const RING: usize = 0;

    /// `accounts` is the list's length, a list too short for the frame's counts is refused.
    pub fn new<P: TransferPrefix>(
        frame: &RingTransactData<'_>,
        ring: &RingPolicy,
        accounts: usize,
        prefix: &mut P,
    ) -> Result<Self, P::Error> {
        let delegate_rail = frame.rail().is_delegate();
        let read = RootRead::transfer(ring, delegate_rail);
        let registries = 1..1 + ring.root_registries(read).count();
        let pairs = prefix.co_signer_pairs(registries.end, frame.co_sign_requirement(ring))?;
        let co_signers = registries.end..registries.end + 2 * pairs;
        let delegate = delegate_rail.then_some(co_signers.end);
        let mint = match delegate {
            Some(signer) if prefix.delegate_mint(signer)? => Some(signer + 1),
            _ => None,
        };
        let spp = co_signers.end + usize::from(delegate.is_some()) + usize::from(mint.is_some());
        Ok(Self {
            registries,
            co_signers,
            delegate,
            mint,
            ..Self::pool(frame, spp..accounts)?
        })
    }

    /// SPP's windows alone, `list` spans the accounts it receives.
    pub fn pool(frame: &RingTransactData<'_>, list: Range<usize>) -> Result<Self, CustomRingError> {
        let trees = list.start + RING_TRANSACT_ACCOUNT_PREFIX_LEN;
        let trees = trees..trees + frame.input_trees.count();
        let nullifiers = trees.end..trees.end + frame.nullifiers.len();
        // The cached selector's accounts follow the settlement groups, a run read through them would
        // take a cache as an unsigned owner and every cached frame would refuse.
        let legs = frame.interface_transfers.iter();
        let settlement_len: usize = legs.map(|leg| leg.settlement_account_count()).sum();
        let (cache, settlement) = usize::checked_sub(list.end, frame.cache_accounts())
            .and_then(|cache| Some((cache, cache.checked_sub(settlement_len)?)))
            .filter(|(_, settlement)| *settlement >= nullifiers.end)
            .ok_or(CustomRingError::InvalidSettlementAccounts)?;
        Ok(Self {
            spp: list.start,
            owners: nullifiers.end..settlement,
            trees,
            nullifiers,
            settlement: settlement..cache,
            cache: cache..list.end,
            ..Self::default()
        })
    }

    pub const fn payer(&self) -> usize {
        self.spp + TRANSACT_PAYER_INDEX
    }

    /// SPP reads its output tree after the payer.
    pub const fn output_tree(&self) -> usize {
        self.payer() + 1
    }

    /// The one slot of the ring auth PDA.
    pub const fn ring_config(&self) -> usize {
        self.spp + RING_TRANSACT_RING_CONFIG_INDEX
    }

    /// The groups of `transfers`, the frame's legs, in leg order.
    pub fn legs<'a>(
        &self,
        transfers: &'a [InterfaceTransfer],
    ) -> impl Iterator<Item = SettlementLeg> + 'a {
        let mut start = self.settlement.start;
        transfers.iter().map(move |transfer| {
            let accounts = start..start + transfer.settlement_account_count();
            start = accounts.end;
            SettlementLeg {
                transfer: *transfer,
                funder: funder_account_position(transfer).map(|funder| accounts.start + funder),
                accounts,
            }
        })
    }
}
