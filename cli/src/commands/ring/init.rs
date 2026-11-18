//! `init`, every step skipped when the chain already holds it, so a rerun resumes.

use anyhow::{bail, ensure, Context as _, Result};
use custom_ring_interface::{RingRole, RingRoles};
use custom_ring_sdk::{
    CustomRingConfig, InitialPolicy, Initialize, ListRegistry, PolicyTable, PolicyUpdate,
    RingError, RingKind, SourceOwner, UpdatePolicy,
};
use solana_address::Address;
use spr_keypair::X25519Pubkey;
use zolana_operator_custody::OperatorSigner;
use zolana_ring_client::rpc::{read_auditor_pubkey, write_auditor_pubkey};
use zolana_ring_policy::{ListId, RuleTable};

use crate::{
    catalogue,
    config::{expand_tilde, ModeName, Target},
    deploy, fund, line,
    policy::{self, CompiledPolicy, PolicySpec},
    status, Context, InitArgs, Trust,
};

const PRESENT: &str = "already present";
const CREATED: &str = "created";
const NEEDS_UPGRADE_AUTHORITY: &str = "the upgrade authority signs `Initialize`";
const POLICY_DRIFT: &str =
    "the pinned policy differs from ring.toml, `zolana-ring policy set` replaces the table";
const PARTIAL_INIT: &str =
    "the ring config exists without the policy or SPP registration `Initialize` creates";

/// `Initialize` fixes the auditor public key for the life of the ring.
#[must_use]
struct ConfigCreation<'a> {
    deployer: Option<&'a dyn OperatorSigner>,
    /// Holds the policy role and pays.
    policy_authority: &'a dyn OperatorSigner,
    auditor_pk: X25519Pubkey,
    policy: Option<&'a CompiledPolicy>,
    mode: ModeName,
    existing: Option<&'a CustomRingConfig>,
}

/// `Initialize` over the records Ring RPC holds.
#[must_use]
struct RingInitialization<'a> {
    deployer: &'a dyn OperatorSigner,
    policy_authority: &'a dyn OperatorSigner,
    roles: RingRoles,
    auditor_pk: X25519Pubkey,
    kind: RingKind,
    policy: Option<&'a CompiledPolicy>,
}

#[must_use]
struct PolicyPin<'a> {
    authority: &'a dyn OperatorSigner,
    policy: &'a CompiledPolicy,
    outcome: &'static str,
}

pub fn run(ctx: &Context, args: InitArgs) -> Result<()> {
    let key_file = ctx.path(&expand_tilde(&args.auditor_pubkey_file)?);
    let compile = |spec: &PolicySpec| spec.compile(ctx.config.target);
    let policy = ctx.config.policy.as_ref().map(compile).transpose()?;
    let policy_authority = ctx.funded_authority()?;
    let existing = ctx.block(ctx.ring.config(ctx.chain()))?;
    // Only the upgrade authority creates the config, the policy authority pins the rest.
    let upgrade_authority = match existing {
        Some(_) => None,
        None => Some(ctx.upgrade_authority_signer()?),
    };
    if let Some(deployer) = upgrade_authority.as_ref() {
        let program = ctx.ring.program_id();
        let data = deploy::program_data(ctx)?
            .with_context(|| format!("program {program} is not deployed, run `deploy` first"))?;
        let authority = data.upgrade_authority.with_context(|| {
            format!("program {program} is immutable and has no config, nothing can create one")
        })?;
        ensure!(
            authority == deployer.address(),
            "program {program} is upgradeable by {authority}, ring.toml names {}",
            deployer.address()
        );
    }
    let auditor_pk = match existing.as_ref() {
        Some(config) => config.auditor_pubkey,
        None => {
            let genesis_hash = ctx.rpc().genesis_hash()?;
            let authority = upgrade_authority
                .as_deref()
                .context(NEEDS_UPGRADE_AUTHORITY)?;
            let trust = ctx.trust(match args.trust_ring_rpc {
                true => Trust::Unpinned,
                false => Trust::Refuse,
            })?;
            ctx.ring_rpc()
                .release_auditor_key(
                    ctx.ring.program_id(),
                    genesis_hash,
                    &ctx.request_signer(authority),
                )?
                .require(trust)?
        }
    };
    let cached_key = key_file.try_exists()?;
    if cached_key {
        ensure!(
            read_auditor_pubkey(&key_file)? == auditor_pk,
            "auditor public key at {} differs from the ring auditor {}",
            key_file.display(),
            hex::encode(auditor_pk.as_bytes())
        );
    }
    let ring = ctx.ring;
    let created = ConfigCreation {
        deployer: upgrade_authority.as_deref(),
        policy_authority: policy_authority.as_ref(),
        auditor_pk,
        policy: policy.as_ref(),
        mode: ctx.config.mode,
        existing: existing.as_ref(),
    }
    .send(ctx)?;
    let ring_config = ctx.ring_config()?;
    let records = ring_config.account().records;
    let policy = match &policy {
        None if created == CREATED => "records only, created",
        None => "records only",
        Some(policy) => PolicyPin {
            authority: policy_authority.as_ref(),
            policy,
            outcome: created,
        }
        .send(ctx)?,
    };
    let spp_ring = match ctx.block(ring.registration(ctx.chain()))? {
        Some(_) if created == CREATED => "registered",
        Some(_) => PRESENT,
        None => bail!(PARTIAL_INIT),
    };
    if !cached_key {
        write_auditor_pubkey(&key_file, &auditor_pk)?;
        line(
            "auditor pk",
            format_args!(
                "{} (written to {})",
                hex::encode(auditor_pk.as_bytes()),
                key_file.display()
            ),
        );
    }
    line("config", created);
    status::print_roles(&ring_config.roles);
    line("records", format_args!("{records} {created}"));
    line("policy", policy);
    line("spp ring", spp_ring);
    status::announce(&ctx.config);
    Ok(())
}

impl ConfigCreation<'_> {
    fn send(self, ctx: &Context) -> Result<&'static str> {
        let policy_key = self.policy_authority.address();
        if let Some(config) = self.existing {
            ensure!(
                config.roles.policy == policy_key,
                "the ring policy authority is {}, the operator manifest names {policy_key}",
                config.roles.policy
            );
            // The tier is immutable, transfer dispatches on it.
            ensure!(
                config.has_policy == self.policy.is_some(),
                "the ring is deployed as {}, ring.toml now selects the other tier, redeploy is not a tier change",
                if config.has_policy { "a policy ring" } else { "audit-only" }
            );
            ensure!(
                ModeName::of(config.mode) == self.mode,
                "ring.toml selects the {} mode, the deployed config has another, redeploy is not a mode change",
                self.mode
            );
            return Ok(PRESENT);
        }
        let deployer = self.deployer.context(NEEDS_UPGRADE_AUTHORITY)?;
        let roles = RingRoles {
            admin: ctx.role_signer(RingRole::Admin)?.address(),
            policy: policy_key,
            pauser: ctx.role_signer(RingRole::Pauser)?.address(),
            cosign_admin: ctx.role_signer(RingRole::CosignAdmin)?.address(),
            enrollment_writer: ctx.role_signer(RingRole::EnrollmentWriter)?.address(),
        };
        // The ring rpc pays every enrollment batch from the writer's balance.
        if ctx.config.target == Target::Localnet {
            ctx.fund(roles.enrollment_writer, fund::MIN_AUTHORITY_BALANCE)?;
            if ctx.operator_manifest()?.localnet_kms.is_some() {
                if let Some(storage) = ctx.local_rpc_roles()?.and_then(|rpc| rpc.storage_authority)
                {
                    ctx.fund(storage.address, fund::MIN_AUTHORITY_BALANCE)?;
                }
            }
        }
        let kind = match (self.mode, self.policy.is_some()) {
            (ModeName::Confidential, false) => RingKind::AuditOnly,
            (ModeName::Confidential, true) => RingKind::Policy,
            (ModeName::Anonymous, false) => RingKind::Anonymous,
            (ModeName::Anonymous, true) => RingKind::AnonymousPolicy,
        };
        RingInitialization {
            deployer,
            policy_authority: self.policy_authority,
            roles,
            auditor_pk: self.auditor_pk,
            kind,
            policy: self.policy,
        }
        .send(ctx)?;
        Ok(CREATED)
    }
}

impl RingInitialization<'_> {
    fn send(self, ctx: &Context) -> Result<()> {
        let target = self.policy_authority.address();
        let records = ctx
            .ring_rpc()
            .create_records_registry(ctx.ring_rpc_target()?, &ctx.request_signer(self.deployer))?;
        let empty = RuleTable::empty();
        let (rules, shared_sources) = match self.policy {
            Some(policy) => {
                for (list, curator) in &policy.shared_sources {
                    catalogue::check_curator(ctx, *curator, *list)?;
                }
                (
                    &policy.rules,
                    policy::curator_sources(ctx, &policy.shared_sources)?,
                )
            }
            None => (&empty, Vec::new()),
        };
        let instruction = |shared_sources: &[(ListId, Address)]| {
            Initialize {
                ring: ctx.ring,
                payer: target,
                authority: self.deployer.address(),
                auditor_pubkey: self.auditor_pk,
                kind: self.kind,
                roles: self.roles,
                policy: Some(InitialPolicy {
                    records,
                    table: PolicyTable {
                        rules,
                        shared_sources,
                    },
                }),
            }
            .instruction()
        };
        // Curators past the transaction size are pointed one by one by the policy pin.
        let initialize = match instruction(&shared_sources) {
            Err(RingError::TransactionTooLarge { .. }) if !shared_sources.is_empty() => {
                instruction(&[])?
            }
            result => result?,
        };
        ctx.send_operator(&[self.policy_authority, self.deployer], &[initialize])?;
        Ok(())
    }
}

impl PolicyPin<'_> {
    /// Sources are pointed only over rows that agree, a drifted table stays untouched.
    fn send(self, ctx: &Context) -> Result<&'static str> {
        let Self {
            authority,
            policy,
            outcome,
        } = self;
        for (list, curator) in &policy.shared_sources {
            catalogue::check_curator(ctx, *curator, *list)?;
        }
        let shared_sources = policy::curator_sources(ctx, &policy.shared_sources)?;
        let pinned = policy::pinned_if_any(ctx)?.context(PARTIAL_INIT)?;
        policy::verify_rows(policy, &pinned).context(POLICY_DRIFT)?;
        crate::records::write_mint_policies(ctx, &policy::mint_changes(ctx, policy)?)?;
        let mut pointed = false;
        for ((list, curator), (_, registry)) in policy.shared_sources.iter().zip(&shared_sources) {
            let outcome =
                if pinned.account.registry_for(*list) == Some(ListRegistry::Curator(*registry)) {
                    PRESENT
                } else {
                    let point = UpdatePolicy {
                        ring: ctx.ring,
                        authority: authority.address(),
                        current: &pinned.account,
                        update: PolicyUpdate::Source {
                            list_id: *list,
                            source: SourceOwner::Curator(*registry),
                        },
                    }
                    .instruction()?;
                    ctx.send_operator(&[authority], &[point])?;
                    pointed = true;
                    CREATED
                };
            line(
                list.name(),
                format_args!("curator {} {outcome}", curator.program_id()),
            );
        }
        let pinned = if pointed {
            policy::pinned(ctx)?
        } else {
            pinned
        };
        policy::verify_sources(ctx, policy, &pinned).context(POLICY_DRIFT)?;
        Ok(outcome)
    }
}
