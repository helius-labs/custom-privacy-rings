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

use ring_service_core::{
    authorize::{Authorized, PendingNonce, WalletCheck},
    upstream::TransactionSource,
};

use crate::{error::RingRpcError, hub::Hub};

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
        PolicyAction {
            hub: self,
            ring,
            auth: &request.auth,
            action,
        }
        .authorize(|authority| writer.require_policy_authority(ring, authority))
        .await?;
        Ok(writer.pending_list_approvals(request).await?)
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
        PolicyAction {
            hub: self,
            ring,
            auth: &request.auth,
            action,
        }
        .authorize(|authority| writer.require_policy_authority(ring, authority))
        .await?;
        Ok(writer.refuse_list_approval(request).await?)
    }

    /// The upgrade authority signs before `Initialize`, the policy authority after.
    pub async fn create_records_registry(
        &self,
        request: &CreateRecordsRegistryRequest,
    ) -> Result<CreateRecordsRegistryResponse, RingRpcError> {
        let ring = request.ring.0;
        let writer = self.record_writer()?;
        PolicyAction {
            hub: self,
            ring,
            auth: &request.auth,
            action: CreateRecords,
        }
        .authorize(|authority| self.shared().require_ring_authority(ring, authority))
        .await?;
        Ok(CreateRecordsRegistryResponse {
            registry: writer.create_records(ring).await?.into(),
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
        PolicyAction {
            hub: self,
            ring,
            auth: &request.auth,
            action,
        }
        .authorize(|authority| applier.authorize_registration(ring, authority))
        .await?;
        applier.complete_registration(ring).await?;
        Ok(RegisterEnrollmentRingResponse {})
    }
}

impl<S: TransactionSource, A: PolicyAuthorityAction> PolicyAction<'_, S, A> {
    pub(crate) async fn authorize<T, E, F, Fut>(self, authorize: F) -> Result<T, RingRpcError>
    where
        F: FnOnce(Address) -> Fut,
        Fut: Future<Output = Result<T, E>>,
        RingRpcError: From<E>,
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc, time::Duration};

    use bytemuck::Zeroable;
    use custom_ring_interface::{
        tracking::TrackingRegistry, RingPolicy, RING_POLICY, STORAGE_PROGRAM_ID,
    };
    use ring_service_core::{
        authorize::Unauthorized,
        clock::Clock,
        keys::RootSecret,
        replay::ReplayGuard,
        upstream::{ChainSource, Upstreams},
    };
    use ring_writer::enrollment_applier::{EnrollmentApplier, EnrollmentApplierParts};
    use solana_keypair::Keypair;
    use solana_signer::Signer;
    use zolana_ring_client::CustomRing;

    use super::*;
    use crate::{
        error::ServiceError,
        hub::ServiceIdentity,
        role_pins::tests::{account, RingAccounts},
    };

    const REGISTERED: Address = Address::new_from_array([13; 32]);
    const REGISTRY: Address = Address::new_from_array([2; 32]);
    const CLUSTER: [u8; 32] = [1; 32];
    const NOW: u64 = 1_700_000_000;

    fn policy_key() -> Keypair {
        Keypair::new_from_array([10; 32])
    }

    fn payer() -> Keypair {
        Keypair::new_from_array([9; 32])
    }

    fn service_key() -> Keypair {
        Keypair::new_from_array([12; 32])
    }

    /// `REGISTERED` names the payer as its writer over an initialized tracking registry.
    async fn applier() -> EnrollmentApplier {
        let mut policy = RingPolicy::zeroed();
        policy.discriminator = RING_POLICY;
        policy.tracking = REGISTRY;
        policy.roles.policy = policy_key().pubkey();
        policy.roles.enrollment_writer = payer().pubkey();
        let mut registry = TrackingRegistry::zeroed();
        registry.discriminator = TrackingRegistry::DISCRIMINATOR;
        registry.next_index = 1u64.to_le_bytes();
        let chain = RingAccounts(Some(HashMap::from([
            (
                CustomRing::new(REGISTERED).policy_pda(),
                account(REGISTERED, bytemuck::bytes_of(&policy)),
            ),
            (
                REGISTRY,
                account(STORAGE_PROGRAM_ID, bytemuck::bytes_of(&registry)),
            ),
        ])));
        EnrollmentApplierParts {
            chain: Arc::new(chain),
            payer: Arc::new(payer()),
            rings: Vec::new(),
            budget_lamports: u64::MAX,
            ring_budget_lamports: u64::MAX,
            max_rings: 2,
            store: None,
        }
        .applier()
        .await
        .expect("applier")
    }

    async fn hub() -> Hub<ChainSource> {
        let upstreams = Upstreams {
            indexer_url: "http://127.0.0.1:1",
            rpc_url: "http://127.0.0.1:1",
            timeout: Duration::from_secs(1),
        };
        let identity = ServiceIdentity {
            genesis_hash: CLUSTER,
            service: Arc::new(service_key()),
        };
        Hub::builder(ChainSource::connect(upstreams).expect("source"), identity)
            .with_clock(Clock::Fixed(NOW))
            .with_enrollment_applier(Arc::new(applier().await))
            .derived(RootSecret::from_bytes([7; 32]).expect("root"))
            .expect("hub")
    }

    #[tokio::test]
    async fn the_hub_spends_each_nonce_once_and_binds_the_writer() {
        let hub = hub().await;
        let target = ReadTarget {
            cluster: CLUSTER,
            service: service_key().pubkey(),
            ring: REGISTERED,
        };
        let signed = |writer: Address| {
            RegisterEnrollmentRingRequest::register(target, policy_key().pubkey(), writer)
                .at(NOW)
                .sign(&policy_key())
                .expect("signed")
        };
        let other_writer = hub
            .register_enrollment_ring(&signed(Keypair::new_from_array([1; 32]).pubkey()))
            .await;
        assert!(
            matches!(
                other_writer,
                Err(RingRpcError::Service(ServiceError::Unauthorized(
                    Unauthorized::BadSignature
                )))
            ),
            "{other_writer:?}"
        );
        let request = signed(payer().pubkey());
        hub.register_enrollment_ring(&request)
            .await
            .expect("registered");
        let replayed = hub.register_enrollment_ring(&request).await;
        assert!(
            matches!(
                replayed,
                Err(RingRpcError::Service(ServiceError::Unauthorized(
                    Unauthorized::Replay
                )))
            ),
            "{replayed:?}"
        );
    }

    #[tokio::test]
    async fn unauthorized_registration_consumes_no_replay_capacity() {
        let hub = hub().await;
        let stranger = Keypair::new_from_array([99; 32]);
        let fake_ring = Address::new_from_array([77; 32]);
        let request = RegisterEnrollmentRingRequest::register(
            ReadTarget {
                cluster: CLUSTER,
                service: service_key().pubkey(),
                ring: fake_ring,
            },
            stranger.pubkey(),
            payer().pubkey(),
        )
        .at(NOW)
        .sign(&stranger)
        .expect("signed");

        assert!(hub.register_enrollment_ring(&request).await.is_err());
        let ReplayGuard::Local(nonces) = &hub.shared().replay else {
            panic!("expected local nonces");
        };
        assert_eq!(nonces.ring_count(), 0);
    }
}
