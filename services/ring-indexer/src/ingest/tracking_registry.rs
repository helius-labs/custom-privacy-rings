//! Projects a tracking registry from `InitializeTracking` and the `EnrollmentsApplied` events, the projection reads no account.
//!
//! Storage emits one event per `ApplyEnrollments` by self invocation, after every owner signature,
//! every `RegisterKey` proof and the insertion proof verified. Anyone can send `tag::EMIT_EVENT` bytes, so an
//! event counts only under a successful storage `ApplyEnrollments` parent naming the same registry.
//! The writes it carries run through the member registry's apply rule, and the projection faults on
//! any root other than the one the event records.
//!
//! Read after `program-libs/interface/src/accounts/tracking.rs`, `program-libs/member-registry/src/tracking.rs`.
//! Read before `services/ring-indexer/src/rpc/tracking.rs`.

use custom_ring_interface::{
    storage::StorageError,
    storage_instruction::{
        tag, EnrollmentChange, EnrollmentWrite, EnrollmentsAppliedEvent, InitializeTrackingIxData,
        StorageEvent,
    },
    tracking::{next_viewing_seq, tracking_chain_start, TrackingRegistry, TrackingRootUpdate},
};
use sea_orm::ConnectionTrait;
use solana_address::Address;
use zolana_ring_indexer::tree::{TrackingRecord, TrackingWriteProofInput};

use crate::{
    block::Instruction,
    indexer::{fault, from_storage, ApplyError, Projection, RegistryCall, RegistryInstruction},
    member_registry::{plant_tree, AppliedBatch},
    storage::{LeafSeal, LeafStamp, StorageRoot, TrackingState, TrackingStore},
};

pub(crate) enum TrackingCall<'a> {
    Initialize(TrackingInit<'a>),
    Applied(AppliedEnrollments<'a>),
}

pub(crate) struct TrackingInit<'a> {
    pub creator: Address,
    pub data: &'a InitializeTrackingIxData,
}

pub(crate) struct AppliedEnrollments<'a> {
    pub header: EnrollmentsAppliedEvent,
    pub writes: Vec<EnrollmentWrite<'a>>,
}

/// `InitializeTracking`, accounts `payer, creator, tracking_registry, system_program`.
pub(crate) fn decode(instruction: &Instruction) -> Option<RegistryInstruction<TrackingCall<'_>>> {
    let (&tag::INITIALIZE_TRACKING, data) = instruction.data.split_first()? else {
        return None;
    };
    let account = |index: usize| instruction.accounts.get(index).copied();
    Some(RegistryInstruction {
        registry: account(2)?,
        call: account(1)
            .ok_or(StorageError::MissingAccounts)
            .and_then(|creator| {
                let data = bytemuck::try_from_bytes(data)
                    .map_err(|_| StorageError::InvalidInstructionData)?;
                Ok(TrackingCall::Initialize(TrackingInit { creator, data }))
            }),
    })
}

/// An event under its `ApplyEnrollments` parent, accounts `authority, tracking_registry, ..`.
pub(crate) fn decode_event<'a>(
    parent: &Instruction,
    event: &'a Instruction,
) -> Option<RegistryInstruction<TrackingCall<'a>>> {
    let StorageEvent::EnrollmentsApplied { header, writes } = StorageEvent::decode(&event.data)?;
    let registry = header.registry;
    let named = parent.accounts.get(1) == Some(&registry);
    Some(RegistryInstruction {
        registry,
        call: named
            .then_some(TrackingCall::Applied(AppliedEnrollments { header, writes }))
            .ok_or(StorageError::InvalidTrackingRegistry),
    })
}

impl RegistryCall<TrackingRegistry> for TrackingCall<'_> {
    fn created(&self, registry: Address) -> Option<TrackingState> {
        let Self::Initialize(init) = self else {
            return None;
        };
        let (canonical, _) = TrackingRegistry::find_address(&init.creator, &init.data.ring_id);
        (canonical == registry).then(|| TrackingState {
            registry: registry.to_bytes(),
            account: Box::new(TrackingRegistry::new(init.creator, init.data)),
            fault: None,
        })
    }

    async fn create<C: ConnectionTrait>(
        store: &TrackingStore<'_, C>,
        state: &TrackingState,
    ) -> anyhow::Result<()> {
        plant_tree(store.tree()).await?;
        store
            .save_root(&StorageRoot::current(&*state.account))
            .await?;
        store.save_state(state).await
    }

    /// A refusal of the program's own checks means the projection drifted.
    async fn project<C: ConnectionTrait>(
        self,
        projection: Projection<'_, '_, C, TrackingRegistry>,
    ) -> Result<(), ApplyError> {
        match self {
            Self::Initialize(_) => Err(fault("initialized twice")),
            Self::Applied(applied) => projection.apply(applied).await,
        }
    }
}

impl<C: ConnectionTrait> Projection<'_, '_, C, TrackingRegistry> {
    async fn apply(self, applied: AppliedEnrollments<'_>) -> Result<(), ApplyError> {
        let AppliedEnrollments { header, writes } = applied;
        // 1. The entries the program hashed, in write order.
        let entries = writes
            .iter()
            .map(EnrollmentWrite::entry)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| fault(format!("entry hash failed ({error})")))?;
        // 2. The roots rotate as the program rotates them, the event names the revision reached.
        let root = StorageRoot::current(&*self.state.account);
        let proven_at = self.state.account.newest().revision;
        let new_next_index = u64::from_le_bytes(header.new_next_index);
        let mut rotated = *self.state.account;
        rotated
            .update(TrackingRootUpdate {
                revision: proven_at,
                slot: self.slot,
                new_root: header.new_root,
                new_next_index,
            })
            .map_err(|error| fault(format!("apply refused ({error})")))?;
        if rotated.newest().revision != header.revision {
            return Err(fault("the event names another revision"));
        }
        let revision = header.revision.value();
        let registry = Address::new_from_array(self.state.registry);
        let chain_start = tracking_chain_start(&registry, proven_at)
            .map_err(|error| fault(format!("chain start failed ({error})")))?;
        // 3. The rule replays every write, the rows save only at the root the event recorded.
        let inputs = AppliedBatch::<TrackingRecord> {
            at: root.tree(),
            chain_start,
            entries: &entries,
            new_root: header.new_root,
            new_next_index,
            revision,
        }
        .save(self.store.tree())
        .await?;
        // 4. Each written leaf version keeps the key its `viewing_hash` commits to and its seal.
        self.store
            .stamp_leaves(revision, &leaf_stamps(&writes, &inputs.writes)?)
            .await
            .map_err(from_storage)?;
        *self.state.account = rotated;
        self.store
            .save_root(&StorageRoot::current(&rotated))
            .await?;
        Ok(())
    }
}

/// In write order, a Rotate of an absent owner leaves no leaf, an Enroll of a present one only its key.
fn leaf_stamps(
    writes: &[EnrollmentWrite<'_>],
    proofs: &[TrackingWriteProofInput],
) -> Result<Vec<LeafStamp>, ApplyError> {
    writes
        .iter()
        .zip(proofs)
        .filter_map(|(write, proof)| {
            let (viewing_u, seq, seal) = match write.change {
                EnrollmentChange::Enroll { ix, .. } => (
                    ix.viewing_u,
                    0,
                    (!proof.present).then_some(LeafSeal {
                        owner: write.owner.to_bytes(),
                        eph_u: ix.eph_u,
                        ciphertext: ix.ciphertext,
                    }),
                ),
                EnrollmentChange::Rotate { .. } if !proof.present => return None,
                EnrollmentChange::Rotate {
                    viewing_u,
                    prior_seq,
                    ..
                } => {
                    let Some(seq) = next_viewing_seq(u64::from_le_bytes(*prior_seq)) else {
                        return Some(Err(fault("a rotation counter overflowed")));
                    };
                    (*viewing_u, seq, None)
                }
            };
            Some(Ok(LeafStamp {
                member: proof.entry.member,
                viewing_u,
                seq,
                seal,
            }))
        })
        .collect()
}
