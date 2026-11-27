use super::*;
use crate::tests::support::{pinned_request, redacts};

#[test]
fn an_opening_prints_no_blinding() {
    let opening = Opening {
        blinding: [0xab; 32],
        ..Opening::padding()
    };
    let printed = format!("{opening:?}");
    assert!(redacts(&printed, &["blinding"]));
    assert!(!printed.contains("171, 171"));
}

/// `UPDATE_PROVER_JSON=1` writes the file, Go `TestRustPolicyTableReachesTheAssignment` decodes it.
#[test]
fn a_private_frozen_source_sends_its_salted_bit_past_a_byte() {
    use zolana_ring_policy::{ListId, ListSet, RuleTable, Subject};

    let lists = [ListId::PrivateBlock, ListId::PrivateFrozen];
    let sources = SourceMap::new(&lists.map(|list_id| (list_id, [0; 32]))).unwrap();
    let rules = RuleTable::builder()
        .forbid_each(Subject::Sender, ListSet::of(&lists))
        .build()
        .encode();
    let table = serde_json::to_value(PolicyTable {
        rules: &rules,
        sources: &sources,
    })
    .unwrap();
    assert_eq!(table["salted"], ListSet::of(&lists).bits());
    assert!(ListSet::of(&lists).bits() > u16::from(u8::MAX));
    assert_eq!(
        pinned_request(
            "../../test-vectors/proof/ring-request-policy-table.json",
            &table
        ),
        table
    );
}
