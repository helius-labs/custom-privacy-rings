use std::future::Future;

use solana_address::Address;
use zolana_ring_client::rpc::{
    PolicyAuthorityAction, PolicyAuthorityAttestation, PolicyAuthorityAuth, ReadTarget,
};

use ring_service_core::{
    authorize::{Authorized, AuthorizedNonce, PendingNonce, WalletCheck},
    upstream::TransactionSource,
};

use crate::{error::RingAuditError, shared::Shared};

#[must_use]
pub struct PolicyAction<'a, S, A> {
    pub shared: &'a Shared<S>,
    pub ring: Address,
    pub auth: &'a PolicyAuthorityAuth,
    pub action: A,
}

impl<S: TransactionSource, A: PolicyAuthorityAction> PolicyAction<'_, S, A> {
    pub async fn authorize<T, E, Failure, F, Fut>(self, authorize: F) -> Result<T, Failure>
    where
        F: FnOnce(Address) -> Fut,
        Fut: Future<Output = Result<T, E>>,
        Failure: From<E> + From<RingAuditError>,
    {
        let shared = self.shared;
        let authority = self.auth.authority.0;
        let pending = self.pending()?;
        let Authorized {
            value,
            nonce: authorized,
        } = pending.authorize(authorize(authority)).await?;
        Self::accept(shared, authorized).await?;
        Ok(value)
    }

    fn pending(self) -> Result<PendingNonce, RingAuditError> {
        let Self {
            shared,
            ring,
            auth,
            action,
        } = self;
        shared.validate_ring(ring)?;
        let authority = auth.authority.0;
        let now = shared.clock.now()?;
        let nonce = WalletCheck {
            wallet: authority,
            timestamp: auth.timestamp,
            nonce: &auth.nonce,
            signature: &auth.signature,
            attestation: &PolicyAuthorityAttestation {
                target: ReadTarget {
                    cluster: shared.genesis_hash,
                    service: shared.service,
                    ring,
                },
                authority,
                action,
            },
            now,
        }
        .decide()?;
        Ok(PendingNonce {
            ring,
            nonce,
            timestamp: auth.timestamp,
        })
    }

    async fn accept(shared: &Shared<S>, authorized: AuthorizedNonce) -> Result<(), RingAuditError> {
        shared
            .replay
            .accept(authorized, shared.clock.now()?)
            .await?;
        Ok(())
    }
}
