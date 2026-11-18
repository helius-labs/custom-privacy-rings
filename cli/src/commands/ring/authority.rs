//! `authority`, the program's upgrade authority and the ring roles.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use custom_ring_interface::RingRole;
use custom_ring_sdk::UpdateAuthority;
use solana_loader_v3_interface::instruction as loader;
use zolana_operator_custody::{OperatorSigner, SignerRef, SigningIdentity};

use crate::{
    config::expand_tilde,
    controls::{self, require_initialized},
    deploy::{self, ProgramBinary},
    file,
    release::RingProgram,
    AuthorityCommand, Context,
};

pub fn run(ctx: &Context, command: AuthorityCommand) -> Result<()> {
    let program = ctx.ring.program_id();
    let declined = format!("{program} keeps its upgrade authority, the change was not confirmed");
    match command {
        AuthorityCommand::Transfer { new_signer, yes } => {
            let signer = read_signer(ctx, &new_signer)?;
            let new_authority = signer.address();
            let target = ctx.config.target;
            ctx.confirm(
                yes,
                &format!("hand {program} on {target} to {new_authority}? only that key can hand it back"),
                format!("pass --yes to hand {program} on {target} to {new_authority}, only that key can hand it back"),
                declined,
            )?;
            require_initialized(ctx)?;
            set_upgrade_authority(ctx, Some(signer.as_ref()))?;
            println!(
                "upgrade authority of {program} is now {new_authority}, update upgrade_authority in {}",
                ctx.config.operator_manifest.display()
            );
        }
        AuthorityCommand::UpdateRole { role, new_signer } => {
            let payer = ctx.funded_authority()?;
            let admin = ctx.role_signer(RingRole::Admin)?;
            let new_authority = read_signer(ctx, &new_signer)?;
            let hand_over = UpdateAuthority {
                ring: ctx.ring,
                admin: admin.address(),
                new_authority: new_authority.address(),
                role: role.role(),
            }
            .instruction()?;
            ctx.send_operator(
                &[payer.as_ref(), admin.as_ref(), new_authority.as_ref()],
                &[hand_over],
            )
            .context("sending update_authority failed")?;
            println!(
                "{role} of {program} is now {}, update the role in {}",
                new_authority.address(),
                ctx.config.operator_manifest.display()
            );
        }
        AuthorityCommand::Pause => controls::pause(ctx, true)?,
        AuthorityCommand::Resume => controls::pause(ctx, false)?,
        AuthorityCommand::Renounce { yes, program_so } => {
            ctx.confirm(
                yes,
                &format!("make {program} immutable? nothing can upgrade it again"),
                format!("renouncing is irreversible, pass --yes to make {program} immutable"),
                declined,
            )?;
            require_initialized(ctx)?;
            expected_binary(ctx, program_so)?.verify_deployed(ctx)?;
            set_upgrade_authority(ctx, None)?;
            println!("{program} is immutable");
        }
    }
    Ok(())
}

/// The release lock pins the binary unless the operator names one.
fn expected_binary(ctx: &Context, program_so: Option<PathBuf>) -> Result<ProgramBinary> {
    if let Some(path) = program_so {
        return ProgramBinary::read(&ctx.path(&path));
    }
    let asset = RingProgram::from_lock()?.asset;
    let sha256 = hex::decode(&asset.sha256)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .with_context(|| format!("release lock sha256 {} is not hex", asset.sha256))?;
    Ok(ProgramBinary {
        len: asset.size as usize,
        sha256,
    })
}

/// `None` renounces, the program stays as deployed for good.
fn set_upgrade_authority(ctx: &Context, new_authority: Option<&dyn OperatorSigner>) -> Result<()> {
    let authority = ctx.upgrade_authority_signer()?;
    let program = ctx.ring.program_id();
    let current = deploy::program_data(ctx)?.with_context(|| {
        format!("program {program} is not deployed under the upgradeable loader")
    })?;
    deploy::check_upgrade_authority(program, &current, authority.address())?;
    match new_authority {
        Some(incoming) => {
            let rotate = loader::set_upgrade_authority_checked(
                &program,
                &authority.address(),
                &incoming.address(),
            );
            ctx.send_operator(&[authority.as_ref(), incoming], &[rotate])?;
        }
        None => {
            let renounce = loader::set_upgrade_authority(&program, &authority.address(), None);
            ctx.send_operator(&[authority.as_ref()], &[renounce])?;
        }
    }
    Ok(())
}

fn read_signer(ctx: &Context, path: &Path) -> Result<SigningIdentity> {
    let path = ctx.path(&expand_tilde(path)?);
    let reference: SignerRef = serde_json::from_str(&file::read(&path)?)
        .with_context(|| format!("invalid signer reference {}", path.display()))?;
    ctx.signer(&reference)
}
