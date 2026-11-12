use std::{path::PathBuf, time::Duration};

use zolana_ring_client::rpc::{ReadTarget, RegisterEnrollmentRingRequest};

use super::*;
use crate::{
    authorize::Unauthorized,
    config::RootSecret,
    database::{Store, STORE_CONNECTIONS},
    hub::{Clock, Hub, ServiceIdentity},
    upstream::{ChainSource, Upstreams},
};

const REGISTERED: Address = Address::new_from_array([13; 32]);
const ELSEWHERE: Address = Address::new_from_array([14; 32]);
const CLUSTER: [u8; 32] = [1; 32];
const NOW: u64 = 1_700_000_000;

fn policy_key() -> Keypair {
    Keypair::new_from_array([10; 32])
}

fn ring_naming(chain: &StubChain, ring: Address, writer: Address) {
    chain.policy(
        ring,
        RingRoles {
            policy: policy_key().pubkey(),
            enrollment_writer: writer,
            ..RingRoles::zeroed()
        },
    );
}

/// `REGISTERED` names the payer as its writer, `RING` stays the named ring.
fn registered_chain() -> Arc<StubChain> {
    let chain = StubChain::new();
    ring_naming(&chain, REGISTERED, payer().pubkey());
    chain
}

async fn register(applier: &EnrollmentApplier, ring: Address) -> Result<(), RingRpcError> {
    applier.register(ring, policy_key().pubkey()).await
}

/// A fresh file each call, removed with its directory by the caller.
fn store_url(label: &str) -> (PathBuf, String) {
    let directory =
        std::env::temp_dir().join(format!("enrollment-rings-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("directory");
    let url = format!(
        "sqlite://{}?mode=rwc",
        directory.join("store.sqlite").display()
    );
    (directory, url)
}

async fn stored(chain: &Arc<StubChain>, url: &str, payer: Keypair) -> EnrollmentApplier {
    EnrollmentApplierParts {
        payer,
        store: Some(Store::open(url, STORE_CONNECTIONS).await.expect("store")),
        ..parts(chain)
    }
    .applier()
    .await
    .expect("applier")
}

#[tokio::test]
async fn the_policy_role_registers_a_ring_that_names_the_writer() {
    let chain = registered_chain();
    chain.enrolled([3]);
    let applier = applier(&chain).await;
    assert_eq!(
        refusal(applier.submit(rotation_in(REGISTERED, 3)).await),
        EnrollmentRefusal::NotServed
    );
    register(&applier, REGISTERED).await.expect("registered");
    register(&applier, REGISTERED)
        .await
        .expect("registered once");
    applier
        .submit(rotation_in(REGISTERED, 3))
        .await
        .expect("held");
    applier.flush().await;
    assert_eq!(chain.proved(), [(REGISTRY, 1)]);
    assert_eq!(chain.sent.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_full_ring_set_refuses_the_next_registration() {
    let chain = registered_chain();
    ring_naming(&chain, ELSEWHERE, payer().pubkey());
    let applier = EnrollmentApplierParts {
        max_rings: 1,
        ..parts(&chain)
    }
    .applier()
    .await
    .expect("applier");
    register(&applier, REGISTERED).await.expect("registered");
    assert_eq!(
        refusal(register(&applier, ELSEWHERE).await),
        EnrollmentRefusal::RingsFull
    );
    register(&applier, REGISTERED)
        .await
        .expect("a registered ring registers again");
}

#[tokio::test]
async fn a_signer_other_than_the_policy_role_is_refused() {
    let chain = registered_chain();
    let applier = applier(&chain).await;
    let refused = applier.register(REGISTERED, payer().pubkey()).await;
    assert!(
        matches!(
            refused,
            Err(RingRpcError::Unauthorized(Unauthorized::NotRingAuthority))
        ),
        "{refused:?}"
    );
    assert_eq!(
        refusal(applier.submit(rotation_in(REGISTERED, 3)).await),
        EnrollmentRefusal::NotServed
    );
}

#[tokio::test]
async fn a_ring_without_its_policy_is_refused() {
    let chain = registered_chain();
    let applier = applier(&chain).await;
    assert_eq!(
        refusal(register(&applier, ELSEWHERE).await),
        EnrollmentRefusal::RingMissing
    );
}

#[tokio::test]
async fn a_ring_that_names_another_writer_is_refused() {
    let chain = registered_chain();
    ring_naming(&chain, ELSEWHERE, owner(1).pubkey());
    let applier = applier(&chain).await;
    assert_eq!(
        refusal(register(&applier, ELSEWHERE).await),
        EnrollmentRefusal::RingMissing
    );
    assert_eq!(
        refusal(applier.submit(rotation_in(ELSEWHERE, 3)).await),
        EnrollmentRefusal::NotServed
    );
}

#[tokio::test]
async fn one_ring_spends_only_its_share_of_the_budget() {
    let chain = registered_chain();
    chain.enrolled([3, 4]);
    let applier = EnrollmentApplierParts {
        ring_budget_lamports: signature_fees(2),
        ..parts(&chain)
    }
    .applier()
    .await
    .expect("applier");
    register(&applier, REGISTERED).await.expect("registered");
    applier
        .submit(rotation_in(REGISTERED, 3))
        .await
        .expect("held");
    applier.flush().await;
    assert_eq!(chain.sent.lock().unwrap().len(), 1);
    assert_eq!(
        refusal(applier.submit(rotation_in(REGISTERED, 4)).await),
        EnrollmentRefusal::Budget
    );
    applier
        .submit(rotation(4))
        .await
        .expect("another ring's share");
}

#[tokio::test]
async fn a_registered_ring_survives_a_new_applier_over_the_same_store() {
    let chain = registered_chain();
    let (directory, url) = store_url("restart");
    let first = stored(&chain, &url, payer()).await;
    register(&first, REGISTERED).await.expect("registered");
    drop(first);
    let reopened = stored(&chain, &url, payer()).await;
    reopened
        .submit(rotation_in(REGISTERED, 3))
        .await
        .expect("held after a restart");
    let other_writer = stored(&chain, &url, owner(1)).await;
    assert_eq!(
        refusal(other_writer.submit(rotation_in(REGISTERED, 3)).await),
        EnrollmentRefusal::NotServed
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn a_batch_pass_drops_a_registered_ring_whose_policy_names_another_writer() {
    let chain = registered_chain();
    chain.enrolled([3]);
    let (directory, url) = store_url("moved");
    let applier = stored(&chain, &url, payer()).await;
    register(&applier, REGISTERED).await.expect("registered");
    applier
        .submit(rotation_in(REGISTERED, 3))
        .await
        .expect("held");
    ring_naming(&chain, REGISTERED, owner(1).pubkey());
    applier.flush().await;
    assert!(chain.proved().is_empty());
    assert_eq!(
        refusal(applier.submit(rotation_in(REGISTERED, 3)).await),
        EnrollmentRefusal::NotServed
    );
    let reopened = stored(&chain, &url, payer()).await;
    assert_eq!(
        refusal(reopened.submit(rotation_in(REGISTERED, 3)).await),
        EnrollmentRefusal::NotServed
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn the_hub_spends_each_nonce_once_and_binds_the_writer() {
    let chain = registered_chain();
    let service = Keypair::new_from_array([12; 32]);
    let upstreams = Upstreams {
        indexer_url: "http://127.0.0.1:1",
        rpc_url: "http://127.0.0.1:1",
        timeout: Duration::from_secs(1),
    };
    let identity = ServiceIdentity {
        genesis_hash: CLUSTER,
        service: service.insecure_clone(),
    };
    let hub = Hub::builder(ChainSource::connect(upstreams).expect("source"), identity)
        .with_clock(Clock::Fixed(NOW))
        .with_enrollment_applier(Arc::new(applier(&chain).await))
        .derived(RootSecret::from_bytes([7; 32]).expect("root"))
        .expect("hub");
    let target = ReadTarget {
        cluster: CLUSTER,
        service: service.pubkey(),
        ring: REGISTERED,
    };
    let signed = |writer: Address| {
        RegisterEnrollmentRingRequest::register(target, policy_key().pubkey(), writer)
            .at(NOW)
            .sign(&policy_key())
            .expect("signed")
    };
    let other_writer = hub
        .register_enrollment_ring(&signed(owner(1).pubkey()))
        .await;
    assert!(
        matches!(
            other_writer,
            Err(RingRpcError::Unauthorized(Unauthorized::BadSignature))
        ),
        "{other_writer:?}"
    );
    let request = signed(payer().pubkey());
    hub.register_enrollment_ring(&request)
        .await
        .expect("registered");
    let replayed = hub.register_enrollment_ring(&request).await;
    assert!(
        matches!(
            replayed,
            Err(RingRpcError::Unauthorized(Unauthorized::Replay))
        ),
        "{replayed:?}"
    );
}
