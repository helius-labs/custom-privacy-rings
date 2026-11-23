use std::{
    borrow::Cow,
    collections::HashSet,
    net::{IpAddr, SocketAddr},
    num::{NonZeroU32, NonZeroU64, NonZeroUsize},
    path::PathBuf,
    time::Duration,
};

use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use p256::elliptic_curve::subtle::ConstantTimeEq;
use solana_address::Address;
use solana_hash::Hash;
use spr_keypair::X25519Keypair;
use thiserror::Error;
use zeroize::Zeroizing;
use zolana_operator_custody::{
    CustodyError, KeyArn, Kms, KmsEndpoint, RpcCustody, SecretContext, SecretPurpose, SignerRef,
    SigningIdentity, WrappedSecret,
};

use crate::{
    client_rate::ClientLimitArgs,
    cosign::ApproveClass,
    keys::KeySource,
    origins::{loopback_url, OriginTransport},
    server::BindPolicy,
    wallet_snapshots::WalletSnapshotLimits,
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
    secrets: SecretArgs,
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
    #[arg(long, env = "RING_RPC_ENROLLMENT_MAX_RINGS")]
    enrollment_max_rings: Option<usize>,
    /// A ring whose pending enrollments the applier sends without a registration.
    #[arg(
        long = "enrollment-apply-ring",
        env = "RING_RPC_ENROLLMENT_APPLY_RINGS",
        value_delimiter = ','
    )]
    pub enrollment_apply_rings: Vec<Address>,
    /// A vault program trusted with the enrollment writer's signature.
    #[arg(
        long = "enrollment-vault-program",
        env = "RING_RPC_ENROLLMENT_VAULT_PROGRAMS",
        value_delimiter = ',',
        requires = "enrollment_applier_key_file"
    )]
    pub enrollment_vault_programs: Vec<Address>,
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
    /// AWS KMS region, defaults to AWS_REGION.
    #[arg(long, conflicts_with = "localnet_kms")]
    kms_region: Option<String>,
    #[arg(long, env = "RING_RPC_LOCALNET_KMS")]
    localnet_kms: Option<SocketAddr>,
}

#[derive(Debug, Args)]
struct SecretArgs {
    #[command(flatten)]
    kms: KmsArgs,
    #[arg(long, env = "RING_RPC_DEPLOYMENT", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    deployment: String,
}

#[derive(Debug, Args)]
pub struct ProvisionArgs {
    #[command(flatten)]
    secrets: SecretArgs,
    #[arg(long)]
    wrapping_key_arn: KeyArn,
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
    key_arn: KeyArn,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Purpose {
    Auditor,
    AuditorRoot,
    GrantRows,
}

/// No two roles share a secret.
pub struct ServeKeys {
    pub auditor: KeySource,
    pub service: SigningIdentity,
    pub storage_authority: Option<SigningIdentity>,
    pub co_signer: Option<SigningIdentity>,
    pub grant_rows: Option<GrantRowsSecret>,
    pub fee_payer: Option<SigningIdentity>,
    pub enrollment_applier: Option<SigningIdentity>,
}

pub struct GrantRowsSecret(Zeroizing<[u8; 32]>);
pub struct RootSecret(Zeroizing<[u8; 32]>);

#[derive(Debug, Error)]
pub enum CustodyConfigError {
    #[error("cannot read the custody manifest")]
    File(#[from] std::io::Error),
    #[error("invalid custody manifest")]
    Manifest(#[from] serde_json::Error),
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
}

#[derive(Debug, Error)]
#[error("root secret cannot be zero")]
pub struct RootSecretError;

impl ProvisionArgs {
    pub async fn provision(self) -> Result<WrappedSecret, CustodyConfigError> {
        let purpose = self.secret_purpose()?;
        let kms = self
            .secrets
            .kms
            .endpoint()?
            .connect(&self.genesis_hash)
            .await?;
        let context = SecretContext {
            deployment: &self.secrets.deployment,
            genesis: &self.genesis_hash,
            purpose,
        };
        Ok(kms.provision(self.wrapping_key_arn, context).await?)
    }

    fn secret_purpose(&self) -> Result<SecretPurpose, CustodyConfigError> {
        match (self.purpose, self.ring_program_id) {
            (Purpose::Auditor, Some(ring)) => Ok(SecretPurpose::Auditor(ring)),
            (Purpose::AuditorRoot, None) => Ok(SecretPurpose::AuditorRoot),
            (Purpose::GrantRows, None) => Ok(SecretPurpose::GrantRows),
            _ => Err(CustodyConfigError::AuditorRing),
        }
    }
}

impl SignerRefArgs {
    pub async fn resolve(self) -> Result<SignerRef, CustodyConfigError> {
        Ok(self.kms.endpoint()?.signer_ref(self.key_arn).await?)
    }
}

impl ServeArgs {
    pub fn enrollment_ring_budget(&self) -> u64 {
        self.enrollment_ring_budget_lamports
            .unwrap_or(self.enrollment_budget_lamports / 10)
    }

    pub fn enrollment_max_rings(&self) -> usize {
        self.enrollment_max_rings
            .unwrap_or(DEFAULT_ENROLLMENT_MAX_RINGS)
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

    /// KMS opens each secret and pins each signer of a manifest the arguments accept.
    pub async fn read_keys(
        &self,
        genesis_hash: &[u8; 32],
    ) -> Result<ServeKeys, CustodyConfigError> {
        let endpoint = self.secrets.kms.endpoint()?;
        let manifest = self.checked_manifest(&self.custody_json()?, &endpoint)?;
        let genesis = Hash::new_from_array(*genesis_hash);
        let kms = endpoint.connect(&genesis).await?;
        let context = |purpose| SecretContext {
            deployment: &self.secrets.deployment,
            genesis: &genesis,
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
                    return Err(CustodyConfigError::SharedSecret);
                }
                Some(GrantRowsSecret(grant))
            }
            None => None,
        };
        let auditor = match self.ring_program_id {
            Some(ring) => KeySource::Local {
                ring,
                auditor: X25519Keypair::from_bytes(&secret),
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

    fn custody_json(&self) -> Result<Cow<'_, str>, CustodyConfigError> {
        Ok(match &self.custody {
            Some(path) => Cow::Owned(std::fs::read_to_string(path)?),
            None => Cow::Borrowed(self.custody_json.as_deref().unwrap_or_default()),
        })
    }

    fn checked_manifest(
        &self,
        json: &str,
        endpoint: &KmsEndpoint,
    ) -> Result<RpcCustody, CustodyConfigError> {
        if matches!(endpoint, KmsEndpoint::Localnet(_))
            && !(self.bind.is_loopback() && loopback_url(&self.rpc_url))
        {
            return Err(CustodyConfigError::RemoteCluster);
        }
        let manifest: RpcCustody = serde_json::from_str(json)?;
        self.validate(&manifest)?;
        Ok(manifest)
    }

    fn validate(&self, manifest: &RpcCustody) -> Result<(), CustodyConfigError> {
        let store = self.store_url.is_some();
        let applier = manifest.enrollment_applier.is_some();
        if store != manifest.storage_authority.is_some() {
            return Err(CustodyConfigError::StorageAuthority);
        }
        if (store || applier) != self.prover_url.is_some() {
            return Err(CustodyConfigError::Prover);
        }
        if !store && (manifest.co_signer.is_some() || manifest.fee_payer.is_some()) {
            return Err(CustodyConfigError::SigningStore);
        }
        if !self.co_signer_approve.is_empty() && manifest.co_signer.is_none() {
            return Err(CustodyConfigError::Approval);
        }
        if (self.relay_budget_lamports != 0 || self.relay_signer_budget_lamports != 0)
            && manifest.fee_payer.is_none()
        {
            return Err(CustodyConfigError::RelayBudget);
        }
        if !applier
            && (!self.enrollment_apply_rings.is_empty()
                || self.enrollment_budget_lamports != 0
                || self.enrollment_ring_budget_lamports.is_some()
                || self.enrollment_max_rings.is_some())
        {
            return Err(CustodyConfigError::Enrollment);
        }
        if self.reader_grants == ReaderGrantStore::Database && manifest.grant_rows.is_none() {
            return Err(CustodyConfigError::GrantRows);
        }
        let mut addresses = HashSet::new();
        if !manifest
            .signers()
            .all(|signer| addresses.insert(signer.address))
        {
            return Err(CustodyConfigError::SharedAddress);
        }
        Ok(())
    }
}

impl KmsArgs {
    fn endpoint(&self) -> Result<KmsEndpoint, CustodyConfigError> {
        if let Some(endpoint) = self.localnet_kms {
            return Ok(KmsEndpoint::Localnet(endpoint));
        }
        // Read by hand, a clap env value would conflict with the local KMS flag.
        self.kms_region
            .clone()
            .or_else(|| std::env::var("AWS_REGION").ok())
            .filter(|region| !region.is_empty())
            .map(|region| KmsEndpoint::Aws { region })
            .ok_or(CustodyConfigError::MissingRegion)
    }
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

async fn optional_signer(
    kms: &Kms,
    identity: Option<&SignerRef>,
) -> Result<Option<SigningIdentity>, CustodyError> {
    match identity {
        Some(identity) => kms.signer(identity).await.map(Some),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use std::{mem::discriminant, sync::Arc};

    use serde_json::json;
    use solana_keypair::Keypair;
    use solana_signer::Signer;

    use super::*;
    use crate::{
        database::Store,
        hub::{Hub, ServiceIdentity},
        storage_chain::LiveStorageChain,
        upstream::{ChainSource, Upstreams},
    };

    const KEY_ARN: &str =
        "arn:aws:kms:us-east-1:123456789012:key/1a2b3c4d-0000-4000-8000-000000000000";
    const UNREACHABLE: &str = "http://127.0.0.1:1";
    const AWS_KMS: [&str; 2] = ["--kms-region", "us-east-1"];
    const LOCAL_KMS: [&str; 2] = ["--localnet-kms", "127.0.0.1:4566"];
    const STORE: [&str; 4] = [
        "--store-url",
        "sqlite::memory:",
        "--prover-url",
        UNREACHABLE,
    ];

    fn address(seed: u8) -> String {
        Address::new_from_array([seed; 32]).to_string()
    }

    /// The auditor secret and the service signer, then each named role under the address of its seed.
    fn custody(roles: &[(&str, u8)]) -> String {
        let wrapped = json!({ "key_arn": KEY_ARN, "ciphertext": "AA==" });
        let signer = |seed: u8| json!({ "key_arn": KEY_ARN, "address": address(seed) });
        let mut manifest = json!({ "auditor": wrapped, "service": signer(1) });
        for (role, seed) in roles {
            manifest[*role] = match *role {
                "grant_rows" => wrapped.clone(),
                _ => signer(*seed),
            };
        }
        manifest.to_string()
    }

    fn serve(flags: &[&str], custody: &str) -> Result<ServeArgs, clap::Error> {
        let fixed = [
            "ring-rpc",
            "serve",
            "--deployment",
            "test",
            "--custody-json",
            custody,
        ];
        match Cli::try_parse_from(fixed.iter().chain(flags))?.command {
            Command::Serve(args) => Ok(*args),
            _ => panic!("serve parses as another command"),
        }
    }

    /// The manifest the arguments accept, no KMS call is made.
    fn checked(flags: &[&str], custody: &str) -> Result<RpcCustody, CustodyConfigError> {
        let args = serve(flags, custody).expect("serve flags");
        args.checked_manifest(&args.custody_json()?, &args.secrets.kms.endpoint()?)
    }

    fn checked_on_aws(flags: &[&str], custody: &str) -> Result<RpcCustody, CustodyConfigError> {
        checked(&[&AWS_KMS, flags].concat(), custody)
    }

    #[test]
    fn each_role_option_without_its_counterpart_is_refused_by_name() {
        use CustodyConfigError::*;
        let ring = address(9);
        let stored = |flags: &[&'static str]| [&STORE[..], flags].concat();
        let cases: Vec<(&str, Vec<&str>, String, CustodyConfigError)> = vec![
            (
                "a store without a storage authority",
                stored(&[]),
                custody(&[]),
                StorageAuthority,
            ),
            (
                "a storage authority without a store",
                vec![],
                custody(&[("storage_authority", 2)]),
                StorageAuthority,
            ),
            (
                "a prover without a store or an applier",
                vec!["--prover-url", UNREACHABLE],
                custody(&[]),
                Prover,
            ),
            (
                "an applier without a prover",
                vec![],
                custody(&[("enrollment_applier", 5)]),
                Prover,
            ),
            (
                "a co-signer without a store",
                vec![],
                custody(&[("co_signer", 3)]),
                SigningStore,
            ),
            (
                "a fee payer without a store",
                vec![],
                custody(&[("fee_payer", 4)]),
                SigningStore,
            ),
            (
                "approval classes without a co-signer",
                vec!["--co-signer-approve", "transfers"],
                custody(&[]),
                Approval,
            ),
            (
                "a relay budget without a fee payer",
                vec!["--relay-budget-lamports", "1"],
                custody(&[]),
                RelayBudget,
            ),
            (
                "a relay signer budget without a fee payer",
                vec![
                    "--relay-budget-lamports",
                    "0",
                    "--relay-signer-budget-lamports",
                    "1",
                ],
                custody(&[]),
                RelayBudget,
            ),
            (
                "a named ring without an applier",
                vec!["--enrollment-apply-ring", &ring],
                custody(&[]),
                Enrollment,
            ),
            (
                "an enrollment budget without an applier",
                vec!["--enrollment-budget-lamports", "1"],
                custody(&[]),
                Enrollment,
            ),
            (
                "an enrollment ring budget without an applier",
                vec!["--enrollment-ring-budget-lamports", "0"],
                custody(&[]),
                Enrollment,
            ),
            (
                "the default ring count stated without an applier",
                vec!["--enrollment-max-rings", "256"],
                custody(&[]),
                Enrollment,
            ),
            (
                "database reader grants without a grant rows secret",
                stored(&["--reader-grants", "database"]),
                custody(&[("storage_authority", 2)]),
                GrantRows,
            ),
            (
                "two roles pinning one address",
                stored(&[]),
                custody(&[("storage_authority", 2), ("co_signer", 2)]),
                SharedAddress,
            ),
            (
                "a role pinning the service address",
                stored(&[]),
                custody(&[("storage_authority", 1)]),
                SharedAddress,
            ),
        ];
        for (case, flags, custody, expected) in cases {
            let refused = checked_on_aws(&flags, &custody).expect_err(case);
            assert_eq!(
                discriminant(&refused),
                discriminant(&expected),
                "{case} refused as {refused}"
            );
        }
    }

    #[test]
    fn a_manifest_with_every_role_and_its_options_is_accepted() {
        let ring = address(9);
        let flags = [
            &STORE[..],
            &[
                "--replica-id",
                "replica-a",
                "--co-signer-approve",
                "transfers,deposits",
                "--relay-budget-lamports",
                "1000",
                "--relay-signer-budget-lamports",
                "100",
                "--enrollment-apply-ring",
                &ring,
                "--enrollment-budget-lamports",
                "1000",
                "--enrollment-ring-budget-lamports",
                "7",
                "--enrollment-max-rings",
                "4",
                "--reader-grants",
                "database",
            ],
        ]
        .concat();
        let roles = [
            ("storage_authority", 2),
            ("co_signer", 3),
            ("fee_payer", 4),
            ("enrollment_applier", 5),
            ("grant_rows", 0),
        ];
        let manifest = checked_on_aws(&flags, &custody(&roles)).expect("every role");
        assert_eq!(manifest.signers().count(), 5);
        checked_on_aws(&[], &custody(&[])).expect("the auditor and the service alone");
    }

    #[test]
    fn an_applier_serves_without_a_store_and_its_ring_budget_defaults_to_a_tenth() {
        let applier = custody(&[("enrollment_applier", 5)]);
        let prover = ["--prover-url", UNREACHABLE];
        let budgeted = [&prover[..], &["--enrollment-budget-lamports", "1000"]].concat();
        checked_on_aws(&budgeted, &applier).expect("an applier without a store");
        let defaults = serve(&budgeted, &applier).expect("flags");
        assert_eq!(defaults.enrollment_ring_budget(), 100);
        assert_eq!(
            defaults.enrollment_max_rings(),
            DEFAULT_ENROLLMENT_MAX_RINGS
        );

        let stated = [
            &budgeted[..],
            &[
                "--enrollment-ring-budget-lamports",
                "7",
                "--enrollment-max-rings",
                "3",
            ],
        ]
        .concat();
        let stated = serve(&stated, &applier).expect("flags");
        assert_eq!(stated.enrollment_ring_budget(), 7);
        assert_eq!(stated.enrollment_max_rings(), 3);
    }

    #[test]
    fn a_local_kms_serves_only_a_loopback_bind_and_cluster() {
        let cases: [(&[&str], bool); 6] = [
            (&[], true),
            (&["--rpc-url", "http://localhost:8899"], true),
            (&["--bind", "::1", "--rpc-url", "http://[::1]:8899"], true),
            (&["--bind", "0.0.0.0"], false),
            (&["--rpc-url", "https://api.devnet.solana.com"], false),
            (&["--rpc-url", "not a url"], false),
        ];
        for (flags, accepted) in cases {
            let checked = checked(&[&LOCAL_KMS, flags].concat(), &custody(&[]));
            match accepted {
                true => drop(checked.expect("a loopback deployment")),
                false => assert!(
                    matches!(checked, Err(CustodyConfigError::RemoteCluster)),
                    "{flags:?}"
                ),
            }
        }
        let remote = ["--bind", "0.0.0.0", "--rpc-url", "https://rpc.example"];
        checked_on_aws(&remote, &custody(&[])).expect("AWS KMS serves a remote cluster");
    }

    #[test]
    fn the_kms_endpoint_is_one_region_or_one_local_address() {
        let endpoint =
            |flags: &[&str]| serve(flags, &custody(&[])).map(|args| args.secrets.kms.endpoint());
        assert!(matches!(
            endpoint(&["--kms-region", "eu-west-1"]).expect("flags"),
            Ok(KmsEndpoint::Aws { region }) if region == "eu-west-1"
        ));
        assert!(matches!(
            endpoint(&LOCAL_KMS).expect("flags"),
            Ok(KmsEndpoint::Localnet(address)) if address.port() == 4566
        ));
        assert!(matches!(
            endpoint(&["--kms-region", ""]).expect("flags"),
            Err(CustodyConfigError::MissingRegion)
        ));
        assert!(endpoint(&[&AWS_KMS[..], &LOCAL_KMS].concat()).is_err());
    }

    #[test]
    fn an_invalid_manifest_is_refused_as_a_manifest_error() {
        let mut unknown: serde_json::Value = serde_json::from_str(&custody(&[])).expect("json");
        unknown["operator"] = unknown["service"].clone();
        let mut alias = unknown.clone();
        alias.as_object_mut().expect("object").remove("operator");
        alias["service"]["key_arn"] = json!("alias/ring-rpc-service");
        let mut no_service = alias.clone();
        no_service
            .as_object_mut()
            .expect("object")
            .remove("service");
        for manifest in [
            "{".to_owned(),
            unknown.to_string(),
            alias.to_string(),
            no_service.to_string(),
        ] {
            assert!(
                matches!(
                    checked_on_aws(&[], &manifest),
                    Err(CustodyConfigError::Manifest(_))
                ),
                "{manifest}"
            );
        }
    }

    #[test]
    fn a_manifest_comes_from_one_flag_and_a_missing_file_is_a_file_error() {
        let parse = |flags: &[&str]| {
            let fixed = ["ring-rpc", "serve", "--deployment", "test"];
            Cli::try_parse_from(fixed.iter().chain(&AWS_KMS).chain(flags))
        };
        assert!(parse(&[]).is_err());
        assert!(parse(&["--custody", "/custody.json", "--custody-json", "{}"]).is_err());
        let Command::Serve(args) = parse(&["--custody", "/nonexistent/custody.json"])
            .expect("a manifest path")
            .command
        else {
            panic!("serve parses as another command");
        };
        assert!(matches!(
            args.custody_json(),
            Err(CustodyConfigError::File(_))
        ));
    }

    fn provision(flags: &[&str]) -> Result<ProvisionArgs, clap::Error> {
        let genesis = Hash::new_from_array([7; 32]).to_string();
        let fixed = [
            "ring-rpc",
            "provision",
            "--deployment",
            "test",
            "--wrapping-key-arn",
            KEY_ARN,
            "--genesis-hash",
            &genesis,
        ];
        match Cli::try_parse_from(fixed.iter().chain(&AWS_KMS).chain(flags))?.command {
            Command::Provision(args) => Ok(args),
            _ => panic!("provision parses as another command"),
        }
    }

    #[test]
    fn only_a_single_ring_auditor_secret_binds_a_ring() {
        let ring = Address::new_from_array([9; 32]);
        let ring_flag = ["--ring-program-id", &ring.to_string()];
        let purpose = |name: &'static str, flags: &[&str]| {
            provision(&[&["--purpose", name], flags].concat()).map(|args| args.secret_purpose())
        };
        assert!(matches!(
            purpose("auditor", &ring_flag).expect("flags"),
            Ok(SecretPurpose::Auditor(bound)) if bound == ring
        ));
        assert!(matches!(
            purpose("auditor-root", &[]).expect("flags"),
            Ok(SecretPurpose::AuditorRoot)
        ));
        assert!(matches!(
            purpose("grant-rows", &[]).expect("flags"),
            Ok(SecretPurpose::GrantRows)
        ));
        for name in ["auditor-root", "grant-rows"] {
            assert!(
                matches!(
                    purpose(name, &ring_flag).expect("flags"),
                    Err(CustodyConfigError::AuditorRing)
                ),
                "{name}"
            );
        }
        assert!(purpose("auditor", &[]).is_err());
    }

    #[tokio::test]
    async fn each_role_signs_with_the_identity_the_hub_was_given() {
        let [service, storage, co_signer] =
            [1, 2, 3].map(|seed| Keypair::new_from_array([seed; 32]));
        let addresses = [&service, &storage, &co_signer].map(Signer::pubkey);
        let upstreams = Upstreams {
            indexer_url: UNREACHABLE,
            rpc_url: UNREACHABLE,
            timeout: Duration::from_secs(1),
        };
        let chain = LiveStorageChain::connect(upstreams, UNREACHABLE).expect("chain");
        let identity = ServiceIdentity {
            genesis_hash: [1; 32],
            service: Arc::new(service),
        };
        let hub = Hub::builder(ChainSource::connect(upstreams).expect("source"), identity)
            .with_store(Store::open("sqlite::memory:", 1).await.expect("store"))
            .with_storage_chain(Arc::new(chain), Arc::new(storage))
            .with_co_signer(Arc::new(co_signer))
            .derived(RootSecret::from_bytes([4; 32]).expect("root"))
            .expect("hub");

        assert_eq!(
            [
                Some(hub.service_pubkey()),
                Some(hub.record_writer().expect("writer").authority()),
                hub.co_signer().signer(),
            ],
            addresses.map(Some)
        );
    }
}
