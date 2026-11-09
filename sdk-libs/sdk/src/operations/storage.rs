//! Storage registry builders, the apply proof of a record batch and the enrollment writer's batch.
//!
//! A public batch proves under the public apply key. A batch with salted owner writes uses the
//! private apply key of its width and proves, for each one, the owner's enrolled tracking row under
//! one tracking root and the written key `ListKey(member, salt)` under the row's list tag. The update
//! names that root's revision, Storage reads it from the pinned tracking registry and raises
//! `tracking_floor` to it in the same instruction, so no frame proves absence under an older root.
//!
//! Read after `program-libs/interface/src/instructions/storage.rs` and
//! `program-libs/member-registry/src/record.rs`.
//! Read before `programs/storage/src/instructions/update.rs`.

use custom_ring_interface::{
    domain,
    key_catalog::RingCircuit,
    owner_signature::{Ed25519Signatures, OwnerSignature},
    storage::{RegistryId, Revision, StorageRegistry},
    storage_instruction::{
        tag, ApplyEnrollmentsIxData, ApplyEnrollmentsRequest, AssetWrite, InitializeIxData,
        RecordWriteClassification, StorageUpdateIxData,
    },
    tag as ring_tag,
    tracking::TrackingEntry,
    ApplyRecordsIxData, ApplyRecordsStatement, CustomRingError, GRACE_SLOTS, STORAGE_PROGRAM_ID,
    SYSTEM_PROGRAM_ID,
};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_sdk_ids::{ed25519_program, sysvar};
use spr_client::AsyncRpc;
use spr_hasher::primitives::solana_owner_identity;
use spr_indexer_api::Hash;
use spr_interface::ZERO_NULLIFIER_PUBKEY;
use zolana_ring_client::{
    records::path_of, resolver::tracking_record, rpc::SubmitEnrollmentRequest, AsyncRingIndexer,
};
use zolana_ring_indexer::tracking::{
    GetTrackingApplyProofInputsRequest, GetTrackingApplyProofInputsResponse, RingTrackingEntryView,
};
use zolana_ring_member_registry::{
    ApplyProofInputs, RecordStatementContext, RecordTracking, TrackingOpening,
    TrackingWriteProofInput,
};
use zolana_ring_policy::Member;

use crate::{
    client::{RingClient, RingIndexerBackend},
    prover::{to_instruction_proof, RingRequest},
    records::TrackingMemberRead,
    request::{
        ApplyEnrollments as ApplyEnrollmentsBody, ApplyRecords, PrivateApplyRecords,
        PrivateOwnerRequest, RecordWriteProofInputRequest,
    },
    submission::BoundRoot,
    CustomRing, RingError,
};

#[cfg(test)]
#[path = "../tests/operations/storage.rs"]
mod tests;

/// The creator signs and becomes the authority, the payer funds the account.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitializeStorage {
    pub payer: Address,
    pub creator: Address,
    pub registry_id: RegistryId,
    pub grace_slots: u64,
    /// `ring_id_field` of the ring it serves, immutable, zero serves as a curator only.
    pub ring_id: [u8; 32],
    /// Nonzero creates a private source paired with the named tracking registry.
    pub tracking_registry: Address,
}

/// `tracking_registry` is the registry's pin, read only by a batch with private writes.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateStorage<'a> {
    pub authority: Address,
    pub registry: Address,
    pub tracking_registry: Address,
    pub data: StorageUpdateIxData<'a>,
}
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpdateStorageAuthority {
    pub authority: Address,
    pub new_authority: Address,
    pub registry: Address,
}

/// Proves a public batch with the public apply key of its write count, padded with empty writes.
#[must_use]
#[derive(Clone, Copy, Debug)]
pub struct StorageApplyProof<'a> {
    pub inputs: &'a ApplyProofInputs,
    /// The registry as read, its root and cursor must equal the inputs' old state.
    pub registry: &'a StorageRegistry,
    /// The mint preimages of private asset writes.
    pub asset_writes: &'a [AssetWrite],
}

/// The owner and list salt behind the salted key at `write_index`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SaltedOwnerWrite {
    pub write_index: u8,
    pub owner: Address,
    pub list_salt: [u8; 32],
}

/// Proves a batch with salted owner writes, one per private write without a mint preimage.
#[must_use]
#[derive(Clone, Copy)]
pub struct PrivateStorageWrite<'a> {
    pub ring: CustomRing,
    pub inputs: &'a ApplyProofInputs,
    /// The registry as read, its root and cursor must equal the inputs' old state.
    pub registry: &'a StorageRegistry,
    /// The mint preimages of private asset writes.
    pub asset_writes: &'a [AssetWrite],
    pub owners: &'a [SaltedOwnerWrite],
}

/// The proof and the tracking revision the update names beside its writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvenPrivateWrite {
    pub apply: ApplyRecordsIxData,
    pub tracking_revision: Revision,
}

/// Everything ahead of the prover, a refusal here sends no request.
pub(crate) struct PrivateWritePlan {
    pub circuit: RingCircuit,
    pub statement: ApplyRecordsStatement,
    pub body: ApplyRecords,
    pub tracking_revision: Revision,
}

impl InitializeStorage {
    /// The ring's records, keyed by the ring program id under the creator who pays.
    ///
    /// The pin is immutable and a later policy can serve a private list from these records, so
    /// every ring pins its tracking registry at creation. An unpinned source refuses every frame
    /// reading a private list, and without the floor a blocked enrolled owner could prove itself
    /// unenrolled under an old tracking root.
    pub fn records(ring: CustomRing, creator: Address) -> Self {
        Self {
            payer: creator,
            creator,
            registry_id: RegistryId(ring.program_id().to_bytes()),
            grace_slots: GRACE_SLOTS,
            ring_id: ring.ring_id().unwrap_or_default(),
            tracking_registry: ring.tracking_registry().unwrap_or_default(),
        }
    }

    pub fn registry(&self) -> Address {
        StorageRegistry::find_address(&self.creator, &self.registry_id).0
    }

    pub fn instruction(self) -> Instruction {
        let data = InitializeIxData {
            registry_id: self.registry_id,
            grace_slots: self.grace_slots.to_le_bytes(),
            ring_id: self.ring_id,
            tracking_registry: self.tracking_registry,
        };
        storage_ix(
            vec![
                AccountMeta::new(self.payer, true),
                AccountMeta::new_readonly(self.creator, true),
                AccountMeta::new(self.registry(), false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            ],
            [&[tag::INITIALIZE], bytemuck::bytes_of(&data)].concat(),
        )
    }
}

impl UpdateStorage<'_> {
    /// The pinned tracking registry rides exactly with a private batch, Storage refuses any other.
    pub fn instruction(self) -> Result<Instruction, RingError> {
        let private = self.data.private_writes()? != 0;
        let mut accounts = vec![
            AccountMeta::new_readonly(self.authority, true),
            AccountMeta::new(self.registry, false),
        ];
        accounts.extend(private.then(|| AccountMeta::new_readonly(self.tracking_registry, false)));
        Ok(storage_ix(
            accounts,
            [vec![tag::UPDATE], self.data.encode()?].concat(),
        ))
    }
}

impl UpdateStorageAuthority {
    pub fn instruction(self) -> Instruction {
        storage_ix(
            vec![
                AccountMeta::new_readonly(self.authority, true),
                AccountMeta::new_readonly(self.new_authority, true),
                AccountMeta::new(self.registry, false),
            ],
            vec![tag::UPDATE_AUTHORITY],
        )
    }
}

impl StorageApplyProof<'_> {
    pub async fn prove<R: AsyncRpc, I: RingIndexerBackend>(
        self,
        client: &RingClient<R, I>,
    ) -> Result<ApplyRecordsIxData, RingError> {
        let statement = self.inputs.statement(RecordStatementContext {
            registry: self.registry,
            asset_writes: self.asset_writes,
            tracking: RecordTracking::Public,
        })?;
        let key = ApplyKey::of(&statement)?;
        let body = key.body(self.inputs, &statement, Vec::new())?;
        prove_apply(client, key.circuit, &statement, &body).await
    }
}

impl PrivateStorageWrite<'_> {
    pub async fn prove<R: AsyncRpc, I: RingIndexerBackend>(
        self,
        client: &RingClient<R, I>,
    ) -> Result<ProvenPrivateWrite, RingError> {
        let plan = self.plan(client.zolana()).await?;
        Ok(ProvenPrivateWrite {
            apply: prove_apply(client, plan.circuit, &plan.statement, &plan.body).await?,
            tracking_revision: plan.tracking_revision,
        })
    }

    pub(crate) async fn plan(
        self,
        rpc: &(impl AsyncRpc + AsyncRingIndexer + ?Sized),
    ) -> Result<PrivateWritePlan, RingError> {
        // 1. The salted owners must name exactly the writes Storage classifies as private, and each
        //    written key must be its owner's list key, so a wrong salt fails before any read.
        let width = usize::from(custom_ring_interface::key_catalog::apply_width(
            self.inputs.writes.len() as u64,
        ));
        let private_writes = RecordWriteClassification {
            entries: self
                .inputs
                .writes
                .iter()
                .map(|write| Ok::<_, CustomRingError>(write.entry)),
            asset_writes: self.asset_writes,
        }
        .private_writes()?;
        let owners = self
            .owners
            .iter()
            .map(BoundOwner::bind(self.inputs))
            .collect::<Result<Vec<_>, _>>()?;
        let named = owners.iter().try_fold(0u16, |mask, owner| {
            1u16.checked_shl(u32::from(owner.write.write_index))
                .filter(|bit| mask & bit == 0)
                .map(|bit| mask | bit)
                .ok_or(RingError::PrivateWritesMismatch)
        })?;
        if named != private_writes || named == 0 {
            return Err(RingError::PrivateWritesMismatch);
        }
        // 2. One tracking root answers every owner. Storage raises the floor to its revision, so a
        //    frame never proves absence under a root older than the rows the write relies on.
        let tracking = BoundRoot::tracking(&*self.ring.require_policy(rpc).await?, rpc).await?;
        let mut requests = vec![None; width];
        for owner in &owners {
            let opening = TrackingMemberRead {
                root: tracking,
                member: owner.member,
            }
            .read(rpc)
            .await?;
            // 3. Only an enrolled row under a nonzero key carries a list tag the salt can match.
            let TrackingOpening::Present { record, path } = opening else {
                return Err(RingError::WriteOwnerNotEnrolled {
                    write_index: owner.write.write_index,
                });
            };
            if record.nullifier_pk == ZERO_NULLIFIER_PUBKEY {
                return Err(RingError::WriteOwnerNotEnrolled {
                    write_index: owner.write.write_index,
                });
            }
            if domain::list_tag(&owner.write.list_salt)? != record.list_tag {
                return Err(RingError::ListSaltMismatch {
                    member: owner.member,
                });
            }
            requests[usize::from(owner.write.write_index)] = Some(PrivateOwnerRequest {
                list_salt: owner.write.list_salt,
                record: (&record).into(),
                index: path.index,
                path: path.siblings.as_flattened().to_vec(),
            });
        }
        let statement = self.inputs.statement(RecordStatementContext {
            registry: self.registry,
            asset_writes: self.asset_writes,
            tracking: RecordTracking::Private {
                tracking_root: tracking.root,
            },
        })?;
        let key = ApplyKey::of(&statement)?;
        let body = key.body(self.inputs, &statement, requests)?;
        Ok(PrivateWritePlan {
            circuit: key.circuit,
            statement,
            body,
            tracking_revision: tracking.revision,
        })
    }
}

/// A salted owner write with its owner's member, its written key checked.
struct BoundOwner {
    write: SaltedOwnerWrite,
    member: [u8; 32],
}

impl BoundOwner {
    fn bind(
        inputs: &ApplyProofInputs,
    ) -> impl Fn(&SaltedOwnerWrite) -> Result<Self, RingError> + '_ {
        move |write| {
            let member = solana_owner_identity(write.owner.as_array())?;
            let key = Member::salted(&Member::from_bytes(member)?, &write.list_salt)?;
            match inputs.writes.get(usize::from(write.write_index)) {
                Some(written) if written.entry.member == *key.as_bytes() => Ok(Self {
                    write: *write,
                    member,
                }),
                _ => Err(RingError::ListKeyMismatch {
                    write_index: write.write_index,
                }),
            }
        }
    }
}

/// The apply key of a statement, writes past the batch pad to `width`.
struct ApplyKey {
    circuit: RingCircuit,
    width: usize,
}

impl ApplyKey {
    /// The statement's mode picks the family, as Storage does from the classified writes.
    fn of(statement: &ApplyRecordsStatement) -> Result<Self, RingError> {
        match statement.circuit() {
            circuit @ RingCircuit::ApplyRecords { writes, .. } => Ok(Self {
                circuit,
                width: usize::from(writes),
            }),
            circuit => Err(RingError::UnpinnedProvingKey(circuit)),
        }
    }

    /// Writes and owners past the batch assign zeros, a public key takes no owner.
    fn body(
        &self,
        inputs: &ApplyProofInputs,
        statement: &ApplyRecordsStatement,
        mut owners: Vec<Option<PrivateOwnerRequest>>,
    ) -> Result<ApplyRecords, RingError> {
        let mut writes: Vec<Option<RecordWriteProofInputRequest>> = inputs
            .writes
            .iter()
            .map(|write| Some(write.into()))
            .collect();
        writes.resize(self.width, None);
        let private = statement.private.as_ref().map(|private| {
            owners.resize(self.width, None);
            PrivateApplyRecords {
                private_writes: u64::from(private.private_writes),
                tracking_root: private.tracking_root,
                owners,
            }
        });
        Ok(ApplyRecords {
            public_input_hash: statement.hash()?,
            old_root: statement.old_root,
            new_root: statement.new_root,
            old_next_index: statement.old_next_index,
            new_next_index: statement.new_next_index,
            chain_start: statement.chain_start,
            chain_end: statement.chain_end,
            count: u8::try_from(statement.count).map_err(|_| RingError::RecordProjectionStale)?,
            writes,
            private,
        })
    }
}

async fn prove_apply<R: AsyncRpc, I: RingIndexerBackend>(
    client: &RingClient<R, I>,
    circuit: RingCircuit,
    statement: &ApplyRecordsStatement,
    body: &ApplyRecords,
) -> Result<ApplyRecordsIxData, RingError> {
    let proof = RingRequest::new(circuit, body)?
        .prove(client, body.public_input_hash)
        .await?;
    Ok(ApplyRecordsIxData {
        proof: to_instruction_proof(proof)?,
        new_root: statement.new_root,
        new_next_index: statement.new_next_index.to_le_bytes(),
        restrictive: u8::from(statement.restrictive),
    })
}

/// The writer's batch, signed by the ring's `EnrollmentWriter` role and preceded by every owner signature.
#[must_use]
pub struct EnrollBatch<'a> {
    pub ring: CustomRing,
    pub writer: Address,
    pub tracking_registry: Address,
    /// Owner writes in the order the batch applies them.
    pub writes: &'a [SubmitEnrollmentRequest],
}

/// The batch proven from the root the indexer's current revision holds.
#[must_use]
pub struct ProvenEnrollBatch {
    /// The ed25519 precompile instruction, then the ring `EnrollBatch` it must directly precede.
    pub instructions: [Instruction; 2],
    /// A landed apply past it leaves the batch unsendable.
    pub revision: Revision,
}

impl EnrollBatch<'_> {
    pub async fn prove<R: AsyncRpc, I: RingIndexerBackend>(
        self,
        client: &RingClient<R, I>,
    ) -> Result<ProvenEnrollBatch, RingError> {
        // 1. Each write verifies again, a write whose signature fails would refuse the whole batch.
        let writes_len = self.writes.len();
        let writes = self
            .writes
            .iter()
            .map(|request| request.verify(&self.tracking_registry))
            .collect::<Result<Vec<_>, _>>()?;
        let signed = writes
            .iter()
            .map(|write| write.signed(&self.tracking_registry))
            .collect::<Result<Vec<_>, _>>()?;
        let raw: Vec<[u8; 64]> = self
            .writes
            .iter()
            .map(|request| request.signature.0.into())
            .collect();
        let signatures: Vec<_> = writes
            .iter()
            .zip(&raw)
            .zip(&signed)
            .map(|((write, signature), signed)| OwnerSignature {
                owner: write.owner,
                signature,
                message: &signed.message,
            })
            .collect();
        // 2. The apply proof covers the entries in write order from the current root.
        let entries = writes
            .iter()
            .map(|write| Ok(entry_view(&write.entry()?)))
            .collect::<Result<Vec<_>, RingError>>()?;
        let response = client
            .zolana()
            .get_tracking_apply_proof_inputs(GetTrackingApplyProofInputsRequest {
                registry: self.tracking_registry.into(),
                entries,
            })
            .await?;
        let inputs = tracking_apply_inputs(&response)?;
        let revision = Revision::from(response.revision);
        let statement = inputs.statement();
        let Some(circuit @ RingCircuit::ApplyEnrollments { writes: batch }) =
            RingCircuit::apply_enrollments(statement.count)
        else {
            return Err(RingError::EnrollmentBatchSize {
                writes: usize::try_from(statement.count).unwrap_or(usize::MAX),
            });
        };
        let public_input_hash = statement.hash()?;
        let mut proof_writes: Vec<_> = inputs
            .writes
            .iter()
            .map(|write| Some(write.into()))
            .collect();
        proof_writes.resize_with(usize::from(batch), || None);
        let body = ApplyEnrollmentsBody {
            public_input_hash,
            old_root: statement.old_root,
            new_root: statement.new_root,
            old_next_index: statement.old_next_index,
            new_next_index: statement.new_next_index,
            chain_start: statement.chain_start,
            chain_end: statement.chain_end,
            count: u8::try_from(statement.count).map_err(|_| RingError::RecordProjectionStale)?,
            writes: proof_writes,
        };
        let proof = RingRequest::new(circuit, &body)?
            .prove(client, public_input_hash)
            .await?;
        let ix = ApplyEnrollmentsIxData {
            revision,
            proof: to_instruction_proof(proof)?,
            new_root: statement.new_root,
            new_next_index: statement.new_next_index.to_le_bytes(),
            count: body.count,
        };
        let data = ApplyEnrollmentsRequest { ix: &ix, writes }.encode();
        // 3. Storage reads the signatures from the instruction just before the top level one.
        let precompile = Instruction {
            program_id: ed25519_program::ID,
            accounts: Vec::new(),
            data: Ed25519Signatures::encode(&signatures)
                .ok_or(RingError::EnrollmentBatchSize { writes: writes_len })?,
        };
        let ring = self.ring;
        let accounts = vec![
            AccountMeta::new_readonly(self.writer, true),
            AccountMeta::new_readonly(ring.policy_pda(), false),
            AccountMeta::new_readonly(ring.ring_auth_pda(), false),
            AccountMeta::new(self.tracking_registry, false),
            AccountMeta::new_readonly(sysvar::instructions::ID, false),
            AccountMeta::new_readonly(STORAGE_PROGRAM_ID, false),
        ];
        let batch = ring.ix(accounts, [&[ring_tag::ENROLL_BATCH][..], &data].concat());
        Ok(ProvenEnrollBatch {
            instructions: [precompile, batch],
            revision,
        })
    }
}

fn entry_view(entry: &TrackingEntry) -> RingTrackingEntryView {
    RingTrackingEntryView {
        member: Hash(entry.member),
        nullifier_pk: Hash(entry.nullifier_pk),
        ct_hash: Hash(entry.ct_hash),
        list_tag: Hash(entry.list_tag),
        viewing_hash: Hash(entry.viewing_hash),
        prior_viewing_hash: Hash(entry.prior_viewing_hash),
        proven: entry.proven,
    }
}

fn tracking_apply_inputs(
    response: &GetTrackingApplyProofInputsResponse,
) -> Result<ApplyProofInputs<TrackingWriteProofInput>, RingError> {
    let writes = response
        .writes
        .iter()
        .map(|write| {
            Ok(TrackingWriteProofInput {
                entry: TrackingEntry {
                    member: write.member.0,
                    nullifier_pk: write.nullifier_pk.0,
                    ct_hash: write.ct_hash.0,
                    list_tag: write.list_tag.0,
                    viewing_hash: write.viewing_hash.0,
                    prior_viewing_hash: write.prior_viewing_hash.0,
                    proven: write.proven,
                },
                present: write.present,
                low: tracking_record(&write.low),
                low_path: path_of(&write.low_path)?,
                slot: path_of(&write.slot)?,
            })
        })
        .collect::<Result<_, RingError>>()?;
    Ok(ApplyProofInputs {
        old_root: response.old_root.0,
        new_root: response.new_root.0,
        old_next_index: response.old_next_index,
        new_next_index: response.new_next_index,
        chain_start: response.chain_start.0,
        chain_end: response.chain_end.0,
        writes,
    })
}

pub(crate) fn storage_ix(accounts: Vec<AccountMeta>, data: Vec<u8>) -> Instruction {
    Instruction {
        program_id: STORAGE_PROGRAM_ID,
        accounts,
        data,
    }
}
