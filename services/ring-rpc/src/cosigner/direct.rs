use solana_message::VersionedMessage;
use zolana_ring_client::rpc::{CoSignTransactionRequest, CoSignTransactionResponse};

use super::scope::{ApprovedCall, SignerScope};
use crate::{
    cosign::{CoSignDecision, CoSignError, CoSignPolicy, Sender},
    error::RingRpcError,
    landing::{ChainSubmit, InnerCall, Simulation},
    relay::RelayError,
};

#[must_use]
pub(crate) struct DirectCoSign<'a> {
    pub chain: &'a dyn ChainSubmit,
    pub policy: &'a dyn CoSignPolicy,
}

impl DirectCoSign<'_> {
    pub async fn sign(
        self,
        request: &CoSignTransactionRequest,
    ) -> Result<CoSignTransactionResponse, RingRpcError> {
        let signer = self.policy.signer().ok_or(CoSignError::Disabled)?;
        if request.message.0.len() > solana_message::v1::MAX_TRANSACTION_SIZE {
            return Err(CoSignError::MessageScope.into());
        }
        let message: VersionedMessage =
            wincode::deserialize(&request.message.0).map_err(|_| CoSignError::MessageScope)?;
        let scoped = SignerScope {
            ring: request.ring.0,
            signer,
            approved_transfer: request.approved_transfer.clone(),
        }
        .admit(message)?;
        // A reservation is released past the height, a later one would land uncounted.
        let last_valid_block_height = self
            .chain
            .blockhash_expiry(*scoped.message().recent_blockhash())
            .await?
            .ok_or(CoSignError::StaleBlockhash)?;
        // No rule verifies the ring proof, an unproven frame would hold a window reservation.
        let calls = match self.chain.simulate(scoped.message()).await? {
            Simulation::Passed { calls } => calls,
            Simulation::Refused { reason } => return Err(RelayError::SendRefused { reason }.into()),
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
    use solana_address::Address;
    use solana_hash::Hash;
    use solana_instruction::{AccountMeta, Instruction};
    use solana_message::{v0, AddressLookupTableAccount, Message};
    use solana_signature::Signature;
    use solana_transaction::versioned::VersionedTransaction;
    use spr_client::{compile_message, ComputeBudgetConfig};
    use zolana_ring_client::rpc::ApprovedTransfer;
    use zolana_ring_client::CustomRing;

    use super::*;
    use crate::{
        cosign::{CoSignRequest, CoSignSettlement},
        landing::{ChainOutcome, FinalizedTip, LatestBlockhash},
    };

    const SIGNER: Address = Address::new_from_array([0x44; 32]);
    const PAYER: Address = Address::new_from_array([0x55; 32]);

    /// Counts what reaches the chain and the signing policy.
    #[derive(Default)]
    struct Reached {
        chain_reads: AtomicUsize,
        decisions: AtomicUsize,
    }

    #[async_trait]
    impl ChainSubmit for Reached {
        async fn slot(&self) -> Result<u64, RingRpcError> {
            unreachable!("a direct co-sign reads no slot")
        }

        async fn latest_blockhash(&self) -> Result<LatestBlockhash, RingRpcError> {
            unreachable!("a direct co-sign compiles no message")
        }

        async fn block_height(&self) -> Result<u64, RingRpcError> {
            unreachable!("a direct co-sign tracks no landing")
        }

        async fn account_data(&self, _address: Address) -> Result<Option<Vec<u8>>, RingRpcError> {
            unreachable!("a direct co-sign reads no account")
        }

        async fn rent_exempt_lamports(&self, _data_len: usize) -> Result<u64, RingRpcError> {
            unreachable!("a direct co-sign pays no rent")
        }

        async fn send(&self, _transaction: &VersionedTransaction) -> Result<(), RingRpcError> {
            unreachable!("the wallet sends")
        }

        async fn outcome(
            &self,
            _signature: Signature,
        ) -> Result<Option<ChainOutcome>, RingRpcError> {
            unreachable!("a direct co-sign tracks no signature")
        }

        async fn simulate(&self, _message: &VersionedMessage) -> Result<Simulation, RingRpcError> {
            self.chain_reads.fetch_add(1, Ordering::Relaxed);
            Ok(Simulation::Passed { calls: Vec::new() })
        }

        async fn blockhash_expiry(&self, _blockhash: Hash) -> Result<Option<u64>, RingRpcError> {
            self.chain_reads.fetch_add(1, Ordering::Relaxed);
            Ok(Some(500))
        }
    }

    #[async_trait]
    impl CoSignPolicy for Reached {
        fn signer(&self) -> Option<Address> {
            Some(SIGNER)
        }

        async fn decide(&self, _request: &CoSignRequest) -> Result<CoSignDecision, RingRpcError> {
            self.decisions.fetch_add(1, Ordering::Relaxed);
            Ok(CoSignDecision::Signed(Signature::default()))
        }

        async fn settle(&self, _settlement: CoSignSettlement) -> Result<(), RingRpcError> {
            Ok(())
        }

        async fn sweep(&self, _tip: FinalizedTip) -> Result<(), RingRpcError> {
            Ok(())
        }
    }

    /// A lookup table could load the ring or a second transfer past every scope check.
    #[tokio::test]
    async fn a_legacy_or_a_table_loading_message_is_refused_before_any_signature() {
        let record = CustomRing::new(RING).co_signer_pda(&SIGNER);
        let co_signed = Instruction {
            program_id: RING,
            accounts: vec![
                AccountMeta::new(PAYER, true),
                AccountMeta::new_readonly(record, false),
                AccountMeta::new_readonly(SIGNER, true),
            ],
            data: vec![tag::TRANSFER],
        };
        let instructions = [co_signed];
        let table = AddressLookupTableAccount {
            key: Address::new_from_array([0x66; 32]),
            addresses: vec![record],
        };
        let loading = v0::Message::try_compile(&PAYER, &instructions, &[table], Hash::default())
            .expect("v0 message");
        assert_eq!(loading.address_table_lookups.len(), 1);
        let sign = |message: VersionedMessage| async move {
            let reached = Reached::default();
            let signed = DirectCoSign {
                chain: &reached,
                policy: &reached,
            }
            .sign(&CoSignTransactionRequest {
                ring: RING.into(),
                message: message.serialize().into(),
                approved_transfer: None,
            })
            .await;
            (
                signed.map(|response| response.signature.0),
                reached.chain_reads.load(Ordering::Relaxed),
                reached.decisions.load(Ordering::Relaxed),
            )
        };

        for refused in [
            VersionedMessage::Legacy(Message::new(&instructions, Some(&PAYER))),
            VersionedMessage::V0(loading),
            VersionedMessage::V0(
                v0::Message::try_compile(&PAYER, &instructions, &[], Hash::default())
                    .expect("v0 message"),
            ),
        ] {
            let (signed, chain_reads, decisions) = sign(refused).await;
            assert!(matches!(
                signed,
                Err(RingRpcError::CoSign(CoSignError::MessageScope))
            ));
            assert_eq!((chain_reads, decisions), (0, 0));
        }

        let version_one = compile_message(
            &PAYER,
            &instructions,
            Hash::default(),
            ComputeBudgetConfig::new(1),
        )
        .expect("v1 message");
        let (signed, chain_reads, decisions) = sign(version_one).await;
        assert_eq!(signed.expect("signed"), Signature::default());
        assert_eq!((chain_reads, decisions), (2, 1));
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
