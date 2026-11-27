use async_trait::async_trait;
use custom_ring_interface::{storage::Revision, RingMode};
use spr_client::AsyncRpc;
use spr_indexer_api::Hash;
use zolana_ring_indexer::{
    rpc::{
        GetStorageMemberOpeningsRequest, GetStorageRecordPoolRequest, GetStorageRecordPoolResponse,
        RingRecordPathView,
    },
    tracking::{
        GetTrackingOpeningsRequest, GetTrackingOpeningsResponse, GetTrackingRecordPoolRequest,
        GetTrackingRecordPoolResponse,
    },
};
use zolana_ring_member_registry::TrackingOpening;

use crate::{
    records::RecordPoolCheck,
    registry_records::{TrackingMemberRead, TrackingPoolRead, TrackingRead},
    submission::{BoundRoot, RootTree},
    tests::support::{
        keypair, owner_identity, owner_record, Chain, Registry, RingSettings, RECORDS, REVISION,
        TRACKING,
    },
    AsyncRingIndexer, AuditError, ResolveError, RingError, RingIndexerError, StorageTip,
    TrackingLeafCheck,
};

const OTHER_ROOT: [u8; 32] = [1; 32];
/// Below every member, the record brackets the member it replaces.
const FORGED_LOW: Hash = Hash([0; 32]);

fn chain() -> Chain {
    Chain::ring(RingSettings::new(RingMode::Confidential))
}

fn storage_members() -> Vec<[u8; 32]> {
    (1..=2)
        .map(|seed| *owner_record(&keypair(seed)).as_bytes())
        .collect()
}

fn tracking_members() -> Vec<[u8; 32]> {
    (1..=2)
        .map(|seed| *owner_identity(&keypair(seed)).as_bytes())
        .collect()
}

fn hashes(members: &[[u8; 32]]) -> Vec<Hash> {
    members.iter().copied().map(Hash).collect()
}

fn cut_level(path: &mut RingRecordPathView) {
    path.siblings.pop();
}

/// Serves the tracking answers a test forged.
#[derive(Default)]
struct Served {
    pool: Option<GetTrackingRecordPoolResponse>,
    openings: Option<GetTrackingOpeningsResponse>,
}

#[async_trait]
impl AsyncRpc for Served {}

#[async_trait]
impl AsyncRingIndexer for Served {
    async fn get_tracking_record_pool(
        &self,
        _request: GetTrackingRecordPoolRequest,
    ) -> Result<GetTrackingRecordPoolResponse, RingIndexerError> {
        Ok(self.pool.clone().expect("a pool read"))
    }

    async fn get_tracking_openings(
        &self,
        _request: GetTrackingOpeningsRequest,
    ) -> Result<GetTrackingOpeningsResponse, RingIndexerError> {
        Ok(self.openings.clone().expect("an openings read"))
    }
}

fn tracking_root(root: [u8; 32]) -> BoundRoot {
    BoundRoot {
        registry: TRACKING,
        revision: Revision::from(REVISION),
        root,
        tree: RootTree::Tracking,
    }
}

async fn read_pool(
    response: &GetTrackingRecordPoolResponse,
    root: [u8; 32],
    members: &[[u8; 32]],
) -> Result<TrackingPoolRead, RingError> {
    let served = Served {
        pool: Some(response.clone()),
        ..Served::default()
    };
    TrackingRead {
        root: tracking_root(root),
        members,
        openings: 2,
    }
    .read(&served)
    .await
}

async fn read_opening(
    response: &GetTrackingOpeningsResponse,
    root: [u8; 32],
    member: [u8; 32],
) -> Result<TrackingOpening, RingError> {
    let served = Served {
        openings: Some(response.clone()),
        ..Served::default()
    };
    TrackingMemberRead {
        root: tracking_root(root),
        member,
    }
    .read(&served)
    .await
}

#[tokio::test]
async fn a_record_pool_off_its_root_canopy_or_subjects_or_with_a_forged_absence_is_refused() {
    let registry = Registry::new(&[(&keypair(1), 1), (&keypair(2), 1)]);
    let root = registry.root();
    let chain = chain().with_records(registry);
    let members = storage_members();
    let pool = |openings| {
        chain.get_storage_record_pool(GetStorageRecordPoolRequest {
            registry: RECORDS.into(),
            members: hashes(&members),
            expected_root: Hash(root),
            openings,
        })
    };
    let check = |response: &GetStorageRecordPoolResponse, root: &[u8; 32], members: &[[u8; 32]]| {
        RecordPoolCheck {
            response,
            root,
            members,
            openings: 2,
        }
        .verify()
    };
    let honest = pool(2).await.unwrap();
    assert!(check(&honest, &root, &members).is_ok());

    let mut named = honest.clone();
    named.root.root = Hash(OTHER_ROOT);
    let mut forged = honest.clone();
    forged.records[honest.subjects[0] as usize].record.member = FORGED_LOW;
    let wide = pool(5).await.unwrap();
    for (response, root, members) in [
        (&named, &root, &members[..]),
        (&named, &OTHER_ROOT, &members[..]),
        (&wide, &root, &members[..]),
        (&honest, &root, &members[..1]),
        (&forged, &root, &members[..]),
    ] {
        assert!(matches!(
            check(response, root, members),
            Err(AuditError::InvalidMemberRegistry)
        ));
    }
}

#[tokio::test]
async fn member_openings_off_the_root_out_of_order_or_with_a_forged_absence_are_refused() {
    let registry = Registry::new(&[(&keypair(1), 1), (&keypair(2), 1)]);
    let root = registry.root();
    let chain = chain().with_records(registry);
    let members = storage_members();
    let tip = StorageTip {
        registry: RECORDS,
        root,
        revision: REVISION,
    };
    let honest = chain
        .get_storage_member_openings(GetStorageMemberOpeningsRequest {
            registry: RECORDS.into(),
            members: hashes(&members),
            expected_root: Hash(root),
        })
        .await
        .unwrap();
    assert_eq!(tip.verify_openings(&honest, &members).unwrap().len(), 2);

    let mut short = honest.clone();
    short.openings.pop();
    let mut swapped = honest.clone();
    swapped.openings.swap(0, 1);
    let mut forged = honest.clone();
    forged.openings[0].present = false;
    forged.openings[0].record.member = FORGED_LOW;
    let mut malformed = honest.clone();
    cut_level(&mut malformed.openings[0].path);
    let elsewhere = StorageTip {
        root: OTHER_ROOT,
        ..tip
    };
    for (tip, response) in [
        (&tip, &short),
        (&tip, &swapped),
        (&tip, &forged),
        (&tip, &malformed),
        (&elsewhere, &honest),
    ] {
        assert!(matches!(
            tip.verify_openings(response, &members),
            Err(AuditError::InvalidMemberRegistry)
        ));
    }
}

#[tokio::test]
async fn a_tracking_pool_off_its_root_canopy_or_subjects_or_with_a_forged_absence_is_refused() {
    let chain = chain();
    let members = tracking_members();
    let pool = |openings| {
        chain.get_tracking_record_pool(GetTrackingRecordPoolRequest {
            registry: TRACKING.into(),
            members: hashes(&members),
            expected_root: Hash([0; 32]),
            openings,
        })
    };
    let honest = pool(2).await.unwrap();
    let root = honest.root.root.0;
    assert!(read_pool(&honest, root, &members).await.is_ok());

    let mut named = honest.clone();
    named.root.root = Hash(OTHER_ROOT);
    let mut forged = honest.clone();
    forged.records[honest.subjects[0] as usize].record.member = FORGED_LOW;
    let wide = pool(5).await.unwrap();
    for (response, root, members) in [
        (&named, root, &members[..]),
        (&named, OTHER_ROOT, &members[..]),
        (&wide, root, &members[..]),
        (&honest, root, &members[..1]),
        (&forged, root, &members[..]),
    ] {
        assert!(matches!(
            read_pool(response, root, members).await,
            Err(RingError::InvalidRecordOpening)
        ));
    }
}

#[tokio::test]
async fn a_tracking_opening_off_the_root_miscounted_malformed_or_forged_absent_is_refused() {
    let member = tracking_members()[0];
    let honest = chain()
        .get_tracking_openings(GetTrackingOpeningsRequest {
            registry: TRACKING.into(),
            members: vec![Hash(member)],
            expected_root: Hash([0; 32]),
        })
        .await
        .unwrap();
    let root = honest.root.root.0;
    assert!(read_opening(&honest, root, member).await.is_ok());

    let mut empty = honest.clone();
    empty.openings.clear();
    let mut doubled = honest.clone();
    doubled.openings.push(honest.openings[0].clone());
    let mut forged = honest.clone();
    forged.openings[0].present = false;
    forged.openings[0].record.member = FORGED_LOW;
    let mut malformed = honest.clone();
    cut_level(&mut malformed.openings[0].path);
    for (response, root) in [
        (&empty, root),
        (&doubled, root),
        (&forged, root),
        (&malformed, root),
        (&honest, OTHER_ROOT),
    ] {
        assert!(matches!(
            read_opening(response, root, member).await,
            Err(RingError::InvalidRecordOpening)
        ));
    }
    assert!(matches!(
        TrackingLeafCheck {
            view: &malformed.openings[0],
            root: &root,
            member: &member,
        }
        .verify(),
        Err(ResolveError::MalformedOpening)
    ));
}
