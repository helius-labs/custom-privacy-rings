//! Record tree constants and the record writes an apply proof chains, the tree
//! lives in `zolana-ring-member-registry`.
//!
//! A record holds one member's list bits and, on an asset, its mint settings. A transfer proof opens
//! a present member by its leaf and an absent member by the leaf bracketing it, so no prover can leave
//! out the record that convicts its subject. The sentinel and the strict member range make that
//! bracket exist for every member.
//!
//! Read after `docs/design/records-and-roots.md`. Read before `program-libs/interface/src/accounts/storage.rs`,
//! `program-libs/member-registry/src/tree.rs`, `prover/circuits/registry/apply_records.go`.

use spr_hasher::{
    primitives::{right_align, BN254_SCALAR_MODULUS_BE},
    Hasher, HasherError, Poseidon,
};
use zolana_ring_policy::MintPolicy;

use crate::{domain, error::CustomRingError};

pub const QUAD_ARITY: usize = 4;
/// The height of both trees, a policy registry and a tracking registry.
pub const QUAD_HEIGHT: usize = 16;
pub const QUAD_CAPACITY: u64 = 1 << (2 * QUAD_HEIGHT);
/// Writes the larger apply key proves.
pub const APPLY_BATCH: usize = 16;
/// Default slot allowance for a retired policy record root.
pub const GRACE_SLOTS: u64 = 750;

/// `q - 1`, the sentinel's next, every member sorts strictly below it.
pub const FIELD_MAX: [u8; 32] = {
    let mut max = BN254_SCALAR_MODULUS_BE;
    max[31] -= 1;
    max
};

/// Root of the tree holding only the sentinel, the first history entry of a registry.
pub const MEMBER_REGISTRY_EMPTY_ROOT: [u8; 32] = [
    0x29, 0x5e, 0xc1, 0x6f, 0x1f, 0xf5, 0x87, 0xf0, 0x26, 0x9e, 0xb2, 0x1d, 0xdf, 0x07, 0x9a, 0x2b,
    0x44, 0xcb, 0x3e, 0x0c, 0x68, 0x52, 0xf3, 0x54, 0x31, 0xab, 0x02, 0xf7, 0x65, 0x1a, 0xc9, 0x67,
];

/// The sentinel `(0, q - 1)` brackets every member in `[1, q - 2]` strictly.
pub fn check_member(member: &[u8; 32]) -> Result<(), CustomRingError> {
    // Big endian bytes compare as the field values do.
    if *member == [0; 32] || *member >= FIELD_MAX {
        return Err(CustomRingError::MemberOutOfRange);
    }
    Ok(())
}

/// One masked write, `lists' = (lists & !mask) | value`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordWrite {
    pub member: [u8; 32],
    pub mask: u64,
    /// `RecordWriteData::storage_entry` keeps it inside `mask`, the apply circuit takes it as given.
    pub value: u64,
    /// Only authority writes replace mint settings, list writes preserve them.
    pub mint_policy: Option<MintPolicy>,
}

impl RecordWrite {
    /// `Poseidon(CRQW, member, mask, value, mint_policy_write, withdrawal_threshold, limits_0, limits_1, limits_2)`, mirror
    /// `prover/circuits/registry/apply_records.go`.
    pub fn hash(&self) -> Result<[u8; 32], HasherError> {
        let policy = self.mint_policy.unwrap_or_default();
        let [limits_0, limits_1, limits_2] = policy.limits.fields();
        Poseidon::hashv(&[
            &domain::field(domain::RECORD_WRITE),
            &self.member,
            &right_align(&self.mask.to_be_bytes()),
            &right_align(&self.value.to_be_bytes()),
            &self.mint_policy_field(),
            &right_align(&policy.withdrawal_threshold.to_be_bytes()),
            &limits_0,
            &limits_1,
            &limits_2,
        ])
    }

    /// Bit 192 distinguishes a replacement (including clearing) from no change.
    ///
    /// Byte 7 of the big endian field, above the three amounts `MintPolicy::field` packs. The apply
    /// circuit decomposes the field into 193 bits and reads the flag at the top.
    pub fn mint_policy_field(&self) -> [u8; 32] {
        self.mint_policy.map_or([0; 32], |policy| {
            let mut field = policy.field();
            field[7] = 1;
            field
        })
    }
}

/// `chain(p) = Poseidon(chain(p - 1), entry_p)` from `chain_start` of the registry and revision.
pub fn write_chain(previous: &[u8; 32], entry: &RecordWrite) -> Result<[u8; 32], HasherError> {
    chain_link(previous, &entry.hash()?)
}

/// One link of an apply chain, both trees' circuits fold `Poseidon(previous, entry_hash)`.
pub fn chain_link(previous: &[u8; 32], entry_hash: &[u8; 32]) -> Result<[u8; 32], HasherError> {
    Poseidon::hashv(&[previous, entry_hash])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(byte: u8) -> [u8; 32] {
        right_align(&[byte])
    }

    #[test]
    fn members_lie_strictly_between_the_sentinel_bounds() {
        // `q - 2`, the modulus ends in `f0000001`.
        let mut below_max = FIELD_MAX;
        below_max[28] -= 1;
        below_max[29..].fill(0xff);
        assert_eq!(check_member(&member(1)), Ok(()));
        assert_eq!(check_member(&below_max), Ok(()));
        for outside in [[0; 32], FIELD_MAX, BN254_SCALAR_MODULUS_BE] {
            assert_eq!(
                check_member(&outside),
                Err(CustomRingError::MemberOutOfRange)
            );
        }
    }
}
