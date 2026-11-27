use std::net::{IpAddr, SocketAddr};

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

pub struct KmsNetwork<'a> {
    pub bind: IpAddr,
    pub rpc_url: &'a str,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("local KMS requires a loopback bind address and Solana RPC URL")]
pub struct RemoteCluster;

impl KmsNetwork<'_> {
    pub fn check(self, endpoint: &KmsEndpoint) -> Result<(), RemoteCluster> {
        if matches!(endpoint, KmsEndpoint::Localnet(_))
            && !(self.bind.is_loopback() && crate::origins::loopback_url(self.rpc_url))
        {
            return Err(RemoteCluster);
        }
        Ok(())
    }
}

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
