//! Registry records, read under a root or signed as record updates.

use custom_ring_interface::domain;
use solana_address::Address;
use solana_signer::Signer;
use spr_client::AsyncRpc;
use spr_indexer_api::Hash;
use spr_interface::ZERO_NULLIFIER_PUBKEY;
use zeroize::{Zeroize, ZeroizeOnDrop};
use zolana_ring_indexer::{
    rpc::{GetStorageRecordPoolRequest, RingListWriteView, MAX_RING_MEMBER_PAGE_LIMIT},
    tracking::{GetTrackingOpeningsRequest, GetTrackingRecordPoolRequest},
};
use zolana_ring_keys::RingAddress;
use zolana_ring_member_registry::{Canopy, TrackingOpening, TrackingPool, TrackingRecord};
use zolana_ring_policy::{ListWrite, Member, MintPolicy, Writer};

use crate::{
    records::{
        MemberOpeningRead, RecordPoolCheck, RecordPoolRead, ServedOpening, ServedPool, StorageTip,
    },
    resolver::tracking_record,
    rpc::{GetListSaltResponse, RecordUpdate, RecordUpdateEntry, SubmitRecordUpdateRequest},
    submission::BoundRoot,
    AsyncRingIndexer, CustomRing, RecipientResolver, ResolvedRecipient, RingError,
};

#[cfg(test)]
#[path = "../tests/records/registry.rs"]
mod tests;

/// Members' records under the newest root of the ring's records registry.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemberRecords<'a> {
    pub ring: CustomRing,
    pub members: &'a [Member],
}

impl MemberRecords<'_> {
    /// One opening per member in member order, each verified against the root.
    pub async fn read(
        self,
        rpc: &(impl AsyncRpc + AsyncRingIndexer + ?Sized),
    ) -> Result<Vec<MemberOpeningRead>, RingError> {
        let members: Vec<[u8; 32]> = self
            .members
            .iter()
            .map(|member| *member.as_bytes())
            .collect();
        RecordReads {
            root: BoundRoot::newest(self.ring, rpc).await?,
            members: &members,
        }
        .read(rpc)
        .await
    }
}

pub(crate) struct RecordReads<'a> {
    pub root: BoundRoot,
    pub members: &'a [[u8; 32]],
}

impl RecordReads<'_> {
    pub async fn read(
        self,
        rpc: &(impl AsyncRpc + AsyncRingIndexer + ?Sized),
    ) -> Result<Vec<MemberOpeningRead>, RingError> {
        let BoundRoot {
            registry,
            root,
            revision,
            ..
        } = self.root;
        let tip = StorageTip {
            registry,
            root,
            revision: revision.value(),
        };
        let mut reads = Vec::with_capacity(self.members.len());
        for chunk in self.members.chunks(MAX_RING_MEMBER_PAGE_LIMIT as usize) {
            let response = rpc
                .get_storage_member_openings(tip.openings_request(chunk))
                .await?;
            reads.extend(tip.verify_openings(&response, chunk)?);
        }
        Ok(reads)
    }
}

/// Distinct members' records under a bound root, the canopy sized for `openings` paths.
pub(crate) struct PoolRead<'a> {
    pub root: BoundRoot,
    pub members: &'a [[u8; 32]],
    pub openings: usize,
}

impl PoolRead<'_> {
    pub async fn read(
        self,
        rpc: &(impl AsyncRpc + AsyncRingIndexer + ?Sized),
    ) -> Result<RecordPoolRead, RingError> {
        let BoundRoot { registry, root, .. } = self.root;
        let response = rpc
            .get_storage_record_pool(GetStorageRecordPoolRequest {
                registry: registry.into(),
                members: self.members.iter().copied().map(Hash).collect(),
                expected_root: Hash(root),
                openings: self.openings as u64,
            })
            .await?;
        Ok(RecordPoolCheck {
            response: &response,
            root: &root,
            members: self.members,
            openings: self.openings,
        }
        .verify()?)
    }
}

/// `members` stay distinct, the canopy sized for `openings` paths.
pub(crate) struct TrackingRead<'a> {
    pub root: BoundRoot,
    pub members: &'a [[u8; 32]],
    pub openings: usize,
}

/// An absent subject reads its bracketing low row.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct TrackingPoolRead {
    pool: TrackingPool,
    /// Aligned with the pool's records, the sentinel serves none.
    viewing_seqs: Vec<Option<u64>>,
}

impl TrackingRead<'_> {
    pub async fn read(
        self,
        rpc: &(impl AsyncRpc + AsyncRingIndexer + ?Sized),
    ) -> Result<TrackingPoolRead, RingError> {
        let BoundRoot { registry, root, .. } = self.root;
        let response = rpc
            .get_tracking_record_pool(GetTrackingRecordPoolRequest {
                registry: registry.into(),
                members: self.members.iter().copied().map(Hash).collect(),
                expected_root: Hash(root),
                openings: self.openings as u64,
            })
            .await?;
        let pool = ServedPool {
            response: &response,
            root: &root,
            members: self.members,
            canopy: Canopy::covering_tracking(self.openings)?,
        }
        .verify(tracking_record)
        .map_err(|_| RingError::InvalidRecordOpening)?;
        let viewing_seqs = response
            .records
            .iter()
            .map(|row| row.record.seal.as_ref().map(|seal| seal.seq))
            .collect();
        Ok(TrackingPoolRead { pool, viewing_seqs })
    }
}

impl TrackingPoolRead {
    pub fn pool(&self) -> &TrackingPool {
        &self.pool
    }

    /// `None` for an absent member or one outside the pool.
    pub fn record(&self, member: &[u8; 32]) -> Option<&TrackingRecord> {
        self.pool.records.get(self.row(member)?)
    }

    /// `None` for an absent member, one outside the pool or a row served without its seal.
    pub fn viewing_seq(&self, member: &[u8; 32]) -> Option<u64> {
        self.viewing_seqs.get(self.row(member)?).copied().flatten()
    }

    fn row(&self, member: &[u8; 32]) -> Option<usize> {
        let subject = self
            .pool
            .subjects
            .iter()
            .find(|subject| subject.member == *member)?;
        let row = subject.record_index as usize;
        self.pool
            .records
            .get(row)
            .is_some_and(|record| record.member == *member)
            .then_some(row)
    }
}

/// A salted entry reads only under the salt the owner's row commits, any other disarms it silently.
#[must_use]
pub struct SaltedOwnerCheck<'a> {
    pub ring: CustomRing,
    pub address: &'a RingAddress,
}

impl SaltedOwnerCheck<'_> {
    pub async fn run(
        self,
        rpc: &(impl AsyncRpc + AsyncRingIndexer + ?Sized),
    ) -> Result<ResolvedRecipient, RingError> {
        let policy = self.ring.require_policy(rpc).await?;
        let tracking = BoundRoot::tracking(&policy, rpc).await?;
        let resolved = RecipientResolver {
            address: self.address,
            registry: tracking.registry,
            root: tracking.root,
        }
        .resolve(rpc)
        .await?;
        // A payment reaches an owner with no row, a salted entry does not.
        match resolved.list_tag {
            Some(_) => Ok(resolved),
            None => Err(RingError::SaltedOwnerUnregistered {
                member: self
                    .address
                    .recipient
                    .signing_pubkey
                    .owner_proof_input_hash()?,
            }),
        }
    }
}

/// The on-chain tags an owner's private list keys open against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnerListTags {
    pub owner: Address,
    /// `None` while the owner holds no tracking row.
    pub list_tag: Option<[u8; 32]>,
    /// Zero while the policy commits no ring salt.
    pub ring_list_tag: [u8; 32],
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnerListTagsRead {
    pub ring: CustomRing,
    pub owner: Address,
}

impl OwnerListTagsRead {
    /// The row is read and verified under the tracking root the policy pins.
    pub async fn run(
        self,
        rpc: &(impl AsyncRpc + AsyncRingIndexer + ?Sized),
    ) -> Result<OwnerListTags, RingError> {
        let member = *Member::owner_tag(self.owner.as_array())?.as_bytes();
        let policy = self.ring.require_policy(rpc).await?;
        let root = BoundRoot::tracking(&policy, rpc).await?;
        let list_tag = match (TrackingMemberRead { root, member }).read(rpc).await? {
            TrackingOpening::Present { record, .. } => Some(record.list_tag),
            TrackingOpening::Absent { .. } => None,
        };
        Ok(OwnerListTags {
            owner: self.owner,
            list_tag,
            ring_list_tag: policy.ring_list_tag,
        })
    }
}

pub(crate) struct TrackingMemberRead {
    pub root: BoundRoot,
    pub member: [u8; 32],
}

impl TrackingMemberRead {
    pub async fn read(
        self,
        rpc: &(impl AsyncRpc + AsyncRingIndexer + ?Sized),
    ) -> Result<TrackingOpening, RingError> {
        let BoundRoot { registry, root, .. } = self.root;
        let response = rpc
            .get_tracking_openings(GetTrackingOpeningsRequest {
                registry: registry.into(),
                members: vec![Hash(self.member)],
                expected_root: Hash(root),
            })
            .await?;
        let [view] = response.openings.as_slice() else {
            return Err(RingError::InvalidRecordOpening);
        };
        ServedOpening {
            view,
            root: &root,
            member: &self.member,
        }
        .verify(tracking_record)
        .map_err(|_| RingError::InvalidRecordOpening)
    }
}

/// Mirror `AssertOutputOwners` in `prover/circuits/transfer/owners.go`.
///
/// A nonzero key pays any owner, registered or not, and only a matching registration spends it.
pub(crate) struct OutputKey<'a> {
    pub row: Option<&'a TrackingRecord>,
    pub nullifier_pk: &'a [u8; 32],
    pub slot_index: usize,
}

impl OutputKey<'_> {
    pub(crate) fn check(self) -> Result<(), RingError> {
        let Self {
            row,
            nullifier_pk,
            slot_index,
        } = self;
        if *nullifier_pk == [0; 32] {
            return Err(RingError::RecipientLiteralZeroKey { slot_index });
        }
        match (row, *nullifier_pk == ZERO_NULLIFIER_PUBKEY) {
            (Some(_), true) => Err(RingError::EnrolledRecipientAtZeroKey { slot_index }),
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateKeyKind {
    OwnerSalted,
    RingSalted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrivateListMember {
    pub member: Member,
    pub kind: PrivateKeyKind,
    pub owner: Address,
}

impl PrivateListMember {
    pub const fn unregistered_owner(self) -> Option<Address> {
        match self.kind {
            PrivateKeyKind::OwnerSalted => None,
            PrivateKeyKind::RingSalted => Some(self.owner),
        }
    }
}

/// Salts as a service or an address supplied them, a key exists only under a salt opening its on-chain tag.
#[derive(Clone, Default, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct OwnerListSalts {
    pub list_salt: Option<[u8; 32]>,
    pub ring_salt: Option<[u8; 32]>,
}

zolana_ring_keys::redacted_debug!(OwnerListSalts {
    present: [list_salt, ring_salt],
});

impl OwnerListSalts {
    pub fn from_response(response: &GetListSaltResponse) -> Self {
        Self {
            list_salt: response.list_salt.as_ref().map(|salt| salt.0),
            ring_salt: response.ring_salt.as_ref().map(|salt| salt.0),
        }
    }

    /// An enrolled owner is listed under its own salt, any other owner under the ring salt.
    pub fn write_member(&self, tags: &OwnerListTags) -> Result<PrivateListMember, RingError> {
        match tags.list_tag {
            Some(list_tag) => self.owner_salted(tags.owner, &list_tag),
            None => self.ring_salted(tags),
        }
    }

    /// Every key `resolveOwner` in `prover/circuits/policy/subjects.go` reads, a clear of fewer leaves the entry in force.
    pub fn revocation_members(
        &self,
        tags: &OwnerListTags,
    ) -> Result<Vec<PrivateListMember>, RingError> {
        let Some(list_tag) = tags.list_tag else {
            return Ok(vec![self.ring_salted(tags)?]);
        };
        let mut members = vec![self.owner_salted(tags.owner, &list_tag)?];
        if tags.ring_list_tag != [0; 32] {
            members.push(self.ring_salted(tags)?);
        }
        Ok(members)
    }

    fn owner_salted(
        &self,
        owner: Address,
        list_tag: &[u8; 32],
    ) -> Result<PrivateListMember, RingError> {
        let owner_tag = Member::owner_tag(owner.as_array())?;
        let member = *owner_tag.as_bytes();
        let list_salt = self
            .list_salt
            .as_ref()
            .ok_or(RingError::ListSaltUnknown { member })?;
        if domain::list_tag(list_salt)? != *list_tag {
            return Err(RingError::ListSaltMismatch { member });
        }
        Ok(PrivateListMember {
            member: Member::salted(&owner_tag, list_salt)?,
            kind: PrivateKeyKind::OwnerSalted,
            owner,
        })
    }

    fn ring_salted(&self, tags: &OwnerListTags) -> Result<PrivateListMember, RingError> {
        let ring_salt = self
            .ring_salt
            .as_ref()
            .filter(|_| tags.ring_list_tag != [0; 32])
            .ok_or(RingError::RingSaltUnknown)?;
        if domain::list_tag(ring_salt)? != tags.ring_list_tag {
            return Err(RingError::RingSaltUnknown);
        }
        let owner_tag = Member::owner_tag(tags.owner.as_array())?;
        Ok(PrivateListMember {
            member: Member::ring_salted(&owner_tag, ring_salt)?,
            kind: PrivateKeyKind::RingSalted,
            owner: tags.owner,
        })
    }
}

/// The writer refuses the update after `valid_until_slot`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordUpdateTarget {
    /// The cluster the writer serves.
    pub genesis_hash: [u8; 32],
    pub ring: CustomRing,
    /// The ring's own `StorageRegistry`.
    pub registry: Address,
    pub valid_until_slot: u64,
}

/// Signed by the wallet, every entry names the wallet.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletRecordUpdate {
    draft: UpdateDraft,
    member: Member,
}

/// Signed by the policy authority, a write may name any record.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityRecordUpdate {
    draft: UpdateDraft,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UpdateDraft {
    target: RecordUpdateTarget,
    signer: Address,
    entries: Vec<RecordUpdateEntry>,
}

impl WalletRecordUpdate {
    pub fn new(target: RecordUpdateTarget, wallet: Address) -> Result<Self, RingError> {
        Ok(Self {
            draft: UpdateDraft::new(target, wallet),
            member: Member::owner_tag(wallet.as_array())?,
        })
    }

    /// Writes the wallet's public record, a grant keys on its identity instead.
    pub fn with_lists(mut self, lists: ListWrite) -> Result<Self, RingError> {
        check_writer(lists, Writer::Member)?;
        let record = Member::public(&self.member)?;
        self.draft
            .entries
            .push(RecordUpdateEntry::Write(list_write(record, lists)));
        Ok(self)
    }

    pub fn sign<S: Signer + ?Sized>(
        self,
        wallet: &S,
    ) -> Result<SubmitRecordUpdateRequest, RingError> {
        self.draft.sign(wallet)
    }
}

impl AuthorityRecordUpdate {
    pub const fn new(target: RecordUpdateTarget, authority: Address) -> Self {
        Self {
            draft: UpdateDraft::new(target, authority),
        }
    }

    /// `member` is any record key, an owner identity or an asset.
    pub fn with_write(mut self, member: Member, write: ListWrite) -> Result<Self, RingError> {
        check_writer(write, Writer::Authority)?;
        self.draft
            .entries
            .push(RecordUpdateEntry::Write(list_write(member, write)));
        Ok(self)
    }

    /// Replaces all mint settings, zero values clear them.
    pub fn with_mint_policy(mut self, member: Member, policy: MintPolicy) -> Self {
        self.draft
            .entries
            .push(RecordUpdateEntry::Write(RingListWriteView {
                mint_policy: Some(policy),
                member: Hash(*member.as_bytes()),
                mask: 0,
                value: 0,
            }));
        self
    }

    /// The writer refuses a signer other than the policy authority.
    pub fn sign<S: Signer + ?Sized>(
        self,
        authority: &S,
    ) -> Result<SubmitRecordUpdateRequest, RingError> {
        self.draft.sign(authority)
    }
}

impl UpdateDraft {
    const fn new(target: RecordUpdateTarget, signer: Address) -> Self {
        Self {
            target,
            signer,
            entries: Vec::new(),
        }
    }

    fn sign<S: Signer + ?Sized>(self, signer: &S) -> Result<SubmitRecordUpdateRequest, RingError> {
        if self.entries.is_empty() {
            return Err(RingError::EmptyListWrite);
        }
        if signer.pubkey() != self.signer {
            return Err(RingError::RecordUpdateSigner {
                expected: self.signer,
            });
        }
        Ok(RecordUpdate {
            genesis_hash: Hash(self.target.genesis_hash),
            ring: self.target.ring.program_id().into(),
            entries: self.entries,
            valid_until_slot: self.target.valid_until_slot,
            awaited_lists: 0,
        }
        .sign(self.target.registry, signer)?)
    }
}

fn list_write(member: Member, write: ListWrite) -> RingListWriteView {
    RingListWriteView {
        mint_policy: None,
        member: Hash(*member.as_bytes()),
        mask: write.mask(),
        value: write.value(),
    }
}

fn check_writer(write: ListWrite, writer: Writer) -> Result<(), RingError> {
    let lists = write.set().union(write.clear());
    match lists.difference(writer.writable()).iter().next() {
        Some(list) => Err(RingError::ListNotWritable { list, writer }),
        None => Ok(()),
    }
}
