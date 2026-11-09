//! `audit`, the ring's transactions opened with the auditor key, read from the indexer.

use std::{
    collections::{hash_map::Entry, BTreeSet, HashMap},
    fmt,
};

use anyhow::{Context as _, Result};
use custom_ring_interface::RingMode;
use solana_address::Address;
use spr_interface::ZERO_NULLIFIER_PUBKEY;
use spr_keypair::{NullifierKey, X25519Keypair};
use zolana_ring_client::{
    AuditedTransaction, AuditedUtxo, OutputOpening, RegisteredKeys, RingScan,
};

use crate::{
    controls::ring_auditor_key,
    line,
    ui::{self, icon},
    AuditArgs, Context,
};

pub(crate) struct OpenedSlot<'a> {
    pub slot: u32,
    pub kind: &'a str,
    pub owner: Address,
    pub asset: String,
    pub amount: u64,
}

pub fn run(ctx: &Context, args: AuditArgs) -> Result<()> {
    let config = ctx.ring_config()?;
    let auditor = ring_auditor_key(ctx, &args.auditor_key)?;
    let tracking = ctx.block(ctx.ring.require_policy(ctx.chain()))?.tracking;
    let mut scan = RingScan::new(ctx.ring.program_id(), auditor.pubkey())
        .with_mode(config.mode)
        .with_tracking_registry(tracking);
    if let Some(cursor) = &args.cursor {
        scan = scan.with_cursor(
            cursor
                .parse()
                .context("the cursor is not a history cursor")?,
        );
    }
    let chain = ctx.chain();
    let page = ctx.block(scan.audit(&auditor, chain.indexer(), chain.rpc()))?;
    let senders = match config.mode {
        RingMode::Anonymous => HiddenSenders::read(ctx, &auditor, config.mode, &tracking)?,
        RingMode::Confidential => HiddenSenders::default(),
    };
    for transaction in &page.transactions {
        print(transaction, &senders);
    }
    line("transactions", page.transactions.len());
    if let Some(cursor) = page.next_cursor {
        line("next cursor", cursor);
    }
    Ok(())
}

/// The owner of the note each nullifier spends, over the ring's whole history.
///
/// An anonymous spend publishes no owner. The auditor opens each owner's nullifier key from the
/// seal its enrollment holds, and a published nullifier the key derives from a created note names
/// that note's owner. The match binds the key to the note, so an unverified seal names no sender.
#[derive(Default)]
struct HiddenSenders(HashMap<[u8; 32], Address>);

impl HiddenSenders {
    fn read(
        ctx: &Context,
        auditor: &X25519Keypair,
        mode: RingMode,
        tracking: &Address,
    ) -> Result<Self> {
        // A zero key note spends under the public zero secret, every other under the enrolled key.
        let zero = NullifierKey::from_secret([0; 31]);
        let mut keys = HashMap::new();
        let mut spent = HashMap::new();
        for (hash, utxo) in created_notes(ctx, auditor, mode, tracking)? {
            let owner = Address::new_from_array(utxo.owner_key);
            let key = match keys.entry(owner) {
                Entry::Occupied(held) => held.into_mut(),
                Entry::Vacant(open) => open.insert(enrolled_key(ctx, auditor, tracking, &owner)?),
            };
            // A note no key rebuilds still traces under the enrolled secret, a note under another
            // key derives a nullifier no spend publishes.
            let key = match utxo.owner.nullifier_pk() {
                Some(ZERO_NULLIFIER_PUBKEY) => Some(&zero),
                Some(nullifier_pk) => key
                    .as_ref()
                    .filter(|key| key.pubkey().ok() == Some(nullifier_pk)),
                None => key.as_ref(),
            };
            if let Some(key) = key {
                spent.insert(key.nullifier(&hash, &utxo.blinding)?, owner);
            }
        }
        Ok(Self(spent))
    }

    fn of(&self, transaction: &AuditedTransaction) -> BTreeSet<Address> {
        transaction
            .nullifiers
            .iter()
            .filter_map(|nullifier| self.0.get(nullifier).copied())
            .collect()
    }
}

/// A spend can name a note any earlier page created, so the read starts at the first page.
fn created_notes(
    ctx: &Context,
    auditor: &X25519Keypair,
    mode: RingMode,
    tracking: &Address,
) -> Result<Vec<([u8; 32], AuditedUtxo)>> {
    let chain = ctx.chain();
    let mut notes = Vec::new();
    let mut cursor = None;
    loop {
        let mut scan = RingScan::new(ctx.ring.program_id(), auditor.pubkey())
            .with_mode(mode)
            .with_tracking_registry(*tracking);
        if let Some(cursor) = cursor.take() {
            scan = scan.with_cursor(cursor);
        }
        let page = ctx.block(scan.audit(auditor, chain.indexer(), chain.rpc()))?;
        notes.extend(page.transactions.iter().flat_map(|transaction| {
            transaction
                .outputs
                .iter()
                .filter_map(|output| Some((output.context.hash, *output.opening.utxo()?)))
        }));
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => return Ok(notes),
        }
    }
}

/// `None` for an owner holding no leaf under the registry's newest root.
fn enrolled_key(
    ctx: &Context,
    auditor: &X25519Keypair,
    tracking: &Address,
    owner: &Address,
) -> Result<Option<NullifierKey>> {
    let chain = ctx.zolana.as_async();
    let owner = owner.to_bytes();
    let mut leaves = ctx.block(async {
        let leaves = RegisteredKeys {
            registry: *tracking,
            owners: &[owner],
        }
        .leaves(chain, chain)
        .await?;
        anyhow::Ok(leaves)
    })?;
    Ok(leaves
        .remove(&owner)
        .map(|leaf| leaf.nullifier_key(auditor))
        .transpose()?)
}

impl fmt::Display for OpenedSlot<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let OpenedSlot {
            slot,
            kind,
            owner,
            asset,
            amount,
        } = self;
        write!(
            f,
            "  slot {slot}  {kind} to {owner}  asset {asset}  amount {amount}"
        )
    }
}

fn print(transaction: &AuditedTransaction, senders: &HiddenSenders) {
    let heading = format!("{} at slot {}", transaction.tx_signature, transaction.slot);
    ui::heading(icon::AUDITOR, &heading);
    for sender in senders.of(transaction) {
        println!("  hidden sender {sender}");
    }
    println!("  nullifiers {}", transaction.nullifiers.len());
    for output in &transaction.outputs {
        let (kind, utxo) = match &output.opening {
            OutputOpening::Ring { utxo, .. } => ("ring", utxo),
            OutputOpening::Exit(utxo) => ("exit", utxo),
            OutputOpening::Dummy => {
                println!("  slot {}  dummy", output.slot_index);
                continue;
            }
        };
        let opened = OpenedSlot {
            slot: output.slot_index,
            kind,
            owner: Address::new_from_array(utxo.owner_key),
            asset: utxo
                .mint
                .map_or_else(|| hex::encode(utxo.asset), |mint| mint.to_string()),
            amount: utxo.amount,
        };
        println!("{opened}");
    }
    for error in &transaction.payload_errors {
        println!("  slot {} payload {:?}", error.slot_index, error.kind);
    }
}
