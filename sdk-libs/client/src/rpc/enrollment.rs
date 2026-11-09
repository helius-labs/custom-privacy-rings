//! An owner-signed enrollment write as the owner hands it to Ring RPC, the ring's sole enrollment writer.
//!
//! The owner signs `EnrollmentWrite::signed`, and storage `ApplyEnrollments` checks that
//! signature through the ed25519 precompile instruction ahead of the batch. The writer holds no key
//! that could stand in for the owner, so a held write changes nothing the owner did not sign.
//!
//! Read after `program-libs/interface/src/instructions/storage.rs`.
//! Read before `services/ring-rpc/src/operations/enrollment_applier.rs`.

use custom_ring_interface::storage_instruction::EnrollmentWrite;
use serde::{Deserialize, Serialize};
use solana_address::Address;
use solana_signature::Signature;
use solana_signer::{Signer, SignerError};
use spr_indexer_api::{Base64String, RpcMethod, SerializablePubkey, SerializableSignature};
use thiserror::Error;

use super::SUBMIT_ENROLLMENT;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SubmitEnrollmentRequest {
    pub ring: SerializablePubkey,
    /// `EnrollmentWrite::encode_into`, the owner first.
    pub write: Base64String,
    /// Raw Ed25519 of the owner over `EnrollmentWrite::signed`.
    pub signature: SerializableSignature,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SubmitEnrollmentResponse {}

#[derive(Debug, Error)]
pub enum EnrollmentRequestError {
    #[error("the enrollment write does not parse exactly")]
    Malformed,
    #[error("the enrollment write is refused by the storage program's checks")]
    Refused(#[source] custom_ring_interface::storage::StorageError),
    #[error("the owner's signature does not verify over the signed message")]
    Signature,
    #[error(transparent)]
    Signing(#[from] SignerError),
}

/// A write the owner signs for one tracking registry.
#[must_use]
pub struct SignedEnrollment<'a> {
    pub ring: Address,
    pub registry: Address,
    pub write: &'a EnrollmentWrite<'a>,
}

pub struct SubmitEnrollment;

impl RpcMethod for SubmitEnrollment {
    const NAME: &'static str = SUBMIT_ENROLLMENT;
    type Request = SubmitEnrollmentRequest;
    type Response = SubmitEnrollmentResponse;
}

impl SignedEnrollment<'_> {
    /// `owner` must be the write's owner, a PDA holds no key and cannot enroll.
    pub fn sign<S: Signer + ?Sized>(
        self,
        owner: &S,
    ) -> Result<SubmitEnrollmentRequest, EnrollmentRequestError> {
        let signed = self
            .write
            .signed(&self.registry)
            .map_err(EnrollmentRequestError::Refused)?;
        let signature = owner.try_sign_message(&signed.message)?;
        let mut write = Vec::new();
        self.write.encode_into(&mut write);
        Ok(SubmitEnrollmentRequest {
            ring: self.ring.into(),
            write: Base64String(write),
            signature: signature.into(),
        })
    }
}

impl SubmitEnrollmentRequest {
    /// The write it carries once it parses exactly, passes the program's field checks and the owner signed it.
    pub fn verify(
        &self,
        registry: &Address,
    ) -> Result<EnrollmentWrite<'_>, EnrollmentRequestError> {
        let (write, rest) =
            EnrollmentWrite::parse(&self.write.0).map_err(|_| EnrollmentRequestError::Malformed)?;
        if !rest.is_empty() {
            return Err(EnrollmentRequestError::Malformed);
        }
        write.check().map_err(EnrollmentRequestError::Refused)?;
        let signed = write
            .signed(registry)
            .map_err(EnrollmentRequestError::Refused)?;
        let signature: &Signature = &self.signature.0;
        if !signature.verify(write.owner.as_ref(), &signed.message) {
            return Err(EnrollmentRequestError::Signature);
        }
        Ok(write)
    }
}
