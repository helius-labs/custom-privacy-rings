//! The audit layout element and the packing of a ciphertext into statement fields, mirror of `prover/circuits/audit/layout.go`.

use spr_hasher::primitives::PACK_BE_CHUNK_BYTES;

use crate::audit_stream::{RecordForm, SEALED_KEY_LEN};

pub const TRANSFER_OUTPUT_CAPACITIES: [usize; 4] = [2, 4, 8, 16];

/// The transfer statement's layout element, mirror `prover/circuits/audit/layout.go`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AuditLayout {
    /// The key carries default bits and one evidence entry per published input.
    pub entry: bool,
    /// The frame publishes the voucher's sealed key after the slot keys.
    pub vouched: bool,
    pub records: RecordForm,
}

impl AuditLayout {
    /// Bit 0 vouched, bit 1 entry, bit 2 data, bit 3 ring data.
    pub const fn value(self) -> u8 {
        self.vouched as u8
            | (self.entry as u8) << 1
            | (self.records.data as u8) << 2
            | (self.records.ring_data as u8) << 3
    }

    /// `None` on a vouched region shorter than one key.
    pub fn split_keys(self, sealed: &[u8]) -> Option<SealedKeys<'_>> {
        if self.vouched {
            sealed
                .split_last_chunk()
                .map(|(slots, voucher)| SealedKeys {
                    slots,
                    voucher: Some(voucher),
                })
        } else {
            Some(SealedKeys {
                slots: sealed,
                voucher: None,
            })
        }
    }
}

/// A frame's sealed key region, the slot keys then the voucher's key on a vouched frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SealedKeys<'a> {
    pub slots: &'a [u8],
    pub voucher: Option<&'a [u8; SEALED_KEY_LEN]>,
}

/// Zero pads the plaintext at its end to whole 31-byte chunks, one chunk per field.
pub fn pack_disclosure(plaintext: &[u8]) -> Vec<[u8; 32]> {
    plaintext
        .chunks(PACK_BE_CHUNK_BYTES)
        .map(|chunk| {
            let mut field = [0u8; 32];
            field[1..1 + chunk.len()].copy_from_slice(chunk);
            field
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_value_is_the_circuit_flag() {
        let layout = |entry, vouched| {
            AuditLayout {
                entry,
                vouched,
                records: RecordForm::default(),
            }
            .value()
        };
        assert_eq!(layout(false, false), 0);
        assert_eq!(layout(false, true), 1);
        assert_eq!(layout(true, false), 2);
        assert_eq!(layout(true, true), 3);
        let records = |data, ring_data| {
            AuditLayout {
                records: RecordForm { data, ring_data },
                ..AuditLayout::default()
            }
            .value()
        };
        assert_eq!(records(true, false), 4);
        assert_eq!(records(false, true), 8);
    }

    #[test]
    fn a_short_tail_chunk_is_left_aligned_and_zero_padded() {
        let plaintext: Vec<u8> = (1..=40).collect();
        let fields = pack_disclosure(&plaintext);
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0][0], 0);
        assert_eq!(fields[0][1..], plaintext[..31]);
        assert_eq!(fields[1][..10], [0, 32, 33, 34, 35, 36, 37, 38, 39, 40]);
        assert_eq!(fields[1][10..], [0u8; 22]);
    }
}
