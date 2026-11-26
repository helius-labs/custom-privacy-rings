/// Modelled at the base fee, the service sets no priority fee.
pub(crate) fn signature_fees(signatures: usize) -> u64 {
    const LAMPORTS_PER_SIGNATURE: u64 = 5_000;
    u64::try_from(signatures)
        .unwrap_or(u64::MAX)
        .saturating_mul(LAMPORTS_PER_SIGNATURE)
}

use std::{str::FromStr, time::Duration};

use async_trait::async_trait;
use custom_ring_sdk::SignatureHistory;
use solana_address::Address;
use solana_commitment_config::{CommitmentConfig, CommitmentLevel};
use solana_hash::Hash;
use solana_message::VersionedMessage;
use solana_rpc_client_api::{
    client_error::{Error as RpcClientError, ErrorKind},
    config::RpcSimulateTransactionConfig,
    custom_error::JSON_RPC_SERVER_ERROR_SEND_TRANSACTION_PREFLIGHT_FAILURE,
    request::RpcError,
};
use solana_signature::Signature;
use solana_transaction::versioned::VersionedTransaction;
use solana_transaction_error::TransactionError;
use solana_transaction_status_client_types::{
    TransactionConfirmationStatus, TransactionStatus, UiInnerInstructions, UiInstruction,
    UiParsedInstruction,
};
use spr_client::{AsyncRpc, AsyncSolanaRpc, ClientError, RpcSendTransactionConfig};

use crate::{error::RingRpcError, relay::RelayError, upstream::timed_rpc};

#[async_trait]
pub trait ChainSubmit: Send + Sync {
    async fn slot(&self) -> Result<u64, RingRpcError>;

    async fn latest_blockhash(&self) -> Result<LatestBlockhash, RingRpcError>;

    /// Finalized height, pending signatures cannot expire against a newer fork.
    async fn block_height(&self) -> Result<u64, RingRpcError>;

    async fn account_data(&self, address: Address) -> Result<Option<Vec<u8>>, RingRpcError>;

    async fn rent_exempt_lamports(&self, data_len: usize) -> Result<u64, RingRpcError>;

    /// A transaction the cluster already processed counts as sent, one its preflight refuses fails.
    async fn send(&self, transaction: &VersionedTransaction) -> Result<(), RingRpcError>;

    /// `None` until success or failure is finalized.
    async fn outcome(&self, signature: Signature) -> Result<Option<ChainOutcome>, RingRpcError>;

    /// Signatures stay unchecked, every program and proof check runs.
    async fn simulate(&self, message: &VersionedMessage) -> Result<Simulation, RingRpcError>;

    /// A height no block past lands a message under `blockhash`, `None` for a hash the cluster
    /// does not hold.
    async fn blockhash_expiry(&self, _blockhash: Hash) -> Result<Option<u64>, RingRpcError> {
        Err(RingRpcError::StateUnavailable)
    }

    /// One finalized bank, its slot and its height read together.
    async fn finalized_tip(&self) -> Result<FinalizedTip, RingRpcError> {
        Err(RingRpcError::StateUnavailable)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinalizedTip {
    pub slot: u64,
    pub block_height: u64,
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Simulation {
    Passed { calls: Vec<InnerCall> },
    Refused { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InnerCall {
    /// Index of the top-level instruction the call ran under.
    pub position: u8,
    pub program: Address,
    pub accounts: Vec<Address>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainOutcome {
    Confirmed { slot: u64 },
    Failed { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatestBlockhash {
    pub blockhash: Hash,
    pub last_valid_block_height: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Attempt {
    pub signature: Signature,
    pub last_valid_block_height: u64,
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Landing {
    Pending,
    Landed(ChainOutcome),
    /// Final, the blockhash can no longer land the signature.
    Expired,
}

pub struct SolanaChain {
    rpc: AsyncSolanaRpc,
}

impl From<(Hash, u64)> for LatestBlockhash {
    fn from((blockhash, last_valid_block_height): (Hash, u64)) -> Self {
        Self {
            blockhash,
            last_valid_block_height,
        }
    }
}

impl InnerCall {
    /// `None` on an index outside the message keys or text outside base58.
    fn decode(
        message: &VersionedMessage,
        simulated: Vec<UiInnerInstructions>,
    ) -> Option<Vec<Self>> {
        let keys = message.static_account_keys();
        let key = |index: &u8| keys.get(usize::from(*index)).copied();
        let address = |text: &String| Address::from_str(text).ok();
        let bytes = |text: &str| bs58::decode(text).into_vec().ok();
        let mut calls = Vec::new();
        for group in simulated {
            for instruction in group.instructions {
                let (program, accounts, data) = match instruction {
                    UiInstruction::Compiled(call) => (
                        key(&call.program_id_index)?,
                        call.accounts.iter().map(key).collect::<Option<_>>()?,
                        bytes(&call.data)?,
                    ),
                    UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(call)) => (
                        address(&call.program_id)?,
                        call.accounts.iter().map(address).collect::<Option<_>>()?,
                        bytes(&call.data)?,
                    ),
                    // The node parses only calls of the programs it knows, their bytes are gone.
                    UiInstruction::Parsed(UiParsedInstruction::Parsed(_)) => continue,
                };
                calls.push(Self {
                    position: group.index,
                    program,
                    accounts,
                    data,
                });
            }
        }
        Some(calls)
    }
}

impl Attempt {
    pub async fn landing(self, chain: &dyn ChainSubmit) -> Result<Landing, RingRpcError> {
        if let Some(outcome) = chain.outcome(self.signature).await? {
            return Ok(Landing::Landed(outcome));
        }
        if chain.block_height().await? <= self.last_valid_block_height {
            return Ok(Landing::Pending);
        }
        // The signature can land between the first lookup and the height read.
        Ok(chain
            .outcome(self.signature)
            .await?
            .map_or(Landing::Expired, Landing::Landed))
    }
}

impl SolanaChain {
    pub fn new(rpc_url: &str) -> Self {
        Self {
            rpc: AsyncSolanaRpc::new(rpc_url),
        }
    }

    pub fn with_timeout(rpc_url: &str, timeout: Duration) -> Self {
        Self {
            rpc: timed_rpc(rpc_url, timeout),
        }
    }
}

#[async_trait]
impl ChainSubmit for SolanaChain {
    async fn account_data(&self, address: Address) -> Result<Option<Vec<u8>>, RingRpcError> {
        Ok(self
            .rpc
            .get_account(address)
            .await?
            .map(|account| account.data))
    }

    async fn rent_exempt_lamports(&self, data_len: usize) -> Result<u64, RingRpcError> {
        Ok(self
            .rpc
            .get_minimum_balance_for_rent_exemption(data_len)
            .await?)
    }

    async fn slot(&self) -> Result<u64, RingRpcError> {
        Ok(self.rpc.get_slot().await?)
    }

    async fn latest_blockhash(&self) -> Result<LatestBlockhash, RingRpcError> {
        Ok(self.rpc.get_latest_blockhash().await?.into())
    }

    async fn block_height(&self) -> Result<u64, RingRpcError> {
        self.rpc
            .client()
            .get_block_height_with_commitment(CommitmentConfig::finalized())
            .await
            .map_err(|error| ClientError::Rpc(format!("get_block_height: {error}")).into())
    }

    async fn send(&self, transaction: &VersionedTransaction) -> Result<(), RingRpcError> {
        let config = RpcSendTransactionConfig {
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            ..RpcSendTransactionConfig::default()
        };
        match self
            .rpc
            .send_transaction_with_config(transaction, config)
            .await
        {
            Ok(_) => Ok(()),
            Err(error) => send_refusal(error),
        }
    }

    async fn outcome(&self, signature: Signature) -> Result<Option<ChainOutcome>, RingRpcError> {
        // Without history a landed signature reads as expired once the status cache drops it.
        let statuses = self
            .rpc
            .signature_statuses_with_history(vec![signature])
            .await?;
        let Some(Some(status)) = statuses.into_iter().next() else {
            return Ok(None);
        };
        Ok(finalized_outcome(status))
    }

    async fn simulate(&self, message: &VersionedMessage) -> Result<Simulation, RingRpcError> {
        let transaction = VersionedTransaction {
            signatures: vec![
                Signature::default();
                usize::from(message.header().num_required_signatures)
            ],
            message: message.clone(),
        };
        let config = RpcSimulateTransactionConfig {
            sig_verify: false,
            replace_recent_blockhash: true,
            commitment: Some(CommitmentConfig::confirmed()),
            inner_instructions: true,
            ..RpcSimulateTransactionConfig::default()
        };
        let simulated = self
            .rpc
            .client()
            .simulate_transaction_with_config(&transaction, config)
            .await
            .map_err(|error| ClientError::Rpc(format!("simulate_transaction: {error}")))?
            .value;
        if let Some(error) = simulated.err {
            return Ok(Simulation::Refused {
                reason: error.to_string(),
            });
        }
        let calls = InnerCall::decode(message, simulated.inner_instructions.unwrap_or_default())
            .ok_or_else(|| ClientError::Rpc("simulate_transaction: inner instructions".into()))?;
        Ok(Simulation::Passed { calls })
    }

    async fn blockhash_expiry(&self, blockhash: Hash) -> Result<Option<u64>, RingRpcError> {
        let client = self.rpc.client();
        let commitment = CommitmentConfig::confirmed();
        // The tip is read second, a hash valid in the first bank is no newer than it.
        let held = client
            .is_blockhash_valid(&blockhash, commitment)
            .await
            .map_err(|error| ClientError::Rpc(format!("is_blockhash_valid: {error}")))?;
        if !held {
            return Ok(None);
        }
        let (_, last_valid_block_height) = client
            .get_latest_blockhash_with_commitment(commitment)
            .await
            .map_err(|error| ClientError::Rpc(format!("get_latest_blockhash: {error}")))?;
        Ok(Some(last_valid_block_height))
    }

    async fn finalized_tip(&self) -> Result<FinalizedTip, RingRpcError> {
        let epoch = self
            .rpc
            .client()
            .get_epoch_info_with_commitment(CommitmentConfig::finalized())
            .await
            .map_err(|error| ClientError::Rpc(format!("get_epoch_info: {error}")))?;
        Ok(FinalizedTip {
            slot: epoch.absolute_slot,
            block_height: epoch.block_height,
        })
    }
}

fn send_refusal(error: ClientError) -> Result<(), RingRpcError> {
    let ClientError::SolanaRpcTransaction { source, .. } = &error else {
        return Err(error.into());
    };
    match source.get_transaction_error() {
        Some(TransactionError::AlreadyProcessed) => Ok(()),
        _ if surfpool_already_processed(source) => Ok(()),
        Some(refused) => Err(RelayError::SendRefused {
            reason: refused.to_string(),
        }
        .into()),
        None => Err(error.into()),
    }
}

/// Surfpool names a resent landed transaction only in the text of its preflight failure.
fn surfpool_already_processed(error: &RpcClientError) -> bool {
    matches!(
        error.kind(),
        ErrorKind::RpcError(RpcError::RpcResponseError { code, message, .. })
            if *code == JSON_RPC_SERVER_ERROR_SEND_TRANSACTION_PREFLIGHT_FAILURE
                && message.contains("already been processed")
    )
}

fn finalized_outcome(status: TransactionStatus) -> Option<ChainOutcome> {
    if status.confirmation_status != Some(TransactionConfirmationStatus::Finalized) {
        return None;
    }
    Some(match status.err {
        None => ChainOutcome::Confirmed { slot: status.slot },
        Some(error) => ChainOutcome::Failed {
            reason: error.to_string(),
        },
    })
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex};

    use super::*;

    struct ScriptedLanding {
        statuses: Mutex<VecDeque<Result<Option<TransactionStatus>, RingRpcError>>>,
        finalized_height: Option<u64>,
    }

    #[async_trait]
    impl ChainSubmit for ScriptedLanding {
        async fn simulate(
            &self,
            _message: &solana_message::VersionedMessage,
        ) -> Result<Simulation, RingRpcError> {
            Ok(Simulation::Passed { calls: Vec::new() })
        }

        async fn account_data(
            &self,
            _address: solana_address::Address,
        ) -> Result<Option<Vec<u8>>, RingRpcError> {
            Ok(None)
        }

        async fn rent_exempt_lamports(&self, _data_len: usize) -> Result<u64, RingRpcError> {
            Ok(0)
        }

        async fn slot(&self) -> Result<u64, RingRpcError> {
            unreachable!()
        }

        async fn latest_blockhash(&self) -> Result<LatestBlockhash, RingRpcError> {
            unreachable!()
        }

        async fn block_height(&self) -> Result<u64, RingRpcError> {
            self.finalized_height.ok_or(RingRpcError::StateUnavailable)
        }

        async fn send(&self, _transaction: &VersionedTransaction) -> Result<(), RingRpcError> {
            unreachable!()
        }

        async fn outcome(
            &self,
            _signature: Signature,
        ) -> Result<Option<ChainOutcome>, RingRpcError> {
            self.statuses
                .lock()
                .expect("statuses")
                .pop_front()
                .expect("scripted status")
                .map(|status| status.and_then(finalized_outcome))
        }
    }

    fn attempt() -> Attempt {
        Attempt {
            signature: Signature::default(),
            last_valid_block_height: 50,
        }
    }

    fn status(
        confirmation_status: TransactionConfirmationStatus,
        err: Option<TransactionError>,
    ) -> TransactionStatus {
        TransactionStatus {
            slot: 41,
            confirmations: (confirmation_status != TransactionConfirmationStatus::Finalized)
                .then_some(1),
            status: err.clone().map_or(Ok(()), Err),
            err,
            confirmation_status: Some(confirmation_status),
        }
    }

    #[test]
    fn an_inner_call_reads_the_same_from_indices_and_from_addresses() {
        use solana_transaction_status_client_types::{
            ParsedInstruction, UiCompiledInstruction, UiPartiallyDecodedInstruction,
        };

        let keys = [1, 2, 3].map(|seed| Address::new_from_array([seed; 32]));
        let message = VersionedMessage::Legacy(solana_message::Message {
            account_keys: keys.to_vec(),
            ..solana_message::Message::default()
        });
        let data = bs58::encode([9, 8]).into_string();
        let compiled = UiInstruction::Compiled(UiCompiledInstruction {
            program_id_index: 2,
            accounts: vec![1, 0],
            data: data.clone(),
            stack_height: Some(2),
        });
        let addressed = |accounts: [Address; 2]| {
            UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(
                UiPartiallyDecodedInstruction {
                    program_id: keys[2].to_string(),
                    accounts: accounts.iter().map(Address::to_string).collect(),
                    data: data.clone(),
                    stack_height: Some(2),
                },
            ))
        };
        let parsed = UiInstruction::Parsed(UiParsedInstruction::Parsed(ParsedInstruction {
            program: "system".to_owned(),
            program_id: keys[0].to_string(),
            parsed: serde_json::Value::Null,
            stack_height: Some(3),
        }));
        let under = |index, instructions| UiInnerInstructions {
            index,
            instructions,
        };
        let call = |position| InnerCall {
            position,
            program: keys[2],
            accounts: vec![keys[1], keys[0]],
            data: vec![9, 8],
        };
        let simulated = vec![
            under(0, vec![compiled.clone(), parsed]),
            under(3, vec![addressed([keys[1], keys[0]])]),
        ];
        assert_eq!(
            InnerCall::decode(&message, simulated),
            Some(vec![call(0), call(3)])
        );

        let outside = UiInstruction::Compiled(UiCompiledInstruction {
            program_id_index: 3,
            accounts: Vec::new(),
            data,
            stack_height: None,
        });
        assert_eq!(
            InnerCall::decode(&message, vec![under(0, vec![outside])]),
            None
        );
    }

    #[tokio::test]
    async fn unfinalized_success_and_failure_keep_the_attempt_pending() {
        for confirmation in [
            TransactionConfirmationStatus::Processed,
            TransactionConfirmationStatus::Confirmed,
        ] {
            for error in [None, Some(TransactionError::AccountNotFound)] {
                let chain = ScriptedLanding {
                    statuses: Mutex::new(VecDeque::from([Ok(Some(status(
                        confirmation.clone(),
                        error,
                    )))])),
                    finalized_height: Some(attempt().last_valid_block_height),
                };
                assert_eq!(
                    attempt().landing(&chain).await.expect("landing"),
                    Landing::Pending
                );
            }
        }
    }

    #[tokio::test]
    async fn an_orphaned_confirmation_expires_after_finalized_height_passes_the_window() {
        let chain = ScriptedLanding {
            statuses: Mutex::new(VecDeque::from([
                Ok(Some(status(TransactionConfirmationStatus::Confirmed, None))),
                Ok(None),
            ])),
            finalized_height: Some(attempt().last_valid_block_height + 1),
        };
        assert_eq!(
            attempt().landing(&chain).await.expect("landing"),
            Landing::Expired
        );
    }

    #[tokio::test]
    async fn finalization_between_status_and_height_wins_over_expiry() {
        for (error, expected) in [
            (None, ChainOutcome::Confirmed { slot: 41 }),
            (
                Some(TransactionError::AccountNotFound),
                ChainOutcome::Failed {
                    reason: TransactionError::AccountNotFound.to_string(),
                },
            ),
        ] {
            let finalized = status(TransactionConfirmationStatus::Finalized, error);
            let chain = ScriptedLanding {
                statuses: Mutex::new(VecDeque::from([Ok(None), Ok(Some(finalized))])),
                finalized_height: Some(attempt().last_valid_block_height + 1),
            };
            assert_eq!(
                attempt().landing(&chain).await.expect("landing"),
                Landing::Landed(expected),
            );
        }
    }

    #[test]
    fn a_surfpool_resend_of_a_landed_transaction_counts_as_sent() {
        let preflight = |message: &str| ClientError::SolanaRpcTransaction {
            operation: "send_transaction_with_config",
            source: ErrorKind::RpcError(RpcError::RpcResponseError {
                code: JSON_RPC_SERVER_ERROR_SEND_TRANSACTION_PREFLIGHT_FAILURE,
                message: message.to_owned(),
                data: solana_rpc_client_api::request::RpcResponseErrorData::Empty,
            })
            .into(),
        };
        let landed = r#"Transaction verification failed for transaction Internal error: "Transaction error: This transaction has already been processed""#;
        assert!(send_refusal(preflight(landed)).is_ok());
        assert!(send_refusal(preflight("Transaction simulation failed")).is_err());
    }

    #[tokio::test]
    async fn status_or_height_errors_never_expire_an_attempt() {
        for chain in [
            ScriptedLanding {
                statuses: Mutex::new(VecDeque::from([Err(RingRpcError::StateUnavailable)])),
                finalized_height: Some(attempt().last_valid_block_height + 1),
            },
            ScriptedLanding {
                statuses: Mutex::new(VecDeque::from([Ok(None)])),
                finalized_height: None,
            },
            ScriptedLanding {
                statuses: Mutex::new(VecDeque::from([
                    Ok(None),
                    Err(RingRpcError::StateUnavailable),
                ])),
                finalized_height: Some(attempt().last_valid_block_height + 1),
            },
        ] {
            assert!(matches!(
                attempt().landing(&chain).await,
                Err(RingRpcError::StateUnavailable),
            ));
        }
    }
}
