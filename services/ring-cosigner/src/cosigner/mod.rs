//! The co-signer as a chain of rules.
//!
//! A new check is one type and one `then`.
//!
//! ```ignore
//! struct BusinessHours;
//!
//! #[async_trait]
//! impl CoSignRule for BusinessHours {
//!     async fn admit(&self, operation: &Operation<'_>) -> Result<Verdict, RingCosignerError> {
//!         Ok(match open_now() {
//!             true => Verdict::Admitted,
//!             false => Verdict::Refused(CoSignRefusal::NotApproved),
//!         })
//!     }
//! }
//!
//! CoSigner::new(key, ListRule::new(parties, lists).then(BusinessHours).then(window));
//! ```
//!
//! A rule reads what it needs from the `Operation` and owns its sources. A rule that reserves
//! state keys it by `Operation::ticket` and frees it in `settle`. The relay settles its own send,
//! the rule settles a `Sender::Wallet` reservation in `sweep`. Put a rule that reserves last, a
//! refusal after it releases the reservation.
//!
//! [`ExitVelocity`] is an example of a rule that reserves. It settles no wallet send, and the
//! service's chain does not hold it.

mod direct;
mod exit_velocity;
mod lists;
mod rule;
mod scope;

use async_trait::async_trait;
use solana_address::Address;
use zolana_operator_custody::SigningIdentity;

pub use self::direct::DirectCoSign;
pub use self::{
    exit_velocity::{ExitCap, ExitVelocity},
    lists::{
        co_sign_lists, edit_co_sign_list, FrameParties, List, ListDecision, ListEntry, ListRule,
        ListStore, Lists, Parties, PartySource, StoredLists, Subject, SubjectKind,
    },
    rule::{Chain, CoSignRule, Operation, Verdict},
    scope::ApprovedCall,
};
use ring_service_core::landing::FinalizedTip;

use crate::{
    cosign::{CoSignDecision, CoSignPolicy, CoSignRequest, CoSignSettlement, Ticket},
    error::RingCosignerError,
};

/// Signs what its rule admits.
pub struct CoSigner<R> {
    signer: SigningIdentity,
    pub(crate) rule: R,
}

impl<R: CoSignRule> CoSigner<R> {
    pub(crate) const fn new(signer: SigningIdentity, rule: R) -> Self {
        Self { signer, rule }
    }

    /// `first` judges ahead of the rules the co-signer holds.
    #[must_use]
    pub(crate) fn after<G: CoSignRule>(self, first: G) -> CoSigner<Chain<G, R>> {
        CoSigner {
            signer: self.signer,
            rule: first.then(self.rule),
        }
    }
}

#[async_trait]
impl<R: CoSignRule> CoSignPolicy for CoSigner<R> {
    fn signer(&self) -> Option<Address> {
        Some(self.signer.address())
    }

    async fn decide(&self, request: &CoSignRequest) -> Result<CoSignDecision, RingCosignerError> {
        let operation = Operation {
            ring: request.ring,
            message: &request.message,
            approved_transfer: request.approved_transfer.as_ref(),
            flows: &request.flows,
            sender: request.sender,
            ticket: Ticket::of(&self.signer.address(), &request.message),
        };
        Ok(match self.rule.admit(&operation).await? {
            // A failed signature keeps the reservation, a repeat of the message signs under it.
            Verdict::Admitted => {
                CoSignDecision::Signed(self.signer.sign(&request.message.serialize()).await?)
            }
            Verdict::Refused(refusal) => CoSignDecision::Refused(refusal),
        })
    }

    async fn settle(&self, settlement: CoSignSettlement) -> Result<(), RingCosignerError> {
        self.rule.settle(settlement).await
    }

    async fn sweep(&self, tip: FinalizedTip) -> Result<(), RingCosignerError> {
        self.rule.sweep(tip).await
    }
}

#[cfg(test)]
mod tests;
