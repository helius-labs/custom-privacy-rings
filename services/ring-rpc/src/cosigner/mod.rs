//! The co-signer as a chain of rules.
//!
//! A new check is one type and one `then`.
//!
//! ```ignore
//! struct BusinessHours;
//!
//! #[async_trait]
//! impl CoSignRule for BusinessHours {
//!     async fn admit(&self, operation: &Operation<'_>) -> Result<Verdict, RingRpcError> {
//!         Ok(match open_now() {
//!             true => Verdict::Admitted,
//!             false => Verdict::Refused(CoSignRefusal::Custom),
//!         })
//!     }
//! }
//!
//! CoSigner::new(key, ListRule::new(parties, lists).then(BusinessHours).then(window));
//! ```
//!
//! A rule reads what it needs from the `Operation` and owns its sources. A rule that reserves
//! state keys it by `Operation::ticket` and frees it in `settle`. Put a rule that reserves last,
//! a refusal after it releases the reservation.

mod direct;
mod lists;
mod rule;
mod scope;

use async_trait::async_trait;
use solana_address::Address;
use zolana_operator_custody::SigningIdentity;

pub(crate) use self::{direct::DirectCoSign, lists::migration as list_migration};
pub use self::{
    lists::{
        FrameParties, List, ListDecision, ListEntry, ListRule, ListStore, Lists, Parties,
        PartySource, StoredLists, Subject, SubjectKind,
    },
    rule::{Chain, CoSignRule, Operation, Verdict},
};
use crate::{
    cosign::{CoSignDecision, CoSignPolicy, CoSignRequest, CoSignSettlement},
    error::RingRpcError,
};

/// Signs what its rule admits.
pub struct CoSigner<R> {
    signer: SigningIdentity,
    pub(crate) rule: R,
}

impl<R: CoSignRule> CoSigner<R> {
    pub const fn new(signer: SigningIdentity, rule: R) -> Self {
        Self { signer, rule }
    }

    /// `first` judges ahead of the rules the co-signer holds.
    #[must_use]
    pub fn after<G: CoSignRule>(self, first: G) -> CoSigner<Chain<G, R>> {
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

    async fn decide(&self, request: &CoSignRequest) -> Result<CoSignDecision, RingRpcError> {
        let ticket = self.signer.sign(&request.message.serialize()).await?;
        let operation = Operation {
            ring: request.ring,
            message: &request.message,
            flows: &request.flows,
            ticket,
        };
        Ok(match self.rule.admit(&operation).await? {
            Verdict::Admitted => CoSignDecision::Signed(ticket),
            Verdict::Refused(refusal) => CoSignDecision::Refused(refusal),
        })
    }

    async fn settle(&self, settlement: CoSignSettlement) -> Result<(), RingRpcError> {
        self.rule.settle(settlement).await
    }
}

#[cfg(test)]
mod tests;
