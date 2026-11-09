//! Mirror of `prover/circuits/audit`.
//!
//! SPP publishes these bytes and opens none of them. The proof holds each output record to its slot's
//! note and the auditor stream to the transaction's viewing key, so every length and counter here must
//! match the circuit, a key read at another counter decrypts to noise. A key fixes the counters by its
//! capacity, and a frame transmits its published slots alone.
//!
//! The encrypted envelope holds the viewing secret in bytes 0 through 31 and note count `R` at byte 32.
//! An entry envelope adds the big endian default bitmap in bytes 33 through 40, entry records start at byte 41.
//! The recipient keys follow at the first counter past the envelope's reserved blocks, two blocks each.
//! Each output record is its own stream under its slot's delivered secret.
//!
//! Read after `program-libs/interface/src/proof/domain.rs`. Read before `program-libs/interface/src/proof/disclosure.rs`,
//! `prover/circuits/audit/audit.go`.

use spr_interface::ZERO_NULLIFIER_PUBKEY;

use crate::{bytes::Bytes, error::CustomRingError};

pub const STREAM_BLOCK_LEN: usize = 16;
pub const STREAM_FIRST_COUNTER: u32 = 2;
/// The sealed `tx_viewing_sk`, the envelope's first bytes.
pub const ENVELOPE_KEY_LEN: usize = 32;
/// `R`, spent notes among the published inputs.
pub const REAL_INPUT_COUNT_LEN: usize = 1;
pub const ENVELOPE_HEAD_LEN: usize = ENVELOPE_KEY_LEN + REAL_INPUT_COUNT_LEN;
/// Keys with entry capacity alone, bit `k` set when note `k` is a default-ring note.
pub const DEFAULT_INPUT_BITS_LEN: usize = 8;
/// `u8(kind) || u64be(amount) || owner_key || mint || fe32(blinding)`, what a non-exit slot publishes.
pub const RING_RECORD_LEN: usize = 105;
/// The ring record then `fe32(nullifier_pk)`, what an exit publishes and every slot seals.
pub const EXIT_RECORD_LEN: usize = RING_RECORD_LEN + FIELD_LEN;
pub const ENTRY_RECORD_LEN: usize = 136;
pub const MERGE_RECORD_LEN: usize = EXIT_RECORD_LEN + FIELD_LEN;
/// Set in the kind byte exactly when a live record's key is `ZERO_NULLIFIER_PUBKEY`.
pub const RECORD_ZERO_KEY_BIT: u8 = 4;
/// `eph_sk || recipient_u`, sealed to the auditor beside a merge record.
pub const MERGE_ENVELOPE_LEN: usize = 2 * FIELD_LEN;
/// One sealed recipient key, its region holds no padding.
pub const SEALED_KEY_LEN: usize = FIELD_LEN;
pub const SEALED_KEY_BLOCKS: usize = SEALED_KEY_LEN / STREAM_BLOCK_LEN;
/// The `u16be` length leading a recipient payload.
pub const RECIPIENT_PAYLOAD_LENGTH_LEN: usize = 2;
/// `viewing_u || owner_key || nullifier_pk`, one byte for the wincode of no data.
pub const MIN_REPLY_LEN: usize = 97;
const FIELD_LEN: usize = 32;
const AMOUNT_LEN: usize = 8;

const _: () = assert!(1 + AMOUNT_LEN + 3 * FIELD_LEN == RING_RECORD_LEN);
const _: () = assert!(AMOUNT_LEN + 4 * FIELD_LEN == ENTRY_RECORD_LEN);
const _: () = assert!(SEALED_KEY_LEN.is_multiple_of(STREAM_BLOCK_LEN));
const _: () = assert!(3 * FIELD_LEN + 1 == MIN_REPLY_LEN);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditKey {
    /// The widest published input count.
    pub class: usize,
    pub entry: bool,
}

impl AuditKey {
    pub const fn class(self) -> usize {
        self.class
    }

    /// `S`, the owner signers the key's statement chains.
    pub const fn signer_slots(self) -> usize {
        self.class
    }

    const fn head_len(self) -> usize {
        ENVELOPE_HEAD_LEN
            + if self.entry {
                DEFAULT_INPUT_BITS_LEN
            } else {
                0
            }
    }

    pub const fn entry_len(self) -> usize {
        match self.entry {
            true => ENTRY_RECORD_LEN,
            false => 0,
        }
    }

    /// `E`, the same at every published input count so no key's counter moves.
    pub const fn envelope_blocks(self) -> usize {
        (self.head_len() + self.entry_len() * self.class).div_ceil(STREAM_BLOCK_LEN)
    }

    /// `K0`.
    pub const fn first_key_counter(self) -> u32 {
        STREAM_FIRST_COUNTER + self.envelope_blocks() as u32
    }
}

/// One stretch of the auditor keystream, no counter serves two regions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamRegion {
    pub start_counter: u32,
    pub max_blocks: usize,
    /// Transmitted bytes, padding and unused blocks never leave the prover.
    pub len: usize,
}

impl StreamRegion {
    /// Where the region starts in a keystream read from the first counter.
    pub const fn keystream_offset(self) -> usize {
        (self.start_counter - STREAM_FIRST_COUNTER) as usize * STREAM_BLOCK_LEN
    }
}

/// Every length of one transfer's auditor stream, a frame of any other length is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditStreamLayout {
    pub key: AuditKey,
    /// `n`, the frame's nullifier count, never the key capacity or a private live count.
    pub inputs: usize,
    /// `m`, the published slots.
    pub outputs: usize,
    /// `N`, the key's output capacity, the key region spans every slot of it.
    pub capacity: usize,
}

impl AuditStreamLayout {
    pub fn checked(self) -> Result<Self, CustomRingError> {
        match self.inputs >= 1
            && self.inputs <= self.key.class()
            && self.key.class() <= u64::BITS as usize
            && self.outputs >= 1
            && self.outputs <= self.capacity
        {
            true => Ok(self),
            false => Err(CustomRingError::RingTransactLength),
        }
    }

    pub const fn first_key_counter(self) -> u32 {
        self.key.first_key_counter()
    }

    /// Slots at or past `n` stay absent from the evidence.
    pub const fn envelope(self) -> StreamRegion {
        StreamRegion {
            start_counter: STREAM_FIRST_COUNTER,
            max_blocks: self.key.envelope_blocks(),
            len: self.key.head_len() + self.key.entry_len() * self.inputs,
        }
    }

    /// A slot at or past `m` transmits no byte, the proof forces it empty.
    pub const fn recipient_key(self, slot: usize) -> StreamRegion {
        StreamRegion {
            start_counter: self.first_key_counter() + (slot * SEALED_KEY_BLOCKS) as u32,
            max_blocks: SEALED_KEY_BLOCKS,
            len: match slot < self.outputs {
                true => SEALED_KEY_LEN,
                false => 0,
            },
        }
    }

    pub const fn keys_len(self) -> usize {
        SEALED_KEY_LEN * self.outputs
    }

    pub const fn sealed_len(self) -> usize {
        self.envelope().len + self.keys_len()
    }

    /// The blocks a reader decrypts.
    pub const fn capacity_blocks(self) -> usize {
        self.key.envelope_blocks() + self.capacity * SEALED_KEY_BLOCKS
    }
}

/// The circuit derives the kind from the slot's domain and ring id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum RecordKind {
    /// The auditor rebuilds its hash from the derived blinding and the tree id.
    #[default]
    Dummy = 0,
    Ring = 1,
    Exit = 2,
}

impl RecordKind {
    pub const fn published_len(self) -> usize {
        match self {
            Self::Exit => EXIT_RECORD_LEN,
            Self::Dummy | Self::Ring => RING_RECORD_LEN,
        }
    }
}

/// A non-exit record names its key by the zero key bit alone, the proof derives the bit from the committed key.
/// A ring note spends under a nonzero key only as its owner's registered tracking key, so an accepted
/// nonzero-key spend matches the owner's immutable registered key, the one a reader takes from the owner's row.
/// An exit publishes its key, the default ring registers nobody.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RecordKey {
    /// `ZERO_NULLIFIER_PUBKEY`, named by [`RECORD_ZERO_KEY_BIT`].
    Zero,
    /// A ring output's nonzero key, its owner's registered key, or a dummy's.
    #[default]
    Unpublished,
    /// A nonzero key the record carries, an exit's or a merge output's.
    Carried([u8; 32]),
}

impl RecordKey {
    const fn sealed_bytes(self) -> [u8; 32] {
        match self {
            Self::Zero => ZERO_NULLIFIER_PUBKEY,
            Self::Unpublished => [0; 32],
            Self::Carried(key) => key,
        }
    }

    fn read(zero_key: bool, carried: Option<[u8; 32]>) -> Result<Self, CustomRingError> {
        let key = match carried {
            None if zero_key => Self::Zero,
            None => Self::Unpublished,
            Some(key) if zero_key == (key == ZERO_NULLIFIER_PUBKEY) => match zero_key {
                true => Self::Zero,
                false => Self::Carried(key),
            },
            Some(_) => return Err(CustomRingError::RingTransactLength),
        };
        Ok(key)
    }
}

/// One slot's record, the proof holds a dummy's plaintext to zero.
/// SPP commits the owner key and the mint as hashes no reader inverts, so the record carries the raw values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OutputRecord {
    pub kind: RecordKind,
    pub amount: u64,
    pub owner_key: [u8; 32],
    pub mint: [u8; 32],
    /// SPP's derived blinding, the recipient holds no viewing secret to derive it.
    pub blinding: [u8; 32],
    pub key: RecordKey,
}

impl OutputRecord {
    /// The sealed plaintext through [`Self::published_len`], an unpublished key encodes as zero past it.
    pub fn encode(&self) -> [u8; EXIT_RECORD_LEN] {
        let mut out = [0; EXIT_RECORD_LEN];
        if self.kind == RecordKind::Dummy {
            return out;
        }
        let key = self.key.sealed_bytes();
        out[0] = self.kind as u8 | (u8::from(key == ZERO_NULLIFIER_PUBKEY) * RECORD_ZERO_KEY_BIT);
        out[1..9].copy_from_slice(&self.amount.to_be_bytes());
        out[9..41].copy_from_slice(&self.owner_key);
        out[41..73].copy_from_slice(&self.mint);
        out[73..RING_RECORD_LEN].copy_from_slice(&self.blinding);
        out[RING_RECORD_LEN..].copy_from_slice(&key);
        out
    }

    pub const fn published_len(&self) -> usize {
        self.kind.published_len()
    }

    /// Refuses bytes off the kind's published length.
    pub fn decode(published: &[u8]) -> Result<Self, CustomRingError> {
        read_all(published, |bytes| {
            let head = RecordHead::read(bytes)?;
            let carried = match head.record.kind {
                RecordKind::Exit => Some(*bytes.array()?),
                RecordKind::Dummy | RecordKind::Ring => None,
            };
            head.with_key(carried)
        })
    }
}

/// The published prefix of a record, before its key.
struct RecordHead {
    record: OutputRecord,
    zero_key: bool,
}

impl RecordHead {
    fn read(bytes: &mut Bytes<'_>) -> Result<Self, CustomRingError> {
        let kind_byte = bytes.byte()?;
        let kind = match kind_byte & !RECORD_ZERO_KEY_BIT {
            0 => RecordKind::Dummy,
            1 => RecordKind::Ring,
            2 => RecordKind::Exit,
            _ => return Err(CustomRingError::RingTransactLength),
        };
        Ok(Self {
            record: OutputRecord {
                kind,
                amount: read_amount(bytes)?,
                owner_key: *bytes.array()?,
                mint: *bytes.array()?,
                blinding: *bytes.array()?,
                key: RecordKey::Unpublished,
            },
            zero_key: kind_byte & RECORD_ZERO_KEY_BIT != 0,
        })
    }

    /// A dummy carries no byte and no key, every other key follows the zero key bit.
    fn with_key(self, carried: Option<[u8; 32]>) -> Result<OutputRecord, CustomRingError> {
        if self.record.kind == RecordKind::Dummy {
            return match self.record == OutputRecord::default()
                && !self.zero_key
                && carried.is_none_or(|key| key == [0; 32])
            {
                true => Ok(self.record),
                false => Err(CustomRingError::RingTransactLength),
            };
        }
        Ok(OutputRecord {
            key: RecordKey::read(self.zero_key, carried)?,
            ..self.record
        })
    }
}

/// A merge output's record, its seed rebuilds the blinding with no viewing secret.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MergeRecord {
    pub record: OutputRecord,
    pub seed: [u8; 32],
}

impl MergeRecord {
    /// The full record form, the merge output's key travels with its seed.
    pub fn encode(&self) -> [u8; MERGE_RECORD_LEN] {
        let mut out = [0; MERGE_RECORD_LEN];
        out[..EXIT_RECORD_LEN].copy_from_slice(&self.record.encode());
        out[EXIT_RECORD_LEN..].copy_from_slice(&self.seed);
        out
    }

    pub fn decode(plaintext: &[u8]) -> Result<Self, CustomRingError> {
        read_all(plaintext, |bytes| {
            let head = RecordHead::read(bytes)?;
            let carried = *bytes.array()?;
            Ok(Self {
                record: head.with_key(Some(carried))?,
                seed: *bytes.array()?,
            })
        })
    }
}

/// What the auditor reads first of a merge, the key pair that opens its record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MergeEnvelope {
    pub eph_sk: [u8; 32],
    pub recipient_u: [u8; 32],
}

impl MergeEnvelope {
    pub fn encode(&self) -> [u8; MERGE_ENVELOPE_LEN] {
        let mut out = [0; MERGE_ENVELOPE_LEN];
        out[..FIELD_LEN].copy_from_slice(&self.eph_sk);
        out[FIELD_LEN..].copy_from_slice(&self.recipient_u);
        out
    }

    pub fn decode(plaintext: &[u8]) -> Result<Self, CustomRingError> {
        read_all(plaintext, |bytes| {
            Ok(Self {
                eph_sk: *bytes.array()?,
                recipient_u: *bytes.array()?,
            })
        })
    }
}

/// One default-ring input's record, zero on any other slot, never its blinding or nullifier secret.
/// A default-ring note can lack an earlier ring record, `input_commitment` names it in place of the
/// blinding and the proof holds every other field to its preimage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntryRecord {
    pub owner_key: [u8; 32],
    pub nullifier_pk: [u8; 32],
    pub amount: u64,
    pub mint: [u8; 32],
    /// SPP's spent UTXO hash, the value `private_tx_hash` chains.
    pub input_commitment: [u8; 32],
}

impl EntryRecord {
    fn extend(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.owner_key);
        out.extend_from_slice(&self.nullifier_pk);
        out.extend_from_slice(&self.amount.to_be_bytes());
        out.extend_from_slice(&self.mint);
        out.extend_from_slice(&self.input_commitment);
    }

    fn read(bytes: &mut Bytes<'_>) -> Result<Self, CustomRingError> {
        Ok(Self {
            owner_key: *bytes.array()?,
            nullifier_pk: *bytes.array()?,
            amount: read_amount(bytes)?,
            mint: *bytes.array()?,
            input_commitment: *bytes.array()?,
        })
    }
}

/// Entry evidence follows the sealed viewing key and note count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditEvidence {
    None,
    Entry {
        /// Bit `k` names note `k`.
        default_note_bits: u64,
        /// One per published input, record `k` names note `k`.
        inputs: Vec<EntryRecord>,
    },
}

/// The auditor derives every output blinding from the sealed viewing secret, the published salt and `n0`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditEnvelope {
    pub tx_viewing_secret: [u8; ENVELOPE_KEY_LEN],
    pub real_inputs: u8,
    pub evidence: AuditEvidence,
}

impl AuditEnvelope {
    /// Writes the raw fields, use `decode` to check the counts and the zero records.
    pub fn encode(&self) -> Vec<u8> {
        let evidence_len = match &self.evidence {
            AuditEvidence::None => 0,
            AuditEvidence::Entry { inputs, .. } => {
                DEFAULT_INPUT_BITS_LEN + ENTRY_RECORD_LEN * inputs.len()
            }
        };
        let mut out = Vec::with_capacity(ENVELOPE_HEAD_LEN + evidence_len);
        out.extend_from_slice(&self.tx_viewing_secret);
        out.push(self.real_inputs);
        if let AuditEvidence::Entry {
            default_note_bits,
            inputs,
        } = &self.evidence
        {
            out.extend_from_slice(&default_note_bits.to_be_bytes());
            inputs.iter().for_each(|input| input.extend(&mut out));
        }
        out
    }

    /// Refuses lengths and counts outside `layout`, ciphertext authentication remains with the caller.
    pub fn decode(plaintext: &[u8], layout: AuditStreamLayout) -> Result<Self, CustomRingError> {
        // 1. The selected layout bounds both counts, read_all refuses bytes after its final record.
        layout.checked()?;
        read_all(plaintext, |bytes| {
            let tx_viewing_secret = *bytes.array()?;
            let real_inputs = bytes.byte()?;
            let evidence = match layout.key.entry {
                false => AuditEvidence::None,
                true => AuditEvidence::Entry {
                    default_note_bits: u64::from_be_bytes(*bytes.array()?),
                    inputs: (0..layout.inputs)
                        .map(|_| EntryRecord::read(bytes))
                        .collect::<Result<_, CustomRingError>>()?,
                },
            };
            // 2. R notes fit the published inputs, the circuit holds every note below n.
            if usize::from(real_inputs) > layout.inputs {
                return Err(CustomRingError::RingTransactLength);
            }
            // 3. Default bits select notes below R, every other record is zero as the circuit's note selector holds it.
            // Selected values remain unverified here, the ring proof binds them to the input commitments.
            if let AuditEvidence::Entry {
                default_note_bits,
                inputs,
            } = &evidence
            {
                if default_note_bits
                    .checked_shr(u32::from(real_inputs))
                    .unwrap_or(0)
                    != 0
                {
                    return Err(CustomRingError::RingTransactLength);
                }
                for (note, record) in inputs.iter().enumerate() {
                    if default_note_bits & (1 << note) == 0 && *record != EntryRecord::default() {
                        return Err(CustomRingError::RingTransactLength);
                    }
                }
            }
            Ok(Self {
                tx_viewing_secret,
                real_inputs,
                evidence,
            })
        })
    }
}

/// The recipient keys the auditor stream seals, slot `i` at key `i`.
/// The auditor reads each `V_i` here, no directory names a recipient.
pub fn encode_recipient_keys(keys: &[[u8; 32]]) -> Vec<u8> {
    keys.as_flattened().to_vec()
}

pub fn decode_recipient_keys(
    plaintext: &[u8],
    layout: AuditStreamLayout,
) -> Result<Vec<[u8; 32]>, CustomRingError> {
    read_all(plaintext, |bytes| {
        Ok(bytes.fields(layout.outputs)?.to_vec())
    })
}

/// The unproven part of a recipient body, after the record under the same stream.
/// It takes the keystream from [`RING_RECORD_LEN`], where the record's sealed key tail stays
/// unpublished, so no keystream byte covers two published bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecipientPayload {
    /// The commitment covers it, a nonzero value travels with the note.
    pub ring_data_hash: Option<[u8; 32]>,
    /// Empty, or `data_hash || application_data`.
    pub data: Vec<u8>,
}

impl RecipientPayload {
    /// `u16be(len) || u8(has_ring_data) || ring_data_hash? || data`, unpadded.
    pub fn encoded_len(&self) -> usize {
        RECIPIENT_PAYLOAD_LENGTH_LEN
            + 1
            + self.ring_data_hash.map_or(0, |_| FIELD_LEN)
            + self.data.len()
    }

    /// Zero padded to `width`, one width per frame keeps the payloads alike.
    pub fn encode_padded(&self, width: usize) -> Result<Vec<u8>, CustomRingError> {
        if width < self.encoded_len() || width > usize::from(u16::MAX) {
            return Err(CustomRingError::OutputBodyLength);
        }
        let length = u16::try_from(self.encoded_len() - RECIPIENT_PAYLOAD_LENGTH_LEN)
            .map_err(|_| CustomRingError::OutputBodyLength)?;
        let mut out = Vec::with_capacity(width);
        out.extend_from_slice(&length.to_be_bytes());
        out.push(u8::from(self.ring_data_hash.is_some()));
        if let Some(ring_data_hash) = self.ring_data_hash {
            out.extend_from_slice(&ring_data_hash);
        }
        out.extend_from_slice(&self.data);
        out.resize(width, 0);
        Ok(out)
    }

    pub fn decode(plaintext: &[u8]) -> Result<Self, CustomRingError> {
        let mut bytes = Bytes::new(plaintext, CustomRingError::RingTransactLength);
        let length = usize::from(u16::from_be_bytes(*bytes.array()?));
        let mut payload = Bytes::new(bytes.take(length)?, CustomRingError::RingTransactLength);
        if bytes.rest().iter().any(|byte| *byte != 0) {
            return Err(CustomRingError::RingTransactLength);
        }
        let ring_data_hash = match payload.byte()? {
            0 => None,
            1 => Some(*payload.array()?),
            _ => return Err(CustomRingError::RingTransactLength),
        };
        Ok(Self {
            ring_data_hash,
            data: payload.rest().to_vec(),
        })
    }
}

/// One length at every `m`.
pub const SENDER_BUNDLE_LEN: usize = FIELD_LEN;

/// `output_seed` outside the proof, the sender reads each `V_i` from the auditor stream, the key record `i` is sealed to.
/// Sealed to the sender's viewing key at index `m`, an opening marks a candidate the sender checks further.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SenderBundle {
    pub output_seed: [u8; 32],
}

impl SenderBundle {
    pub const fn encode(&self) -> [u8; SENDER_BUNDLE_LEN] {
        self.output_seed
    }

    pub fn decode(plaintext: &[u8]) -> Result<Self, CustomRingError> {
        read_all(plaintext, |bytes| {
            Ok(Self {
                output_seed: *bytes.array()?,
            })
        })
    }
}

fn read_amount(bytes: &mut Bytes<'_>) -> Result<u64, CustomRingError> {
    Ok(u64::from_be_bytes(*bytes.array()?))
}

fn read_all<'a, T>(
    plaintext: &'a [u8],
    read: impl FnOnce(&mut Bytes<'a>) -> Result<T, CustomRingError>,
) -> Result<T, CustomRingError> {
    let mut bytes = Bytes::new(plaintext, CustomRingError::RingTransactLength);
    let value = read(&mut bytes)?;
    match bytes.rest().is_empty() {
        true => Ok(value),
        false => Err(CustomRingError::RingTransactLength),
    }
}
