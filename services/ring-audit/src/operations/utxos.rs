//! The notes a ring's auditor opened, their nullifiers, and the keys its enrollments hold.
//!
//! A wallet read takes its rows and its history slot from one snapshot, never older rows with newer
//! coverage.
//!
//! The mirror is a bounded cache only commits and reloads fill.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::{PoisonError, RwLock},
    time::Duration,
};

use solana_address::Address;
use solana_signature::Signature;
use spr_indexer_api::{Context, Hash};
use spr_interface::ZERO_NULLIFIER_PUBKEY;
use spr_keypair::{NullifierKey, PublicKey};
use spr_transaction::{Data, Mint, Utxo, WalletUtxo};
use tokio::sync::Mutex as AsyncMutex;
use zolana_ring_client::{
    history::HistoryPosition,
    rpc::{
        AuditedUtxoView, GetSpendCoverageResponse, GetWalletStatusResponse, WalletBalance,
        WalletEnrollment, WalletNote, WalletNoteStatus,
    },
    AuditedMergeOutput, ConfirmedMerge, KnownUtxo, UtxoOwner,
};
use zolana_ring_policy::Member;
use zolana_ring_transaction::RingAssets;

use ring_service_core::{bounded_map::BoundedMap, database::Store};

use crate::{
    error::{RingAuditError, ServiceError},
    frame_repairs::RepairBook,
    merge_selection::{MergeTarget, MergeTotals},
    owner_notes::{OwnerNotesPage, OWNER_ROW_BATCH},
    projection_lease::ProjectionLease,
    projection_reads::{EnrollmentKey, ProjectionReads},
    spend_coverage::CoverageCursor,
    spend_machine::{NoteRead, SpendRow, SpendState},
    spent_index::{read_snapshot, Checkpoint, CommittedPage, LoadedSpends, NoteSeal},
    tracking::{EnrollmentAdmission, HeldEnrollment, HeldEnrollments, TrackingCursor},
    wallet_snapshots::{
        SnapshotPage, WalletSnapshotLimits, WalletSnapshotRelease, WalletSnapshots,
    },
};

#[path = "page_work.rs"]
mod page_work;

pub use page_work::PageWork;
pub(crate) use page_work::MAX_LATE_OWNERS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpendRead {
    pub spent: bool,
    /// Every finalized frame of the ring up to the slot is read.
    pub history_through: u64,
}

pub const UTXO_BACKFILL_INTERVAL: Duration = Duration::from_secs(30);

/// Pages one backfill run reads, the next run resumes from its position.
pub(crate) const MAX_BACKFILL_PAGES: usize = 64;

/// Per map of the mirror.
pub(crate) const MAX_CACHED_ENTRIES: usize = 65_536;

/// The guards are never held across an await, a commit writes the mirror after its transaction.
pub struct AuditStore {
    /// A note whose record publishes no key rebuilds under this ring's program id.
    pub(crate) ring: Address,
    book: RwLock<SpendBook>,
    enrollments: RwLock<HeldEnrollments>,
    /// The registry last named on the notes, a different one names them again.
    assets: RwLock<Option<RingAssets>>,
    /// Owners whose key arrived, each with the key its derivation resumes after, kept until a
    /// committed page derived their last note.
    late_owners: RwLock<BTreeMap<[u8; 32], Option<[u8; 32]>>>,
    /// One backfill and key refresh at a time.
    pub(crate) backfill: AsyncMutex<Backfill>,
    backing: Backing,
    snapshot_limits: WalletSnapshotLimits,
}

pub(crate) enum Backing {
    Database {
        store: Store,
        ring: Address,
        seal: Box<NoteSeal>,
        capacity: usize,
    },
    /// The ring's seal failed to derive, every read beyond the mirror refuses.
    Unavailable,
}

#[derive(Default)]
pub(crate) struct Backfill {
    /// `None` until the durable tables load.
    pub history: Option<HistoryPosition>,
    /// The tracking leaf the next scan reads first, durable on the lease holder.
    pub tracking_cursor: TrackingCursor,
    pub repairs: RepairBook,
    /// The lease the last claim returned, `None` while another replica writes the ring.
    pub writer: Option<ProjectionLease>,
}

struct SpendBook {
    notes: BoundedMap<[u8; 32], StoredUtxo>,
    /// The note index, a late secret fills it for that owner's notes.
    by_nullifier: BoundedMap<[u8; 32], [u8; 32]>,
    rows: BoundedMap<[u8; 32], SpendRow>,
    first_unread: Option<CreatedIn>,
    context: Context,
    position_slot: Option<u64>,
    /// The projection revision every cached entry is current at, `None` before the first load.
    revision: Option<i64>,
}

/// What one page changes, the mirror takes it after its database transaction commits.
#[derive(Default)]
pub struct PageDelta {
    notes: BTreeMap<[u8; 32], StoredUtxo>,
    by_nullifier: HashMap<[u8; 32], [u8; 32]>,
    rows: BTreeMap<[u8; 32], SpendRow>,
    unread: BTreeSet<CreatedIn>,
    read: BTreeSet<CreatedIn>,
    faults: Vec<SpendRow>,
    checkpoint: Option<Checkpoint>,
    late_owners: BTreeMap<[u8; 32], LateProgress>,
    owners: Vec<OwnerChange>,
    /// The history slot before the page, a spend at or before it leaves the index.
    prune_through: u64,
    /// Set once the page commits, the cache is current through it.
    revision: Option<i64>,
    first_unread: Option<Option<CreatedIn>>,
}

/// One note's whole index row as a page leaves it, keyed by its owner's member tag.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OwnerChange {
    pub member: [u8; 32],
    pub note: IndexedNote,
}

/// `key` orders a wallet page and names its cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IndexedNote {
    pub key: [u8; 32],
    pub commitment: [u8; 32],
    pub note: StoredUtxo,
    pub spent_at: Option<u64>,
}

/// One owner's rows after `after` in key order, `limit` rows at most.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct OwnerRange {
    pub member: [u8; 32],
    pub after: Option<[u8; 32]>,
    pub limit: usize,
}

/// `more` when a row past the range exists.
#[derive(Clone, Debug, Default)]
pub(crate) struct OwnerPage {
    pub notes: Vec<IndexedNote>,
    pub more: bool,
}

/// An owner whose key arrived, `resume` names the key a committed page derived through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LateOwner {
    pub owner: [u8; 32],
    pub resume: Option<[u8; 32]>,
}

/// How far one page derived a late owner's notes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LateProgress {
    Done,
    /// The next page resumes after the key.
    After([u8; 32]),
}

/// The inputs of a page that resolved to the reading wallet's notes.
#[derive(Debug, Default)]
pub struct InputParties {
    own: HashSet<[u8; 32]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CreatedIn {
    pub slot: u64,
    pub tx_signature: Signature,
    pub event_index: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct WalletStatusQuery {
    pub principal: [u8; 34],
    pub now: u64,
    pub enrollment: WalletEnrollment,
    pub wallet: Address,
    pub ring: Address,
    pub after: Option<WalletCursor>,
    pub limit: usize,
}

#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct WalletCursor {
    ring: [u8; 32],
    wallet: [u8; 32],
    pub(crate) snapshot: [u8; 32],
    pub(crate) after: [u8; 32],
}

impl WalletCursor {
    pub fn decode(bytes: &[u8]) -> Result<Self, RingAuditError> {
        bytemuck::try_from_bytes::<Self>(bytes)
            .copied()
            .map_err(|_| RingAuditError::InvalidCursor)
    }
}

pub(crate) struct WalletCursorScope {
    pub ring: Address,
    pub wallet: Address,
}

impl WalletCursor {
    pub(crate) fn validate_scope(&self, scope: WalletCursorScope) -> Result<(), RingAuditError> {
        if self.ring != scope.ring.to_bytes() || self.wallet != scope.wallet.to_bytes() {
            return Err(RingAuditError::InvalidCursor);
        }
        Ok(())
    }
}

impl WalletStatusQuery {
    fn validate_scope(&self) -> Result<(), RingAuditError> {
        if self.limit == 0 || self.limit > OWNER_ROW_BATCH {
            return Err(RingAuditError::InvalidPage);
        }
        if let Some(cursor) = self.after {
            cursor.validate_scope(WalletCursorScope {
                ring: self.ring,
                wallet: self.wallet,
            })?;
        }
        Ok(())
    }
}

/// One owner's notes under one key, for every mint in `mints`.
#[must_use]
pub struct SpendableNotes<'a> {
    pub ring: Address,
    pub owner_key: [u8; 32],
    pub nullifier_key: &'a NullifierKey,
    pub mints: &'a [Mint],
    /// Nullifiers a pending merge holds.
    pub held: &'a HashSet<[u8; 32]>,
}

struct SpendableScan<'a> {
    query: SpendableNotes<'a>,
    nullifier_pubkey: [u8; 32],
    mints: HashMap<Address, Mint>,
    range: Option<OwnerRange>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredUtxo {
    pub utxo: KnownUtxo,
    pub(crate) created_in: CreatedIn,
    pub(crate) slot_index: u32,
    pub(crate) leaf_index: u64,
    /// False on an exit, its note leaves the ring.
    pub(crate) ring_note: bool,
    /// `None` until the owner's secret names it.
    pub(crate) nullifier: Option<[u8; 32]>,
}

impl SpendBook {
    fn bounded(capacity: usize) -> Self {
        Self {
            notes: BoundedMap::bounded(capacity),
            by_nullifier: BoundedMap::bounded(capacity),
            rows: BoundedMap::bounded(capacity),
            first_unread: None,
            context: Context::default(),
            position_slot: None,
            revision: None,
        }
    }

    fn absorb(&mut self, delta: PageDelta) {
        self.notes.extend(delta.notes);
        self.by_nullifier.extend(delta.by_nullifier);
        self.rows.extend(delta.rows);
        if let Some(first_unread) = delta.first_unread {
            self.first_unread = first_unread;
        }
        if let Some(checkpoint) = delta.checkpoint {
            self.position_slot = checkpoint.position.checkpoint();
            if checkpoint.through.slot >= self.context.slot {
                self.context = checkpoint.through;
            }
        }
        if delta.revision.is_some() {
            self.revision = delta.revision;
        }
    }

    fn history_through(&self) -> u64 {
        history_through(self.first_unread.map(|frame| frame.slot), self.context.slot)
    }
}

/// Only an unreadable frame holds the claim back, an unresolved row never does.
pub(crate) fn history_through(first_unread_slot: Option<u64>, context_slot: u64) -> u64 {
    first_unread_slot.map_or(context_slot, |slot| {
        context_slot.min(slot.saturating_sub(1))
    })
}

impl IndexedNote {
    /// Reads one note's own spend and no other row.
    fn status(&self, through: u64) -> WalletNoteStatus {
        let utxo = &self.note.utxo;
        NoteRead {
            nullifier_known: self.note.nullifier.is_some(),
            opened: utxo.owner.nullifier_pk().is_some(),
            spent_at: self.spent_at,
            zero_key_wallet: utxo.owner.nullifier_pk() == Some(ZERO_NULLIFIER_PUBKEY)
                && Address::new_from_array(utxo.owner_key).is_on_curve(),
            history_through: through,
        }
        .status()
    }

    /// `None` for a spent note or one past the history slot, a spent input and its change never both
    /// count.
    fn listed_status(&self, through: u64) -> Option<WalletNoteStatus> {
        if self.note.created_in.slot > through {
            return None;
        }
        Some(self.status(through)).filter(|status| *status != WalletNoteStatus::Spent)
    }
}

/// `Unspent` notes summed by asset.
#[derive(Default)]
pub(crate) struct Balances(BTreeMap<[u8; 32], WalletBalance>);

impl Balances {
    pub(crate) fn add(&mut self, indexed: &IndexedNote, through: u64) {
        if indexed.listed_status(through) != Some(WalletNoteStatus::Unspent) {
            return;
        }
        let utxo = &indexed.note.utxo;
        let balance = self.0.entry(utxo.asset).or_insert_with(|| WalletBalance {
            asset: Hash(utxo.asset),
            mint: utxo.mint.map(Into::into),
            amount: 0,
        });
        balance.amount = balance.amount.saturating_add(utxo.amount);
    }
}

impl Balances {
    pub(crate) fn into_values(self) -> Vec<WalletBalance> {
        self.0.into_values().collect()
    }
}

impl SnapshotPage {
    fn response(self, query: WalletStatusQuery) -> GetWalletStatusResponse {
        let Self { page, metadata, id } = self;
        let last = page.notes.last().map(|indexed| indexed.key);
        GetWalletStatusResponse {
            history_through: metadata.through,
            enrollment: metadata.enrollment,
            balances: if query.after.is_none() {
                metadata.balances
            } else {
                Vec::new()
            },
            notes: page
                .notes
                .iter()
                .filter_map(|indexed| {
                    let status = indexed.listed_status(metadata.through)?;
                    Some(indexed.note.wallet_note(&indexed.commitment, status))
                })
                .collect(),
            cursor: last.filter(|_| page.more).zip(id).map(|(after, snapshot)| {
                let cursor = WalletCursor {
                    ring: query.ring.to_bytes(),
                    wallet: query.wallet.to_bytes(),
                    snapshot,
                    after,
                };
                bytemuck::bytes_of(&cursor).to_vec().into()
            }),
            snapshot_expires_at: id.map(|_| metadata.expires_at),
        }
    }
}

impl PageDelta {
    pub(crate) const fn derived_owners(&self) -> &BTreeMap<[u8; 32], LateProgress> {
        &self.late_owners
    }

    pub(crate) fn notes(&self) -> impl Iterator<Item = (&[u8; 32], &StoredUtxo)> {
        self.notes.iter()
    }

    pub(crate) fn rows(&self) -> impl Iterator<Item = &SpendRow> {
        self.rows.values()
    }

    pub(crate) fn faults(&self) -> &[SpendRow] {
        &self.faults
    }

    pub(crate) fn read_frames(&self) -> impl Iterator<Item = &CreatedIn> {
        self.read.iter()
    }

    pub(crate) fn unread_frames(&self) -> impl Iterator<Item = &CreatedIn> {
        self.unread.iter()
    }

    pub(crate) const fn checkpoint(&self) -> Option<&Checkpoint> {
        self.checkpoint.as_ref()
    }

    pub(crate) fn owner_changes(&self) -> &[OwnerChange] {
        &self.owners
    }

    pub(crate) const fn prune_through(&self) -> u64 {
        self.prune_through
    }

    /// The revision its commit wrote, the cache is current through it once the mirror takes it.
    #[must_use]
    pub(crate) const fn at_commit(mut self, committed: CommittedPage) -> Self {
        self.revision = Some(committed.revision);
        self.first_unread = Some(committed.first_unread);
        self
    }
}

impl AuditStore {
    pub(crate) fn new(ring: Address, backing: Backing) -> Self {
        let capacity = match &backing {
            Backing::Database { capacity, .. } => *capacity,
            Backing::Unavailable => 0,
        };
        Self {
            ring,
            book: RwLock::new(SpendBook::bounded(capacity)),
            enrollments: RwLock::new(HeldEnrollments::bounded(capacity)),
            assets: RwLock::default(),
            late_owners: RwLock::default(),
            backfill: AsyncMutex::default(),
            backing,
            snapshot_limits: WalletSnapshotLimits::default(),
        }
    }

    fn read_book(&self) -> std::sync::RwLockReadGuard<'_, SpendBook> {
        self.book.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn reads(&self) -> Result<ProjectionReads<'_>, RingAuditError> {
        match &self.backing {
            Backing::Database {
                store, ring, seal, ..
            } => Ok(ProjectionReads {
                db: store.connection(),
                ring: *ring,
                seal,
            }),
            Backing::Unavailable => Err(RingAuditError::Service(ServiceError::StateUnavailable)),
        }
    }

    pub(crate) fn refresh(&self, loaded: LoadedSpends, revision: i64) {
        let LoadedSpends {
            checkpoint,
            first_unread,
        } = loaded;
        let mut book = self.book.write().unwrap_or_else(PoisonError::into_inner);
        if book.revision != Some(revision) {
            book.notes.clear();
            book.by_nullifier.clear();
            book.rows.clear();
            self.enrollments
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .clear();
        }
        book.absorb(PageDelta {
            checkpoint: Some(checkpoint),
            revision: Some(revision),
            first_unread: Some(first_unread),
            ..PageDelta::default()
        });
    }

    /// Runs after the page's transaction commits, a crash before it replays the page.
    pub fn commit(&self, delta: PageDelta) {
        let mut late = self
            .late_owners
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        for (owner, progress) in &delta.late_owners {
            match progress {
                LateProgress::Done => late.remove(owner),
                LateProgress::After(key) => late.insert(*owner, Some(*key)),
            };
        }
        drop(late);
        self.book
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .absorb(delta);
    }

    /// The cache answers first, storage answers each key the cache may have evicted.
    pub(crate) async fn notes(
        &self,
        commitments: &[[u8; 32]],
    ) -> Result<HashMap<[u8; 32], StoredUtxo>, RingAuditError> {
        let (mut found, unknown) =
            self.cached(commitments, |book, key| book.notes.get(key).copied());
        if !unknown.is_empty() {
            found.extend(self.reads()?.notes(&unknown).await?);
        }
        Ok(found)
    }

    pub async fn notes_of_nullifiers(
        &self,
        nullifiers: &[[u8; 32]],
    ) -> Result<HashMap<[u8; 32], ([u8; 32], StoredUtxo)>, RingAuditError> {
        let (commitments, unknown) =
            self.cached(nullifiers, |book, key| book.by_nullifier.get(key).copied());
        let (mut found, evicted) = {
            let book = self.read_book();
            let mut found = HashMap::new();
            let mut evicted = Vec::new();
            for (nullifier, commitment) in commitments {
                match book.notes.get(&commitment) {
                    Some(note) => {
                        found.insert(nullifier, (commitment, *note));
                    }
                    None => evicted.push(nullifier),
                }
            }
            (found, evicted)
        };
        let unknown: Vec<[u8; 32]> = unknown.into_iter().chain(evicted).collect();
        if !unknown.is_empty() {
            for (commitment, note) in self.reads()?.notes_by_nullifier(&unknown).await? {
                if let Some(nullifier) = note.nullifier {
                    found.insert(nullifier, (commitment, note));
                }
            }
        }
        Ok(found)
    }

    /// The coverage is read first, a spend the rows miss lies past it.
    pub async fn spend_read(&self, nullifiers: &[[u8; 32]]) -> Result<SpendRead, RingAuditError> {
        let history_through = self.read_book().history_through();
        Ok(SpendRead {
            spent: !self.spend_rows(nullifiers).await?.is_empty(),
            history_through,
        })
    }

    pub async fn spend_rows(
        &self,
        nullifiers: &[[u8; 32]],
    ) -> Result<HashMap<[u8; 32], SpendRow>, RingAuditError> {
        let (mut found, unknown) = self.cached(nullifiers, |book, key| book.rows.get(key).copied());
        if !unknown.is_empty() {
            found.extend(
                self.reads()?
                    .rows(&unknown)
                    .await?
                    .into_iter()
                    .map(|row| (row.nullifier, row)),
            );
        }
        Ok(found)
    }

    fn cached<V>(
        &self,
        keys: &[[u8; 32]],
        lookup: impl Fn(&SpendBook, &[u8; 32]) -> Option<V>,
    ) -> (HashMap<[u8; 32], V>, Vec<[u8; 32]>) {
        let book = self.read_book();
        split_cached(keys, |key| lookup(&book, key))
    }

    #[cfg(test)]
    pub(crate) fn row_state(&self, nullifier: &[u8; 32]) -> Option<SpendState> {
        self.read_book().rows.get(nullifier).map(|row| row.state)
    }

    /// One bounded range of the owner's live notes and unsettled spends, never another owner's
    /// row or a settled spend.
    pub(crate) async fn owner_page(&self, range: OwnerRange) -> Result<OwnerPage, RingAuditError> {
        let reads = self.reads()?;
        Ok(OwnerNotesPage {
            ring: reads.ring,
            seal: reads.seal,
            range,
        }
        .read(reads.db)
        .await?)
    }

    #[must_use]
    pub(crate) const fn with_snapshot_limits(mut self, limits: WalletSnapshotLimits) -> Self {
        self.snapshot_limits = limits;
        self
    }

    pub async fn wallet_status(
        &self,
        query: WalletStatusQuery,
    ) -> Result<GetWalletStatusResponse, RingAuditError> {
        query.validate_scope()?;
        if Member::owner_tag(query.wallet.as_array()).is_err() {
            if query.after.is_some() {
                return Err(RingAuditError::InvalidCursor);
            }
            return Ok(GetWalletStatusResponse {
                history_through: self.read_book().history_through(),
                enrollment: query.enrollment,
                balances: Vec::new(),
                notes: Vec::new(),
                cursor: None,
                snapshot_expires_at: None,
            });
        }
        let snapshots = self.snapshots(query.ring)?;
        let page = if query.after.is_some() {
            snapshots.read(query).await?
        } else {
            snapshots.capture(query).await?
        };
        Ok(page.response(query))
    }

    pub async fn release_wallet_snapshot(
        &self,
        release: WalletSnapshotRelease,
    ) -> Result<(), RingAuditError> {
        release.validate()?;
        self.snapshots(release.ring)?.release(release).await
    }

    fn snapshots(&self, ring: Address) -> Result<WalletSnapshots<'_>, RingAuditError> {
        let reads = self.reads()?;
        if ring != reads.ring {
            return Err(RingAuditError::InvalidCursor);
        }
        Ok(WalletSnapshots {
            db: reads.db,
            seal: reads.seal,
            limits: self.snapshot_limits,
        })
    }

    pub(crate) async fn spend_coverage(
        &self,
        cursor: CoverageCursor,
    ) -> Result<GetSpendCoverageResponse, RingAuditError> {
        Ok(self.reads()?.coverage(cursor).await?)
    }

    pub(crate) async fn due_repairs(
        &self,
        repairs: &mut RepairBook,
        now: u64,
    ) -> Result<Vec<CreatedIn>, RingAuditError> {
        let due = self.reads()?.due_repairs(now).await?;
        let frames = due.iter().map(|(frame, _)| *frame).collect();
        *repairs = due.into_iter().collect();
        Ok(frames)
    }

    pub async fn visit_spendable_notes(
        &self,
        notes: SpendableNotes<'_>,
        mut visit: impl FnMut(WalletUtxo),
    ) -> Result<(), RingAuditError> {
        let scan = SpendableScan::new(notes)?;
        let reads = self.reads()?;
        let snapshot = read_snapshot(reads.db).await?;
        scan.visit(&reads.at(&snapshot), &mut visit).await?;
        snapshot.commit().await?;
        Ok(())
    }

    pub async fn merge_plans(
        &self,
        notes: SpendableNotes<'_>,
        targets: &HashMap<Address, MergeTarget>,
    ) -> Result<HashMap<Address, Vec<WalletUtxo>>, RingAuditError> {
        let scan = SpendableScan::new(notes)?;
        let mut totals = MergeTotals::new(targets);
        let reads = self.reads()?;
        let snapshot = read_snapshot(reads.db).await?;
        let reads = reads.at(&snapshot);
        scan.visit(&reads, |note| totals.record(note)).await?;
        let mut selected = totals.select();
        if !selected.is_empty() {
            scan.visit(&reads, |note| selected.record(note)).await?;
        }
        snapshot.commit().await?;
        Ok(selected.finish())
    }

    /// The cache answers first, storage answers each key the cache may have evicted.
    pub(crate) async fn enrollments(
        &self,
        key: EnrollmentKey,
        values: &[[u8; 32]],
    ) -> Result<HashMap<[u8; 32], HeldEnrollment>, RingAuditError> {
        let (mut found, unknown) = {
            let cache = self.read_enrollments();
            split_cached(values, |value| match key {
                EnrollmentKey::Member => cache.by_member(value),
                EnrollmentKey::ListKey => cache.by_list_key(value),
            })
        };
        if !unknown.is_empty() {
            found.extend(
                self.reads()?
                    .enrollments(key, &unknown)
                    .await?
                    .into_iter()
                    .map(|held| (key.of(&held), held)),
            );
        }
        Ok(found)
    }

    pub(crate) async fn enrollment(
        &self,
        key: EnrollmentKey,
        value: &[u8; 32],
    ) -> Result<Option<HeldEnrollment>, RingAuditError> {
        Ok(self.enrollments(key, &[*value]).await?.remove(value))
    }

    /// Checks a scanned page against every stored row it names, before anything commits.
    pub async fn enrollment_admission(
        &self,
        found: Vec<HeldEnrollment>,
    ) -> Result<EnrollmentAdmission, RingAuditError> {
        let members: Vec<[u8; 32]> = found.iter().map(HeldEnrollment::member).collect();
        let held = self.enrollments(EnrollmentKey::Member, &members).await?;
        Ok(HeldEnrollments::admission(found, &held)?)
    }

    /// Runs after the page commits, `pending_derivations` queues each new owner's notes.
    pub fn admit_enrollments(&self, admission: EnrollmentAdmission) {
        self.enrollments
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(admission.rows().iter().copied());
    }

    pub(crate) fn replace_late_owners(&self, owners: Vec<LateOwner>) {
        *self
            .late_owners
            .write()
            .unwrap_or_else(PoisonError::into_inner) = owners
            .into_iter()
            .take(MAX_LATE_OWNERS)
            .map(|LateOwner { owner, resume }| (owner, resume))
            .collect();
    }

    /// An owner already queued keeps the key its derivation reached.
    #[cfg(test)]
    pub(crate) fn queue_late_owners(&self, owners: impl IntoIterator<Item = impl Into<LateOwner>>) {
        let mut late = self
            .late_owners
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        for LateOwner { owner, resume } in owners.into_iter().map(Into::into) {
            late.entry(owner).or_insert(resume);
        }
    }

    /// The held enrollment's key, `None` on a cache miss, the replay then resolves the note.
    pub(crate) fn registered_key(&self, owner_key: &[u8; 32]) -> Option<[u8; 32]> {
        let member = Member::owner_tag(owner_key).ok()?;
        self.read_enrollments()
            .by_member(member.as_bytes())
            .map(|held| held.nullifier_pk)
    }

    fn read_enrollments(&self) -> std::sync::RwLockReadGuard<'_, HeldEnrollments> {
        self.enrollments
            .read()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn context(&self) -> Context {
        self.read_book().context
    }

    pub(crate) async fn view(
        &self,
        utxo_hash: &Hash,
    ) -> Result<Option<AuditedUtxoView>, RingAuditError> {
        let Some(stored) = self.notes(&[utxo_hash.0]).await?.remove(&utxo_hash.0) else {
            return Ok(None);
        };
        Ok(Some(AuditedUtxoView {
            utxo_hash: Hash(utxo_hash.0),
            utxo: stored.utxo.into(),
            slot: stored.created_in.slot,
            tx_signature: stored.created_in.tx_signature.into(),
            event_index: stored.created_in.event_index,
            slot_index: stored.slot_index,
        }))
    }

    /// The audit view of a merge the checkpoint opened.
    pub(crate) async fn merge_output(
        &self,
        merge: &ConfirmedMerge,
    ) -> Result<Option<AuditedMergeOutput>, RingAuditError> {
        let commitment = merge.output_commitment();
        let opened = self.notes(&[commitment]).await?.contains_key(&commitment);
        Ok(opened.then(|| merge.output().ok()).flatten())
    }

    /// The nullifiers among `nullifiers` whose resolved note `wallet` owns.
    pub async fn input_parties<'i>(
        &self,
        wallet: &Address,
        nullifiers: impl IntoIterator<Item = &'i Hash>,
    ) -> Result<InputParties, RingAuditError> {
        let nullifiers: Vec<[u8; 32]> = nullifiers
            .into_iter()
            .map(|nullifier| nullifier.0)
            .collect();
        let resolved: Vec<([u8; 32], [u8; 32])> = self
            .spend_rows(&nullifiers)
            .await?
            .into_values()
            .filter_map(|row| match row.state {
                SpendState::Resolved { note } => Some((row.nullifier, note)),
                _ => None,
            })
            .collect();
        let commitments: Vec<[u8; 32]> = resolved.iter().map(|(_, note)| *note).collect();
        let notes = self.notes(&commitments).await?;
        Ok(InputParties {
            own: resolved
                .into_iter()
                .filter(|(_, note)| {
                    notes
                        .get(note)
                        .is_some_and(|note| note.utxo.owner_key == wallet.to_bytes())
                })
                .map(|(nullifier, _)| nullifier)
                .collect(),
        })
    }
}

impl From<[u8; 32]> for LateOwner {
    fn from(owner: [u8; 32]) -> Self {
        Self {
            owner,
            resume: None,
        }
    }
}

impl InputParties {
    /// Fails closed, a nullifier no resolved row names to the wallet reads as another party's.
    pub fn owns(&self, nullifier: &Hash) -> bool {
        self.own.contains(&nullifier.0)
    }
}

impl StoredUtxo {
    fn wallet_note(&self, commitment: &[u8; 32], status: WalletNoteStatus) -> WalletNote {
        let utxo = &self.utxo;
        WalletNote {
            utxo_hash: Hash(*commitment),
            status,
            nullifier_pk: utxo.owner.nullifier_pk().map(Hash),
            asset: Hash(utxo.asset),
            mint: utxo.mint.map(Into::into),
            amount: utxo.amount,
            blinding: Hash(utxo.blinding),
            data_hash: utxo.data_hash.map(Hash),
            ring_data_hash: utxo.ring_data_hash.map(Hash),
            tree_id: utxo.tree_id,
            leaf_index: self.leaf_index,
            slot: self.created_in.slot,
            tx_signature: self.created_in.tx_signature.into(),
            event_index: self.created_in.event_index,
            slot_index: self.slot_index,
        }
    }
}

impl<'a> SpendableScan<'a> {
    fn new(query: SpendableNotes<'a>) -> Result<Self, RingAuditError> {
        let nullifier_pubkey = query.nullifier_key.pubkey()?;
        let mints = query.mints.iter().map(|mint| (mint.asset, *mint)).collect();
        let range = Member::owner_tag(&query.owner_key)
            .ok()
            .map(|member| OwnerRange {
                member: *member.as_bytes(),
                after: None,
                limit: OWNER_ROW_BATCH,
            });
        Ok(Self {
            query,
            nullifier_pubkey,
            mints,
            range,
        })
    }

    fn candidate(&self, indexed: &IndexedNote) -> Option<WalletUtxo> {
        let stored = &indexed.note;
        let utxo = &stored.utxo;
        let nullifier = stored.nullifier?;
        let mint = self.mints.get(&utxo.mint?)?;
        if indexed.spent_at.is_some()
            || utxo.owner_key != self.query.owner_key
            || utxo.owner
                != (UtxoOwner::Keyed {
                    nullifier_pk: self.nullifier_pubkey,
                })
            || utxo.data_hash != Some([0; 32])
            || utxo.ring_data_hash.is_none()
            || self.query.held.contains(&nullifier)
        {
            return None;
        }
        Some(WalletUtxo {
            utxo: Utxo {
                owner: PublicKey::from_ed25519(&utxo.owner_key),
                asset: *mint,
                amount: utxo.amount,
                blinding: utxo.blinding,
                ring_program_id: Some(self.query.ring),
                data: Data::default(),
            },
            nullifier_pubkey: self.nullifier_pubkey,
            utxo_hash: indexed.commitment,
            nullifier,
            tx_viewing_key: None,
            data_hash: None,
            ring_data_hash: utxo.ring_data_hash.filter(|hash| *hash != [0; 32]),
            tree_id: utxo.tree_id,
            leaf_index: stored.leaf_index,
            latest_tree_id: None,
            slot: stored.created_in.slot,
            tx_signature: stored.created_in.tx_signature,
            slot_index: stored.slot_index,
        })
    }

    async fn visit(
        &self,
        reads: &ProjectionReads<'_, sea_orm::DatabaseTransaction>,
        mut visit: impl FnMut(WalletUtxo),
    ) -> Result<(), RingAuditError> {
        let Some(mut range) = self.range else {
            return Ok(());
        };
        loop {
            let page = OwnerNotesPage {
                ring: reads.ring,
                seal: reads.seal,
                range,
            }
            .read(reads.db)
            .await?;
            let last = page.notes.last().map(|note| note.key);
            let candidates: Vec<_> = page
                .notes
                .iter()
                .filter_map(|indexed| self.candidate(indexed))
                .collect();
            let nullifiers: Vec<_> = candidates.iter().map(|note| note.nullifier).collect();
            let spent: HashSet<_> = reads
                .rows(&nullifiers)
                .await?
                .into_iter()
                .map(|row| row.nullifier)
                .collect();
            for note in candidates {
                if !spent.contains(&note.nullifier) {
                    visit(note);
                }
            }
            if !page.more {
                break;
            }
            let Some(last) = last else {
                break;
            };
            range.after = Some(last);
        }
        Ok(())
    }
}

/// Splits keys into cache hits and keys only storage can answer.
fn split_cached<V>(
    keys: &[[u8; 32]],
    lookup: impl Fn(&[u8; 32]) -> Option<V>,
) -> (HashMap<[u8; 32], V>, Vec<[u8; 32]>) {
    let mut found = HashMap::new();
    let mut unknown = Vec::new();
    for key in keys {
        match lookup(key) {
            Some(value) => {
                found.insert(*key, value);
            }
            None => unknown.push(*key),
        }
    }
    (found, unknown)
}

/// Decoded state in a fixed order, sealed rows differ in their nonces alone.
#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StoreDump {
    pub notes: BTreeMap<[u8; 32], StoredUtxo>,
    pub rows: BTreeMap<[u8; 32], SpendRow>,
    pub first_unread: Option<CreatedIn>,
    pub context: Context,
    pub position_slot: Option<u64>,
}

#[cfg(any(test, feature = "test-fixtures"))]
impl AuditStore {
    /// Notes whose held enrollment opens to `key`, their nullifiers derive as the page opens them.
    pub async fn record(
        &self,
        created_in: CreatedIn,
        utxos: impl IntoIterator<Item = zolana_ring_client::CreatedUtxo>,
        key: &NullifierKey,
    ) {
        let opener = |_: &HeldEnrollment| Some(key.clone());
        let mut work = PageWork::new(self, &opener);
        work.open(created_in, utxos);
        let delta = work.finish().await.expect("fixture page");
        self.commit_fixture(delta).await;
    }

    pub async fn on_internal_database(ring: Address, auditor: &spr_keypair::X25519Keypair) -> Self {
        Self::new(
            ring,
            Backing::Database {
                store: Store::in_memory().await.expect("internal store"),
                ring,
                seal: Box::new(NoteSeal::derive(auditor, ring).expect("seal")),
                capacity: MAX_CACHED_ENTRIES,
            },
        )
    }

    pub(crate) async fn fixture_writer(&self) -> crate::spent_index::ProjectionWriter<'_> {
        let Backing::Database {
            store, ring, seal, ..
        } = &self.backing
        else {
            panic!("a fixture store holds a database");
        };
        let clock = ring_service_core::clock::Clock::System;
        let lease = crate::projection_lease::LeaseClaim {
            db: store.connection(),
            ring: *ring,
            holder: store.lease_holder(),
            now: clock.now().expect("clock"),
        }
        .claim()
        .await
        .expect("claim")
        .expect("the fixture replica holds the lease");
        crate::spent_index::ProjectionWriter {
            db: store.connection(),
            ring: *ring,
            seal,
            lease,
            clock,
        }
    }

    /// The store reads wallets from its tables, the page writes them first.
    async fn commit_fixture(&self, delta: PageDelta) {
        let revision = self
            .fixture_writer()
            .await
            .commit_page(&delta)
            .await
            .expect("fixture page");
        self.commit(delta.at_commit(revision));
    }

    /// Published rows with no frame the test reads.
    pub async fn spend(&self, nullifiers: impl IntoIterator<Item = [u8; 32]>) {
        use crate::spend_machine::{InputRead, Observation};
        let opener = |_: &HeldEnrollment| None;
        let mut work = PageWork::new(self, &opener);
        for nullifier in nullifiers {
            work.observe(Observation {
                nullifier,
                frame: CreatedIn {
                    slot: 1,
                    tx_signature: Signature::from(
                        nullifier.repeat(2).try_into().unwrap_or([0; 64]),
                    ),
                    event_index: 0,
                },
                position: 0,
                read: InputRead::Spend,
                hit: None,
            });
        }
        let delta = work.finish().await.expect("fixture page");
        self.commit_fixture(delta).await;
    }
}

#[cfg(test)]
impl AuditStore {
    pub(crate) fn cached_enrollments(&self) -> [usize; 2] {
        self.read_enrollments().len()
    }

    /// Entries the cache holds of notes, the note index and spend rows.
    pub(crate) fn cached_entries(&self) -> [usize; 3] {
        let book = self.read_book();
        [
            book.notes.iter().count(),
            book.by_nullifier.iter().count(),
            book.rows.iter().count(),
        ]
    }

    pub(crate) fn dump(&self) -> StoreDump {
        let book = self.read_book();
        StoreDump {
            notes: book.notes.iter().map(|(key, note)| (*key, *note)).collect(),
            rows: book.rows.iter().map(|(key, row)| (*key, *row)).collect(),
            first_unread: book.first_unread,
            context: book.context,
            position_slot: book.position_slot,
        }
    }
}

#[cfg(test)]
#[path = "spend_store_tests.rs"]
mod spend_store_tests;
