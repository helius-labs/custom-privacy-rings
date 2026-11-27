//! `new`, the ring directory every other command reads.

use std::path::{Path, PathBuf};

use anyhow::{ensure, Context as _, Result};
use serde::Deserialize;

use crate::{
    config::{
        expand_tilde, parse_ring_toml, Base58Address, OperatorManifest, RingConfig, Target, Urls,
    },
    file, line,
    policy::PolicySpec,
    ui::{self, icon, Ask},
    wizard::{live_catalogue, Wizard},
    NewArgs, RING_TOML,
};

const DEVNET_RPC: &str = "https://api.devnet.solana.com";
const DEVNET_INDEXER: &str = "https://d2xah7tnhdhcom.cloudfront.net";
const DEVNET_PROVER: &str = "https://dd73hh473h6ha.cloudfront.net";
const DEVNET_RING_RPC: &str = "https://d24brah9h1i4q9.cloudfront.net";
const DEVNET_RING_RPC_PUBKEY: &str = "C8TeRCdueRdXj3y4uFDtNywYThc3qeiH2ZGfcbYZzVSs";

/// A `ring.toml` or a file holding only its `[policy]` table.
#[derive(Deserialize)]
struct PolicyFile {
    policy: PolicySpec,
}

/// Every answer is taken before anything is written.
pub fn run(args: NewArgs, ask: &mut dyn Ask, catalogue: Option<&str>) -> Result<()> {
    ensure!(
        args.name.is_some() || ask.interactive(),
        "a name is needed without a terminal, pass it as the argument"
    );
    if let Some(name) = &args.name {
        refuse_existing(&args.dest.join(name))?;
    }
    let preset = args.policy_from.as_deref().map(read_policy).transpose()?;
    let local = |port: u16| format!("http://127.0.0.1:{port}");
    let answers = Wizard {
        ask: &mut *ask,
        curators: &mut |target: Target, rpc_url: &str| live_catalogue(catalogue, target, rpc_url),
        localnet: Urls {
            rpc: local(8899),
            indexer: local(8786),
            prover: local(3001),
            ring_rpc: local(8785),
            ring_rpc_pubkey: None,
            prover_tee_policy: None,
        },
        devnet: Urls {
            rpc: DEVNET_RPC.to_owned(),
            indexer: DEVNET_INDEXER.to_owned(),
            prover: DEVNET_PROVER.to_owned(),
            ring_rpc: DEVNET_RING_RPC.to_owned(),
            ring_rpc_pubkey: Some(DEVNET_RING_RPC_PUBKEY.to_owned()),
            prover_tee_policy: None,
        },
    }
    .run(args.name, preset)?;
    let ring_dir = args.dest.join(&answers.name);
    refuse_existing(&ring_dir)?;
    let manifest = OperatorManifest::load(&expand_tilde(&args.operator_manifest)?)?;
    let config = RingConfig {
        name: answers.name,
        target: answers.target,
        program_id: Base58Address(manifest.program.address),
        operator_manifest: Some(recorded_path(args.operator_manifest)?),
        mode: answers.mode,
        policy: answers.policy,
        cosigner: None,
        localnet: answers.localnet,
        devnet: answers.devnet,
    };
    let rendered = config.render()?;
    ui::heading(icon::WIZARD, "preview");
    print!("{rendered}");
    ensure!(
        ask.confirm("write the ring directory?", true)?,
        "nothing written, the preview was not confirmed"
    );

    file::create_dir_all(&args.dest)?;
    std::fs::create_dir(&ring_dir)
        .with_context(|| format!("cannot create ring directory {}", ring_dir.display()))?;
    file::write(&ring_dir.join(RING_TOML), rendered)?;
    file::write(&ring_dir.join(".gitignore"), "keys/\n.env\n")?;

    ui::heading(icon::RING, &config.name);
    line("ring", ring_dir.display());
    line("program id", manifest.program.address);
    println!(
        "next, `cd {}` and `zolana-ring localnet` or `zolana-ring devnet`",
        ring_dir.display()
    );
    Ok(())
}

/// Compiled for every cluster before it is taken.
pub(crate) fn read_policy(path: &Path) -> Result<PolicySpec> {
    let file: PolicyFile = parse_ring_toml(path)?;
    file.policy.check()?;
    Ok(file.policy)
}

/// ring.toml resolves a relative path against the ring directory.
fn recorded_path(given: PathBuf) -> Result<PathBuf> {
    if given.is_absolute() || given.starts_with("~") {
        return Ok(given);
    }
    std::path::absolute(&given).with_context(|| format!("cannot resolve {}", given.display()))
}

fn refuse_existing(dir: &Path) -> Result<()> {
    ensure!(
        !dir.exists(),
        "destination {} already exists",
        dir.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use zolana_ring_policy::{ListId, Rule, RuleTable, Subject};

    use super::*;
    use crate::{
        config::ModeName,
        operator_manifest::tests::document as manifest_document,
        policy::grammar::PolicyError,
        ui::{Answer, Defaults, Scripted},
    };

    fn temp(name: &str) -> PathBuf {
        let dest = std::env::temp_dir().join(format!("ring-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dest);
        dest
    }

    fn args(name: Option<&str>, dest: &Path, silent: bool) -> NewArgs {
        let manifest = dest.join("operator.json");
        std::fs::create_dir_all(dest).unwrap();
        std::fs::write(&manifest, manifest_document().to_string()).unwrap();
        NewArgs {
            name: name.map(str::to_owned),
            dest: dest.to_path_buf(),
            silent,
            operator_manifest: manifest,
            policy_from: None,
        }
    }

    #[test]
    fn a_silent_new_writes_a_loadable_ring_and_refuses_to_overwrite() {
        let dest = temp("new");
        run(args(Some("smoke"), &dest, true), &mut Defaults, None).expect("new");

        let ring_dir = dest.join("smoke");
        let config = RingConfig::load(&ring_dir.join(RING_TOML)).expect("load");
        let recorded = config.operator_manifest.as_deref().expect("recorded");
        let manifest = OperatorManifest::load(recorded).expect("manifest");
        assert_eq!(config.program_id.0, manifest.program.address);
        assert_eq!(config.target, Target::Localnet);
        assert_eq!(recorded, dest.join("operator.json"));
        assert_eq!(config.mode, ModeName::Confidential);
        assert!(config.policy.is_none());
        assert_eq!(
            config.devnet.ring_rpc_pubkey.as_deref(),
            Some(DEVNET_RING_RPC_PUBKEY)
        );
        assert!(config.localnet.ring_rpc_is_local());
        assert_eq!(
            std::fs::read_to_string(ring_dir.join(".gitignore")).expect("gitignore"),
            "keys/\n.env\n"
        );
        let exists = |result: Result<()>| {
            result.is_err_and(|error| error.to_string().ends_with("already exists"))
        };
        assert!(exists(run(
            args(Some("smoke"), &dest, true),
            &mut Defaults,
            None
        )));
        // Refused before the first question, an empty script answers nothing.
        assert!(exists(run(
            args(Some("smoke"), &dest, false),
            &mut Scripted::new([]),
            None
        )));
        let unnamed = run(args(None, &dest, true), &mut Defaults, None).expect_err("no name");
        assert!(unnamed.to_string().starts_with("a name is needed"));
        std::fs::remove_dir_all(dest).expect("cleanup");
    }

    #[test]
    fn the_manifest_path_is_recorded_as_given_or_made_absolute() {
        let working_dir = std::env::current_dir().expect("working directory");
        for (given, recorded) in [
            (
                "/etc/ring/operator.json",
                PathBuf::from("/etc/ring/operator.json"),
            ),
            (
                "~/ring/operator.json",
                PathBuf::from("~/ring/operator.json"),
            ),
            ("ops/operator.json", working_dir.join("ops/operator.json")),
            ("./operator.json", working_dir.join("operator.json")),
        ] {
            assert_eq!(
                recorded_path(PathBuf::from(given)).expect("recorded"),
                recorded
            );
        }
    }

    #[test]
    fn a_policy_file_is_compiled_and_written_without_policy_questions() {
        let dest = temp("policy-from");
        std::fs::create_dir_all(&dest).expect("dest");
        let policy = dest.join("policy.toml");
        std::fs::write(
            &policy,
            "[policy]\n\n[[policy.rules]]\nsubject = \"sender\"\nforbid = \"frozen\"\n",
        )
        .expect("policy file");
        let mut ask = Scripted::new(
            ["", "", "", "", "", "", "", ""]
                .map(Answer::from)
                .into_iter()
                .chain([
                    Answer::Yes(true),
                    Answer::from("devnet"),
                    Answer::from("anonymous"),
                    Answer::Yes(true),
                ]),
        );
        run(
            NewArgs {
                policy_from: Some(policy.clone()),
                ..args(Some("governed"), &dest, false)
            },
            &mut ask,
            None,
        )
        .expect("new with a policy file");
        assert!(ask.is_drained());
        let config = RingConfig::load(&dest.join("governed").join(RING_TOML)).expect("load");
        assert_eq!(config.target, Target::Devnet);
        assert_eq!(config.mode, ModeName::Anonymous);
        let written_policy = config.policy.expect("policy tier");
        let compiled = written_policy.compile(Target::Devnet).expect("compiles");
        assert_eq!(
            compiled.rules,
            RuleTable::builder()
                .rule(Rule::forbid(Subject::Sender, ListId::Frozen))
                .build()
        );
        let text = std::fs::read_to_string(dest.join("governed").join(RING_TOML)).expect("read");
        assert!(text.contains("# the sender must not be on the frozen list\n[[policy.rules]]"));

        let example =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/own-blocklist/ring.toml");
        run(
            NewArgs {
                policy_from: Some(example),
                ..args(Some("from-example"), &dest, true)
            },
            &mut Defaults,
            None,
        )
        .expect("new from a full ring.toml");
        let config = RingConfig::load(&dest.join("from-example").join(RING_TOML)).expect("load");
        assert_eq!(config.policy.expect("policy tier").rules.len(), 1);

        std::fs::write(
            &policy,
            "[policy]\n\n[[policy.rules]]\nsubject = \"sender\"\nrequire = \"allow\"\nabove = 1\n",
        )
        .expect("policy file");
        let refused = run(
            NewArgs {
                policy_from: Some(policy),
                ..args(Some("refused"), &dest, true)
            },
            &mut Defaults,
            None,
        )
        .expect_err("a sender guard");
        assert_eq!(
            refused.downcast_ref::<PolicyError>(),
            Some(&PolicyError::SenderGuard { rule: 0 })
        );
        assert!(!dest.join("refused").exists());
        std::fs::remove_dir_all(dest).expect("cleanup");
    }
}
