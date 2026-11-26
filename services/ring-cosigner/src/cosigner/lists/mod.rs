mod admin;
mod decision;
mod store;

use std::future::Future;

use async_trait::async_trait;
use solana_address::Address;

pub use self::{
    admin::{co_sign_lists, edit_co_sign_list},
    decision::{List, ListDecision, ListEntry, Lists, Parties, Subject, SubjectKind},
    store::{ListStore, StoredLists},
};
use super::rule::{CoSignRule, Operation, Verdict};
use crate::{
    cosign::{CoSignFlows, FrameSource},
    error::RingCosignerError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameParties {
    /// The ring demands no co-signer of its delegate, no list reads the move.
    Delegate,
    Transfer(Parties),
}

pub trait PartySource: Send + Sync {
    /// Read without the input notes wherever the frame names its senders.
    fn parties(
        &self,
        ring: Address,
        frame: FrameSource<'_>,
    ) -> impl Future<Output = Result<FrameParties, RingCosignerError>> + Send;
}

pub struct ListRule<P, S> {
    parties: P,
    lists: S,
}

impl<P: PartySource, S: ListStore> ListRule<P, S> {
    pub(crate) const fn new(parties: P, lists: S) -> Self {
        Self { parties, lists }
    }
}

#[async_trait]
impl<P: PartySource, S: ListStore> CoSignRule for ListRule<P, S> {
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
