use anyhow::{ensure, Context as _, Result};
use custom_ring_interface::RingRole;
use std::path::PathBuf;
use zolana_operator_custody::{Kms, SecretContext, SecretPurpose, SignerRef, SigningIdentity};

use crate::{config, fund, Context, Target};

impl Context {
    pub(crate) fn operator_manifest_path(&self) -> Result<PathBuf> {
        self.config.operator_manifest_path(&self.root)
    }

    pub(crate) fn operator_manifest(&self) -> Result<&config::OperatorManifest> {
        if let Some(manifest) = self.operators.get() {
            return Ok(manifest);
        }
        let manifest = config::OperatorManifest::load(&self.operator_manifest_path()?)?;
        ensure!(
            manifest.program.address == self.ring.program_id(),
            "the KMS program address differs from ring.toml"
        );
        Ok(self.operators.get_or_init(|| manifest))
    }

    pub(crate) fn signer(&self, identity: &SignerRef) -> Result<SigningIdentity> {
        let kms = self.kms()?;
        self.block(async { Ok(kms.signer(identity).await?) })
    }

    pub(crate) fn program_signer(&self) -> Result<SigningIdentity> {
        self.signer(&self.operator_manifest()?.program)
    }

    pub(crate) fn upgrade_authority_signer(&self) -> Result<SigningIdentity> {
        self.signer(&self.operator_manifest()?.upgrade_authority)
    }

    pub(crate) fn delegate_signer(&self) -> Result<SigningIdentity> {
        self.signer(
            self.operator_manifest()?
                .delegate
                .as_ref()
                .context("missing KMS delegate")?,
        )
    }

    pub(crate) fn role_signer(&self, role: RingRole) -> Result<SigningIdentity> {
        self.signer(self.operator_manifest()?.role(role))
    }

    pub(crate) fn authority(&self) -> Result<SigningIdentity> {
        self.role_signer(RingRole::Policy)
    }

    pub(crate) fn audit_key(&self) -> Result<spr_keypair::X25519Keypair> {
        let manifest = self.operator_manifest()?;
        let wrapped = manifest
            .auditor
            .as_ref()
            .context("missing wrapped ring auditor")?;
        let genesis_hash = self.rpc().genesis_hash()?;
        let context = SecretContext {
            deployment: &manifest.deployment,
            genesis_hash: &genesis_hash,
            purpose: SecretPurpose::Auditor(self.ring.program_id()),
        };
        let kms = self.kms()?;
        let secret =
            self.block(async { Ok::<_, anyhow::Error>(kms.open(wrapped, context).await?) })?;
        Ok(spr_keypair::X25519Keypair::from_bytes(&secret))
    }

    pub(crate) fn funded_authority(&self) -> Result<SigningIdentity> {
        let authority = self.authority()?;
        self.fund(authority.address(), fund::MIN_AUTHORITY_BALANCE)?;
        Ok(authority)
    }

    pub(crate) fn local_rpc_roles(&self) -> Result<Option<config::RpcSigningRoles>> {
        if self.config.target != Target::Localnet
            || !config::Urls::is_loopback(&self.config.urls().ring_rpc)
        {
            return Ok(None);
        }
        let operator_path = self.operator_manifest_path()?;
        if self.operators.get().is_none() && !operator_path.try_exists()? {
            return Ok(None);
        }
        let manifest = self.operator_manifest()?;
        if manifest.rpc_custody.is_none() {
            return Ok(None);
        }
        let path = manifest.rpc_custody_path(&operator_path)?;
        Ok(Some(config::RpcSigningRoles::load(&path)?))
    }

    fn kms(&self) -> Result<&Kms> {
        if let Some(kms) = self.kms.get() {
            return Ok(kms);
        }
        let manifest = self.operator_manifest()?;
        let kms = match manifest.localnet_kms {
            Some(endpoint) => {
                ensure!(
                    self.config.target == Target::Localnet
                        && config::Urls::is_loopback(&self.config.urls().rpc),
                    "the local KMS requires a loopback localnet RPC"
                );
                Kms::localnet_for_genesis(endpoint, &self.rpc().genesis_hash()?)?
            }
            None => {
                self.block(async { Ok::<_, anyhow::Error>(Kms::aws(&manifest.region).await?) })?
            }
        };
        Ok(self.kms.get_or_init(|| kms))
    }
}
