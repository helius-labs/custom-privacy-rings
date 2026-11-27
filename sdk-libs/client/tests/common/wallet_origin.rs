#[path = "origin.rs"]
mod fixture;

use solana_address::Address;
use solana_signature::Signature;
use spr_transaction::ShieldedTransaction;
use zolana_ring_client::{AuditError, RingOrigin, TransactionOrigin};

pub struct WalletChain(pub Vec<ShieldedTransaction>);

impl zolana_ring_client::FinalizedTransactionOrigin for WalletChain {}

impl TransactionOrigin for WalletChain {
    async fn origin(
        &self,
        signature: Signature,
        index: u16,
        ring: Address,
    ) -> Result<RingOrigin, AuditError> {
        let row = self
            .0
            .iter()
            .find(|row| {
                row.tx_signature == signature
                    && row.event_index == Some(index)
                    && row.ring_program_id == Some(ring)
            })
            .ok_or(AuditError::ConfirmedEventMismatch)?;
        Ok(RingOrigin {
            ring_invoked: true,
            event: fixture::event(row).with_slot(row.slot),
            signers: Vec::new(),
            pool_accounts: Vec::new(),
            withdrawals: Vec::new(),
            merge_disclosure: None,
            transact: None,
        })
    }
}
