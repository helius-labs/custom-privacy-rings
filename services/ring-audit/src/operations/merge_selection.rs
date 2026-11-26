use std::{
    collections::{BTreeMap, HashMap, HashSet},
    num::NonZeroU32,
};

use solana_address::Address;
use spr_transaction::WalletUtxo;
use zolana_ring_client::{rpc::MergeSettings, select_merge_where, MERGE_DEFAULT_INPUT_COUNT};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergeTarget {
    pub(crate) note_count: NonZeroU32,
    pub(crate) min_average_amount: u64,
}

impl MergeTarget {
    /// `None` for a zero target, a later request revokes with it.
    pub fn of(settings: &MergeSettings) -> Option<Self> {
        Some(Self {
            note_count: NonZeroU32::new(settings.target_note_count)?,
            min_average_amount: settings.min_average_amount,
        })
    }

    pub fn plan(&self, notes: &[WalletUtxo]) -> Option<Vec<WalletUtxo>> {
        let mut summary = MergeSummary::default();
        for note in notes {
            summary.record(note);
        }
        let mut selection = summary.select(*self)?;
        for note in notes {
            selection.record(note.clone());
        }
        selection.finish()
    }
}

#[derive(Default)]
struct MergeSummary {
    asset: Option<Address>,
    note_count: usize,
    total: u128,
    trees: BTreeMap<u16, usize>,
}

struct MergeSelection {
    asset: Address,
    tree: u16,
    width: usize,
    notes: Vec<WalletUtxo>,
}

pub(crate) struct MergeTotals<'a> {
    targets: &'a HashMap<Address, MergeTarget>,
    assets: HashMap<Address, MergeSummary>,
}

pub(crate) struct MergeSelections {
    assets: HashMap<Address, MergeSelection>,
}

impl MergeSummary {
    fn record(&mut self, note: &WalletUtxo) {
        let asset = *self.asset.get_or_insert(note.utxo.asset.asset);
        self.note_count += 1;
        self.total += u128::from(note.utxo.amount);
        if note.utxo.asset.asset == asset
            && note.utxo.amount > 0
            && note.nullifier_pubkey != spr_interface::ZERO_NULLIFIER_PUBKEY
        {
            *self.trees.entry(note.tree_id()).or_default() += 1;
        }
    }

    fn select(self, target: MergeTarget) -> Option<MergeSelection> {
        let by_average = match target.min_average_amount {
            0 => self.note_count,
            least => usize::try_from(self.total / u128::from(least))
                .unwrap_or(usize::MAX)
                .max(1),
        };
        let keep = by_average.min(usize::try_from(target.note_count.get()).unwrap_or(usize::MAX));
        let width = self.note_count.checked_sub(keep)?.checked_add(1)?;
        let (tree, count) = self
            .trees
            .into_iter()
            .rev()
            .max_by_key(|(_, count)| *count)?;
        if width < 2 || count < 2 {
            return None;
        }
        Some(MergeSelection {
            asset: self.asset?,
            tree,
            width: width.min(MERGE_DEFAULT_INPUT_COUNT),
            notes: Vec::new(),
        })
    }
}

impl MergeSelection {
    fn record(&mut self, note: WalletUtxo) {
        if note.utxo.asset.asset != self.asset
            || note.tree_id() != self.tree
            || note.utxo.amount == 0
            || note.nullifier_pubkey == spr_interface::ZERO_NULLIFIER_PUBKEY
        {
            return;
        }
        let index = self
            .notes
            .partition_point(|held| held.utxo.amount <= note.utxo.amount);
        if index < self.width {
            if self.notes.len() == self.width {
                self.notes.pop();
            }
            self.notes.insert(index, note);
        }
    }

    fn finish(self) -> Option<Vec<WalletUtxo>> {
        select_merge_where(&self.notes, self.asset, self.width, &HashSet::new(), |_| {
            true
        })
        .ok()
    }
}

impl<'a> MergeTotals<'a> {
    pub(crate) fn new(targets: &'a HashMap<Address, MergeTarget>) -> Self {
        Self {
            targets,
            assets: HashMap::new(),
        }
    }

    pub(crate) fn record(&mut self, note: WalletUtxo) {
        if self.targets.contains_key(&note.utxo.asset.asset) {
            self.assets
                .entry(note.utxo.asset.asset)
                .or_default()
                .record(&note);
        }
    }

    pub(crate) fn select(self) -> MergeSelections {
        MergeSelections {
            assets: self
                .assets
                .into_iter()
                .filter_map(|(asset, summary)| {
                    Some((asset, summary.select(*self.targets.get(&asset)?)?))
                })
                .collect(),
        }
    }
}

impl MergeSelections {
    pub(crate) fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }

    pub(crate) fn record(&mut self, note: WalletUtxo) {
        if let Some(selection) = self.assets.get_mut(&note.utxo.asset.asset) {
            selection.record(note);
        }
    }

    pub(crate) fn finish(self) -> HashMap<Address, Vec<WalletUtxo>> {
        self.assets
            .into_iter()
            .filter_map(|(asset, selection)| Some((asset, selection.finish()?)))
            .collect()
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    use solana_signature::Signature;
    use spr_keypair::PublicKey;
    use spr_transaction::{Data, Mint, Utxo};

    fn note(tree: u16, amount: u64) -> WalletUtxo {
        WalletUtxo {
            utxo: Utxo {
                owner: PublicKey::from_ed25519(&[1; 32]),
                asset: Mint::SOL,
                amount,
                blinding: [0; 32],
                ring_program_id: Some(Address::new_from_array([8; 32])),
                data: Data::default(),
            },
            nullifier_pubkey: [1; 32],
            utxo_hash: [0; 32],
            nullifier: [0; 32],
            tx_viewing_key: None,
            data_hash: None,
            ring_data_hash: None,
            tree_id: tree,
            leaf_index: 0,
            latest_tree_id: None,
            slot: 0,
            tx_signature: Signature::default(),
            slot_index: 0,
        }
    }

    #[test]
    fn singleton_trees_keep_no_candidate_notes() {
        let target = MergeTarget {
            note_count: NonZeroU32::new(1).expect("target"),
            min_average_amount: 0,
        };
        let targets = [(Mint::SOL.asset, target)].into();
        let mut totals = MergeTotals::new(&targets);
        for tree in 0..1_000 {
            totals.record(note(tree, 1));
        }
        for amount in 2..=100 {
            totals.record(note(700, amount));
        }
        assert_eq!(totals.assets[&Mint::SOL.asset].trees.len(), 1_000);
        let mut selected = totals.select();
        assert_eq!(selected.assets[&Mint::SOL.asset].tree, 700);
        assert!(selected.assets[&Mint::SOL.asset].notes.is_empty());
        for tree in 0..1_000 {
            selected.record(note(tree, 1));
        }
        for amount in (2..=100).rev() {
            selected.record(note(700, amount));
            assert!(selected.assets[&Mint::SOL.asset].notes.len() <= MERGE_DEFAULT_INPUT_COUNT);
        }
        let plan = selected.finish().remove(&Mint::SOL.asset).expect("plan");
        assert_eq!(plan.len(), MERGE_DEFAULT_INPUT_COUNT);
        assert!(plan.iter().all(|note| note.tree_id() == 700));
        assert_eq!(
            plan.iter().map(|note| note.utxo.amount).collect::<Vec<_>>(),
            (1..=MERGE_DEFAULT_INPUT_COUNT as u64).collect::<Vec<_>>()
        );
    }

    #[test]
    fn targets_use_all_amounts_and_counts_before_selection() {
        let notes = vec![note(2, 10), note(1, 10), note(2, 20), note(1, 20)];
        let unchanged = MergeTarget {
            note_count: NonZeroU32::new(4).expect("target"),
            min_average_amount: 0,
        };
        assert!(unchanged.plan(&notes).is_none());
        let average = MergeTarget {
            min_average_amount: 30,
            ..unchanged
        };
        let plan = average.plan(&notes).expect("average target");
        assert_eq!(plan.len(), 2);
        assert!(plan.iter().all(|note| note.tree_id() == 1));
        let count = MergeTarget {
            note_count: NonZeroU32::new(3).expect("target"),
            ..unchanged
        };
        assert_eq!(count.plan(&notes).expect("count target").len(), 2);
    }
}
