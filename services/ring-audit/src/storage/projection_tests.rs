use spr_keypair::NullifierKey;

use super::*;
use ring_service_core::database::Store;

use crate::{
    projection_lease::{LeaseClaim, PROJECTION_LEASE_TTL},
    projection_reads::ProjectionReads,
    spend_machine::{InputRead, Observation},
    utxos::{AuditStore, PageWork},
};

const RING: Address = Address::new_from_array([8; 32]);
const NOW: u64 = 1_000;

/// A file every replica of one test opens, so reads and writes take separate connections.
fn shared_file(name: &str) -> String {
    let directory = std::env::temp_dir().join(format!("projection-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("directory");
    format!(
        "sqlite://{}?mode=rwc",
        directory.join("ring.sqlite").display()
    )
}

fn seal() -> NoteSeal {
    NoteSeal::derive(&X25519Keypair::from_bytes(&[0x41; 32]), RING).expect("seal")
}

async fn claim(store: &Store, now: u64) -> Option<ProjectionLease> {
    LeaseClaim {
        db: store.connection(),
        ring: RING,
        holder: store.lease_holder(),
        now,
    }
    .claim()
    .await
    .expect("claim")
}

/// A page through `slot`, spending `nullifier` when one is named.
async fn page(slot: u64, nullifier: Option<[u8; 32]>) -> PageDelta {
    let book =
        AuditStore::on_internal_database(RING, &X25519Keypair::from_bytes(&[0x41; 32])).await;
    let opener = |_: &HeldEnrollment| -> Option<NullifierKey> { None };
    let mut work = PageWork::new(&book, &opener);
    if let Some(nullifier) = nullifier {
        work.observe(Observation {
            nullifier,
            frame: CreatedIn {
                slot,
                tx_signature: Signature::from([0x51; 64]),
                event_index: 0,
            },
            position: 0,
            read: InputRead::Spend,
            hit: None,
        });
    }
    work.checkpoint(
        HistoryPosition::after_slot(slot),
        Some(Context {
            slot,
            block_time: 0,
        }),
    );
    work.finish().await.expect("page")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_spend_committed_between_two_reads_shows_in_neither() {
    let store = Store::open(&shared_file("snapshot"), 2)
        .await
        .expect("store");
    let seal = seal();
    let writer = ProjectionWriter {
        db: store.connection(),
        ring: RING,
        seal: &seal,
        lease: claim(&store, NOW).await.expect("held"),
        clock: Clock::Fixed(NOW),
    };
    writer
        .commit_page(&page(5, None).await)
        .await
        .expect("first page");
    let tables = LoadSpends {
        db: store.connection(),
        ring: RING,
    };
    let spent = [[0x33; 32]];
    let rows = |db| ProjectionReads {
        db,
        ring: RING,
        seal: &seal,
    };

    let snapshot = read_snapshot(store.connection()).await.expect("snapshot");
    let first = tables.read(&snapshot).await.expect("first read");
    writer
        .commit_page(&page(9, Some([0x33; 32])).await)
        .await
        .expect("a spend between the reads");
    let second = tables.read(&snapshot).await.expect("second read");
    let unseen = rows(&snapshot).rows(&spent).await.expect("rows");
    snapshot.commit().await.expect("end");

    assert_eq!(
        second, first,
        "the spend and its checkpoint stay out together"
    );
    assert!(unseen.is_empty());
    let later = tables.load().await.expect("later load");
    let seen = ProjectionReads {
        db: store.connection(),
        ring: RING,
        seal: &seal,
    }
    .rows(&spent)
    .await
    .expect("rows");
    assert_eq!(seen.len(), 1);
    assert_eq!(later.spends.checkpoint.through.slot, 9);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_replica_is_refused_and_a_taken_lease_fences_the_first() {
    let url = shared_file("lease");
    let first = Store::open(&url, 2).await.expect("first replica");
    let second = Store::open(&url, 2).await.expect("second replica");
    let seal = seal();
    let held = claim(&first, NOW).await.expect("the first replica holds");
    assert_eq!(claim(&second, NOW + 1).await, None, "a live lease refuses");

    let expired = NOW + PROJECTION_LEASE_TTL;
    let taken = claim(&second, expired).await.expect("taken after expiry");
    assert_ne!(taken, held);
    // The stale writer's clock still reads its lease as live, the moved token refuses it.
    let stale = ProjectionWriter {
        db: first.connection(),
        ring: RING,
        seal: &seal,
        lease: held,
        clock: Clock::Fixed(NOW + 1),
    };
    assert!(stale
        .commit_page(&page(5, Some([0x33; 32])).await)
        .await
        .is_err());
    ProjectionWriter {
        db: second.connection(),
        ring: RING,
        seal: &seal,
        lease: taken,
        clock: Clock::Fixed(expired),
    }
    .commit_page(&page(7, None).await)
    .await
    .expect("the holder commits");

    let loaded = LoadSpends {
        db: first.connection(),
        ring: RING,
    }
    .load()
    .await
    .expect("load");
    let rows = ProjectionReads {
        db: first.connection(),
        ring: RING,
        seal: &seal,
    }
    .rows(&[[0x33; 32]])
    .await
    .expect("rows");
    assert!(rows.is_empty(), "the fenced page wrote nothing");
    assert_eq!(loaded.spends.checkpoint.through.slot, 7);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restarted_replica_writes_at_once_and_fences_its_old_instance() {
    let url = shared_file("restart");
    let before = Store::open(&url, 2)
        .await
        .expect("before the restart")
        .with_replica("replica-a");
    let seal = seal();
    let held = claim(&before, NOW).await.expect("held before the restart");

    let after = Store::open(&url, 2)
        .await
        .expect("after the restart")
        .with_replica("replica-a");
    let taken = claim(&after, NOW + 1)
        .await
        .expect("no wait for the expiry");
    assert_ne!(taken, held);
    ProjectionWriter {
        db: after.connection(),
        ring: RING,
        seal: &seal,
        lease: taken,
        clock: Clock::Fixed(NOW + 1),
    }
    .commit_page(&page(7, None).await)
    .await
    .expect("the restarted instance commits");
    let stale = ProjectionWriter {
        db: before.connection(),
        ring: RING,
        seal: &seal,
        lease: held,
        clock: Clock::Fixed(NOW + 2),
    };
    assert!(stale
        .commit_page(&page(9, Some([0x33; 32])).await)
        .await
        .is_err());
}

/// A stored spend link of nullifier `[0x22; 32]` to note `[0x33; 32]`.
const SEALED_LINK: [u8; 60] = [
    97, 189, 242, 144, 204, 18, 82, 194, 127, 195, 235, 184, 42, 240, 165, 53, 226, 183, 197, 160,
    227, 50, 59, 222, 100, 88, 22, 19, 187, 40, 33, 110, 121, 155, 143, 18, 108, 25, 53, 27, 40,
    158, 5, 216, 167, 154, 209, 128, 133, 95, 205, 252, 121, 5, 101, 43, 254, 72, 191, 224,
];

#[test]
fn a_sealed_row_opens_only_under_its_ring_and_row_key() {
    let refused = "Custom Error: note row does not open under the auditor key";
    let nullifier = [0x22; 32];
    let other_ring = NoteSeal::derive(
        &X25519Keypair::from_bytes(&[0x41; 32]),
        Address::new_from_array([9; 32]),
    )
    .expect("seal");

    assert_eq!(
        seal().open(&nullifier, &SEALED_LINK).expect("opens"),
        [0x33; 32]
    );
    for refusal in [
        seal().open(&[0x23; 32], &SEALED_LINK),
        other_ring.open(&nullifier, &SEALED_LINK),
        seal().open(&nullifier, &SEALED_LINK[..11]),
    ] {
        assert_eq!(refusal.expect_err("refused").to_string(), refused);
    }
}

#[test]
fn every_sealed_note_has_one_length() {
    for flags in 0..32_u8 {
        let set = |bit: u8| flags >> bit & 1 == 1;
        let note = StoredUtxo {
            utxo: KnownUtxo {
                owner_key: [1; 32],
                owner: if set(0) {
                    UtxoOwner::Keyed {
                        nullifier_pk: [2; 32],
                    }
                } else {
                    UtxoOwner::Unresolved
                },
                asset: [3; 32],
                mint: set(1).then_some(Address::new_from_array([4; 32])),
                amount: 5,
                blinding: [6; 32],
                data_hash: set(2).then_some([7; 32]),
                ring_data_hash: set(2).then_some([8; 32]),
                tree_id: 9,
            },
            created_in: CreatedIn {
                slot: 10,
                tx_signature: Signature::from([11; 64]),
                event_index: 12,
            },
            slot_index: 13,
            leaf_index: 14,
            ring_note: set(3),
            nullifier: set(4).then_some([15; 32]),
        };
        let body = note_body(&note);

        assert_eq!(note_from_body(&body).expect("opens"), note);
        assert!(note_from_body(&body[1..]).is_err());
        assert_eq!(
            seal().seal(&[0x33; 32], &body).expect("sealed").len(),
            12 + 357 + 16
        );
    }
}
