//! What a program proves over before its ring transfer stages.

use solana_address::Address;
use spr_interface::shape::Shape;
use spr_keypair::viewing_key::Salt;
use spr_program::{transaction_hash, PublicTransfer};
use spr_transaction::{
    instructions::transact::{SettlementTransfer, SppProofInputs},
    SppProofOutputUtxo,
};

use crate::RingError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramCommitment {
    pub salt: Salt,
    pub private_tx_hash: [u8; 32],
    pub transaction_hash: [u8; 32],
}

/// The sealed transfer's values, staging under its salt reproduces each one.
#[must_use = "prove over the plan, then commit the transfer to it"]
#[derive(Clone)]
pub struct ProgramPlan {
    pub salt: Salt,
    pub blinding_seed: [u8; 32],
    pub shape: Shape,
    pub first_nullifier: [u8; 32],
    pub output_tree_id: u16,
    /// Final slot order with every blinding, padding included.
    pub outputs: Vec<SppProofOutputUtxo>,
    pub private_tx_hash: [u8; 32],
    /// `private_tx_hash` chained with the public transfers in leg order.
    pub transaction_hash: [u8; 32],
}

pub(super) struct CommittedHashes {
    private_tx_hash: [u8; 32],
    transaction_hash: [u8; 32],
}

impl CommittedHashes {
    pub(super) fn of(sealed: &SppProofInputs) -> Result<Self, RingError> {
        let private_tx_hash = sealed.padding_independent_private_tx_hash()?;
        let transfers: Vec<_> = sealed
            .external_data
            .interface_transfers
            .iter()
            .map(public_transfer)
            .collect();
        Ok(Self {
            private_tx_hash,
            transaction_hash: transaction_hash(&private_tx_hash, &transfers)?,
        })
    }
}

fn public_transfer(transfer: &SettlementTransfer) -> PublicTransfer {
    let [_, account] = transfer.settlement_accounts();
    PublicTransfer {
        mint: transfer.asset(),
        is_deposit: transfer.is_deposit(),
        amount: transfer.amount(),
        account: Address::new_from_array(account),
    }
}

impl ProgramCommitment {
    pub(super) fn check(&self, staged: &CommittedHashes) -> Result<(), RingError> {
        if staged.private_tx_hash != self.private_tx_hash {
            return Err(RingError::ProgramCommitmentMismatch {
                committed: self.private_tx_hash,
                staged: staged.private_tx_hash,
            });
        }
        if staged.transaction_hash != self.transaction_hash {
            return Err(RingError::ProgramTransactionHashMismatch {
                committed: self.transaction_hash,
                staged: staged.transaction_hash,
            });
        }
        Ok(())
    }
}

impl ProgramPlan {
    pub const fn commitment(&self) -> ProgramCommitment {
        ProgramCommitment {
            salt: self.salt,
            private_tx_hash: self.private_tx_hash,
            transaction_hash: self.transaction_hash,
        }
    }

    /// The check staging runs, a commitment it refuses stages no transfer of the plan.
    pub fn check(&self, commitment: &ProgramCommitment) -> Result<(), RingError> {
        commitment.check(&CommittedHashes {
            private_tx_hash: self.private_tx_hash,
            transaction_hash: self.transaction_hash,
        })
    }

    pub(super) fn of(sealed: &SppProofInputs) -> Result<Self, RingError> {
        let hashes = CommittedHashes::of(sealed)?;
        Ok(Self {
            salt: sealed.external_data.salt,
            blinding_seed: sealed.blinding_seed,
            shape: Shape::new(sealed.input_utxos.len(), sealed.output_utxos.len()),
            first_nullifier: sealed.first_nullifier()?,
            output_tree_id: sealed.output_tree_id,
            outputs: sealed.output_utxos.clone(),
            private_tx_hash: hashes.private_tx_hash,
            transaction_hash: hashes.transaction_hash,
        })
    }
}
