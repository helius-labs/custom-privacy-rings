//! The operator manifest, the KMS pins `ring.toml` names.

use std::path::{Path, PathBuf};

use anyhow::{ensure, Result};
use custom_ring_interface::RingRole;
use serde::{Deserialize, Serialize};
use solana_address::Address;
use zolana_operator_custody::{KmsEndpoint, SignerRef, WrappedSecret};

use crate::{
    config::parse_json,
    context::{config_root, expanded},
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorManifest {
    pub region: String,
    pub deployment: String,
    pub localnet_kms: Option<std::net::SocketAddr>,
    pub program: SignerRef,
    pub upgrade_authority: SignerRef,
    pub admin: SignerRef,
    pub policy: SignerRef,
    pub pauser: SignerRef,
    pub cosign_admin: SignerRef,
    pub enrollment_writer: SignerRef,
    pub delegate: Option<SignerRef>,
    pub auditor: Option<WrappedSecret>,
    pub rpc_custody: Option<PathBuf>,
}

impl OperatorManifest {
    pub fn load(path: &Path) -> Result<Self> {
        let manifest: Self = parse_json(path)?;
        ensure!(
            !manifest.deployment.is_empty() && manifest.deployment == manifest.deployment.trim(),
            "{} names an empty or padded KMS deployment",
            path.display()
        );
        Ok(manifest)
    }

    pub(crate) fn load_for(path: &Path, program_id: Address) -> Result<Self> {
        let manifest = Self::load(path)?;
        ensure!(
            manifest.program.address == program_id,
            "{} pins program {}, ring.toml names {program_id}",
            path.display(),
            manifest.program.address
        );
        Ok(manifest)
    }

    pub fn kms_endpoint(&self) -> KmsEndpoint {
        match self.localnet_kms {
            Some(endpoint) => KmsEndpoint::Localnet(endpoint),
            None => KmsEndpoint::Aws {
                region: self.region.clone(),
            },
        }
    }

    /// Relative to the directory of the manifest at `manifest_path`.
    pub(crate) fn rpc_custody_path(&self, manifest_path: &Path) -> Result<Option<PathBuf>> {
        self.rpc_custody
            .as_deref()
            .map(|path| expanded(&config_root(manifest_path), path))
            .transpose()
    }

    pub(crate) fn role(&self, role: RingRole) -> &SignerRef {
        match role {
            RingRole::Admin => &self.admin,
            RingRole::Policy => &self.policy,
            RingRole::Pauser => &self.pauser,
            RingRole::CosignAdmin => &self.cosign_admin,
            RingRole::EnrollmentWriter => &self.enrollment_writer,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use serde_json::{json, Value};

    use super::*;

    const ROLES: [&str; 7] = [
        "program",
        "upgrade_authority",
        "admin",
        "policy",
        "pauser",
        "cosign_admin",
        "enrollment_writer",
    ];

    pub(crate) fn pinned_address(role: &str) -> Address {
        let index = ROLES.iter().position(|name| *name == role).expect("role");
        Address::new_from_array([index as u8 + 1; 32])
    }

    pub(crate) fn pin(address: Address) -> Value {
        json!({
            "key_arn": format!("arn:aws:kms:us-east-1:123456789012:key/key-{}", address.as_ref()[0]),
            "address": address.to_string(),
        })
    }

    pub(crate) fn document() -> Value {
        let mut document = json!({ "region": "us-east-1", "deployment": "sample" });
        for role in ROLES {
            document[role] = pin(pinned_address(role));
        }
        document
    }

    pub(crate) fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ring-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn manifest(document: Value) -> OperatorManifest {
        serde_json::from_value(document).expect("manifest")
    }

    #[test]
    fn a_manifest_is_taken_only_for_the_program_it_pins() {
        let dir = scratch_dir("manifest-program");
        let path = dir.join("operator.json");
        std::fs::write(&path, document().to_string()).expect("write");
        let program = pinned_address("program");
        let loaded = OperatorManifest::load_for(&path, program).expect("pinned program");
        assert_eq!(loaded.program.address, program);
        let other = pinned_address("admin");
        let refused = OperatorManifest::load_for(&path, other).expect_err("another program");
        assert_eq!(
            refused.to_string(),
            format!(
                "{} pins program {program}, ring.toml names {other}",
                path.display()
            )
        );
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn a_manifest_without_a_deployment_a_key_arn_or_a_role_is_refused() {
        let dir = scratch_dir("manifest-refusals");
        let path = dir.join("operator.json");
        const ALIAS: &str = "arn:aws:kms:us-east-1:123456789012:alias/ring";
        type Edit = fn(&mut Value);
        let edits: [(Edit, &str); 7] = [
            (
                |document| document["deployment"] = json!(""),
                "names an empty or padded KMS deployment",
            ),
            (
                |document| document["deployment"] = json!(" \t"),
                "names an empty or padded KMS deployment",
            ),
            (
                |document| document["deployment"] = json!(" sample "),
                "names an empty or padded KMS deployment",
            ),
            (
                |document| document["policy"]["key_arn"] = json!(ALIAS),
                "invalid KMS key ARN",
            ),
            (
                |document| document["extra"] = json!(1),
                "unknown field `extra`",
            ),
            (
                |document| document["policy"]["alias"] = json!("ring"),
                "unknown field `alias`",
            ),
            (
                |document| {
                    document.as_object_mut().expect("object").remove("pauser");
                },
                "missing field `pauser`",
            ),
        ];
        for (edit, reason) in edits {
            let mut document = document();
            edit(&mut document);
            std::fs::write(&path, document.to_string()).expect("write");
            let refused = format!("{:#}", OperatorManifest::load(&path).expect_err(reason));
            assert!(refused.contains(reason), "{refused}");
        }
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn the_rpc_custody_path_resolves_against_the_manifest_directory() {
        let resolved = |recorded: Option<&str>, manifest_path: &str| {
            let mut document = document();
            document["rpc_custody"] = json!(recorded);
            manifest(document)
                .rpc_custody_path(Path::new(manifest_path))
                .expect("resolved")
        };
        for (recorded, manifest_path, expected) in [
            ("custody.json", "ops/operator.json", "ops/custody.json"),
            ("custody.json", "operator.json", "./custody.json"),
            (
                "../custody.json",
                "/etc/ring/operator.json",
                "/etc/ring/../custody.json",
            ),
            (
                "/var/custody.json",
                "ops/operator.json",
                "/var/custody.json",
            ),
        ] {
            assert_eq!(
                resolved(Some(recorded), manifest_path),
                Some(PathBuf::from(expected))
            );
        }
        let home = resolved(Some("~/ring/custody.json"), "ops/operator.json").expect("home");
        assert!(home.is_absolute() && home.ends_with("ring/custody.json"));
        assert!(!home.starts_with("ops"));
        assert_eq!(resolved(None, "ops/operator.json"), None);
    }

    #[test]
    fn a_local_kms_address_wins_over_the_region() {
        let aws = manifest(document());
        assert_eq!(
            aws.kms_endpoint(),
            KmsEndpoint::Aws {
                region: "us-east-1".to_owned()
            }
        );
        let mut document = document();
        document["localnet_kms"] = json!("127.0.0.1:4599");
        let endpoint = "127.0.0.1:4599".parse().expect("address");
        assert_eq!(
            manifest(document).kms_endpoint(),
            KmsEndpoint::Localnet(endpoint)
        );
    }

    #[test]
    fn each_ring_role_signs_with_its_own_pin() {
        let manifest = manifest(document());
        for (role, name) in [
            (RingRole::Admin, "admin"),
            (RingRole::Policy, "policy"),
            (RingRole::Pauser, "pauser"),
            (RingRole::CosignAdmin, "cosign_admin"),
            (RingRole::EnrollmentWriter, "enrollment_writer"),
        ] {
            assert_eq!(manifest.role(role).address, pinned_address(name));
        }
    }
}
