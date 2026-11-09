pub mod cli;
pub mod policy;
pub mod ring_indexer;
pub mod ring_rpc;
pub mod ring_wallet;
pub mod shared;

/// False while zolana pins no ring merge key and `verify_ring` refuses every ring merge.
pub const RING_MERGES_SETTLE: bool = false;
