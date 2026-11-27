//! The proving key of each ring circuit and the widths a spend takes of its key.

use custom_ring_interface::{
    key_catalog::{
        FrameKey, RecordPools, RingCircuit, TransferShape, TransferSpend, TRANSFER_SHAPES,
    },
    PROVING_KEY_SHA256S,
};
use spr_interface::shape::MAX_TRANSACTION_ADDRESSES;

pub use crate::ring_keys::RING_KEYS;

pub trait TransferKeyRow: Sized {
    /// A stream sealed under one row opens under the other, only an entry key's class moves its first key counter.
    #[must_use]
    fn seals_like(self, other: Self) -> bool;

    /// The catalog row of `circuit`, `None` outside the transfer keys.
    #[must_use]
    fn of(circuit: RingCircuit) -> Option<Self>;
}

impl TransferKeyRow for TransferShape {
    fn seals_like(self, other: Self) -> bool {
        self.audit_key().first_key_counter() == other.audit_key().first_key_counter()
    }

    fn of(circuit: RingCircuit) -> Option<Self> {
        TRANSFER_SHAPES
            .into_iter()
            .find(|shape| shape.circuit() == circuit)
    }
}

/// Addresses SPP takes beside its nullifier PDAs, zolana/docs/spec.md.
const SPP_ADDRESSES: usize = 4;
/// Ring program, ring, ring config, own, curator, tracking.
const RING_ADDRESSES: usize = 6;

/// The only spend input that moves a row's address cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowAuthorization {
    /// Every row signs in the owner run.
    Run,
    /// One row signs inside SPP's anonymous proof and spends ring notes only.
    InProof,
    /// One row hides, every withdrawal names one destination.
    Hidden,
}

impl RowAuthorization {
    /// Rows outside the owner run, each takes its nullifier PDA alone.
    pub const fn unsigned_rows(self) -> usize {
        match self {
            Self::Run => 0,
            Self::InProof | Self::Hidden => 1,
        }
    }
}

/// What takes a v1 transaction's addresses beside its owner signers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransferAddresses {
    /// The shape's nullifier PDAs.
    pub n_inputs: usize,
    pub co_signers: usize,
    pub legs: usize,
}

impl TransferAddresses {
    /// The owner signers that fit, a co-signer takes its record too.
    #[must_use]
    pub const fn owner_signers(self) -> usize {
        MAX_TRANSACTION_ADDRESSES
            .saturating_sub(SPP_ADDRESSES + RING_ADDRESSES)
            .saturating_sub(self.n_inputs)
            .saturating_sub(self.co_signers.saturating_mul(2))
            .saturating_sub(self.legs)
    }

    /// The rows that fit, one bound for the catalog's pools and the SDK's refusal.
    #[must_use]
    pub const fn owner_rows(self, authorization: RowAuthorization) -> usize {
        self.owner_signers() + authorization.unsigned_rows()
    }
}

pub trait FittedSpend {
    /// The wide flag follows the narrow key failing to hold the spend.
    #[must_use]
    fn fitted(self) -> Self;
}

impl FittedSpend for TransferSpend {
    fn fitted(self) -> Self {
        let narrow = FrameKey {
            wide: false,
            ..self.frame_key()
        }
        .shape();
        Self {
            wide: !narrow.is_some_and(|shape| shape.holds(self)),
            ..self
        }
    }
}

pub trait KeyWidths {
    /// The withdrawal slots of a transfer key, `None` outside the transfer keys.
    fn withdrawals(self) -> Option<u8>;

    /// The owner rows of a transfer key, `None` outside the transfer keys.
    fn owners(self) -> Option<u8>;

    /// Each pool takes as many subjects as records, `None` outside the transfer keys.
    fn record_pools(self) -> Option<RecordPools>;
}

impl KeyWidths for RingCircuit {
    fn withdrawals(self) -> Option<u8> {
        match self {
            Self::Transfer { withdrawals, .. } => Some(withdrawals),
            _ => None,
        }
    }

    fn owners(self) -> Option<u8> {
        match self {
            Self::Transfer { owners, .. } => Some(owners),
            _ => None,
        }
    }

    fn record_pools(self) -> Option<RecordPools> {
        TransferShape::of(self).map(|shape| shape.pools)
    }
}

pub fn proving_key_hashes() -> impl Iterator<Item = (&'static str, [u8; 32])> {
    PROVING_KEY_SHA256S.iter().copied()
}

/// `name` is the prover's circuit name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingKey {
    pub circuit: RingCircuit,
    pub name: &'static str,
}

impl RingKey {
    pub fn file_name(self) -> String {
        format!("{}.key", self.name.replace('-', "_"))
    }

    /// The release lock section, `custom-ring-apply-records-2` reads `apply_records_2_key`.
    pub fn section(self) -> String {
        let stem = self.name.strip_prefix(NAME_PREFIX).unwrap_or(self.name);
        key_suffixed(&stem.replace('-', "_"), "_")
    }

    pub fn asset(self, tag: &str) -> String {
        format!("{}-{tag}.key", key_suffixed(self.name, "-"))
    }
}

const NAME_PREFIX: &str = "custom-ring-";

/// One `key` suffix, a name that already ends in key takes none.
fn key_suffixed(name: &str, separator: &str) -> String {
    if name.ends_with("key") {
        name.to_owned()
    } else {
        format!("{name}{separator}key")
    }
}

#[cfg(test)]
mod tests {
    use custom_ring_interface::{
        key_catalog::{ApplyMode, TRANSFER_SHAPES},
        RingRail,
    };

    use super::*;

    #[test]
    fn key_names_carry_one_key_suffix() {
        let apply = RingKey {
            circuit: RingCircuit::ApplyRecords {
                writes: 2,
                mode: ApplyMode::Public,
            },
            name: "custom-ring-apply-records-2",
        };
        let register = RingKey {
            circuit: RingCircuit::RegisterKey,
            name: "custom-ring-register-key",
        };
        assert_eq!(apply.file_name(), "custom_ring_apply_records_2.key");
        assert_eq!(apply.section(), "apply_records_2_key");
        assert_eq!(apply.asset("v6"), "custom-ring-apply-records-2-key-v6.key");
        assert_eq!(register.section(), "register_key");
        assert_eq!(register.asset("v6"), "custom-ring-register-key-v6.key");
    }

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
    const WIDE: RingCircuit = TRANSFER_SHAPES[4].circuit();
    const WIDE_INPUTS: RingCircuit = TRANSFER_SHAPES[5].circuit();
    const ENTRY_FOUR: RingCircuit = TRANSFER_SHAPES[6].circuit();
    const ENTRY_EIGHT_TWO: RingCircuit = TRANSFER_SHAPES[7].circuit();
    const ENTRY_EIGHT_FIVE: RingCircuit = TRANSFER_SHAPES[8].circuit();
    const WIDE_ENTRY: RingCircuit = TRANSFER_SHAPES[9].circuit();

    #[test]
    fn a_narrow_spend_past_24_inputs_fits_the_wide_flag() {
        let fitted = |inputs| {
            TransferSpend {
                inputs,
                ..spend(RingRail::Confidential, 2, false, false)
            }
            .fitted()
        };
        assert!(!fitted(24).wide);
        assert_eq!(RingCircuit::transfer(fitted(24)), Some(TWO));
        assert!(fitted(25).wide);
        assert_eq!(RingCircuit::transfer(fitted(25)), Some(WIDE_INPUTS));
    }

    #[test]
    fn an_entry_past_the_small_keys_fits_the_wide_entry_key() {
        let entered = |inputs, outputs, legs, owners| {
            RingCircuit::transfer(
                TransferSpend {
                    inputs,
                    legs,
                    owners,
                    ..spend(RingRail::Anonymous, outputs, true, false)
                }
                .fitted(),
            )
        };
        assert_eq!(entered(8, 2, 4, 8), Some(ENTRY_FOUR));
        assert_eq!(entered(2, 8, 4, 2), Some(ENTRY_EIGHT_TWO));
        assert_eq!(entered(1, 2, 5, 1), Some(WIDE_ENTRY));
        assert_eq!(entered(8, 2, 32, 8), Some(WIDE_ENTRY));
        // Four and eight output entries past four legs fit one inline transaction.
        assert_eq!(entered(2, 4, 32, 2), Some(WIDE_ENTRY));
        assert_eq!(entered(1, 8, 27, 1), Some(WIDE_ENTRY));
        assert_eq!(entered(3, 8, 21, 3), Some(WIDE_ENTRY));
        assert_eq!(entered(9, 2, 0, 1), None);
        assert_eq!(entered(1, 2, 33, 1), None);
        assert_eq!(entered(8, 2, 5, 9), None);
    }

    #[test]
    fn a_spend_past_the_narrow_key_fits_the_wide_flag() {
        let fitted = |outputs, inputs, legs, owners| {
            TransferSpend {
                inputs,
                legs,
                owners,
                ..spend(RingRail::Confidential, outputs, false, false)
            }
            .fitted()
            .wide
        };
        assert!(!fitted(2, 24, 4, 4));
        assert!(fitted(2, 1, 5, 1));
        assert!(fitted(2, 5, 0, 5));
        assert!(fitted(8, 17, 0, 1));
    }

    #[test]
    fn the_address_bound_leaves_room_for_each_key_of_the_catalog() {
        let bound = |n_inputs, co_signers, legs| {
            TransferAddresses {
                n_inputs,
                co_signers,
                legs,
            }
            .owner_signers()
        };
        assert_eq!(bound(24, 0, 0), 30);
        assert_eq!(bound(32, 0, 0), 22);
        assert_eq!(bound(24, 5, 4), 16);
        assert_eq!(bound(64, 5, 32), 0);
        for shape in TRANSFER_SHAPES {
            let owners = usize::from(shape.owners);
            assert!(owners <= bound(owners, 0, 0));
        }
    }

    #[test]
    fn oversized_account_counts_leave_no_owner_capacity() {
        let base = TransferAddresses::default();
        for accounts in [
            TransferAddresses {
                n_inputs: usize::MAX,
                ..base
            },
            TransferAddresses {
                co_signers: usize::MAX,
                ..base
            },
            TransferAddresses {
                legs: usize::MAX,
                ..base
            },
        ] {
            assert_eq!(accounts.owner_signers(), 0);
        }
    }

    #[test]
    fn the_pools_reserve_distinct_input_assets() {
        let pools = |circuit: RingCircuit| {
            let pools = circuit.record_pools().unwrap();
            (pools.own, pools.curator, pools.tracking)
        };
        assert_eq!(pools(TWO), (39, 39, 6));
        assert_eq!(pools(FOUR), (43, 43, 8));
        assert_eq!(pools(EIGHT), (43, 43, 12));
        assert_eq!(pools(WIDE), (142, 142, 42));
        assert_eq!(pools(WIDE_INPUTS), (62, 62, 12));
        assert_eq!(pools(ENTRY_FOUR), (63, 63, 16));
        assert_eq!(pools(ENTRY_EIGHT_TWO), (55, 55, 14));
        assert_eq!(pools(ENTRY_EIGHT_FIVE), (64, 64, 16));
        assert_eq!(pools(WIDE_ENTRY), (121, 121, 41));
    }

    #[test]
    fn a_spend_past_a_narrow_keys_pools_moves_to_the_wide_key() {
        let fitted = |outputs, pools| {
            RingCircuit::transfer(
                TransferSpend {
                    pools,
                    ..spend(RingRail::Confidential, outputs, false, false)
                }
                .fitted(),
            )
        };
        let tracked = RecordPools {
            tracking: 7,
            ..RecordPools::default()
        };
        assert_eq!(fitted(2, tracked), Some(WIDE));
        let opened = |own| RecordPools {
            own,
            ..RecordPools::default()
        };
        let capacity = TWO.record_pools().unwrap().own;
        assert_eq!(fitted(2, opened(capacity)), Some(TWO));
        assert_eq!(fitted(2, opened(capacity + 1)), Some(WIDE));
        assert_eq!(fitted(2, RecordPools::default()), Some(TWO));
        let past_wide = RecordPools {
            tracking: 43,
            ..RecordPools::default()
        };
        assert_eq!(fitted(8, past_wide), None);
    }

    #[test]
    fn forty_distinct_input_assets_fit_the_largest_input_key() {
        let inputs = 40;
        let owner_subjects = 3;
        let candidate = TransferSpend {
            inputs,
            pools: RecordPools {
                own: inputs + owner_subjects,
                curator: inputs + owner_subjects,
                tracking: owner_subjects,
            },
            ..spend(RingRail::Confidential, 2, false, false)
        };
        assert_eq!(RingCircuit::transfer(candidate.fitted()), Some(WIDE_INPUTS));
        let limit = WIDE_INPUTS.record_pools().unwrap().own;
        assert_eq!(
            RingCircuit::transfer(TransferSpend {
                pools: RecordPools {
                    own: limit + 1,
                    ..candidate.pools
                },
                ..candidate.fitted()
            }),
            None
        );
    }

    #[test]
    fn ordinary_seals_keep_key_positions_across_input_and_output_capacities() {
        for (smaller, larger) in [
            (TRANSFER_SHAPES[0], TRANSFER_SHAPES[5]),
            (TRANSFER_SHAPES[0], TRANSFER_SHAPES[4]),
            (TRANSFER_SHAPES[2], TRANSFER_SHAPES[4]),
        ] {
            assert!(smaller.seals_like(larger));
        }
    }

    #[test]
    fn entry_seals_bind_input_capacity() {
        assert!(!TRANSFER_SHAPES[7].seals_like(TRANSFER_SHAPES[8]));
    }
}
