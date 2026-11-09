use std::ops::Range;

use custom_ring_interface::{
    storage_instruction::ApplyEnrollmentsIxData,
    tagged_x25519,
    tracking::{tracking_chain_start, EnrollmentQueue, TrackingRegistry},
};
use sea_orm::{ConnectionTrait, DatabaseConnection};
use solana_address::Address;
use spr_hasher::primitives::hash_bytes;
use spr_indexer_api::{Base64String, Context as ApiContext, Hash};
use zolana_ring_indexer::{
    tracking::{
        GetEnrollmentQueueRequest, GetEnrollmentQueueResponse, GetTrackingApplyProofInputsRequest,
        GetTrackingApplyProofInputsResponse, GetTrackingOpeningsRequest,
        GetTrackingOpeningsResponse, GetTrackingRecordPoolRequest, GetTrackingRecordPoolResponse,
        GetTrackingRecordsRequest, GetTrackingRecordsResponse, RingIndexedTrackingRecordView,
        RingQueuedEnrollmentView, RingTrackingOpening, RingTrackingRecordView,
        RingTrackingSealView, RingTrackingWriteView,
    },
    tree::{Canopy, TrackingRecord, TrackingWriteProofInput},
};

use super::{
    check_members, check_openings, check_page_limit, internal, path_view, storage_root_view,
    validation, ChainSource, LeafPage, OpenedLeaf, Released, Served, Snapshot,
};
use crate::{
    error::RingApiError,
    member_registry::{LoadedRegistry, TreeRead},
    storage::{QueueBounds, QueuedEnrollment, RegistryState, StorageRoot, TrackingStore},
};

type TrackingSnapshot = Snapshot<RegistryState<TrackingRegistry>>;
type LoadedTracking = LoadedRegistry<TrackingRecord>;

pub(crate) async fn tracking_openings(
    db: &DatabaseConnection,
    chain: &impl ChainSource,
    request: GetTrackingOpeningsRequest,
) -> Result<GetTrackingOpeningsResponse, RingApiError> {
    check_members(&request.members)?;
    let read = TrackingSnapshot::registry(db, &request.registry)
        .await?
        .at_root(&request.expected_root.0)
        .await?;
    let loaded: LoadedTracking = read.load(&request.members, None).await?;
    let store = read.snapshot.store();
    let version = read.root.revision;
    let mut openings = Vec::with_capacity(request.members.len());
    for member in &request.members {
        let opened = loaded.opening(&read.root.root, &member.0)?;
        openings.push(RingTrackingOpening {
            present: opened.present,
            record: sealed_view(&store, &opened.record, version).await?,
            path: opened.path,
        });
    }
    let root = read.root;
    let context = read.snapshot.confirm(chain, Served::Root(&root)).await?;
    Ok(GetTrackingOpeningsResponse {
        context,
        root: storage_root_view(&root),
        openings,
    })
}

/// Every leaf carries its viewing key and the seal of the entry that inserted it.
pub(crate) async fn tracking_records(
    db: &DatabaseConnection,
    chain: &impl ChainSource,
    request: GetTrackingRecordsRequest,
) -> Result<GetTrackingRecordsResponse, RingApiError> {
    check_page_limit(request.limit)?;
    let read = TrackingSnapshot::registry(db, &request.registry)
        .await?
        .at_root(&request.expected_root.0)
        .await?;
    let root = read.root;
    if !request.starts_within(root.next_index) {
        return Err(validation("invalid tracking page start"));
    }
    let leaves: Range<u64> = request.start_index
        ..request
            .start_index
            .saturating_add(request.limit)
            .min(root.next_index);
    let end = leaves.end;
    let store = read.snapshot.store();
    let opened = LeafPage {
        store: store.tree(),
        root: &root.root,
        at: root.tree(),
        leaves,
    }
    .read()
    .await?;
    let records = indexed_views(&store, opened, root.revision).await?;
    let context = read.snapshot.confirm(chain, Served::Root(&root)).await?;
    Ok(GetTrackingRecordsResponse {
        context,
        root: storage_root_view(&root),
        records,
        next_leaf_index: (end < root.next_index).then_some(end),
    })
}

pub(crate) async fn tracking_record_pool(
    db: &DatabaseConnection,
    chain: &impl ChainSource,
    request: GetTrackingRecordPoolRequest,
) -> Result<GetTrackingRecordPoolResponse, RingApiError> {
    check_members(&request.members)?;
    check_openings(&request.members, request.openings, |pools| pools.tracking)?;
    let canopy = Canopy::covering(request.openings as usize).map_err(validation)?;
    let read = TrackingSnapshot::registry(db, &request.registry)
        .await?
        .at_root(&request.expected_root.0)
        .await?;
    let loaded: LoadedTracking = read.load(&request.members, Some(canopy)).await?;
    let pool = loaded.pool(&read.root.root, &request.members, canopy)?;
    let records = indexed_views(&read.snapshot.store(), pool.records, read.root.revision).await?;
    let root = read.root;
    let context = read.snapshot.confirm(chain, Served::Root(&root)).await?;
    Ok(GetTrackingRecordPoolResponse {
        context,
        root: storage_root_view(&root),
        records,
        canopy: pool.canopy,
        subjects: pool.subjects,
    })
}

/// Unverified entries in sequence order, a queued entry grants nothing.
pub(crate) async fn enrollment_queue(
    db: &DatabaseConnection,
    chain: &impl ChainSource,
    request: GetEnrollmentQueueRequest,
) -> Result<GetEnrollmentQueueResponse, RingApiError> {
    check_page_limit(request.limit)?;
    let snapshot = TrackingSnapshot::registry(db, &request.registry).await?;
    let store = snapshot.store();
    let bounds = store.queue_bounds().await.map_err(internal)?;
    let from = request.after.map_or(bounds.head, |after| {
        after.saturating_add(1).max(bounds.head)
    });
    let rows = store.queued(from, request.limit).await.map_err(internal)?;
    let context = snapshot.confirm_queue(chain, bounds).await?;
    Ok(GetEnrollmentQueueResponse {
        context,
        head: bounds.head,
        tail: bounds.tail,
        entries: rows.iter().map(queued_view).collect::<Result<_, _>>()?,
    })
}

/// The next `count` entries from `head` apply from the current root, as the next `ApplyEnrollments` consumes them.
pub(crate) async fn tracking_apply_proof_inputs(
    db: &DatabaseConnection,
    chain: &impl ChainSource,
    request: GetTrackingApplyProofInputsRequest,
) -> Result<GetTrackingApplyProofInputsResponse, RingApiError> {
    let count = u8::try_from(request.count).map_err(validation)?;
    ApplyEnrollmentsIxData::check_count(count).map_err(validation)?;
    let count = usize::from(count);
    let snapshot = TrackingSnapshot::registry(db, &request.registry).await?;
    let root = StorageRoot::current(&*snapshot.state.account);
    let store = snapshot.store();
    let head = store.queue_bounds().await.map_err(internal)?.head;
    let rows = store.queued(head, request.count).await.map_err(internal)?;
    if rows.len() != count {
        return Err(validation("the queue holds fewer entries"));
    }
    let entries = rows
        .iter()
        .map(QueuedEnrollment::entry)
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    let chain_start =
        tracking_chain_start(&request.registry.0, root.revision.into()).map_err(internal)?;
    let loaded: LoadedTracking = TreeRead::batch(root.tree(), &entries)
        .load(store.tree())
        .await
        .map_err(internal)?;
    let inputs = loaded
        .tree
        .apply_batch(chain_start, &entries)
        .map_err(internal)?
        .inputs;
    if inputs.old_root != root.root {
        return Err(internal("the loaded tree does not reach the current root"));
    }
    let context = snapshot.confirm(chain, Served::Current(&root)).await?;
    Ok(GetTrackingApplyProofInputsResponse {
        context,
        revision: root.revision,
        old_root: Hash(inputs.old_root),
        new_root: Hash(inputs.new_root),
        old_next_index: inputs.old_next_index,
        new_next_index: inputs.new_next_index,
        chain_start: Hash(inputs.chain_start),
        chain_end: Hash(inputs.chain_end),
        writes: rows
            .iter()
            .zip(&inputs.writes)
            .map(|(row, write)| write_view(row.sequence, write))
            .collect(),
    })
}

impl TrackingSnapshot {
    /// The queue account at the tip holds the bounds the read served.
    async fn confirm_queue(
        self,
        chain: &impl ChainSource,
        bounds: QueueBounds,
    ) -> Result<ApiContext, RingApiError> {
        let Released { tip, state } = self.release().await?;
        let (queue, _) = EnrollmentQueue::find_address(&Address::new_from_array(state.registry));
        let confirmation = chain
            .confirmation::<EnrollmentQueue>(&tip, &queue.to_bytes())
            .await?;
        let held = &*confirmation.chain;
        if held.head() != bounds.head || held.tail() != bounds.tail {
            return Err(RingApiError::RootChanged);
        }
        Ok(confirmation.context)
    }
}

async fn indexed_views<C: ConnectionTrait>(
    store: &TrackingStore<'_, C>,
    leaves: Vec<OpenedLeaf<TrackingRecord>>,
    version: u64,
) -> Result<Vec<RingIndexedTrackingRecordView>, RingApiError> {
    let mut views = Vec::with_capacity(leaves.len());
    for leaf in leaves {
        views.push(RingIndexedTrackingRecordView {
            record: sealed_view(store, &leaf.record, version).await?,
            path: leaf.path,
        });
    }
    Ok(views)
}

/// Serves a key only under the `viewing_hash` and a seal only under the `ct_hash` its leaf version binds.
async fn sealed_view<C: ConnectionTrait>(
    store: &TrackingStore<'_, C>,
    record: &TrackingRecord,
    version: u64,
) -> Result<RingTrackingRecordView, RingApiError> {
    if record.member == [0; 32] {
        return Ok(record_view(record));
    }
    let key = store
        .leaf_key(&record.member, version)
        .await
        .map_err(internal)?
        .ok_or_else(|| internal("a tracking leaf has no viewing key"))?;
    if hash_bytes(&key.viewing_u).map_err(internal)? != record.viewing_hash {
        return Err(internal("a stored viewing key disagrees with its leaf"));
    }
    let inserted = store
        .queued_at(key.sequence)
        .await
        .map_err(internal)?
        .ok_or_else(|| internal("a tracking leaf has no inserting entry"))?;
    let entry = inserted.entry().map_err(internal)?;
    if !inserted.proven || entry.member != record.member || entry.ct_hash != record.ct_hash {
        return Err(internal("a stored seal disagrees with its leaf"));
    }
    Ok(RingTrackingRecordView {
        seal: Some(RingTrackingSealView {
            owner: Address::new_from_array(inserted.owner).into(),
            viewing_u: Base64String(key.viewing_u.to_vec()),
            eph_pk: Base64String(tagged_x25519(&inserted.ix.eph_u).to_vec()),
            ciphertext: Base64String(inserted.ix.ciphertext.to_vec()),
        }),
        ..record_view(record)
    })
}

fn record_view(record: &TrackingRecord) -> RingTrackingRecordView {
    RingTrackingRecordView {
        member: Hash(record.member),
        next: Hash(record.next),
        nullifier_pk: Hash(record.nullifier_pk),
        ct_hash: Hash(record.ct_hash),
        list_tag: Hash(record.list_tag),
        viewing_hash: Hash(record.viewing_hash),
        seal: None,
    }
}

fn write_view(sequence: u64, write: &TrackingWriteProofInput) -> RingTrackingWriteView {
    let entry = &write.entry;
    RingTrackingWriteView {
        sequence,
        member: Hash(entry.member),
        proven: entry.proven,
        present: write.present,
        nullifier_pk: Hash(entry.nullifier_pk),
        ct_hash: Hash(entry.ct_hash),
        list_tag: Hash(entry.list_tag),
        viewing_hash: Hash(entry.viewing_hash),
        slot: path_view(write.slot.index, &write.slot.siblings),
        low: record_view(&write.low),
        low_path: path_view(write.low_path.index, &write.low_path.siblings),
    }
}

fn queued_view(row: &QueuedEnrollment) -> Result<RingQueuedEnrollmentView, RingApiError> {
    let ix = &row.ix;
    Ok(RingQueuedEnrollmentView {
        sequence: row.sequence,
        owner: Address::new_from_array(row.owner).into(),
        member: Hash(row.entry().map_err(internal)?.member),
        proven: row.proven,
        nullifier_pk: Hash(ix.nullifier_pk),
        list_tag: Hash(ix.list_tag),
        viewing_u: Base64String(ix.viewing_u.to_vec()),
        eph_pk: Base64String(tagged_x25519(&ix.eph_u).to_vec()),
        ciphertext: Base64String(ix.ciphertext.to_vec()),
    })
}
