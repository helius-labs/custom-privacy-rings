use std::{collections::HashMap, future::Future};

use async_trait::async_trait;
use custom_ring_interface::{storage::StorageRegistry, ApplyRecordsIxData};
use custom_ring_sdk::{EnrollBatch, ProvenEnrollBatch, RingClient, StorageApplyProof};
use solana_account::Account;
use solana_address::Address;
use solana_signature::Signature;
use solana_transaction::versioned::VersionedTransaction;
use spr_client::{AsyncProverClient, AsyncRpc, AsyncSolanaRpc, AsyncSprClient, AsyncSprIndexer};
use zolana_ring_client::{AsyncRingIndexer, AuditError, RegisteredKeys, TrackingLeaf};
use zolana_ring_indexer::rpc::{
    GetStorageApplyProofInputsRequest, GetStorageApplyProofInputsResponse,
    GetStorageMemberOpeningsRequest, GetStorageMemberOpeningsResponse,
};
use zolana_ring_member_registry::ApplyProofInputs;

use crate::{
    error::RingRpcError,
    landing::{ChainOutcome, ChainSubmit, LatestBlockhash, Simulation, SolanaChain},
    upstream::{ChainSource, OpeningFetch, RingConfiguration, TransactionSource, Upstreams},
};

#[async_trait]
pub trait StorageChain: ChainSubmit {
    async fn account(&self, address: Address) -> Result<Option<Account>, RingRpcError>;

    async fn ring_config(&self, ring: Address) -> Result<Option<RingConfiguration>, RingRpcError>;

    async fn member_openings(
        &self,
        request: GetStorageMemberOpeningsRequest,
    ) -> Result<GetStorageMemberOpeningsResponse, RingRpcError>;

    async fn apply_inputs(
        &self,
        request: GetStorageApplyProofInputsRequest,
    ) -> Result<GetStorageApplyProofInputsResponse, RingRpcError>;

    async fn prove_apply(
        &self,
        inputs: &ApplyProofInputs,
        registry: &StorageRegistry,
    ) -> Result<ApplyRecordsIxData, RingRpcError>;

    /// The batch's writes applied in order from the current root.
    async fn prove_enrollments(
        &self,
        batch: EnrollBatch<'_>,
    ) -> Result<ProvenEnrollBatch, RingRpcError>;

    /// Each present owner's leaf under the registry's newest root.
    async fn tracking_leaves(
        &self,
        registry: Address,
        owners: &[[u8; 32]],
    ) -> Result<HashMap<[u8; 32], TrackingLeaf>, RingRpcError>;
}

impl OpeningFetch for dyn StorageChain {
    fn fetch_openings(
        &self,
        request: GetStorageMemberOpeningsRequest,
    ) -> impl Future<Output = Result<GetStorageMemberOpeningsResponse, RingRpcError>> + Send {
        self.member_openings(request)
    }
}

pub struct LiveStorageChain {
    source: ChainSource,
    client: RingClient<AsyncSolanaRpc, AsyncSprIndexer>,
    submit: SolanaChain,
}

impl LiveStorageChain {
    /// The prover serves the ring apply proofs.
    pub fn connect(upstreams: Upstreams<'_>, prover_url: &str) -> Result<Self, RingRpcError> {
        Ok(Self {
            source: ChainSource::connect(upstreams)?,
            client: live_ring_client(upstreams, prover_url),
            submit: SolanaChain::new(upstreams.rpc_url),
        })
    }
}

pub fn live_ring_client(
    upstreams: Upstreams<'_>,
    prover_url: &str,
) -> RingClient<AsyncSolanaRpc, AsyncSprIndexer> {
    let zolana = AsyncSprClient::new(
        AsyncSolanaRpc::new(upstreams.rpc_url),
        AsyncSprIndexer::new(upstreams.indexer_url),
        AsyncProverClient::new(prover_url.to_owned()),
    );
    RingClient::new(zolana, AsyncProverClient::new(prover_url.to_owned()))
}

#[async_trait]
impl ChainSubmit for LiveStorageChain {
    async fn account_data(&self, address: Address) -> Result<Option<Vec<u8>>, RingRpcError> {
        self.submit.account_data(address).await
    }

    async fn rent_exempt_lamports(&self, data_len: usize) -> Result<u64, RingRpcError> {
        self.submit.rent_exempt_lamports(data_len).await
    }

    async fn slot(&self) -> Result<u64, RingRpcError> {
        self.submit.slot().await
    }

    async fn latest_blockhash(&self) -> Result<LatestBlockhash, RingRpcError> {
        self.submit.latest_blockhash().await
    }

    async fn block_height(&self) -> Result<u64, RingRpcError> {
        self.submit.block_height().await
    }

    async fn send(&self, transaction: &VersionedTransaction) -> Result<(), RingRpcError> {
        self.submit.send(transaction).await
    }

    async fn outcome(&self, signature: Signature) -> Result<Option<ChainOutcome>, RingRpcError> {
        self.submit.outcome(signature).await
    }

    async fn simulate(
        &self,
        message: &solana_message::VersionedMessage,
    ) -> Result<Simulation, RingRpcError> {
        self.submit.simulate(message).await
    }
}

#[async_trait]
impl StorageChain for LiveStorageChain {
    async fn account(&self, address: Address) -> Result<Option<Account>, RingRpcError> {
        Ok(self.client.get_account(address).await?)
    }

    async fn ring_config(&self, ring: Address) -> Result<Option<RingConfiguration>, RingRpcError> {
        Ok(self.source.ring_config(ring).await?)
    }

    async fn member_openings(
        &self,
        request: GetStorageMemberOpeningsRequest,
    ) -> Result<GetStorageMemberOpeningsResponse, RingRpcError> {
        Ok(self.source.storage_member_openings(request).await?)
    }

    async fn apply_inputs(
        &self,
        request: GetStorageApplyProofInputsRequest,
    ) -> Result<GetStorageApplyProofInputsResponse, RingRpcError> {
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
    ) -> Result<ApplyRecordsIxData, RingRpcError> {
        StorageApplyProof {
            inputs,
            registry,
            asset_writes: &[],
        }
        .prove(&self.client)
        .await
        .map_err(|error| RingRpcError::StorageUpdate(error.into()))
    }

    async fn prove_enrollments(
        &self,
        batch: EnrollBatch<'_>,
    ) -> Result<ProvenEnrollBatch, RingRpcError> {
        batch
            .prove(&self.client)
            .await
            .map_err(|error| RingRpcError::StorageUpdate(error.into()))
    }

    async fn tracking_leaves(
        &self,
        registry: Address,
        owners: &[[u8; 32]],
    ) -> Result<HashMap<[u8; 32], TrackingLeaf>, RingRpcError> {
        let client = self.client.zolana();
        RegisteredKeys { registry, owners }
            .leaves(client, client)
            .await
            .map_err(|error| AuditError::from(error).into())
    }
}
