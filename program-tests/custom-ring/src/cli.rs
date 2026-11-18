//! The operator cli driven from a test, over a ring directory the harness writes.

use std::{
    path::PathBuf,
    process::{Command, Output},
};

use anyhow::{anyhow, Context, Result};
use custom_ring_cli::{config::OperatorManifest, ListName, ModeName, SENDER_KEYPAIR_FILE};
use custom_ring_sdk::CustomRing;
use solana_address::Address;
use solana_keypair::Keypair;
use solana_loader_v3_interface::instruction as loader;
use solana_signer::Signer;
use spr_keypair::{ShieldedKeypair, X25519Keypair};
use spr_test_utils::localnet::isolated_temp_path;
use zolana_operator_custody::{Kms, SecretContext, SecretPurpose};
use zolana_ring_policy::ListId;

use crate::{
    policy::{owner_member, wait_for_list},
    ring_rpc::{LocalRingRpc, RingRpcSpec},
    shared::{prover_url, send, TestEnv},
};

const RING_CLI: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/debug/zolana-ring"
);

pub struct RingProject {
    pub policy_authority: Address,
    pub auditor: X25519Keypair,
    pub ring_rpc: LocalRingRpc,
    dir: PathBuf,
    program_id: Address,
}

pub struct RingToml<'a> {
    pub env: &'a TestEnv,
    pub ring_rpc: &'a str,
    /// A `[policy]` block selects the policy tier.
    pub policy: Option<&'a str>,
    pub mode: ModeName,
}

pub struct ListWrite<'a> {
    pub env: &'a TestEnv,
    pub list_id: ListId,
    pub member: &'a ShieldedKeypair,
    pub present: bool,
}

impl RingProject {
    /// Blocks until the indexer serves the written record.
    pub fn write_list(&self, write: ListWrite<'_>) -> Result<()> {
        let verb = if write.present { "add" } else { "clear" };
        let list = ListName::of(write.list_id)?.to_string();
        self.run(&[
            "list",
            verb,
            &list,
            "--owner",
            &owner_tag_arg(write.member)?,
        ])?;
        let ring = CustomRing::new(self.program_id);
        let member = owner_member(write.member)?;
        wait_for_list(write.env, ring, write.list_id, member, write.present)
    }
}

pub fn owner_tag_arg(owner: &ShieldedKeypair) -> Result<String> {
    let tag = owner.signing_pubkey().confidential_view_tag()?;
    Ok(Address::new_from_array(tag).to_string())
}

pub fn merged(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

impl RingProject {
    pub fn create(env: &TestEnv) -> Result<Self> {
        let dir = PathBuf::from(isolated_temp_path("zolana-ring-cli"));
        std::fs::create_dir_all(&dir)?;
        let path = PathBuf::from(std::env::var("RING_TEST_OPERATOR_MANIFEST")?);
        let mut operator = OperatorManifest::load(&path)?;
        let program_id = operator.program.address;
        let genesis = env.client.rpc().genesis_hash()?;
        let context = || SecretContext {
            deployment: &operator.deployment,
            genesis_hash: &genesis,
            purpose: SecretPurpose::Auditor(program_id),
        };
        let endpoint = operator
            .localnet_kms
            .context("missing local KMS endpoint")?;
        let kms = Kms::localnet_for_genesis(endpoint, &genesis)?;
        let wrapping_key = std::env::var("RING_TEST_WRAPPING_KEY")?;
        let wrapped = env
            .client
            .block_on(async { anyhow::Ok(kms.provision(&wrapping_key, context()).await?) })?;
        let secret = env
            .client
            .block_on(async { anyhow::Ok(kms.open(&wrapped, context()).await?) })?;
        let auditor = X25519Keypair::from_bytes(&secret);
        let writer = env
            .client
            .block_on(async { anyhow::Ok(kms.signer(&operator.enrollment_writer).await?) })?;
        operator.auditor = Some(wrapped);
        operator.rpc_custody = None;
        serde_json::to_writer(std::fs::File::create(dir.join("operator.json"))?, &operator)?;
        send(
            env.client.rpc(),
            &env.payer,
            &[loader::set_upgrade_authority(
                &program_id,
                &env.payer.pubkey(),
                Some(&operator.upgrade_authority.address),
            )],
        )?;
        let ring_rpc = RingRpcSpec {
            env,
            ring: program_id,
            auditor: auditor.clone(),
        }
        .serve_with_writer(writer)?;
        Ok(Self {
            policy_authority: operator.policy.address,
            program_id,
            dir,
            auditor,
            ring_rpc,
        })
    }

    pub fn enrollment_rpc(&self) -> custom_ring_cli::RingRpcClient {
        custom_ring_cli::RingRpcClient::new(&self.ring_rpc.url)
    }

    pub fn member_fee_payer(&self) -> Result<&'static str> {
        let path = "fee-payer.json";
        solana_keypair::write_keypair_file(&Keypair::new(), self.dir.join(path))
            .map_err(|error| anyhow!("write member fee payer {error}"))?;
        Ok(path)
    }

    pub fn write_config(&self, toml: RingToml<'_>) -> Result<()> {
        let policy = toml
            .policy
            .map(|block| format!("{}\n\n", block.trim_end()))
            .unwrap_or_default();
        std::fs::write(
            self.dir.join("ring.toml"),
            format!(
                "name = \"cli\"\ntarget = \"localnet\"\nprogram_id = \"{}\"\nmode = \"{}\"\n\
                 operator_manifest = \"operator.json\"\n\n{policy}\
                 [localnet]\nrpc = \"{}\"\nindexer = \"{}\"\nprover = \"{}\"\nring_rpc = \"{}\"\nring_rpc_pubkey = \"{}\"\n\n\
                 [devnet]\nrpc = \"https://api.devnet.solana.com\"\nindexer = \"http://indexer.invalid\"\n\
                 prover = \"http://prover.invalid\"\nring_rpc = \"http://ring.invalid\"\n",
                self.program_id,
                toml.mode,
                toml.env.rpc_url,
                toml.env.indexer_url,
                prover_url(),
                toml.ring_rpc,
                self.ring_rpc.service,
            ),
        )?;
        Ok(())
    }

    /// Re-pins under the policy authority.
    pub fn policy_set(&self, block: &str) -> Result<String> {
        let path = self.dir.join("ring.toml");
        let text = std::fs::read_to_string(&path)?;
        std::fs::write(&path, with_policy_block(&text, block)?)?;
        self.run(&["policy", "set", "--yes"])
    }

    /// Exits non-zero while `ring.toml` and the chain disagree.
    pub fn policy_check(&self) -> Result<Output> {
        self.output(&["policy", "check"])
    }

    pub fn demo_sender(&self) -> Result<ShieldedKeypair> {
        let keypair = solana_keypair::read_keypair_file(self.dir.join(SENDER_KEYPAIR_FILE))
            .map_err(|e| anyhow!("read the demo sender keypair {e}"))?;
        Ok(ShieldedKeypair::from_keypair(&keypair)?)
    }

    pub fn output(&self, args: &[&str]) -> Result<Output> {
        Command::new(RING_CLI)
            .arg("--config")
            .arg(self.dir.join("ring.toml"))
            .args(args)
            .output()
            .with_context(|| format!("run zolana-ring {}", args.join(" ")))
    }

    /// Stdout and stderr merged, a non-zero exit is an error.
    pub fn run(&self, args: &[&str]) -> Result<String> {
        let output = self.output(args)?;
        let text = merged(&output);
        if !output.status.success() {
            return Err(anyhow!("zolana-ring {} failed\n{text}", args.join(" ")));
        }
        Ok(text)
    }

    pub fn remove(self) -> Result<()> {
        std::fs::remove_dir_all(self.dir)?;
        Ok(())
    }
}

/// The block spans `[policy]` to `[localnet]`, the layout `write_config` writes.
fn with_policy_block(text: &str, block: &str) -> Result<String> {
    let tail = text
        .find("[localnet]\n")
        .ok_or_else(|| anyhow!("ring.toml has no [localnet] table"))?;
    let head = text
        .find("[policy]")
        .filter(|start| *start < tail)
        .unwrap_or(tail);
    Ok(format!(
        "{}{}\n\n{}",
        &text[..head],
        block.trim_end(),
        &text[tail..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_policy_block_is_swapped_ahead_of_the_cluster_tables() {
        let written = "name = \"x\"\n\n[policy]\n\n[[policy.rules]]\nsubject = \"sender\"\n\
                       forbid = \"frozen\"\n\n[localnet]\nrpc = \"a\"\n\n[devnet]\nrpc = \"b\"\n";
        let block = "[policy]\n\n[policy.sources.localnet]\nblock = \"c\"\n";
        assert_eq!(
            with_policy_block(written, block).expect("replace"),
            "name = \"x\"\n\n[policy]\n\n[policy.sources.localnet]\nblock = \"c\"\n\n\
             [localnet]\nrpc = \"a\"\n\n[devnet]\nrpc = \"b\"\n"
        );
        assert_eq!(
            with_policy_block("name = \"x\"\n\n[localnet]\nrpc = \"a\"\n", "[policy]")
                .expect("insert"),
            "name = \"x\"\n\n[policy]\n\n[localnet]\nrpc = \"a\"\n"
        );
        assert!(with_policy_block("name = \"x\"\n", "[policy]").is_err());
    }
}
