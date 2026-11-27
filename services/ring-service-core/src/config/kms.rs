use std::net::SocketAddr;

use clap::Args;
use thiserror::Error;
use zolana_operator_custody::KmsEndpoint;

#[derive(Debug, Args)]
pub struct KmsArgs {
    /// AWS KMS region, defaults to AWS_REGION.
    #[arg(long, conflicts_with = "localnet_kms")]
    kms_region: Option<String>,
    #[arg(long, env = "RING_RPC_LOCALNET_KMS")]
    localnet_kms: Option<SocketAddr>,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("missing AWS region")]
pub struct MissingRegion;

impl KmsArgs {
    pub fn endpoint(&self) -> Result<KmsEndpoint, MissingRegion> {
        if let Some(endpoint) = self.localnet_kms {
            return Ok(KmsEndpoint::Localnet(endpoint));
        }
        // Read by hand, a clap env value would conflict with the local KMS flag.
        self.kms_region
            .clone()
            .or_else(|| std::env::var("AWS_REGION").ok())
            .filter(|region| !region.is_empty())
            .map(|region| KmsEndpoint::Aws { region })
            .ok_or(MissingRegion)
    }
}
