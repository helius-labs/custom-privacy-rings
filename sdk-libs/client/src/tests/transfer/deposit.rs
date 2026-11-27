use custom_ring_interface::{
    stream::slot_tag, CoSignScope, CustomRingError, RailAuthorization, RingMode, RingRail,
};
use solana_address::Address;
use solana_signer::Signer;
use spr_client::ClientError;
use spr_indexer_api::SerializablePubkey;
use spr_interface::pda;
use zolana_ring_keys::RingAddress;

use super::DepositTransfer;
use crate::{
    pipeline::Assembled,
    rpc::{
        RelayIntent, RelayProofInputs, RelayQuote, RelayRail, RelayRequestAttestation,
        ServiceTarget,
    },
    tests::support::{fresh_wallet_tags, keypair, ring, Chain, RingSettings, TREE_ID},
    DepositAsset, DepositOutput, DepositRecipient, PreparedRingDeposit, RelayedTransferInput,
    RingDeposit, RingDepositBatch, RingError, RingTransfer,
};

fn deposit<'a>(payer: &'a (dyn Signer + Sync), recipient: DepositRecipient<'a>) -> RingDeposit<'a> {
    RingDeposit {
        ring: ring(),
        payer,
        recipient,
        tree: pda::tree(TREE_ID),
        asset: DepositAsset::Sol,
        amount: 5,
        cosigner: None,
        list_salt: None,
        tags: None,
    }
}

/// The transfer frame the attempt would prove, every refusal ahead of the prover.
async fn assemble(prepared: &PreparedRingDeposit, chain: &Chain) -> Result<Assembled, RingError> {
    let client = chain.clone().client();
    match prepared.transfer()? {
        DepositTransfer::Confidential(transfer) => transfer.assemble(&client).await,
        DepositTransfer::Anonymous(transfer) => transfer.assemble(&client).await,
    }
}

fn to(owner: &spr_keypair::ShieldedKeypair) -> RingAddress {
    RingAddress {
        recipient: owner.shielded_address().unwrap(),
        ring_program_id: ring().program_id(),
        list_salt: [7; 32],
    }
}

/// No note is spent, the payer's leg funds the recipient's output and no owner signs.
#[tokio::test]
async fn a_deposit_proves_a_transfer_frame_of_only_dummies() {
    let (member, payer) = (keypair(1), keypair(9));
    let recipient = member.shielded_address().unwrap();
    for (mode, rail) in [
        (RingMode::Confidential, RingRail::Confidential),
        (RingMode::Anonymous, RingRail::Anonymous),
    ] {
        let chain = Chain::ring(RingSettings::new(mode)).with_tracking(&[recipient]);
        let paid = to(&member);
        let target = match mode {
            RingMode::Confidential => DepositRecipient::Confidential(recipient),
            RingMode::Anonymous => DepositRecipient::AnonymousPayment {
                to: &paid,
                request_tag: None,
            },
        };
        let tags = fresh_wallet_tags(&member);
        let prepared = RingDeposit {
            tags: (mode == RingMode::Anonymous).then_some(&tags),
            ..deposit(&payer, target)
        }
        .prepare(&chain)
        .await
        .unwrap();
        assert_eq!(
            prepared.sender.shielded_address().unwrap().nullifier_pubkey,
            spr_interface::ZERO_NULLIFIER_PUBKEY
        );
        let assembled = assemble(&prepared, &chain).await.unwrap();
        let inputs = assembled.proof_inputs();
        assert!(inputs.input_utxos.iter().all(|input| input.is_dummy()));
        let parts = assembled.parts();
        assert!(parts.no_spend);
        assert_eq!(parts.authorization.rail(), rail);
        assert_eq!(
            parts.authorization == RailAuthorization::AnonymousPublic,
            mode == RingMode::Anonymous
        );
        assert!(assembled.accounts().owner_signers.is_empty());
        assert_eq!(assembled.co_sign_demand(), prepared.co_sign_demand());
        assert_eq!(prepared.co_sign_demand(), CoSignScope::DEPOSITS);
        let [leg] = inputs.external_data.interface_transfers.as_slice() else {
            panic!("one deposit leg");
        };
        assert!(leg.is_deposit());
        assert_eq!(leg.interface_transfer().amount(), 5);
        assert!(inputs
            .output_utxos
            .iter()
            .any(|output| { output.owner_address == Some(recipient) && output.amount == 5 }));
    }
}

#[tokio::test]
async fn an_anonymous_deposit_publishes_no_bootstrap_tag_of_its_recipient() {
    let (wallet, member, payer) = (keypair(1), keypair(2), keypair(9));
    let recipient = member.shielded_address().unwrap();
    let chain = Chain::ring(RingSettings::new(RingMode::Anonymous)).with_tracking(&[recipient]);
    let paid = to(&member);
    let target = |request_tag| DepositRecipient::AnonymousPayment {
        to: &paid,
        request_tag,
    };
    let requested = [0x5a; 32];
    let tags = fresh_wallet_tags(&wallet);
    for sender_tag in [None, Some(&tags)] {
        for request in [None, Some(recipient.viewing_pubkey.u()), Some(requested)] {
            let prepared = RingDeposit {
                tags: sender_tag,
                ..deposit(&payer, target(request))
            }
            .prepare(&chain)
            .await
            .unwrap();
            let assembled = assemble(&prepared, &chain).await;
            if request != Some(requested) {
                assert!(matches!(assembled, Err(RingError::DepositBootstrapTag)));
                continue;
            }
            let assembled = assembled.unwrap();
            let slot = assembled
                .proof_inputs()
                .output_utxos
                .iter()
                .position(|output| output.owner_address == Some(recipient))
                .unwrap();
            assert_eq!(
                assembled.parts().outputs[slot].owner_tag,
                slot_tag(requested)
            );
        }
    }
}

#[tokio::test]
async fn a_deposit_publishes_the_wallets_sender_tag_on_the_note_to_its_own_key_alone() {
    let (member, payer) = (keypair(1), keypair(9));
    let recipient = member.shielded_address().unwrap();
    let chain = Chain::ring(RingSettings::new(RingMode::Anonymous)).with_tracking(&[recipient]);
    let paid = to(&member);
    let tags = fresh_wallet_tags(&member);
    let prepared = RingDeposit {
        tags: Some(&tags),
        ..deposit(
            &payer,
            DepositRecipient::AnonymousPayment {
                to: &paid,
                request_tag: None,
            },
        )
    }
    .prepare(&chain)
    .await
    .unwrap();
    let assembled = assemble(&prepared, &chain).await.unwrap();
    let outputs = &assembled.proof_inputs().output_utxos;
    for owner in [prepared.sender.shielded_address().unwrap(), recipient] {
        assert!(outputs
            .iter()
            .any(|output| output.owner_address == Some(owner)));
    }
    for (output, published) in outputs.iter().zip(&assembled.parts().outputs) {
        assert_eq!(
            published.owner_tag == tags.sender(),
            output.owner_address == Some(recipient)
        );
    }
}

#[tokio::test]
async fn a_batch_deposit_of_two_own_notes_publishes_the_sender_tag_once() {
    let (member, payer) = (keypair(1), keypair(9));
    let recipient = member.shielded_address().unwrap();
    let chain = Chain::ring(RingSettings::new(RingMode::Anonymous)).with_tracking(&[recipient]);
    let paid = to(&member);
    let tags = fresh_wallet_tags(&member);
    let output = DepositOutput {
        recipient: DepositRecipient::AnonymousPayment {
            to: &paid,
            request_tag: None,
        },
        asset: DepositAsset::Sol,
        amount: 5,
        list_salt: None,
    };
    let prepared = RingDepositBatch {
        ring: ring(),
        payer: &payer,
        tree: pda::tree(TREE_ID),
        outputs: vec![output; 2],
        cosigner: None,
        tags: Some(&tags),
    }
    .prepare(&chain)
    .await
    .unwrap();
    let assembled = assemble(&prepared, &chain).await.unwrap();
    let own: Vec<bool> = assembled
        .proof_inputs()
        .output_utxos
        .iter()
        .zip(&assembled.parts().outputs)
        .filter(|(output, _)| output.owner_address == Some(recipient))
        .map(|(_, published)| published.owner_tag == tags.sender())
        .collect();
    assert_eq!(own.len(), 2);
    assert_eq!(own.iter().filter(|tagged| **tagged).count(), 1);
}

#[tokio::test]
async fn a_recipient_of_another_mode_is_refused() {
    let (member, payer) = (keypair(1), keypair(9));
    let anonymous = Chain::ring(RingSettings::new(RingMode::Anonymous));
    let confidential = Chain::ring(RingSettings::new(RingMode::Confidential));
    let hidden = DepositRecipient::Anonymous { keys: &member };
    let confidential_target = DepositRecipient::Confidential(member.shielded_address().unwrap());
    assert!(matches!(
        deposit(&payer, confidential_target)
            .prepare(&anonymous)
            .await,
        Err(RingError::AnonymousDepositUntagged)
    ));
    assert!(matches!(
        deposit(&payer, hidden).prepare(&confidential).await,
        Err(RingError::NotAnonymousRing)
    ));
    assert!(matches!(
        deposit(&member, hidden).prepare(&anonymous).await,
        Err(RingError::AnonymousPublicSignerLinksOwner)
    ));
}

#[tokio::test]
async fn a_deposit_refuses_an_address_of_another_ring() {
    let payer = keypair(9);
    let chain = Chain::ring(RingSettings::new(RingMode::Anonymous));
    let other = Address::new_from_array([0x47; 32]);
    let foreign = RingAddress {
        ring_program_id: other,
        ..to(&keypair(2))
    };
    assert!(matches!(
        deposit(&payer, DepositRecipient::AnonymousPayment {
            to: &foreign,
            request_tag: None,
        })
            .prepare(&chain)
            .await,
        Err(RingError::ForeignRingAddress { address, ring: paid })
            if address == other && paid == ring().program_id()
    ));
}

/// Nine recipients pass the widest output row, four assets the public slots.
#[tokio::test]
async fn a_batch_past_the_frame_is_refused_before_any_read() {
    let payer = keypair(9);
    let empty = Chain::default();
    let recipient = DepositRecipient::Confidential(keypair(1).shielded_address().unwrap());
    let output = |asset| DepositOutput {
        recipient,
        asset,
        amount: 1,
        list_salt: None,
    };
    let spl = |byte| {
        DepositAsset::Spl(crate::DepositSplAccounts {
            mint: Address::new_from_array([byte; 32]),
            user_token: Address::new_from_array([byte + 1; 32]),
            token_program: pda::spl_token_program_id(),
        })
    };
    let batch = |outputs| RingDepositBatch {
        ring: ring(),
        payer: &payer,
        tree: pda::tree(TREE_ID),
        outputs,
        cosigner: None,
        tags: None,
    };
    for outputs in [
        vec![output(DepositAsset::Sol); 9],
        vec![
            output(DepositAsset::Sol),
            output(spl(0x10)),
            output(spl(0x20)),
            output(spl(0x30)),
        ],
        Vec::new(),
    ] {
        assert!(matches!(
            batch(outputs).prepare(&empty).await,
            Err(RingError::DepositUnkeyed)
        ));
    }
}

/// A keyed note pays an owner with no row, a registered owner's viewing key must be the one its row holds.
#[tokio::test]
async fn a_deposit_takes_the_owners_tracking_key_before_any_proof() {
    let (member, payer) = (keypair(2), keypair(9));
    let enrolled = member.shielded_address().unwrap();
    let unenrolled = Chain::ring(RingSettings::new(RingMode::Confidential)).with_tracking(&[]);
    let prepared = deposit(&payer, DepositRecipient::Confidential(enrolled))
        .prepare(&unenrolled)
        .await
        .unwrap();
    assert!(assemble(&prepared, &unenrolled).await.is_ok());
    let rotated = spr_keypair::ShieldedAddress {
        viewing_pubkey: keypair(3).shielded_address().unwrap().viewing_pubkey,
        ..enrolled
    };
    let chain = Chain::ring(RingSettings::new(RingMode::Confidential)).with_tracking(&[enrolled]);
    let stale = deposit(&payer, DepositRecipient::Confidential(rotated))
        .prepare(&chain)
        .await
        .unwrap();
    assert!(matches!(
        assemble(&stale, &chain).await,
        Err(RingError::RecipientViewingKeyStale { .. })
    ));
}

#[tokio::test]
async fn a_deposit_names_the_co_signer_a_deposit_scope_demands() {
    let (member, payer, co_signer, stranger) = (keypair(1), keypair(9), keypair(8), keypair(7));
    let recipient = DepositRecipient::Confidential(member.shielded_address().unwrap());
    for (scope, signs) in [
        (CoSignScope::WITHDRAWALS, false),
        (CoSignScope::DEPOSITS, true),
    ] {
        let chain = Chain::ring(RingSettings {
            co_signer: Some((co_signer.pubkey(), scope)),
            ..RingSettings::new(RingMode::Confidential)
        });
        let named = RingDeposit {
            cosigner: Some(&co_signer),
            ..deposit(&payer, recipient)
        };
        let prepared = named.prepare(&chain).await.unwrap();
        assert_eq!(
            named.batch().signers(&prepared).len(),
            1 + usize::from(signs)
        );
        if !signs {
            continue;
        }
        assert!(matches!(
            deposit(&payer, recipient).prepare(&chain).await,
            Err(RingError::Program(CustomRingError::MissingCoSigner))
        ));
        let wrong = RingDeposit {
            cosigner: Some(&stranger),
            ..deposit(&payer, recipient)
        };
        assert!(matches!(
            wrong.prepare(&chain).await,
            Err(RingError::Program(CustomRingError::InvalidCoSignerRecord))
        ));
    }
}

/// A retry draws a new sender and dummy, so its outputs take new blindings.
#[tokio::test]
async fn a_retry_draws_a_new_frame_and_an_attempt_proves_once() {
    let (member, payer) = (keypair(1), keypair(9));
    let chain = Chain::ring(RingSettings::new(RingMode::Confidential))
        .with_tracking(&[member.shielded_address().unwrap()]);
    let client = chain.clone().client();
    let prepared = deposit(
        &payer,
        DepositRecipient::Confidential(member.shielded_address().unwrap()),
    )
    .prepare(&chain)
    .await
    .unwrap();
    let retried = prepared.fresh_attempt().unwrap();
    assert_ne!(
        retried.transaction.first_nullifier(),
        prepared.transaction.first_nullifier()
    );
    let refused = |proved| matches!(proved, Err(RingError::Client(ClientError::ProverServer(_))));
    assert!(refused(prepared.prove(&client).await));
    assert!(matches!(
        prepared.prove(&client).await,
        Err(RingError::DepositFrameReused)
    ));
    assert!(refused(retried.prove(&client).await));
}

mod relay {
    use super::*;

    const FEE_PAYER: Address = Address::new_from_array([0x7e; 32]);
    const SERVICE: ServiceTarget = ServiceTarget {
        cluster: [0x7c; 32],
        service: Address::new_from_array([0x7d; 32]),
    };

    fn quote() -> RelayQuote {
        RelayQuote {
            fee_payer: SerializablePubkey(FEE_PAYER),
            valid_until_slot: u64::MAX,
        }
    }

    /// The depositor signs the request text, the relay stages the same frame on the deposit rail.
    #[tokio::test]
    async fn a_relayed_deposit_carries_a_dummy_frame_its_funder_signs() {
        let (member, payer) = (keypair(1), keypair(9));
        let recipient = member.shielded_address().unwrap();
        let paid = to(&member);
        for mode in [RingMode::Confidential, RingMode::Anonymous] {
            let chain = Chain::ring(RingSettings::new(mode)).with_tracking(&[recipient]);
            let target = match mode {
                RingMode::Confidential => DepositRecipient::Confidential(recipient),
                RingMode::Anonymous => DepositRecipient::AnonymousPayment {
                    to: &paid,
                    request_tag: None,
                },
            };
            let tags = fresh_wallet_tags(&member);
            let deposit = RingDeposit {
                tags: (mode == RingMode::Anonymous).then_some(&tags),
                ..deposit(&payer, target)
            };
            relayed_deposit_stages(deposit, chain, mode).await;
        }
    }

    async fn relayed_deposit_stages(deposit: RingDeposit<'_>, chain: Chain, mode: RingMode) {
        let payer = deposit.payer;
        let prepared = deposit
            .prepare_relay(&chain, SERVICE, quote())
            .await
            .unwrap();
        let RelayIntent::Deposit {
            proof_inputs,
            depositor_signature,
        } = &prepared.request().intent
        else {
            panic!("a deposit intent");
        };
        let inputs = RelayProofInputs::decode(proof_inputs.as_bytes()).unwrap();
        let text = RelayRequestAttestation::deposit(
            SERVICE.ring(ring().program_id()),
            payer.pubkey(),
            &inputs,
            proof_inputs,
        )
        .message();
        assert!(depositor_signature.0.verify(payer.pubkey().as_ref(), &text));
        assert!(matches!(
            &inputs.rail,
            RelayRail::Deposit { plan } if plan.is_some() == (mode == RingMode::Anonymous)
        ));
        assert!(inputs.inputs.iter().all(|input| input.is_dummy()));
        assert_eq!(
            inputs.deposit_funders().collect::<Vec<_>>(),
            [payer.pubkey()]
        );
        let relayed = RingTransfer::relayed(RelayedTransferInput {
            ring: ring(),
            proof_inputs: &inputs,
            fee_payer: FEE_PAYER,
        });
        let assembled = relayed.assemble(&chain.client()).await.unwrap();
        assert!(assembled.parts().no_spend);
        assert!(assembled.accounts().owner_signers.is_empty());
    }

    #[tokio::test]
    async fn a_relay_fee_payer_never_funds_a_deposit() {
        let (member, fee_payer) = (keypair(1), keypair(9));
        let chain = Chain::ring(RingSettings::new(RingMode::Confidential));
        let quote = RelayQuote {
            fee_payer: SerializablePubkey(fee_payer.pubkey()),
            valid_until_slot: u64::MAX,
        };
        let recipient = DepositRecipient::Confidential(member.shielded_address().unwrap());
        assert!(matches!(
            deposit(&fee_payer, recipient)
                .prepare_relay(&chain, SERVICE, quote)
                .await,
            Err(RingError::RelayFeePayerFundsDeposit)
        ));
    }

    /// A deposit rail that spends a note or names the other mode's plan is refused.
    #[tokio::test]
    async fn the_relay_refuses_a_deposit_rail_off_its_ring_mode() {
        let (member, payer) = (keypair(1), keypair(9));
        let recipient = member.shielded_address().unwrap();
        let chain =
            Chain::ring(RingSettings::new(RingMode::Confidential)).with_tracking(&[recipient]);
        let prepared = deposit(&payer, DepositRecipient::Confidential(recipient))
            .prepare_relay(&chain, SERVICE, quote())
            .await
            .unwrap();
        let mut inputs =
            RelayProofInputs::decode(prepared.request().intent.proof_inputs().as_bytes()).unwrap();
        inputs.rail = RelayRail::Deposit {
            plan: Some(Box::default()),
        };
        let relayed = RingTransfer::relayed(RelayedTransferInput {
            ring: ring(),
            proof_inputs: &inputs,
            fee_payer: FEE_PAYER,
        });
        assert!(matches!(
            relayed.assemble(&chain.client()).await,
            Err(RingError::NotAnonymousRing)
        ));
    }
}
