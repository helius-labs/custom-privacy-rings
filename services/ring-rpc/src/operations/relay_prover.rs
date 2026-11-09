use async_trait::async_trait;
use custom_ring_interface::{compute_unit_limit, InstructionTag};
use custom_ring_sdk::{
    written_tree_fees, AllowedNullifiers, CustomRing, DelegateMove, DelegatePayment,
    DelegateSource, DelegateWithdrawal, RelayedTransferInput, RingClient, RingIndexerBackend,
    RingTransfer,
};
use solana_address::Address;
use solana_instruction::Instruction;
use spr_client::{AsyncRpc, ComputeBudgetConfig};
use spr_keypair::{NullifierKey, ShieldedAddress, ViewingKey};
use spr_transaction::{AssetRegistry, WalletUtxo};
use zolana_ring_client::rpc::{RelayMergeInputs, RelayProofInputs, RelayRail};

use crate::{
    error::RingRpcError,
    escrow_merge::OwnerMergeProof,
    managed_merge::OwnerMerge,
    relay::{RelayError, RelayPayload},
};

pub struct RelayProofRequest<'a> {
    pub ring: Address,
    pub parties: RelayParties,
    pub payload: &'a RelayPayload,
    /// Verified over `SppProofInputs::message_hash`, the anonymous and confidential proofs take it.
    pub owner_signature: Option<[u8; 64]>,
}

/// The source's notes open under the escrowed key, the source signs nothing.
#[must_use]
pub struct DelegateSeal<'a> {
    pub ring: Address,
    pub delegate: Address,
    pub sources: &'a [SealedSource],
    /// Admitted from `sources`, the relay holds the sealed inputs to it before proving.
    pub allowed: &'a AllowedNullifiers,
    pub tx_viewing_key: ViewingKey,
    pub payments: Vec<DelegatePayment>,
    pub exits: Vec<DelegatePayment>,
    pub withdrawals: Vec<DelegateWithdrawal>,
    pub assets: &'a AssetRegistry,
}

/// One member the service holds the key and opened notes of.
pub struct SealedSource {
    pub address: ShieldedAddress,
    pub nullifier_key: NullifierKey,
    pub notes: Vec<WalletUtxo>,
}

/// Read from the relay, never from the request.
#[derive(Clone)]
pub struct RelayParties {
    pub fee_payer: Address,
    /// In run order, read while a demanded class stays uncovered.
    pub co_signers: Vec<Address>,
}

pub struct ProvenRelay {
    pub instruction: Instruction,
    pub budget: ComputeBudgetConfig,
    pub co_signers: Vec<Address>,
    /// The owners, the delegate or the funders, each signs the returned message.
    pub signers: Vec<Address>,
    /// SPP takes it from the fee payer only when the message lands.
    pub tree_fees: u64,
}

#[async_trait]
pub trait RelayProver: Send + Sync {
    async fn prove(&self, request: RelayProofRequest<'_>) -> Result<ProvenRelay, RingRpcError>;

    /// Selects the notes and seals the delegate's move, `prove` proves it.
    async fn seal_delegate(
        &self,
        seal: DelegateSeal<'_>,
        parties: RelayParties,
    ) -> Result<RelayProofInputs, RingRpcError>;
}

pub struct SdkRelayProver<R, I> {
    client: RingClient<R, I>,
}

impl<R, I> SdkRelayProver<R, I> {
    pub fn new(client: RingClient<R, I>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<R: AsyncRpc + 'static, I: RingIndexerBackend + 'static> RelayProver for SdkRelayProver<R, I> {
    async fn prove(&self, request: RelayProofRequest<'_>) -> Result<ProvenRelay, RingRpcError> {
        match request.payload {
            RelayPayload::Transfer(proof_inputs) | RelayPayload::Deposit(proof_inputs) => {
                self.transfer(&request, proof_inputs).await
            }
            RelayPayload::Merge(merge) => self.merge(&request, merge).await,
        }
    }

    async fn seal_delegate(
        &self,
        seal: DelegateSeal<'_>,
        parties: RelayParties,
    ) -> Result<RelayProofInputs, RingRpcError> {
        for withdrawal in &seal.withdrawals {
            if let Some(program) = withdrawal.token_program {
                let token_program = [
                    spr_interface::SPL_TOKEN_PROGRAM_ID,
                    spr_interface::SPL_TOKEN_2022_PROGRAM_ID,
                ]
                .contains(program.as_array());
                let mint = self.client.zolana().get_account(withdrawal.asset).await?;
                if !token_program || mint.is_none_or(|mint| mint.owner != program) {
                    return Err(
                        crate::authority_transfer::AuthorityTransferError::WithdrawalMint.into(),
                    );
                }
            }
        }
        let transfer = DelegateMove {
            ring: CustomRing::new(seal.ring),
            delegate: seal.delegate,
            payer: parties.fee_payer,
            sources: seal
                .sources
                .iter()
                .map(|source| DelegateSource {
                    address: source.address,
                    nullifier_key: &source.nullifier_key,
                    utxos: &source.notes,
                })
                .collect(),
            payments: seal.payments,
            exits: seal.exits,
            withdrawals: seal.withdrawals,
            assets: seal.assets,
            tx_viewing_key: seal.tx_viewing_key,
        }
        .select(&self.client)
        .await
        .map_err(RelayError::proof)?;
        transfer
            .relay_proof_inputs(self.client.zolana())
            .await
            .map_err(RelayError::proof)
    }
}

impl<R: AsyncRpc + 'static, I: RingIndexerBackend + 'static> SdkRelayProver<R, I> {
    async fn transfer(
        &self,
        request: &RelayProofRequest<'_>,
        proof_inputs: &RelayProofInputs,
    ) -> Result<ProvenRelay, RingRpcError> {
        let mut transfer = RingTransfer::relayed(RelayedTransferInput {
            ring: CustomRing::new(request.ring),
            proof_inputs,
            fee_payer: request.parties.fee_payer,
        });
        if let Some(signature) = request.owner_signature {
            transfer = transfer.with_owner_signature(signature);
        }
        for co_signer in &request.parties.co_signers {
            transfer = transfer.with_co_signer(*co_signer);
        }
        let proven = transfer
            .prove(&self.client)
            .await
            .map_err(RelayError::proof)?;
        let (tag, delegate) = match proof_inputs.rail {
            RelayRail::Delegate { delegate, .. } => {
                (InstructionTag::AuthorityTransfer, Some(delegate))
            }
            _ => (InstructionTag::Transfer, None),
        };
        let tree_fees = written_tree_fees(self.client.zolana(), proven.written_trees())
            .await
            .map_err(RelayError::proof)?;
        Ok(ProvenRelay {
            instruction: proven.instruction().map_err(RelayError::proof)?,
            budget: ComputeBudgetConfig::new(compute_unit_limit(tag)),
            co_signers: proven.co_signers.clone(),
            signers: proven
                .owner_signers()
                .iter()
                .copied()
                .chain(delegate)
                .chain(
                    proven
                        .deposit_funders()
                        .into_iter()
                        .filter(|funder| *funder != request.parties.fee_payer),
                )
                .collect(),
            tree_fees,
        })
    }

    async fn merge(
        &self,
        request: &RelayProofRequest<'_>,
        merge: &RelayMergeInputs,
    ) -> Result<ProvenRelay, RingRpcError> {
        let owner = merge
            .sender
            .signing_pubkey
            .as_ed25519()
            .map_err(|error| RelayError::proof(error.into()))?;
        let proven = OwnerMergeProof {
            client: &self.client,
            payer: request.parties.fee_payer,
        }
        .prove(OwnerMerge {
            ring: merge.ring_program_id,
            owner: Address::new_from_array(owner),
            nullifier_key: &merge.nullifier_key,
            inputs: merge.inputs.clone(),
        })
        .await
        .map_err(|error| RelayError::proof(*error))?;
        let tree_fees = written_tree_fees(self.client.zolana(), proven.written_trees())
            .await
            .map_err(RelayError::proof)?;
        Ok(ProvenRelay {
            instruction: proven
                .instruction(request.parties.fee_payer)
                .map_err(RelayError::proof)?,
            budget: ComputeBudgetConfig::new(compute_unit_limit(InstructionTag::Merge)),
            co_signers: Vec::new(),
            signers: Vec::new(),
            tree_fees,
        })
    }
}
