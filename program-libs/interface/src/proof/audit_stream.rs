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
//! Each output record is its own stream under its slot's delivered secret. Every transfer slot seals
//! [`TRANSFER_RECORD_LEN`] bytes and publishes the prefix its kind takes under the frame's [`RecordForm`].
//!
//! Read after `program-libs/interface/src/proof/domain.rs`. Read before `program-libs/interface/src/proof/disclosure.rs`,
//! `prover/circuits/audit/audit.go`.

use crate::error::CustomRingError;

pub const STREAM_BLOCK_LEN: usize = 16;
pub const STREAM_FIRST_COUNTER: u32 = 2;
/// The sealed `tx_viewing_sk`, the envelope's first bytes.
pub const ENVELOPE_KEY_LEN: usize = 32;
/// `R`, spent notes among the published inputs.
pub const REAL_INPUT_COUNT_LEN: usize = 1;
pub const ENVELOPE_HEAD_LEN: usize = ENVELOPE_KEY_LEN + REAL_INPUT_COUNT_LEN;
/// Keys with entry capacity alone, bit `k` set when note `k` is a default-ring note.
pub const DEFAULT_INPUT_BITS_LEN: usize = 8;
/// `u8(kind) || u64be(amount) || owner_key || mint || fe32(blinding)`, what a non-exit slot publishes under a clear form.
pub const RING_RECORD_LEN: usize = 105;
/// The ring record then `fe32(nullifier_pk)`, what an exit publishes under a clear form.
pub const EXIT_RECORD_LEN: usize = RING_RECORD_LEN + FIELD_LEN;
/// The plaintext every transfer slot seals, an exit's key then its `data_hash`, any other slot's two hashes.
pub const TRANSFER_RECORD_LEN: usize = EXIT_RECORD_LEN + FIELD_LEN;
pub const ENTRY_RECORD_LEN: usize = 136;
/// `head || fe32(nullifier_pk) || seed`.
pub const MERGE_RECORD_LEN: usize = EXIT_RECORD_LEN + FIELD_LEN;
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
    /// The frame publishes the voucher's key.
    pub vouched: bool,
    pub records: RecordForm,
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

    /// The voucher's key on the blocks past the last slot's, no slot key's counter moves for it.
    pub const fn voucher(self) -> StreamRegion {
        StreamRegion {
            start_counter: self.first_key_counter() + (self.capacity * SEALED_KEY_BLOCKS) as u32,
            max_blocks: SEALED_KEY_BLOCKS,
            len: match self.vouched {
                true => SEALED_KEY_LEN,
                false => 0,
            },
        }
    }

    pub const fn keys_len(self) -> usize {
        SEALED_KEY_LEN * self.outputs + self.voucher().len
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

/// The hashes a frame's records publish at one length per kind, the proof holds an unpublished hash to zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordForm {
    /// Every record publishes `data_hash`.
    pub data: bool,
    /// Every non-exit record publishes `ring_data_hash`.
    pub ring_data: bool,
}

impl RecordForm {
    /// The prefix of [`TRANSFER_RECORD_LEN`] a slot of `kind` publishes, a dummy as a ring slot.
    pub const fn published_len(self, kind: RecordKind) -> usize {
        match kind {
            RecordKind::Exit => EXIT_RECORD_LEN + FIELD_LEN * self.data as usize,
            RecordKind::Dummy | RecordKind::Ring => {
                RING_RECORD_LEN + FIELD_LEN * (self.data as usize + self.ring_data as usize)
            }
        }
    }

    /// A data frame's payload follows the record at its length prefix or longer, any other body ends at the record.
    pub const fn fits_recipient_body(self, body_len: usize) -> bool {
        let record_len = self.published_len(RecordKind::Ring);
        match self.data {
            true => body_len >= record_len + RECIPIENT_PAYLOAD_LENGTH_LEN,
            false => body_len == record_len,
        }
    }
}
