//! A program's call into the ring's `Transfer` under its own PDA.
//!
//! The ring takes an owner by its signature alone, a PDA signs only through its program's call.
//! `RingTransferCpi` raises that one PDA to a signer and forwards no tag but `TRANSFER`.

use spr_interface::instruction::InterfaceTransfer;

use crate::{tag, CustomRingError, ParsedTransact, RingMode, RingRail, RingTransactData};

#[cfg(feature = "cpi")]
use {
    crate::{
        cosign::CoSignRequirement, CoSigner, RingAccount, RingPolicy, TransferLayout,
        TransferPrefix, MAX_TRANSFER_ACCOUNTS,
    },
    arrayvec::ArrayVec,
    solana_address::Address,
    solana_program_error::ProgramError,
};

/// What a calling program binds its own proof to ahead of the call.
pub struct TransferFrame<'a> {
    source: &'a [u8],
    parsed: ParsedTransact<'a>,
}

impl<'a> TransferFrame<'a> {
    /// `data` from the `TRANSFER` tag.
    pub fn parse(data: &'a [u8], mode: RingMode) -> Result<Self, CustomRingError> {
        let frame = data
            .strip_prefix(&[tag::TRANSFER])
            .ok_or(CustomRingError::InvalidInstructionData)?;
        Ok(Self {
            source: data,
            parsed: RingTransactData::parse(frame, RingRail::transfer(mode))?,
        })
    }

    /// The bytes `parse` read, the call forwards them.
    pub const fn data(&self) -> &'a [u8] {
        self.source
    }

    pub fn private_tx_hash(&self) -> &'a [u8; 32] {
        self.parsed.data().private_tx_hash
    }

    pub fn interface_transfers(&self) -> &[InterfaceTransfer] {
        &self.parsed.data().interface_transfers
    }
}

#[cfg(feature = "cpi")]
pub trait TransferCpiAccount {
    fn address(&self) -> &Address;
    fn owner(&self) -> &Address;
    fn is_signer(&self) -> bool;
    fn with_data<R>(&self, f: impl FnOnce(&[u8]) -> Option<R>) -> Option<R>;
}

#[cfg(feature = "cpi")]
type SignerFlags = ArrayVec<bool, MAX_TRANSFER_ACCOUNTS>;

#[cfg(feature = "cpi")]
#[must_use]
pub struct TransferSignerCheck<'a, A> {
    ring_program: &'a Address,
    owner: &'a Address,
    accounts: &'a [A],
    flags: SignerFlags,
}

#[cfg(feature = "cpi")]
impl<A: TransferCpiAccount> TransferPrefix for TransferSignerCheck<'_, A> {
    type Error = ProgramError;

    fn co_signer_pairs(
        &mut self,
        first: usize,
        requirement: Option<CoSignRequirement>,
    ) -> Result<usize, ProgramError> {
        CoSignRequirement::pairs(requirement, |pair, current| {
            let signer = first + 2 * pair + 1;
            let [record, slot] = self
                .accounts
                .get(signer - 1..=signer)
                .ok_or(ProgramError::NotEnoughAccountKeys)?
            else {
                return Err(ProgramError::NotEnoughAccountKeys);
            };
            let cover = read_ring_account(self.ring_program, record, |stored: &CoSigner| {
                current.cover(stored)
            })?;
            if slot.address() == self.owner {
                if cover.signer != *self.owner {
                    return Err(CustomRingError::UnauthorizedCoSigner.into());
                }
                self.flags[signer] = true;
            }
            Ok(cover)
        })
    }

    fn delegate_mint(&mut self, _: usize) -> Result<bool, ProgramError> {
        Err(CustomRingError::InvalidInstructionData.into())
    }
}

#[cfg(feature = "cpi")]
impl<'a, A: TransferCpiAccount> TransferSignerCheck<'a, A> {
    /// `frame` yields the call's frame under the ring's mode.
    #[inline(never)]
    pub fn signers<'f>(
        ring_program: &'a Address,
        owner: &'a Address,
        accounts: &'a [A],
        frame: impl FnOnce(RingMode) -> Result<TransferFrame<'f>, CustomRingError>,
    ) -> Result<SignerFlags, ProgramError> {
        if accounts.len() > MAX_TRANSFER_ACCOUNTS {
            return Err(CustomRingError::TooManyAccounts.into());
        }
        let mut check = Self {
            ring_program,
            owner,
            accounts,
            flags: accounts.iter().map(A::is_signer).collect(),
        };
        let ring_account = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
        let (frame, layout) =
            read_ring_account(ring_program, ring_account, |ring: &RingPolicy| {
                let frame = frame(ring.mode()?)?;
                let layout =
                    TransferLayout::new(frame.parsed.data(), ring, accounts.len(), &mut check);
                Ok((frame, layout))
            })?;
        let layout = layout?;
        let transfers = frame.interface_transfers();
        // Duplicate CPI accounts share signer privilege across the inner instruction, the PDA signs in
        // the owner run and its own co-signer slots alone.
        for index in layout.owners.clone() {
            check.flags[index] |= accounts[index].address() == owner;
        }
        let unsigned = |index: &usize| accounts[*index].address() == owner && !check.flags[*index];
        if core::iter::once(layout.payer())
            .chain(layout.cache.clone())
            .chain(layout.legs(transfers).filter_map(|leg| leg.funder))
            .any(|index| unsigned(&index))
        {
            return Err(CustomRingError::MissingFundingSigner.into());
        }
        // SPP pays a SOL withdrawal to its recipient unsigned.
        let recipients = || {
            layout.legs(transfers).filter_map(|leg| {
                matches!(leg.transfer, InterfaceTransfer::SolWithdrawal { .. })
                    .then_some(leg.accounts.start + 1)
            })
        };
        if (0..accounts.len())
            .filter(unsigned)
            .any(|index| !recipients().any(|recipient| recipient == index))
        {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(check.flags)
    }
}

#[cfg(feature = "cpi")]
fn read_ring_account<T: RingAccount, A: TransferCpiAccount, R>(
    ring_program: &Address,
    account: &A,
    f: impl FnOnce(&T) -> Result<R, CustomRingError>,
) -> Result<R, CustomRingError> {
    if account.owner() != ring_program {
        return Err(T::INVALID);
    }
    account
        .with_data(|bytes| T::from_data(bytes).map(f))
        .ok_or(T::INVALID)?
}

#[cfg(feature = "cpi")]
pub use cpi::RingTransferCpi;

#[cfg(feature = "cpi")]
mod cpi {
    use arrayvec::ArrayVec;
    use pinocchio::{
        cpi::{invoke_signed_with_bounds, Signer, MAX_STATIC_CPI_ACCOUNTS},
        instruction::{InstructionAccount, InstructionView},
        AccountView, Address, ProgramResult,
    };

    use super::{TransferCpiAccount, TransferFrame, TransferSignerCheck};
    use crate::{CustomRingError, RingRail};

    impl TransferCpiAccount for AccountView {
        fn address(&self) -> &Address {
            AccountView::address(self)
        }

        fn owner(&self) -> &Address {
            AccountView::owner(self)
        }

        fn is_signer(&self) -> bool {
            AccountView::is_signer(self)
        }

        fn with_data<R>(&self, f: impl FnOnce(&[u8]) -> Option<R>) -> Option<R> {
            let bytes = self.try_borrow().ok()?;
            f(&bytes)
        }
    }

    #[must_use]
    pub struct RingTransferCpi<'a> {
        /// Fixed by the caller's signer seeds or its own state, never taken from the caller's accounts alone.
        pub ring_program: &'a Address,
        /// The PDA the signer seeds derive.
        pub owner: &'a Address,
        /// The transfer's list in call order.
        pub accounts: &'a [AccountView],
        /// From the `TRANSFER` tag.
        pub data: &'a [u8],
    }

    impl RingTransferCpi<'_> {
        /// Signs only in note owner or cosigner slots, never as payer or deposit funder.
        pub fn invoke_signed(self, signer: Signer<'_, '_>) -> ProgramResult {
            let data = self.data;
            self.invoke_under(|mode| TransferFrame::parse(data, mode), signer)
        }

        /// `invoke_signed` over a frame the caller parsed from `data`.
        pub fn invoke_signed_parsed(
            self,
            frame: TransferFrame<'_>,
            signer: Signer<'_, '_>,
        ) -> ProgramResult {
            let data = self.data;
            self.invoke_under(
                |mode| {
                    if core::ptr::eq(frame.data(), data)
                        && frame.parsed.data().rail() == RingRail::transfer(mode)
                    {
                        Ok(frame)
                    } else {
                        Err(CustomRingError::InvalidInstructionData)
                    }
                },
                signer,
            )
        }

        #[inline(never)]
        fn invoke_under<'f>(
            self,
            frame: impl FnOnce(crate::RingMode) -> Result<TransferFrame<'f>, CustomRingError>,
            signer: Signer<'_, '_>,
        ) -> ProgramResult {
            if self.accounts.len() > MAX_STATIC_CPI_ACCOUNTS {
                return Err(CustomRingError::TooManyAccounts.into());
            }
            let signers =
                TransferSignerCheck::signers(self.ring_program, self.owner, self.accounts, frame)?;
            let mut metas = ArrayVec::<InstructionAccount, MAX_STATIC_CPI_ACCOUNTS>::new();
            for (account, signs) in self.accounts.iter().zip(signers) {
                metas
                    .try_push(InstructionAccount::new(
                        account.address(),
                        account.is_writable(),
                        signs,
                    ))
                    .map_err(|_| CustomRingError::TooManyAccounts)?;
            }
            let instruction = InstructionView {
                program_id: self.ring_program,
                accounts: &metas,
                data: self.data,
            };
            invoke(&instruction, self.accounts, signer)
        }
    }

    /// Its own frame, the metas and pinocchio's account array together pass one SBF frame.
    #[inline(never)]
    fn invoke(
        instruction: &InstructionView,
        accounts: &[AccountView],
        signer: Signer<'_, '_>,
    ) -> ProgramResult {
        invoke_signed_with_bounds::<MAX_STATIC_CPI_ACCOUNTS, _>(instruction, accounts, &[signer])
    }
}
