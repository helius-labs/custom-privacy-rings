use serde::{Deserialize, Serialize};
use spr_indexer_api::{Context, Hash, RpcMethod, SerializablePubkey};
use thiserror::Error;

/// Pages and opening batches include one authentication path per record.
pub const MAX_RING_MEMBER_PAGE_LIMIT: u64 = 100;

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[repr(i32)]
pub enum RingRpcError {
    #[error("the ring member registry indexer is catching up or recovering")]
    RegistryOutOfSync = -32070,
    #[error("the ring member registry root has changed")]
    RegistryRootChanged = -32071,
    #[error("the ring member registry page has an index gap or inconsistent leaf")]
    RegistryPageGap = -32075,
}

impl RingRpcError {
    pub const ALL: [Self; 3] = [
        Self::RegistryOutOfSync,
        Self::RegistryRootChanged,
        Self::RegistryPageGap,
    ];

    pub fn code(self) -> i32 {
        self as i32
    }

    pub fn from_code(code: i64) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|error| i64::from(error.code()) == code)
    }
}

pub mod method {
    use super::*;

    pub struct GetStorageMemberOpenings;
    pub struct GetStorageMemberRecords;
    pub struct GetStorageRecordPool;
    pub struct GetStorageApplyProofInputs;

    impl RpcMethod for GetStorageMemberOpenings {
        const NAME: &'static str = "getStorageMemberOpenings";
        type Request = GetStorageMemberOpeningsRequest;
        type Response = GetStorageMemberOpeningsResponse;
    }

    impl RpcMethod for GetStorageMemberRecords {
        const NAME: &'static str = "getStorageMemberRecords";
        type Request = GetStorageMemberRecordsRequest;
        type Response = GetStorageMemberRecordsResponse;
    }

    impl RpcMethod for GetStorageRecordPool {
        const NAME: &'static str = "getStorageRecordPool";
        type Request = GetStorageRecordPoolRequest;
        type Response = GetStorageRecordPoolResponse;
    }

    impl RpcMethod for GetStorageApplyProofInputs {
        const NAME: &'static str = "getStorageApplyProofInputs";
        type Request = GetStorageApplyProofInputsRequest;
        type Response = GetStorageApplyProofInputsResponse;
    }
}

/// Leaves from `start_index` under one root the registry history holds, leaf 0 the sentinel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RegistryRecordsRequest {
    pub registry: SerializablePubkey,
    pub expected_root: Hash,
    pub start_index: u64,
    /// Between 1 and [`MAX_RING_MEMBER_PAGE_LIMIT`].
    pub limit: u64,
}

pub type GetStorageMemberRecordsRequest = RegistryRecordsRequest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RingListWriteView {
    pub mint_policy: Option<zolana_ring_policy::MintPolicy>,
    pub member: Hash,
    pub mask: u64,
    /// Lies inside `mask`.
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RingMemberRecordView {
    pub member: Hash,
    pub next: Hash,
    /// Bit `i` marks list id `i + 1`.
    pub lists: u64,
    pub mint_policy: zolana_ring_policy::MintPolicy,
}

/// A leaf index and the three siblings of every level in ascending child order, leaf level first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RingRecordPathView {
    pub index: u64,
    pub siblings: Vec<[Hash; 3]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RingRecordOpening<R> {
    /// False proves the member holds no row under `root`.
    pub present: bool,
    /// Absent members open the low record, `record.member < member < record.next`.
    pub record: R,
    pub path: RingRecordPathView,
}

pub type RingMemberOpening = RingRecordOpening<RingMemberRecordView>;

/// One contiguous leaf and its authentication path at `root`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RingIndexedRecordView<R> {
    pub record: R,
    pub path: RingRecordPathView,
}

pub type RingIndexedMemberRecordView = RingIndexedRecordView<RingMemberRecordView>;

/// Paths are under the root the preceding writes reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RingRecordWriteView {
    pub member: Hash,
    pub mask: u64,
    pub value: u64,
    pub exists: bool,
    pub old_next: Hash,
    pub old_lists: u64,
    pub old_mint_policy: zolana_ring_policy::MintPolicy,
    /// The member's leaf, for an absent member the append leaf after its low record relinks.
    pub slot: RingRecordPathView,
    /// An absent member's low record, empty for a present member.
    pub low: RingMemberRecordView,
    pub low_path: RingRecordPathView,
    pub mint_policy: Option<zolana_ring_policy::MintPolicy>,
}

/// Up to [`MAX_RING_MEMBER_PAGE_LIMIT`] members under one root the registry history holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RegistryOpeningsRequest {
    /// A `StorageRegistry` or `TrackingRegistry` account.
    pub registry: SerializablePubkey,
    pub members: Vec<Hash>,
    pub expected_root: Hash,
}

pub type GetStorageMemberOpeningsRequest = RegistryOpeningsRequest;

/// Each member's record or low record under one root, a shared record opened once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RegistryRecordPoolRequest {
    pub registry: SerializablePubkey,
    pub members: Vec<Hash>,
    pub expected_root: Hash,
    /// At least `members.len()` and at most the widest pool a catalog transfer key reads.
    pub openings: u64,
}

pub type GetStorageRecordPoolRequest = RegistryRecordPoolRequest;

/// The writes one `Update` carries, applied in order from the current root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetStorageApplyProofInputsRequest {
    pub registry: SerializablePubkey,
    pub writes: Vec<StorageWriteView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StorageWriteView {
    pub member: Hash,
    pub mask: u64,
    /// Lies inside `mask`.
    pub value: u64,
    pub mint_policy: Option<zolana_ring_policy::MintPolicy>,
}

impl From<&custom_ring_interface::RecordWrite> for StorageWriteView {
    fn from(entry: &custom_ring_interface::RecordWrite) -> Self {
        Self {
            member: Hash(entry.member),
            mask: entry.mask,
            value: entry.value,
            mint_policy: entry.mint_policy,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StorageRootView {
    pub root: Hash,
    pub revision: u64,
    pub next_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RegistryOpeningsResponse<R> {
    pub context: Context,
    pub root: StorageRootView,
    /// In request order.
    pub openings: Vec<RingRecordOpening<R>>,
}

pub type GetStorageMemberOpeningsResponse = RegistryOpeningsResponse<RingMemberRecordView>;

/// A complete leaf range, an indexer gap fails the call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RegistryRecordsResponse<R> {
    pub context: Context,
    pub root: StorageRootView,
    pub records: Vec<RingIndexedRecordView<R>>,
    pub next_leaf_index: Option<u64>,
}

pub type GetStorageMemberRecordsResponse = RegistryRecordsResponse<RingMemberRecordView>;

/// Each path stops below the canopy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RegistryRecordPoolResponse<R> {
    pub context: Context,
    pub root: StorageRootView,
    /// Distinct leaves in first use order.
    pub records: Vec<RingIndexedRecordView<R>>,
    /// The narrowest tree level at least `openings` nodes wide.
    pub canopy: Vec<Hash>,
    /// The `records` index opening each requested member, in request order.
    pub subjects: Vec<u32>,
}

pub type GetStorageRecordPoolResponse = RegistryRecordPoolResponse<RingMemberRecordView>;

/// `chain_start` binds the registry and `revision`, the revision `Update` or `ApplyEnrollments` names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RegistryApplyProofInputsResponse<W> {
    pub context: Context,
    pub revision: u64,
    pub old_root: Hash,
    pub new_root: Hash,
    pub old_next_index: u64,
    pub new_next_index: u64,
    pub chain_start: Hash,
    pub chain_end: Hash,
    /// In request order.
    pub writes: Vec<W>,
}

pub type GetStorageApplyProofInputsResponse = RegistryApplyProofInputsResponse<RingRecordWriteView>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(
            RingRpcError::ALL.map(RingRpcError::code),
            [-32070, -32071, -32075]
        );
        for error in RingRpcError::ALL {
            assert_eq!(
                RingRpcError::from_code(i64::from(error.code())),
                Some(error)
            );
        }
        assert_eq!(RingRpcError::from_code(-32072), None);
        assert_eq!(RingRpcError::from_code(-32076), None);
        assert_eq!(RingRpcError::from_code(-32603), None);
    }
}
