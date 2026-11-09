//! An owner signs one enrollment write and hands it to Ring RPC, the ring's sole enrollment writer.
//!
//! A first enrollment proves `RegisterKey`, the ciphertext opens to the nullifier secret and list salt
//! under the registry's auditor key, and inserts the owner's leaf with the key, list tag and seal fixed
//! for good. A rotation carries only the new viewing key and moves only the leaf's `viewing_hash`. The
//! writer batches signed writes into one `EnrollBatch`, storage verifies every owner signature and
//! proof, and a write grants nothing until that batch lands under a root.
//!
//! Read after `sdk-libs/client/src/rpc/enrollment.rs` and
//! `programs/storage/src/instructions/apply_enrollments.rs`.
//! Read before `services/ring-rpc/src/operations/enrollment_applier.rs`.

use async_trait::async_trait;
use custom_ring_interface::{
    domain,
    key_catalog::RingCircuit,
    storage_instruction::{tag, EnrollIxData, EnrollmentChange, EnrollmentWrite, OwnerGrantIxData},
    CustomRingProof, ReaderKeyBytes, RegisterKeyStatement, SYSTEM_PROGRAM_ID,
};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_signer::Signer;
use spr_api::SprApi;
use spr_client::{AsyncRpc, ClientError};
use spr_hasher::primitives::{right_align, solana_owner_identity};
use spr_keypair::{NullifierKey, X25519Pubkey};
use zeroize::Zeroizing;
use zolana_ring_client::{
    rpc::{SignedEnrollment, SubmitEnrollment, SubmitEnrollmentRequest},
    AsyncRingIndexer, NullifierKeyEnvelope, RegisteredKeys,
};

use crate::client::{RingClient, RingIndexerBackend};
use crate::{
    prover::{to_instruction_proof, RingRequest},
    request::KeyRegister,
    storage::storage_ix,
    CustomRing, RingError,
};

#[cfg(test)]
#[path = "../tests/operations/enrollment.rs"]
mod tests;

/// The owner signs with its own key, a PDA holds none and cannot enroll.
#[must_use]
pub struct Enroll<'a> {
    pub ring: CustomRing,
    pub owner: Address,
    pub nullifier_key: &'a NullifierKey,
    /// The key senders read from the leaf, a rotation moves only it.
    pub viewing_pubkey: X25519Pubkey,
    /// `ListSaltHolder::list_salt` of the ring.
    pub list_salt: [u8; 32],
}

/// The program signs through CPI for a PDA.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteOwnerGrant {
    pub payer: Address,
    pub owner: Address,
    pub tracking_registry: Address,
    pub grant: Address,
    pub reader: ReaderKeyBytes,
    pub active: bool,
}

/// One owner's write for one tracking registry, signed only when it goes to the writer.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerEnrollment {
    pub ring: CustomRing,
    pub tracking_registry: Address,
    pub owner: Address,
    pub change: OwnedChange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnedChange {
    Enroll {
        ix: EnrollIxData,
        proof: Box<CustomRingProof>,
    },
    /// Bound to the leaf's key and counter it replaces, a stale pair refuses the batch.
    Rotate {
        viewing_u: [u8; 32],
        prior_viewing_u: [u8; 32],
        /// Little endian.
        prior_seq: [u8; 8],
    },
}

/// The ring's enrollment writer, the one way an enrollment reaches the chain.
#[async_trait]
pub trait EnrollmentService: Send + Sync {
    /// Refuses before sending a request whose signature does not verify.
    async fn submit_enrollment(&self, request: SubmitEnrollmentRequest) -> Result<(), RingError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnrolledLeaf {
    pub ix: EnrollIxData,
    /// Rotations `viewing_hash` binds.
    pub seq: u64,
}

/// The enrollment an owner holds, its leaf's fields under the registry's newest root.
#[must_use]
pub struct EnrollmentRead {
    pub tracking_registry: Address,
    pub owner: Address,
}

impl Enroll<'_> {
    /// An owner holding a leaf takes a rotation, its first key stays final.
    pub async fn prove<R: AsyncRpc, I: RingIndexerBackend>(
        &self,
        client: &RingClient<R, I>,
    ) -> Result<OwnerEnrollment, RingError> {
        let config = self.ring.require_config(client.zolana()).await?;
        let tracking_registry = config.account().tracking;
        let enrollment = |change| OwnerEnrollment {
            ring: self.ring,
            tracking_registry,
            owner: self.owner,
            change,
        };
        // 1. A leaf fixes its key, tag and seal, a rotation moves only its viewing key.
        let enrolled = EnrollmentRead {
            tracking_registry,
            owner: self.owner,
        }
        .read(client.zolana())
        .await?;
        let viewing_u = self.viewing_pubkey.u();
        if let Some(enrolled) = enrolled {
            if enrolled.ix.nullifier_pk != self.nullifier_key.pubkey()? {
                return Err(RingError::AlreadyEnrolled { owner: self.owner });
            }
            return Ok(enrollment(enrolled.rotation(viewing_u)));
        }
        // 2. RegisterKey proves the ciphertext opens to the nullifier secret and list salt under
        //    the auditor key the registry pins, so the auditor recovers both from the inserting write.
        let ring_id = self.ring.ring_id()?;
        let auditor_pk = config.auditor_pubkey;
        let key = self.nullifier_key;
        let envelope = NullifierKeyEnvelope::new(key, &self.list_salt, &auditor_pk)?;
        let nullifier_pk = key.pubkey()?;
        let list_tag = domain::list_tag(&self.list_salt)?;
        let member = solana_owner_identity(self.owner.as_array())?;
        let eph_u = envelope.sealed.eph_pk.u();
        let public_input_hash = RegisterKeyStatement {
            ring_id: &ring_id,
            member: &member,
            nullifier_pk: &nullifier_pk,
            list_tag: &list_tag,
            auditor_pubkey: auditor_pk.as_bytes(),
            eph_u: &eph_u,
            ciphertext: &envelope.sealed.ciphertext,
        }
        .hash()?;
        let body = KeyRegister {
            public_input_hash,
            ring_id,
            member,
            nullifier_secret: Zeroizing::new(right_align(&key.secret())),
            list_salt: self.list_salt,
            eph_sk: envelope.ephemeral_sk.clone(),
            auditor_pk,
        };
        let proof = RingRequest::new(RingCircuit::RegisterKey, &body)?
            .prove(client, public_input_hash)
            .await?;
        Ok(enrollment(OwnedChange::Enroll {
            ix: EnrollIxData {
                nullifier_pk,
                list_tag,
                viewing_u,
                eph_u,
                ciphertext: envelope.sealed.ciphertext,
            },
            proof: Box::new(to_instruction_proof(proof)?),
        }))
    }
}

impl EnrolledLeaf {
    pub(crate) fn rotation(&self, viewing_u: [u8; 32]) -> OwnedChange {
        OwnedChange::Rotate {
            viewing_u,
            prior_viewing_u: self.ix.viewing_u,
            prior_seq: self.seq.to_le_bytes(),
        }
    }
}

impl OwnerEnrollment {
    /// The bytes storage parses and the entry it hashes.
    pub fn write(&self) -> EnrollmentWrite<'_> {
        let change = match &self.change {
            OwnedChange::Enroll { ix, proof } => EnrollmentChange::Enroll { ix, proof },
            OwnedChange::Rotate {
                viewing_u,
                prior_viewing_u,
                prior_seq,
            } => EnrollmentChange::Rotate {
                viewing_u,
                prior_viewing_u,
                prior_seq,
            },
        };
        EnrollmentWrite {
            owner: &self.owner,
            change,
        }
    }

    pub fn sign(&self, owner: &dyn Signer) -> Result<SubmitEnrollmentRequest, RingError> {
        if owner.try_pubkey()? != self.owner {
            return Err(RingError::EnrollmentSigner { owner: self.owner });
        }
        Ok(SignedEnrollment {
            ring: self.ring.program_id(),
            registry: self.tracking_registry,
            write: &self.write(),
        }
        .sign(owner)?)
    }

    /// The write reaches the chain once the writer's next batch lands.
    pub async fn submit(
        &self,
        owner: &dyn Signer,
        writer: &(impl EnrollmentService + ?Sized),
    ) -> Result<(), RingError> {
        writer.submit_enrollment(self.sign(owner)?).await
    }
}

impl EnrollmentRead {
    /// `None` with no leaf, a write the writer still holds grants nothing.
    pub async fn read(
        self,
        client: &(impl AsyncRpc + AsyncRingIndexer + ?Sized),
    ) -> Result<Option<EnrolledLeaf>, RingError> {
        let owner = self.owner.to_bytes();
        let leaf = RegisteredKeys {
            registry: self.tracking_registry,
            owners: &[owner],
        }
        .leaves(client, client)
        .await?
        .remove(&owner);
        Ok(leaf.map(|leaf| EnrolledLeaf {
            ix: EnrollIxData {
                nullifier_pk: leaf.record.nullifier_pk,
                list_tag: leaf.record.list_tag,
                viewing_u: leaf.viewing_u,
                eph_u: leaf.seal.eph_pk.u(),
                ciphertext: leaf.seal.ciphertext,
            },
            seq: leaf.seq,
        }))
    }
}

#[async_trait]
impl EnrollmentService for SprApi {
    async fn submit_enrollment(&self, request: SubmitEnrollmentRequest) -> Result<(), RingError> {
        self.call::<SubmitEnrollment>(request)
            .await
            .map_err(|error| ClientError::Rpc(error.to_string()))?;
        Ok(())
    }
}

impl WriteOwnerGrant {
    pub fn instruction(self) -> Instruction {
        let data = OwnerGrantIxData {
            reader: self.reader,
            active: u8::from(self.active),
        };
        storage_ix(
            vec![
                AccountMeta::new(self.payer, true),
                AccountMeta::new_readonly(self.owner, true),
                AccountMeta::new_readonly(self.tracking_registry, false),
                AccountMeta::new(self.grant, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            ],
            [&[tag::OWNER_GRANT][..], bytemuck::bytes_of(&data)].concat(),
        )
    }
}
