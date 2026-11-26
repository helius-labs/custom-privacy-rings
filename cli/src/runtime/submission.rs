use anyhow::Result;
use solana_hash::Hash;
use solana_message::VersionedMessage;
use solana_signer::SignerError;
use spr_client::{rpc::compile_message, Rpc};
use thiserror::Error;
use zolana_operator_custody::{
    presigned_transaction, signed_transaction, CollectedSignature, OperatorSigner,
};
use zolana_ring_client::{LocalSigners, SubmissionSigners};

use super::{
    line, Address, Context, Instruction, RingError, RingOperation, RingRpcClient, RingSubmission,
    SettledSubmission, Signature, Signer,
};
use crate::{context::transaction_failure, Zolana};

#[must_use]
pub struct Landing<'a> {
    pub operation: RingOperation<'a>,
    pub payer: &'a (dyn Signer + Sync),
    pub signers: &'a [&'a (dyn Signer + Sync)],
}

#[must_use]
pub(crate) struct MemberSigners<'a> {
    pub wallet: LocalSigners<'a>,
    pub cosigner: Option<Address>,
}

#[must_use]
#[derive(Clone, Copy)]
pub(crate) struct Operators<'a> {
    pub payer: &'a dyn OperatorSigner,
    pub signers: &'a [&'a dyn OperatorSigner],
}

#[must_use]
pub(crate) struct OperatorTransaction<'a> {
    pub operators: Operators<'a>,
    pub instructions: &'a [Instruction],
}

struct CoSigned<'a> {
    wallet: LocalSigners<'a>,
    cosigner: Address,
    ring: Address,
    rpc: RingRpcClient,
}

#[must_use]
struct Settlement<'a, 'operation, S> {
    submission: &'a mut RingSubmission<'operation>,
    signers: &'a S,
}

#[derive(Debug, Error)]
enum CoSignError {
    #[error("the wallet signs as the co-signer {0}, the Ring RPC holds that key")]
    WalletIsCoSigner(Address),
    #[error("the Ring RPC co-signed as {answered}, the ring names {expected}")]
    ForeignCoSigner {
        expected: Address,
        answered: Address,
    },
}

impl Landing<'_> {
    /// Resent until the cluster settles it, a stale root is proved again.
    pub fn settle(self, zolana: &Zolana) -> Result<Signature> {
        Settlement {
            submission: &mut RingSubmission::new(self.operation),
            signers: &LocalSigners {
                payer: self.payer,
                signers: self.signers,
            },
        }
        .settle(zolana)
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
        operators: Operators<'_>,
    ) -> Result<Signature> {
        Settlement {
            submission: &mut RingSubmission::new(operation),
            signers: &operators,
        }
        .settle(&self.zolana)
    }

    pub(crate) fn send_operator(
        &self,
        operators: Operators<'_>,
        instructions: &[Instruction],
    ) -> Result<Signature> {
        OperatorTransaction {
            operators,
            instructions,
        }
        .send(self)
    }
}

impl MemberSigners<'_> {
    pub(crate) fn submit<'a>(
        self,
        ctx: &Context,
        operation: impl Into<RingOperation<'a>>,
    ) -> Result<Signature> {
        self.settle(ctx, &mut RingSubmission::new(operation))
    }

    pub(crate) fn settle(
        self,
        ctx: &Context,
        submission: &mut RingSubmission<'_>,
    ) -> Result<Signature> {
        match self.cosigner {
            Some(cosigner) => Settlement {
                submission,
                signers: &CoSigned {
                    wallet: self.wallet,
                    cosigner,
                    ring: ctx.ring.program_id(),
                    rpc: ctx.ring_rpc()?,
                },
            }
            .settle(&ctx.zolana),
            None => Settlement {
                submission,
                signers: &self.wallet,
            }
            .settle(&ctx.zolana),
        }
    }
}

impl<'a> Operators<'a> {
    pub(crate) fn solo(payer: &'a dyn OperatorSigner) -> Self {
        Self {
            payer,
            signers: &[],
        }
    }

    /// Waits until the transaction confirms or the blockhash of `message` expires.
    pub(crate) fn land(self, ctx: &Context, message: &VersionedMessage) -> Result<Signature> {
        let signers = self.all();
        let transaction =
            ctx.block(async { anyhow::Ok(signed_transaction(message, &signers).await?) })?;
        ctx.rpc()
            .process_transaction(transaction)
            .map_err(transaction_failure)
    }

    fn all(self) -> Vec<&'a dyn OperatorSigner> {
        core::iter::once(self.payer)
            .chain(self.signers.iter().copied())
            .collect()
    }
}

impl OperatorTransaction<'_> {
    pub(crate) fn message(&self, ctx: &Context, blockhash: Hash) -> Result<VersionedMessage> {
        Ok(compile_message(
            &self.operators.payer.address(),
            self.instructions,
            blockhash,
            ctx.budget(self.instructions),
        )?)
    }

    pub(crate) fn send(self, ctx: &Context) -> Result<Signature> {
        let (blockhash, _) = ctx.rpc().get_latest_blockhash()?;
        self.operators.land(ctx, &self.message(ctx, blockhash)?)
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
        signed_transaction(message, &self.all())
            .await
            .map(|transaction| transaction.signatures)
            .map_err(signer_failure)
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
        let expected = self.cosigner;
        if self.wallet.all().any(|signer| signer.pubkey() == expected) {
            return Err(signer_failure(CoSignError::WalletIsCoSigner(expected)));
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
        let (rpc, ring) = (self.rpc.clone(), self.ring);
        let answer = tokio::task::spawn_blocking(move || rpc.co_sign_transaction(ring, bytes))
            .await
            .map_err(signer_failure)?
            .map_err(signer_failure)?;
        signatures.push(named_co_signer(expected, answer).map_err(signer_failure)?);
        presigned_transaction(message, &signatures)
            .map(|transaction| transaction.signatures)
            .map_err(signer_failure)
    }
}

impl<S: SubmissionSigners> Settlement<'_, '_, S> {
    fn settle(self, zolana: &Zolana) -> Result<Signature> {
        let Self {
            submission,
            signers,
        } = self;
        let client = zolana.ring_client();
        let charged = zolana.block_on(submission.prove_with(client, signers))?;
        // A composed transfer can charge an owner for value another party moves out.
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
}

fn named_co_signer(
    expected: Address,
    answer: CollectedSignature,
) -> Result<CollectedSignature, CoSignError> {
    if answer.signer != expected {
        return Err(CoSignError::ForeignCoSigner {
            expected,
            answered: answer.signer,
        });
    }
    Ok(answer)
}

fn signer_failure(error: impl Into<anyhow::Error>) -> RingError {
    SignerError::Custom(format!("{:#}", error.into())).into()
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use solana_instruction::AccountMeta;
    use solana_keypair::Keypair;
    use spr_client::ComputeBudgetConfig;

    use super::*;

    fn message(payer: Address, signers: &[Address]) -> VersionedMessage {
        let instruction = Instruction {
            program_id: Address::new_from_array([9; 32]),
            accounts: signers
                .iter()
                .map(|signer| AccountMeta::new_readonly(*signer, true))
                .collect(),
            data: Vec::new(),
        };
        let budget = ComputeBudgetConfig::new(1_000);
        compile_message(&payer, &[instruction], Hash::default(), budget).expect("message")
    }

    fn signed<S: SubmissionSigners>(
        signers: &S,
        message: &VersionedMessage,
    ) -> Result<Vec<Signature>, RingError> {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime")
            .block_on(signers.sign(message))
    }

    fn unreachable_ring_rpc() -> RingRpcClient {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let url = format!("http://{}", listener.local_addr().expect("address"));
        RingRpcClient::new(&url).expect("loopback")
    }

    #[test]
    fn a_wallet_holding_the_co_signer_key_is_refused_before_the_ring_rpc_is_asked() {
        let (holder, other) = (Keypair::new(), Keypair::new());
        let cosigner = holder.pubkey();
        let message = message(other.pubkey(), &[cosigner]);
        let rpc = unreachable_ring_rpc();
        let wallets: [(&Keypair, &[&(dyn Signer + Sync)]); 2] =
            [(&holder, &[]), (&other, &[&holder])];
        for (payer, signers) in wallets {
            let co_signed = CoSigned {
                wallet: LocalSigners { payer, signers },
                cosigner,
                ring: Address::new_from_array([7; 32]),
                rpc: rpc.clone(),
            };
            let refused = signed(&co_signed, &message).expect_err("wallet key");
            let named = CoSignError::WalletIsCoSigner(cosigner).to_string();
            assert!(refused.to_string().contains(&named), "{refused}");
        }
        let co_signed = CoSigned {
            wallet: LocalSigners {
                payer: &other,
                signers: &[],
            },
            cosigner,
            ring: Address::new_from_array([7; 32]),
            rpc,
        };
        assert_eq!(
            co_signed.additional().collect::<Vec<_>>(),
            [cosigner],
            "the message requires the co-signer"
        );
        let unanswered = signed(&co_signed, &message).expect_err("no ring rpc");
        assert!(!unanswered.to_string().contains("the wallet signs"));
    }

    #[test]
    fn only_the_co_signer_the_ring_names_may_answer() {
        let (expected, answered) = (Keypair::new(), Keypair::new());
        let answer = |keypair: &Keypair| CollectedSignature {
            signer: keypair.pubkey(),
            signature: keypair.sign_message(b"message"),
        };
        assert_eq!(
            named_co_signer(expected.pubkey(), answer(&expected)).expect("named"),
            answer(&expected)
        );
        let refused = named_co_signer(expected.pubkey(), answer(&answered)).expect_err("foreign");
        assert_eq!(
            refused.to_string(),
            format!(
                "the Ring RPC co-signed as {}, the ring names {}",
                answered.pubkey(),
                expected.pubkey()
            )
        );
    }

    #[test]
    fn an_operator_listed_twice_signs_once_and_an_unrequired_operator_is_refused() {
        let (payer, second, stranger) = (Keypair::new(), Keypair::new(), Keypair::new());
        let message = message(payer.pubkey(), &[second.pubkey()]);
        let repeated = Operators {
            payer: &payer,
            signers: &[&payer, &second, &second],
        };
        let signatures = signed(&repeated, &message).expect("signed");
        let bytes = message.serialize();
        assert_eq!(signatures.len(), 2);
        assert!(signatures[0].verify(payer.pubkey().as_ref(), &bytes));
        assert!(signatures[1].verify(second.pubkey().as_ref(), &bytes));
        let unrequired = Operators {
            payer: &payer,
            signers: &[&second, &stranger],
        };
        let refused = signed(&unrequired, &message).expect_err("unrequired");
        let named = format!("the message does not require signer {}", stranger.pubkey());
        assert!(refused.to_string().contains(&named), "{refused}");
    }
}
