//! The windows of a `Transfer` or `AuthorityTransfer` account list, one map for the program, its
//! callers and every reader.
//!
//! SPP reads its list by position, and the ring program prepends its own slots. A reader that cut a
//! window at another index would check or sign accounts the program does not read there.
//!
//! The list in order:
//!
//! 1. The ring account.
//! 2. The registries `RootRead` names, own then curator then tracking.
//! 3. One `[record, signer]` or `[record, instructions_sysvar]` pair per co-signer the demand needs, SPP does not receive them.
//! 4. The delegate's signer on the authority rails, then the mint when its permanent delegate signs, SPP receives neither.
//! 5. SPP's list, forwarded whole. Payer and `ring_config` in its prefix, input trees, nullifier PDAs, owner run, settlement groups, cache accounts.
//!
//! `test-vectors/proof/transfer-layout.json` pins the windows, `tests/transfer_layout.rs` writes it.
//!
//! Read after `program-libs/interface/src/instructions/cosign.rs`. Read before
//! `program-libs/interface/src/instructions/transfer_cpi.rs`, `programs/custom-ring/src/instructions/transfer/accounts.rs`.

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

/// A prefix its builder or a located SPP window fixes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KnownPrefix {
    pub co_signer_pairs: usize,
    pub delegate_mint: bool,
}

impl KnownPrefix {
    /// The prefix of a list whose SPP window starts at `spp`, a delegate frame takes no pair.
    pub fn ending_at(
        frame: &RingTransactData<'_>,
        ring: &RingPolicy,
        spp: usize,
    ) -> Result<Self, CustomRingError> {
        let delegate = frame.rail.is_delegate();
        let slots = spp.checked_sub(1 + registry_count(ring, delegate));
        let (co_signer_pairs, delegate_mint) = match (delegate, slots) {
            (true, Some(1)) => (0, false),
            (true, Some(2)) => (0, true),
            (false, Some(slots)) if slots % 2 == 0 => (slots / 2, false),
            _ => return Err(CustomRingError::InvalidSettlementAccounts),
        };
        Ok(Self {
            co_signer_pairs,
            delegate_mint,
        })
    }
}

impl TransferPrefix for KnownPrefix {
    type Error = CustomRingError;

    fn co_signer_pairs(
        &mut self,
        _: usize,
        _: Option<CoSignRequirement>,
    ) -> Result<usize, CustomRingError> {
        Ok(self.co_signer_pairs)
    }

    fn delegate_mint(&mut self, _: usize) -> Result<bool, CustomRingError> {
        Ok(self.delegate_mint)
    }
}

/// Indices into the whole list, the ring account at [`TransferLayout::RING`].
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// SPP's owner signer run.
    pub owners: Range<usize>,
    pub settlement: Range<usize>,
    pub cache: Range<usize>,
}

/// One leg's settlement group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementLeg {
    pub transfer: InterfaceTransfer,
    pub accounts: Range<usize>,
    /// The signer a deposit moves funds from.
    pub funder: Option<usize>,
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
        let delegate_rail = frame.rail.is_delegate();
        let registries = 1..1 + registry_count(ring, delegate_rail);
        let pairs = prefix.co_signer_pairs(registries.end, frame.co_sign_requirement(ring))?;
        let co_signers = registries.end..registries.end + 2 * pairs;
        let delegate = delegate_rail.then_some(co_signers.end);
        let mint = match delegate {
            Some(signer) if prefix.delegate_mint(signer)? => Some(signer + 1),
            _ => None,
        };
        let spp = co_signers.end + usize::from(delegate.is_some()) + usize::from(mint.is_some());
        let trees = spp + RING_TRANSACT_ACCOUNT_PREFIX_LEN;
        let trees = trees..trees + frame.input_trees.count();
        let nullifiers = trees.end..trees.end + frame.nullifiers.len();
        // The cached selector's accounts follow the settlement groups, a run read through them would
        // take a cache as an unsigned owner and every cached frame would refuse.
        let settlement_len = frame
            .interface_transfers
            .iter()
            .map(|transfer| transfer.settlement_account_count())
            .sum::<usize>();
        let cache = accounts.checked_sub(frame.cache_accounts());
        let settlement = cache.and_then(|cache| cache.checked_sub(settlement_len));
        let (Some(cache), Some(settlement)) = (cache, settlement) else {
            return Err(CustomRingError::InvalidSettlementAccounts.into());
        };
        if settlement < nullifiers.end {
            return Err(CustomRingError::InvalidSettlementAccounts.into());
        }
        Ok(Self {
            registries,
            co_signers,
            delegate,
            mint,
            spp,
            owners: nullifiers.end..settlement,
            trees,
            nullifiers,
            settlement: settlement..cache,
            cache: cache..accounts,
        })
    }

    pub const fn payer(&self) -> usize {
        self.spp + TRANSACT_PAYER_INDEX
    }

    /// The one slot of the ring auth PDA.
    pub const fn ring_config(&self) -> usize {
        self.spp + RING_TRANSACT_RING_CONFIG_INDEX
    }

    /// `(record, signer)` of each pair.
    pub fn co_signer_pairs(&self) -> impl Iterator<Item = (usize, usize)> {
        self.co_signers
            .clone()
            .step_by(2)
            .map(|record| (record, record + 1))
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

fn registry_count(ring: &RingPolicy, delegate: bool) -> usize {
    ring.root_registries(RootRead::transfer(ring, delegate))
        .count()
}
