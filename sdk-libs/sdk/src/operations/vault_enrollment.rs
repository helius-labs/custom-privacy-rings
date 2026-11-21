use custom_ring_interface::{
    pda_enrollment::{EnrollmentOwner, ENROLL_PDA},
    InstructionTag,
};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_message::VersionedMessage;
use solana_signature::Signature;
use solana_signer::Signer;
use spr_api::SprApi;
use spr_client::{compile_message, AsyncRpc, ClientError};
use spr_indexer_api::Base64String;
use zolana_ring_client::rpc::{
    PrepareVaultEnrollment, SignedVaultEnrollment, SubmitVaultEnrollment, VaultEnrollmentBlock,
    VaultEnrollmentRequest,
};

use crate::{
    storage::EnrollmentAccounts,
    submission::{check_transaction_size, outcome, send_budget, Outcome, PENDING_POLL},
    CustomRing, Enroll, RingClient, RingError, RingIndexerBackend, SignatureHistory,
};

pub struct EnrollVault<'a> {
    pub enrollment: Enroll<'a>,
    pub program: Address,
    pub manager: &'a dyn Signer,
}

pub struct VaultEnrollmentMessage<'a> {
    pub request: &'a VaultEnrollmentRequest,
    pub writer: Address,
    pub registry: Address,
}

impl EnrollVault<'_> {
    pub async fn send<R: AsyncRpc + SignatureHistory, I: RingIndexerBackend>(
        self,
        writer_service: &SprApi,
        client: &RingClient<R, I>,
    ) -> Result<Signature, RingError> {
        let change = self.enrollment.prove(client).await?;
        let config = change.ring.require_config(client.zolana()).await?;
        let writer = config.account().roles.enrollment_writer;
        let inner = change.prove_cpi(writer, client).await?;
        let request = VaultEnrollmentRequest {
            ring: change.ring.program_id().into(),
            program: self.program.into(),
            manager: self.manager.try_pubkey()?.into(),
            vault: change.owner.into(),
            payload: Base64String(inner.data),
        };
        let block = writer_service
            .call::<PrepareVaultEnrollment>(request.clone())
            .await
            .map_err(|error| ClientError::Rpc(error.to_string()))?;
        let message = VaultEnrollmentMessage {
            request: &request,
            writer,
            registry: change.tracking_registry,
        }
        .compile(&block)?;
        let bytes = message.serialize();
        let signature = self.manager.try_sign_message(&bytes)?;
        let sent = writer_service
            .call::<SubmitVaultEnrollment>(SignedVaultEnrollment {
                enrollment: request,
                block: block.clone(),
                signature: signature.into(),
            })
            .await
            .map_err(|error| ClientError::Rpc(error.to_string()))?
            .0;
        if !sent.verify(writer.as_ref(), &bytes) {
            return Err(RingError::WriterSignature { writer });
        }
        loop {
            match outcome(client.zolana(), sent, block.last_valid_block_height).await? {
                Outcome::Confirmed { .. } => return Ok(sent),
                Outcome::Unknown => tokio::time::sleep(PENDING_POLL).await,
                Outcome::Stale(error) | Outcome::Failed(error) => {
                    return Err(RingError::Rejected {
                        signature: sent,
                        error,
                    })
                }
            }
        }
    }
}

impl VaultEnrollmentMessage<'_> {
    pub fn compile(self, block: &VaultEnrollmentBlock) -> Result<VersionedMessage, RingError> {
        let request = self.request;
        let ring = CustomRing::new(request.ring.0);
        let manager = request.manager.0;
        if manager == self.writer || !manager.is_on_curve() {
            return Err(RingError::VaultManager { manager });
        }
        EnrollmentOwner {
            owner: &request.vault.0,
            ring_authority: &ring.ring_auth_pda(),
        }
        .parse(&request.payload.0)?;
        let mut accounts = EnrollmentAccounts {
            ring,
            writer: self.writer,
            tracking_registry: self.registry,
            authorization: AccountMeta::new_readonly(request.vault.0, false),
        }
        .metas();
        accounts.insert(0, AccountMeta::new_readonly(manager, true));
        accounts.push(AccountMeta::new_readonly(ring.program_id(), false));
        let instruction = Instruction {
            program_id: request.program.0,
            accounts,
            data: [ENROLL_PDA.as_slice(), &request.payload.0].concat(),
        };
        let budget = send_budget(InstructionTag::EnrollBatch, None);
        check_transaction_size(&self.writer, &instruction, budget)?;
        Ok(compile_message(
            &self.writer,
            &[instruction],
            block.blockhash.0.into(),
            budget,
        )?)
    }
}
