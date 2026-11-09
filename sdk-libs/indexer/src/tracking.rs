use serde::{Deserialize, Serialize};
use spr_indexer_api::{Base64String, Context, Hash, RpcMethod, SerializablePubkey};

use crate::rpc::{RingRecordPathView, StorageRootView};

pub mod method {
    use super::*;

    pub struct GetTrackingOpenings;
    pub struct GetTrackingRecords;
    pub struct GetTrackingRecordPool;
    pub struct GetTrackingApplyProofInputs;

    impl RpcMethod for GetTrackingOpenings {
        const NAME: &'static str = "getTrackingOpenings";
        type Request = GetTrackingOpeningsRequest;
        type Response = GetTrackingOpeningsResponse;
    }

    impl RpcMethod for GetTrackingRecords {
        const NAME: &'static str = "getTrackingRecords";
        type Request = GetTrackingRecordsRequest;
        type Response = GetTrackingRecordsResponse;
    }

    impl RpcMethod for GetTrackingRecordPool {
        const NAME: &'static str = "getTrackingRecordPool";
        type Request = GetTrackingRecordPoolRequest;
        type Response = GetTrackingRecordPoolResponse;
    }

    impl RpcMethod for GetTrackingApplyProofInputs {
        const NAME: &'static str = "getTrackingApplyProofInputs";
        type Request = GetTrackingApplyProofInputsRequest;
        type Response = GetTrackingApplyProofInputsResponse;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RingTrackingRecordView {
    pub member: Hash,
    pub next: Hash,
    pub nullifier_pk: Hash,
    pub ct_hash: Hash,
    pub list_tag: Hash,
    /// Binds the seal's `viewing_u` and `seq`, a sender trusts the key only after checking it.
    pub viewing_hash: Hash,
    /// Absent on the sentinel alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seal: Option<RingTrackingSealView>,
}

/// `ciphertext` hashes to the leaf's `ct_hash`, `eph_pk` and `ciphertext` come from the write that inserted the leaf.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RingTrackingSealView {
    /// Its owner tag is the leaf's `member`.
    pub owner: SerializablePubkey,
    /// The leaf's current X25519 viewing key, 32 bytes.
    pub viewing_u: Base64String,
    /// Rotations the leaf has taken, zero on a first enrollment.
    pub seq: u64,
    /// Tagged X25519, 33 bytes.
    pub eph_pk: Base64String,
    /// 64 bytes.
    pub ciphertext: Base64String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RingTrackingOpening {
    /// False proves the member holds no tracking row under `root`.
    pub present: bool,
    /// An absent member opens its low record.
    pub record: RingTrackingRecordView,
    pub path: RingRecordPathView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RingIndexedTrackingRecordView {
    pub record: RingTrackingRecordView,
    pub path: RingRecordPathView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetTrackingOpeningsRequest {
    /// A `TrackingRegistry` account.
    pub registry: SerializablePubkey,
    pub members: Vec<Hash>,
    pub expected_root: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetTrackingOpeningsResponse {
    pub context: Context,
    pub root: StorageRootView,
    /// In request order.
    pub openings: Vec<RingTrackingOpening>,
}

/// Leaf 0 is the sentinel.
pub const FIRST_TRACKING_LEAF: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetTrackingRecordsRequest {
    pub registry: SerializablePubkey,
    pub expected_root: Hash,
    pub start_index: u64,
    pub limit: u64,
}

impl GetTrackingRecordsRequest {
    /// The indexer serves a page only from an enrolled leaf no later than the root's next index.
    pub const fn starts_within(&self, next_index: u64) -> bool {
        self.start_index >= FIRST_TRACKING_LEAF && self.start_index <= next_index
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetTrackingRecordsResponse {
    pub context: Context,
    pub root: StorageRootView,
    pub records: Vec<RingIndexedTrackingRecordView>,
    pub next_leaf_index: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetTrackingRecordPoolRequest {
    pub registry: SerializablePubkey,
    pub members: Vec<Hash>,
    pub expected_root: Hash,
    pub openings: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetTrackingRecordPoolResponse {
    pub context: Context,
    pub root: StorageRootView,
    /// Distinct leaves in first use order.
    pub records: Vec<RingIndexedTrackingRecordView>,
    /// The narrowest tree level at least `openings` nodes wide.
    pub canopy: Vec<Hash>,
    /// The `records` index opening each requested member, in request order.
    pub subjects: Vec<u32>,
}

/// One entry an `ApplyEnrollments` write hashes, `TrackingEntry` field for field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RingTrackingEntryView {
    pub member: Hash,
    pub nullifier_pk: Hash,
    pub ct_hash: Hash,
    pub list_tag: Hash,
    pub viewing_hash: Hash,
    /// The viewing hash a Rotate replaces, zero on an Enroll.
    pub prior_viewing_hash: Hash,
    /// An Enroll write, a Rotate is unproven.
    pub proven: bool,
}

/// The writer's entries applied in order from the current root, as its next `ApplyEnrollments` writes them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetTrackingApplyProofInputsRequest {
    pub registry: SerializablePubkey,
    pub entries: Vec<RingTrackingEntryView>,
}

/// Paths are under the root the preceding writes reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RingTrackingWriteView {
    pub member: Hash,
    pub proven: bool,
    /// The owner holds a leaf under the running root, the write moves only its `viewing_hash`.
    pub present: bool,
    pub nullifier_pk: Hash,
    pub ct_hash: Hash,
    pub list_tag: Hash,
    pub viewing_hash: Hash,
    pub prior_viewing_hash: Hash,
    /// The append leaf after the low record relinks, leaf 0 unread when nothing inserts.
    pub slot: RingRecordPathView,
    pub low: RingTrackingRecordView,
    pub low_path: RingRecordPathView,
}

/// `chain_start` binds the registry and the revision `ApplyEnrollments` names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetTrackingApplyProofInputsResponse {
    pub context: Context,
    pub revision: u64,
    pub old_root: Hash,
    pub new_root: Hash,
    pub old_next_index: u64,
    pub new_next_index: u64,
    pub chain_start: Hash,
    pub chain_end: Hash,
    /// In request order.
    pub writes: Vec<RingTrackingWriteView>,
}
