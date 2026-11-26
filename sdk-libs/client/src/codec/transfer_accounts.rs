//! The account windows, policy sources and exit destinations a client derives beside a transfer frame.

use custom_ring_interface::{
    anonymous_exit::TokenAccountControl, cosign::CoSignRequirement, source_map_of, tag,
    transfer_layout::TransferPrefix, CustomRingError, PolicyWithdrawal, RingPolicy,
    RingTransactData, RootRead, TransferApproval, TransferLayout,
};
use solana_address::Address;
use spr_hasher::{
    primitives::{hash_bytes, solana_owner_identity},
    HasherError,
};
use spr_interface::SOL_ASSET_FIELD;
use zolana_ring_policy::{ListId, RuleTable, SourceMap};

pub const SYSTEM_PROGRAM_ID: Address = Address::new_from_array([0u8; 32]);

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
        let slots = spp.checked_sub(
            1 + ring
                .root_registries(RootRead::transfer(ring, delegate))
                .count(),
        );
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

pub trait CoSignerPairs {
    /// `(record, signer)` of each pair.
    fn co_signer_pairs(&self) -> impl Iterator<Item = (usize, usize)>;
}

impl CoSignerPairs for TransferLayout {
    fn co_signer_pairs(&self) -> impl Iterator<Item = (usize, usize)> {
        self.co_signers
            .clone()
            .step_by(2)
            .map(|record| (record, record + 1))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum AnonymousExitDestination<'a> {
    Sol {
        recipient: &'a Address,
        owner: &'a Address,
        data_len: usize,
    },
    Spl {
        token_account: &'a Address,
        control: &'a TokenAccountControl,
    },
}

impl<'a> AnonymousExitDestination<'a> {
    /// The key the anonymous audit proof proves apart from the hidden owner.
    pub fn named_key(self) -> Result<&'a [u8; 32], CustomRingError> {
        match self {
            // Data or a program owner could name a second key.
            Self::Sol {
                recipient,
                owner,
                data_len,
            } => {
                if *owner == SYSTEM_PROGRAM_ID && data_len == 0 {
                    Ok(recipient.as_array())
                } else {
                    Err(CustomRingError::AnonymousExitAccountHasData)
                }
            }
            // The proof binds the owner only, a keypair address could be the hidden owner's.
            Self::Spl { token_account, .. } if token_account.is_on_curve() => {
                Err(CustomRingError::AnonymousExitTokenAccountOnCurve)
            }
            Self::Spl {
                control: TokenAccountControl::SoleOwner(owner),
                ..
            } => Ok(owner),
            Self::Spl {
                control: TokenAccountControl::Shared(_),
                ..
            } => Err(CustomRingError::AnonymousExitTokenAccountShared),
        }
    }
}

pub trait WithdrawalLeg: Sized {
    fn new(owner: &[u8; 32], mint: Option<&[u8; 32]>, amount: u64) -> Result<Self, HasherError>;
}

impl WithdrawalLeg for PolicyWithdrawal {
    fn new(owner: &[u8; 32], mint: Option<&[u8; 32]>, amount: u64) -> Result<Self, HasherError> {
        Ok(Self {
            owner: solana_owner_identity(owner)?,
            asset: match mint {
                Some(mint) => hash_bytes(mint)?,
                None => SOL_ASSET_FIELD,
            },
            amount,
        })
    }
}

pub const TRANSFER_APPROVAL_DATA_LEN: usize = 1 + TransferApproval::BODY_LEN;

pub trait ApprovalEncode {
    /// The instruction data from the tag.
    fn encode(&self) -> [u8; TRANSFER_APPROVAL_DATA_LEN];
}

impl ApprovalEncode for TransferApproval {
    fn encode(&self) -> [u8; TRANSFER_APPROVAL_DATA_LEN] {
        let mut data = [0; TRANSFER_APPROVAL_DATA_LEN];
        data[0] = tag::APPROVE_TRANSFER;
        data[1] = self.position;
        data[2..].copy_from_slice(&self.digest);
        data
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ListRegistry {
    Own,
    Curator(Address),
}

pub trait PolicySources {
    /// The registry serving `list_id`, `None` when the table does not reference it.
    fn registry_for(&self, list_id: ListId) -> Option<ListRegistry>;

    /// The map `policy_hash` was computed over.
    fn source_map(&self) -> Result<SourceMap, CustomRingError>;

    fn rule_table(&self) -> Result<RuleTable, CustomRingError>;
}

impl PolicySources for RingPolicy {
    fn registry_for(&self, list_id: ListId) -> Option<ListRegistry> {
        let slot = self.sources[list_id.slot()];
        (slot.list_id != 0).then(|| match slot.registry {
            registry if registry == Address::default() => ListRegistry::Own,
            registry => ListRegistry::Curator(registry),
        })
    }

    fn source_map(&self) -> Result<SourceMap, CustomRingError> {
        source_map_of(&self.sources, self.ring_list_tag)
    }

    fn rule_table(&self) -> Result<RuleTable, CustomRingError> {
        Ok(self.rules.validate(&self.source_map()?)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_data_parses_only_at_its_exact_length_and_tag() {
        let approval = TransferApproval {
            position: 3,
            digest: [9; 32],
        };
        let data = approval.encode();
        assert_eq!(TransferApproval::parse(&data), Some(approval));
        assert_eq!(TransferApproval::parse(&data[..33]), None);
        assert_eq!(
            TransferApproval::parse(&[data.as_slice(), &[0]].concat()),
            None
        );
        let mut other_tag = data;
        other_tag[0] = tag::TRANSFER;
        assert_eq!(TransferApproval::parse(&other_tag), None);
    }
}
