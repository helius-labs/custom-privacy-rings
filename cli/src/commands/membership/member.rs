use anyhow::Result;
use custom_ring_interface::storage_instruction::EnrollIxData;
use custom_ring_sdk::{CustomRing, Enroll, EnrollmentRead, RingError};
use solana_signer::Signer;
use spr_client::{BlockingIndexer, BlockingRpc};
use spr_keypair::ShieldedKeypair;
use spr_transaction::ShieldedKeys;
use zolana_ring_client::{RegisteredKeys, RingIndexer};
use zolana_ring_keys::ListSaltHolder;

use crate::{line, wait_for, Context, KeyCommand, Probe, Zolana};

const SEAL_PROMPT: &str = "seal the member key to the ring auditor and enroll it?";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plan {
    AlreadyRegistered,
    Enroll,
    Declined,
}

#[must_use]
pub(crate) struct KeyEnrollment<'a> {
    pub members: &'a [&'a ShieldedKeypair],
}

pub fn key(ctx: &Context, command: KeyCommand) -> Result<()> {
    match command {
        KeyCommand::Register => register_key(ctx),
        KeyCommand::Rotate => crate::viewing_keys::rotate(ctx),
    }
}

fn register_key(ctx: &Context) -> Result<()> {
    let sender = ctx.sender()?;
    let member = ShieldedKeypair::from_keypair(&sender)?;
    line("member", sender.pubkey());
    let registered = registered(ctx.ring, &ctx.zolana, &member)?;
    match plan(registered, || ctx.ask_yes(SEAL_PROMPT, true))? {
        Plan::AlreadyRegistered => line("key", "already enrolled"),
        Plan::Declined => line("key", "not enrolled"),
        Plan::Enroll => {
            register(ctx, &member)?;
            line("key", "enrolled");
        }
    }
    Ok(())
}

impl KeyEnrollment<'_> {
    pub(crate) fn ensure(self, ctx: &Context) -> Result<()> {
        for member in self.members {
            let outcome = if registered(ctx.ring, &ctx.zolana, member)? {
                "already enrolled"
            } else {
                register(ctx, member)?;
                "enrolled"
            };
            line("member key", format_args!("{} {outcome}", member.pubkey()));
        }
        // The writer lands held enrollments in its next batch.
        wait_for("every member key applied", || {
            for member in self.members {
                match applied(ctx.ring, &ctx.zolana, member) {
                    Ok(true) => {}
                    Ok(false) => return Ok(Probe::NotYet),
                    Err(error) if error.is_projection_lag() => {
                        return Ok(Probe::Retry(error.into()))
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(Probe::Ready(()))
        })
    }
}

fn register(ctx: &Context, member: &ShieldedKeypair) -> Result<()> {
    let enrollment = ctx.block(
        Enroll {
            ring: ctx.ring,
            owner: member.pubkey(),
            nullifier_key: &member.nullifier_key,
            viewing_pubkey: ctx.ring_keys(member)?.address()?.viewing_pubkey,
            list_salt: member.list_salt(&ctx.ring.program_id())?,
        }
        .prove(ctx.ring_client()),
    )?;
    ctx.ring_rpc().submit_enrollment(enrollment.sign(member)?)?;
    Ok(())
}

fn plan(registered: bool, confirm: impl FnOnce() -> Result<bool>) -> Result<Plan> {
    Ok(match registered {
        true => Plan::AlreadyRegistered,
        false if confirm()? => Plan::Enroll,
        false => Plan::Declined,
    })
}

/// The first key is final, the member's leaf holds it.
fn registered<R: BlockingRpc, I: BlockingIndexer + RingIndexer>(
    ring: CustomRing,
    zolana: &Zolana<R, I>,
    keys: &ShieldedKeypair,
) -> Result<bool> {
    Ok(enrollment(ring, zolana, keys)?.is_some())
}

/// An apply has landed the member's leaf under the newest tracking root.
pub(crate) fn applied<R: BlockingRpc, I: BlockingIndexer + RingIndexer>(
    ring: CustomRing,
    zolana: &Zolana<R, I>,
    keys: &ShieldedKeypair,
) -> Result<bool, RingError> {
    let chain = zolana.as_async();
    let owner = keys.pubkey().to_bytes();
    zolana.block_on(async {
        let policy = ring.require_policy(chain.rpc()).await?;
        let leaves = RegisteredKeys {
            registry: policy.tracking,
            owners: &[owner],
        }
        .leaves(chain, chain)
        .await?;
        Ok(leaves.contains_key(&owner))
    })
}

/// The member's leaf fields, `None` until the writer lands its enrollment.
pub(crate) fn enrollment<R: BlockingRpc, I: BlockingIndexer + RingIndexer>(
    ring: CustomRing,
    zolana: &Zolana<R, I>,
    keys: &ShieldedKeypair,
) -> Result<Option<EnrollIxData>> {
    let chain = zolana.as_async();
    let policy = zolana.block_on(ring.require_policy(chain.rpc()))?;
    let enrolled = zolana.block_on(
        EnrollmentRead {
            tracking_registry: policy.tracking,
            owner: keys.pubkey(),
        }
        .read(chain),
    )?;
    Ok(enrolled.map(|enrolled| enrolled.ix))
}
