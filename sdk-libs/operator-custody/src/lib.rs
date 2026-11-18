use std::{net::SocketAddr, sync::Arc, time::Duration};

mod secret;
mod transaction;

pub use secret::{SecretContext, SecretPurpose, WrappedSecret};
pub use transaction::{presigned_transaction, signed_transaction, CollectedSignature};

use async_trait::async_trait;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_kms::{
    config::{timeout::TimeoutConfig, Credentials},
    primitives::Blob,
    types::{KeySpec, KeyUsageType, MessageType, SigningAlgorithmSpec},
    Client,
};
use ed25519_dalek::{pkcs8::DecodePublicKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use solana_address::Address;
use solana_signature::Signature;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CustodyError {
    #[error(transparent)]
    Transaction(#[from] spr_client::ClientError),
    #[error("invalid KMS key ARN")]
    KeyArn,
    #[error("invalid AWS region")]
    Region,
    #[error("local KMS requires a loopback address")]
    Endpoint,
    #[error("KMS key does not match the pinned signing identity")]
    KeyMismatch,
    #[error("message exceeds the KMS raw signing limit")]
    MessageTooLong,
    #[error("invalid KMS signature")]
    Signature,
    #[error("missing deployment name")]
    Deployment,
    #[error("invalid wrapped secret")]
    Secret,
    #[error("KMS operation failed")]
    Service(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl CustodyError {
    fn service(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Service(Box::new(error))
    }
}

#[async_trait]
pub trait OperatorSigner: Send + Sync {
    fn address(&self) -> Address;
    async fn sign(&self, message: &[u8]) -> Result<Signature, CustodyError>;
}

pub type SigningIdentity = Arc<dyn OperatorSigner>;

#[serde_with::serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignerRef {
    pub key_arn: String,
    #[serde_as(as = "serde_with::DisplayFromStr")]
    pub address: Address,
}

#[derive(Clone)]
pub struct Kms(Client);

struct KmsSigner {
    client: Client,
    identity: SignerRef,
}

impl Kms {
    pub async fn aws(region: &str) -> Result<Self, CustodyError> {
        if region.is_empty()
            || !region
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        {
            return Err(CustodyError::Region);
        }
        let suffix = if region.starts_with("cn-") {
            "amazonaws.com.cn"
        } else {
            "amazonaws.com"
        };
        let endpoint = format!("https://kms.{region}.{suffix}");
        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(region.to_owned()))
            .load()
            .await;
        let config = aws_sdk_kms::config::Builder::from(&config).endpoint_url(endpoint);
        Ok(Self::client(config))
    }

    pub fn localnet_for_genesis(
        endpoint: SocketAddr,
        genesis: &[u8; 32],
    ) -> Result<Self, CustodyError> {
        let cluster = Address::new_from_array(*genesis).to_string();
        if matches!(
            cluster.as_str(),
            "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"
                | "4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY"
                | "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"
        ) {
            return Err(CustodyError::Endpoint);
        }
        Self::localnet(endpoint)
    }

    pub async fn localnet_public_key(
        endpoint: SocketAddr,
        key_arn: &str,
    ) -> Result<Address, CustodyError> {
        Self::localnet(endpoint)?.public_key(key_arn).await
    }

    fn localnet(endpoint: SocketAddr) -> Result<Self, CustodyError> {
        if !endpoint.ip().is_loopback() {
            return Err(CustodyError::Endpoint);
        }
        let config = aws_sdk_kms::Config::builder()
            .behavior_version_latest()
            .region(Region::new("us-east-1"))
            .credentials_provider(Credentials::new("local", "local", None, None, "localnet"))
            .endpoint_url(format!("http://{endpoint}"));
        Ok(Self::client(config))
    }

    fn client(config: aws_sdk_kms::config::Builder) -> Self {
        let timeout = TimeoutConfig::builder()
            .operation_timeout(Duration::from_secs(10))
            .build();
        Self(Client::from_conf(config.timeout_config(timeout).build()))
    }

    pub async fn public_key(&self, key_arn: &str) -> Result<Address, CustodyError> {
        check_key_arn(key_arn)?;
        let key = self
            .0
            .get_public_key()
            .key_id(key_arn)
            .send()
            .await
            .map_err(CustodyError::service)?;
        if key.key_id() != Some(key_arn)
            || key.key_spec() != Some(&KeySpec::EccNistEdwards25519)
            || key.key_usage() != Some(&KeyUsageType::SignVerify)
            || !key
                .signing_algorithms()
                .contains(&SigningAlgorithmSpec::Ed25519Sha512)
        {
            return Err(CustodyError::KeyMismatch);
        }
        let der = key.public_key().ok_or(CustodyError::KeyMismatch)?;
        let public = VerifyingKey::from_public_key_der(der.as_ref())
            .map_err(|_| CustodyError::KeyMismatch)?;
        if public.is_weak() {
            return Err(CustodyError::KeyMismatch);
        }
        Ok(Address::new_from_array(public.to_bytes()))
    }

    pub async fn signer(&self, identity: &SignerRef) -> Result<SigningIdentity, CustodyError> {
        if self.public_key(&identity.key_arn).await? != identity.address {
            return Err(CustodyError::KeyMismatch);
        }
        Ok(Arc::new(KmsSigner {
            client: self.0.clone(),
            identity: identity.clone(),
        }))
    }
}

#[async_trait]
impl OperatorSigner for KmsSigner {
    fn address(&self) -> Address {
        self.identity.address
    }

    async fn sign(&self, message: &[u8]) -> Result<Signature, CustodyError> {
        if message.len() > 4096 {
            return Err(CustodyError::MessageTooLong);
        }
        let reply = self
            .client
            .sign()
            .key_id(&self.identity.key_arn)
            .message_type(MessageType::Raw)
            .signing_algorithm(SigningAlgorithmSpec::Ed25519Sha512)
            .message(Blob::new(message))
            .send()
            .await
            .map_err(CustodyError::service)?;
        if reply.key_id() != Some(self.identity.key_arn.as_str())
            || reply.signing_algorithm() != Some(&SigningAlgorithmSpec::Ed25519Sha512)
        {
            return Err(CustodyError::KeyMismatch);
        }
        let signature =
            Signature::try_from(reply.signature().ok_or(CustodyError::Signature)?.as_ref())
                .map_err(|_| CustodyError::Signature)?;
        if !signature.verify(self.address().as_ref(), message) {
            return Err(CustodyError::Signature);
        }
        Ok(signature)
    }
}

fn check_key_arn(arn: &str) -> Result<(), CustodyError> {
    let mut parts = arn.splitn(6, ':');
    match std::array::from_fn(|_| parts.next().unwrap_or_default()) {
        ["arn", "aws" | "aws-cn" | "aws-us-gov", "kms", region, account, resource]
            if !region.is_empty()
                && account.len() == 12
                && account.bytes().all(|c| c.is_ascii_digit())
                && resource.strip_prefix("key/").is_some_and(|id| {
                    !id.is_empty() && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
                }) =>
        {
            Ok(())
        }
        _ => Err(CustodyError::KeyArn),
    }
}

#[cfg(feature = "test-fixtures")]
#[async_trait]
impl OperatorSigner for solana_keypair::Keypair {
    fn address(&self) -> Address {
        solana_signer::Signer::pubkey(self)
    }

    async fn sign(&self, message: &[u8]) -> Result<Signature, CustodyError> {
        Ok(solana_signer::Signer::sign_message(self, message))
    }
}

#[cfg(test)]
mod tests;
