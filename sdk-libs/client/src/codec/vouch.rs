//! The digest a voucher signs.

use custom_ring_interface::domain::{field, VOUCH};
use spr_hasher::{Hasher, HasherError, Poseidon};

/// The transfer a voucher signs for, mirror `VouchDigest` in `prover/circuits/shared/derivation.go`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VouchMessage<'a> {
    pub private_tx_hash: &'a [u8; 32],
    pub withdrawal_hash: &'a [u8; 32],
}

impl VouchMessage<'_> {
    /// `Poseidon(CRVC, private_tx_hash, withdrawal_hash)`, the tag keeps the signed bytes apart from SPP's owner authorization digest.
    pub fn digest(&self) -> Result<[u8; 32], HasherError> {
        Poseidon::hashv(&[&field(VOUCH), self.private_tx_hash, self.withdrawal_hash])
    }
}
