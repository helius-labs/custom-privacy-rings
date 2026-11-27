//! `Transfer` and `AuthorityTransfer`.

mod accounts;
mod approve_transfer;
mod processor;
mod verify;

pub(crate) use approve_transfer::process_approve_transfer_ix;
pub(crate) use processor::process_transfer_ix;
