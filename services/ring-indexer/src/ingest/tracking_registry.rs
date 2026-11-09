//! Projects a tracking registry from its instructions alone, the program emits no event and the projection reads no account.
//!
//! A successful `Enroll` appends its instruction data at the queue's tail, outer or inner, so every
//! queued field is one the program verified. `ApplyEnrollments` consumes `count` entries from the
//! head through the member registry's apply rule and faults on any root other than the one the
//! program recorded. Sequences follow successful `Enroll` instructions in block order, as the
//! program assigns them.
//!
//! Read after `program-libs/interface/src/accounts/tracking.rs`, `program-libs/member-registry/src/tracking.rs`.
//! Read before `services/ring-indexer/src/rpc/tracking.rs`.

use custom_ring_interface::{
    storage::StorageError,
    storage_instruction::{tag, ApplyEnrollmentsIxData, EnrollRequest, InitializeTrackingIxData},
    tracking::{
        tracking_chain_start, TrackingRegistry, TrackingRootUpdate, ENROLLMENT_QUEUE_CAPACITY,
    },
};
use sea_orm::ConnectionTrait;
use solana_address::Address;
use zolana_ring_indexer::tree::{TrackingRecord, TrackingWriteProofInput};

use crate::{
    block::Instruction,
    indexer::{fault, from_storage, ApplyError, Projection, RegistryCall, RegistryInstruction},
    member_registry::{plant_tree, AppliedBatch},
    storage::{LeafStamp, QueuedEnrollment, StorageRoot, TrackingState, TrackingStore},
};

pub(crate) enum TrackingCall<'a> {
    Initialize(TrackingInit<'a>),
    Enroll(EnrollCall<'a>),
    Apply(&'a ApplyEnrollmentsIxData),
}

pub(crate) struct TrackingInit<'a> {
    pub creator: Address,
    pub data: &'a InitializeTrackingIxData,
}

pub(crate) struct EnrollCall<'a> {
    pub owner: Address,
    pub request: EnrollRequest<'a>,
}

/// Account order the tags document in `storage_instruction`.
pub(crate) fn decode(instruction: &Instruction) -> Option<RegistryInstruction<TrackingCall<'_>>> {
    let (&tag, data) = instruction.data.split_first()?;
    let account = |index: usize| {
        instruction
            .accounts
            .get(index)
            .copied()
            .ok_or(StorageError::MissingAccounts)
    };
    let registry = |index: usize| account(index).ok();
    let instruction = match tag {
        tag::INITIALIZE_TRACKING => RegistryInstruction {
            registry: registry(2)?,
            call: account(1).and_then(|creator| {
                let data = bytemuck::try_from_bytes(data)
                    .map_err(|_| StorageError::InvalidInstructionData)?;
                Ok(TrackingCall::Initialize(TrackingInit { creator, data }))
            }),
        },
        tag::ENROLL => RegistryInstruction {
            registry: registry(1)?,
            call: account(0).and_then(|owner| {
                Ok(TrackingCall::Enroll(EnrollCall {
                    owner,
                    request: EnrollRequest::parse(data)?,
                }))
            }),
        },
        tag::APPLY_ENROLLMENTS => RegistryInstruction {
            registry: registry(0)?,
            call: bytemuck::try_from_bytes(data)
                .map(TrackingCall::Apply)
                .map_err(|_| StorageError::InvalidInstructionData),
        },
        _ => return None,
    };
    Some(instruction)
}

impl RegistryCall<TrackingRegistry> for TrackingCall<'_> {
    fn created(&self, registry: Address) -> Option<TrackingState> {
        let Self::Initialize(init) = self else {
            return None;
        };
        let (canonical, _) = TrackingRegistry::find_address(&init.creator, &init.data.ring_id);
        (canonical == registry).then(|| TrackingState {
            registry: registry.to_bytes(),
            account: Box::new(TrackingRegistry::new(init.data)),
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
            Self::Enroll(enroll) => projection.enroll(enroll).await,
            Self::Apply(data) => projection.apply(data).await,
        }
    }
}

impl<C: ConnectionTrait> Projection<'_, '_, C, TrackingRegistry> {
    /// The program's data and capacity checks, the proof itself verified on chain.
    async fn enroll(self, call: EnrollCall<'_>) -> Result<(), ApplyError> {
        let EnrollCall { owner, request } = call;
        request
            .ix
            .check()
            .map_err(|error| fault(format!("enrollment refused ({error})")))?;
        let bounds = self.store.queue_bounds().await.map_err(from_storage)?;
        if bounds.tail - bounds.head >= ENROLLMENT_QUEUE_CAPACITY as u64 {
            return Err(fault("the enrollment queue is full"));
        }
        let queued = QueuedEnrollment {
            sequence: bounds.tail,
            owner: owner.to_bytes(),
            proven: request.proof.is_some(),
            ix: *request.ix,
        };
        self.store
            .enqueue(&queued, self.slot)
            .await
            .map_err(from_storage)
    }

    async fn apply(self, data: &ApplyEnrollmentsIxData) -> Result<(), ApplyError> {
        // 1. The program's count checks, an apply consumes the oldest entries and never names them.
        ApplyEnrollmentsIxData::check_count(data.count)
            .map_err(|error| fault(format!("apply refused ({error})")))?;
        let count = usize::from(data.count);
        let bounds = self.store.queue_bounds().await.map_err(from_storage)?;
        let rows = self
            .store
            .queued(bounds.head, count as u64)
            .await
            .map_err(from_storage)?;
        if rows.len() != count {
            return Err(fault("an apply consumes more entries than the queue holds"));
        }
        let entries = rows
            .iter()
            .map(QueuedEnrollment::entry)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| fault(format!("entry hash failed ({error})")))?;
        // 2. The roots rotate as the program rotates them, the refusal is the program's own.
        let root = StorageRoot::current(&*self.state.account);
        let mut rotated = *self.state.account;
        rotated
            .update(TrackingRootUpdate {
                revision: data.revision,
                slot: self.slot,
                new_root: data.new_root,
                new_next_index: data.new_next_index(),
            })
            .map_err(|error| fault(format!("apply refused ({error})")))?;
        let revision = rotated.newest().revision.value();
        let registry = Address::new_from_array(self.state.registry);
        let chain_start = tracking_chain_start(&registry, data.revision)
            .map_err(|error| fault(format!("chain start failed ({error})")))?;
        // 3. The rule replays every entry, the rows save only at the root the program recorded.
        let inputs = AppliedBatch::<TrackingRecord> {
            at: root.tree(),
            chain_start,
            entries: &entries,
            new_root: data.new_root,
            new_next_index: data.new_next_index(),
            revision,
        }
        .save(self.store.tree())
        .await?;
        // 4. Each written leaf version keeps the key its `viewing_hash` commits to.
        self.store
            .stamp_leaves(revision, &leaf_stamps(&rows, &inputs.writes))
            .await
            .map_err(from_storage)?;
        self.store
            .mark_applied(bounds.head, count as u64, self.slot)
            .await
            .map_err(from_storage)?;
        *self.state.account = rotated;
        self.store
            .save_root(&StorageRoot::current(&rotated))
            .await?;
        Ok(())
    }
}

/// In sequence order, an unproven entry of an absent owner leaves no leaf.
fn leaf_stamps(rows: &[QueuedEnrollment], writes: &[TrackingWriteProofInput]) -> Vec<LeafStamp> {
    rows.iter()
        .zip(writes)
        .filter(|(row, write)| write.present || row.proven)
        .map(|(row, write)| LeafStamp {
            member: write.entry.member,
            viewing_u: row.ix.viewing_u,
            inserted: (!write.present).then_some(row.sequence),
        })
        .collect()
}
