use solana_address::Address;
use solana_message::VersionedMessage;
use solana_signature::Signature;
use solana_transaction::versioned::VersionedTransaction;
use zolana_ring_client::rpc::{CoSignTransactionRequest, CoSignTransactionResponse};

use ring_service_core::{
    landing::{ChainSubmit, InnerCall, Simulation},
    ServiceError,
};

use super::scope::{ApprovedCall, SignerScope};

use crate::{
    cosign::{CoSignDecision, CoSignError, CoSignPolicy, Sender},
    error::RingCosignerError,
};

#[must_use]
pub struct DirectCoSign<'a> {
    pub chain: &'a dyn ChainSubmit,
    pub policy: &'a dyn CoSignPolicy,
}

impl DirectCoSign<'_> {
    pub async fn sign(
        self,
        request: &CoSignTransactionRequest,
    ) -> Result<CoSignTransactionResponse, RingCosignerError> {
        let signer = self.policy.signer().ok_or(CoSignError::Disabled)?;
        if request.transaction.0.len() > solana_message::v1::MAX_TRANSACTION_SIZE {
            return Err(CoSignError::MessageScope.into());
        }
        let VersionedTransaction {
            signatures,
            message,
        } = wincode::deserialize(&request.transaction.0).map_err(|_| CoSignError::MessageScope)?;
        let scoped = SignerScope {
            ring: request.ring.0,
            signer,
            approved_transfer: request.approved_transfer.clone(),
        }
        .admit(message)?;
        OtherSigners {
            message: scoped.message(),
            signatures: &signatures,
            co_signer: signer,
        }
        .signed()?;
        // A reservation is released past the height, a later one would land uncounted.
        let last_valid_block_height = self
            .chain
            .blockhash_expiry(*scoped.message().recent_blockhash())
            .await?
            .ok_or(CoSignError::StaleBlockhash)?;
        // No rule verifies the ring proof, an unproven frame would hold a window reservation.
        let calls = match self.chain.simulate(scoped.message()).await? {
            Simulation::Passed { calls } => calls,
            Simulation::Refused { reason } => {
                return Err(ServiceError::SendRefused { reason }.into())
            }
        };
        if let Some(approved) = scoped.approved_transfer() {
            approved.ran_once_in(&calls)?;
        }
        let request = scoped.into_request(Sender::Wallet {
            last_valid_block_height,
        });
        match self.policy.decide(&request).await? {
            CoSignDecision::Signed(signature) => Ok(CoSignTransactionResponse {
                signer: signer.into(),
                signature: signature.into(),
            }),
            CoSignDecision::Refused(refusal) => Err(CoSignError::from(refusal).into()),
        }
    }
}

struct OtherSigners<'a> {
    message: &'a VersionedMessage,
    signatures: &'a [Signature],
    co_signer: Address,
}

impl OtherSigners<'_> {
    /// The service simulates and reserves only for a transaction every other required signer signed.
    fn signed(self) -> Result<(), CoSignError> {
        let required = usize::from(self.message.header().num_required_signatures);
        let bytes = self.message.serialize();
        let signed = match self.message.static_account_keys().get(..required) {
            Some(keys) if self.signatures.len() == required => {
                keys.iter().zip(self.signatures).all(|(key, signature)| {
                    *key == self.co_signer || signature.verify(key.as_ref(), &bytes)
                })
            }
            _ => false,
        };
        signed.then_some(()).ok_or(CoSignError::MissingSignature)
    }
}

impl ApprovedCall {
    /// A program can pass simulation and never run the transfer its approval names.
    fn ran_once_in(&self, calls: &[InnerCall]) -> Result<(), CoSignError> {
        let position = self.approval().position;
        let made = calls
            .iter()
            .filter(|call| {
                call.position == position
                    && call.program == self.ring()
                    && call.data == self.data()
                    && call.accounts == self.accounts()
            })
            .count();
        match made {
            1 => Ok(()),
            _ => Err(CoSignError::ApprovedCallNotOnce),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use custom_ring_interface::{tag, TransferApproval};
    use solana_hash::Hash;
    use solana_instruction::{AccountMeta, Instruction};
    use solana_keypair::Keypair;
    use solana_message::{v0, AddressLookupTableAccount, Message};
    use solana_signer::{null_signer::NullSigner, Signer};
    use spr_client::{compile_message, ComputeBudgetConfig};
    use zolana_ring_client::rpc::ApprovedTransfer;
    use zolana_ring_client::CustomRing;

    use super::*;
    use ring_service_core::landing::{ChainOutcome, FinalizedTip, LatestBlockhash};

    use crate::cosign::{CoSignRequest, CoSignSettlement};

    const SIGNER: Address = Address::new_from_array([0x44; 32]);

    fn payer() -> Keypair {
        Keypair::new_from_array([0x55; 32])
    }

    /// Counts what reaches the chain and the signing policy.
    #[derive(Default)]
    struct Reached {
        chain_reads: AtomicUsize,
        decisions: AtomicUsize,
    }

    impl Reached {
        async fn co_sign(
            &self,
            transaction: &VersionedTransaction,
        ) -> Result<Signature, RingCosignerError> {
            DirectCoSign {
                chain: self,
                policy: self,
            }
            .sign(&CoSignTransactionRequest {
                ring: RING.into(),
                transaction: wincode::serialize(transaction).expect("encoded").into(),
                approved_transfer: None,
            })
            .await
            .map(|response| response.signature.0)
        }

        fn chain_reads_and_decisions(&self) -> [usize; 2] {
            [&self.chain_reads, &self.decisions].map(|count| count.load(Ordering::Relaxed))
        }
    }

    fn unsigned(message: VersionedMessage) -> VersionedTransaction {
        VersionedTransaction {
            signatures: vec![
                Signature::default();
                usize::from(message.header().num_required_signatures)
            ],
            message,
        }
    }

    #[async_trait]
    impl ChainSubmit for Reached {
        async fn slot(&self) -> Result<u64, ServiceError> {
            unreachable!("a direct co-sign reads no slot")
        }

        async fn latest_blockhash(&self) -> Result<LatestBlockhash, ServiceError> {
            unreachable!("a direct co-sign compiles no message")
        }

        async fn block_height(&self) -> Result<u64, ServiceError> {
            unreachable!("a direct co-sign tracks no landing")
        }

        async fn account_data(&self, _address: Address) -> Result<Option<Vec<u8>>, ServiceError> {
            unreachable!("a direct co-sign reads no account")
        }

        async fn rent_exempt_lamports(&self, _data_len: usize) -> Result<u64, ServiceError> {
            unreachable!("a direct co-sign pays no rent")
        }

        async fn send(&self, _transaction: &VersionedTransaction) -> Result<(), ServiceError> {
            unreachable!("the wallet sends")
        }

        async fn outcome(
            &self,
            _signature: Signature,
        ) -> Result<Option<ChainOutcome>, ServiceError> {
            unreachable!("a direct co-sign tracks no signature")
        }

        async fn simulate(&self, _message: &VersionedMessage) -> Result<Simulation, ServiceError> {
            self.chain_reads.fetch_add(1, Ordering::Relaxed);
            Ok(Simulation::Passed { calls: Vec::new() })
        }

        async fn blockhash_expiry(&self, _blockhash: Hash) -> Result<Option<u64>, ServiceError> {
            self.chain_reads.fetch_add(1, Ordering::Relaxed);
            Ok(Some(500))
        }
    }

    #[async_trait]
    impl CoSignPolicy for Reached {
        fn signer(&self) -> Option<Address> {
            Some(SIGNER)
        }

        async fn decide(
            &self,
            _request: &CoSignRequest,
        ) -> Result<CoSignDecision, RingCosignerError> {
            self.decisions.fetch_add(1, Ordering::Relaxed);
            Ok(CoSignDecision::Signed(Signature::default()))
        }

        async fn settle(&self, _settlement: CoSignSettlement) -> Result<(), RingCosignerError> {
            Ok(())
        }

        async fn sweep(&self, _tip: FinalizedTip) -> Result<(), RingCosignerError> {
            Ok(())
        }
    }

    /// `[payer, ..signers, record, co-signer]`.
    fn co_signed(signers: &[Address]) -> Instruction {
        let record = CustomRing::new(RING).co_signer_pda(&SIGNER);
        let accounts = signers
            .iter()
            .map(|signer| AccountMeta::new_readonly(*signer, true))
            .chain([
                AccountMeta::new_readonly(record, false),
                AccountMeta::new_readonly(SIGNER, true),
            ]);
        Instruction {
            program_id: RING,
            accounts: core::iter::once(AccountMeta::new(payer().pubkey(), true))
                .chain(accounts)
                .collect(),
            data: vec![tag::TRANSFER],
        }
    }

    fn version_one(instruction: Instruction) -> VersionedMessage {
        compile_message(
            &payer().pubkey(),
            &[instruction],
            Hash::default(),
            ComputeBudgetConfig::new(1),
        )
        .expect("v1 message")
    }

    /// A lookup table could load the ring or a second transfer past every scope check.
    #[tokio::test]
    async fn a_legacy_or_a_table_loading_message_is_refused_before_any_signature() {
        let payer = payer().pubkey();
        let instructions = [co_signed(&[])];
        let table = AddressLookupTableAccount {
            key: Address::new_from_array([0x66; 32]),
            addresses: vec![CustomRing::new(RING).co_signer_pda(&SIGNER)],
        };
        let loading = v0::Message::try_compile(&payer, &instructions, &[table], Hash::default())
            .expect("v0 message");
        assert_eq!(loading.address_table_lookups.len(), 1);

        for refused in [
            VersionedMessage::Legacy(Message::new(&instructions, Some(&payer))),
            VersionedMessage::V0(loading),
            VersionedMessage::V0(
                v0::Message::try_compile(&payer, &instructions, &[], Hash::default())
                    .expect("v0 message"),
            ),
        ] {
            let reached = Reached::default();
            assert!(matches!(
                reached.co_sign(&unsigned(refused)).await,
                Err(RingCosignerError::CoSign(CoSignError::MessageScope))
            ));
            assert_eq!(reached.chain_reads_and_decisions(), [0, 0]);
        }
    }

    #[tokio::test]
    async fn a_transaction_another_signer_has_not_signed_reaches_no_chain_read() {
        let (payer, owner, stranger) = (payer(), Keypair::new(), Keypair::new());
        let message = version_one(co_signed(&[owner.pubkey()]));
        let co_signer = NullSigner::new(&SIGNER);
        let signed = |signers: &[&dyn Signer]| {
            VersionedTransaction::try_new(message.clone(), signers).expect("signed")
        };
        let owner_slot = message
            .static_account_keys()
            .iter()
            .position(|key| *key == owner.pubkey())
            .expect("owner signs");
        let mut foreign = signed(&[&payer, &owner, &co_signer]);
        foreign.signatures[owner_slot] = stranger.sign_message(&message.serialize());

        for refused in [
            unsigned(message.clone()),
            signed(&[&payer, &NullSigner::new(&owner.pubkey()), &co_signer]),
            foreign,
        ] {
            let reached = Reached::default();
            assert!(matches!(
                reached.co_sign(&refused).await,
                Err(RingCosignerError::CoSign(CoSignError::MissingSignature))
            ));
            assert_eq!(reached.chain_reads_and_decisions(), [0, 0]);
        }

        let reached = Reached::default();
        let answer = reached
            .co_sign(&signed(&[&payer, &owner, &co_signer]))
            .await;
        assert_eq!(answer.expect("co-signed"), Signature::default());
        assert_eq!(reached.chain_reads_and_decisions(), [2, 1]);
    }

    const RING: Address = Address::new_from_array([0x11; 32]);
    const ACCOUNTS: [Address; 2] = [
        Address::new_from_array([0x22; 32]),
        Address::new_from_array([0x33; 32]),
    ];
    const DATA: [u8; 2] = [tag::TRANSFER, 8];

    fn call() -> InnerCall {
        InnerCall {
            position: 1,
            program: RING,
            accounts: ACCOUNTS.to_vec(),
            data: DATA.to_vec(),
        }
    }

    #[test]
    fn the_approved_call_runs_exactly_once_under_its_position() {
        let approval = TransferApproval {
            position: 1,
            digest: TransferApproval::digest(&RING, &DATA, &ACCOUNTS).expect("digest"),
        };
        let transfer = ApprovedTransfer {
            data: DATA.to_vec().into(),
            accounts: ACCOUNTS.map(Into::into).to_vec(),
        };
        let approved = ApprovedCall::committed(RING, approval, transfer).expect("committed");
        let unrelated = InnerCall {
            program: ACCOUNTS[0],
            ..call()
        };
        assert_eq!(approved.ran_once_in(&[unrelated.clone(), call()]), Ok(()));

        let reordered = ACCOUNTS.iter().rev().copied().collect();
        for calls in [
            Vec::new(),
            vec![unrelated],
            vec![call(), call()],
            vec![InnerCall {
                position: 0,
                ..call()
            }],
            vec![InnerCall {
                data: vec![tag::TRANSFER],
                ..call()
            }],
            vec![InnerCall {
                accounts: reordered,
                ..call()
            }],
            vec![InnerCall {
                accounts: ACCOUNTS[..1].to_vec(),
                ..call()
            }],
        ] {
            assert_eq!(
                approved.ran_once_in(&calls),
                Err(CoSignError::ApprovedCallNotOnce)
            );
        }
    }
}
