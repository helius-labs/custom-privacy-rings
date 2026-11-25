use hkdf::Hkdf;
use sha2::Sha256;
use solana_address::Address;
use spr_keypair::{shielded::ShieldedKeypair, ViewingKey};
use zeroize::Zeroizing;

use crate::{RingKeyError, RotatingRingKeys};

pub const LIST_SALT_LABEL: [u8; 32] = *b"CRING/list_salt\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";

/// No root the auditor holds reaches another ring's salt.
pub trait ListSaltHolder {
    fn list_salt(&self, ring_program_id: &Address) -> Result<[u8; 32], RingKeyError>;
}

impl ListSaltHolder for ShieldedKeypair {
    fn list_salt(&self, ring_program_id: &Address) -> Result<[u8; 32], RingKeyError> {
        derive(&self.viewing_key, ring_program_id)
    }
}

impl ListSaltHolder for RotatingRingKeys {
    fn list_salt(&self, ring_program_id: &Address) -> Result<[u8; 32], RingKeyError> {
        derive(self.list_root(), ring_program_id)
    }
}

/// The same extract as zolana's view root, the label keeps the salt apart from every viewing role.
const VIEW_ROOT_SALT: &[u8] = b"TSPP/view_root/x25519/v1";
const LIST_SALT_INFO: &[u8] = b"CRING/mrg_seed/v1";

/// Canonical by its zero top byte, nonzero by its low bit.
fn derive(root: &ViewingKey, ring_program_id: &Address) -> Result<[u8; 32], RingKeyError> {
    let secret = Zeroizing::new(root.secret_bytes());
    let view_root = Hkdf::<Sha256>::new(Some(VIEW_ROOT_SALT), secret.as_slice());
    let mut salt = Zeroizing::new([0u8; 32]);
    view_root
        .expand_multi_info(
            &[LIST_SALT_INFO, ring_program_id.as_array(), &LIST_SALT_LABEL],
            &mut salt[1..],
        )
        .map_err(|_| RingKeyError::InvalidListSalt)?;
    salt[31] |= 1;
    Ok(*salt)
}
