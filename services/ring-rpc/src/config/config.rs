use std::{
    collections::HashSet,
    fs::File,
    net::{IpAddr, SocketAddr},
    num::{NonZeroU32, NonZeroU64, NonZeroUsize},
    path::PathBuf,
    time::Duration,
};

use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use p256::elliptic_curve::subtle::ConstantTimeEq;
use serde::Deserialize;
use solana_address::Address;
use solana_hash::Hash;
use thiserror::Error;
use zeroize::Zeroizing;
use zolana_operator_custody::{
    CustodyError, Kms, SecretContext, SecretPurpose, SignerRef, SigningIdentity, WrappedSecret,
};
use zolana_ring_client::rpc::{auditor_keypair, KeyFileError};

use crate::{
    client_rate::ClientLimitArgs, cosign::ApproveClass, keys::KeySource, origins::OriginTransport,
    server::BindPolicy, wallet_snapshots::WalletSnapshotLimits,
};

const DEFAULT_ENROLLMENT_MAX_RINGS: usize = 256;

#[derive(Debug, Parser)]
#[command(
    name = "ring-rpc",
    about = "Ring RPC for a custom ring with an auditor"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Serve(Box<ServeArgs>),
    Provision(ProvisionArgs),
    SignerRef(SignerRefArgs),
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("manifest").args(["custody", "custody_json"]).required(true)
))]
pub struct ServeArgs {
    #[arg(long, env = "RING_RPC_BIND", default_value = "127.0.0.1")]
    pub bind: IpAddr,
    #[arg(long, env = "RING_RPC_PORT", default_value_t = 8785)]
    pub port: u16,
    #[arg(
        long,
        env = "RING_RPC_INDEXER_URL",
        default_value = "http://127.0.0.1:8784"
    )]
    pub indexer_url: String,
    #[arg(
        long,
        env = "RING_RPC_SOLANA_RPC_URL",
        default_value = "http://127.0.0.1:8899"
    )]
    pub rpc_url: String,
    #[command(flatten)]
    kms: KmsArgs,
    #[arg(long, env = "RING_RPC_DEPLOYMENT", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    deployment: String,
    #[arg(long, env = "RING_RPC_CUSTODY")]
    custody: Option<PathBuf>,
    #[arg(long, env = "RING_RPC_CUSTODY_JSON")]
    custody_json: Option<String>,
    #[arg(long, env = "RING_RPC_RING_PROGRAM_ID")]
    ring_program_id: Option<Address>,
    /// The classes the co-signer signs for, none refuses every operation.
    #[arg(
        long,
        env = "RING_RPC_CO_SIGNER_APPROVE",
        value_enum,
        value_delimiter = ','
    )]
    pub co_signer_approve: Vec<ApproveClass>,
    #[arg(
        long = "allow-origin",
        env = "RING_RPC_ALLOW_ORIGINS",
        value_delimiter = ','
    )]
    pub allow_origins: Vec<String>,
    #[arg(long, env = "RING_RPC_WEBAUTHN_RP_ID")]
    pub webauthn_rp_id: Option<String>,
    #[arg(long, env = "RING_RPC_MAX_CONNECTIONS", default_value = "256")]
    pub max_connections: NonZeroU32,
    #[arg(long, env = "RING_RPC_WALLET_SNAPSHOTS", default_value_t = WalletSnapshotLimits::default().store)]
    pub wallet_snapshots: NonZeroUsize,
    #[arg(long, env = "RING_RPC_WALLET_SNAPSHOTS_PER_PRINCIPAL", default_value_t = WalletSnapshotLimits::default().principal_wallet)]
    pub wallet_snapshots_per_principal: NonZeroUsize,
    #[arg(long, env = "RING_RPC_REQUEST_TIMEOUT_SECS", default_value = "30")]
    pub request_timeout_secs: NonZeroU64,
    #[arg(long, env = "RING_RPC_UPSTREAM_TIMEOUT_SECS", default_value = "10")]
    pub upstream_timeout_secs: NonZeroU64,
    /// Lamports reserved and spent over the store's life on enrollments and token accounts, zero refuses every charge.
    #[arg(long, env = "RING_RPC_RELAY_BUDGET_LAMPORTS", default_value_t = 0)]
    pub relay_budget_lamports: u64,
    /// Lamports one signer holds outstanding at once, the fees of its attempts that never landed stay counted, zero refuses every charge.
    #[arg(
        long,
        env = "RING_RPC_RELAY_SIGNER_BUDGET_LAMPORTS",
        default_value_t = 0,
        requires = "relay_budget_lamports"
    )]
    pub relay_signer_budget_lamports: u64,
    /// `postgres://` or `sqlite://` URL of the service store, also turns the record writer on.
    #[arg(long, env = "RING_RPC_STORE_URL", requires = "prover_url")]
    pub store_url: Option<String>,
    /// Stable per deployed replica and unique among live ones, a restart then writes at once.
    #[arg(long, env = "RING_RPC_REPLICA_ID", requires = "store_url")]
    pub replica_id: Option<String>,
    /// Serves the ring keys the record writer and the relay prove with.
    #[arg(long, env = "RING_RPC_PROVER_URL")]
    pub prover_url: Option<String>,
    /// Lamports the enrollment writer spends on batch fees over the process life, zero refuses every write.
    #[arg(long, env = "RING_RPC_ENROLLMENT_BUDGET_LAMPORTS", default_value_t = 0)]
    pub enrollment_budget_lamports: u64,
    /// Lamports one ring's batches spend over the process life, a tenth of the budget when unset.
    #[arg(long, env = "RING_RPC_ENROLLMENT_RING_BUDGET_LAMPORTS")]
    pub enrollment_ring_budget_lamports: Option<u64>,
    /// Rings `registerEnrollmentRing` adds, the rings of `--enrollment-apply-ring` aside.
    #[arg(long, env = "RING_RPC_ENROLLMENT_MAX_RINGS", default_value_t = DEFAULT_ENROLLMENT_MAX_RINGS)]
    pub enrollment_max_rings: usize,
    /// A ring whose pending enrollments the applier sends without a registration.
    #[arg(
        long = "enrollment-apply-ring",
        env = "RING_RPC_ENROLLMENT_APPLY_RINGS",
        value_delimiter = ','
    )]
    pub enrollment_apply_rings: Vec<Address>,
    /// Serve plain HTTP on a public address and accept plain HTTP origins, test deployments only.
    #[arg(long, env = "RING_RPC_INSECURE_PUBLIC_BIND")]
    pub insecure_public_bind: bool,
    /// The one store the service reads reader grants from.
    #[arg(
        long,
        env = "RING_RPC_READER_GRANTS",
        value_enum,
        default_value_t = ReaderGrantStore::Tree,
        requires_if("database", "store_url")
    )]
    pub reader_grants: ReaderGrantStore,
    #[command(flatten)]
    pub client_limits: ClientLimitArgs,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum ReaderGrantStore {
    /// `Reader` lists of the ring's own storage tree.
    #[default]
    Tree,
    /// Sealed rows of the service store, signed through `setReaderGrant`.
    Database,
}

#[derive(Debug, Args)]
struct KmsArgs {
    #[arg(
        long,
        conflicts_with = "localnet_kms",
        help = "AWS KMS region, defaults to AWS_REGION"
    )]
    kms_region: Option<String>,
    #[arg(long, env = "RING_RPC_LOCALNET_KMS")]
    localnet_kms: Option<SocketAddr>,
}

#[derive(Debug, Args)]
pub struct ProvisionArgs {
    #[command(flatten)]
    kms: KmsArgs,
    #[arg(long, env = "RING_RPC_DEPLOYMENT", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    deployment: String,
    #[arg(long)]
    wrapping_key_arn: String,
    #[arg(long, value_enum)]
    purpose: Purpose,
    #[arg(long)]
    genesis_hash: Hash,
    #[arg(long, required_if_eq("purpose", "auditor"))]
    ring_program_id: Option<Address>,
}

#[derive(Debug, Args)]
pub struct SignerRefArgs {
    #[command(flatten)]
    kms: KmsArgs,
    #[arg(long)]
    key_arn: String,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Purpose {
    Auditor,
    AuditorRoot,
    GrantRows,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CustodyManifest {
    auditor: WrappedSecret,
    service: SignerRef,
    storage_authority: Option<SignerRef>,
    co_signer: Option<SignerRef>,
    grant_rows: Option<WrappedSecret>,
    fee_payer: Option<SignerRef>,
    enrollment_applier: Option<SignerRef>,
}

pub struct ServeKeys {
    pub auditor: KeySource,
    pub service: SigningIdentity,
    pub storage_authority: Option<SigningIdentity>,
    pub co_signer: Option<SigningIdentity>,
    pub grant_rows: Option<GrantRowsSecret>,
    pub fee_payer: Option<SigningIdentity>,
    pub enrollment_applier: Option<SigningIdentity>,
}

#[derive(Debug, Error)]
pub enum ServeKeyError {
    #[error("cannot read the custody manifest")]
    File(#[from] std::io::Error),
    #[error("invalid custody manifest")]
    Manifest(#[from] serde_json::Error),
    #[error("missing custody manifest")]
    MissingManifest,
    #[error("missing AWS region")]
    MissingRegion,
    #[error("only a single ring auditor requires a ring program ID")]
    AuditorRing,
    #[error("local KMS requires a loopback bind address and Solana RPC URL")]
    RemoteCluster,
    #[error("the store and its storage authority must be configured together")]
    StorageAuthority,
    #[error("configure a prover for a store or an enrollment applier")]
    Prover,
    #[error("the co-signer and fee payer require a store")]
    SigningStore,
    #[error("approval classes require a co-signer")]
    Approval,
    #[error("relay budgets require a fee payer")]
    RelayBudget,
    #[error("enrollment options require an enrollment applier")]
    Enrollment,
    #[error("database reader grants require a wrapped grant rows secret")]
    GrantRows,
    #[error("each signing role requires a distinct address")]
    SharedAddress,
    #[error("the grant rows secret matches the auditor secret")]
    SharedSecret,
    #[error(transparent)]
    Kms(#[from] CustodyError),
    #[error(transparent)]
    Auditor(#[from] KeyFileError),
}

pub struct GrantRowsSecret(Zeroizing<[u8; 32]>);
pub struct RootSecret(Zeroizing<[u8; 32]>);

#[derive(Debug, Error)]
#[error("root secret cannot be zero")]
pub struct RootSecretError;

impl KmsArgs {
    async fn connect(&self, genesis_hash: &[u8; 32]) -> Result<Kms, ServeKeyError> {
        match self.localnet_kms {
            Some(endpoint) => Ok(Kms::localnet_for_genesis(endpoint, genesis_hash)?),
            None => self.aws().await,
        }
    }

    async fn aws(&self) -> Result<Kms, ServeKeyError> {
        let region = self
            .kms_region
            .clone()
            .or_else(|| std::env::var("AWS_REGION").ok())
            .ok_or(ServeKeyError::MissingRegion)?;
        Ok(Kms::aws(&region).await?)
    }
}

impl ProvisionArgs {
    pub async fn provision(&self) -> Result<WrappedSecret, ServeKeyError> {
        let purpose = match self.purpose {
            Purpose::Auditor => {
                SecretPurpose::Auditor(self.ring_program_id.ok_or(ServeKeyError::AuditorRing)?)
            }
            _ if self.ring_program_id.is_some() => return Err(ServeKeyError::AuditorRing),
            Purpose::AuditorRoot => SecretPurpose::AuditorRoot,
            Purpose::GrantRows => SecretPurpose::GrantRows,
        };
        let genesis_hash = self.genesis_hash.to_bytes();
        let kms = self.kms.connect(&genesis_hash).await?;
        Ok(kms
            .provision(
                &self.wrapping_key_arn,
                SecretContext {
                    deployment: &self.deployment,
                    genesis_hash: &genesis_hash,
                    purpose,
                },
            )
            .await?)
    }
}

impl SignerRefArgs {
    pub async fn resolve(self) -> Result<SignerRef, ServeKeyError> {
        let address = match self.kms.localnet_kms {
            Some(endpoint) => Kms::localnet_public_key(endpoint, &self.key_arn).await?,
            None => self.kms.aws().await?.public_key(&self.key_arn).await?,
        };
        Ok(SignerRef {
            key_arn: self.key_arn,
            address,
        })
    }
}

impl ServeArgs {
    pub fn enrollment_ring_budget(&self) -> u64 {
        self.enrollment_ring_budget_lamports
            .unwrap_or(self.enrollment_budget_lamports / 10)
    }

    pub fn origin_transport(&self) -> OriginTransport {
        if self.insecure_public_bind {
            OriginTransport::InsecureHttp
        } else {
            OriginTransport::SecureOnly
        }
    }

    pub fn bind_policy(&self) -> BindPolicy {
        if self.insecure_public_bind {
            BindPolicy::InsecurePublic
        } else {
            BindPolicy::LoopbackOnly
        }
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs.get())
    }

    pub fn upstream_timeout(&self) -> Duration {
        Duration::from_secs(self.upstream_timeout_secs.get())
    }

    pub async fn read_keys(&self, genesis_hash: &[u8; 32]) -> Result<ServeKeys, ServeKeyError> {
        let manifest: CustodyManifest = if let Some(path) = &self.custody {
            serde_json::from_reader(File::open(path)?)?
        } else {
            serde_json::from_str(
                self.custody_json
                    .as_deref()
                    .ok_or(ServeKeyError::MissingManifest)?,
            )?
        };
        manifest.validate(self)?;
        if self.kms.localnet_kms.is_some()
            && (!self.bind.is_loopback() || !loopback_url(&self.rpc_url))
        {
            return Err(ServeKeyError::RemoteCluster);
        }
        let kms = self.kms.connect(genesis_hash).await?;
        let context = |purpose| SecretContext {
            deployment: &self.deployment,
            genesis_hash,
            purpose,
        };
        let purpose = self
            .ring_program_id
            .map_or(SecretPurpose::AuditorRoot, SecretPurpose::Auditor);
        let secret = kms.open(&manifest.auditor, context(purpose)).await?;
        let grant_rows = match &manifest.grant_rows {
            Some(wrapped) => {
                let grant = kms.open(wrapped, context(SecretPurpose::GrantRows)).await?;
                if bool::from(secret.as_slice().ct_eq(grant.as_slice())) {
                    return Err(ServeKeyError::SharedSecret);
                }
                Some(GrantRowsSecret(grant))
            }
            None => None,
        };
        let auditor = match self.ring_program_id {
            Some(ring) => KeySource::Local {
                ring,
                auditor: auditor_keypair(&secret)?,
            },
            None => KeySource::Derived(RootSecret(secret)),
        };
        let (service, storage_authority, co_signer, fee_payer, enrollment_applier) = tokio::try_join!(
            kms.signer(&manifest.service),
            optional_signer(&kms, manifest.storage_authority.as_ref()),
            optional_signer(&kms, manifest.co_signer.as_ref()),
            optional_signer(&kms, manifest.fee_payer.as_ref()),
            optional_signer(&kms, manifest.enrollment_applier.as_ref()),
        )?;
        Ok(ServeKeys {
            auditor,
            service,
            storage_authority,
            co_signer,
            grant_rows,
            fee_payer,
            enrollment_applier,
        })
    }
}

impl CustodyManifest {
    fn validate(&self, args: &ServeArgs) -> Result<(), ServeKeyError> {
        let store = args.store_url.is_some();
        let applier = self.enrollment_applier.is_some();
        if store != self.storage_authority.is_some() {
            return Err(ServeKeyError::StorageAuthority);
        }
        if (store || applier) != args.prover_url.is_some() {
            return Err(ServeKeyError::Prover);
        }
        if !store && (self.co_signer.is_some() || self.fee_payer.is_some()) {
            return Err(ServeKeyError::SigningStore);
        }
        if !args.co_signer_approve.is_empty() && self.co_signer.is_none() {
            return Err(ServeKeyError::Approval);
        }
        if (args.relay_budget_lamports != 0 || args.relay_signer_budget_lamports != 0)
            && self.fee_payer.is_none()
        {
            return Err(ServeKeyError::RelayBudget);
        }
        if !applier
            && (!args.enrollment_apply_rings.is_empty()
                || args.enrollment_budget_lamports != 0
                || args.enrollment_ring_budget_lamports.is_some()
                || args.enrollment_max_rings != DEFAULT_ENROLLMENT_MAX_RINGS)
        {
            return Err(ServeKeyError::Enrollment);
        }
        if args.reader_grants == ReaderGrantStore::Database && self.grant_rows.is_none() {
            return Err(ServeKeyError::GrantRows);
        }
        let mut addresses = HashSet::new();
        for identity in [
            Some(&self.service),
            self.storage_authority.as_ref(),
            self.co_signer.as_ref(),
            self.fee_payer.as_ref(),
            self.enrollment_applier.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if !addresses.insert(identity.address) {
                return Err(ServeKeyError::SharedAddress);
            }
        }
        Ok(())
    }
}

async fn optional_signer(
    kms: &Kms,
    identity: Option<&SignerRef>,
) -> Result<Option<SigningIdentity>, CustodyError> {
    match identity {
        Some(identity) => kms.signer(identity).await.map(Some),
        None => Ok(None),
    }
}

fn loopback_url(url: &str) -> bool {
    reqwest::Url::parse(url).ok().is_some_and(|url| {
        url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .trim_matches(['[', ']'])
                    .parse::<IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        })
    })
}

impl RootSecret {
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, RootSecretError> {
        if bytes == [0; 32] {
            return Err(RootSecretError);
        }
        Ok(Self(Zeroizing::new(bytes)))
    }

    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl GrantRowsSecret {
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
