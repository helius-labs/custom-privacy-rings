//! Every ring circuit and the shape of each transfer key, no key names and no verifying keys.
//!
//! A frame names its key by shape, and a key's widths fix its statement chains, its stream and its
//! record pools. A frame off the catalog has no key and `RingShapeUnkeyed` refuses it.

use crate::{
    audit_stream::AuditKey, member_registry::APPLY_BATCH, storage_instruction::ENROLLMENT_BATCH,
    transact::RingRail,
};

pub use crate::ring_keys::TRANSFER_SHAPES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingCircuit {
    /// Every rail, the statement's bits name it.
    Transfer {
        outputs: u8,
        /// The widest published input count.
        inputs: u8,
        withdrawals: u8,
        owners: u8,
        /// One evidence slot per input, the key also spends default-ring notes.
        entry: bool,
        /// A public record and the records under the owner's salt and the ring salt fit.
        wide: bool,
    },
    ApplyRecords {
        writes: u8,
        mode: ApplyMode,
    },
    ApplyEnrollments {
        writes: u8,
    },
    RegisterKey,
    Merge,
}

/// Mirror `Key.Private` in `prover/ring/keys.go`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyMode {
    /// No write needs a tracking row, the key compiles no row opening.
    Public,
    /// At least one private owner write, the key compiles a tracking row opening per write slot.
    Private,
}

/// One transfer key, mirror `Shapes` in `prover/circuits/transfer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferShape {
    pub outputs: u8,
    pub inputs: u8,
    pub withdrawals: u8,
    pub owners: u8,
    pub entry: bool,
    pub wide: bool,
    /// Header wide bit, clear on narrow entry keys whose pools stay wide.
    pub frame_wide: bool,
    /// The most tracking rows one inline transaction on the key opens.
    pub tracking: u8,
    /// Excludes input and public slot assets.
    pub records: u8,
    pub pools: RecordPools,
}

/// What names a frame's key, nothing else moves a recipient key's counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameKey {
    pub rail: RingRail,
    /// `n`, the frame's nullifier count.
    pub inputs: usize,
    pub outputs: usize,
    pub spends_default_ring: bool,
    pub wide: bool,
}

impl FrameKey {
    pub fn shape(self) -> Option<TransferShape> {
        // SPP's authority rail holds every input to the ring.
        let delegate = matches!(self.rail, RingRail::Delegate | RingRail::AnonymousDelegate);
        if delegate && self.spends_default_ring {
            return None;
        }
        // Rows of one family ascend in outputs, a frame takes the first row holding its SPP width.
        // No wide row holds 16 outputs, each owner row and output takes two salted reads.
        // The wide flag takes the first row holding the frame, the last only past 24 inputs.
        TRANSFER_SHAPES.into_iter().find(|shape| {
            shape.entry == self.spends_default_ring
                && shape.frame_wide == self.wide
                && self.outputs <= usize::from(shape.outputs)
                && self.inputs <= usize::from(shape.inputs)
        })
    }
}

impl TransferShape {
    pub fn holds(self, spend: TransferSpend) -> bool {
        (1..=usize::from(self.inputs)).contains(&spend.inputs)
            && spend.legs <= usize::from(self.withdrawals)
            && spend.owners <= usize::from(self.owners)
            && spend.pools.within(self.pools)
    }

    pub const fn audit_key(self) -> AuditKey {
        AuditKey {
            class: self.inputs as usize,
            entry: self.entry,
        }
    }

    pub const fn circuit(self) -> RingCircuit {
        RingCircuit::Transfer {
            outputs: self.outputs,
            inputs: self.inputs,
            withdrawals: self.withdrawals,
            owners: self.owners,
            entry: self.entry,
            wide: self.wide,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferSpend {
    pub rail: RingRail,
    /// Published inputs, the spent notes.
    pub inputs: usize,
    pub outputs: usize,
    pub spends_default_ring: bool,
    /// Public withdrawal legs.
    pub legs: usize,
    /// Distinct live input owners, rows the key opens.
    pub owners: usize,
    /// Distinct records the spend opens in each pool, a salted read takes a row beside the public one.
    pub pools: RecordPools,
    /// The header's wide flag, `fitted` derives it from the spend.
    pub wide: bool,
}

impl TransferSpend {
    pub const fn frame_key(self) -> FrameKey {
        FrameKey {
            rail: self.rail,
            inputs: self.inputs,
            outputs: self.outputs,
            spends_default_ring: self.spends_default_ring,
            wide: self.wide,
        }
    }
}

impl RingCircuit {
    /// The one transfer key serving the spend, `None` when the catalog keys none.
    pub fn transfer(spend: TransferSpend) -> Option<Self> {
        spend
            .frame_key()
            .shape()
            .filter(|shape| shape.holds(spend))
            .map(TransferShape::circuit)
    }

    pub const fn apply_records(pending: u64, mode: ApplyMode) -> Self {
        Self::ApplyRecords {
            writes: apply_width(pending),
            mode,
        }
    }

    /// `None` outside `1..=ENROLLMENT_BATCH`, the counts `check_count` refuses.
    pub const fn apply_enrollments(pending: u64) -> Option<Self> {
        if pending == 0 || pending > ENROLLMENT_BATCH as u64 {
            return None;
        }
        Some(Self::ApplyEnrollments {
            writes: apply_width(pending),
        })
    }
}

/// The narrowest apply key width holding `pending` writes up to `APPLY_BATCH`, the same for every apply circuit.
pub const fn apply_width(pending: u64) -> u8 {
    match pending {
        0..=1 => 1,
        2 => 2,
        3..=4 => 4,
        5..=8 => 8,
        _ => APPLY_BATCH as u8,
    }
}

/// A transfer key's record pool widths, mirror `recordCapacity` in `prover/circuits/policy/record_pool.go`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordPools {
    pub own: usize,
    pub curator: usize,
    pub tracking: usize,
}

impl RecordPools {
    const fn within(self, capacity: Self) -> bool {
        self.own <= capacity.own
            && self.curator <= capacity.curator
            && self.tracking <= capacity.tracking
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spend(
        rail: RingRail,
        outputs: usize,
        spends_default_ring: bool,
        wide: bool,
    ) -> TransferSpend {
        TransferSpend {
            rail,
            inputs: 1,
            outputs,
            spends_default_ring,
            legs: 0,
            owners: 1,
            pools: RecordPools::default(),
            wide,
        }
    }

    const TWO: RingCircuit = TRANSFER_SHAPES[0].circuit();
    const FOUR: RingCircuit = TRANSFER_SHAPES[1].circuit();
    const EIGHT: RingCircuit = TRANSFER_SHAPES[2].circuit();
    const SIXTEEN: RingCircuit = TRANSFER_SHAPES[3].circuit();
    const WIDE: RingCircuit = TRANSFER_SHAPES[4].circuit();
    const WIDE_INPUTS: RingCircuit = TRANSFER_SHAPES[5].circuit();
    const ENTRY_FOUR: RingCircuit = TRANSFER_SHAPES[6].circuit();
    const ENTRY_EIGHT_TWO: RingCircuit = TRANSFER_SHAPES[7].circuit();
    const ENTRY_EIGHT_FIVE: RingCircuit = TRANSFER_SHAPES[8].circuit();

    #[test]
    fn every_rail_shares_the_key_of_its_output_width() {
        for rail in [
            RingRail::Confidential,
            RingRail::Anonymous,
            RingRail::Delegate,
            RingRail::AnonymousDelegate,
        ] {
            let key = |outputs| RingCircuit::transfer(spend(rail, outputs, false, false));
            assert_eq!(key(2), Some(TWO));
            assert_eq!(key(4), Some(FOUR));
            assert_eq!(key(8), Some(EIGHT));
            assert_eq!(key(16), Some(SIXTEEN));
        }
    }

    #[test]
    fn the_wide_flag_names_the_wide_key_of_the_width() {
        let key =
            |outputs| RingCircuit::transfer(spend(RingRail::Confidential, outputs, false, true));
        assert_eq!(key(2), Some(WIDE));
        assert_eq!(key(8), Some(WIDE));
        assert_eq!(key(16), None);
    }

    #[test]
    fn the_published_inputs_route_a_two_output_wide_spend() {
        let wide_spend = |inputs, legs, owners| {
            RingCircuit::transfer(TransferSpend {
                inputs,
                legs,
                owners,
                ..spend(RingRail::Confidential, 2, false, true)
            })
        };
        assert_eq!(wide_spend(24, 0, 1), Some(WIDE));
        assert_eq!(wide_spend(25, 0, 1), Some(WIDE_INPUTS));
        assert_eq!(wide_spend(40, 4, 9), Some(WIDE_INPUTS));
        assert_eq!(wide_spend(41, 0, 1), None);
        assert_eq!(wide_spend(25, 5, 1), None);
        assert_eq!(wide_spend(25, 0, 10), None);
    }

    #[test]
    fn a_spend_past_its_key_is_unkeyed() {
        let past = |inputs, legs, owners, wide| {
            RingCircuit::transfer(TransferSpend {
                inputs,
                legs,
                owners,
                ..spend(RingRail::Confidential, 2, false, wide)
            })
        };
        assert_eq!(past(24, 4, 4, false), Some(TWO));
        assert_eq!(past(0, 0, 1, false), None);
        assert_eq!(past(25, 0, 1, false), None);
        assert_eq!(past(1, 5, 1, false), None);
        assert_eq!(past(1, 0, 5, false), None);
        assert_eq!(past(1, 5, 5, true), Some(WIDE));
        assert_eq!(past(1, 33, 1, true), None);
        assert_eq!(past(1, 0, 14, true), None);
    }

    #[test]
    fn an_entry_takes_the_first_entry_key_holding_its_width() {
        let entered = |rail, outputs, inputs, owners| {
            RingCircuit::transfer(TransferSpend {
                inputs,
                owners,
                ..spend(rail, outputs, true, false)
            })
        };
        assert_eq!(entered(RingRail::Confidential, 2, 8, 8), Some(ENTRY_FOUR));
        assert_eq!(entered(RingRail::Anonymous, 4, 1, 1), Some(ENTRY_FOUR));
        assert_eq!(entered(RingRail::Anonymous, 8, 2, 2), Some(ENTRY_EIGHT_TWO));
        assert_eq!(
            entered(RingRail::Anonymous, 8, 3, 1),
            Some(ENTRY_EIGHT_FIVE)
        );
        assert_eq!(
            entered(RingRail::Confidential, 8, 4, 4),
            Some(ENTRY_EIGHT_FIVE)
        );
        assert_eq!(
            entered(RingRail::Confidential, 8, 5, 3),
            Some(ENTRY_EIGHT_FIVE)
        );
        assert_eq!(entered(RingRail::Anonymous, 8, 6, 1), None);
        assert_eq!(entered(RingRail::Anonymous, 8, 9, 1), None);
        assert_eq!(entered(RingRail::Delegate, 2, 1, 1), None);
        assert_eq!(entered(RingRail::AnonymousDelegate, 2, 1, 1), None);
    }

    #[test]
    fn a_sixteen_output_payout_has_only_the_narrow_key() {
        for spends_default_ring in [false, true] {
            for wide in [false, true] {
                let key = FrameKey {
                    rail: RingRail::Confidential,
                    inputs: 1,
                    outputs: 16,
                    spends_default_ring,
                    wide,
                };
                let narrow = !spends_default_ring && !wide;
                assert_eq!(key.shape(), narrow.then_some(TRANSFER_SHAPES[3]), "{key:?}");
            }
        }
    }
}
