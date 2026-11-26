//! The anonymous rail's co-signer cells, and the exit account types the wallet's privacy check reads.
//!
//! The proof holds the hidden owner apart from every co-signer cell, a hidden owner holding a co-signer
//! key would approve its own outflow. The payer, the funders and the exit destination take no cell, the
//! program lets any of them name the hidden owner and the wallet decides. A SOL destination with data or
//! a program owner, or a token account with a second authority or a keypair address, could name the
//! hidden owner behind another key, so the wallet refuses each.
//!
//! Read after `program-libs/interface/src/instructions/withdrawals.rs`. Read before `program-libs/interface/src/instructions/cosign.rs`,
//! `programs/custom-ring/src/instructions/transfer/accounts.rs`.

use solana_address::Address;
use spl_token_2022_interface::{extension::PodStateWithExtensions, pod::PodAccount};
use spr_hasher::primitives::solana_owner_identity;
use spr_interface::SPL_TOKEN_ACCOUNT_INITIALIZED;

use crate::CustomRingError;

pub const ANONYMOUS_PARTIES: usize = crate::cosign::MAX_CO_SIGNERS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenAccountControl {
    SoleOwner([u8; 32]),
    /// A delegate or a close authority acts beside the owner.
    Shared([u8; 32]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenAccountState {
    pub mint: [u8; 32],
    pub control: TokenAccountControl,
}

pub trait SettlementAccount {
    fn address(&self) -> &Address;
    fn owner(&self) -> &Address;
    fn token_account(&self) -> Result<TokenAccountState, CustomRingError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnonymousParties<'a> {
    pub co_signers: &'a [Address],
}

impl TokenAccountState {
    pub fn unpack(data: &[u8]) -> Result<Self, CustomRingError> {
        let state = PodStateWithExtensions::<PodAccount>::unpack(data)
            .map_err(|_| CustomRingError::AnonymousExitTokenAccountInvalid)?;
        if state.base.state != SPL_TOKEN_ACCOUNT_INITIALIZED {
            return Err(CustomRingError::AnonymousExitTokenAccountInvalid);
        }
        let owner = state.base.owner.to_bytes();
        let control = if state.base.delegate.is_none() && state.base.close_authority.is_none() {
            TokenAccountControl::SoleOwner(owner)
        } else {
            TokenAccountControl::Shared(owner)
        };
        Ok(Self {
            mint: state.base.mint.to_bytes(),
            control,
        })
    }
}

impl TokenAccountControl {
    pub fn owner(&self) -> &[u8; 32] {
        match self {
            Self::SoleOwner(owner) | Self::Shared(owner) => owner,
        }
    }
}

impl AnonymousParties<'_> {
    /// Solana owner identities in co-signer order, zero past the last co-signer.
    pub fn identities(&self) -> Result<[[u8; 32]; ANONYMOUS_PARTIES], CustomRingError> {
        if self.co_signers.len() > ANONYMOUS_PARTIES {
            return Err(CustomRingError::TooManyAnonymousParties);
        }
        let mut identities = [[0u8; 32]; ANONYMOUS_PARTIES];
        for (cell, co_signer) in identities.iter_mut().zip(self.co_signers) {
            *cell = solana_owner_identity(co_signer.as_array())?;
        }
        Ok(identities)
    }
}
