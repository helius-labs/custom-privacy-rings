use bytemuck::Zeroable;
use custom_ring_interface::{
    storage::Revision, storage_instruction::tag as storage_tag, RecordWriteData,
};
use zolana_ring_policy::ListId;

use super::*;
use crate::tests::support::ring;

const AUTHORITY: Address = Address::new_from_array([3; 32]);
const REGISTRY: Address = Address::new_from_array([4; 32]);
const TRACKING: Address = Address::new_from_array([5; 32]);

fn write_of(list: ListId) -> RecordWriteData {
    let mut write = RecordWriteData::zeroed();
    write.member = [1; 32];
    write.lists.mask = list.record_bit().to_le_bytes();
    write
}

fn update(list: ListId, tracking_revision: Option<Revision>) -> Result<Instruction, RingError> {
    let apply = ApplyRecordsIxData::zeroed();
    UpdateStorage {
        authority: AUTHORITY,
        registry: REGISTRY,
        tracking_registry: TRACKING,
        data: StorageUpdateIxData {
            revision: Revision::from(1),
            writes: vec![write_of(list)],
            asset_writes: Vec::new(),
            tracking_revision,
            apply: &apply,
        },
    }
    .instruction()
}

#[test]
fn a_rings_records_pin_its_tracking_registry_at_their_own_address() {
    let storage = InitializeStorage::records(ring(), AUTHORITY);
    assert_eq!(
        storage.tracking_registry,
        ring().tracking_registry().unwrap()
    );
    assert_eq!(
        storage.registry(),
        StorageRegistry::find_address(&AUTHORITY, &RegistryId(ring().program_id().to_bytes())).0
    );
    let data = storage.instruction().data;
    assert_eq!(data[0], storage_tag::INITIALIZE);
    let sent: &InitializeIxData = bytemuck::from_bytes(&data[1..]);
    assert_eq!(sent.tracking_registry, storage.tracking_registry);
}

#[test]
fn a_public_write_names_the_authority_and_registry_alone() {
    let accounts = update(ListId::Block, None).unwrap().accounts;
    assert_eq!(
        accounts.iter().map(|meta| meta.pubkey).collect::<Vec<_>>(),
        [AUTHORITY, REGISTRY]
    );
}

#[test]
fn a_private_write_reads_the_pinned_tracking_registry() {
    let accounts = update(ListId::PrivateBlock, Some(Revision::from(2)))
        .unwrap()
        .accounts;
    assert_eq!(
        accounts.iter().map(|meta| meta.pubkey).collect::<Vec<_>>(),
        [AUTHORITY, REGISTRY, TRACKING]
    );
    assert!(!accounts[2].is_writable && !accounts[2].is_signer);
}

/// Ring RPC sends this transaction with its writer as the fee payer, each text at its widest.
#[test]
fn a_full_batch_of_first_enrollments_fits_one_transaction() {
    use custom_ring_interface::{
        compute_unit_limit,
        storage_instruction::{ENROLLMENT_BATCH, SIGNED_MESSAGE_MAX_LEN},
        CustomRingProof, InstructionTag,
    };
    use solana_message::v1::MAX_TRANSACTION_SIZE;
    use solana_signer::Signer;
    use spr_client::ComputeBudgetConfig;

    use crate::{
        tests::support::{keypair, ServedLeaf},
        FitSigners, OwnedChange, OwnerEnrollment, TransactionFit,
    };

    let requests: Vec<_> = (0..ENROLLMENT_BATCH as u8)
        .map(|seed| {
            let owner = keypair(seed + 90);
            let leaf = ServedLeaf::sealed(owner.pubkey(), &owner.nullifier_key, &[8; 32]);
            OwnerEnrollment {
                ring: ring(),
                tracking_registry: TRACKING,
                owner: owner.pubkey(),
                change: OwnedChange::Enroll {
                    ix: leaf.enroll(),
                    proof: Box::new(CustomRingProof::zeroed()),
                },
            }
            .sign(&owner)
            .unwrap()
        })
        .collect();
    let batch = EnrollBatch {
        ring: ring(),
        writer: AUTHORITY,
        tracking_registry: TRACKING,
        writes: &requests,
    };
    let writes = batch.verified().unwrap();
    let widening: usize = writes
        .iter()
        .map(|write| SIGNED_MESSAGE_MAX_LEN - write.signed(&TRACKING).unwrap().message.len())
        .sum();
    let [precompile, enroll] = batch
        .instructions(&ApplyEnrollmentsIxData::zeroed(), writes)
        .unwrap();
    let ring_auth = enroll
        .accounts
        .iter()
        .find(|meta| meta.pubkey == ring().ring_auth_pda())
        .expect("ring auth");
    assert!(!ring_auth.is_writable);
    let fit = TransactionFit::of(
        &enroll,
        FitSigners::new(AUTHORITY).with_setup(core::slice::from_ref(&precompile)),
        ComputeBudgetConfig::new(compute_unit_limit(InstructionTag::EnrollBatch)),
    )
    .unwrap();
    assert!(fit.bytes + widening <= MAX_TRANSACTION_SIZE);
}

mod owner_request {
    use custom_ring_interface::{domain, tracking::TRACKING_QUAD_HEIGHT};
    use spr_hasher::primitives::solana_owner_identity;
    use zolana_ring_member_registry::{TrackingOpening, TrackingPath, TrackingRecord};
    use zolana_ring_policy::Member;

    use super::super::{BoundOwner, SaltedOwnerWrite};
    use crate::RingError;

    const OWNER: solana_address::Address = solana_address::Address::new_from_array([7; 32]);
    const OWN_SALT: [u8; 32] = [4; 32];
    const RING_SALT: [u8; 32] = [6; 32];

    fn bound(list_salt: [u8; 32], ring: bool) -> BoundOwner {
        BoundOwner {
            write: SaltedOwnerWrite {
                write_index: 0,
                owner: OWNER,
                list_salt,
            },
            member: solana_owner_identity(OWNER.as_array()).unwrap(),
            ring,
        }
    }

    fn path() -> TrackingPath {
        TrackingPath {
            index: 3,
            siblings: [[[0; 32]; 3]; TRACKING_QUAD_HEIGHT],
        }
    }

    fn row(list_salt: &[u8; 32]) -> TrackingRecord {
        TrackingRecord {
            member: solana_owner_identity(OWNER.as_array()).unwrap(),
            next: [0; 32],
            nullifier_pk: [5; 32],
            ct_hash: [0; 32],
            list_tag: domain::list_tag(list_salt).unwrap(),
            viewing_hash: [0; 32],
        }
    }

    fn present() -> TrackingOpening {
        TrackingOpening::Present {
            record: row(&OWN_SALT),
            path: path(),
        }
    }

    fn absent() -> TrackingOpening {
        TrackingOpening::Absent {
            low: TrackingRecord {
                member: [0; 32],
                ..row(&OWN_SALT)
            },
            path: path(),
        }
    }

    #[test]
    fn a_ring_salted_write_opens_the_committed_tag_for_an_owner_with_or_without_a_row() {
        let tag = domain::list_tag(&RING_SALT).unwrap();
        let unregistered = bound(RING_SALT, true).request(absent(), &tag).unwrap();
        assert!(unregistered.ring && unregistered.absent.is_some());
        let registered = bound(RING_SALT, true).request(present(), &tag).unwrap();
        assert!(registered.ring && registered.absent.is_none());
        for (salt, committed) in [(RING_SALT, [0; 32]), ([9; 32], tag), (OWN_SALT, tag)] {
            for opening in [absent(), present()] {
                assert!(matches!(
                    bound(salt, true).request(opening, &committed),
                    Err(RingError::RingSaltUncommitted { write_index: 0 })
                ));
            }
        }
    }

    #[test]
    fn a_write_under_the_owners_salt_needs_its_enrolled_row_and_tag() {
        let tag = domain::list_tag(&RING_SALT).unwrap();
        let own = bound(OWN_SALT, false).request(present(), &tag).unwrap();
        assert!(!own.ring && own.absent.is_none());
        assert!(matches!(
            bound(OWN_SALT, false).request(absent(), &tag),
            Err(RingError::WriteOwnerNotEnrolled { write_index: 0 })
        ));
        assert!(matches!(
            bound(RING_SALT, false).request(present(), &tag),
            Err(RingError::ListSaltMismatch { .. })
        ));
    }

    #[test]
    fn a_written_key_binds_to_its_family_or_is_refused() {
        let member = Member::from_bytes(solana_owner_identity(OWNER.as_array()).unwrap()).unwrap();
        let write = SaltedOwnerWrite {
            write_index: 0,
            owner: OWNER,
            list_salt: RING_SALT,
        };
        let ring_key = *Member::ring_salted(&member, &RING_SALT).unwrap().as_bytes();
        let own_key = *Member::salted(&member, &RING_SALT).unwrap().as_bytes();
        assert!(BoundOwner::keyed(&write, Some(ring_key)).unwrap().ring);
        assert!(!BoundOwner::keyed(&write, Some(own_key)).unwrap().ring);
        for other in [Some([1; 32]), None] {
            assert!(matches!(
                BoundOwner::keyed(&write, other),
                Err(RingError::ListKeyMismatch { write_index: 0 })
            ));
        }
    }
}
