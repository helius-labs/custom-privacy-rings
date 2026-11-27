use std::{collections::HashMap, future::Future};

use async_trait::async_trait;
use custom_ring_interface::{
    storage::StorageRegistry, ApplyRecordsIxData, RingAccount, RingPolicy,
};
use solana_account::Account;
use solana_address::Address;
use spr_client::{AsyncProverClient, AsyncRpc, AsyncSolanaRpc, AsyncSprClient, AsyncSprIndexer};
use zolana_ring_client::{
    AsyncRingIndexer, AuditError, CustomRing, EnrollBatch, ProvenEnrollBatch, RegisteredKeys,
    RingClient, StorageApplyProof, TrackingLeaf,
};
use zolana_ring_indexer::rpc::{
    GetStorageApplyProofInputsRequest, GetStorageApplyProofInputsResponse,
    GetStorageMemberOpeningsRequest, GetStorageMemberOpeningsResponse,
};
use zolana_ring_member_registry::ApplyProofInputs;

use ring_service_core::{
    landing::{ChainSubmit, SolanaChain},
    upstream::{
        timed_indexer, timed_rpc, ChainSource, OpeningFetch, RingConfiguration, TransactionSource,
        Upstreams,
    },
    ServiceError,
};

use crate::error::WriterError;

#[async_trait]
pub trait StorageChain: Send + Sync {
    /// The chain the writer sends and lands its transactions on.
    fn submit(&self) -> &dyn ChainSubmit;

    async fn account(&self, address: Address) -> Result<Option<Account>, WriterError>;

    async fn ring_config(&self, ring: Address) -> Result<Option<RingConfiguration>, WriterError>;

    async fn member_openings(
        &self,
        request: GetStorageMemberOpeningsRequest,
    ) -> Result<GetStorageMemberOpeningsResponse, ServiceError>;

    async fn apply_inputs(
        &self,
        request: GetStorageApplyProofInputsRequest,
    ) -> Result<GetStorageApplyProofInputsResponse, WriterError>;

    async fn prove_apply(
        &self,
        inputs: &ApplyProofInputs,
        registry: &StorageRegistry,
    ) -> Result<ApplyRecordsIxData, WriterError>;

    /// The batch's writes applied in order from the current root.
    async fn prove_enrollments(
        &self,
        batch: EnrollBatch<'_>,
    ) -> Result<ProvenEnrollBatch, WriterError>;

    /// Each present owner's leaf under the registry's newest root.
    async fn tracking_leaves(
        &self,
        registry: Address,
        owners: &[[u8; 32]],
    ) -> Result<HashMap<[u8; 32], TrackingLeaf>, WriterError>;
}

impl OpeningFetch for dyn StorageChain {
    fn fetch_openings(
        &self,
        request: GetStorageMemberOpeningsRequest,
    ) -> impl Future<Output = Result<GetStorageMemberOpeningsResponse, ServiceError>> + Send {
        self.member_openings(request)
    }
}

/// `None` until `Initialize` wrote the ring's policy.
pub async fn ring_policy(
    chain: &dyn StorageChain,
    ring: Address,
) -> Result<Option<RingPolicy>, WriterError> {
    let account = chain.account(CustomRing::new(ring).policy_pda()).await?;
    Ok(account.and_then(|account| RingPolicy::decode(&account.owner, &ring, &account.data)))
}

pub async fn storage_registry(
    chain: &dyn StorageChain,
    address: Address,
) -> Result<Option<StorageRegistry>, WriterError> {
    Ok(chain
        .account(address)
        .await?
        .and_then(|account| StorageRegistry::decode(&account.owner, &account.data)))
}

pub struct LiveStorageChain {
    source: ChainSource,
    client: RingClient<AsyncSolanaRpc, AsyncSprIndexer>,
    submit: SolanaChain,
}

impl LiveStorageChain {
    /// The prover serves the ring apply proofs.
    pub fn connect(upstreams: Upstreams<'_>, prover_url: &str) -> Result<Self, WriterError> {
        Ok(Self {
            source: ChainSource::connect(upstreams)?,
            client: live_ring_client(upstreams, prover_url)?,
            submit: SolanaChain::with_timeout(upstreams.rpc_url, upstreams.timeout),
        })
    }

    /// Proves the salted owner writes the record writer serves.
    pub fn client(&self) -> &RingClient<AsyncSolanaRpc, AsyncSprIndexer> {
        &self.client
    }
}

pub fn live_ring_client(
    upstreams: Upstreams<'_>,
    prover_url: &str,
) -> Result<RingClient<AsyncSolanaRpc, AsyncSprIndexer>, WriterError> {
    let zolana = AsyncSprClient::new(
        timed_rpc(upstreams.rpc_url, upstreams.timeout),
        timed_indexer(upstreams.indexer_url, upstreams.timeout)?,
        AsyncProverClient::new(prover_url.to_owned()),
    );
    Ok(RingClient::new(
        zolana,
        AsyncProverClient::new(prover_url.to_owned()),
    ))
}

#[async_trait]
impl StorageChain for LiveStorageChain {
    fn submit(&self) -> &dyn ChainSubmit {
        &self.submit
    }

    async fn account(&self, address: Address) -> Result<Option<Account>, WriterError> {
        Ok(self.client.get_account(address).await?)
    }

    async fn ring_config(&self, ring: Address) -> Result<Option<RingConfiguration>, WriterError> {
        Ok(self.source.ring_config(ring).await?)
    }

    async fn member_openings(
        &self,
        request: GetStorageMemberOpeningsRequest,
    ) -> Result<GetStorageMemberOpeningsResponse, ServiceError> {
        Ok(self.source.storage_member_openings(request).await?)
    }

    async fn apply_inputs(
        &self,
        request: GetStorageApplyProofInputsRequest,
    ) -> Result<GetStorageApplyProofInputsResponse, WriterError> {
        Ok(self
            .client
            .indexer()
            .get_storage_apply_proof_inputs(request)
            .await?)
    }

    async fn prove_apply(
        &self,
        inputs: &ApplyProofInputs,
        registry: &StorageRegistry,
    ) -> Result<ApplyRecordsIxData, WriterError> {
        StorageApplyProof {
            inputs,
            registry,
            asset_writes: &[],
        }
        .prove(&self.client)
        .await
        .map_err(|error| WriterError::StorageUpdate(error.into()))
    }

    async fn prove_enrollments(
        &self,
        batch: EnrollBatch<'_>,
    ) -> Result<ProvenEnrollBatch, WriterError> {
        batch
            .prove(&self.client)
            .await
            .map_err(|error| WriterError::StorageUpdate(error.into()))
    }

    async fn tracking_leaves(
        &self,
        registry: Address,
        owners: &[[u8; 32]],
    ) -> Result<HashMap<[u8; 32], TrackingLeaf>, WriterError> {
        let client = self.client.zolana();
        RegisteredKeys { registry, owners }
            .leaves(client, client)
            .await
            .map_err(|error| AuditError::from(error).into())
    }
}
