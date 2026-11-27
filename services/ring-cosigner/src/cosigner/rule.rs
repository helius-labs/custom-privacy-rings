use async_trait::async_trait;
use solana_address::Address;
use solana_message::VersionedMessage;

use super::scope::ApprovedCall;
use ring_service_core::landing::FinalizedTip;

use crate::{
    cosign::{CoSignFlows, CoSignRefusal, CoSignSettlement, FrameSource, Sender, Ticket},
    error::RingCosignerError,
};

#[derive(Clone, Copy)]
pub struct Operation<'a> {
    pub(crate) ring: Address,
    pub(crate) message: &'a VersionedMessage,
    pub(crate) approved_transfer: Option<&'a ApprovedCall>,
    pub(crate) flows: &'a CoSignFlows,
    pub(crate) sender: Sender,
    /// Of `message`, the key of every reservation.
    pub(crate) ticket: Ticket,
}

impl<'a> Operation<'a> {
    pub(crate) const fn frame_source(&self) -> FrameSource<'a> {
        FrameSource {
            message: self.message,
            approved_transfer: self.approved_transfer,
        }
    }
}

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Admitted,
    Refused(CoSignRefusal),
}

/// One check of the co-signer, the only extension point.
#[async_trait]
pub(crate) trait CoSignRule: Send + Sync {
    async fn admit(&self, operation: &Operation<'_>) -> Result<Verdict, RingCosignerError>;

    /// Keeps or frees what `admit` reserved under the settled ticket.
    async fn settle(&self, _settlement: CoSignSettlement) -> Result<(), RingCosignerError> {
        Ok(())
    }

    /// Settles one bounded page of what `admit` reserved for a wallet's own send.
    async fn sweep(&self, _tip: FinalizedTip) -> Result<(), RingCosignerError> {
        Ok(())
    }

    /// `next` judges only what `self` admits.
    fn then<R: CoSignRule>(self, next: R) -> Chain<Self, R>
    where
        Self: Sized,
    {
        Chain { first: self, next }
    }
}

/// Two rules in order, itself a rule.
pub struct Chain<A, B> {
    first: A,
    next: B,
}

#[async_trait]
impl<A: CoSignRule, B: CoSignRule> CoSignRule for Chain<A, B> {
    async fn admit(&self, operation: &Operation<'_>) -> Result<Verdict, RingCosignerError> {
        match self.first.admit(operation).await? {
            Verdict::Admitted => {}
            refused @ Verdict::Refused(_) => return Ok(refused),
        }
        match self.next.admit(operation).await {
            Ok(Verdict::Admitted) => Ok(Verdict::Admitted),
            // The first rule frees its reservation, no signature leaves the service.
            unsigned => {
                self.first
                    .settle(CoSignSettlement::Failed(operation.ticket))
                    .await?;
                unsigned
            }
        }
    }

    async fn settle(&self, settlement: CoSignSettlement) -> Result<(), RingCosignerError> {
        self.first.settle(settlement).await?;
        self.next.settle(settlement).await
    }

    async fn sweep(&self, tip: FinalizedTip) -> Result<(), RingCosignerError> {
        self.first.sweep(tip).await?;
        self.next.sweep(tip).await
    }
}
