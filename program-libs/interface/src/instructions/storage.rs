//! The storage program's tags, instruction data and write classification.
//!
//! The apply proof, and no authority label, decides whether an update retires the older roots. The
//! instruction carries the restrictive bit and the statement binds it, so a label the writes do not
//! prove fails verification. A private mask requires its owner's tracking row unless a mint preimage
//! authenticates an asset key at the same write position.
//!
//! Read after `program-libs/interface/src/accounts/storage.rs`, `program-libs/interface/src/accounts/tracking.rs`.
//! Read before `programs/storage/src/instructions/update.rs`.

use arrayvec::ArrayVec;
use bytemuck::{Pod, Zeroable};
use solana_address::Address;
use zolana_ring_policy::{ListSet, Member};

use crate::{
    domain,
    error::CustomRingError,
    instruction::{list_and_mint_entry, write_count},
    member_registry::RecordWrite,
    storage::{RegistryId, Revision, StorageError},
    tracking::{check_viewing_key, TrackingEntry},
    ApplyRecordsIxData, CustomRingProof, ReaderKeyBytes, RecordWriteData,
    REGISTER_KEY_CIPHERTEXT_LEN, TAGGED_X25519_KEY_LEN,
};

pub mod tag {
    pub const INITIALIZE: u8 = 0;
    pub const UPDATE: u8 = 1;
    /// No data, the authority and the new authority both sign.
    pub const UPDATE_AUTHORITY: u8 = 4;
    /// Signed by the registry's authority, the ring's `ring_auth` for its `EnrollmentWriter`.
    pub const APPLY_ENROLLMENTS: u8 = 7;
    pub const OWNER_GRANT: u8 = 8;
    /// The ring program signs its ring PDA as creator through CPI.
    pub const INITIALIZE_TRACKING: u8 = 9;
    /// A no-op the program invokes on itself to record a `StorageEvent`.
    pub const EMIT_EVENT: u8 = 10;
}

/// The byte after `tag::EMIT_EVENT`.
pub mod event_kind {
    pub const ENROLLMENTS_APPLIED: u8 = 1;
}

/// The byte after a write's owner.
pub mod enrollment_kind {
    pub const ENROLL: u8 = 0;
    pub const ROTATE: u8 = 1;
}

/// Most writes per `ApplyEnrollments`, as many first enrollments and their signatures as one transaction holds.
pub const ENROLLMENT_BATCH: usize = 5;
/// The longest `EnrollmentWrite::signed` text, each base58 value at its widest.
pub const SIGNED_MESSAGE_MAX_LEN: usize = domain::ENROLLMENT_SIGNATURE_DOMAIN.len()
    + REGISTRY_LINE.len()
    + BASE58_MAX_LEN
    + ENTRY_LINE.len()
    + BASE58_MAX_LEN
    + ACTION_LINE.len()
    + ENROLL_ACTION.len();

const REGISTRY_LINE: &[u8] = b"\nregistry: ";
const ENTRY_LINE: &[u8] = b"\nentry: ";
const ACTION_LINE: &[u8] = b"\naction: ";
const ENROLL_ACTION: &[u8; 6] = b"enroll";
const ROTATE_ACTION: &[u8; 6] = b"rotate";
const BASE58_MAX_LEN: usize = five8::BASE58_ENCODED_32_MAX_LEN;

/// The creator signs and becomes the authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct InitializeIxData {
    pub registry_id: RegistryId,
    /// Little endian.
    pub grace_slots: [u8; 8],
    /// `ring_id_field` of the ring it serves, immutable, zero serves as a curator only.
    pub ring_id: [u8; 32],
    /// Nonzero lets a salted list read the registry, immutable.
    pub tracking_registry: Address,
}

/// Tag 9, accounts `payer, creator, tracking_registry, system_program`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct InitializeTrackingIxData {
    /// `ring_id_field` of the ring program.
    pub ring_id: [u8; 32],
    pub auditor_pubkey: [u8; TAGGED_X25519_KEY_LEN],
}

/// A first enrollment's record, verified by its `RegisterKey` proof and signed by its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct EnrollIxData {
    /// Final once a proven entry inserts the owner's row, with `list_tag` and the ciphertext hash.
    pub nullifier_pk: [u8; 32],
    pub list_tag: [u8; 32],
    /// Canonical and nonzero, the owner's signature alone authorizes it.
    pub viewing_u: [u8; 32],
    pub eph_u: [u8; 32],
    /// The auditor derives every nullifier of the owner from the sealed secret and its private list key from the salt.
    pub ciphertext: [u8; REGISTER_KEY_CIPHERTEXT_LEN],
}

/// Tag 7 header, accounts `authority, tracking_registry, instructions_sysvar, storage_program`, `count` writes follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct ApplyEnrollmentsIxData {
    pub revision: Revision,
    pub proof: CustomRingProof,
    pub new_root: [u8; 32],
    /// Little endian.
    pub new_next_index: [u8; 8],
    pub count: u8,
}

/// `owner || kind || body`, one owner-signed change to the tracking tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnrollmentWrite<'a> {
    pub owner: &'a Address,
    pub change: EnrollmentChange<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnrollmentChange<'a> {
    /// Inserts an absent owner's row, a present owner's apply refuses.
    Enroll {
        ix: &'a EnrollIxData,
        proof: &'a CustomRingProof,
    },
    /// Moves a present owner's viewing key from the prior key at `prior_seq`, an absent owner stays absent.
    Rotate {
        viewing_u: &'a [u8; 32],
        prior_viewing_u: &'a [u8; 32],
        /// Little endian, the new key takes `prior_seq + 1`.
        prior_seq: &'a [u8; 8],
    },
}

/// The chain and the signature check read the same entry hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedWrite {
    pub entry_hash: [u8; 32],
    /// The exact bytes a wallet's `signMessage` signs.
    pub message: ArrayVec<u8, SIGNED_MESSAGE_MAX_LEN>,
}

/// Tag 7 data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyEnrollmentsRequest<'a> {
    pub ix: &'a ApplyEnrollmentsIxData,
    pub writes: Vec<EnrollmentWrite<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct EnrollmentsAppliedEvent {
    pub registry: Address,
    /// The revision `new_root` holds.
    pub revision: Revision,
    pub new_root: [u8; 32],
    /// Little endian.
    pub new_next_index: [u8; 8],
    pub count: u8,
}

/// `tag::EMIT_EVENT || kind || body`, trusted only as the inner instruction of a successful `ApplyEnrollments` of the storage program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageEvent<'a> {
    /// The header, then the batch's writes as the instruction carried them, proofs included.
    EnrollmentsApplied {
        header: EnrollmentsAppliedEvent,
        /// The tracking leaf holds the `RegisterKey` seal as `ct_hash` alone, the auditor opens each enrolled secret from these writes.
        writes: Vec<EnrollmentWrite<'a>>,
    },
}

/// Tag 8, accounts `payer, owner, tracking_registry, grant, system_program`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct OwnerGrantIxData {
    pub reader: ReaderKeyBytes,
    /// One grants, zero revokes.
    pub active: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct AssetWrite {
    pub write_index: u8,
    pub mint: Address,
}

#[must_use]
pub struct RecordWriteClassification<'a, I> {
    pub entries: I,
    pub asset_writes: &'a [AssetWrite],
}

/// Tag 1, `revision || count u8 || writes || asset_count u8 || asset writes || tracking_revision || ApplyRecordsIxData`,
/// `tracking_revision` present exactly when a private write needs its owner's row. Accounts `authority, registry`,
/// then the pinned `tracking_registry` with that revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageUpdateIxData<'a> {
    pub revision: Revision,
    pub writes: Vec<RecordWriteData>,
    pub asset_writes: Vec<AssetWrite>,
    /// Names the tracking root the selected rows open under, the update raises the floor to it.
    pub tracking_revision: Option<Revision>,
    pub apply: &'a ApplyRecordsIxData,
}

impl InitializeIxData {
    pub const SIZE: usize = core::mem::size_of::<Self>();

    pub const fn grace_slots(&self) -> u64 {
        u64::from_le_bytes(self.grace_slots)
    }
}

impl InitializeTrackingIxData {
    pub const SIZE: usize = core::mem::size_of::<Self>();
}

impl EnrollIxData {
    pub const SIZE: usize = core::mem::size_of::<Self>();
}

impl<'a> EnrollmentWrite<'a> {
    /// One write and the bytes after it.
    pub fn parse(data: &'a [u8]) -> Result<(Self, &'a [u8]), StorageError> {
        let invalid = StorageError::InvalidInstructionData;
        let (owner, rest) = data.split_first_chunk::<32>().ok_or(invalid)?;
        let (kind, rest) = rest.split_first().ok_or(invalid)?;
        let (change, rest) = match *kind {
            enrollment_kind::ENROLL => {
                let (ix, rest) = rest.split_at_checked(EnrollIxData::SIZE).ok_or(invalid)?;
                let (proof, rest) = rest
                    .split_at_checked(CustomRingProof::SIZE)
                    .ok_or(invalid)?;
                let change = EnrollmentChange::Enroll {
                    ix: bytemuck::try_from_bytes(ix).map_err(|_| invalid)?,
                    proof: bytemuck::try_from_bytes(proof).map_err(|_| invalid)?,
                };
                (change, rest)
            }
            enrollment_kind::ROTATE => {
                let (viewing_u, rest) = rest.split_first_chunk::<32>().ok_or(invalid)?;
                let (prior_viewing_u, rest) = rest.split_first_chunk::<32>().ok_or(invalid)?;
                let (prior_seq, rest) = rest.split_first_chunk::<8>().ok_or(invalid)?;
                let change = EnrollmentChange::Rotate {
                    viewing_u,
                    prior_viewing_u,
                    prior_seq,
                };
                (change, rest)
            }
            _ => return Err(invalid),
        };
        let owner = bytemuck::try_from_bytes(owner).map_err(|_| invalid)?;
        Ok((Self { owner, change }, rest))
    }

    pub fn encode_into(&self, data: &mut Vec<u8>) {
        data.extend_from_slice(self.owner.as_ref());
        match self.change {
            EnrollmentChange::Enroll { ix, proof } => {
                data.push(enrollment_kind::ENROLL);
                data.extend_from_slice(bytemuck::bytes_of(ix));
                data.extend_from_slice(bytemuck::bytes_of(proof));
            }
            EnrollmentChange::Rotate {
                viewing_u,
                prior_viewing_u,
                prior_seq,
            } => {
                data.push(enrollment_kind::ROTATE);
                data.extend_from_slice(viewing_u);
                data.extend_from_slice(prior_viewing_u);
                data.extend_from_slice(prior_seq);
            }
        }
    }

    /// What no proof binds, a `RegisterKey` proof covers the rest of an Enroll.
    pub fn check(&self) -> Result<(), StorageError> {
        match self.change {
            EnrollmentChange::Enroll { ix, .. } => ix.check(),
            EnrollmentChange::Rotate { viewing_u, .. } => check_viewing_key(viewing_u),
        }
    }

    pub fn entry(&self) -> Result<TrackingEntry, StorageError> {
        match self.change {
            EnrollmentChange::Enroll { ix, .. } => TrackingEntry::from_enroll(self.owner, ix),
            EnrollmentChange::Rotate {
                viewing_u,
                prior_viewing_u,
                prior_seq,
            } => TrackingEntry::from_rotation(
                self.owner,
                viewing_u,
                prior_viewing_u,
                u64::from_le_bytes(*prior_seq),
            ),
        }
    }

    /// The entry hash the apply chains, and the text binding it to the registry so no other ring applies the signature.
    /// Values are base58, and the owner is the signing key the entry hash also binds.
    pub fn signed(&self, registry: &Address) -> Result<SignedWrite, StorageError> {
        let entry_hash = self
            .entry()?
            .hash()
            .map_err(|_| StorageError::HashingFailed)?;
        let action = match self.change {
            EnrollmentChange::Enroll { .. } => ENROLL_ACTION,
            EnrollmentChange::Rotate { .. } => ROTATE_ACTION,
        };
        let (mut registry_text, mut entry_text) = ([0; BASE58_MAX_LEN], [0; BASE58_MAX_LEN]);
        let registry_len = usize::from(five8::encode_32(registry.as_array(), &mut registry_text));
        let entry_len = usize::from(five8::encode_32(&entry_hash, &mut entry_text));
        let parts: [&[u8]; 7] = [
            domain::ENROLLMENT_SIGNATURE_DOMAIN.as_bytes(),
            REGISTRY_LINE,
            &registry_text[..registry_len],
            ENTRY_LINE,
            &entry_text[..entry_len],
            ACTION_LINE,
            &action[..],
        ];
        let mut message = ArrayVec::new();
        for part in parts {
            message
                .try_extend_from_slice(part)
                .map_err(|_| StorageError::InvalidInstructionData)?;
        }
        Ok(SignedWrite {
            entry_hash,
            message,
        })
    }
}

impl<'a> ApplyEnrollmentsRequest<'a> {
    /// The header and exactly `count` writes.
    pub fn parse(data: &'a [u8]) -> Result<Self, StorageError> {
        let (ix, writes) = data
            .split_at_checked(ApplyEnrollmentsIxData::SIZE)
            .ok_or(StorageError::InvalidInstructionData)?;
        let ix: &ApplyEnrollmentsIxData =
            bytemuck::try_from_bytes(ix).map_err(|_| StorageError::InvalidInstructionData)?;
        ApplyEnrollmentsIxData::check_count(ix.count)?;
        Ok(Self {
            ix,
            writes: parse_writes(ix.count, writes)?,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut data = bytemuck::bytes_of(self.ix).to_vec();
        self.writes
            .iter()
            .for_each(|write| write.encode_into(&mut data));
        data
    }
}

impl ApplyEnrollmentsIxData {
    pub const SIZE: usize = core::mem::size_of::<Self>();

    pub const fn new_next_index(&self) -> u64 {
        u64::from_le_bytes(self.new_next_index)
    }

    /// One to `ENROLLMENT_BATCH` writes.
    pub fn check_count(count: u8) -> Result<(), StorageError> {
        match usize::from(count) {
            0 => Err(StorageError::NoEnrollmentsNamed),
            1..=ENROLLMENT_BATCH => Ok(()),
            _ => Err(StorageError::BatchTooLarge),
        }
    }
}

impl OwnerGrantIxData {
    pub const SIZE: usize = core::mem::size_of::<Self>();
}

impl<'a> StorageEvent<'a> {
    pub fn encode(&self) -> Vec<u8> {
        let Self::EnrollmentsApplied { header, writes } = self;
        let mut data = vec![tag::EMIT_EVENT, event_kind::ENROLLMENTS_APPLIED];
        data.extend_from_slice(bytemuck::bytes_of(header));
        writes.iter().for_each(|write| write.encode_into(&mut data));
        data
    }

    /// `None` unless the tag, the kind, the header and exactly its `count` writes match.
    pub fn decode(data: &'a [u8]) -> Option<Self> {
        let ([tag::EMIT_EVENT, event_kind::ENROLLMENTS_APPLIED], body) =
            data.split_first_chunk::<2>()?
        else {
            return None;
        };
        let (header, writes) =
            body.split_at_checked(core::mem::size_of::<EnrollmentsAppliedEvent>())?;
        let header: EnrollmentsAppliedEvent = bytemuck::try_pod_read_unaligned(header).ok()?;
        Some(Self::EnrollmentsApplied {
            header,
            writes: parse_writes(header.count, writes).ok()?,
        })
    }
}

impl<I> RecordWriteClassification<'_, I>
where
    I: Iterator<Item = Result<RecordWrite, CustomRingError>>,
{
    /// One bit per write whose private mask needs its owner's tracking row.
    pub fn private_writes(self) -> Result<u16, StorageError> {
        let mut assets = self.asset_writes.iter().peekable();
        let mut private_writes = 0u16;
        let mut count = 0;
        for (index, entry) in self.entries.enumerate() {
            // 1. The batch bound fixes the bitmap width, private masks require inclusion regardless of written values.
            count = index + 1;
            storage_write_count(count)?;
            let entry = entry.map_err(|_| StorageError::InvalidInstructionData)?;
            let private = entry.mask & ListSet::PRIVATE_LISTS.record_bits() != 0;
            // 2. Mint preimages exempt only their matching private positions in strictly ascending order.
            if let Some(asset) = assets
                .peek()
                .filter(|asset| usize::from(asset.write_index) <= index)
            {
                if usize::from(asset.write_index) != index
                    || !private
                    || Member::asset(&asset.mint)
                        .map_err(|_| StorageError::InvalidAssetWrite)?
                        .as_bytes()
                        != &entry.member
                {
                    return Err(StorageError::InvalidAssetWrite);
                }
                assets.next();
            } else if private {
                private_writes |= 1u16 << index;
            }
        }
        // 3. Empty batches and asset positions outside the consumed writes have no valid statement.
        storage_write_count(count)?;
        if assets.next().is_some() {
            return Err(StorageError::InvalidAssetWrite);
        }
        Ok(private_writes)
    }
}

impl<'a> StorageUpdateIxData<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, StorageError> {
        // 1. The bounded write count fixes every record boundary before optional proof metadata.
        let (revision, data) = data
            .split_first_chunk::<8>()
            .ok_or(StorageError::InvalidInstructionData)?;
        let (count, rest) = data
            .split_first()
            .ok_or(StorageError::InvalidInstructionData)?;
        let count = usize::from(storage_write_count(usize::from(*count))?);
        let (writes, rest) = rest
            .split_at_checked(count * core::mem::size_of::<RecordWriteData>())
            .ok_or(StorageError::InvalidInstructionData)?;
        // 2. Asset preimages name write positions, their hashes and ordering are checked with the derived bitmap.
        let (asset_count, rest) = rest
            .split_first()
            .ok_or(StorageError::InvalidInstructionData)?;
        if usize::from(*asset_count) > count {
            return Err(StorageError::InvalidAssetWrite);
        }
        let (assets, rest) = rest
            .split_at_checked(usize::from(*asset_count) * core::mem::size_of::<AssetWrite>())
            .ok_or(StorageError::InvalidInstructionData)?;
        let writes: Vec<RecordWriteData> = writes
            .as_chunks::<{ core::mem::size_of::<RecordWriteData>() }>()
            .0
            .iter()
            .map(|bytes| bytemuck::pod_read_unaligned(bytes))
            .collect();
        let asset_writes: Vec<AssetWrite> = assets
            .as_chunks::<{ core::mem::size_of::<AssetWrite>() }>()
            .0
            .iter()
            .map(|bytes| bytemuck::pod_read_unaligned(bytes))
            .collect();
        // 3. The derived bitmap, never a flag, decides whether a tracking revision follows.
        let private_writes = RecordWriteClassification {
            entries: writes.iter().map(RecordWriteData::storage_entry),
            asset_writes: &asset_writes,
        }
        .private_writes()?;
        let (tracking_revision, rest) = match private_writes {
            0 => (None, rest),
            _ => {
                let (revision, rest) = rest
                    .split_first_chunk::<8>()
                    .ok_or(StorageError::InvalidInstructionData)?;
                (Some(Revision(*revision)), rest)
            }
        };
        let apply =
            ApplyRecordsIxData::parse(rest).map_err(|_| StorageError::InvalidInstructionData)?;
        // 4. The restrictive bit has one encoding, the statement binds it.
        if apply.restrictive > 1 {
            return Err(StorageError::InvalidInstructionData);
        }
        Ok(Self {
            revision: Revision(*revision),
            writes,
            asset_writes,
            tracking_revision,
            apply,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, StorageError> {
        let count = storage_write_count(self.writes.len())?;
        if (self.private_writes()? != 0) != self.tracking_revision.is_some() {
            return Err(StorageError::InvalidTrackingRegistry);
        }
        let asset_count =
            u8::try_from(self.asset_writes.len()).map_err(|_| StorageError::InvalidAssetWrite)?;
        let mut data = Vec::new();
        data.extend_from_slice(&self.revision.0);
        data.push(count);
        data.extend_from_slice(bytemuck::cast_slice(&self.writes));
        data.push(asset_count);
        data.extend_from_slice(bytemuck::cast_slice(&self.asset_writes));
        if let Some(revision) = self.tracking_revision {
            data.extend_from_slice(&revision.0);
        }
        data.extend_from_slice(bytemuck::bytes_of(self.apply));
        Ok(data)
    }

    pub fn private_writes(&self) -> Result<u16, StorageError> {
        RecordWriteClassification {
            entries: self.entries(),
            asset_writes: &self.asset_writes,
        }
        .private_writes()
    }

    /// The leaf writes the apply proof hashes, a mint settings tag above 1 refused.
    pub fn entries(&self) -> impl Iterator<Item = Result<RecordWrite, CustomRingError>> + '_ {
        self.writes.iter().map(RecordWriteData::storage_entry)
    }
}

impl RecordWriteData {
    /// The authority's write, the storage program takes no other.
    pub fn storage_entry(&self) -> Result<RecordWrite, CustomRingError> {
        list_and_mint_entry(
            self.member,
            &self.lists,
            &self.mint_policy,
            ListSet::STORAGE_WRITES,
        )
    }
}

/// Exactly `count` writes and no byte after them.
fn parse_writes(count: u8, mut data: &[u8]) -> Result<Vec<EnrollmentWrite<'_>>, StorageError> {
    let writes = (0..count)
        .map(|_| {
            let (write, rest) = EnrollmentWrite::parse(data)?;
            data = rest;
            Ok(write)
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    match data.is_empty() {
        true => Ok(writes),
        false => Err(StorageError::InvalidInstructionData),
    }
}

fn storage_write_count(count: usize) -> Result<u8, StorageError> {
    write_count(count).map_err(|error| match error {
        CustomRingError::AuthorityBatchTooLarge => StorageError::BatchTooLarge,
        _ => StorageError::InvalidInstructionData,
    })
}

const _: () = assert!(core::mem::align_of::<AssetWrite>() == 1);
const _: () = assert!(core::mem::size_of::<AssetWrite>() == 33);
const _: () = assert!(core::mem::align_of::<InitializeIxData>() == 1);
const _: () = assert!(InitializeIxData::SIZE == 104);
const _: () = assert!(core::mem::align_of::<InitializeTrackingIxData>() == 1);
const _: () = assert!(InitializeTrackingIxData::SIZE == 65);
const _: () = assert!(core::mem::align_of::<EnrollIxData>() == 1);
const _: () = assert!(EnrollIxData::SIZE == 192);
const _: () = assert!(core::mem::align_of::<ApplyEnrollmentsIxData>() == 1);
const _: () = assert!(ApplyEnrollmentsIxData::SIZE == 241);
const _: () = assert!(core::mem::align_of::<EnrollmentsAppliedEvent>() == 1);
const _: () = assert!(core::mem::size_of::<EnrollmentsAppliedEvent>() == 81);
const _: () = assert!(SIGNED_MESSAGE_MAX_LEN == 144);
const _: () = assert!(core::mem::align_of::<OwnerGrantIxData>() == 1);
const _: () = assert!(OwnerGrantIxData::SIZE == 35);
