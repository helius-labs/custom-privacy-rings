use custom_ring_interface::{
    chain_link,
    storage_instruction::EnrollIxData,
    tracking::{TrackingEntry, TRACKING_CAPACITY},
    FIELD_MAX,
};
use solana_address::Address;
use spr_hasher::primitives::right_align;
use zolana_ring_member_registry::{
    AppliedTree, ApplyProofInputs, Canopy, IndexedRecord, NodePosition, OpeningCheck,
    RecordOpening, RecordStore, RecordTree, RecordTreeError, TrackingPath, TrackingRecord,
    TrackingTree, TrackingWriteProofInput,
};

fn member(byte: u8) -> [u8; 32] {
    right_align(&[byte])
}

/// A first enrollment, its `Enroll` verified `RegisterKey`.
fn entry(byte: u8) -> TrackingEntry {
    TrackingEntry {
        member: member(byte),
        nullifier_pk: member(byte ^ 0x40),
        ct_hash: member(byte ^ 0x50),
        list_tag: member(byte ^ 0x70),
        viewing_hash: member(byte ^ 0x60),
        prior_viewing_hash: [0; 32],
        proven: true,
    }
}

/// A viewing key rotation as the program computes it, from `prior` to `viewing`, its sealed fields zero.
fn rotation(byte: u8, prior: [u8; 32], viewing: u8) -> TrackingEntry {
    TrackingEntry {
        nullifier_pk: [0; 32],
        ct_hash: [0; 32],
        list_tag: [0; 32],
        viewing_hash: member(viewing),
        prior_viewing_hash: prior,
        proven: false,
        ..entry(byte)
    }
}

/// The rotation from the key a first enrollment of `byte` inserted.
fn first_rotation(byte: u8, viewing: u8) -> TrackingEntry {
    rotation(byte, entry(byte).viewing_hash, viewing)
}

fn inserted(byte: u8, next: [u8; 32]) -> TrackingRecord {
    let entry = entry(byte);
    TrackingRecord {
        member: entry.member,
        next,
        nullifier_pk: entry.nullifier_pk,
        ct_hash: entry.ct_hash,
        list_tag: entry.list_tag,
        viewing_hash: entry.viewing_hash,
    }
}

/// Keeps the tree on a refusal, as an indexer keeps its accepted state.
fn apply(
    tree: &mut TrackingTree,
    chain_start: [u8; 32],
    entries: &[TrackingEntry],
) -> Result<ApplyProofInputs<TrackingWriteProofInput>, RecordTreeError> {
    let AppliedTree { tree: next, proof } = tree.clone().apply(chain_start, entries)?;
    *tree = next;
    Ok(proof)
}

fn presence(inputs: &ApplyProofInputs<TrackingWriteProofInput>) -> Vec<bool> {
    inputs.writes.iter().map(|write| write.present).collect()
}

fn record(tree: &TrackingTree, byte: u8) -> Option<TrackingRecord> {
    tree.record(&member(byte))
        .unwrap()
        .map(|indexed| indexed.record)
}

#[test]
fn a_proven_insert_relinks_the_low_record_and_holds_the_entry() {
    let mut tree = TrackingTree::new().unwrap();
    let first = apply(&mut tree, [1; 32], &[entry(9)]).unwrap();
    assert_eq!(
        first.chain_end,
        chain_link(&[1; 32], &entry(9).hash().unwrap()).unwrap()
    );
    let relinked = tree.root().unwrap();
    let inputs = apply(&mut tree, first.chain_end, &[entry(4)]).unwrap();
    let insert = inputs.writes[0];
    assert!(!insert.present);
    assert_eq!(insert.slot.index, 2);
    assert_eq!(insert.low.next, member(9));
    assert_eq!(
        insert.low_path.root(insert.low.hash().unwrap()),
        Ok(relinked)
    );
    assert_eq!(record(&tree, 4), Some(inserted(4, member(9))));
    assert_eq!(tree.records()[0].next, member(4));
    let statement = inputs.statement();
    assert_eq!((statement.old_next_index, statement.new_next_index), (2, 3));
    assert_eq!(statement.new_root, tree.root().unwrap());
    assert_eq!(statement.count, 1);
}

#[test]
fn a_present_owner_rotates_only_from_the_key_its_row_holds() {
    let mut tree = TrackingTree::new().unwrap();
    apply(&mut tree, [0; 32], &[entry(5)]).unwrap();
    let before = tree.root().unwrap();
    // 1. Each rotation names the key the row holds, and moves the viewing hash alone.
    for update in [first_rotation(5, 0x21), rotation(5, member(0x21), 0x22)] {
        let inputs = apply(&mut tree, [0; 32], &[update]).unwrap();
        let write = inputs.writes[0];
        assert!(write.present);
        assert_eq!(write.low.member, member(5));
        assert_eq!(
            write.low_path.root(write.low.hash().unwrap()),
            Ok(inputs.old_root)
        );
        assert_eq!(write.slot, TrackingPath::default());
        assert_eq!(inputs.new_next_index, inputs.old_next_index);
        assert_eq!(
            record(&tree, 5),
            Some(TrackingRecord {
                viewing_hash: update.viewing_hash,
                ..inserted(5, FIELD_MAX)
            })
        );
    }
    assert_ne!(tree.root().unwrap(), before);
    // 2. A replayed older rotation and a second first enrollment name a key the row does not hold.
    let rotated = tree.clone();
    for stale in [first_rotation(5, 0x21), entry(5)] {
        assert_eq!(
            apply(&mut tree, [0; 32], &[stale]),
            Err(RecordTreeError::StaleViewingKey)
        );
    }
    assert_eq!(tree, rotated);
}

#[test]
fn a_saved_rotation_fails_once_its_key_returns_and_a_wrong_seq_never_applies() {
    let owner = Address::new_from_array([0x21; 32]);
    let (a, b) = ([0x18; 32], [0x19; 32]);
    let enroll = EnrollIxData {
        nullifier_pk: member(0x40),
        list_tag: member(0x41),
        viewing_u: a,
        eph_u: [5; 32],
        ciphertext: [6; 64],
    };
    let rotate = |viewing_u, prior_viewing_u, prior_seq| {
        TrackingEntry::from_rotation(&owner, viewing_u, prior_viewing_u, prior_seq).unwrap()
    };
    let a_to_b = rotate(&b, &a, 0);
    let mut tree = TrackingTree::new().unwrap();
    apply(
        &mut tree,
        [0; 32],
        &[
            TrackingEntry::from_enroll(&owner, &enroll).unwrap(),
            a_to_b,
            rotate(&a, &b, 1),
        ],
    )
    .unwrap();
    // The leaf holds A at seq 2, the saved A to B names A at seq 0 and a guessed seq names A at seq 1.
    let held = tree.clone();
    for stale in [a_to_b, rotate(&b, &a, 1)] {
        assert_eq!(
            apply(&mut tree, [0; 32], &[stale]),
            Err(RecordTreeError::StaleViewingKey)
        );
    }
    assert_eq!(tree, held);
    apply(&mut tree, [0; 32], &[rotate(&b, &a, 2)]).unwrap();
}

#[test]
fn an_unproven_entry_of_an_absent_owner_changes_nothing_and_still_chains() {
    let mut tree = TrackingTree::new().unwrap();
    let before = tree.clone();
    let inputs = apply(&mut tree, [2; 32], &[first_rotation(6, 0x23)]).unwrap();
    let write = inputs.writes[0];
    assert!(!write.present);
    assert_eq!(write.slot, TrackingPath::default());
    assert_eq!(tree, before);
    assert_eq!(inputs.new_root, inputs.old_root);
    assert_eq!(inputs.new_next_index, inputs.old_next_index);
    assert_eq!(
        inputs.chain_end,
        chain_link(&[2; 32], &first_rotation(6, 0x23).hash().unwrap()).unwrap()
    );
}

#[test]
fn a_duplicate_in_one_batch_inserts_then_updates() {
    let mut tree = TrackingTree::new().unwrap();
    let inputs = apply(&mut tree, [0; 32], &[entry(6), first_rotation(6, 0x24)]).unwrap();
    assert_eq!(presence(&inputs), [false, true]);
    assert_eq!(inputs.writes[1].low, inserted(6, FIELD_MAX));
    assert_eq!((inputs.old_next_index, inputs.new_next_index), (1, 2));
    assert_eq!(
        record(&tree, 6).map(|record| record.viewing_hash),
        Some(member(0x24))
    );
}

#[test]
fn the_tree_refuses_entries_the_program_never_chains() {
    let mut tree = TrackingTree::new().unwrap();
    let before = tree.clone();
    for (outside, refusal) in [
        (
            TrackingEntry {
                nullifier_pk: [0; 32],
                ..entry(5)
            },
            RecordTreeError::TrackingKeyChanged,
        ),
        (
            TrackingEntry {
                member: FIELD_MAX,
                ..entry(5)
            },
            RecordTreeError::MemberOutOfRange,
        ),
    ] {
        assert_eq!(apply(&mut tree, [0; 32], &[outside]), Err(refusal));
    }
    assert_eq!(tree, before);
}

#[test]
fn openings_prove_presence_and_absence_under_the_root() {
    let mut tree = TrackingTree::new().unwrap();
    apply(&mut tree, [0; 32], &[entry(4), entry(9)]).unwrap();
    let root = tree.root().unwrap();
    let [four, seven] = [4, 7].map(member);
    let present = tree.opening(&four).unwrap();
    let verify = |opening, member| {
        OpeningCheck {
            opening,
            root: &root,
            member,
        }
        .verify()
    };
    assert_eq!(verify(&present, &four), Ok(record(&tree, 4)));
    let absent = tree.opening(&seven).unwrap();
    assert!(matches!(absent, RecordOpening::Absent { low, .. } if low.member == four));
    assert_eq!(verify(&absent, &seven), Ok(None));
    assert_eq!(verify(&absent, &four), Err(RecordTreeError::NotBracketed));
    assert_eq!(
        OpeningCheck {
            opening: &present,
            root: &[3; 32],
            member: &four,
        }
        .verify(),
        Err(RecordTreeError::RootMismatch)
    );
}

#[test]
fn a_record_pool_opens_the_tracking_tree_and_refuses_a_policy_canopy() {
    let mut tree = TrackingTree::new().unwrap();
    apply(&mut tree, [0; 32], &[entry(4), entry(9)]).unwrap();
    let root = tree.root().unwrap();
    let members = [9, 6, 4, 9].map(member);
    let pool = tree
        .record_pool(&members, Canopy::covering_tracking(4).unwrap())
        .unwrap();
    assert_eq!(pool.records.len(), 2);
    assert_eq!(pool.verify(&root), Ok(()));
    assert_eq!(pool.clone().padded(4).unwrap().verify(&root), Ok(()));
    let mut forged = pool;
    forged.records[1].viewing_hash = member(9);
    assert_eq!(forged.verify(&root), Err(RecordTreeError::RootMismatch));
    assert_eq!(
        tree.record_pool(&members, Canopy::covering(4).unwrap()),
        Err(RecordTreeError::PoolShape)
    );
}

#[test]
fn a_full_tree_refuses_an_insert_and_still_applies_an_update_and_a_no_op() {
    assert!(matches!(
        FullStore::with_present(member(10)).apply([0; 32], &[entry(7)]),
        Err(RecordTreeError::Full)
    ));
    let AppliedTree { proof, .. } = FullStore::with_present(member(10))
        .apply(
            [0; 32],
            &[first_rotation(10, 0x25), first_rotation(7, 0x26)],
        )
        .unwrap();
    assert_eq!(presence(&proof), [true, false]);
    assert_eq!(proof.new_next_index, TRACKING_CAPACITY);
}

/// `TRACKING_CAPACITY` leaves written, only what these writes read is stored.
struct FullStore {
    records: Vec<IndexedRecord<TrackingRecord>>,
}

impl FullStore {
    fn with_present(present: [u8; 32]) -> Self {
        let sentinel = TrackingRecord {
            next: present,
            ..TrackingRecord::SENTINEL
        };
        let written = TrackingRecord {
            member: present,
            ..inserted(10, FIELD_MAX)
        };
        Self {
            records: vec![
                IndexedRecord {
                    index: 0,
                    record: sentinel,
                },
                IndexedRecord {
                    index: 5,
                    record: written,
                },
            ],
        }
    }
}

impl RecordStore<TrackingRecord> for FullStore {
    type Error = RecordTreeError;

    fn next_index(&self) -> Result<u64, RecordTreeError> {
        Ok(TRACKING_CAPACITY)
    }

    fn node(&self, _at: NodePosition) -> Result<[u8; 32], RecordTreeError> {
        Ok([1; 32])
    }

    fn set_node(&mut self, _at: NodePosition, _node: [u8; 32]) -> Result<(), RecordTreeError> {
        Ok(())
    }

    fn record(
        &self,
        member: &[u8; 32],
    ) -> Result<Option<IndexedRecord<TrackingRecord>>, RecordTreeError> {
        Ok(self
            .records
            .iter()
            .find(|indexed| indexed.record.member == *member)
            .copied())
    }

    fn low_record(
        &self,
        member: &[u8; 32],
    ) -> Result<IndexedRecord<TrackingRecord>, RecordTreeError> {
        self.records
            .iter()
            .rfind(|indexed| indexed.record.member < *member)
            .copied()
            .ok_or(RecordTreeError::NotBracketed)
    }

    fn set_record(&mut self, _index: u64, _record: TrackingRecord) -> Result<(), RecordTreeError> {
        Ok(())
    }
}
