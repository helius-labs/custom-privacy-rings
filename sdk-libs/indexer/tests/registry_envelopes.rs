//! The JSON keys and their order of every storage and tracking registry envelope.

use serde::Serialize;
use spr_indexer_api::{Context, Hash, SerializablePubkey};
use zolana_ring_indexer::{
    rpc::{
        GetStorageApplyProofInputsResponse, GetStorageMemberOpeningsRequest,
        GetStorageMemberOpeningsResponse, GetStorageMemberRecordsRequest,
        GetStorageMemberRecordsResponse, GetStorageRecordPoolRequest, GetStorageRecordPoolResponse,
        RingIndexedMemberRecordView, RingMemberOpening, RingMemberRecordView, RingRecordPathView,
        StorageRootView,
    },
    tracking::{
        GetTrackingApplyProofInputsResponse, GetTrackingOpeningsRequest,
        GetTrackingOpeningsResponse, GetTrackingRecordPoolRequest, GetTrackingRecordPoolResponse,
        GetTrackingRecordsRequest, GetTrackingRecordsResponse, RingIndexedTrackingRecordView,
        RingTrackingOpening, RingTrackingRecordView,
    },
};
use zolana_ring_policy::MintPolicy;

/// The unchanged inner types, composites ahead of the parts they hold.
const PARTS: [(&str, &str); 7] = [
    ("$CONTEXT", r#"{"blockTime":0,"slot":0}"#),
    ("$ROOT", r#"{"root":$HASH,"revision":0,"nextIndex":0}"#),
    ("$PATH", r#"{"index":0,"siblings":[]}"#),
    (
        "$MEMBER_RECORD",
        r#"{"member":$HASH,"next":$HASH,"lists":0,"mintPolicy":$POLICY}"#,
    ),
    (
        "$TRACKING_RECORD",
        r#"{"member":$HASH,"next":$HASH,"nullifierPk":$HASH,"ctHash":$HASH,"listTag":$HASH,"viewingHash":$HASH}"#,
    ),
    (
        "$POLICY",
        r#"{"threshold":0,"cap":0,"cosignAbove":0,"withdrawalThreshold":0,"limits":{"senderCap":0,"senderWindowSlots":0,"volumeCap":0,"volumeWindowSlots":0,"depositCap":0,"depositWindowSlots":0,"withdrawalCap":0,"withdrawalWindowSlots":0}}"#,
    ),
    ("$HASH", r#""11111111111111111111111111111111""#),
];

fn assert_json(value: &impl Serialize, template: &str) {
    let expected = PARTS
        .iter()
        .fold(template.to_owned(), |text, (name, part)| {
            text.replace(name, part)
        });
    assert_eq!(serde_json::to_string(value).unwrap(), expected);
}

fn root() -> StorageRootView {
    StorageRootView {
        root: Hash::default(),
        revision: 0,
        next_index: 0,
    }
}

fn path() -> RingRecordPathView {
    RingRecordPathView {
        index: 0,
        siblings: Vec::new(),
    }
}

fn member_record() -> RingMemberRecordView {
    RingMemberRecordView {
        member: Hash::default(),
        next: Hash::default(),
        lists: 0,
        mint_policy: MintPolicy::default(),
    }
}

fn tracking_record() -> RingTrackingRecordView {
    RingTrackingRecordView {
        member: Hash::default(),
        next: Hash::default(),
        nullifier_pk: Hash::default(),
        ct_hash: Hash::default(),
        list_tag: Hash::default(),
        viewing_hash: Hash::default(),
        seal: None,
    }
}

#[test]
fn openings_requests_keep_their_keys() {
    let template = r#"{"registry":$HASH,"members":[$HASH],"expectedRoot":$HASH}"#;
    let storage = GetStorageMemberOpeningsRequest {
        registry: SerializablePubkey::default(),
        members: vec![Hash::default()],
        expected_root: Hash::default(),
    };
    let tracking = GetTrackingOpeningsRequest {
        registry: SerializablePubkey::default(),
        members: vec![Hash::default()],
        expected_root: Hash::default(),
    };
    assert_json(&storage, template);
    assert_json(&tracking, template);
}

#[test]
fn openings_responses_keep_their_keys() {
    let storage = GetStorageMemberOpeningsResponse {
        context: Context::default(),
        root: root(),
        openings: vec![RingMemberOpening {
            present: true,
            record: member_record(),
            path: path(),
        }],
    };
    let tracking = GetTrackingOpeningsResponse {
        context: Context::default(),
        root: root(),
        openings: vec![RingTrackingOpening {
            present: true,
            record: tracking_record(),
            path: path(),
        }],
    };
    assert_json(
        &storage,
        r#"{"context":$CONTEXT,"root":$ROOT,"openings":[{"present":true,"record":$MEMBER_RECORD,"path":$PATH}]}"#,
    );
    assert_json(
        &tracking,
        r#"{"context":$CONTEXT,"root":$ROOT,"openings":[{"present":true,"record":$TRACKING_RECORD,"path":$PATH}]}"#,
    );
}

#[test]
fn records_requests_keep_their_keys() {
    let template = r#"{"registry":$HASH,"expectedRoot":$HASH,"startIndex":1,"limit":2}"#;
    let storage = GetStorageMemberRecordsRequest {
        registry: SerializablePubkey::default(),
        expected_root: Hash::default(),
        start_index: 1,
        limit: 2,
    };
    let tracking = GetTrackingRecordsRequest {
        registry: SerializablePubkey::default(),
        expected_root: Hash::default(),
        start_index: 1,
        limit: 2,
    };
    assert_json(&storage, template);
    assert_json(&tracking, template);
}

#[test]
fn records_responses_keep_their_keys() {
    let storage = GetStorageMemberRecordsResponse {
        context: Context::default(),
        root: root(),
        records: vec![RingIndexedMemberRecordView {
            record: member_record(),
            path: path(),
        }],
        next_leaf_index: Some(3),
    };
    let tracking = GetTrackingRecordsResponse {
        context: Context::default(),
        root: root(),
        records: vec![RingIndexedTrackingRecordView {
            record: tracking_record(),
            path: path(),
        }],
        next_leaf_index: Some(3),
    };
    assert_json(
        &storage,
        r#"{"context":$CONTEXT,"root":$ROOT,"records":[{"record":$MEMBER_RECORD,"path":$PATH}],"nextLeafIndex":3}"#,
    );
    assert_json(
        &tracking,
        r#"{"context":$CONTEXT,"root":$ROOT,"records":[{"record":$TRACKING_RECORD,"path":$PATH}],"nextLeafIndex":3}"#,
    );
}

#[test]
fn record_pool_requests_keep_their_keys() {
    let template = r#"{"registry":$HASH,"members":[$HASH],"expectedRoot":$HASH,"openings":4}"#;
    let storage = GetStorageRecordPoolRequest {
        registry: SerializablePubkey::default(),
        members: vec![Hash::default()],
        expected_root: Hash::default(),
        openings: 4,
    };
    let tracking = GetTrackingRecordPoolRequest {
        registry: SerializablePubkey::default(),
        members: vec![Hash::default()],
        expected_root: Hash::default(),
        openings: 4,
    };
    assert_json(&storage, template);
    assert_json(&tracking, template);
}

#[test]
fn record_pool_responses_keep_their_keys() {
    let storage = GetStorageRecordPoolResponse {
        context: Context::default(),
        root: root(),
        records: vec![RingIndexedMemberRecordView {
            record: member_record(),
            path: path(),
        }],
        canopy: vec![Hash::default()],
        subjects: vec![5],
    };
    let tracking = GetTrackingRecordPoolResponse {
        context: Context::default(),
        root: root(),
        records: vec![RingIndexedTrackingRecordView {
            record: tracking_record(),
            path: path(),
        }],
        canopy: vec![Hash::default()],
        subjects: vec![5],
    };
    assert_json(
        &storage,
        r#"{"context":$CONTEXT,"root":$ROOT,"records":[{"record":$MEMBER_RECORD,"path":$PATH}],"canopy":[$HASH],"subjects":[5]}"#,
    );
    assert_json(
        &tracking,
        r#"{"context":$CONTEXT,"root":$ROOT,"records":[{"record":$TRACKING_RECORD,"path":$PATH}],"canopy":[$HASH],"subjects":[5]}"#,
    );
}

#[test]
fn apply_proof_inputs_responses_keep_their_keys() {
    let template = r#"{"context":$CONTEXT,"revision":6,"oldRoot":$HASH,"newRoot":$HASH,"oldNextIndex":7,"newNextIndex":8,"chainStart":$HASH,"chainEnd":$HASH,"writes":[]}"#;
    let storage = GetStorageApplyProofInputsResponse {
        context: Context::default(),
        revision: 6,
        old_root: Hash::default(),
        new_root: Hash::default(),
        old_next_index: 7,
        new_next_index: 8,
        chain_start: Hash::default(),
        chain_end: Hash::default(),
        writes: Vec::new(),
    };
    let tracking = GetTrackingApplyProofInputsResponse {
        context: Context::default(),
        revision: 6,
        old_root: Hash::default(),
        new_root: Hash::default(),
        old_next_index: 7,
        new_next_index: 8,
        chain_start: Hash::default(),
        chain_end: Hash::default(),
        writes: Vec::new(),
    };
    assert_json(&storage, template);
    assert_json(&tracking, template);
}
