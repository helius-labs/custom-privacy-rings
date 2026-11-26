use solana_address::Address;
use spr_keypair::{KeypairError, X25519Keypair};
use thiserror::Error;
use zeroize::Zeroizing;
use zolana_ring_client::rpc::auditor_keypair;

use crate::error::ServiceError;

const DERIVATION_INFO: &[u8] = b"zolana/ring-auditor";
pub const GRANT_ROW_KEY_INFO: &[u8] = b"zolana/ring-rpc-grant-rows";
const RING_SALT_INFO: &[u8] = b"zolana/ring-salt";
/// Part of the pinned derivation info of every registered auditor key.
const DERIVATION_SUFFIX: &[u8] = &[0];

pub enum KeySource {
    Local {
        ring: Address,
        auditor: X25519Keypair,
    },
    Derived(RootSecret),
}

pub struct GrantRowsSecret(Zeroizing<[u8; 32]>);
pub struct RootSecret(Zeroizing<[u8; 32]>);

#[derive(Debug, Error)]
#[error("root secret cannot be zero")]
pub struct RootSecretError;

impl RootSecret {
    pub fn unwrapped(secret: Zeroizing<[u8; 32]>) -> Self {
        Self(secret)
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, RootSecretError> {
        if bytes == [0; 32] {
            return Err(RootSecretError);
        }
        Ok(Self(Zeroizing::new(bytes)))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl GrantRowsSecret {
    pub fn unwrapped(secret: Zeroizing<[u8; 32]>) -> Self {
        Self(secret)
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// The cluster and ring bind each auditor key.
#[must_use]
pub struct AuditorKeyDerivation<'a> {
    pub root: &'a RootSecret,
    pub genesis_hash: &'a [u8; 32],
    pub ring: Address,
}

impl AuditorKeyDerivation<'_> {
    pub fn derive(self) -> Result<X25519Keypair, ServiceError> {
        let mut secret = Zeroizing::new([0u8; 32]);
        hkdf::Hkdf::<sha2::Sha256>::new(None, self.root.as_bytes())
            .expand_multi_info(
                &[
                    DERIVATION_INFO,
                    self.genesis_hash,
                    self.ring.as_ref(),
                    DERIVATION_SUFFIX,
                ],
                secret.as_mut_slice(),
            )
            .map_err(|_| KeypairError::Hkdf)?;
        Ok(auditor_keypair(&secret)?)
    }
}

/// The cluster and ring bind each ring salt, the label keeps it apart from the auditor's X25519 role.
#[must_use]
pub struct RingSaltDerivation<'a> {
    pub auditor: &'a X25519Keypair,
    pub genesis_hash: &'a [u8; 32],
    pub ring: Address,
}

impl RingSaltDerivation<'_> {
    /// Canonical by its zero top byte, nonzero by its low bit.
    pub fn derive(self) -> Result<Zeroizing<[u8; 32]>, ServiceError> {
        let mut salt = Zeroizing::new([0u8; 32]);
        hkdf::Hkdf::<sha2::Sha256>::new(None, self.auditor.secret_bytes().as_slice())
            .expand_multi_info(
                &[RING_SALT_INFO, self.genesis_hash, self.ring.as_ref()],
                &mut salt[1..],
            )
            .map_err(|_| KeypairError::Hkdf)?;
        salt[31] |= 1;
        Ok(salt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ring_salt_is_a_nonzero_field_element_of_its_ring_and_cluster() {
        let auditor = X25519Keypair::from_bytes(&[7; 32]);
        let salt = |genesis_hash: [u8; 32], ring: u8| {
            RingSaltDerivation {
                auditor: &auditor,
                genesis_hash: &genesis_hash,
                ring: Address::new_from_array([ring; 32]),
            }
            .derive()
            .expect("ring salt")
        };
        let derived = salt([9; 32], 5);

        assert_eq!((derived[0], derived[31] & 1), (0, 1));
        assert_eq!(derived, salt([9; 32], 5));
        assert_ne!(derived, salt([9; 32], 6));
        assert_ne!(derived, salt([8; 32], 5));
        assert_ne!(*derived, *auditor.secret_bytes());
    }

    /// The ring config pins its auditor key forever, so a new derivation domain orphans every ring.
    #[test]
    fn auditor_keys_are_pinned_to_their_derivation_domain() {
        let root = RootSecret::from_bytes([7; 32]).expect("root secret");
        let auditor = AuditorKeyDerivation {
            root: &root,
            genesis_hash: &[9; 32],
            ring: Address::new_from_array([5; 32]),
        }
        .derive()
        .expect("auditor key");

        assert_eq!(
            hex::encode(auditor.pubkey().as_bytes()),
            "04dce77eb5607cf1f4b729cdbaa87b6698b1a65d52f40bcf503d4c2f978c97143f"
        );
    }
}
