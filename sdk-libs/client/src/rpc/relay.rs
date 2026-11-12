use std::fmt::Write as _;

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use solana_address::Address;
use spr_indexer_api::{Base64String, Hash, SerializablePubkey, SerializableSignature};
use zeroize::Zeroizing;

use super::{ReadTarget, RelayProofInputs};

const RELAY_DOMAIN: &[u8] = b"zolana/ring-rpc-relay";
const DEPOSIT_REQUEST_DOMAIN: &str = "zolana/ring-rpc-relay-deposit";
const MERGE_REQUEST_DOMAIN: &str = "zolana/ring-rpc-relay-merge";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelayOperation {
    Anonymous,
    Confidential,
    Delegate,
    Deposit,
    Merge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetRelayQuoteRequest {
    pub ring: SerializablePubkey,
    pub operation: RelayOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RelayQuote {
    pub fee_payer: SerializablePubkey,
    pub valid_until_slot: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PrepareRelayRequest {
    pub ring: SerializablePubkey,
    pub quote: RelayQuote,
    pub intent: RelayIntent,
    /// In run order, empty names the service alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub co_signers: Vec<SerializablePubkey>,
}

/// `proof_inputs` holds the rail's proof inputs without Merkle paths or record openings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RelayIntent {
    /// `owner_signature` is raw Ed25519 over `SppProofInputs::message_hash`.
    Anonymous {
        proof_inputs: ProofInputBytes,
        owner_signature: SerializableSignature,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        token_account_setup: Vec<TokenAccountSetup>,
    },
    Confidential {
        proof_inputs: ProofInputBytes,
        owner_signature: SerializableSignature,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        token_account_setup: Vec<TokenAccountSetup>,
    },
    Delegate {
        proof_inputs: ProofInputBytes,
        /// The ring delegate's Ed25519 over `SppProofInputs::message_hash`.
        delegate_signature: SerializableSignature,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        token_account_setup: Vec<TokenAccountSetup>,
    },
    /// `proof_inputs` is a deposit rail `RelayProofInputs`, its one funder signs the `RelayRequestAttestation` here and the returned message later.
    Deposit {
        proof_inputs: ProofInputBytes,
        depositor_signature: SerializableSignature,
    },
    /// `proof_inputs` is a `RelayMergeInputs`, the owner signs its `RelayRequestAttestation` here.
    Merge {
        proof_inputs: ProofInputBytes,
        owner_signature: SerializableSignature,
    },
}

/// One ring rpc on one cluster, a relayed deposit or merge names it beside the ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceTarget {
    /// The genesis hash.
    pub cluster: [u8; 32],
    /// The ring rpc's signing key.
    pub service: Address,
}

/// One interface transfer of a deposit frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayLeg {
    pub deposit: bool,
    pub mint: Address,
    pub amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayRequest {
    /// `legs` in frame order.
    Deposit {
        digest: [u8; 32],
        legs: Vec<RelayLeg>,
    },
    Merge {
        digest: [u8; 32],
    },
}

/// The text a depositor or a merge owner signs, raw Ed25519 over `message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayRequestAttestation {
    pub target: ReadTarget,
    pub signer: Address,
    pub request: RelayRequest,
}

/// The fee payer creates the associated token account a withdrawal of `mint` pays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TokenAccountSetup {
    pub owner: SerializablePubkey,
    pub mint: SerializablePubkey,
}

/// Base64 on the wire, zeroized on drop and never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct ProofInputBytes(Zeroizing<Vec<u8>>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RelayId(pub Hash);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "step",
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RelayStep {
    Submitted {
        relay_id: RelayId,
    },
    /// `message` is the v1 message the wallet signs and returns through `submitRelay`.
    SignMessage {
        relay_id: RelayId,
        message: Base64String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SubmitRelayRequest {
    pub relay_id: RelayId,
    pub signer: SerializablePubkey,
    pub signature: SerializableSignature,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetRelayStatusRequest {
    pub relay_id: RelayId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RelayStatus {
    AwaitingSignature,
    Sent {
        signature: SerializableSignature,
    },
    Confirmed {
        signature: SerializableSignature,
        slot: u64,
    },
    Failed {
        reason: String,
    },
}

impl RelayIntent {
    pub const fn operation(&self) -> RelayOperation {
        match self {
            Self::Anonymous { .. } => RelayOperation::Anonymous,
            Self::Confidential { .. } => RelayOperation::Confidential,
            Self::Delegate { .. } => RelayOperation::Delegate,
            Self::Deposit { .. } => RelayOperation::Deposit,
            Self::Merge { .. } => RelayOperation::Merge,
        }
    }

    pub fn token_account_setup(&self) -> &[TokenAccountSetup] {
        match self {
            Self::Anonymous {
                token_account_setup,
                ..
            }
            | Self::Confidential {
                token_account_setup,
                ..
            }
            | Self::Delegate {
                token_account_setup,
                ..
            } => token_account_setup,
            Self::Deposit { .. } | Self::Merge { .. } => &[],
        }
    }

    pub const fn proof_inputs(&self) -> &ProofInputBytes {
        match self {
            Self::Anonymous { proof_inputs, .. }
            | Self::Confidential { proof_inputs, .. }
            | Self::Delegate { proof_inputs, .. }
            | Self::Deposit { proof_inputs, .. }
            | Self::Merge { proof_inputs, .. } => proof_inputs,
        }
    }
}

impl ServiceTarget {
    pub const fn ring(self, ring: Address) -> ReadTarget {
        ReadTarget {
            cluster: self.cluster,
            service: self.service,
            ring,
        }
    }
}

impl RelayRequestAttestation {
    /// `proof_inputs` encodes `inputs`.
    pub fn deposit(
        target: ReadTarget,
        depositor: Address,
        inputs: &RelayProofInputs,
        proof_inputs: &ProofInputBytes,
    ) -> Self {
        Self {
            target,
            signer: depositor,
            request: RelayRequest::Deposit {
                digest: proof_inputs.deposit_request_digest(),
                legs: inputs.relay_legs(),
            },
        }
    }

    pub fn merge(target: ReadTarget, owner: Address, proof_inputs: &ProofInputBytes) -> Self {
        Self {
            target,
            signer: owner,
            request: RelayRequest::Merge {
                digest: proof_inputs.merge_request_digest(),
            },
        }
    }

    pub fn message(&self) -> Vec<u8> {
        let (domain, digest, legs) = match &self.request {
            RelayRequest::Deposit { digest, legs } => (DEPOSIT_REQUEST_DOMAIN, digest, &legs[..]),
            RelayRequest::Merge { digest } => (MERGE_REQUEST_DOMAIN, digest, &[][..]),
        };
        let mut text = format!("{domain}\n{}\nsigner: {}", self.target.lines(), self.signer);
        for leg in legs {
            let kind = if leg.deposit { "deposit" } else { "withdrawal" };
            let _ = write!(text, "\n{kind}: {} {}", leg.mint, leg.amount);
        }
        let _ = write!(text, "\nrequest: {}", Hash(*digest).to_base58());
        text.into_bytes()
    }
}

impl PrepareRelayRequest {
    /// SHA-256 over the domain and the request's own serialization, a retry names the same relay.
    pub fn relay_id(&self) -> Result<RelayId, serde_json::Error> {
        relay_id(self)
    }
}

pub(super) fn relay_id(request: &impl Serialize) -> Result<RelayId, serde_json::Error> {
    let request = Zeroizing::new(serde_json::to_vec(request)?);
    let digest: [u8; 32] = Sha256::new()
        .chain_update(RELAY_DOMAIN)
        .chain_update(&request)
        .finalize()
        .into();
    Ok(RelayId(Hash(digest)))
}

impl ProofInputBytes {
    pub fn new(bytes: Zeroizing<Vec<u8>>) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// It names no quote, so a retry keeps the depositor's signature.
    pub fn deposit_request_digest(&self) -> [u8; 32] {
        self.request_digest(DEPOSIT_REQUEST_DOMAIN)
    }

    /// It names no quote, so a retry keeps the owner's signature.
    pub fn merge_request_digest(&self) -> [u8; 32] {
        self.request_digest(MERGE_REQUEST_DOMAIN)
    }

    /// Every byte the relay would prove from, the secrets among them stay behind SHA-256.
    fn request_digest(&self, domain: &str) -> [u8; 32] {
        Sha256::new()
            .chain_update(domain.as_bytes())
            .chain_update(&*self.0)
            .finalize()
            .into()
    }
}

impl core::fmt::Debug for ProofInputBytes {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ProofInputBytes(..)")
    }
}

impl Serialize for ProofInputBytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&Zeroizing::new(STANDARD.encode(&*self.0)))
    }
}

impl<'de> Deserialize<'de> for ProofInputBytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let encoded = Zeroizing::new(String::deserialize(deserializer)?);
        STANDARD
            .decode(encoded.as_bytes())
            .map(|bytes| Self(Zeroizing::new(bytes)))
            .map_err(de::Error::custom)
    }
}
