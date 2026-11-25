//! `getListSalt`, an owner's salt or the ring salt for the writers of the ring's salted lists, never a nullifier secret.

use serde::{Deserialize, Serialize};
use solana_address::Address;
use solana_signer::Signer;
use spr_indexer_api::{Base64String, Hash, SerializablePubkey, SerializableSignature};
use zeroize::Zeroize;

use super::{base64, Attestation, ReadTarget, RequestBuildError, SignedRequest};
use crate::REDACTED;

pub const GET_LIST_SALT: &str = "getListSalt";
const LIST_SALT_DOMAIN: &str = "zolana/ring-rpc-list-salt";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetListSaltRequest {
    pub ring: SerializablePubkey,
    pub writer: SerializablePubkey,
    pub owner: SerializablePubkey,
    pub timestamp: u64,
    pub nonce: Base64String,
    pub signature: SerializableSignature,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetListSaltResponse {
    /// Absent while the owner has no tracking row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_salt: Option<Hash>,
    /// Present whenever the ring commits a ring list tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ring_salt: Option<Hash>,
}

impl core::fmt::Debug for GetListSaltResponse {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Self {
            list_salt,
            ring_salt,
        } = self;
        f.debug_struct("GetListSaltResponse")
            .field("list_salt", &list_salt.as_ref().map(|_| REDACTED))
            .field("ring_salt", &ring_salt.as_ref().map(|_| REDACTED))
            .finish()
    }
}

impl Drop for GetListSaltResponse {
    fn drop(&mut self) {
        if let Some(salt) = &mut self.list_salt {
            salt.0.zeroize();
        }
        if let Some(salt) = &mut self.ring_salt {
            salt.0.zeroize();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSaltAttestation {
    pub target: ReadTarget,
    pub writer: Address,
    pub owner: Address,
}

pub type ListSaltRequest = SignedRequest<ListSaltAttestation>;

impl Attestation for ListSaltAttestation {
    fn message(&self, timestamp: u64, nonce: &[u8; 32]) -> Vec<u8> {
        format!(
            "{LIST_SALT_DOMAIN}\n{}\nwriter: {}\nowner: {}\ntimestamp: {timestamp}\nnonce: {}",
            self.target.lines(),
            self.writer,
            self.owner,
            base64(nonce),
        )
        .into_bytes()
    }
}

impl ListSaltRequest {
    /// `writer` must be the attestation's writer.
    pub fn sign<S: Signer + ?Sized>(
        self,
        writer: &S,
    ) -> Result<GetListSaltRequest, RequestBuildError> {
        let (timestamp, message) = self.message()?;
        let signature = writer
            .try_sign_message(&message)
            .map_err(RequestBuildError::Signature)?;
        Ok(GetListSaltRequest {
            ring: self.attestation.target.ring.into(),
            writer: self.attestation.writer.into(),
            owner: self.attestation.owner.into(),
            timestamp,
            nonce: self.nonce.to_vec().into(),
            signature: signature.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_list_salt_response_debug_redacts_both_salts() {
        let response = GetListSaltResponse {
            list_salt: Some(Hash([0xab; 32])),
            ring_salt: Some(Hash([0xcd; 32])),
        };
        let rendered = format!("{response:?}");
        assert!(rendered.contains(REDACTED), "{rendered}");
        assert!(
            !rendered.contains(&response.list_salt.as_ref().unwrap().to_string()),
            "{rendered}"
        );
        assert!(
            !rendered.contains(&response.ring_salt.as_ref().unwrap().to_string()),
            "{rendered}"
        );
    }
}
