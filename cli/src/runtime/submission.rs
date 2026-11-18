use anyhow::Result;
use custom_ring_sdk::{LocalSigners, SubmissionSigners};
use solana_message::VersionedMessage;
use solana_signer::SignerError;
use zolana_operator_custody::{
    presigned_transaction, signed_transaction, CollectedSignature, OperatorSigner,
};
use zolana_ring_client::rpc::{
    CoSignTransactionRequest, CoSignTransactionResponse, CO_SIGN_TRANSACTION,
};

use super::{
    line, Address, Context, RingError, RingOperation, RingRpcClient, RingSubmission,
    SettledSubmission, Signature, Signer,
};
use crate::Zolana;

pub struct Landing<'a> {
    pub operation: RingOperation<'a>,
    pub payer: &'a (dyn Signer + Sync),
    pub signers: &'a [&'a (dyn Signer + Sync)],
}

pub(crate) struct MemberSigners<'a> {
    pub wallet: LocalSigners<'a>,
    pub cosigner: Option<Address>,
}

struct CoSigned<'a> {
    wallet: LocalSigners<'a>,
    cosigner: Address,
    ring: Address,
    service: &'a str,
}

struct Operators<'a> {
    payer: &'a dyn OperatorSigner,
    signers: &'a [&'a dyn OperatorSigner],
}

impl Landing<'_> {
    pub fn settle(self, zolana: &Zolana) -> Result<Signature> {
        settle(
            &mut RingSubmission::new(self.operation),
            zolana,
            &LocalSigners {
                payer: self.payer,
                signers: self.signers,
            },
        )
    }
}

impl Context {
    pub fn submit<'a>(
        &self,
        operation: impl Into<RingOperation<'a>>,
        payer: &(dyn Signer + Sync),
        signers: &[&(dyn Signer + Sync)],
    ) -> Result<Signature> {
        Landing {
            operation: operation.into(),
            payer,
            signers,
        }
        .settle(&self.zolana)
    }

    pub(crate) fn submit_operator<'a>(
        &self,
        operation: impl Into<RingOperation<'a>>,
        payer: &dyn OperatorSigner,
        signers: &[&dyn OperatorSigner],
    ) -> Result<Signature> {
        settle(
            &mut RingSubmission::new(operation),
            &self.zolana,
            &Operators { payer, signers },
        )
    }
}

impl MemberSigners<'_> {
    pub(crate) fn settle(
        self,
        ctx: &Context,
        submission: &mut RingSubmission<'_>,
    ) -> Result<Signature> {
        match self.cosigner {
            Some(cosigner) => settle(
                submission,
                &ctx.zolana,
                &CoSigned {
                    wallet: self.wallet,
                    cosigner,
                    ring: ctx.ring.program_id(),
                    service: &ctx.config.urls().ring_rpc,
                },
            ),
            None => settle(submission, &ctx.zolana, &self.wallet),
        }
    }
}

impl SubmissionSigners for CoSigned<'_> {
    fn payer(&self) -> Address {
        self.wallet.payer()
    }

    fn additional(&self) -> impl Iterator<Item = Address> {
        self.wallet
            .additional()
            .chain(core::iter::once(self.cosigner))
    }

    async fn sign(&self, message: &VersionedMessage) -> Result<Vec<Signature>, RingError> {
        if self
            .wallet
            .all()
            .any(|signer| signer.pubkey() == self.cosigner)
        {
            return Err(SignerError::KeypairPubkeyMismatch.into());
        }
        let bytes = message.serialize();
        let mut signatures = self
            .wallet
            .all()
            .map(|signer| {
                Ok(CollectedSignature {
                    signer: signer.try_pubkey()?,
                    signature: signer.try_sign_message(&bytes)?,
                })
            })
            .collect::<Result<Vec<_>, SignerError>>()?;
        let request = CoSignTransactionRequest {
            ring: self.ring.into(),
            message: bytes.into(),
        };
        let service = self.service.to_owned();
        let answer: CoSignTransactionResponse = tokio::task::spawn_blocking(move || {
            RingRpcClient::new(&service).call(CO_SIGN_TRANSACTION, request)
        })
        .await
        .map_err(|error| SignerError::Custom(error.to_string()))?
        .map_err(|error| SignerError::Custom(error.to_string()))?;
        if answer.signer.0 != self.cosigner {
            return Err(SignerError::KeypairPubkeyMismatch.into());
        }
        signatures.push(CollectedSignature {
            signer: self.cosigner,
            signature: answer.signature.0,
        });
        Ok(presigned_transaction(message, &signatures)?.signatures)
    }
}

impl SubmissionSigners for Operators<'_> {
    fn payer(&self) -> Address {
        self.payer.address()
    }

    fn additional(&self) -> impl Iterator<Item = Address> {
        self.signers.iter().map(|signer| signer.address())
    }

    async fn sign(&self, message: &VersionedMessage) -> Result<Vec<Signature>, RingError> {
        let signers: Vec<_> = self.all().collect();
        signed_transaction(message, &signers)
            .await
            .map(|transaction| transaction.signatures)
            .map_err(|error| SignerError::Custom(error.to_string()).into())
    }
}

impl Operators<'_> {
    fn all(&self) -> impl Iterator<Item = &dyn OperatorSigner> {
        core::iter::once(self.payer).chain(self.signers.iter().copied())
    }
}

fn settle(
    submission: &mut RingSubmission<'_>,
    zolana: &Zolana,
    signers: &impl SubmissionSigners,
) -> Result<Signature> {
    let client = zolana.ring_client();
    let charged = zolana.block_on(submission.prove_with(client, signers))?;
    for flow in charged.iter().filter(|flow| flow.charge != 0) {
        line(
            "charged outflow",
            format_args!(
                "owner {} asset {} {} with {} charged beyond its debit",
                hex::encode(flow.owner),
                hex::encode(flow.asset),
                flow.charged_outflow,
                flow.charge
            ),
        );
    }
    match zolana.block_on(submission.send_until_settled_with(client, signers))? {
        SettledSubmission::Confirmed { signature, .. } => Ok(signature),
        SettledSubmission::Failed { signature, error } => {
            Err(RingError::Rejected { signature, error }.into())
        }
    }
}
