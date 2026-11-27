use serde::{Deserialize, Serialize};
use spr_indexer_api::{Base64String, Hash, RpcMethod, SerializablePubkey};

use crate::rpc::{
    RegistryApplyProofInputsResponse, RegistryOpeningsRequest, RegistryOpeningsResponse,
    RegistryRecordPoolRequest, RegistryRecordPoolResponse, RegistryRecordsRequest,
    RegistryRecordsResponse, RingIndexedRecordView, RingRecordOpening, RingRecordPathView,
};

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

pub type RingTrackingOpening = RingRecordOpening<RingTrackingRecordView>;

pub type RingIndexedTrackingRecordView = RingIndexedRecordView<RingTrackingRecordView>;

pub type GetTrackingOpeningsRequest = RegistryOpeningsRequest;

pub type GetTrackingOpeningsResponse = RegistryOpeningsResponse<RingTrackingRecordView>;

/// Leaf 0 is the sentinel.
pub const FIRST_TRACKING_LEAF: u64 = 1;

pub type GetTrackingRecordsRequest = RegistryRecordsRequest;

impl GetTrackingRecordsRequest {
    /// The indexer serves a page only from an enrolled leaf no later than the root's next index.
    pub const fn starts_within(&self, next_index: u64) -> bool {
        self.start_index >= FIRST_TRACKING_LEAF && self.start_index <= next_index
    }
}

pub type GetTrackingRecordsResponse = RegistryRecordsResponse<RingTrackingRecordView>;

pub type GetTrackingRecordPoolRequest = RegistryRecordPoolRequest;

pub type GetTrackingRecordPoolResponse = RegistryRecordPoolResponse<RingTrackingRecordView>;

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

pub type GetTrackingApplyProofInputsResponse =
    RegistryApplyProofInputsResponse<RingTrackingWriteView>;
