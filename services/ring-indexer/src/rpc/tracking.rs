use std::ops::Range;

use custom_ring_interface::{
    check_member,
    storage_instruction::ApplyEnrollmentsIxData,
    tagged_x25519,
    tracking::{owner_member, tracking_chain_start, TrackingEntry, TrackingRegistry},
};
use sea_orm::{ConnectionTrait, DatabaseConnection};
use solana_address::Address;
use spr_hasher::primitives::hash_bytes;
use spr_indexer_api::{Base64String, Hash};
use zolana_ring_indexer::{
    tracking::{
        GetTrackingApplyProofInputsRequest, GetTrackingApplyProofInputsResponse,
        GetTrackingOpeningsRequest, GetTrackingOpeningsResponse, GetTrackingRecordPoolRequest,
        GetTrackingRecordPoolResponse, GetTrackingRecordsRequest, GetTrackingRecordsResponse,
        RingIndexedTrackingRecordView, RingTrackingEntryView, RingTrackingOpening,
        RingTrackingRecordView, RingTrackingSealView, RingTrackingWriteView,
    },
    tree::{Canopy, TrackingRecord, TrackingWriteProofInput},
};

use super::{
    check_members, check_openings, check_page_limit, internal, path_view, storage_root_view,
    validation, ChainSource, LeafPage, OpenedLeaf, Served, Snapshot,
};
use crate::{
    error::RingApiError,
    member_registry::{LoadedRegistry, TreeRead},
    storage::{RegistryState, StorageRoot, TrackingStore},
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

/// The writer's entries apply in request order from the current root, as its next `ApplyEnrollments` writes them.
pub(crate) async fn tracking_apply_proof_inputs(
    db: &DatabaseConnection,
    chain: &impl ChainSource,
    request: GetTrackingApplyProofInputsRequest,
) -> Result<GetTrackingApplyProofInputsResponse, RingApiError> {
    let count = u8::try_from(request.entries.len()).map_err(validation)?;
    ApplyEnrollmentsIxData::check_count(count).map_err(validation)?;
    let entries = request
        .entries
        .iter()
        .map(entry_of)
        .collect::<Result<Vec<_>, _>>()?;
    let snapshot = TrackingSnapshot::registry(db, &request.registry).await?;
    let root = StorageRoot::current(&*snapshot.state.account);
    let store = snapshot.store();
    let chain_start =
        tracking_chain_start(&request.registry.0, root.revision.into()).map_err(internal)?;
    let loaded: LoadedTracking = TreeRead::batch(root.tree(), &entries)
        .load(store.tree())
        .await
        .map_err(internal)?;
    let inputs = loaded
        .tree
        .apply_batch(chain_start, &entries)
        .map_err(validation)?
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
        writes: inputs.writes.iter().map(write_view).collect(),
    })
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
    let seal = &key.seal;
    let owner = Address::new_from_array(seal.owner);
    if hash_bytes(&key.viewing_u).map_err(internal)? != record.viewing_hash
        || hash_bytes(&seal.ciphertext).map_err(internal)? != record.ct_hash
        || owner_member(&owner).map_err(internal)? != record.member
    {
        return Err(internal("a stored key or seal disagrees with its leaf"));
    }
    Ok(RingTrackingRecordView {
        seal: Some(RingTrackingSealView {
            owner: owner.into(),
            viewing_u: Base64String(key.viewing_u.to_vec()),
            eph_pk: Base64String(tagged_x25519(&seal.eph_u).to_vec()),
            ciphertext: Base64String(seal.ciphertext.to_vec()),
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

fn write_view(write: &TrackingWriteProofInput) -> RingTrackingWriteView {
    let entry = &write.entry;
    RingTrackingWriteView {
        member: Hash(entry.member),
        proven: entry.proven,
        present: write.present,
        nullifier_pk: Hash(entry.nullifier_pk),
        ct_hash: Hash(entry.ct_hash),
        list_tag: Hash(entry.list_tag),
        viewing_hash: Hash(entry.viewing_hash),
        prior_viewing_hash: Hash(entry.prior_viewing_hash),
        slot: path_view(write.slot.index, &write.slot.siblings),
        low: record_view(&write.low),
        low_path: path_view(write.low_path.index, &write.low_path.siblings),
    }
}

fn entry_of(view: &RingTrackingEntryView) -> Result<TrackingEntry, RingApiError> {
    check_member(&view.member.0).map_err(validation)?;
    Ok(TrackingEntry {
        member: view.member.0,
        nullifier_pk: view.nullifier_pk.0,
        ct_hash: view.ct_hash.0,
        list_tag: view.list_tag.0,
        viewing_hash: view.viewing_hash.0,
        prior_viewing_hash: view.prior_viewing_hash.0,
        proven: view.proven,
    })
}
