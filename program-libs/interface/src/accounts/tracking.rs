//! The tracking registry, one sorted tree per ring naming each enrolled owner's nullifier key, ciphertext hash, list tag and viewing key hash, and the queue its enrollments wait in.
//!
//! An SPP sender names any nullifier key on an output, and SPP never sees the secret behind it. A
//! ring's auditor must compute each nullifier, so each owner enrolls once with a proof that seals the
//! nullifier secret and the list salt to the auditor key. Every transfer, deposit and merge proof then
//! holds a live ring slot's key, other than `ZERO_NULLIFIER_PUBKEY`, to its owner's row under the tracking
//! root, and a transfer or deposit output under that key to an absent row. Without the row a sender could
//! pay a key the auditor cannot open, and a wallet could skip its salted lists.
//!
//! No account exists per owner. `Enroll` appends one entry hash to the registry's `EnrollmentQueue`, and
//! `ApplyEnrollments` consumes the queue in order. The instruction data of each successful `Enroll` is the
//! record an indexer and the auditor read.
//!
//! A ring pins its tracking registry's address, `ring_id` and the seeds authenticate nothing.
//!
//! Read after `docs/design/records-and-roots.md`, `program-libs/interface/src/accounts/storage.rs`.
//! Read before `programs/storage/src/instructions/enroll.rs`, `programs/storage/src/instructions/apply_enrollments.rs`,
//! `prover/circuits/registry/tracking.go`, `prover/circuits/policy/tracking.go`.
//!
//! Invariants:
//! 1. A row's nullifier key, ciphertext hash and list tag never move after insert. A later entry of its owner moves only `viewing_hash`, and an owner holds one row.
//! 2. Enrollment refuses `ZERO_NULLIFIER_PUBKEY` and proves every key a Poseidon output, so a present row's key is nonzero, and the policy circuit reads presence from the literal zero an absent query returns.
//! 3. A queued entry grants nothing, because every check reads the root. Only an entry whose `Enroll` verified `RegisterKey` inserts, a rotation of an absent owner changes nothing.
//! 4. The program stores each entry hash at its sequence and an apply chains the hashes from `head` in sequence order, so an applier picks only the count and no entry is skipped or applied twice.
//! 5. A retired tracking root serves its whole grace, an apply that would evict an unexpired root refuses and the queue stays. A source's `tracking_floor` is the one exception, and only on a salted read.

use bytemuck::{Pod, Zeroable};
use solana_address::Address;
use spr_hasher::{
    hash_chain::create_hash_chain_4_from_slice,
    primitives::{
        hash_bytes, is_canonical_bn254_scalar_be, is_canonical_x25519_public_key, right_align,
        solana_owner_identity,
    },
    Hasher, HasherError, Poseidon,
};
pub use spr_interface::ZERO_NULLIFIER_PUBKEY;
use zolana_ring_policy::registry_field;

use crate::{
    auditor_key::is_auditor_key,
    domain,
    member_registry::{GRACE_SLOTS, QUAD_CAPACITY},
    storage::{advance_roots, root_at, Retirement, Revision, Root, StorageError},
    storage_instruction::{EnrollIxData, InitializeTrackingIxData},
    STORAGE_PROGRAM_ID, TAGGED_X25519_KEY_LEN,
};

/// Fixed by the program, every retired tracking root serves it in full.
pub const TRACKING_GRACE_SLOTS: u64 = GRACE_SLOTS;
/// Every apply retires one root for `TRACKING_GRACE_SLOTS`, the history bounds applies per grace.
pub const TRACKING_ROOT_HISTORY: usize = 128;
/// Entries waiting at once, the queue account stays under the 10,240 byte growth limit of one CPI.
pub const ENROLLMENT_QUEUE_CAPACITY: usize = 256;

/// Root of the tree holding only the sentinel.
pub const TRACKING_EMPTY_ROOT: [u8; 32] = [
    0x19, 0x7c, 0xe7, 0xe4, 0xcc, 0x6e, 0x23, 0xed, 0x6f, 0x4d, 0x5b, 0x32, 0x30, 0x3d, 0xd9, 0x1e,
    0x03, 0x2f, 0x9a, 0xfd, 0xc7, 0xf8, 0xdd, 0x42, 0x4a, 0xb2, 0x63, 0x8d, 0x24, 0x5e, 0x45, 0x59,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct TrackingRegistry {
    pub discriminator: u8,
    /// Immutable, enrollment proofs bind it.
    pub ring_id: [u8; 32],
    /// Immutable, enrollment proofs seal to it.
    pub auditor_pubkey: [u8; TAGGED_X25519_KEY_LEN],
    /// Little endian, the leaf the next new owner takes, at most `QUAD_CAPACITY`.
    pub next_index: [u8; 8],
    /// Newest first, only retired roots have an expiry.
    pub roots: [Root; TRACKING_ROOT_HISTORY],
}

/// The entries of one tracking registry in `Enroll` order, at `[enrollment_queue, tracking_registry]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct EnrollmentQueue {
    pub discriminator: u8,
    /// Immutable, every load compares it.
    pub registry: Address,
    /// Little endian, the sequence of the oldest entry no apply consumed.
    pub head: [u8; 8],
    /// Little endian, the sequence the next `Enroll` takes.
    pub tail: [u8; 8],
    /// `TrackingEntry::hash` of sequence `s` at `s % ENROLLMENT_QUEUE_CAPACITY`.
    pub entries: [[u8; 32]; ENROLLMENT_QUEUE_CAPACITY],
}

/// The write an apply proof chains, computed by the program from the `Enroll` it checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackingEntry {
    pub member: [u8; 32],
    pub nullifier_pk: [u8; 32],
    pub ct_hash: [u8; 32],
    pub list_tag: [u8; 32],
    pub viewing_hash: [u8; 32],
    /// The `Enroll` verified `RegisterKey`, only a proven entry inserts a row.
    pub proven: bool,
}

/// A proven application of queued entries.
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
    pub fn new(ix: &InitializeTrackingIxData) -> Self {
        let mut registry = Self::zeroed();
        registry.init(ix);
        registry
    }

    /// `new` written into zeroed account data, the program builds no copy on its 4 KB stack frame.
    pub fn init(&mut self, ix: &InitializeTrackingIxData) {
        self.discriminator = Self::DISCRIMINATOR;
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
        if !(1..=QUAD_CAPACITY).contains(&update.new_next_index) {
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
            && (1..=QUAD_CAPACITY).contains(&self.next_index())
    }
}

impl EnrollmentQueue {
    pub const DISCRIMINATOR: u8 = 4;
    pub const SEED: &'static [u8] = b"enrollment_queue";
    pub const SIZE: usize = core::mem::size_of::<Self>();

    /// One queue per registry, its address names the registry.
    pub fn seeds(registry: &Address) -> [&[u8]; 2] {
        [Self::SEED, registry.as_ref()]
    }

    pub fn find_address(registry: &Address) -> (Address, u8) {
        Address::find_program_address(&Self::seeds(registry), &STORAGE_PROGRAM_ID)
    }

    /// Host only, the value overflows the 4 KB SBF stack frame.
    #[cfg(not(target_os = "solana"))]
    pub fn new(registry: Address) -> Self {
        let mut queue = Self::zeroed();
        queue.init(registry);
        queue
    }

    /// `new` written into zeroed account data, the program builds no copy on its 4 KB stack frame.
    pub fn init(&mut self, registry: Address) {
        self.discriminator = Self::DISCRIMINATOR;
        self.registry = registry;
    }

    pub fn from_data(data: &[u8]) -> Option<&Self> {
        bytemuck::try_from_bytes::<Self>(data)
            .ok()
            .filter(|queue| queue.valid())
    }

    pub fn from_data_mut(data: &mut [u8]) -> Option<&mut Self> {
        bytemuck::try_from_bytes_mut::<Self>(data)
            .ok()
            .filter(|queue| queue.valid())
    }

    /// Host only, the copy overflows the 4 KB SBF stack frame.
    #[cfg(not(target_os = "solana"))]
    pub fn decode(owner: &Address, data: &[u8]) -> Option<Self> {
        (*owner == STORAGE_PROGRAM_ID)
            .then(|| Self::from_data(data).copied())
            .flatten()
    }

    pub const fn head(&self) -> u64 {
        u64::from_le_bytes(self.head)
    }

    pub const fn tail(&self) -> u64 {
        u64::from_le_bytes(self.tail)
    }

    /// Entries no apply consumed, `valid` holds it at most `ENROLLMENT_QUEUE_CAPACITY`.
    pub const fn pending(&self) -> u64 {
        self.tail() - self.head()
    }

    /// Writes nothing on a refusal, returns the entry's sequence.
    pub fn push(&mut self, entry_hash: [u8; 32]) -> Result<u64, StorageError> {
        // A full queue would overwrite the slot of its oldest unconsumed entry.
        if self.pending() == ENROLLMENT_QUEUE_CAPACITY as u64 {
            return Err(StorageError::EnrollmentQueueFull);
        }
        let sequence = self.tail();
        self.entries[slot(sequence)] = entry_hash;
        self.tail = (sequence + 1).to_le_bytes();
        Ok(sequence)
    }

    /// The hashes of sequences `head .. head + count` in order, `head` advanced past them.
    pub fn take(&mut self, count: u64) -> Result<impl Iterator<Item = &[u8; 32]>, StorageError> {
        if count > self.pending() {
            return Err(StorageError::EnrollmentQueueShort);
        }
        let head = self.head();
        self.head = (head + count).to_le_bytes();
        // Advancing `head` frees the slots for later pushes, none happens before the iterator drops.
        let entries = &self.entries;
        Ok((head..head + count).map(move |sequence| &entries[slot(sequence)]))
    }

    fn valid(&self) -> bool {
        self.discriminator == Self::DISCRIMINATOR
            && self
                .tail()
                .checked_sub(self.head())
                .is_some_and(|pending| pending <= ENROLLMENT_QUEUE_CAPACITY as u64)
    }
}

impl TrackingEntry {
    /// The entry of `owner`'s `Enroll`, a rotation when no `RegisterKey` proof verified.
    pub fn from_enroll(
        owner: &Address,
        ix: &EnrollIxData,
        proven: bool,
    ) -> Result<Self, StorageError> {
        Ok(Self {
            member: owner_member(owner)?,
            nullifier_pk: ix.nullifier_pk,
            ct_hash: hash_bytes(&ix.ciphertext).map_err(|_| StorageError::HashingFailed)?,
            list_tag: ix.list_tag,
            viewing_hash: hash_bytes(&ix.viewing_u).map_err(|_| StorageError::HashingFailed)?,
            proven,
        })
    }

    /// `Poseidon(CRTQ, member, nullifier_pk, ct_hash, list_tag, viewing_hash, proven)`, mirror
    /// `prover/circuits/registry/apply_enrollments.go`.
    pub fn hash(&self) -> Result<[u8; 32], HasherError> {
        Poseidon::hashv(&[
            &domain::field(domain::TRACKING_ENTRY),
            &self.member,
            &self.nullifier_pk,
            &self.ct_hash,
            &self.list_tag,
            &self.viewing_hash,
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
        if self.viewing_u == [0; 32] || !is_canonical_x25519_public_key(&self.viewing_u) {
            return Err(StorageError::NonCanonicalViewingKey);
        }
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

/// The leaf member of a signer, a wallet or a PDA alike.
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

/// Sequences grow by one per `Enroll`, a u64 never wraps.
fn slot(sequence: u64) -> usize {
    (sequence % ENROLLMENT_QUEUE_CAPACITY as u64) as usize
}

// Byte fields only, the `Pod` image is the account data with no padding.
const _: () = assert!(core::mem::align_of::<TrackingRegistry>() == 1);
const _: () = assert!(TrackingRegistry::SIZE == 6218);
const _: () = assert!(core::mem::align_of::<EnrollmentQueue>() == 1);
const _: () = assert!(EnrollmentQueue::SIZE == 8241);

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
        for _ in 0..crate::QUAD_HEIGHT {
            node = Poseidon::hashv(&[&node, &zero, &zero, &zero]).unwrap();
            zero = Poseidon::hashv(&[&zero, &zero, &zero, &zero]).unwrap();
        }
        assert_eq!(node, TRACKING_EMPTY_ROOT);
    }
}
