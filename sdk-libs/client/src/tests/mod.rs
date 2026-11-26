#[path = "state/ring_accounts.rs"]
mod account;
#[path = "transfer/limits.rs"]
mod limits;
#[path = "merge/builder.rs"]
mod merge;
pub(crate) mod support;
#[path = "state/pool_trees.rs"]
mod trees;

/// A table every fixture member satisfies on the allow list.
pub(crate) fn pipeline_rules() -> zolana_ring_policy::RuleTable {
    use zolana_ring_policy::{ListId, Rule, RuleTable, Subject};
    RuleTable::builder()
        .rule(Rule::require(Subject::Sender, ListId::Allow))
        .rule(Rule::require(Subject::OutputOwner, ListId::Allow))
        .build()
}
