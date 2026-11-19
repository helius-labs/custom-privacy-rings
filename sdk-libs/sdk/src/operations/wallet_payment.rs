use solana_address::Address;
use solana_signer::Signer;
use spr_client::{AsyncRpc, ReservedKeys, RingTransferRequest, RingTransfers};
use spr_transaction::instructions::transact::ConfidentialTransaction;
use zolana_ring_keys::RingAddress;

use crate::{
    client::{RingClient, RingIndexerBackend},
    pipeline::{
        AnonymousPayment, AnonymousTransfer, CustomRingTransfer, CustomRingTransferInput, NoteData,
        OwnerSource,
    },
    submission::RingOperation,
    CustomRing, RingError, RingSubmission, SignatureHistory,
};

#[cfg(test)]
#[path = "../tests/operations/wallet_payment.rs"]
mod tests;

pub struct RingProving<'a, R, I> {
    pub client: &'a RingClient<R, I>,
    pub ring: CustomRing,
    pub payer: &'a (dyn Signer + Sync),
    /// Signers beyond `payer`, the owner rail's owner when it is not `payer`.
    pub signers: &'a [&'a (dyn Signer + Sync)],
}

pub struct OwnerRingTransfers<'a, R, I>(pub RingProving<'a, R, I>);

/// Core's owner signs inside the proof, a public signer under its key is refused.
pub struct AnonymousRingTransfers<'a, R, I> {
    pub proving: RingProving<'a, R, I>,
    /// A payee missing here must be under the zero key, its list salt is zero.
    pub payees: &'a [RingAddress],
}

impl<R, I> RingProving<'_, R, I> {
    fn transfer<'r>(
        &self,
        keys: &'r ReservedKeys<'_>,
        transaction: ConfidentialTransaction,
    ) -> CustomRingTransfer<'r> {
        CustomRingTransfer::new(CustomRingTransferInput {
            ring: self.ring,
            sender: keys,
            nullifier_key: None,
            transaction,
        })
        .with_proof_authority(keys)
    }
}

impl<R: AsyncRpc + SignatureHistory, I: RingIndexerBackend> RingProving<'_, R, I> {
    /// A refusal a fresh proof fixes needs a new prepare.
    async fn prove<'r>(
        &self,
        operation: impl Into<RingOperation<'r>>,
    ) -> Result<RingSubmission<'static>, RingError> {
        let mut submission = RingSubmission::new(operation);
        submission
            .prove(self.client, self.payer, self.signers)
            .await?;
        submission.detached()
    }
}

impl<R, I> OwnerRingTransfers<'_, R, I> {
    fn transfer<'r>(
        &self,
        request: &'r RingTransferRequest<'_>,
    ) -> Result<CustomRingTransfer<'r>, RingError> {
        // Core reserves only default-ring notes.
        if request.authorizer.owner().is_some() {
            return Err(RingError::InProofDefaultRingInput { index: 0 });
        }
        let payment = &request.payment;
        let mut transaction = request.transaction.clone();
        transaction.transfer_with_ring(
            &payment.recipient,
            payment.asset,
            payment.amount,
            Some(self.0.ring.program_id()),
        )?;
        Ok(self
            .0
            .transfer(&request.keys, transaction)
            .with_authorizer(&request.authorizer))
    }
}

impl<R: AsyncRpc + SignatureHistory, I: RingIndexerBackend> RingTransfers
    for OwnerRingTransfers<'_, R, I>
{
    type Error = RingError;
    type Prepared = RingSubmission<'static>;

    fn ring_program_id(&self) -> Address {
        self.0.ring.program_id()
    }

    async fn prepare(
        &self,
        request: RingTransferRequest<'_>,
    ) -> Result<RingSubmission<'static>, RingError> {
        self.0.prove(self.transfer(&request)?).await
    }
}

impl<R, I> AnonymousRingTransfers<'_, R, I> {
    fn transfer<'r>(
        &self,
        request: &'r RingTransferRequest<'_>,
    ) -> Result<AnonymousTransfer<'r>, RingError> {
        let owner = request
            .authorizer
            .owner()
            .ok_or(RingError::MissingAnonymousSigner)?;
        let payment = &request.payment;
        let ring_program_id = self.proving.ring.program_id();
        let known = self.payees.iter().find(|address| {
            address.recipient == payment.recipient && address.ring_program_id == ring_program_id
        });
        let to = match known {
            Some(address) => address.clone(),
            None => {
                RingAddress::check_list_salt(&payment.recipient, &[0; 32])?;
                RingAddress {
                    recipient: payment.recipient,
                    ring_program_id,
                    list_salt: [0; 32],
                }
            }
        };
        self.proving
            .transfer(&request.keys, request.transaction.clone())
            .with_anonymous_owner(OwnerSource::Authorized {
                authorizer: &request.authorizer,
                owner,
            })
            .with_payment(AnonymousPayment {
                to,
                asset: payment.asset,
                amount: payment.amount,
                reply: None,
                data: NoteData::default(),
            })
    }
}

impl<R: AsyncRpc + SignatureHistory, I: RingIndexerBackend> RingTransfers
    for AnonymousRingTransfers<'_, R, I>
{
    type Error = RingError;
    type Prepared = RingSubmission<'static>;

    fn ring_program_id(&self) -> Address {
        self.proving.ring.program_id()
    }

    async fn prepare(
        &self,
        request: RingTransferRequest<'_>,
    ) -> Result<RingSubmission<'static>, RingError> {
        self.proving.prove(self.transfer(&request)?).await
    }
}
