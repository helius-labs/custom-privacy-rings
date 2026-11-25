//! The Ring RPC methods the ring policy authority signs.
//! Read `record_update` after for the pending state, `record_writer` for how an approval lands, and
//! `enrollment_applier` for the rings a registration serves.

use std::future::Future;

use solana_address::Address;
use zolana_ring_client::rpc::{
    CreateRecords, CreateRecordsRegistryRequest, CreateRecordsRegistryResponse,
    GetPendingListApprovalsRequest, GetPendingListApprovalsResponse, ListPendingApprovals,
    PolicyAuthorityAction, PolicyAuthorityAttestation, PolicyAuthorityAuth, ReadTarget,
    RecordUpdateStatus, RefuseListApproval, RefuseListApprovalRequest, RegisterEnrollment,
    RegisterEnrollmentRingRequest, RegisterEnrollmentRingResponse,
};

use crate::{
    authorize::{Authorized, PendingNonce, WalletCheck},
    error::RingRpcError,
    hub::Hub,
    upstream::TransactionSource,
};

#[must_use]
pub(crate) struct PolicyAction<'a, S, A> {
    pub hub: &'a Hub<S>,
    pub ring: Address,
    pub auth: &'a PolicyAuthorityAuth,
    pub action: A,
}

impl<S: TransactionSource> Hub<S> {
    pub async fn pending_list_approvals(
        &self,
        request: &GetPendingListApprovalsRequest,
    ) -> Result<GetPendingListApprovalsResponse, RingRpcError> {
        let ring = request.ring.0;
        let writer = self.record_writer()?;
        let action = ListPendingApprovals {
            registry: request.registry.0,
            cursor: request.cursor.clone(),
        };
        let verified = PolicyAction {
            hub: self,
            ring,
            auth: &request.auth,
            action,
        }
        .authorize(|authority| writer.require_policy_authority(ring, authority))
        .await?;
        writer.pending_list_approvals(verified, request).await
    }

    pub async fn refuse_list_approval(
        &self,
        request: &RefuseListApprovalRequest,
    ) -> Result<RecordUpdateStatus, RingRpcError> {
        let ring = request.ring.0;
        let writer = self.record_writer()?;
        let action = RefuseListApproval {
            registry: request.registry.0,
            update_id: request.update_id.clone(),
        };
        let verified = PolicyAction {
            hub: self,
            ring,
            auth: &request.auth,
            action,
        }
        .authorize(|authority| writer.require_policy_authority(ring, authority))
        .await?;
        writer.refuse_list_approval(verified, request).await
    }

    /// The upgrade authority signs before `Initialize`, the policy authority after.
    pub async fn create_records_registry(
        &self,
        request: &CreateRecordsRegistryRequest,
    ) -> Result<CreateRecordsRegistryResponse, RingRpcError> {
        let ring = request.ring.0;
        let writer = self.record_writer()?;
        let authority = PolicyAction {
            hub: self,
            ring,
            auth: &request.auth,
            action: CreateRecords,
        }
        .authorize(|authority| async move {
            self.shared()
                .require_ring_authority(ring, authority)
                .await?;
            Ok(authority)
        })
        .await?;
        Ok(CreateRecordsRegistryResponse {
            registry: writer.create_records(ring, authority).await?.into(),
            storage_authority: writer.authority().into(),
        })
    }

    /// The policy authority alone, the signed action names this service's enrollment writer.
    pub async fn register_enrollment_ring(
        &self,
        request: &RegisterEnrollmentRingRequest,
    ) -> Result<RegisterEnrollmentRingResponse, RingRpcError> {
        let ring = request.ring.0;
        let applier = self.enrollment_applier()?;
        let action = RegisterEnrollment {
            writer: applier.payer(),
        };
        let registration = PolicyAction {
            hub: self,
            ring,
            auth: &request.auth,
            action,
        }
        .authorize(|authority| applier.authorize_registration(ring, authority))
        .await?;
        applier.complete_registration(registration).await?;
        Ok(RegisterEnrollmentRingResponse {})
    }
}

impl<S: TransactionSource, A: PolicyAuthorityAction> PolicyAction<'_, S, A> {
    pub(crate) async fn authorize<T, F, Fut>(self, authorize: F) -> Result<T, RingRpcError>
    where
        F: FnOnce(Address) -> Fut,
        Fut: Future<Output = Result<T, RingRpcError>>,
    {
        let Self {
            hub,
            ring,
            auth,
            action,
        } = self;
        let shared = hub.shared();
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
        let Authorized {
            value,
            nonce: authorized,
        } = PendingNonce {
            ring,
            nonce,
            timestamp: auth.timestamp,
        }
        .authorize(authorize(authority))
        .await?;
        shared
            .replay
            .accept(authorized, shared.clock.now()?)
            .await?;
        Ok(value)
    }
}
