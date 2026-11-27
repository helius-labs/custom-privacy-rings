mod admin;
mod decision;
mod store;

use std::future::Future;

use async_trait::async_trait;
use solana_address::Address;

pub use self::admin::{co_sign_lists, edit_co_sign_list};
#[cfg(test)]
pub(crate) use self::decision::{List, ListEntry};
pub(crate) use self::{
    decision::{ListDecision, Parties, Subject},
    store::StoredLists,
};
use super::rule::{CoSignRule, Operation, Verdict};
use crate::{
    cosign::{CoSignFlows, FrameSource},
    error::RingCosignerError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FrameParties {
    /// The ring demands no co-signer of its delegate, no list reads the move.
    Delegate,
    Transfer(Parties),
}

pub(crate) trait PartySource: Send + Sync {
    /// Read without the input notes wherever the frame names its senders.
    fn parties(
        &self,
        ring: Address,
        frame: FrameSource<'_>,
    ) -> impl Future<Output = Result<FrameParties, RingCosignerError>> + Send;
}

pub(crate) struct ListRule<P> {
    parties: P,
    lists: StoredLists,
}

impl<P: PartySource> ListRule<P> {
    pub(crate) const fn new(parties: P, lists: StoredLists) -> Self {
        Self { parties, lists }
    }
}

#[async_trait]
impl<P: PartySource> CoSignRule for ListRule<P> {
    async fn admit(&self, operation: &Operation<'_>) -> Result<Verdict, RingCosignerError> {
        let parties = match operation.flows {
            CoSignFlows::Frame => match self
                .parties
                .parties(operation.ring, operation.frame_source())
                .await?
            {
                FrameParties::Delegate => return Ok(Verdict::Admitted),
                FrameParties::Transfer(parties) => parties,
            },
            // The caller's rows name no owner the frame binds, the assets alone are screened.
            CoSignFlows::Published(flows) => Parties {
                assets: flows.iter().map(|flow| flow.flow.asset).collect(),
                ..Parties::default()
            },
        };
        let subjects: Vec<Subject> = parties.subjects().collect();
        let lists = self.lists.lists(operation.ring, &subjects).await?;
        Ok(ListDecision {
            parties: &parties,
            lists: &lists,
        }
        .evaluate())
    }
}
