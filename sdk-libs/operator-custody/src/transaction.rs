use solana_address::Address;
use solana_message::VersionedMessage;
use solana_signature::Signature;
use solana_signer::{PresignerError, Signer, SignerError};
use solana_transaction::versioned::VersionedTransaction;
use spr_client::sign_transaction;

use crate::{CustodyError, OperatorSigner};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollectedSignature {
    pub signer: Address,
    pub signature: Signature,
}

/// Each required signer must authenticate the same serialized message.
pub fn presigned_transaction(
    message: &VersionedMessage,
    signatures: &[CollectedSignature],
) -> Result<VersionedTransaction, spr_client::ClientError> {
    let signers: Vec<&dyn Signer> = signatures
        .iter()
        .map(|signer| signer as &dyn Signer)
        .collect();
    sign_transaction(message.clone(), &signers)
}

pub async fn signed_transaction(
    message: &VersionedMessage,
    signers: &[&dyn OperatorSigner],
) -> Result<VersionedTransaction, CustodyError> {
    let bytes = message.serialize();
    let mut signatures = Vec::<CollectedSignature>::with_capacity(signers.len());
    for signer in signers {
        let address = signer.address();
        if signatures
            .iter()
            .any(|collected| collected.signer == address)
        {
            continue;
        }
        signatures.push(CollectedSignature {
            signer: address,
            signature: signer.sign(&bytes).await?,
        });
    }
    Ok(presigned_transaction(message, &signatures)?)
}

impl Signer for CollectedSignature {
    fn try_pubkey(&self) -> Result<Address, SignerError> {
        Ok(self.signer)
    }

    fn try_sign_message(&self, message: &[u8]) -> Result<Signature, SignerError> {
        match self.signature.verify(self.signer.as_ref(), message) {
            true => Ok(self.signature),
            false => Err(PresignerError::VerificationFailure.into()),
        }
    }

    fn is_interactive(&self) -> bool {
        false
    }
}
