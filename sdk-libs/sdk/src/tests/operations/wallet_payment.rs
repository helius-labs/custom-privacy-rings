use std::collections::HashSet;

use custom_ring_interface::RingMode;
use spr_client::{RingPayment, RingPaymentWallet};
use spr_keypair::{random_blinding, ShieldedAddress, ShieldedKeypair};
use spr_transaction::{
    Data, Mint, RingPaymentRefusal, SpendableDecryptionResult, TransactionError, Utxo,
};

use super::*;
use crate::pipeline::Assembled;
use crate::tests::support::{keypair, payer, ring, wallet, Chain, Registry, RingSettings, TREE_ID};

/// Stops at assembly and raises the ring's payment by `overpay` past core's terms.
struct Assembling<'a> {
    port: Port<'a>,
    overpay: u64,
}

enum Port<'a> {
    Anonymous(AnonymousRingTransfers<'a, Chain, Chain>),
    Owner(OwnerRingTransfers<'a, Chain, Chain>),
}

impl RingTransfers for Assembling<'_> {
    type Error = RingError;
    type Prepared = Assembled;

    fn ring_program_id(&self) -> Address {
        ring().program_id()
    }

    async fn prepare(&self, mut request: RingTransferRequest<'_>) -> Result<Assembled, RingError> {
        request.payment.amount += self.overpay;
        match &self.port {
            Port::Anonymous(port) => port.transfer(&request)?.assemble(port.proving.client).await,
            Port::Owner(port) => port.transfer(&request)?.assemble(port.0.client).await,
        }
    }
}

fn funded(owner: &ShieldedKeypair) -> SpendableDecryptionResult {
    let note = Utxo {
        owner: owner.signing_pubkey(),
        asset: Mint::SOL,
        amount: 100,
        blinding: random_blinding(),
        ring_program_id: None,
        data: Data::default(),
    };
    SpendableDecryptionResult {
        balances: [wallet(note, &owner.nullifier_key, TREE_ID, 0)]
            .into_iter()
            .collect(),
        ..Default::default()
    }
}

fn payment(recipient: ShieldedAddress) -> RingPayment {
    RingPayment {
        payer: payer(),
        recipient,
        asset: Mint::SOL,
        amount: 30,
    }
}

/// Assembly verifies core's signature against the ring's final proof inputs.
#[tokio::test]
async fn the_anonymous_rail_proves_a_wallet_payment_only_under_cores_signature() {
    let (owner, paid, fee_payer) = (keypair(1), keypair(2), keypair(9));
    let client = Chain::ring(RingSettings::new(RingMode::Anonymous))
        .with_records(Registry::new(&[(&owner, 0), (&paid, 0)]))
        .client();
    let spendable = funded(&owner);
    let excluded = HashSet::new();
    let to = RingAddress {
        recipient: paid.shielded_address().unwrap(),
        ring_program_id: ring().program_id(),
        list_salt: [7; 32],
    };
    let pay = |overpay| {
        let assembling = Assembling {
            port: Port::Anonymous(AnonymousRingTransfers {
                proving: RingProving {
                    client: &client,
                    ring: ring(),
                    payer: &fee_payer,
                    signers: &[],
                },
                payees: core::slice::from_ref(&to),
            }),
            overpay,
        };
        let wallet = RingPaymentWallet {
            spendable: &spendable,
            keys: &owner,
            authority: &owner,
            owner: Some(&owner.signing_key),
            excluded: &excluded,
        };
        async move { payment(to.recipient).prepare(&assembling, wallet).await }
    };

    let assembled = pay(0).await.unwrap();
    assert!(assembled.parts().anonymous.is_some());
    assert!(assembled.proof_inputs().output_utxos.iter().any(|output| {
        output.owner_address == Some(to.recipient)
            && output.amount == 30
            && output.ring_program_id == Some(ring().program_id())
    }));
    assert!(matches!(
        pay(1).await,
        Err(RingError::Transaction(
            TransactionError::RingPaymentRefused(RingPaymentRefusal::RecipientAmount)
        ))
    ));
}

#[tokio::test]
async fn the_owner_rail_without_an_owner_refuses_a_payment_core_did_not_approve() {
    let (owner, fee_payer) = (keypair(1), keypair(9));
    let client = Chain::ring(RingSettings::new(RingMode::Confidential)).client();
    let spendable = funded(&owner);
    let excluded = HashSet::new();
    let recipient = keypair(2).shielded_address().unwrap();
    let pay = |overpay| {
        let assembling = Assembling {
            port: Port::Owner(OwnerRingTransfers(RingProving {
                client: &client,
                ring: ring(),
                payer: &fee_payer,
                signers: &[],
            })),
            overpay,
        };
        let wallet = RingPaymentWallet {
            spendable: &spendable,
            keys: &owner,
            authority: &owner,
            owner: None,
            excluded: &excluded,
        };
        async move { payment(recipient).prepare(&assembling, wallet).await }
    };

    let assembled = pay(0).await.unwrap();
    assert!(assembled.parts().in_proof_owner.is_none());
    assert!(matches!(
        pay(1).await,
        Err(RingError::Transaction(
            TransactionError::RingPaymentRefused(RingPaymentRefusal::RecipientAmount)
        ))
    ));
}

#[tokio::test]
async fn the_owner_rail_refuses_an_in_proof_owner_before_any_proof() {
    let (owner, fee_payer) = (keypair(1), keypair(9));
    let client = Chain::ring(RingSettings::new(RingMode::Confidential)).client();
    let assembling = Assembling {
        port: Port::Owner(OwnerRingTransfers(RingProving {
            client: &client,
            ring: ring(),
            payer: &fee_payer,
            signers: &[],
        })),
        overpay: 0,
    };
    let wallet = RingPaymentWallet {
        spendable: &funded(&owner),
        keys: &owner,
        authority: &owner,
        owner: Some(&owner.signing_key),
        excluded: &HashSet::new(),
    };

    let refused = payment(keypair(2).shielded_address().unwrap())
        .prepare(&assembling, wallet)
        .await;
    assert!(matches!(
        refused,
        Err(RingError::InProofDefaultRingInput { index: 0 })
    ));
}
