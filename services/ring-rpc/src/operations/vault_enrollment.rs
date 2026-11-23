use std::collections::HashMap;

use custom_ring_sdk::VaultEnrollmentMessage;
use solana_address::Address;
use solana_hash::Hash;
use solana_signature::Signature;
use spr_indexer_api::SerializableSignature;
use zolana_ring_client::rpc::{
    SignedVaultEnrollment, VaultEnrollmentBlock, VaultEnrollmentRequest,
};

use super::{
    ring_tracking, EnrollmentApplier, EnrollmentBudget, EnrollmentRefusal, InFlight, RingRpcError,
    RingState, RingTracking,
};
use crate::{
    landing::{Attempt, Simulation},
    relay_submit::{presigned_transaction, CollectedSignature},
};

#[derive(Default)]
pub(super) struct VaultAttempts {
    blocks: HashMap<Hash, u64>,
    submissions: HashMap<Signature, u64>,
}

impl VaultAttempts {
    fn expire(&mut self, height: u64) {
        self.blocks.retain(|_, expiry| *expiry >= height);
        self.submissions.retain(|_, expiry| *expiry >= height);
    }
}

impl EnrollmentApplier {
    #[must_use]
    pub fn with_vault_programs(mut self, programs: impl IntoIterator<Item = Address>) -> Self {
        self.vault_programs.extend(programs);
        self
    }

    async fn vault_tracking(
        &self,
        request: &VaultEnrollmentRequest,
    ) -> Result<RingTracking, RingRpcError> {
        let ring = request.ring.0;
        if !self.vault_programs.contains(&request.program.0)
            || !(self.named.contains(&ring) || self.registered().contains(&ring))
        {
            return Err(EnrollmentRefusal::NotServed.into());
        }
        ring_tracking(self.chain.as_ref(), ring)
            .await?
            .filter(|tracking| tracking.writer == self.payer())
            .ok_or_else(|| EnrollmentRefusal::RingMissing.into())
    }

    pub async fn prepare_vault(
        &self,
        request: VaultEnrollmentRequest,
    ) -> Result<VaultEnrollmentBlock, RingRpcError> {
        let tracking = self.vault_tracking(&request).await?;
        let ring = request.ring.0;
        if !self.budget.holds(ring, EnrollmentBudget::vault_fee()) {
            return Err(EnrollmentRefusal::Budget.into());
        }
        let latest = self.chain.latest_blockhash().await?;
        let block = VaultEnrollmentBlock {
            blockhash: spr_indexer_api::Hash(latest.blockhash.to_bytes()),
            last_valid_block_height: latest.last_valid_block_height,
        };
        VaultEnrollmentMessage {
            request: &request,
            writer: self.payer(),
            registry: tracking.registry,
        }
        .compile(&block)
        .map_err(|_| EnrollmentRefusal::InvalidWrite)?;
        let height = self.chain.block_height().await?;
        let state = self.ring_state(ring);
        let mut state = state.lock().await;
        state.vault.expire(height);
        state
            .vault
            .blocks
            .insert(latest.blockhash, latest.last_valid_block_height);
        Ok(block)
    }

    pub async fn submit_vault(
        &self,
        signed: SignedVaultEnrollment,
    ) -> Result<SerializableSignature, RingRpcError> {
        let request = &signed.enrollment;
        let tracking = self.vault_tracking(request).await?;
        let ring = request.ring.0;
        let message = VaultEnrollmentMessage {
            request,
            writer: self.payer(),
            registry: tracking.registry,
        }
        .compile(&signed.block)
        .map_err(|_| EnrollmentRefusal::InvalidWrite)?;
        let bytes = message.serialize();
        let signature = signed.signature.0;
        if !signature.verify(request.manager.0.as_ref(), &bytes) {
            return Err(EnrollmentRefusal::InvalidWrite.into());
        }
        let state = self.ring_state(ring);
        let mut state = state.lock().await;
        let RingState {
            vault, in_flight, ..
        } = &mut *state;
        let height = self.chain.block_height().await?;
        vault.expire(height);
        let expiry = signed.block.last_valid_block_height;
        if vault
            .blocks
            .get(&Hash::new_from_array(signed.block.blockhash.0))
            != Some(&expiry)
        {
            return Err(EnrollmentRefusal::InvalidWrite.into());
        }
        let writer_signature = self.payer.sign(&bytes).await?;
        let transaction = presigned_transaction(
            &message,
            &[
                CollectedSignature {
                    signer: self.payer(),
                    signature: writer_signature,
                },
                CollectedSignature {
                    signer: request.manager.0,
                    signature,
                },
            ],
        )?;
        if vault.submissions.contains_key(&writer_signature) {
            if self.chain.outcome(writer_signature).await?.is_none() {
                self.chain.send(&transaction).await?;
            }
            return Ok(writer_signature.into());
        }
        // A second proof from the same root could not land.
        if in_flight.is_some() {
            return Err(EnrollmentRefusal::InFlight.into());
        }
        if !matches!(self.chain.simulate(&message).await?, Simulation::Passed) {
            return Err(EnrollmentRefusal::InvalidWrite.into());
        }
        if !self.budget.charge(ring, EnrollmentBudget::vault_fee()) {
            return Err(EnrollmentRefusal::Budget.into());
        }
        vault.submissions.insert(writer_signature, expiry);
        *in_flight = Some(InFlight {
            attempt: Attempt {
                signature: writer_signature,
                last_valid_block_height: expiry,
            },
            writes: Vec::new(),
        });
        self.chain.send(&transaction).await?;
        Ok(writer_signature.into())
    }
}
