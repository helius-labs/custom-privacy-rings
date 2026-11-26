//! The ring output rails, read back as a recipient and the ring program read them.

use custom_ring_interface::{
    stream::slot_tag, RecordForm, RecordKind, SlotBody, CONFIDENTIAL_DATA_FREE_BODY_LEN,
    MIN_REPLY_LEN, RING_RECORD_LEN,
};
use solana_address::Address;
use solana_signature::Signature;
use spr_interface::{instruction::instruction_data::transact::OwnerTag, ZERO_NULLIFIER_PUBKEY};
use spr_keypair::{ShieldedAddress, ShieldedKeypair, SigningKey, ViewingKey, X25519Pubkey};
use spr_transaction::{
    instructions::transact::{ConfidentialTransaction, Shape, SppProofInputs, SppProofOutputUtxo},
    Data, Mint, TransactionError, Utxo, WalletUtxo,
};
use zolana_ring_keys::ring_blinding_seed;
use zolana_ring_keys::ring_body_output;
use zolana_ring_keys::DeliveredSlot;
use zolana_ring_keys::OutputRecord;
use zolana_ring_keys::RecipientPayload;
use zolana_ring_keys::RecordKey;
use zolana_ring_keys::SlotSecret;
use zolana_ring_transaction::{
    open_recipient_body, open_reply, record_key, seal_no_reply, seal_reply, AnonymousOutput,
    AnonymousPlan, AnonymousSlot, BodyForm, OpenedBody, OpenedNote, OpenedRecipientBody,
    OutputBlindings, PayloadRead, PublishedSlot, RecipientBody, ReplyMetadata, RingAssets,
    RingDelivery, RingOutputRail, RingRailError as E, ShapedSeal,
};

const RING: Address = Address::new_from_array([42; 32]);
const TREE: u16 = 7;
const USDC: Mint = Mint {
    asset: Address::new_from_array([9; 32]),
    asset_id: 2,
};
const DATA_HASH: [u8; 32] = [1; 32];
const RING_DATA_HASH: [u8; 32] = [2; 32];
const APP_DATA: [u8; 3] = [7, 8, 9];

fn auditor() -> X25519Pubkey {
    ViewingKey::from_bytes(&[0x51; 32]).pubkey()
}

fn delivered(tx_key: ViewingKey) -> RingOutputRail {
    RingOutputRail::new(tx_key, auditor(), RingDelivery::Delivered)
}

/// A ring record of `owner`, the opening a body carries.
fn record(owner: &ShieldedAddress, mint: Address, amount: u64, blinding: [u8; 32]) -> OutputRecord {
    OutputRecord {
        kind: RecordKind::Ring,
        amount,
        owner_key: owner.signing_pubkey.confidential_view_tag().unwrap(),
        mint: mint.to_bytes(),
        blinding,
        key: record_key(RecordKind::Ring, owner.nullifier_pubkey),
        ..OutputRecord::default()
    }
}

fn keypair(seed: u8) -> ShieldedKeypair {
    ShieldedKeypair::from_keypair(SigningKey::from_ed25519_bytes(&[seed; 32])).unwrap()
}

fn address(owner: &ShieldedKeypair) -> ShieldedAddress {
    owner.shielded_address().unwrap()
}

fn input(owner: &ShieldedKeypair, asset: Mint, amount: u64, leaf: u64) -> WalletUtxo {
    let utxo = Utxo {
        owner: owner.signing_pubkey(),
        asset,
        amount,
        blinding: [3 + leaf as u8; 32],
        ring_program_id: Some(RING),
        data: Data::default(),
    };
    let nullifier_pubkey = address(owner).nullifier_pubkey;
    let utxo_hash = utxo
        .hash(&nullifier_pubkey, &[0; 32], &[0; 32], TREE)
        .unwrap();
    WalletUtxo {
        nullifier: owner.nullifier(&utxo_hash, &utxo.blinding).unwrap(),
        utxo,
        nullifier_pubkey,
        utxo_hash,
        tx_viewing_key: None,
        data_hash: None,
        ring_data_hash: None,
        tree_id: TREE,
        leaf_index: leaf,
        latest_tree_id: None,
        slot: 1,
        tx_signature: Signature::from([1; 64]),
        slot_index: 0,
    }
}

fn ring_spend(
    owner: &ShieldedKeypair,
    inputs: Vec<WalletUtxo>,
    compact: bool,
) -> (ConfidentialTransaction, ViewingKey) {
    let tx_key = owner
        .viewing_key
        .get_transaction_viewing_key(&inputs[0].nullifier)
        .unwrap();
    let payer = address(owner).solana_address().unwrap();
    let transaction = match compact {
        true => ConfidentialTransaction::new_compact_with_ring(inputs, payer, RING),
        false => ConfidentialTransaction::new_with_ring(inputs, payer, RING),
    }
    .unwrap()
    .with_output_tree_id(TREE)
    .unwrap();
    (transaction, tx_key)
}

fn ring_transfer(owner: &ShieldedKeypair, compact: bool) -> (ConfidentialTransaction, ViewingKey) {
    ring_spend(owner, vec![input(owner, Mint::SOL, 10, 1)], compact)
}

fn delivered_secret(
    reader: &ViewingKey,
    proof: &SppProofInputs,
    rail: &RingOutputRail,
    index: u32,
) -> SlotSecret {
    let tx_pk = rail.tx_viewing_key().pubkey();
    DeliveredSlot {
        agreement: &reader.ecdh(&tx_pk).unwrap(),
        tx_viewing_pk: tx_pk.as_bytes(),
        recipient_viewing_pk: reader.pubkey().as_bytes(),
        salt: &rail.salt(),
        output_index: index,
        first_nullifier: &proof.input_utxos[0].nullifier(),
    }
    .secret()
    .unwrap()
}

/// The recipient body at output `position`, `None` when it decodes to no record under `reader`.
fn try_decrypt(
    reader: &ViewingKey,
    rail: &RingOutputRail,
    proof: &SppProofInputs,
    slot: &PublishedSlot<'_>,
    position: u32,
) -> Option<OpenedRecipientBody> {
    let SlotBody::Recipient(body) = slot.body else {
        panic!("recipient slot expected")
    };
    let form = BodyForm::of(&proof.output_utxos).unwrap().records;
    open_recipient_body(body, &delivered_secret(reader, proof, rail, position), form).ok()
}

fn decrypt(
    reader: &ViewingKey,
    rail: &RingOutputRail,
    proof: &SppProofInputs,
    slot: &PublishedSlot<'_>,
    position: u32,
) -> OpenedRecipientBody {
    try_decrypt(reader, rail, proof, slot, position).unwrap()
}

fn payload(body: &OpenedRecipientBody) -> RecipientPayload {
    match &body.payload {
        PayloadRead::Opened(payload) => payload.clone(),
        other => panic!("an opened payload, got {other:?}"),
    }
}

/// `V_i` of each published slot, the keys the auditor stream seals and the sender reads back.
fn sealed_keys(rail: &RingOutputRail, proof: &SppProofInputs) -> Vec<[u8; 32]> {
    rail.recipient_keys(&proof.output_utxos, proof.external_data.outputs.len())
        .unwrap()
        .iter()
        .map(X25519Pubkey::u)
        .collect()
}

/// The change at `slot`, its body opened under the sender's own viewing key.
fn change_at(
    sender: &ShieldedKeypair,
    rail: &RingOutputRail,
    assets: &RingAssets,
    proof: &SppProofInputs,
    slot: usize,
) -> Utxo {
    let published = rail.published_slots(proof, &address(sender)).unwrap();
    let body = decrypt(
        &sender.viewing_key,
        rail,
        proof,
        &published[slot],
        slot as u32,
    );
    note(
        reader(&address(sender), assets, proof, slot)
            .open_body(&body)
            .unwrap(),
    )
}

/// The note a body opens to, panicking on a miss or an unknown mint.
fn note(opened: Option<OpenedBody>) -> Utxo {
    match opened {
        Some(OpenedBody::Note(opened)) => opened.utxo,
        other => panic!("a note expected, got {other:?}"),
    }
}

fn payment(recipient: &ShieldedAddress) -> X25519Pubkey {
    recipient.viewing_pubkey
}

fn anonymous(tx_key: ViewingKey, plan: AnonymousPlan) -> RingOutputRail {
    RingOutputRail::new(tx_key, auditor(), RingDelivery::Anonymous(Box::new(plan)))
}

/// The reader's view of output `position`, every ring mint tried.
fn reader<'a>(
    owner: &'a ShieldedAddress,
    assets: &'a RingAssets,
    proof: &'a SppProofInputs,
    position: usize,
) -> AnonymousOutput<'a> {
    AnonymousOutput {
        owner,
        assets,
        ring: RING,
        tree_id: TREE,
        utxo_hash: &proof.external_data.outputs[position].utxo_hash,
    }
}

struct PaidFrame {
    rail: RingOutputRail,
    proof: SppProofInputs,
    position: usize,
}

fn pay(sender: &ShieldedKeypair, output: SppProofOutputUtxo, delivery: RingDelivery) -> PaidFrame {
    let owner = output.owner_address;
    let (mut tx, tx_key) = ring_transfer(sender, false);
    tx.add_output_utxo(output).unwrap();
    let rail = RingOutputRail::new(tx_key, auditor(), delivery);
    let proof = rail.encrypt(tx, &address(sender)).unwrap();
    let position = proof
        .output_utxos
        .iter()
        .position(|output| output.owner_address == owner)
        .unwrap();
    PaidFrame {
        rail,
        proof,
        position,
    }
}

impl PaidFrame {
    fn body(&self, sender: &ShieldedKeypair, reader: &ViewingKey) -> OpenedRecipientBody {
        let published = self
            .rail
            .published_slots(&self.proof, &address(sender))
            .unwrap();
        decrypt(
            reader,
            &self.rail,
            &self.proof,
            &published[self.position],
            self.position as u32,
        )
    }
}

fn both_rails(recipient: &ShieldedAddress) -> [RingDelivery; 2] {
    [
        RingDelivery::Delivered,
        RingDelivery::Anonymous(Box::new(
            AnonymousPlan::new().with_payment(payment(recipient)),
        )),
    ]
}

fn data_note(recipient: ShieldedAddress) -> SppProofOutputUtxo {
    SppProofOutputUtxo::new(Mint::SOL, 3, recipient)
        .unwrap()
        .with_ring_data_hash(RING, RING_DATA_HASH)
        .with_utxo_data(APP_DATA.to_vec(), DATA_HASH)
}

#[test]
fn a_delivered_output_opens_only_for_the_key_its_owner_tag_names() {
    let sender = keypair(1);
    let receiver = keypair(2);
    let recipient = address(&receiver);
    let (mut tx, tx_key) = ring_transfer(&sender, false);
    tx.transfer_sol(&recipient, 3).unwrap();
    let rail = delivered(tx_key.clone());
    let proof = rail.encrypt(tx, &address(&sender)).unwrap();
    assert_eq!(proof.external_data.salt, rail.salt());
    assert_eq!(
        proof.blinding_seed,
        ring_blinding_seed(&tx_key.secret_bytes(), &rail.salt()).unwrap()
    );
    assert_ne!(
        delivered(tx_key).salt(),
        rail.salt(),
        "a resent attempt draws a fresh salt"
    );
    assert_eq!(
        proof.external_data.outputs[1].owner_tag,
        OwnerTag::Inline(custom_ring_interface::stream::slot_tag(
            recipient.viewing_pubkey.u()
        ))
    );
    let published = rail.published_slots(&proof, &address(&sender)).unwrap();
    assert_eq!(published.len(), proof.output_utxos.len());
    assert!(published
        .iter()
        .all(|slot| slot.anonymous.is_none() && matches!(slot.body, SlotBody::Recipient(_))));
    let assets = RingAssets::new([]).unwrap();
    let body = decrypt(&receiver.viewing_key, &rail, &proof, &published[1], 1);
    let opened = note(
        reader(&recipient, &assets, &proof, 1)
            .open_body(&body)
            .unwrap(),
    );
    assert_eq!(
        (opened.amount, opened.blinding),
        (3, proof.output_utxos[1].blinding)
    );
    assert_eq!(proof.output_utxos[1].ring_data_hash, None);
    let stranger = try_decrypt(&keypair(3).viewing_key, &rail, &proof, &published[1], 1);
    assert!(stranger
        .and_then(|body| reader(&recipient, &assets, &proof, 1)
            .open_body(&body)
            .unwrap())
        .is_none());
}

/// SOL spent first, the SPL change still leads, a change the caller declared first stays put.
#[test]
fn a_delivered_transfer_leads_with_its_change_the_spl_ahead_of_the_sol() {
    let sender = keypair(1);
    let receiver = keypair(2);
    let recipient = address(&receiver);
    let spend = || {
        ring_spend(
            &sender,
            vec![input(&sender, Mint::SOL, 5, 1), input(&sender, USDC, 10, 2)],
            false,
        )
    };
    let (mut appended, tx_key) = spend();
    appended
        .transfer_with_ring(&recipient, USDC, 4, Some(RING))
        .unwrap();
    let (mut declared, _) = spend();
    for (asset, amount) in [(USDC, 6), (Mint::SOL, 5)] {
        declared
            .transfer_with_ring(&address(&sender), asset, amount, Some(RING))
            .unwrap();
    }
    declared
        .transfer_with_ring(&recipient, USDC, 4, Some(RING))
        .unwrap();
    let rail = delivered(tx_key);
    let order = |proof: &SppProofInputs| -> Vec<(Mint, u64)> {
        proof.output_utxos[..3]
            .iter()
            .map(|output| (output.asset, output.amount))
            .collect()
    };
    let proof = rail.encrypt(appended, &address(&sender)).unwrap();
    assert_eq!(order(&proof), [(USDC, 6), (Mint::SOL, 5), (USDC, 4)]);
    let again = rail.clone().encrypt(declared, &address(&sender)).unwrap();
    assert_eq!(again.output_utxos, proof.output_utxos);
    assert_eq!(again.external_data.outputs, proof.external_data.outputs);

    let blindings =
        OutputBlindings::new(&proof.first_nullifier().unwrap(), &proof.blinding_seed).unwrap();
    for (slot, (output, framed)) in proof
        .output_utxos
        .iter()
        .zip(&proof.external_data.outputs)
        .enumerate()
    {
        assert_eq!(output.blinding, blindings.at(slot as u32).unwrap());
        assert_eq!(framed.utxo_hash, output.hash(TREE).unwrap());
    }
    let published = rail.published_slots(&proof, &address(&sender)).unwrap();
    let assets = RingAssets::new([USDC]).unwrap();
    let body = decrypt(&receiver.viewing_key, &rail, &proof, &published[2], 2);
    let paid = note(
        reader(&recipient, &assets, &proof, 2)
            .open_body(&body)
            .unwrap(),
    );
    assert_eq!((paid.asset, paid.amount), (USDC, 4));
    for (slot, change) in [(0, (USDC, 6)), (1, (Mint::SOL, 5))] {
        let body = decrypt(
            &sender.viewing_key,
            &rail,
            &proof,
            &published[slot],
            slot as u32,
        );
        let kept = note(
            reader(&address(&sender), &assets, &proof, slot)
                .open_body(&body)
                .unwrap(),
        );
        assert_eq!((kept.asset, kept.amount), change);
    }
}

/// The change leads under the sender tag with no data.
#[test]
fn an_anonymous_transfer_seals_each_payment_at_its_position() {
    let sender = keypair(1);
    let receiver = keypair(2);
    let recipient = address(&receiver);
    let paid = payment(&recipient);
    let sender_tag = sender.get_sender_view_tag(0).unwrap();
    let (mut tx, tx_key) = ring_transfer(&sender, false);
    tx.transfer_sol(&recipient, 3).unwrap();
    let rail = anonymous(
        tx_key,
        AnonymousPlan::new()
            .with_payment(paid)
            .with_sender_tag(sender_tag),
    );
    let proof = rail.encrypt(tx, &address(&sender)).unwrap();
    let published = rail.published_slots(&proof, &address(&sender)).unwrap();
    let assets = RingAssets::new([]).unwrap();

    assert_eq!(proof.output_utxos[0].owner_address, Some(address(&sender)));
    assert_eq!(published[0].anonymous, Some(AnonymousSlot::Change));
    assert_eq!(
        published[1].anonymous,
        Some(AnonymousSlot::Payment {
            recipient: paid,
            tag: slot_tag(recipient.viewing_pubkey.u()),
        })
    );
    assert_eq!(
        proof.external_data.outputs[0].owner_tag,
        OwnerTag::Inline(slot_tag(sender_tag)),
        "a change slot publishes the sender tag"
    );
    assert_eq!(
        proof.external_data.outputs[1].owner_tag,
        OwnerTag::Inline(slot_tag(paid.u())),
        "a payment without a request publishes the bootstrap tag"
    );
    assert!(
        proof.external_data.messages.is_empty(),
        "the rail adds no message"
    );

    let keys = sealed_keys(&rail, &proof);
    assert_eq!(
        keys,
        [
            address(&sender).viewing_pubkey.u(),
            recipient.viewing_pubkey.u()
        ]
    );
    let change = change_at(&sender, &rail, &assets, &proof, 0);
    assert_eq!((change.asset, change.amount), (Mint::SOL, 7));

    let body = decrypt(&receiver.viewing_key, &rail, &proof, &published[1], 1);
    assert_eq!(body.record.mint, Mint::SOL.asset.to_bytes());
    let utxo = note(
        reader(&recipient, &assets, &proof, 1)
            .open_body(&body)
            .unwrap(),
    );
    assert_eq!(
        (utxo.amount, utxo.blinding),
        (3, proof.output_utxos[1].blinding)
    );
    assert_eq!(
        reader(&recipient, &assets, &proof, 0)
            .open_body(&body)
            .unwrap(),
        None,
        "a body rebuilds only the hash published at its own output"
    );
    let stranger = try_decrypt(&keypair(3).viewing_key, &rail, &proof, &published[1], 1);
    assert!(
        stranger
            .and_then(|body| reader(&recipient, &assets, &proof, 1)
                .open_body(&body)
                .unwrap())
            .is_none(),
        "another viewing key reads other bytes"
    );
}

#[test]
fn the_spl_and_the_sol_change_each_open_under_the_sender_key() {
    let sender = keypair(1);
    let recipient = address(&keypair(2));
    let (mut tx, tx_key) = ring_spend(
        &sender,
        vec![input(&sender, USDC, 10, 1), input(&sender, Mint::SOL, 5, 2)],
        false,
    );
    tx.transfer_with_ring(&recipient, USDC, 4, Some(RING))
        .unwrap();
    let sender_tag = sender.get_sender_view_tag(0).unwrap();
    let rail = anonymous(
        tx_key,
        AnonymousPlan::new()
            .with_payment(payment(&recipient))
            .with_sender_tag(sender_tag),
    );
    let proof = rail.encrypt(tx, &address(&sender)).unwrap();
    let published = rail.published_slots(&proof, &address(&sender)).unwrap();
    assert_eq!(
        (proof.output_utxos[0].asset, proof.output_utxos[1].asset),
        (USDC, Mint::SOL)
    );
    for change in &published[..2] {
        assert!(matches!(change.body, SlotBody::Recipient(_)));
    }
    // Equal tags would show the two change slots share an owner.
    assert_eq!(published[0].owner_tag, &slot_tag(sender_tag));
    assert_ne!(published[1].owner_tag, &slot_tag(sender_tag));
    let assets = RingAssets::new([USDC]).unwrap();
    let keys = sealed_keys(&rail, &proof);
    assert_eq!(keys[2], recipient.viewing_pubkey.u());
    for (position, (asset, amount)) in [(USDC, 6), (Mint::SOL, 5)].into_iter().enumerate() {
        assert_eq!(keys[position], address(&sender).viewing_pubkey.u());
        let utxo = change_at(&sender, &rail, &assets, &proof, position);
        assert_eq!((utxo.asset, utxo.amount), (asset, amount));
    }
    let body = decrypt(&keypair(2).viewing_key, &rail, &proof, &published[2], 2);
    assert_eq!(
        (body.record.mint, body.record.amount),
        (USDC.asset.to_bytes(), 4)
    );
}

#[test]
fn a_zero_amount_change_slot_opens_under_the_sender_key() {
    let sender = keypair(1);
    let (tx, tx_key) = ring_spend(&sender, vec![input(&sender, Mint::SOL, 0, 1)], false);
    let rail = anonymous(tx_key, AnonymousPlan::new());
    let mut tx = tx;
    tx.add_output_utxo(SppProofOutputUtxo {
        owner_address: Some(address(&sender)),
        asset: Mint::SOL,
        ring_program_id: Some(RING),
        ..Default::default()
    })
    .unwrap();
    let proof = rail.encrypt(tx, &address(&sender)).unwrap();
    let keys = sealed_keys(&rail, &proof);
    assert_eq!(keys[0], address(&sender).viewing_pubkey.u());
    let assets = RingAssets::new([]).unwrap();
    let change = change_at(&sender, &rail, &assets, &proof, 0);
    assert_eq!(change.amount, 0);
}

/// The reader rebuilds under its own key or the zero key the record's kind names, never a key it
/// does not hold.
#[test]
fn a_reader_opens_under_its_own_key_or_the_zero_key_alone() {
    let owner = address(&keypair(2));
    let blinding = [5; 32];
    let assets = RingAssets::new([USDC]).unwrap();
    let body = |key: RecordKey| OpenedRecipientBody {
        record: OutputRecord {
            key,
            ..record(&owner, USDC.asset, 9, blinding)
        },
        payload: PayloadRead::Absent,
    };
    let hash_under = |nullifier_pk: &[u8; 32]| {
        Utxo {
            owner: owner.signing_pubkey,
            asset: USDC,
            amount: 9,
            blinding,
            ring_program_id: Some(RING),
            data: Data::default(),
        }
        .hash(nullifier_pk, &[0; 32], &[0; 32], TREE)
        .unwrap()
    };
    let open = |hash: &[u8; 32], body: &OpenedRecipientBody| {
        AnonymousOutput {
            owner: &owner,
            assets: &assets,
            ring: RING,
            tree_id: TREE,
            utxo_hash: hash,
        }
        .open_body(body)
        .unwrap()
    };
    for (key, published) in [
        (owner.nullifier_pubkey, RecordKey::Unpublished),
        (ZERO_NULLIFIER_PUBKEY, RecordKey::Zero),
    ] {
        assert!(matches!(
            open(&hash_under(&key), &body(published)),
            Some(OpenedBody::Note(OpenedNote { nullifier_pk, .. })) if nullifier_pk == key
        ));
    }
    assert_eq!(
        open(&hash_under(&[0x11; 32]), &body(RecordKey::Unpublished)),
        None,
        "a note under a key the reader does not hold misses"
    );
    assert_eq!(
        open(&hash_under(&owner.nullifier_pubkey), &body(RecordKey::Zero)),
        None,
        "the zero key bit names the zero key alone"
    );

    let sender = keypair(1);
    let receiver = keypair(2);
    for delivery in both_rails(&owner) {
        for key in [owner.nullifier_pubkey, ZERO_NULLIFIER_PUBKEY, [0x11; 32]] {
            let named = ShieldedAddress {
                nullifier_pubkey: key,
                ..owner
            };
            let frame = pay(
                &sender,
                SppProofOutputUtxo::new(Mint::SOL, 3, named)
                    .unwrap()
                    .with_ring_program_id(RING),
                delivery.clone(),
            );
            let opened = reader(&owner, &assets, &frame.proof, frame.position)
                .open_body(&frame.body(&sender, &receiver.viewing_key))
                .unwrap();
            let rebuilt = opened.map(|opened| match opened {
                OpenedBody::Note(note) => note.nullifier_pk,
                other => panic!("a note expected, got {other:?}"),
            });
            assert_eq!(rebuilt, (key != [0x11; 32]).then_some(key));
        }
    }
}

#[test]
fn a_data_note_seals_on_both_rails_and_its_reader_rebuilds_the_commitment() {
    let sender = keypair(1);
    let receiver = keypair(2);
    let recipient = address(&receiver);
    let assets = RingAssets::new([]).unwrap();
    for delivery in both_rails(&recipient) {
        let frame = pay(&sender, data_note(recipient), delivery);
        let body = frame.body(&sender, &receiver.viewing_key);
        assert_eq!(body.record.data_hash, DATA_HASH);
        assert_eq!(body.record.ring_data_hash, RING_DATA_HASH);
        assert_eq!(payload(&body).data, APP_DATA);
        let opened = reader(&recipient, &assets, &frame.proof, frame.position)
            .open_body(&body)
            .unwrap();
        let Some(OpenedBody::Note(opened)) = opened else {
            panic!("a note expected, got {opened:?}")
        };
        let output = &frame.proof.output_utxos[frame.position];
        assert_eq!(opened.data_hash, Some(DATA_HASH));
        assert_eq!(opened.ring_data_hash, Some(RING_DATA_HASH));
        assert_eq!(opened.nullifier_pk, recipient.nullifier_pubkey);
        assert_eq!(opened.utxo.data, output.data);
        assert_eq!(
            (opened.utxo.amount, opened.utxo.blinding),
            (3, output.blinding)
        );
    }
}

/// The record's proven hashes rebuild the commitment, a lost payload costs the application data alone.
#[test]
fn a_data_note_rebuilds_from_its_record_with_the_payload_dropped() {
    let sender = keypair(1);
    let receiver = keypair(2);
    let recipient = address(&receiver);
    let assets = RingAssets::new([]).unwrap();
    for delivery in both_rails(&recipient) {
        let frame = pay(&sender, data_note(recipient), delivery);
        let body = frame.body(&sender, &receiver.viewing_key);
        let output = reader(&recipient, &assets, &frame.proof, frame.position);
        let opens = |body: OpenedRecipientBody| output.open_body(&body).unwrap();
        let dropped = opens(OpenedRecipientBody {
            payload: PayloadRead::Malformed,
            ..body.clone()
        });
        let Some(OpenedBody::Note(note)) = dropped else {
            panic!("a note expected, got {dropped:?}")
        };
        assert_eq!(note.data_hash, Some(DATA_HASH));
        assert_eq!(note.ring_data_hash, Some(RING_DATA_HASH));
        assert_eq!(note.utxo.data, Data::default());
        // A record with another hash rebuilds nothing.
        for record in [
            OutputRecord {
                data_hash: [3; 32],
                ..body.record
            },
            OutputRecord {
                ring_data_hash: [3; 32],
                ..body.record
            },
            OutputRecord {
                data_hash: [0; 32],
                ..body.record
            },
        ] {
            assert_eq!(
                opens(OpenedRecipientBody {
                    record,
                    ..body.clone()
                }),
                None
            );
        }
    }
}

/// A recipient holds no frame, the body's length and the commitment select the form.
#[test]
fn a_reader_without_the_frame_opens_every_form_from_the_body() {
    let sender = keypair(1);
    let receiver = keypair(2);
    let recipient = address(&receiver);
    let assets = RingAssets::new([]).unwrap();
    let plain = SppProofOutputUtxo::new(Mint::SOL, 3, recipient)
        .unwrap()
        .with_ring_program_id(RING);
    let ring_data = SppProofOutputUtxo::new(Mint::SOL, 3, recipient)
        .unwrap()
        .with_ring_data_hash(RING, RING_DATA_HASH);
    let data = SppProofOutputUtxo::new(Mint::SOL, 3, recipient)
        .unwrap()
        .with_ring_program_id(RING)
        .with_utxo_data(APP_DATA.to_vec(), DATA_HASH);
    for (output, record_len) in [
        (plain, 105),
        (ring_data, 137),
        (data, 137),
        (data_note(recipient), 169),
    ] {
        for delivery in both_rails(&recipient) {
            let frame = pay(&sender, output.clone(), delivery);
            let published = frame
                .rail
                .published_slots(&frame.proof, &address(&sender))
                .unwrap();
            let slot = &published[frame.position];
            assert_eq!(slot.record().len(), record_len);
            let SlotBody::Recipient(ciphertext) = slot.body else {
                panic!("recipient slot expected")
            };
            let secret = delivered_secret(
                &receiver.viewing_key,
                &frame.proof,
                &frame.rail,
                frame.position as u32,
            );
            let opened = reader(&recipient, &assets, &frame.proof, frame.position)
                .open(ciphertext, &secret)
                .unwrap();
            let Some(OpenedBody::Note(note)) = opened else {
                panic!("a note expected, got {opened:?}")
            };
            let sealed = &frame.proof.output_utxos[frame.position];
            assert_eq!(note.data_hash, sealed.data_hash);
            assert_eq!(note.ring_data_hash, sealed.ring_data_hash);
            assert_eq!(note.utxo.data.utxo_data(), sealed.data.utxo_data());
        }
    }
}

/// An exit record publishes no ring data hash, the sealer refuses before any proof.
#[test]
fn an_exit_under_a_ring_data_hash_is_refused() {
    let exit = SppProofOutputUtxo {
        ring_data_hash: Some(RING_DATA_HASH),
        ..SppProofOutputUtxo::new(Mint::SOL, 3, address(&keypair(2))).unwrap()
    };
    assert!(matches!(
        BodyForm::of(std::slice::from_ref(&exit)),
        Err(E::ExitRingData { slot_index: 0 })
    ));
    let sender = keypair(1);
    let (mut tx, tx_key) = ring_transfer(&sender, false);
    tx.add_output_utxo(exit).unwrap();
    assert!(matches!(
        delivered(tx_key).encrypt(tx, &address(&sender)),
        Err(E::ExitRingData { .. })
    ));
}

#[test]
fn a_rebuilt_body_of_an_unregistered_mint_names_the_mint() {
    let owner = address(&keypair(2));
    let mint = Address::new_from_array([0x44; 32]);
    let utxo = Utxo {
        owner: owner.signing_pubkey,
        asset: Mint::new(mint, 0),
        amount: 9,
        blinding: [5; 32],
        ring_program_id: Some(RING),
        data: Data::default(),
    };
    let hash = utxo
        .hash(&owner.nullifier_pubkey, &[0; 32], &[0; 32], TREE)
        .unwrap();
    let assets = RingAssets::new([USDC]).unwrap();
    let opened = AnonymousOutput {
        owner: &owner,
        assets: &assets,
        ring: RING,
        tree_id: TREE,
        utxo_hash: &hash,
    }
    .open_body(&OpenedRecipientBody {
        record: record(&owner, mint, 9, [5; 32]),
        payload: PayloadRead::Absent,
    })
    .unwrap();
    assert!(matches!(
        opened,
        Some(OpenedBody::UnknownMint(OpenedNote { utxo, .. })) if utxo.asset.asset == mint && utxo.amount == 9
    ));
}

#[test]
fn a_dummy_publishes_a_random_tag_and_a_recipient_length_body() {
    let sender = keypair(1);
    let recipient = address(&keypair(2));
    let (mut tx, tx_key) = ring_transfer(&sender, false);
    tx.transfer_sol(&recipient, 3).unwrap();
    let rail = anonymous(
        tx_key.clone(),
        AnonymousPlan::new().with_payment(payment(&recipient)),
    );
    let proof = ShapedSeal {
        rail: &rail,
        shape: Shape::IN1_OUT4,
        sender: &address(&sender),
    }
    .encrypt(tx)
    .unwrap();
    let published = rail.published_slots(&proof, &address(&sender)).unwrap();
    for dummy in &published[2..] {
        assert_eq!(dummy.anonymous, Some(AnonymousSlot::Dummy));
        assert!(matches!(
            dummy.body,
            SlotBody::Recipient(body) if body.len() == RING_RECORD_LEN
        ));
    }
    // The untagged change and each dummy draw their own tag at the slot width.
    let drawn = std::collections::BTreeSet::from([0, 2, 3].map(|slot| *published[slot].owner_tag));
    assert_eq!(drawn.len(), 3, "a constant tag would mark its slot");
    assert!(drawn.iter().all(|tag| tag[..24] == [0; 24]));
    let keys = sealed_keys(&rail, &proof);
    assert_eq!(
        keys[..2],
        [
            address(&sender).viewing_pubkey.u(),
            recipient.viewing_pubkey.u()
        ]
    );
    // A dummy seals to a key derived from the transaction secret, never a party's.
    assert!(keys[2..]
        .iter()
        .all(|key| *key != address(&sender).viewing_pubkey.u() && *key != [0; 32]));
}

#[test]
fn a_transfer_that_returns_nothing_leads_with_a_zero_change_in_its_spare_slot() {
    let sender = keypair(1);
    let receiver = keypair(2);
    let recipient = address(&receiver);
    let (mut tx, tx_key) = ring_transfer(&sender, false);
    tx.transfer_sol(&recipient, 10).unwrap();
    let rail = anonymous(
        tx_key,
        AnonymousPlan::new().with_payment(payment(&recipient)),
    );
    let proof = rail.encrypt(tx, &address(&sender)).unwrap();
    let published = rail.published_slots(&proof, &address(&sender)).unwrap();
    assert_eq!(published[0].anonymous, Some(AnonymousSlot::Change));
    assert_eq!(
        (proof.output_utxos[0].asset, proof.output_utxos[0].amount),
        (Mint::SOL, 0)
    );
    let keys = sealed_keys(&rail, &proof);
    assert_eq!(
        keys[..2],
        [
            address(&sender).viewing_pubkey.u(),
            recipient.viewing_pubkey.u()
        ]
    );
    let body = decrypt(&receiver.viewing_key, &rail, &proof, &published[1], 1);
    assert_eq!(body.record.amount, 10);
}

#[test]
fn each_change_output_of_any_asset_seals_to_the_sender() {
    let sender = keypair(1);
    let (mut tx, tx_key) = ring_spend(&sender, vec![input(&sender, USDC, 10, 1)], false);
    for _ in 0..2 {
        tx.transfer_with_ring(&address(&sender), USDC, 3, Some(RING))
            .unwrap();
    }
    let rail = anonymous(tx_key, AnonymousPlan::new());
    let proof = rail.encrypt(tx, &address(&sender)).unwrap();
    let keys = sealed_keys(&rail, &proof);
    // The shape's padding dummy seals to its derived key.
    let sender_u = address(&sender).viewing_pubkey.u();
    assert_eq!(keys[..3], [sender_u, sender_u, sender_u]);
    assert_ne!(keys[3], sender_u);
    let assets = RingAssets::new([USDC]).unwrap();
    let mut amounts: Vec<u64> = (0..3)
        .map(|slot| change_at(&sender, &rail, &assets, &proof, slot))
        .inspect(|utxo| assert_eq!(utxo.asset, USDC))
        .map(|utxo| utxo.amount)
        .collect();
    amounts.sort_unstable();
    assert_eq!(amounts, [3, 3, 4]);
}

#[test]
fn change_outputs_past_the_output_count_are_refused_before_any_proof() {
    let sender = keypair(1);
    let (mut tx, tx_key) = ring_spend(&sender, vec![input(&sender, USDC, 10, 1)], false);
    for _ in 0..2 {
        tx.transfer_with_ring(&address(&sender), USDC, 3, Some(RING))
            .unwrap();
    }
    let rail = anonymous(tx_key, AnonymousPlan::new());
    let mut tx = tx
        .with_blinding_seed(rail.blinding_seed().unwrap())
        .unwrap();
    assert!(matches!(
        tx.pad_utxos(Shape::IN1_OUT2, &address(&sender)).err(),
        Some(TransactionError::TooManyOutputsForShape { got: 3, max: 2 })
    ));
}

/// A recipient at the sender's address under another nullifier key is a payment, not change.
#[test]
fn a_recipient_differing_in_its_nullifier_key_is_no_change() {
    let sender = keypair(1);
    let (mut tx, tx_key) = ring_transfer(&sender, false);
    let other_key = ShieldedAddress {
        nullifier_pubkey: ZERO_NULLIFIER_PUBKEY,
        ..address(&sender)
    };
    tx.transfer_with_ring(&other_key, Mint::SOL, 3, Some(RING))
        .unwrap();
    let rail = anonymous(
        tx_key,
        AnonymousPlan::new().with_payment(payment(&other_key)),
    );
    let proof = rail.encrypt(tx, &address(&sender)).unwrap();
    let published = rail.published_slots(&proof, &address(&sender)).unwrap();
    assert!(published[0].anonymous.unwrap().is_change());
    assert!(matches!(
        published[1].anonymous,
        Some(AnonymousSlot::Payment { .. })
    ));
}

#[test]
fn compact_padding_omits_a_suffix_and_keeps_every_index() {
    let sender = keypair(1);
    let receiver = keypair(2);
    let recipient = address(&receiver);
    let (mut tx, tx_key) = ring_transfer(&sender, true);
    tx.transfer_sol(&recipient, 3).unwrap();
    let rail = anonymous(
        tx_key,
        AnonymousPlan::new().with_payment(payment(&recipient)),
    );
    let proof = ShapedSeal {
        rail: &rail,
        shape: Shape::IN1_OUT4,
        sender: &address(&sender),
    }
    .encrypt(tx)
    .unwrap();
    assert_eq!(proof.output_utxos.len(), 4);
    assert_eq!(proof.external_data.outputs.len(), 2);
    let published = rail.published_slots(&proof, &address(&sender)).unwrap();
    assert_eq!(published.len(), 2);
    let body = decrypt(&receiver.viewing_key, &rail, &proof, &published[1], 1);
    assert_eq!(body.record.blinding, proof.output_utxos[1].blinding);
    let assets = RingAssets::new([]).unwrap();
    assert!(reader(&recipient, &assets, &proof, 1)
        .open_body(&body)
        .unwrap()
        .is_some());
}

#[test]
fn a_delivered_rail_omits_compact_padding_from_the_frame() {
    let sender = keypair(1);
    let recipient = address(&keypair(2));
    let (tx, tx_key) = ring_transfer(&sender, true);
    let rail = delivered(tx_key);
    let mut tx = tx
        .with_blinding_seed(rail.blinding_seed().unwrap())
        .unwrap();
    tx.transfer_sol(&recipient, 3).unwrap();
    let proof = ShapedSeal {
        rail: &rail,
        shape: Shape::IN1_OUT4,
        sender: &address(&sender),
    }
    .encrypt(tx)
    .unwrap();
    assert_eq!(proof.external_data.outputs.len(), 2);
    assert_eq!(
        rail.published_slots(&proof, &address(&sender))
            .unwrap()
            .len(),
        2
    );
}

fn reply_of(sender: &ShieldedAddress) -> ReplyMetadata {
    ReplyMetadata {
        viewing_key: sender.viewing_pubkey,
        owner_key: sender.signing_pubkey.as_ed25519().unwrap(),
        nullifier_pk: sender.nullifier_pubkey,
        data: Data::default(),
    }
}

#[test]
fn no_default_path_attaches_a_reply() {
    let sender = keypair(1);
    let recipient = address(&keypair(2));
    let (mut tx, tx_key) = ring_transfer(&sender, false);
    tx.transfer_sol(&recipient, 3).unwrap();
    let rail = anonymous(
        tx_key,
        AnonymousPlan::new().with_payment(payment(&recipient)),
    );
    let proof = rail.encrypt(tx, &address(&sender)).unwrap();
    let published = rail.published_slots(&proof, &address(&sender)).unwrap();
    assert!(published.iter().all(|slot| slot.reply.is_empty()));
}

/// A slot without a reply and a dummy publish the same length, a reply never names its slot.
#[test]
fn a_reply_frame_gives_every_body_one_length() {
    let sender = keypair(1);
    let receiver = keypair(2);
    let recipient = address(&receiver);
    let unanswered = keypair(3);
    let other = address(&unanswered);
    let (mut tx, tx_key) = ring_transfer(&sender, false);
    tx.transfer_sol(&recipient, 3).unwrap();
    tx.transfer_sol(&other, 2).unwrap();
    let rail = anonymous(
        tx_key,
        AnonymousPlan::new()
            .with_payment(payment(&recipient))
            .with_reply(reply_of(&address(&sender)))
            .with_payment(payment(&other)),
    );
    let proof = ShapedSeal {
        rail: &rail,
        shape: Shape::IN1_OUT4,
        sender: &address(&sender),
    }
    .encrypt(tx)
    .unwrap();
    let published = rail.published_slots(&proof, &address(&sender)).unwrap();
    for slot in &published {
        assert_eq!(slot.reply.len(), MIN_REPLY_LEN);
    }
    // Random bytes on a slot its owner opens would decode as a reply by chance.
    for (slot, owner) in [(0u32, &sender), (2, &unanswered)] {
        let secret = delivered_secret(&owner.viewing_key, &proof, &rail, slot);
        let reply = &published[slot as usize].reply;
        let empty = seal_no_reply(MIN_REPLY_LEN, &secret).unwrap();
        assert_eq!(*reply, empty.as_slice());
        assert!(open_reply(reply, &secret).unwrap().is_none());
    }
    let data = proof.external_data.outputs[1].data.as_ref().unwrap();
    let (_, reply) = ring_body_output(data).unwrap();
    let secret = delivered_secret(&receiver.viewing_key, &proof, &rail, 1);
    let claim = open_reply(reply, &secret).unwrap().expect("a reply opens");
    assert_eq!(claim.claimed(), &reply_of(&address(&sender)));
}

#[test]
fn a_reply_longer_than_its_length_is_refused_and_padding_is_checked() {
    let sender = address(&keypair(1));
    let secret = SlotSecret::new([3; 32]).unwrap();
    assert!(matches!(
        seal_reply(&reply_of(&sender), MIN_REPLY_LEN - 1, &secret),
        Err(E::ReplyMetadataLength { .. })
    ));
    let sealed = seal_reply(&reply_of(&sender), MIN_REPLY_LEN + 8, &secret).unwrap();
    assert!(open_reply(&sealed, &secret).unwrap().is_some());
    let mut tampered = sealed;
    *tampered.last_mut().unwrap() ^= 1;
    assert!(open_reply(&tampered, &secret).unwrap().is_none());
}

#[test]
fn a_reply_of_a_zero_or_non_canonical_viewing_key_opens_nothing() {
    let reply = reply_of(&address(&keypair(1)));
    let secret = SlotSecret::new([3; 32]).unwrap();
    let sealed = seal_reply(&reply, MIN_REPLY_LEN, &secret).unwrap();
    let held = reply.viewing_key.u();
    let claimed = |viewing_u: [u8; 32]| {
        let mut forged = sealed.clone();
        for (byte, (held, claimed)) in forged.iter_mut().zip(held.iter().zip(viewing_u)) {
            *byte ^= held ^ claimed;
        }
        open_reply(&forged, &secret).unwrap()
    };
    assert!(claimed(held).is_some());
    assert!(claimed([0; 32]).is_none());
    let mut modulus = [0xff; 32];
    modulus[0] = 0xed;
    modulus[31] = 0x7f;
    assert!(claimed(modulus).is_none());
}

/// One label per kind, a reply never restarts the body's counter under one slot secret.
#[test]
fn a_body_and_a_reply_under_one_secret_read_different_keystreams() {
    let sender = address(&keypair(1));
    let secret = SlotSecret::new([3; 32]).unwrap();
    let body = RecipientBody {
        record: &OutputRecord::default(),
        form: RecordForm::default(),
        payload: None,
    }
    .seal(&secret)
    .unwrap();
    let reply = seal_reply(&reply_of(&sender), MIN_REPLY_LEN, &secret).unwrap();
    let head = reply_of(&sender);
    let keystream = |index: usize, plain: u8| reply[index] ^ plain;
    assert_ne!(
        (0..32)
            .map(|index| keystream(index, head.viewing_key.u()[index]))
            .collect::<Vec<_>>(),
        body[..32],
    );
    assert_eq!(RING_RECORD_LEN, body.len());
}

#[test]
fn an_exit_to_the_hidden_sender_is_refused() {
    let sender = keypair(1);
    let (mut tx, tx_key) = ring_transfer(&sender, false);
    let other_view = ShieldedAddress {
        viewing_pubkey: address(&keypair(2)).viewing_pubkey,
        ..address(&sender)
    };
    tx.transfer_with_ring(&other_view, Mint::SOL, 2, None)
        .unwrap();
    assert!(matches!(
        anonymous(tx_key, AnonymousPlan::new()).encrypt(tx, &address(&sender)),
        Err(E::RingSenderExit { slot_index: 1 })
    ));
}

#[test]
fn an_exit_publishes_scheme_3_under_its_owner_tag() {
    let sender = keypair(1);
    let exit = address(&keypair(3));
    let (mut tx, tx_key) = ring_transfer(&sender, false);
    tx.transfer_with_ring(&exit, Mint::SOL, 2, None).unwrap();
    let rail = anonymous(tx_key, AnonymousPlan::new());
    let proof = rail.encrypt(tx, &address(&sender)).unwrap();
    let published = rail.published_slots(&proof, &address(&sender)).unwrap();
    assert_eq!(published[1].anonymous, Some(AnonymousSlot::Exit));
    assert!(matches!(published[1].body, SlotBody::Exit(_)));
    assert_eq!(
        proof.external_data.outputs[1].owner_tag,
        OwnerTag::Inline(exit.signing_pubkey.confidential_view_tag().unwrap())
    );
    let keys = sealed_keys(&rail, &proof);
    assert_eq!(
        keys[1],
        auditor().u(),
        "an exit seals to the auditor, its owner is never listed"
    );
}

#[test]
fn an_exit_publishes_its_memo_on_both_rails() {
    let sender = keypair(1);
    let exit = address(&keypair(3));
    for delivery in [
        RingDelivery::Delivered,
        RingDelivery::Anonymous(Box::new(AnonymousPlan::new())),
    ] {
        let output = SppProofOutputUtxo::new(Mint::SOL, 2, exit)
            .unwrap()
            .with_memo(vec![1]);
        let frame = pay(&sender, output, delivery);
        let published = frame
            .rail
            .published_slots(&frame.proof, &address(&sender))
            .unwrap();
        assert!(matches!(
            published[frame.position].body,
            SlotBody::Exit(payload) if payload.len() > CONFIDENTIAL_DATA_FREE_BODY_LEN
        ));
    }
}

#[test]
fn an_anonymous_ring_output_to_another_owner_needs_a_planned_payment() {
    let sender = keypair(1);
    let recipient = address(&keypair(2));
    let (mut tx, tx_key) = ring_transfer(&sender, false);
    tx.transfer_sol(&recipient, 3).unwrap();
    assert!(matches!(
        anonymous(tx_key, AnonymousPlan::new()).encrypt(tx, &address(&sender)),
        Err(E::UnplannedRingOutput { slot_index: 1 })
    ));
}

#[test]
fn a_payment_planned_for_another_owner_is_refused() {
    let sender = keypair(1);
    let recipient = address(&keypair(2));
    let other = address(&keypair(3));
    let (mut tx, tx_key) = ring_transfer(&sender, false);
    tx.transfer_sol(&recipient, 3).unwrap();
    let rail = anonymous(tx_key, AnonymousPlan::new().with_payment(payment(&other)));
    assert!(matches!(
        rail.encrypt(tx, &address(&sender)),
        Err(E::RingPaymentSlot { slot_index: 1 })
    ));
}

#[test]
fn a_dataless_delivered_slot_takes_the_plain_body_length() {
    let sender = keypair(1);
    let recipient = address(&keypair(2));
    let (tx, tx_key) = ring_transfer(&sender, false);
    let rail = delivered(tx_key);
    let mut tx = tx
        .with_blinding_seed(rail.blinding_seed().unwrap())
        .unwrap();
    tx.transfer_sol(&recipient, 3).unwrap();
    let proof = ShapedSeal {
        rail: &rail,
        shape: Shape::IN1_OUT4,
        sender: &address(&sender),
    }
    .encrypt(tx)
    .unwrap();
    let published = rail.published_slots(&proof, &address(&sender)).unwrap();
    assert!(published.iter().any(|slot| slot.output.is_dummy()));
    for slot in &published {
        assert!(matches!(
            slot.body,
            SlotBody::Recipient(body) if body.len() == RING_RECORD_LEN
        ));
    }
}

/// A tag on another payment's slot hides both payments from their recipients.
#[test]
fn each_payment_publishes_the_tag_its_plan_names() {
    let sender = keypair(1);
    let [bootstrap, requested, untagged] = [2, 3, 4].map(|seed| address(&keypair(seed)));
    let (mut tx, tx_key) = ring_transfer(&sender, false);
    for recipient in [&bootstrap, &requested, &untagged] {
        tx.transfer_sol(recipient, 1).unwrap();
    }
    let request_tag = keypair(3).get_recipient_request_view_tag(0).unwrap();
    let rail = anonymous(
        tx_key,
        AnonymousPlan::new()
            .with_payment(payment(&bootstrap))
            .with_payment(payment(&requested))
            .with_tag(request_tag)
            .with_payment(payment(&untagged))
            .with_random_tag(),
    );
    let proof = rail.encrypt(tx, &address(&sender)).unwrap();
    let published = rail.published_slots(&proof, &address(&sender)).unwrap();
    assert_eq!(
        published[1].owner_tag,
        &slot_tag(bootstrap.viewing_pubkey.u())
    );
    assert_eq!(published[2].owner_tag, &slot_tag(request_tag));
    assert_ne!(
        published[3].owner_tag,
        &slot_tag(untagged.viewing_pubkey.u()),
        "no tag finds an untagged payment"
    );
    assert_eq!(published[3].owner_tag[..24], [0; 24]);
}

/// Equal tags on two slots would show they share a recipient, one match returns both slots.
#[test]
fn a_frame_publishes_a_repeated_tag_once() {
    let sender = keypair(1);
    let recipient = address(&keypair(2));
    let (mut tx, tx_key) = ring_transfer(&sender, false);
    for _ in 0..2 {
        tx.transfer_sol(&recipient, 1).unwrap();
    }
    let rail = anonymous(
        tx_key,
        AnonymousPlan::new()
            .with_payment(payment(&recipient))
            .with_payment(payment(&recipient)),
    );
    let proof = rail.encrypt(tx, &address(&sender)).unwrap();
    let published = rail.published_slots(&proof, &address(&sender)).unwrap();
    let bootstrap = slot_tag(recipient.viewing_pubkey.u());
    assert_eq!(published[1].owner_tag, &bootstrap);
    assert_ne!(published[2].owner_tag, &bootstrap);
    assert_eq!(published[2].owner_tag[..24], [0; 24]);
}

/// Data under a zero hash binds to no commitment, the note opens without it.
#[test]
fn a_body_with_data_under_a_zero_hash_opens_without_the_data() {
    let owner = keypair(1);
    let owner_address = address(&owner);
    let utxo = Utxo {
        owner: owner_address.signing_pubkey,
        asset: Mint::SOL,
        amount: 5,
        blinding: [7; 32],
        ring_program_id: Some(RING),
        data: Data::default(),
    };
    let hash = utxo
        .hash(&owner_address.nullifier_pubkey, &[0; 32], &[0; 32], TREE)
        .unwrap();
    let assets = RingAssets::new([]).unwrap();
    let output = AnonymousOutput {
        owner: &owner_address,
        assets: &assets,
        ring: RING,
        tree_id: TREE,
        utxo_hash: &hash,
    };
    let body = OpenedRecipientBody {
        record: record(&owner_address, Mint::SOL.asset, 5, [7; 32]),
        payload: PayloadRead::Opened(RecipientPayload {
            data: b"unbound".to_vec(),
        }),
    };
    assert_eq!(note(output.open_body(&body).unwrap()), utxo);
}

#[test]
fn a_memo_on_a_ring_output_is_refused_on_both_rails() {
    let owner = keypair(1);
    let recipient = address(&keypair(2));
    let sender = address(&owner);
    let transaction = |memo: bool| {
        let (mut tx, _) = ring_spend(&owner, vec![input(&owner, Mint::SOL, 10, 0)], false);
        let output = SppProofOutputUtxo {
            owner_address: Some(recipient),
            asset: Mint::SOL,
            amount: 4,
            ring_program_id: Some(RING),
            ..Default::default()
        };
        let output = match memo {
            true => output.with_memo(b"lost".to_vec()),
            false => output,
        };
        tx.add_output_utxo(output).unwrap();
        tx
    };
    for memo in [false, true] {
        let plain = delivered(ViewingKey::new()).encrypt(transaction(memo), &sender);
        let plan = AnonymousPlan::new().with_payment(recipient.viewing_pubkey);
        let hidden = anonymous(ViewingKey::new(), plan).encrypt(transaction(memo), &sender);
        for sealed in [plain, hidden] {
            match memo {
                true => assert!(matches!(
                    sealed,
                    Err(zolana_ring_transaction::RingRailError::OutputDataNotCarried { .. })
                )),
                false => assert!(sealed.is_ok()),
            }
        }
    }
}

#[test]
fn data_padding_hides_real_ring_body_lengths_on_both_rails() {
    let sender = keypair(1);
    let receiver = keypair(2);
    for delivery in both_rails(&address(&receiver)) {
        let (mut transaction, tx_key) = ring_transfer(&sender, false);
        transaction
            .add_output_utxo(data_note(address(&receiver)))
            .unwrap();
        let rail = RingOutputRail::new(tx_key, auditor(), delivery);
        let proof = ShapedSeal {
            rail: &rail,
            shape: Shape::new(1, 8),
            sender: &address(&sender),
        }
        .encrypt(transaction)
        .unwrap();
        let slots = rail.published_slots(&proof, &address(&sender)).unwrap();
        let widths: Vec<_> = slots
            .iter()
            .filter_map(|slot| match slot.body {
                SlotBody::Recipient(body) => Some(body.len()),
                _ => None,
            })
            .collect();
        assert!(widths.len() > 1);
        assert!(widths.iter().all(|width| *width == widths[0]));
        assert!(widths[0] > RING_RECORD_LEN);
        let index = proof
            .output_utxos
            .iter()
            .position(|output| output.owner_address == Some(address(&receiver)))
            .unwrap();
        let body = decrypt(
            &receiver.viewing_key,
            &rail,
            &proof,
            &slots[index],
            index as u32,
        );
        assert_eq!(payload(&body).data, APP_DATA);
    }
}
