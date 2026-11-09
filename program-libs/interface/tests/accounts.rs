use bytemuck::Zeroable;
use custom_ring_interface::{
    chain_link, domain,
    owner_grant::OwnerGrant,
    owner_signature::{Ed25519Signatures, OwnerSignature},
    storage::{RegistryId, Revision, RootUpdate, StorageError, StorageRegistry},
    storage_instruction::{
        event_kind, tag, ApplyEnrollmentsIxData, ApplyEnrollmentsRequest, AssetWrite, EnrollIxData,
        EnrollmentChange, EnrollmentWrite, EnrollmentsAppliedEvent, InitializeIxData,
        InitializeTrackingIxData, OwnerGrantIxData, StorageEvent, StorageUpdateIxData,
        ENROLLMENT_BATCH,
    },
    tracking::{
        tracking_chain_start, TrackingEntry, TrackingRegistry, TrackingRootUpdate,
        TRACKING_EMPTY_ROOT, TRACKING_GRACE_SLOTS, TRACKING_ROOT_HISTORY,
    },
    ApplyRecordsIxData, CustomRingError, CustomRingProof, NewRing, PolicyUpdate, RecordWriteData,
    RingAccount, RingMode, RingPolicy, RingRole, RingRoles, SourceSlot, WriteRecordsIxData,
    MEMBER_REGISTRY_EMPTY_ROOT, N_SOURCE_SLOTS, QUAD_CAPACITY, READER_KEY_ED25519, ROOT_HISTORY,
    STORAGE_PROGRAM_ID, TAGGED_X25519_KEY_LEN, VIEWING_PUBKEY_TAG,
};
use serde_json::{json, Value};
use solana_address::Address;
use spr_hasher::{
    primitives::{hash_bytes, right_align, solana_owner_identity},
    Hasher, Poseidon,
};
use zolana_ring_policy::{ListId, ListSet, Member, Rule, RuleTable, Subject};

const PROGRAM: Address = Address::new_from_array([7; 32]);
const ENROLL_VECTOR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../test-vectors/registry/tracking_enroll.json"
);
fn key(byte: u8) -> Address {
    Address::new_from_array([byte; 32])
}

fn roles() -> RingRoles {
    RingRoles {
        admin: key(1),
        policy: key(2),
        pauser: key(3),
        cosign_admin: key(4),
        enrollment_writer: key(5),
    }
}

fn ring(records: Address) -> Box<RingPolicy> {
    NewRing {
        bump: 254,
        mode: RingMode::Confidential,
        roles: roles(),
        auditor_pubkey: [4; 33],
        created_slot: 0,
        records,
        tracking: key(9),
    }
    .account()
}

#[test]
fn decode_needs_the_owner_the_length_and_the_discriminator() {
    let ring = ring(Address::default());
    let bytes = bytemuck::bytes_of(&*ring).to_vec();
    assert_eq!(RingPolicy::decode(&PROGRAM, &PROGRAM, &bytes), Some(*ring));
    let other = Address::new_from_array([8; 32]);
    assert_eq!(RingPolicy::decode(&other, &PROGRAM, &bytes), None);
    assert_eq!(RingPolicy::decode(&PROGRAM, &PROGRAM, &bytes[1..]), None);
    for unknown in [10, 13] {
        let mut stale = bytes.clone();
        stale[0] = unknown;
        assert_eq!(RingPolicy::decode(&PROGRAM, &PROGRAM, &stale), None);
    }
}

#[test]
fn decode_refuses_values_no_setter_writes() {
    let mut mode = *ring(Address::default());
    mode.mode = 3;
    assert!(RingPolicy::decode(&PROGRAM, &PROGRAM, bytemuck::bytes_of(&mode)).is_none());
    assert_eq!(mode.mode(), Err(CustomRingError::InvalidRingMode));
}

#[test]
fn role_keys_stay_nonzero_and_apart_from_the_delegate() {
    assert_eq!(roles().check(None), Ok(()));
    assert_eq!(
        roles().check(Some(&key(2))),
        Err(CustomRingError::DuplicateRoleKey)
    );
    let mut ring = ring(Address::default());
    ring.rotate_role(RingRole::Pauser, key(1)).unwrap();
    assert_eq!(ring.check_role(RingRole::Pauser, &key(1)), Ok(()));
    assert_eq!(ring.check_role(RingRole::Admin, &key(1)), Ok(()));
    ring.rotate_role(RingRole::Pauser, key(3)).unwrap();
    ring.delegate = key(8);
    assert_eq!(
        ring.rotate_role(RingRole::Pauser, key(8)),
        Err(CustomRingError::DuplicateRoleKey)
    );
    assert_eq!(
        ring.rotate_role(RingRole::Pauser, Address::default()),
        Err(CustomRingError::ZeroRoleKey)
    );
    assert_eq!(ring.roles, roles());
    ring.rotate_role(RingRole::Pauser, key(9)).unwrap();
    assert_eq!(ring.roles.key(RingRole::Pauser), &key(9));
    assert_eq!(ring.check_role(RingRole::Policy, &key(2)), Ok(()));
    assert_eq!(
        ring.check_role(RingRole::Pauser, &key(2)),
        Err(CustomRingError::UnauthorizedAuthority)
    );
}

#[test]
fn a_policy_write_validates_and_repins_the_table() {
    let mut policy = ring(Address::default());
    assert_eq!(policy.policy_hash(), Err(CustomRingError::PolicyNotSet));
    let mut sources = [SourceSlot::zeroed(); N_SOURCE_SLOTS];
    let table = RuleTable::builder()
        .rule(Rule::require(Subject::OutputOwner, ListId::Allow))
        .build()
        .encode();
    let update = |sources, rules, slot| PolicyUpdate {
        sources,
        rules,
        slot,
    };
    assert_eq!(
        policy.set_policy(update(sources, table, 3)),
        Err(CustomRingError::InvalidSource)
    );
    sources[ListId::Allow.slot()].list_id = ListId::Allow as u8;
    policy.set_policy(update(sources, table, 3)).unwrap();
    let hash = *policy.policy_hash().unwrap();
    assert_eq!(hash, table.hash(&policy.source_map().unwrap()).unwrap());
    assert_eq!((policy.generation(), policy.generation_slot()), (1, 3));
    let mut padded = table;
    padded.rules[1][0] = 1;
    assert_eq!(
        policy.set_policy(update(sources, padded, 4)),
        Err(CustomRingError::InvalidPolicyRules)
    );
    assert_eq!(policy.policy_hash(), Ok(&hash));
}

#[test]
fn a_policy_write_refuses_each_table_form_and_keeps_the_pinned_table() {
    let mut sources = [SourceSlot::zeroed(); N_SOURCE_SLOTS];
    for list_id in [ListId::Allow, ListId::Block, ListId::Frozen] {
        sources[list_id.slot()].list_id = list_id as u8;
    }
    let write = |rule: Rule, slot| PolicyUpdate {
        sources,
        rules: RuleTable::builder().rule(rule).build().encode(),
        slot,
    };
    let subject = Subject::OutputOwner;
    let allow = ListSet::single(ListId::Allow);
    let block = ListSet::single(ListId::Block);
    let mut policy = ring(Address::default());
    policy
        .set_policy(PolicyUpdate {
            rules: RuleTable::builder()
                .rule(Rule::require(subject, ListId::Allow))
                .rule(Rule::forbid(subject, ListId::Block))
                .rule(Rule::forbid(subject, ListId::Frozen))
                .build()
                .encode(),
            ..write(Rule::require(subject, ListId::Allow), 1)
        })
        .unwrap();
    let pinned = *policy;
    for (rule, refusal) in [
        (
            Rule::forbid_only_on_all(subject, ListSet::of(&[ListId::Block, ListId::Frozen])),
            CustomRingError::MultiListAbsenceRule,
        ),
        (
            Rule::require(subject, ListId::Block),
            CustomRingError::InvertedListPolarity,
        ),
        (
            Rule::forbid(subject, ListId::Allow),
            CustomRingError::InvertedListPolarity,
        ),
        (
            Rule::any_of(subject, allow, block),
            CustomRingError::MixedPolarityRule,
        ),
    ] {
        assert_eq!(policy.set_policy(write(rule, 2)), Err(refusal), "{rule:?}");
        assert_eq!(*policy, pinned);
    }
}

fn enroll_ix(viewing_u: [u8; 32]) -> EnrollIxData {
    EnrollIxData {
        nullifier_pk: [0x14; 32],
        list_tag: [0x15; 32],
        viewing_u,
        eph_u: [0x16; 32],
        ciphertext: [0x17; 64],
    }
}

#[test]
fn the_entry_binds_every_field_and_comes_from_the_enroll() {
    let (owner, ix) = (key(0x12), enroll_ix([0x18; 32]));
    let entry = TrackingEntry::from_enroll(&owner, &ix).unwrap();
    assert_eq!(
        entry.member,
        solana_owner_identity(owner.as_array()).unwrap()
    );
    assert_eq!(entry.ct_hash, hash_bytes(&ix.ciphertext).unwrap());
    assert_eq!(entry.viewing_hash, hash_bytes(&ix.viewing_u).unwrap());
    assert_eq!(
        entry.hash().unwrap(),
        Poseidon::hashv(&[
            &domain::field(domain::TRACKING_ENTRY),
            &entry.member,
            &entry.nullifier_pk,
            &entry.ct_hash,
            &entry.list_tag,
            &entry.viewing_hash,
            &right_align(&[1]),
        ])
        .unwrap()
    );
    let moved = [
        TrackingEntry {
            member: [9; 32],
            ..entry
        },
        TrackingEntry {
            nullifier_pk: [9; 32],
            ..entry
        },
        TrackingEntry {
            ct_hash: [9; 32],
            ..entry
        },
        TrackingEntry {
            list_tag: [9; 32],
            ..entry
        },
        TrackingEntry {
            viewing_hash: [9; 32],
            ..entry
        },
        // A rotation hashes apart from the first enrollment it shares every field with.
        TrackingEntry {
            proven: false,
            ..entry
        },
    ];
    for other in moved {
        assert_ne!(other.hash(), entry.hash());
    }
    // A rotation carries only its viewing key, unproven.
    assert_eq!(
        TrackingEntry::from_rotation(&owner, &ix.viewing_u),
        Ok(TrackingEntry {
            nullifier_pk: [0; 32],
            ct_hash: [0; 32],
            list_tag: [0; 32],
            proven: false,
            ..entry
        })
    );
}

fn batch<'a>(
    ix: &'a EnrollIxData,
    proof: &'a CustomRingProof,
    owners: &'a [Address; 2],
) -> [EnrollmentWrite<'a>; 2] {
    [
        EnrollmentWrite {
            owner: &owners[0],
            change: EnrollmentChange::Enroll { ix, proof },
        },
        EnrollmentWrite {
            owner: &owners[1],
            change: EnrollmentChange::Rotate {
                viewing_u: &ix.viewing_u,
            },
        },
    ]
}

#[test]
fn a_request_parses_exactly_its_count_of_writes() {
    let (ix, proof, owners) = (
        enroll_ix([9; 32]),
        CustomRingProof::zeroed(),
        [key(1), key(2)],
    );
    let header = ApplyEnrollmentsIxData {
        count: 2,
        ..ApplyEnrollmentsIxData::zeroed()
    };
    let request = ApplyEnrollmentsRequest {
        ix: &header,
        writes: batch(&ix, &proof, &owners).to_vec(),
    };
    let data = request.encode();
    assert_eq!(data.len(), ApplyEnrollmentsIxData::SIZE + 417 + 65);
    assert_eq!(ApplyEnrollmentsRequest::parse(&data), Ok(request));
    let refused = Err(StorageError::InvalidInstructionData);
    let edited = |edit: &dyn Fn(&mut Vec<u8>)| {
        let mut data = data.clone();
        edit(&mut data);
        ApplyEnrollmentsRequest::parse(&data).map(drop)
    };
    assert_eq!(edited(&|data| data.push(0)), refused);
    assert_eq!(edited(&|data| data.truncate(data.len() - 1)), refused);
    // The first write's kind byte follows the header and its owner.
    assert_eq!(
        edited(&|data| data[ApplyEnrollmentsIxData::SIZE + 32] = 2),
        refused
    );
    assert_eq!(
        edited(&|data| data[ApplyEnrollmentsIxData::SIZE - 1] = 0),
        Err(StorageError::NoEnrollmentsNamed)
    );
    assert_eq!(
        edited(&|data| data[ApplyEnrollmentsIxData::SIZE - 1] = ENROLLMENT_BATCH as u8 + 1),
        Err(StorageError::BatchTooLarge)
    );
}

#[test]
fn a_signed_message_names_the_registry_the_owner_and_the_entry() {
    let (ix, proof, owners) = (
        enroll_ix([9; 32]),
        CustomRingProof::zeroed(),
        [key(1), key(2)],
    );
    let [enroll, rotate] = batch(&ix, &proof, &owners);
    let registry = key(0x11);
    let signed = enroll.signed(&registry).unwrap();
    assert_eq!(
        signed.entry_hash,
        TrackingEntry::from_enroll(&owners[0], &ix)
            .unwrap()
            .hash()
            .unwrap()
    );
    assert_eq!(
        signed.message.to_vec(),
        [
            &domain::ENROLLMENT_SIGNATURE_LABEL[..],
            registry.as_ref(),
            owners[0].as_ref(),
            &signed.entry_hash,
        ]
        .concat()
    );
    // Another registry or another write gives another message.
    assert_ne!(enroll.signed(&key(0x12)).unwrap().message, signed.message);
    assert_ne!(rotate.signed(&registry).unwrap().message, signed.message);
}

#[test]
fn a_signature_matches_only_its_own_owner_and_message_inside_the_precompile() {
    let owners = [key(1), key(2)];
    let messages = [[1u8; 105], [2u8; 105]];
    let signature = [7u8; 64];
    let signatures: Vec<_> = owners
        .iter()
        .zip(&messages)
        .map(|(owner, message)| OwnerSignature {
            owner,
            signature: &signature,
            message,
        })
        .collect();
    let data = Ed25519Signatures::encode(&signatures).unwrap();
    let parsed = Ed25519Signatures::parse(&data, 2).unwrap();
    assert_eq!(parsed.check(0, &owners[0], &messages[0]), Ok(()));
    assert_eq!(parsed.check(1, &owners[1], &messages[1]), Ok(()));
    let mismatch = Err(StorageError::OwnerSignatureMismatch);
    assert_eq!(parsed.check(0, &owners[1], &messages[0]), mismatch);
    assert_eq!(parsed.check(0, &owners[0], &messages[1]), mismatch);
    assert_eq!(parsed.check(2, &owners[0], &messages[0]), mismatch);
    assert_eq!(
        Ed25519Signatures::parse(&data, 1),
        Err(StorageError::InvalidSignatureInstruction)
    );
    // A key read from another instruction escapes the byte comparison, the descriptor refuses it.
    let mut foreign = data.clone();
    foreign[2 + 6] = 0;
    assert_eq!(
        Ed25519Signatures::parse(&foreign, 2)
            .unwrap()
            .check(0, &owners[0], &messages[0]),
        mismatch
    );
}

#[test]
fn an_event_decodes_only_under_its_tag_kind_and_exact_writes() {
    let (ix, proof, owners) = (
        enroll_ix([9; 32]),
        CustomRingProof::zeroed(),
        [key(1), key(2)],
    );
    let applied = StorageEvent::EnrollmentsApplied {
        header: EnrollmentsAppliedEvent {
            registry: key(0x11),
            revision: Revision::from(4),
            new_root: [5; 32],
            new_next_index: 6u64.to_le_bytes(),
            count: 2,
        },
        writes: batch(&ix, &proof, &owners).to_vec(),
    };
    let data = applied.encode();
    assert_eq!(
        data[..2],
        [tag::EMIT_EVENT, event_kind::ENROLLMENTS_APPLIED]
    );
    assert_eq!(StorageEvent::decode(&data), Some(applied));
    let decodes = |edit: fn(&mut Vec<u8>)| {
        let mut data = data.clone();
        edit(&mut data);
        StorageEvent::decode(&data).is_some()
    };
    assert!(!decodes(|data| data[0] = tag::APPLY_ENROLLMENTS));
    assert!(!decodes(|data| data[1] = 2));
    assert!(!decodes(|data| data.push(0)));
    assert!(!decodes(|data| data.truncate(data.len() - 1)));
}

#[test]
fn an_apply_names_one_to_a_full_batch_of_writes() {
    assert_eq!(
        ApplyEnrollmentsIxData::check_count(0),
        Err(StorageError::NoEnrollmentsNamed)
    );
    for count in 1..=ENROLLMENT_BATCH as u8 {
        assert_eq!(ApplyEnrollmentsIxData::check_count(count), Ok(()));
    }
    assert_eq!(
        ApplyEnrollmentsIxData::check_count(ENROLLMENT_BATCH as u8 + 1),
        Err(StorageError::BatchTooLarge)
    );
}

#[test]
fn the_enrollment_checks_what_no_proof_binds_and_refuses_a_zero_key() {
    let check = |viewing_u| enroll_ix(viewing_u).check();
    assert_eq!(check([9; 32]), Ok(()));
    assert_eq!(check([0; 32]), Err(StorageError::NonCanonicalViewingKey));
    assert_eq!(check([0xff; 32]), Err(StorageError::NonCanonicalViewingKey));
    assert_eq!(
        EnrollIxData {
            nullifier_pk: spr_interface::ZERO_NULLIFIER_PUBKEY,
            ..enroll_ix([9; 32])
        }
        .check(),
        Err(StorageError::ZeroNullifierKey)
    );
    for not_canonical in [
        EnrollIxData {
            nullifier_pk: [0xff; 32],
            ..enroll_ix([9; 32])
        },
        EnrollIxData {
            list_tag: [0xff; 32],
            ..enroll_ix([9; 32])
        },
    ] {
        assert_eq!(
            not_canonical.check(),
            Err(StorageError::InvalidInstructionData)
        );
    }
}

fn tracking_ix() -> InitializeTrackingIxData {
    let mut auditor_pubkey = [9; TAGGED_X25519_KEY_LEN];
    auditor_pubkey[0] = VIEWING_PUBKEY_TAG;
    InitializeTrackingIxData {
        ring_id: [1; 32],
        auditor_pubkey,
    }
}

#[test]
fn a_tracking_registry_starts_at_the_sentinel_and_pins_ring_and_auditor() {
    let ix = tracking_ix();
    assert_eq!(ix.check(), Ok(()));
    let registry = TrackingRegistry::new(key(0x30), &ix);
    assert_eq!(registry.ring_id, ix.ring_id);
    assert_eq!(registry.auditor_pubkey, ix.auditor_pubkey);
    assert_eq!(registry.newest().hash, TRACKING_EMPTY_ROOT);
    assert_eq!(registry.newest().revision, Revision::from(0));
    assert_eq!(registry.next_index(), 1);
    let bytes = bytemuck::bytes_of(&registry);
    assert_eq!(
        TrackingRegistry::decode(&STORAGE_PROGRAM_ID, bytes),
        Some(registry)
    );
    assert_eq!(TrackingRegistry::decode(&key(1), bytes), None);
    for (ring_id, auditor_pubkey, refusal) in [
        (
            [0; 32],
            ix.auditor_pubkey,
            StorageError::InvalidTrackingRegistry,
        ),
        (
            [0xff; 32],
            ix.auditor_pubkey,
            StorageError::InvalidTrackingRegistry,
        ),
        (
            ix.ring_id,
            [9; TAGGED_X25519_KEY_LEN],
            StorageError::InvalidAuditorKey,
        ),
    ] {
        assert_eq!(
            InitializeTrackingIxData {
                ring_id,
                auditor_pubkey
            }
            .check(),
            Err(refusal)
        );
    }
    let mut low_order = ix;
    low_order.auditor_pubkey[1..].fill(0);
    assert_eq!(low_order.check(), Err(StorageError::InvalidAuditorKey));
    low_order.auditor_pubkey[1] = 1;
    assert_eq!(low_order.check(), Err(StorageError::InvalidAuditorKey));
}

fn tracking_update(revision: u64, slot: u64, byte: u8) -> TrackingRootUpdate {
    TrackingRootUpdate {
        revision: revision.into(),
        slot,
        new_root: [byte; 32],
        new_next_index: 1,
    }
}

#[test]
fn a_tracking_update_serves_the_old_root_for_its_grace() {
    let mut registry = TrackingRegistry::new(key(0x30), &tracking_ix());
    registry.update(tracking_update(0, 10, 1)).unwrap();
    let first_root = TRACKING_EMPTY_ROOT;
    assert_eq!(registry.root(0.into(), 10), Ok(first_root));
    let lapse = 10 + TRACKING_GRACE_SLOTS;
    assert_eq!(registry.root(0.into(), lapse - 1), Ok(first_root));
    assert_eq!(
        registry.root(0.into(), lapse),
        Err(StorageError::RootExpired)
    );
    assert_eq!(
        registry.update(tracking_update(0, 11, 2)),
        Err(StorageError::RevisionChanged)
    );
}

/// The update that would evict the oldest unexpired root refuses and writes nothing, the same update applies once it lapses.
#[test]
fn a_tracking_update_waits_while_its_eviction_would_drop_an_unexpired_root() {
    let mut filled = TrackingRegistry::new(key(0x30), &tracking_ix());
    let full = TRACKING_ROOT_HISTORY as u64 - 1;
    for revision in 0..full {
        filled.update(tracking_update(revision, 10, 3)).unwrap();
    }
    assert_eq!(filled.root(0.into(), 11), Ok(TRACKING_EMPTY_ROOT));
    let before = filled;
    let lapse = 10 + TRACKING_GRACE_SLOTS;
    assert_eq!(
        filled.update(tracking_update(full, lapse - 1, 4)),
        Err(StorageError::RootHistoryRetained)
    );
    assert_eq!(filled, before);
    filled.update(tracking_update(full, lapse, 4)).unwrap();
    assert_eq!(filled.newest().revision, Revision::from(full + 1));
}

#[test]
fn a_tracking_tree_stays_valid_at_capacity_and_not_past_it() {
    let mut registry = TrackingRegistry::new(key(0x30), &tracking_ix());
    registry.next_index = QUAD_CAPACITY.to_le_bytes();
    assert!(TrackingRegistry::from_data(bytemuck::bytes_of(&registry)).is_some());
    registry.next_index = (QUAD_CAPACITY + 1).to_le_bytes();
    assert!(TrackingRegistry::from_data(bytemuck::bytes_of(&registry)).is_none());
}

#[test]
fn an_apply_fills_the_last_leaf_and_refuses_an_index_past_it() {
    let mut registry = TrackingRegistry::new(key(0x30), &tracking_ix());
    let past = TrackingRootUpdate {
        new_next_index: QUAD_CAPACITY + 1,
        ..tracking_update(0, 10, 1)
    };
    let before = registry;
    assert_eq!(registry.update(past), Err(StorageError::TrackingTreeFull));
    assert_eq!(registry, before);
    registry
        .update(TrackingRootUpdate {
            new_next_index: QUAD_CAPACITY,
            ..tracking_update(0, 10, 1)
        })
        .unwrap();
    assert_eq!(registry.next_index(), QUAD_CAPACITY);
}

#[test]
fn the_grant_scopes_owner_and_reader_and_checks_its_scheme() {
    let reader = {
        let mut reader = [7; 34];
        reader[0] = READER_KEY_ED25519;
        reader
    };
    let (one, _) = OwnerGrant::find_address(&key(1), &key(2), &reader);
    let mut other_reader = reader;
    other_reader[33] = 8;
    for other in [
        OwnerGrant::find_address(&key(1), &key(3), &reader).0,
        OwnerGrant::find_address(&key(9), &key(2), &reader).0,
        OwnerGrant::find_address(&key(1), &key(2), &other_reader).0,
    ] {
        assert_ne!(one, other);
    }
    let ix = OwnerGrantIxData { reader, active: 1 };
    assert_eq!(ix.check(), Ok(()));
    let mut unknown = reader;
    unknown[0] = 9;
    assert_eq!(
        OwnerGrantIxData {
            reader: unknown,
            active: 1
        }
        .check(),
        Err(StorageError::InvalidReaderKey)
    );
    assert_eq!(
        OwnerGrantIxData { reader, active: 2 }.check(),
        Err(StorageError::InvalidInstructionData)
    );
}

fn apply_ix() -> ApplyRecordsIxData {
    ApplyRecordsIxData {
        new_root: [8; 32],
        new_next_index: 1u64.to_le_bytes(),
        ..ApplyRecordsIxData::zeroed()
    }
}

fn restrictive_apply_ix() -> ApplyRecordsIxData {
    ApplyRecordsIxData {
        restrictive: 1,
        ..apply_ix()
    }
}

fn list_write(list: ListId) -> RecordWriteData {
    RecordWriteData {
        member: [1; 32],
        lists: WriteRecordsIxData {
            mask: list.record_bit().to_le_bytes(),
            value: list.record_bit().to_le_bytes(),
        },
        mint_policy: custom_ring_interface::mint_policy_write(None),
    }
}

fn update_of<'a>(
    revision: u64,
    writes: Vec<RecordWriteData>,
    apply: &'a ApplyRecordsIxData,
) -> StorageUpdateIxData<'a> {
    let private = writes.iter().any(|write| {
        u64::from_le_bytes(write.lists.mask) & ListSet::PRIVATE_LISTS.record_bits() != 0
    });
    StorageUpdateIxData {
        revision: revision.into(),
        writes,
        asset_writes: Vec::new(),
        tracking_revision: private.then_some(Revision::from(0)),
        apply,
    }
}

fn update_data<'a>(
    revision: u64,
    list: ListId,
    apply: &'a ApplyRecordsIxData,
) -> StorageUpdateIxData<'a> {
    update_of(revision, vec![list_write(list)], apply)
}

fn public_registry(grace_slots: u64) -> StorageRegistry {
    StorageRegistry::new(
        key(1),
        &InitializeIxData {
            registry_id: RegistryId([1; 32]),
            grace_slots: grace_slots.to_le_bytes(),
            ring_id: [0; 32],
            tracking_registry: Address::default(),
        },
    )
}

fn pinned_registry(ring_id: [u8; 32]) -> StorageRegistry {
    StorageRegistry::new(
        key(1),
        &InitializeIxData {
            registry_id: RegistryId([1; 32]),
            grace_slots: 10u64.to_le_bytes(),
            ring_id,
            tracking_registry: key(5),
        },
    )
}

#[test]
fn an_allow_list_addition_keeps_grace_and_a_block_list_addition_ends_every_older_root() {
    let (permissive, restrictive) = (apply_ix(), restrictive_apply_ix());
    let mut registry = public_registry(10);
    registry
        .update(RootUpdate {
            slot: 1,
            data: &update_data(0, ListId::Allow, &permissive),
        })
        .unwrap();
    assert!(registry.root(0.into(), 1).is_ok());
    assert!(registry.root(0.into(), 10).is_ok());
    assert_eq!(registry.root(0.into(), 11), Err(StorageError::RootExpired));
    registry
        .update(RootUpdate {
            slot: 1,
            data: &update_data(1, ListId::Block, &restrictive),
        })
        .unwrap();
    assert_eq!(registry.root(1.into(), 1), Err(StorageError::RootExpired));
    assert_eq!(registry.root(0.into(), 1), Err(StorageError::RootExpired));
    assert!(registry.root(2.into(), 1).is_ok());
}

#[test]
fn a_permissive_update_over_a_full_history_evicts_the_oldest_root() {
    let apply = apply_ix();
    let mut registry = public_registry(u64::MAX);
    let newest = ROOT_HISTORY as u64;
    for revision in 0..newest {
        registry
            .update(RootUpdate {
                slot: 1,
                data: &update_data(revision, ListId::Allow, &apply),
            })
            .unwrap();
    }
    assert_eq!(registry.root(0.into(), 2), Err(StorageError::RootExpired));
    assert!(registry.root(1.into(), 2).is_ok());
    assert!(registry.root(newest.into(), 2).is_ok());
    registry
        .update(RootUpdate {
            slot: 2,
            data: &update_data(newest, ListId::Block, &restrictive_apply_ix()),
        })
        .unwrap();
    assert_eq!(
        registry.root(newest.into(), 2),
        Err(StorageError::RootExpired)
    );
}

#[test]
fn update_data_round_trips_and_refuses_no_writes() {
    let apply = apply_ix();
    let data = update_data(7, ListId::Block, &apply);
    let encoded = data.encode().unwrap();
    assert_eq!(StorageUpdateIxData::parse(&encoded), Ok(data));
    let mut no_writes = encoded;
    no_writes[8] = 0;
    assert_eq!(
        StorageUpdateIxData::parse(&no_writes),
        Err(StorageError::InvalidInstructionData)
    );
}

#[test]
fn a_mint_settings_tag_above_one_is_no_entry() {
    let apply = apply_ix();
    let mut data = update_data(0, ListId::Reader, &apply);
    data.writes[0].mint_policy[0] = 2;
    assert!(data.entries().next().unwrap().is_err());
}

#[test]
fn the_restrictive_byte_has_one_encoding() {
    let apply = restrictive_apply_ix();
    let encoded = update_data(0, ListId::Block, &apply).encode().unwrap();
    assert!(StorageUpdateIxData::parse(&encoded)
        .unwrap()
        .apply
        .restrictive());
    let mut two = encoded;
    *two.last_mut().unwrap() = 2;
    assert_eq!(
        StorageUpdateIxData::parse(&two),
        Err(StorageError::InvalidInstructionData)
    );
}

#[test]
fn private_writes_require_a_tracking_pin_before_any_root_changes() {
    let apply = restrictive_apply_ix();
    let data = update_data(0, ListId::PrivateBlock, &apply);
    let mut public = public_registry(10);
    let before = public;
    assert_eq!(
        public.update(RootUpdate {
            slot: 1,
            data: &data
        }),
        Err(StorageError::PrivateWriteWithoutTracking)
    );
    assert_eq!(public, before);
    for ring_id in [[3; 32], [0; 32]] {
        let mut source = pinned_registry(ring_id);
        let before = source;
        assert_eq!(source.newest().hash, MEMBER_REGISTRY_EMPTY_ROOT);
        let stale = update_data(1, ListId::PrivateBlock, &apply);
        assert_eq!(
            source.update(RootUpdate {
                slot: 1,
                data: &stale
            }),
            Err(StorageError::RevisionChanged)
        );
        assert_eq!(source, before);
    }
}

/// Own and curator sources alike, a stale tracking root must not outlive a new private entry.
#[test]
fn private_updates_keep_the_highest_tracking_floor_and_retire_old_roots() {
    let (apply, restrictive) = (apply_ix(), restrictive_apply_ix());
    for ring_id in [[3; 32], [0; 32]] {
        let mut source = pinned_registry(ring_id);
        for (revision, tracking, floor) in [(0, 5, 5), (1, 3, 5), (2, 8, 8)] {
            let mut data = update_data(revision, ListId::PrivateBlock, &restrictive);
            data.tracking_revision = Some(Revision::from(tracking));
            source
                .update(RootUpdate {
                    slot: 1,
                    data: &data,
                })
                .unwrap();
            assert_eq!(source.tracking_floor, Revision::from(floor));
            assert_eq!(
                source.root(revision.into(), 1),
                Err(StorageError::RootExpired)
            );
        }
        source
            .update(RootUpdate {
                slot: 1,
                data: &update_data(3, ListId::Block, &restrictive),
            })
            .unwrap();
        assert_eq!(source.tracking_floor, Revision::from(8));
        source
            .update(RootUpdate {
                slot: 1,
                data: &update_data(4, ListId::Reader, &apply),
            })
            .unwrap();
        assert_eq!(source.tracking_floor, Revision::from(8));
        assert_eq!(source.root(4.into(), 1), Ok(apply.new_root));
    }
}

#[test]
fn private_masks_need_a_tracking_row_even_when_the_write_clears_the_bit() {
    let apply = restrictive_apply_ix();
    let mut data = update_data(0, ListId::PrivateBlock, &apply);
    data.writes[0].lists.value = [0; 8];
    assert_eq!(data.private_writes(), Ok(1));
    assert!(StorageUpdateIxData::parse(&data.encode().unwrap()).is_ok());
    data.tracking_revision = None;
    assert_eq!(data.encode(), Err(StorageError::InvalidTrackingRegistry));
}

#[test]
fn asset_preimages_exempt_only_the_matching_private_write() {
    let apply = apply_ix();
    let mint = key(9);
    let mut asset = list_write(ListId::PrivateBlock);
    asset.member = *Member::asset(&mint).unwrap().as_bytes();
    let mut data = update_of(0, vec![asset, list_write(ListId::PrivateBlock)], &apply);
    data.asset_writes = vec![AssetWrite {
        write_index: 0,
        mint,
    }];
    assert_eq!(data.private_writes(), Ok(0b10));
    assert!(StorageUpdateIxData::parse(&data.encode().unwrap()).is_ok());
    data.writes.pop();
    data.tracking_revision = None;
    assert_eq!(data.private_writes(), Ok(0));
    assert!(StorageUpdateIxData::parse(&data.encode().unwrap()).is_ok());
    for invalid in [
        AssetWrite {
            write_index: 1,
            mint,
        },
        AssetWrite {
            write_index: 0,
            mint: key(8),
        },
    ] {
        data.asset_writes = vec![invalid];
        assert_eq!(data.private_writes(), Err(StorageError::InvalidAssetWrite));
    }
    data.asset_writes = vec![AssetWrite {
        write_index: 0,
        mint,
    }];
    data.writes[0].lists = list_write(ListId::Block).lists;
    assert!(data.entries().all(|entry| entry.is_ok()));
    assert_eq!(data.private_writes(), Err(StorageError::InvalidAssetWrite));
}

#[test]
fn asset_write_indices_are_distinct_and_in_increasing_order() {
    let apply = apply_ix();
    let mint = key(9);
    let mut asset = list_write(ListId::PrivateBlock);
    asset.member = *Member::asset(&mint).unwrap().as_bytes();
    let mut data = update_of(0, vec![asset, asset], &apply);
    for indexes in [[0, 0], [1, 0], [0, 2]] {
        data.asset_writes = indexes
            .map(|write_index| AssetWrite { write_index, mint })
            .to_vec();
        assert_eq!(data.private_writes(), Err(StorageError::InvalidAssetWrite));
    }
    data.asset_writes = [0, 1]
        .map(|write_index| AssetWrite { write_index, mint })
        .to_vec();
    data.tracking_revision = None;
    assert_eq!(data.private_writes(), Ok(0));
    assert!(data.encode().is_ok());
}

#[test]
fn a_public_update_carries_no_tracking_revision() {
    let apply = apply_ix();
    let mut data = update_data(0, ListId::Block, &apply);
    let encoded = data.encode().unwrap();
    data.tracking_revision = Some(Revision::from(0));
    assert_eq!(data.encode(), Err(StorageError::InvalidTrackingRegistry));
    let mut extended = encoded.clone();
    extended.extend_from_slice(&[0; 8]);
    assert_eq!(
        StorageUpdateIxData::parse(&extended),
        Err(StorageError::InvalidInstructionData)
    );
    assert!(StorageUpdateIxData::parse(&encoded[..encoded.len() - 1]).is_err());
}

#[test]
fn the_parser_reads_a_tracking_revision_exactly_when_a_write_is_private() {
    let apply = restrictive_apply_ix();
    let mut data = update_data(0, ListId::PrivateBlock, &apply);
    for revision in [0, u64::from(u32::MAX) + 1, u64::MAX] {
        data.tracking_revision = Some(Revision::from(revision));
        let encoded = data.encode().unwrap();
        let revision_offset = encoded.len() - core::mem::size_of::<ApplyRecordsIxData>() - 8;
        assert_eq!(
            &encoded[revision_offset..revision_offset + 8],
            revision.to_le_bytes()
        );
        assert_eq!(StorageUpdateIxData::parse(&encoded), Ok(data.clone()));
        for missing in 1..=8 {
            let mut truncated = encoded.clone();
            truncated.drain(revision_offset..revision_offset + missing);
            assert_eq!(
                StorageUpdateIxData::parse(&truncated),
                Err(StorageError::InvalidInstructionData)
            );
        }
    }
}

#[test]
fn public_update_wire_has_no_tracking_revision() {
    let apply = apply_ix();
    let data = update_data(7, ListId::Block, &apply);
    let mut expected = 7u64.to_le_bytes().to_vec();
    expected.push(1);
    expected.extend_from_slice(bytemuck::cast_slice(&data.writes));
    expected.push(0);
    expected.extend_from_slice(bytemuck::bytes_of(&apply));
    assert_eq!(data.encode().unwrap(), expected);
    assert_eq!(StorageUpdateIxData::parse(&expected), Ok(data));
}

fn enrollment_vector() -> Value {
    let registry = key(0x31);
    let owner = key(0x32);
    let ix = EnrollIxData {
        nullifier_pk: [0x23; 32],
        list_tag: [0x24; 32],
        viewing_u: [0x34; 32],
        eph_u: [0x35; 32],
        ciphertext: [0x36; 64],
    };
    let proof = CustomRingProof::zeroed();
    let enroll = EnrollmentWrite {
        owner: &owner,
        change: EnrollmentChange::Enroll {
            ix: &ix,
            proof: &proof,
        },
    };
    let rotate = EnrollmentWrite {
        owner: &owner,
        change: EnrollmentChange::Rotate {
            viewing_u: &[0x37; 32],
        },
    };
    let [signed_enroll, signed_rotate] =
        [enroll, rotate].map(|write| write.signed(&registry).unwrap());
    let chain_start = tracking_chain_start(&registry, Revision::from(0)).unwrap();
    let encoded = |write: EnrollmentWrite<'_>| {
        let mut data = Vec::new();
        write.encode_into(&mut data);
        hex::encode(data)
    };
    json!({
        "description": "One first enrollment and one rotation of the same owner. A write is owner || kind || body, kind 0 an EnrollIxData and a RegisterKey proof, kind 1 a viewing key. entry_hash is Poseidon(CRTQ, member, nullifier_pk, ct_hash, list_tag, viewing_hash, proven), a rotation's sealed fields zero. message is CRING/enr || registry || owner || entry_hash, the bytes the owner signs. chain_start is the hash chain of [CRTA, registry field, revision], chain_end folds Poseidon(chain, entry_hash) over both writes. Produced by program-libs/interface/tests/accounts.rs.",
        "registry": hex::encode(registry),
        "owner": hex::encode(owner),
        "enroll_write": encoded(enroll),
        "rotate_write": encoded(rotate),
        "enroll_entry_hash": hex::encode(signed_enroll.entry_hash),
        "enroll_message": hex::encode(signed_enroll.message),
        "rotate_entry_hash": hex::encode(signed_rotate.entry_hash),
        "rotate_message": hex::encode(signed_rotate.message),
        "chain_start": hex::encode(chain_start),
        "chain_end": hex::encode(
            [signed_enroll, signed_rotate]
                .iter()
                .try_fold(chain_start, |chain, signed| chain_link(&chain, &signed.entry_hash))
                .unwrap()
        ),
    })
}

#[test]
fn the_enrollment_is_the_shared_vector() {
    let committed: Value =
        serde_json::from_str(&std::fs::read_to_string(ENROLL_VECTOR).expect("vector file"))
            .expect("vector json");
    assert_eq!(committed, enrollment_vector());
}

#[test]
#[ignore = "rewrites test-vectors/registry/tracking_enroll.json"]
fn write_tracking_enroll_vector() {
    let json = serde_json::to_string_pretty(&enrollment_vector()).expect("json");
    std::fs::write(ENROLL_VECTOR, json + "\n").expect("write vector file");
}
