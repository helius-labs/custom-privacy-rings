//! `list`, the member records of the ring's registry and where each list reads them.

use anyhow::{bail, ensure, Context as _, Result};
use clap::{builder::PossibleValue, Args, ValueEnum};
use custom_ring_sdk::{
    MemberRecords, OwnerListSalts, OwnerListTags, OwnerListTagsRead, PolicyUpdate, PrivateKeyKind,
    SaltedOwnerCheck, SourceOwner, UpdatePolicy, WalletRecordUpdate,
};
use solana_address::Address;
use solana_signer::Signer;
use zolana_ring_client::owner_record_key;
use zolana_ring_client::ListRegistry;
use zolana_ring_client::PolicySources;
use zolana_ring_keys::{RingAddress, RingKeyError};
use zolana_ring_policy::{ListId, ListSet, ListWrite, ListWriteError, Member};

use crate::{
    catalogue::{check_curator, Catalogue},
    line,
    policy::{self, Pinned},
    records,
    submission::Operators,
    ui::{self, icon},
    Context, EntryArgs, ListCommand,
};

#[derive(Debug, Clone, Args)]
#[group(required = true, multiple = true)]
pub struct MemberArgs {
    /// A member's Solana address, repeatable.
    #[arg(long = "owner", value_name = "ADDRESS")]
    owners: Vec<Address>,
    /// A mint the list screens, repeatable.
    #[arg(long = "asset", value_name = "MINT")]
    assets: Vec<Address>,
    /// A member on a private list, its ring address or its owner key with the salt read from Ring RPC, repeatable.
    #[arg(long = "salted-owner", value_name = "RING_ADDRESS_OR_OWNER", value_parser = parse_salted_owner)]
    salted_owners: Vec<SaltedOwner>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaltedOwner {
    Address(Box<RingAddress>),
    Key(Address),
}

fn parse_salted_owner(text: &str) -> Result<SaltedOwner, RingKeyError> {
    match RingAddress::from_base58(text) {
        Ok(address) => Ok(SaltedOwner::Address(Box::new(address))),
        Err(refused) => text.parse().map(SaltedOwner::Key).map_err(|_| refused),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemberList(ListId);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Subject {
    pub member: Member,
    pub label: String,
    /// The owner of a key under the ring salt, the writer matches it to the entry.
    pub unregistered: Option<Address>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListChange {
    Set,
    Clear,
}

#[must_use]
pub(crate) struct ListUpdate<'a> {
    pub list_id: ListId,
    pub subjects: &'a [Subject],
    pub change: ListChange,
}

pub fn run(ctx: &Context, command: ListCommand) -> Result<()> {
    match command {
        ListCommand::Add(entry) => entry.write(ctx, ListChange::Set),
        ListCommand::Clear(entry) => entry.write(ctx, ListChange::Clear),
        ListCommand::Show(entry) => entry.show(ctx),
        ListCommand::SetSource {
            list_id,
            curator,
            own,
        } => {
            let source = match curator.filter(|_| !own) {
                Some(curator) => {
                    let curator = Catalogue::load(ctx.catalogue.as_deref())?
                        .resolve(ctx.config.target, &curator)?;
                    check_curator(ctx, curator, list_id.id())?;
                    SourceOwner::Curator(policy::curator_records(ctx, curator)?)
                }
                None => SourceOwner::Own,
            };
            set_source(ctx, list_id.id(), source)
        }
        ListCommand::Join { list } => list.write(ctx, ListChange::Set),
        ListCommand::Leave { list } => list.write(ctx, ListChange::Clear),
    }
}

pub(crate) struct SubjectLookup<'a> {
    pub list_id: ListId,
    pub ring: Address,
    pub read_salt: &'a dyn Fn(&Address) -> Result<OwnerListSalts>,
    pub read_tags: &'a dyn Fn(&Address) -> Result<OwnerListTags>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShowSubject {
    pub label: String,
    pub members: Vec<Member>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyRead {
    pub listed: bool,
    pub record_present: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShowRow {
    pub label: String,
    pub listed: bool,
    pub record_present: bool,
}

struct ResolvedSalted {
    salts: OwnerListSalts,
    tags: OwnerListTags,
}

impl SubjectLookup<'_> {
    fn resolve_salted(&self, salted: &SaltedOwner, change: ListChange) -> Result<ResolvedSalted> {
        match salted {
            SaltedOwner::Address(address) => {
                ensure!(
                    address.ring_program_id == self.ring,
                    "a ring address of ring {} cannot name a member of ring {}",
                    address.ring_program_id,
                    self.ring
                );
                ensure!(
                    address.list_salt != [0; 32],
                    "a ring address with no salt has no tracking row and no private record"
                );
                let owner = Address::new_from_array(address.recipient.signing_pubkey.as_ed25519()?);
                let tags = (self.read_tags)(&owner)?;
                let ring_salt = match change {
                    ListChange::Clear if tags.ring_list_tag != [0; 32] => {
                        (self.read_salt)(&owner)?.ring_salt
                    }
                    _ => None,
                };
                Ok(ResolvedSalted {
                    salts: OwnerListSalts {
                        list_salt: Some(address.list_salt),
                        ring_salt,
                    },
                    tags,
                })
            }
            SaltedOwner::Key(owner) => Ok(ResolvedSalted {
                salts: (self.read_salt)(owner)?,
                tags: (self.read_tags)(owner)?,
            }),
        }
    }
}

fn private_label(kind: PrivateKeyKind, owner: Address) -> String {
    match kind {
        PrivateKeyKind::OwnerSalted => format!("salted owner {owner}"),
        PrivateKeyKind::RingSalted => format!("ring-salted owner {owner}"),
    }
}

impl MemberArgs {
    pub(crate) fn subjects(
        &self,
        lookup: SubjectLookup<'_>,
        change: ListChange,
    ) -> Result<Vec<Subject>> {
        Ok(self
            .grouped(&lookup, change)?
            .into_iter()
            .flatten()
            .collect())
    }

    /// A private owner reads under every key a clear writes.
    pub(crate) fn show_subjects(&self, lookup: SubjectLookup<'_>) -> Result<Vec<ShowSubject>> {
        self.grouped(&lookup, ListChange::Clear)?
            .into_iter()
            .map(|keys| {
                let label = keys.first().context("a member with no key")?.label.clone();
                let members = keys.iter().map(|subject| subject.member).collect();
                Ok(ShowSubject { label, members })
            })
            .collect()
    }

    /// One group per named member, a private list reads only an owner's salted keys and a public
    /// one never does.
    fn grouped(&self, lookup: &SubjectLookup<'_>, change: ListChange) -> Result<Vec<Vec<Subject>>> {
        if !ListSet::PRIVATE_LISTS.contains(lookup.list_id) {
            ensure!(
                self.salted_owners.is_empty(),
                "the {} list is public, name its owners with --owner",
                lookup.list_id.name()
            );
            return Ok(self
                .public_subjects()?
                .into_iter()
                .map(|subject| vec![subject])
                .collect());
        }
        ensure!(
            self.owners.is_empty() && self.assets.is_empty(),
            "the {} list keys owners by their salt, name them with --salted-owner",
            lookup.list_id.name()
        );
        self.salted_owners
            .iter()
            .map(|salted| {
                let ResolvedSalted { salts, tags } = lookup.resolve_salted(salted, change)?;
                let targets = match change {
                    ListChange::Set => vec![salts.write_member(&tags)?],
                    ListChange::Clear => salts.revocation_members(&tags)?,
                };
                Ok(targets
                    .into_iter()
                    .map(|target| Subject {
                        member: target.member,
                        label: private_label(target.kind, target.owner),
                        unregistered: target.unregistered_owner(),
                    })
                    .collect())
            })
            .collect()
    }

    fn public_subjects(&self) -> Result<Vec<Subject>> {
        let owners = self.owners.iter().map(|tag| {
            Ok(Subject {
                member: owner_record_key(tag.as_array())?,
                label: format!("owner {tag}"),
                unregistered: None,
            })
        });
        let assets = self.assets.iter().map(|mint| {
            Ok(Subject {
                member: Member::asset(mint)?,
                label: format!("asset {mint}"),
                unregistered: None,
            })
        });
        owners.chain(assets).collect()
    }
}

impl MemberList {
    /// In `ListId::ALL` order.
    pub const ALL: [Self; 1] = [Self(ListId::RingViewing)];

    pub const fn id(self) -> ListId {
        self.0
    }

    fn write(self, ctx: &Context, change: ListChange) -> Result<()> {
        let list_id = self.0;
        let sender = ctx.sender()?;
        let member = owner_record_key(sender.pubkey().as_array())?;
        line(
            "member",
            format_args!("{} {}", list_id.name(), sender.pubkey()),
        );
        let read = records::read_one(
            &ctx.zolana,
            MemberRecords {
                ring: ctx.ring,
                members: &[member],
            },
        )?;
        if read.listed(list_id) == change.present() {
            line("list", "already there");
            return Ok(());
        }
        let update = WalletRecordUpdate::new(ctx.record_update_target()?, sender.pubkey())?
            .with_lists(change.write(list_id)?)?
            .sign(&sender)?;
        warn_link(ctx, sender.pubkey());
        let revision = ctx.ring_rpc()?.apply_record_update(update)?;
        line("list", change.outcome());
        line("revision", revision);
        Ok(())
    }
}

impl ValueEnum for MemberList {
    fn value_variants<'a>() -> &'a [Self] {
        &Self::ALL
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(self.0.name()))
    }
}

impl ListChange {
    fn write(self, list_id: ListId) -> Result<ListWrite, ListWriteError> {
        let list = ListSet::single(list_id);
        let (set, clear) = match self {
            Self::Set => (list, ListSet::EMPTY),
            Self::Clear => (ListSet::EMPTY, list),
        };
        zolana_ring_policy::ListChange { set, clear }.check()
    }

    const fn present(self) -> bool {
        matches!(self, Self::Set)
    }

    const fn outcome(self) -> &'static str {
        match self {
            Self::Set => "added",
            Self::Clear => "removed",
        }
    }
}

struct OwnerSaltReader<'a> {
    ctx: &'a Context,
    writer: crate::ring_rpc::RequestSigner<'a>,
}

impl<'a> OwnerSaltReader<'a> {
    fn new(ctx: &'a Context, authority: &'a dyn zolana_operator_custody::OperatorSigner) -> Self {
        Self {
            ctx,
            writer: ctx.request_signer(authority),
        }
    }

    fn salts(&self, owner: &Address) -> Result<OwnerListSalts> {
        self.ctx
            .ring_rpc()?
            .list_salt(self.ctx.ring_rpc_target()?, *owner, &self.writer)
    }

    fn tags(&self, owner: &Address) -> Result<OwnerListTags> {
        let read = OwnerListTagsRead {
            ring: self.ctx.ring,
            owner: *owner,
        };
        Ok(self
            .ctx
            .zolana
            .block_on(read.run(self.ctx.zolana.as_async()))?)
    }
}

pub(crate) fn aggregate_show_rows(
    subjects: &[ShowSubject],
    reads: &[KeyRead],
) -> Result<Vec<ShowRow>> {
    let mut rest = reads;
    let mut rows = Vec::with_capacity(subjects.len());
    for subject in subjects {
        let (group, tail) = rest
            .split_at_checked(subject.members.len())
            .context("member opening count mismatch")?;
        rest = tail;
        rows.push(ShowRow {
            label: subject.label.clone(),
            listed: group.iter().any(|read| read.listed),
            record_present: group.iter().any(|read| read.record_present),
        });
    }
    ensure!(rest.is_empty(), "member opening count mismatch");
    Ok(rows)
}

impl EntryArgs {
    /// Every salted key opens its on-chain tag, an entry under any other salt is one no proof reads.
    fn checked<T>(
        &self,
        ctx: &Context,
        read: impl FnOnce(&MemberArgs, SubjectLookup<'_>) -> Result<T>,
    ) -> Result<T> {
        let authority = ctx.authority()?;
        let reader = OwnerSaltReader::new(ctx, authority.as_ref());
        let subjects = read(
            &self.members,
            SubjectLookup {
                list_id: self.list_id.id(),
                ring: ctx.ring.program_id(),
                read_salt: &|owner| reader.salts(owner),
                read_tags: &|owner| reader.tags(owner),
            },
        )?;
        self.check_salted_addresses(ctx)?;
        Ok(subjects)
    }

    fn check_salted_addresses(&self, ctx: &Context) -> Result<()> {
        for salted in &self.members.salted_owners {
            let SaltedOwner::Address(address) = salted else {
                continue;
            };
            ctx.zolana
                .block_on(
                    SaltedOwnerCheck {
                        ring: ctx.ring,
                        address,
                    }
                    .run(ctx.zolana.as_async()),
                )
                .with_context(|| {
                    format!(
                        "the salt of {} does not reach a list entry",
                        address.to_base58()
                    )
                })?;
        }
        Ok(())
    }

    fn write(self, ctx: &Context, change: ListChange) -> Result<()> {
        let list_id = self.list_id.id();
        let subjects = self.checked(ctx, |members, lookup| members.subjects(lookup, change))?;
        let outcomes = ListUpdate {
            list_id,
            subjects: &subjects,
            change,
        }
        .send(ctx)?;
        for (subject, outcome) in subjects.iter().zip(outcomes) {
            line(
                "member",
                format_args!("{} {}", list_id.name(), subject.label),
            );
            line("list", outcome);
        }
        Ok(())
    }

    fn show(self, ctx: &Context) -> Result<()> {
        let list_id = self.list_id.id();
        let subjects = self.checked(ctx, MemberArgs::show_subjects)?;
        let members: Vec<Member> = subjects
            .iter()
            .flat_map(|subject| subject.members.iter().copied())
            .collect();
        let pinned = policy::pinned(ctx)?;
        let (source, reads) = match pinned.account.registry_for(list_id) {
            Some(ListRegistry::Curator(registry)) => (
                format!("curator registry {registry}"),
                records::read_curator(ctx, &pinned.account, &members)?,
            ),
            _ => (
                format!("own records {}", pinned.account.records),
                records::read(
                    &ctx.zolana,
                    MemberRecords {
                        ring: ctx.ring,
                        members: &members,
                    },
                )?,
            ),
        };
        let key_reads: Vec<KeyRead> = reads
            .iter()
            .map(|read| KeyRead {
                listed: read.listed(list_id),
                record_present: read.record().is_some(),
            })
            .collect();
        let rows = aggregate_show_rows(&subjects, &key_reads)?;
        line("source", source);
        for row in rows {
            line("member", format_args!("{} {}", list_id.name(), row.label));
            line("listed", row.listed);
            let record = match row.record_present {
                true => "present",
                false => "absent",
            };
            line("record", record);
        }
        Ok(())
    }
}

impl ListUpdate<'_> {
    /// Writes the ring's own registry only, a curator's list is written on the curator ring.
    pub(crate) fn send(self, ctx: &Context) -> Result<Vec<&'static str>> {
        let list_id = self.list_id;
        if let Some(ListRegistry::Curator(registry)) =
            policy::pinned(ctx)?.account.registry_for(list_id)
        {
            bail!(
                "the {} list reads curator registry {registry}, write it on the curator ring",
                list_id.name()
            );
        }
        let members: Vec<Member> = self.subjects.iter().map(|subject| subject.member).collect();
        let reads = records::read(
            &ctx.zolana,
            MemberRecords {
                ring: ctx.ring,
                members: &members,
            },
        )?;
        let mut writes = Vec::new();
        let mut unregistered = Vec::new();
        let mut outcomes = Vec::with_capacity(self.subjects.len());
        for (subject, read) in self.subjects.iter().zip(&reads) {
            if read.listed(list_id) == self.change.present() {
                outcomes.push("already there");
            } else {
                writes.push(subject.member);
                if let Some(owner) = subject.unregistered {
                    if !unregistered.contains(&owner) {
                        unregistered.push(owner);
                    }
                }
                outcomes.push(self.change.outcome());
            }
        }
        if !writes.is_empty() {
            let change = self.change.write(list_id)?;
            ctx.apply_authority_update(ctx.record_update_target()?, &unregistered, |update| {
                Ok(writes
                    .into_iter()
                    .try_fold(update, |update, member| update.with_write(member, change))?)
            })?;
        }
        Ok(outcomes)
    }
}

/// The registry a source names, as the policy stores it.
pub(crate) fn expected_registry(source: SourceOwner) -> ListRegistry {
    match source {
        SourceOwner::Own => ListRegistry::Own,
        SourceOwner::Curator(registry) => ListRegistry::Curator(registry),
    }
}

fn set_source(ctx: &Context, list_id: ListId, source: SourceOwner) -> Result<()> {
    let Pinned { account, .. } = policy::pinned(ctx)?;
    let authority = ctx.funded_authority()?;
    let outcome = if account.registry_for(list_id) == Some(expected_registry(source)) {
        "already present"
    } else {
        let point = UpdatePolicy {
            ring: ctx.ring,
            authority: authority.address(),
            current: &account,
            update: PolicyUpdate::Source { list_id, source },
        }
        .instruction()?;
        ctx.send_operator(Operators::solo(authority.as_ref()), &[point])?;
        "created"
    };
    let registry = policy::pinned(ctx)?
        .account
        .registry_for(list_id)
        .with_context(|| format!("the policy does not reference the {} list", list_id.name()))?;
    let (glyph, reads) = match source {
        SourceOwner::Own => (icon::LISTS, "its own records".to_owned()),
        SourceOwner::Curator(registry) => (icon::CURATOR, format!("curator registry {registry}")),
    };
    ui::heading(glyph, &format!("the {} list reads {reads}", list_id.name()));
    line("source", outcome);
    line(
        "registry",
        match registry {
            ListRegistry::Own => account.records,
            ListRegistry::Curator(registry) => registry,
        },
    );
    Ok(())
}

fn warn_link(ctx: &Context, signer: Address) {
    let ring = ctx.ring.program_id();
    ui::warn(format!(
        "{signer} signs, the chain links that key to ring {ring}"
    ));
}

#[cfg(test)]
mod tests {
    use clap::{error::ErrorKind, Parser};

    use super::*;
    use crate::{Cli, Command, ListCommand};

    const TAG: &str = "9vyTbYGyh3cwxkAQpjjFQGXmdJP6p9B6YcQ5pNuXPNbh";
    const MINT: &str = "So11111111111111111111111111111111111111112";

    fn parse(verb: &str, flags: &[&str]) -> Result<Vec<Subject>, clap::Error> {
        let argv = ["zolana-ring", "list", verb, "block"]
            .into_iter()
            .chain(flags.iter().copied());
        let Command::List(command) = Cli::try_parse_from(argv)?.command else {
            panic!("a list command");
        };
        let members = match command {
            ListCommand::Add(entry) | ListCommand::Clear(entry) | ListCommand::Show(entry) => {
                entry.members
            }
            _ => panic!("a member command"),
        };
        Ok(members
            .subjects(
                SubjectLookup {
                    list_id: ListId::Block,
                    ring: Address::new_from_array([0x11; 32]),
                    read_salt: &|_| bail!("no read"),
                    read_tags: &|_| bail!("no read"),
                },
                ListChange::Set,
            )
            .expect("members"))
    }

    const RING: Address = Address::new_from_array([0x11; 32]);
    const RING_SALT: [u8; 32] = [0x2b; 32];

    fn enrolled(seed: u8) -> RingAddress {
        let wallet = spr_keypair::ShieldedKeypair::from_keypair(
            spr_keypair::SigningKey::from_ed25519_bytes(&[seed; 32]),
        )
        .unwrap();
        RingAddress::enrolled(&wallet, RING).unwrap()
    }

    fn owner_of(address: &RingAddress) -> Address {
        Address::new_from_array(address.recipient.signing_pubkey.as_ed25519().unwrap())
    }

    fn list_tag(salt: &[u8; 32]) -> [u8; 32] {
        custom_ring_interface::domain::list_tag(salt).unwrap()
    }

    /// The chain and the service as one owner's lookup reads them.
    struct World {
        list_tag: Option<[u8; 32]>,
        ring_list_tag: [u8; 32],
        list_salt: Option<[u8; 32]>,
        ring_salt: Option<[u8; 32]>,
    }

    impl World {
        fn subjects(
            &self,
            salted: &SaltedOwner,
            list_id: ListId,
            change: ListChange,
        ) -> Result<Vec<Subject>> {
            self.args(salted).subjects(
                SubjectLookup {
                    list_id,
                    ring: RING,
                    read_salt: &|_| self.salts(),
                    read_tags: &|owner| self.tags(owner),
                },
                change,
            )
        }

        fn show(&self, salted: &SaltedOwner) -> Result<Vec<ShowSubject>> {
            self.args(salted).show_subjects(SubjectLookup {
                list_id: ListId::PrivateBlock,
                ring: RING,
                read_salt: &|_| self.salts(),
                read_tags: &|owner| self.tags(owner),
            })
        }

        fn args(&self, salted: &SaltedOwner) -> MemberArgs {
            MemberArgs {
                owners: Vec::new(),
                assets: Vec::new(),
                salted_owners: vec![salted.clone()],
            }
        }

        fn salts(&self) -> Result<OwnerListSalts> {
            Ok(OwnerListSalts {
                list_salt: self.list_salt,
                ring_salt: self.ring_salt,
            })
        }

        fn tags(&self, owner: &Address) -> Result<OwnerListTags> {
            Ok(OwnerListTags {
                owner: *owner,
                list_tag: self.list_tag,
                ring_list_tag: self.ring_list_tag,
            })
        }
    }

    fn both_forms(address: &RingAddress) -> [SaltedOwner; 2] {
        [
            SaltedOwner::Key(owner_of(address)),
            SaltedOwner::Address(Box::new(address.clone())),
        ]
    }

    #[test]
    fn a_private_list_writes_the_salted_key_and_refuses_a_plain_owner() {
        let address = enrolled(7);
        let owner = owner_of(&address);
        let owner_tag = Member::owner_tag(owner.as_array()).unwrap();
        let row = World {
            list_tag: Some(list_tag(&address.list_salt)),
            ring_list_tag: list_tag(&RING_SALT),
            list_salt: Some(address.list_salt),
            ring_salt: Some(RING_SALT),
        };
        for salted in both_forms(&address) {
            let added = row
                .subjects(&salted, ListId::PrivateBlock, ListChange::Set)
                .unwrap();
            assert_eq!(added.len(), 1);
            assert_eq!(
                added[0].member,
                Member::salted(&owner_tag, &address.list_salt).unwrap()
            );
            assert_eq!(added[0].unregistered, None);
        }
        // An owner with no row keys its entry under the ring salt and names itself to the writer.
        let no_row = World {
            list_tag: None,
            list_salt: None,
            ..row
        };
        let by_key = SaltedOwner::Key(owner);
        let unregistered = no_row
            .subjects(&by_key, ListId::PrivateBlock, ListChange::Set)
            .unwrap();
        assert_eq!(
            unregistered[0].member,
            Member::ring_salted(&owner_tag, &RING_SALT).unwrap()
        );
        assert_eq!(unregistered[0].unregistered, Some(owner));
        // A service naming the ring salt as the owner's salt keys no entry a proof reads.
        let mislabeled = World {
            list_salt: Some(RING_SALT),
            ring_salt: None,
            ..no_row
        };
        assert!(mislabeled
            .subjects(&by_key, ListId::PrivateBlock, ListChange::Set)
            .is_err());

        let by_address = SaltedOwner::Address(Box::new(address));
        assert!(no_row
            .subjects(&by_address, ListId::Block, ListChange::Set)
            .is_err());
        let plain = MemberArgs {
            owners: vec![TAG.parse().unwrap()],
            assets: Vec::new(),
            salted_owners: Vec::new(),
        };
        let lookup = |list_id, ring| SubjectLookup {
            list_id,
            ring,
            read_salt: &|_| bail!("no read"),
            read_tags: &|_| bail!("no read"),
        };
        assert!(plain
            .subjects(lookup(ListId::PrivateAllow, RING), ListChange::Set)
            .is_err());
        let foreign_ring = Address::new_from_array([0x12; 32]);
        assert!(row
            .args(&by_address)
            .subjects(lookup(ListId::PrivateBlock, foreign_ring), ListChange::Set)
            .is_err());
    }

    #[test]
    fn a_private_clear_writes_every_key_the_circuit_reads_or_refuses() {
        let address = enrolled(9);
        let owner = owner_of(&address);
        let owner_tag = Member::owner_tag(owner.as_array()).unwrap();
        let expected_owner = Member::salted(&owner_tag, &address.list_salt).unwrap();
        let expected_ring = Member::ring_salted(&owner_tag, &RING_SALT).unwrap();
        let committed = || World {
            list_tag: Some(list_tag(&address.list_salt)),
            ring_list_tag: list_tag(&RING_SALT),
            list_salt: Some(address.list_salt),
            ring_salt: Some(RING_SALT),
        };
        for salted in both_forms(&address) {
            for list_id in [
                ListId::PrivateAllow,
                ListId::PrivateBlock,
                ListId::PrivateFrozen,
            ] {
                let cleared = committed()
                    .subjects(&salted, list_id, ListChange::Clear)
                    .unwrap();
                assert_eq!(cleared.len(), 2);
                assert_eq!(cleared[0].member, expected_owner);
                assert_eq!(cleared[0].unregistered, None);
                assert_eq!(cleared[1].member, expected_ring);
                assert_eq!(cleared[1].unregistered, Some(owner));
            }
            // A service at an older revision, a faulty one or a hostile one yields no one-key clear.
            for ring_salt in [None, Some([0x1d; 32]), Some(address.list_salt)] {
                let withheld = World {
                    ring_salt,
                    ..committed()
                };
                assert!(withheld
                    .subjects(&salted, ListId::PrivateBlock, ListChange::Clear)
                    .is_err());
                assert!(withheld.show(&salted).is_err());
            }
            let zero_tag = World {
                ring_list_tag: [0; 32],
                ..committed()
            };
            let cleared = zero_tag
                .subjects(&salted, ListId::PrivateBlock, ListChange::Clear)
                .unwrap();
            assert_eq!(cleared.len(), 1);
            assert_eq!(cleared[0].member, expected_owner);
            assert_eq!(cleared[0].unregistered, None);
        }
        let no_owner_salt = World {
            list_salt: None,
            ..committed()
        };
        assert!(no_owner_salt
            .subjects(
                &SaltedOwner::Key(owner),
                ListId::PrivateBlock,
                ListChange::Clear
            )
            .is_err());
    }

    #[test]
    fn show_detects_an_enrolled_owner_listed_only_under_ring_salted() {
        let address = enrolled(10);
        let owner = owner_of(&address);
        let owner_tag = Member::owner_tag(owner.as_array()).unwrap();
        let expected_owner = Member::salted(&owner_tag, &address.list_salt).unwrap();
        let expected_ring = Member::ring_salted(&owner_tag, &RING_SALT).unwrap();
        let world = World {
            list_tag: Some(list_tag(&address.list_salt)),
            ring_list_tag: list_tag(&RING_SALT),
            list_salt: Some(address.list_salt),
            ring_salt: Some(RING_SALT),
        };

        for salted in both_forms(&address) {
            let subjects = world.show(&salted).unwrap();
            assert_eq!(
                subjects,
                vec![ShowSubject {
                    label: format!("salted owner {owner}"),
                    members: vec![expected_owner, expected_ring],
                }]
            );

            let ring_only_listed = aggregate_show_rows(
                &subjects,
                &[
                    KeyRead {
                        listed: false,
                        record_present: false,
                    },
                    KeyRead {
                        listed: true,
                        record_present: true,
                    },
                ],
            )
            .unwrap();
            assert_eq!(
                ring_only_listed,
                vec![ShowRow {
                    label: format!("salted owner {owner}"),
                    listed: true,
                    record_present: true,
                }]
            );

            let neither_listed = aggregate_show_rows(
                &subjects,
                &[
                    KeyRead {
                        listed: false,
                        record_present: false,
                    },
                    KeyRead {
                        listed: false,
                        record_present: false,
                    },
                ],
            )
            .unwrap();
            assert_eq!(
                neither_listed,
                vec![ShowRow {
                    label: format!("salted owner {owner}"),
                    listed: false,
                    record_present: false,
                }]
            );
        }
    }

    fn subject(member: Member, label: String) -> Subject {
        Subject {
            member,
            label,
            unregistered: None,
        }
    }

    #[test]
    fn every_member_command_takes_repeated_owners_and_assets() {
        let tag = owner_record_key(Address::from_str_const(TAG).as_array()).expect("member");
        let mint = Member::asset(&Address::from_str_const(MINT)).expect("mint");
        for verb in ["add", "clear", "show"] {
            assert_eq!(
                parse(verb, &["--owner", TAG]).expect(verb),
                [subject(tag, format!("owner {TAG}"))]
            );
            assert_eq!(
                parse(verb, &["--asset", MINT, "--owner", TAG, "--owner", TAG]).expect(verb),
                [
                    subject(tag, format!("owner {TAG}")),
                    subject(tag, format!("owner {TAG}")),
                    subject(mint, format!("asset {MINT}")),
                ]
            );
        }
        assert_eq!(
            parse("add", &[]).expect_err("no member").kind(),
            ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn a_member_writes_only_its_own_lists() {
        let join = |list: &str| Cli::try_parse_from(["zolana-ring", "list", "join", list]);
        for list in MemberList::ALL {
            assert!(join(list.0.name()).is_ok(), "{}", list.0.name());
        }
        for list in ["allow", "block"] {
            assert!(join(list).is_err(), "{list}");
        }
    }

    #[test]
    fn the_member_lists_are_the_lists_a_member_writes() {
        let lists: Vec<ListId> = MemberList::ALL.into_iter().map(MemberList::id).collect();
        let written: Vec<ListId> = ListId::ALL
            .into_iter()
            .filter(|list_id| ListSet::MEMBER_WRITES.contains(*list_id))
            .collect();
        assert_eq!(lists, written);
    }

    #[test]
    fn a_source_names_the_registry_account() {
        let curator = Address::new_from_array([4; 32]);
        assert_eq!(expected_registry(SourceOwner::Own), ListRegistry::Own);
        assert_eq!(
            expected_registry(SourceOwner::Curator(curator)),
            ListRegistry::Curator(curator)
        );
    }
}
