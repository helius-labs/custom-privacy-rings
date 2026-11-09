//! The tracking registry, one sorted tree per ring naming each enrolled owner's nullifier key, ciphertext hash, list tag and viewing key hash.
//!
//! An SPP sender names any nullifier key on an output, and SPP never sees the secret behind it. A
//! ring's auditor must compute each nullifier, so each owner enrolls once with a proof that seals the
//! nullifier secret and the list salt to the auditor key. Every transfer, deposit and merge proof then
//! holds a live ring slot's key, other than `ZERO_NULLIFIER_PUBKEY`, to its owner's row under the tracking
//! root, and a transfer or deposit output under that key to an absent row. Without the row a sender could
//! pay a key the auditor cannot open, and a wallet could skip its salted lists.
//!
//! No account exists per owner. The registry's authority, the ring's `ring_auth` acting for its
//! `EnrollmentWriter`, submits owner-signed writes in batches, and each batch event carries every record.
//!
//! A ring pins its tracking registry's address, `ring_id` and the seeds authenticate nothing.
//!
//! Read after `docs/design/records-and-roots.md`, `program-libs/interface/src/accounts/storage.rs`.
//! Read before `programs/storage/src/instructions/apply_enrollments.rs`,
//! `prover/circuits/registry/tracking.go`, `prover/circuits/policy/tracking.go`.
//!
//! Invariants:
//! 1. A row's nullifier key, ciphertext hash and list tag never move after insert. A later entry of its owner moves only `viewing_hash`, and an owner holds one row.
//! 2. Enrollment refuses `ZERO_NULLIFIER_PUBKEY` and proves every key a Poseidon output, so a present row's key is nonzero, and the policy circuit reads presence from the literal zero an absent query returns.
//! 3. Only a proven entry inserts, a rotation of an absent owner changes nothing.
//! 4. Every entry carries its owner's signature over the registry and the entry hash, so the writer submits only what the owner signed.
//! 5. A retired tracking root serves its whole grace, an apply that would evict an unexpired root refuses. A source's `tracking_floor` is the one exception, and only on a salted read.

use bytemuck::{Pod, Zeroable};
use solana_address::Address;
use spr_hasher::{
    hash_chain::create_hash_chain_4_from_slice,
    primitives::{
        hash_bytes, is_canonical_bn254_scalar_be, is_canonical_x25519_public_key, pack_be,
        right_align, solana_owner_identity,
    },
    Hasher, HasherError, Poseidon,
};
pub use spr_interface::ZERO_NULLIFIER_PUBKEY;
use zolana_ring_policy::registry_field;

use crate::{
    auditor_key::is_auditor_key,
    domain,
    member_registry::GRACE_SLOTS,
    storage::{advance_roots, root_at, Retirement, Revision, Root, StorageError},
    storage_instruction::{EnrollIxData, InitializeTrackingIxData},
    STORAGE_PROGRAM_ID, TAGGED_X25519_KEY_LEN,
};

/// One leaf per enrolled owner, at least `QUAD_HEIGHT` for `ZERO_SUBTREES` to serve both trees.
pub const TRACKING_QUAD_HEIGHT: usize = 16;
pub const TRACKING_CAPACITY: u64 = 1 << (2 * TRACKING_QUAD_HEIGHT);
/// Fixed by the program, every retired tracking root serves it in full.
pub const TRACKING_GRACE_SLOTS: u64 = GRACE_SLOTS;
/// Every apply retires one root for `TRACKING_GRACE_SLOTS`, the history bounds applies per grace.
pub const TRACKING_ROOT_HISTORY: usize = 128;

/// Root of the tree holding only the sentinel.
pub const TRACKING_EMPTY_ROOT: [u8; 32] = [
    0x19, 0x7c, 0xe7, 0xe4, 0xcc, 0x6e, 0x23, 0xed, 0x6f, 0x4d, 0x5b, 0x32, 0x30, 0x3d, 0xd9, 0x1e,
    0x03, 0x2f, 0x9a, 0xfd, 0xc7, 0xf8, 0xdd, 0x42, 0x4a, 0xb2, 0x63, 0x8d, 0x24, 0x5e, 0x45, 0x59,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct TrackingRegistry {
    pub discriminator: u8,
    /// Immutable, the only signer of `ApplyEnrollments`.
    pub authority: Address,
    /// Immutable, enrollment proofs bind it.
    pub ring_id: [u8; 32],
    /// Immutable, enrollment proofs seal to it.
    pub auditor_pubkey: [u8; TAGGED_X25519_KEY_LEN],
    /// Little endian, the leaf the next new owner takes, at most `TRACKING_CAPACITY`.
    pub next_index: [u8; 8],
    /// Newest first, only retired roots have an expiry.
    pub roots: [Root; TRACKING_ROOT_HISTORY],
}

/// The write an apply proof chains, computed by the program from the owner-signed write it checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackingEntry {
    pub member: [u8; 32],
    pub nullifier_pk: [u8; 32],
    pub ct_hash: [u8; 32],
    pub list_tag: [u8; 32],
    pub viewing_hash: [u8; 32],
    /// The leaf's viewing hash a rotation replaces, zero on a first enrollment.
    pub prior_viewing_hash: [u8; 32],
    /// The write carried a verified `RegisterKey` proof, only a proven entry inserts a row.
    pub proven: bool,
}

/// A proven application of a batch of entries.
#[must_use]
pub struct TrackingRootUpdate {
    /// The revision the proof was made against.
    pub revision: Revision,
    pub slot: u64,
    pub new_root: [u8; 32],
    pub new_next_index: u64,
}

impl TrackingRegistry {
    pub const DISCRIMINATOR: u8 = 3;
    pub const SEED: &'static [u8] = b"tracking_registry";
    pub const SIZE: usize = core::mem::size_of::<Self>();

    /// The creator in the seeds keeps every other signer off its addresses.
    pub fn seeds<'a>(creator: &'a Address, ring_id: &'a [u8; 32]) -> [&'a [u8]; 3] {
        [Self::SEED, creator.as_ref(), ring_id]
    }

    pub fn find_address(creator: &Address, ring_id: &[u8; 32]) -> (Address, u8) {
        Address::find_program_address(&Self::seeds(creator, ring_id), &STORAGE_PROGRAM_ID)
    }

    /// Holds only the sentinel root at revision zero, host only as the value overflows the SBF stack frame.
    #[cfg(not(target_os = "solana"))]
    pub fn new(authority: Address, ix: &InitializeTrackingIxData) -> Self {
        let mut registry = Self::zeroed();
        registry.init(authority, ix);
        registry
    }

    /// `new` written into zeroed account data, the program builds no copy on its 4 KB stack frame.
    pub fn init(&mut self, authority: Address, ix: &InitializeTrackingIxData) {
        self.discriminator = Self::DISCRIMINATOR;
        self.authority = authority;
        self.ring_id = ix.ring_id;
        self.auditor_pubkey = ix.auditor_pubkey;
        self.next_index = 1u64.to_le_bytes();
        self.roots[0].hash = TRACKING_EMPTY_ROOT;
    }

    pub fn from_data(data: &[u8]) -> Option<&Self> {
        bytemuck::try_from_bytes::<Self>(data)
            .ok()
            .filter(|registry| registry.valid())
    }

    pub fn from_data_mut(data: &mut [u8]) -> Option<&mut Self> {
        bytemuck::try_from_bytes_mut::<Self>(data)
            .ok()
            .filter(|registry| registry.valid())
    }

    /// Host only, the copy overflows the 4 KB SBF stack frame.
    #[cfg(not(target_os = "solana"))]
    pub fn decode(owner: &Address, data: &[u8]) -> Option<Self> {
        (*owner == STORAGE_PROGRAM_ID)
            .then(|| Self::from_data(data).copied())
            .flatten()
    }

    pub const fn next_index(&self) -> u64 {
        u64::from_le_bytes(self.next_index)
    }

    pub const fn newest(&self) -> Root {
        self.roots[0]
    }

    pub fn root(&self, revision: Revision, slot: u64) -> Result<[u8; 32], StorageError> {
        root_at(&self.roots, revision, slot)
    }

    /// Writes nothing on a refusal, a retired root stays served for its whole grace.
    pub fn update(&mut self, update: TrackingRootUpdate) -> Result<(), StorageError> {
        // The circuit opens no leaf past the capacity, a larger index would name a leaf no path reaches.
        if !(1..=TRACKING_CAPACITY).contains(&update.new_next_index) {
            return Err(StorageError::TrackingTreeFull);
        }
        advance_roots(
            &mut self.roots,
            update.revision,
            Retirement::Retained {
                slot: update.slot,
                grace_slots: TRACKING_GRACE_SLOTS,
            },
            update.new_root,
        )?;
        self.next_index = update.new_next_index.to_le_bytes();
        Ok(())
    }

    fn valid(&self) -> bool {
        self.discriminator == Self::DISCRIMINATOR
            && (1..=TRACKING_CAPACITY).contains(&self.next_index())
    }
}

impl TrackingEntry {
    /// The proven entry of `owner`'s first enrollment.
    pub fn from_enroll(owner: &Address, ix: &EnrollIxData) -> Result<Self, StorageError> {
        Ok(Self {
            member: owner_member(owner)?,
            nullifier_pk: ix.nullifier_pk,
            ct_hash: hash_bytes(&ix.ciphertext).map_err(|_| StorageError::HashingFailed)?,
            list_tag: ix.list_tag,
            viewing_hash: viewing_hash(&ix.viewing_u, 0)?,
            prior_viewing_hash: [0; 32],
            proven: true,
        })
    }

    /// A rotation's sealed fields stay zero, the circuit moves no other field of a present row.
    pub fn from_rotation(
        owner: &Address,
        viewing_u: &[u8; 32],
        prior_viewing_u: &[u8; 32],
        prior_seq: u64,
    ) -> Result<Self, StorageError> {
        let seq = prior_seq
            .checked_add(1)
            .ok_or(StorageError::InvalidInstructionData)?;
        Ok(Self {
            member: owner_member(owner)?,
            nullifier_pk: [0; 32],
            ct_hash: [0; 32],
            list_tag: [0; 32],
            viewing_hash: viewing_hash(viewing_u, seq)?,
            prior_viewing_hash: viewing_hash(prior_viewing_u, prior_seq)?,
            proven: false,
        })
    }

    /// `Poseidon(CRTQ, member, nullifier_pk, ct_hash, list_tag, viewing_hash, prior_viewing_hash, proven)`, mirror
    /// `prover/circuits/registry/apply_enrollments.go`.
    pub fn hash(&self) -> Result<[u8; 32], HasherError> {
        Poseidon::hashv(&[
            &domain::field(domain::TRACKING_ENTRY),
            &self.member,
            &self.nullifier_pk,
            &self.ct_hash,
            &self.list_tag,
            &self.viewing_hash,
            &self.prior_viewing_hash,
            &right_align(&[u8::from(self.proven)]),
        ])
    }
}

impl InitializeTrackingIxData {
    /// A key no proof can seal to would lock enrollment for good.
    pub fn check(&self) -> Result<(), StorageError> {
        if self.ring_id == [0; 32] || !is_canonical_bn254_scalar_be(&self.ring_id) {
            return Err(StorageError::InvalidTrackingRegistry);
        }
        if !is_auditor_key(&self.auditor_pubkey) {
            return Err(StorageError::InvalidAuditorKey);
        }
        Ok(())
    }
}

impl EnrollIxData {
    /// A zero key has no secret to seal, a program owner stays unenrolled instead.
    pub fn check(&self) -> Result<(), StorageError> {
        check_viewing_key(&self.viewing_u)?;
        if self.nullifier_pk == ZERO_NULLIFIER_PUBKEY {
            return Err(StorageError::ZeroNullifierKey);
        }
        if !is_canonical_bn254_scalar_be(&self.nullifier_pk)
            || !is_canonical_bn254_scalar_be(&self.list_tag)
        {
            return Err(StorageError::InvalidInstructionData);
        }
        Ok(())
    }
}

/// A sender seals to the key, a zero u seals no secret and a noncanonical u re-encodes a canonical one.
pub fn check_viewing_key(viewing_u: &[u8; 32]) -> Result<(), StorageError> {
    if *viewing_u == [0; 32] || !is_canonical_x25519_public_key(viewing_u) {
        return Err(StorageError::NonCanonicalViewingKey);
    }
    Ok(())
}

/// Two limbs as an X25519 u can exceed the field, `seq` sets a returning key's hash apart.
pub fn viewing_hash(viewing_u: &[u8; 32], seq: u64) -> Result<[u8; 32], StorageError> {
    let [high, low] = pack_be::<32, 2>(viewing_u);
    Poseidon::hashv(&[
        &domain::field(domain::VIEWING_HASH),
        &high,
        &low,
        &right_align(&seq.to_be_bytes()),
    ])
    .map_err(|_| StorageError::HashingFailed)
}

/// The leaf member of an owner address.
pub fn owner_member(owner: &Address) -> Result<[u8; 32], StorageError> {
    solana_owner_identity(owner.as_array()).map_err(|_| StorageError::HashingFailed)
}

/// `[CRTA, registry, revision]`, an apply proof serves one registry at one revision.
pub fn tracking_chain_start(
    registry: &Address,
    revision: Revision,
) -> Result<[u8; 32], StorageError> {
    let registry = registry_field(registry.as_array()).map_err(|_| StorageError::HashingFailed)?;
    create_hash_chain_4_from_slice(&[
        domain::field(domain::APPLY_ENROLLMENTS),
        registry,
        right_align(&revision.value().to_be_bytes()),
    ])
    .map_err(|_| StorageError::HashingFailed)
}

// Byte fields only, the `Pod` image is the account data with no padding.
const _: () = assert!(core::mem::align_of::<TrackingRegistry>() == 1);
const _: () = assert!(TrackingRegistry::SIZE == 6250);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_empty_root_folds_the_sentinel_through_every_level() {
        let mut node = Poseidon::hashv(&[
            &domain::field(domain::TRACKING_LEAF),
            &[0; 32],
            &crate::FIELD_MAX,
            &[0; 32],
            &[0; 32],
            &[0; 32],
            &[0; 32],
        ])
        .unwrap();
        let mut zero = [0u8; 32];
        for _ in 0..TRACKING_QUAD_HEIGHT {
            node = Poseidon::hashv(&[&node, &zero, &zero, &zero]).unwrap();
            zero = Poseidon::hashv(&[&zero, &zero, &zero, &zero]).unwrap();
        }
        assert_eq!(node, TRACKING_EMPTY_ROOT);
    }
}
