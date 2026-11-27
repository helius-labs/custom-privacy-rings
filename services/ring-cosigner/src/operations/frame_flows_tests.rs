use solana_signature::Signature;
use spr_indexer_api::Hash;
use spr_keypair::{NullifierKey, PublicKey, X25519Keypair};
use spr_transaction::{Mint, OutputContext};
use zolana_ring_client::{
    AuditedEntryInput, AuditedOutput, AuditedUtxo, CreatedUtxo, KnownUtxo, OutputPayloadError,
    OutputPayloadErrorKind, RingOwnerTag, UtxoOwner,
};

use super::*;
pub use ring_audit::{
    tracking::HeldEnrollment,
    utxos::{CreatedIn, PageWork},
};

const ALICE: [u8; 32] = [0xa1; 32];
const BOB: [u8; 32] = [0xb2; 32];
const RING: Address = Address::new_from_array([0x44; 32]);

fn mint_of_asset() -> Address {
    Address::new_from_array([0x07; 32])
}

fn assets() -> RingAssets {
    RingAssets::new([Mint::new(mint_of_asset(), 6)]).expect("assets")
}

fn asset_field() -> [u8; 32] {
    assets()
        .fields()
        .find(|(_, mint)| mint.asset == mint_of_asset())
        .expect("registered")
        .0
}

fn owner_identity(owner_key: &[u8; 32]) -> [u8; 32] {
    *Member::owner_tag(owner_key).expect("identity").as_bytes()
}

fn key_of(owner_key: &[u8; 32]) -> NullifierKey {
    NullifierKey::from_secret([owner_key[0] & 0x1f; 31])
}

/// `commitment` stays a canonical field element.
fn note(owner_key: [u8; 32], commitment: u8, amount: u64) -> CreatedUtxo {
    CreatedUtxo {
        commitment: [commitment; 32],
        slot_index: 0,
        leaf_index: 0,
        utxo: KnownUtxo {
            owner_key,
            owner: UtxoOwner::Keyed {
                nullifier_pk: key_of(&owner_key).pubkey().expect("key"),
            },
            asset: asset_field(),
            mint: None,
            amount,
            blinding: [5; 32],
            data_hash: Some([0; 32]),
            ring_data_hash: Some([0; 32]),
            tree_id: 1,
        },
        ring_note: true,
    }
}

fn frame() -> CreatedIn {
    CreatedIn {
        slot: 1,
        tx_signature: Signature::from([7; 64]),
        event_index: 0,
    }
}

async fn store_with(notes: &[CreatedUtxo]) -> AuditStore {
    let store =
        AuditStore::on_internal_database(RING, &X25519Keypair::from_bytes(&[0x41; 32])).await;
    let rows = notes
        .iter()
        .map(|note| HeldEnrollment::unsealed(note.utxo.owner_key, &key_of(&note.utxo.owner_key)))
        .collect();
    let admission = store.enrollment_admission(rows).await.expect("admission");
    store.admit_enrollments(admission);
    let opener = |held: &HeldEnrollment| Some(key_of(&held.owner_key));
    let mut work = PageWork::new(&store, &opener);
    work.open(frame(), notes.iter().copied());
    store.commit(work.finish().await.expect("page"));
    store
}

/// One published input as the proof leaves it, a nullifier and, for an entry, its note-order record.
enum Input {
    Note([u8; 32]),
    Entry([u8; 32], AuditedEntryInput),
    Padding([u8; 32]),
}

impl Input {
    fn nullifier(&self) -> [u8; 32] {
        match self {
            Self::Note(nullifier) | Self::Entry(nullifier, _) | Self::Padding(nullifier) => {
                *nullifier
            }
        }
    }
}

fn spend(created: &CreatedUtxo) -> Input {
    Input::Note(
        key_of(&created.utxo.owner_key)
            .nullifier(&created.commitment, &created.utxo.blinding)
            .expect("nullifier"),
    )
}

fn utxo(owner_key: [u8; 32], amount: u64) -> AuditedUtxo {
    AuditedUtxo {
        owner_key,
        owner: UtxoOwner::Keyed {
            nullifier_pk: [3; 32],
        },
        asset: asset_field(),
        mint: None,
        amount,
        blinding: [6; 32],
        data_hash: Some([0; 32]),
        ring_data_hash: Some([0; 32]),
    }
}

fn entry(owner_key: [u8; 32], nullifier: u8, amount: u64) -> Input {
    Input::Entry(
        [nullifier; 32],
        AuditedEntryInput {
            owner_key,
            nullifier_pk: [3; 32],
            asset: asset_field(),
            mint: mint_of_asset(),
            amount,
            commitment: [nullifier ^ 0x0f; 32],
        },
    )
}

fn slot(slot_index: u32, opening: OutputOpening) -> AuditedOutput {
    AuditedOutput {
        slot_index,
        context: OutputContext {
            hash: [slot_index as u8 + 1; 32],
            tree_id: 1,
            leaf_index: 0,
        },
        opening,
    }
}

fn ring_slot(index: u32, owner_key: [u8; 32], amount: u64) -> AuditedOutput {
    slot(
        index,
        OutputOpening::Ring {
            utxo: utxo(owner_key, amount),
            owner_tag: RingOwnerTag([0x33; 32]),
        },
    )
}

fn audited(inputs: Vec<Input>, outputs: Vec<AuditedOutput>) -> AuditedTransaction {
    AuditedTransaction {
        tx_signature: Signature::from([9; 64]),
        slot: 2,
        tx_viewing_pk: X25519Keypair::from_bytes(&[3; 32]).pubkey(),
        outputs,
        payload_errors: Vec::new(),
        real_inputs: inputs
            .iter()
            .filter(|input| !matches!(input, Input::Padding(_)))
            .count() as u8,
        nullifiers: inputs.iter().map(Input::nullifier).collect(),
        entries: inputs
            .into_iter()
            .filter_map(|input| match input {
                Input::Entry(_, entry) => Some(entry),
                Input::Note(_) | Input::Padding(_) => None,
            })
            .collect(),
        voucher: None,
    }
}

async fn read(
    store: &AuditStore,
    audited: &AuditedTransaction,
    public: Vec<PublicFlow>,
) -> Result<TransferRead, RingCosignerError> {
    let notes = real_input_notes(store, audited).await?;
    FrameFlows {
        audited,
        notes: &notes,
        owner_signers: Vec::new(),
        public,
        assets: &assets(),
        ring: RING,
    }
    .read(0)
}

fn outflows(flows: &[OwnerFlow]) -> Vec<([u8; 32], u128)> {
    flows
        .iter()
        .map(|flow| (*flow.owner.as_bytes(), flow.flow.charged_outflow))
        .collect()
}

fn refused<T>(result: Result<T, RingCosignerError>) -> bool {
    matches!(
        result,
        Err(RingCosignerError::CoSign(CoSignError::LimitsUnavailable))
    )
}

fn public(withdrawn: u64, deposited: u64) -> PublicFlow {
    PublicFlow {
        mint: mint_of_asset(),
        asset: asset_field(),
        deposited: deposited.into(),
        withdrawn: withdrawn.into(),
    }
}

/// The vector Go `TestRingSecretsMatchRustVectors` holds `SolanaOwnerIdentity` to.
#[test]
fn the_reservation_key_is_the_identity_a_deposit_and_the_circuit_hold() {
    let wallet = PublicKey::from_ed25519(&ALICE);
    assert_eq!(
        owner_identity(&ALICE),
        wallet.owner_proof_input_hash().expect("identity")
    );
}

#[tokio::test]
async fn change_returns_to_its_owner_and_a_credit_never_moves() {
    let (alice, bob) = (note(ALICE, 0x10, 30), note(BOB, 0x11, 10));
    let store = store_with(&[alice, bob]).await;
    let read = read(
        &store,
        &audited(
            vec![spend(&alice), spend(&bob)],
            vec![ring_slot(0, ALICE, 5), ring_slot(1, BOB, 35)],
        ),
        Vec::new(),
    )
    .await
    .expect("rows");
    assert_eq!(outflows(&read.flows), [(owner_identity(&ALICE), 25)]);
    assert!(read.spends_note);
}

#[tokio::test]
async fn an_exit_to_the_same_owner_counts_as_leaving() {
    let alice = note(ALICE, 0x10, 30);
    let store = store_with(&[alice]).await;
    let exit = slot(0, OutputOpening::Exit(utxo(ALICE, 30)));
    let read = read(
        &store,
        &audited(vec![spend(&alice)], vec![exit]),
        Vec::new(),
    )
    .await
    .expect("rows");
    assert_eq!(outflows(&read.flows), [(owner_identity(&ALICE), 30)]);
}

/// A withdrawal of funds a deposit let the owner keep still leaves, the shared formula charges it.
#[tokio::test]
async fn a_withdrawal_reads_the_statement_gross_and_never_an_empty_one() {
    let alice = note(ALICE, 0x10, 1000);
    let store = store_with(&[alice]).await;
    let transaction = audited(vec![spend(&alice)], vec![ring_slot(0, ALICE, 1000)]);
    let (store, transaction) = (&store, &transaction);
    let read_public = |public_flows| async move {
        read(store, transaction, public_flows)
            .await
            .expect("rows")
            .flows
    };
    assert_eq!(
        outflows(&read_public(vec![public(999, 1000)]).await),
        [(owner_identity(&ALICE), 999)]
    );
    assert!(outflows(&read_public(Vec::new()).await)
        .iter()
        .all(|(_, outflow)| *outflow == 0));
}

/// Of the 100 spent 25 and 5 exit and 40 stay in the ring, a public leg withdraws or deposits 30.
#[tokio::test]
async fn what_leaves_is_each_exit_and_withdrawal_leg_and_never_a_deposit_or_a_ring_output() {
    let alice = note(ALICE, 0x10, 100);
    let store = store_with(&[alice]).await;
    let exits = audited(
        vec![spend(&alice)],
        vec![
            ring_slot(0, BOB, 40),
            slot(1, OutputOpening::Exit(utxo(BOB, 25))),
            slot(2, OutputOpening::Exit(utxo(ALICE, 5))),
        ],
    );
    let stays = audited(vec![spend(&alice)], vec![ring_slot(0, BOB, 100)]);
    let store = &store;
    let leaving = async |transaction: &AuditedTransaction, public_flows: Vec<PublicFlow>| {
        read(store, transaction, public_flows)
            .await
            .expect("rows")
            .leaving
    };
    let of_the_mint = |amount: u128| BTreeMap::from([(mint_of_asset(), amount)]);
    assert_eq!(leaving(&exits, vec![public(30, 0)]).await, of_the_mint(60));
    assert_eq!(leaving(&exits, vec![public(0, 30)]).await, of_the_mint(30));
    assert_eq!(leaving(&stays, vec![public(0, 30)]).await, BTreeMap::new());
}

/// A deposit funds the exit of 15 and no note is spent, the rows ask the registry for no mint.
#[tokio::test]
async fn a_deposit_funded_exit_and_an_exit_of_nothing_need_no_registry_mint() {
    let no_mints = RingAssets::new([]).expect("assets");
    let of_no_mint = AuditedUtxo {
        asset: [9; 32],
        ..utxo(BOB, 0)
    };
    let transaction = audited(
        Vec::new(),
        vec![
            slot(0, OutputOpening::Exit(utxo(BOB, 15))),
            slot(1, OutputOpening::Exit(of_no_mint)),
        ],
    );
    let notes = real_input_notes(&store_with(&[]).await, &transaction)
        .await
        .expect("notes");
    let read = FrameFlows {
        audited: &transaction,
        notes: &notes,
        owner_signers: Vec::new(),
        public: vec![public(0, 15)],
        assets: &no_mints,
        ring: RING,
    }
    .read(0)
    .expect("read");
    assert_eq!(read.leaving, BTreeMap::from([(mint_of_asset(), 15)]));
}

#[tokio::test]
async fn a_dummy_input_and_a_dummy_slot_answer_for_nothing() {
    let store = store_with(&[]).await;
    let read = read(
        &store,
        &audited(
            vec![Input::Padding([0x11; 32])],
            vec![slot(0, OutputOpening::Dummy)],
        ),
        Vec::new(),
    )
    .await
    .expect("rows");
    assert!(read.flows.is_empty());
    assert!(!read.spends_note);
}

/// A real input the index cannot name refuses until its note opens.
#[tokio::test]
async fn an_unresolved_nullifier_refuses_the_signature_until_its_note_opens() {
    let alice = note(ALICE, 0x12, 40);
    let transaction = audited(vec![spend(&alice)], vec![ring_slot(0, BOB, 40)]);
    assert!(refused(
        read(&store_with(&[]).await, &transaction, Vec::new()).await
    ));
    let store = store_with(&[alice]).await;
    let read = read(&store, &transaction, Vec::new())
        .await
        .expect("opened");
    assert_eq!(outflows(&read.flows), [(owner_identity(&ALICE), 40)]);
}

/// Default-ring evidence names its own owner, a co-signer signs an entry with no index row.
#[tokio::test]
async fn an_external_input_answers_under_its_own_evidence() {
    let transaction = audited(
        vec![entry(ALICE, 0x11, 30), entry(BOB, 0x12, 10)],
        vec![ring_slot(0, BOB, 40)],
    );
    let read = read(&store_with(&[]).await, &transaction, Vec::new())
        .await
        .expect("rows");
    assert_eq!(outflows(&read.flows), [(owner_identity(&ALICE), 30)]);
    assert!(read.spends_note);
}

/// One frame's `R` exceeds the notes the index and the entries name, the read refuses.
#[tokio::test]
async fn a_spend_neither_the_index_nor_an_entry_names_refuses_the_read() {
    let alice = note(ALICE, 0x12, 40);
    let mut transaction = audited(
        vec![spend(&alice), entry(BOB, 0x22, 10)],
        vec![ring_slot(0, BOB, 50)],
    );
    let store = store_with(&[alice]).await;
    assert!(read(&store, &transaction, Vec::new()).await.is_ok());
    transaction.real_inputs += 1;
    assert!(refused(read(&store, &transaction, Vec::new()).await));
}

/// A resolved spend names its owner, an unindexed spend stays another party's.
#[tokio::test]
async fn a_resolved_spend_names_its_party_and_an_unresolved_spend_stays_another_party() {
    let resolved = note(ALICE, 0x12, 40);
    let unindexed = note(ALICE, 0x13, 25);
    let store = store_with(&[resolved]).await;
    let nullifiers = [spend(&resolved).nullifier(), spend(&unindexed).nullifier()];
    store.spend(nullifiers).await;
    let published: Vec<Hash> = nullifiers.into_iter().map(Hash).collect();
    let wallet = Address::new_from_array(ALICE);
    let parties = store
        .input_parties(&wallet, &published)
        .await
        .expect("parties");
    let owned: Vec<bool> = published
        .iter()
        .map(|nullifier| parties.owns(nullifier))
        .collect();
    assert_eq!(owned, [true, false]);
}

#[tokio::test]
async fn an_unread_slot_or_an_unknown_mint_refuses_the_whole_read() {
    let alice = note(ALICE, 0x10, 30);
    let store = store_with(&[alice]).await;
    let mut transaction = audited(vec![spend(&alice)], vec![ring_slot(0, BOB, 30)]);
    transaction.payload_errors.push(OutputPayloadError {
        slot_index: 1,
        kind: OutputPayloadErrorKind::Commitment,
    });
    assert!(refused(read(&store, &transaction, Vec::new()).await));
    let no_mints = RingAssets::new([]).expect("assets");
    let transaction = audited(vec![spend(&alice)], vec![ring_slot(0, BOB, 30)]);
    let notes = real_input_notes(&store, &transaction).await.expect("notes");
    assert!(refused(
        FrameFlows {
            audited: &transaction,
            notes: &notes,
            owner_signers: Vec::new(),
            public: Vec::new(),
            assets: &no_mints,
            ring: RING,
        }
        .read(0)
    ));
}

/// A no-spend frame sits on the ring's own tag, only ring caps read its legs.
#[tokio::test]
async fn a_public_funded_transfer_with_no_sender_reads_as_no_spend() {
    let read = read(
        &store_with(&[]).await,
        &audited(Vec::new(), vec![ring_slot(0, BOB, 15)]),
        vec![public(0, 15)],
    )
    .await
    .expect("no spend");
    assert!(!read.spends_note);
    let [row] = read.flows.as_slice() else {
        panic!("one public row, {:?}", read.flows);
    };
    assert_eq!(row.flow.charged_outflow, 0);
    assert_eq!(row.flow.deposited, 15);
    assert_eq!(*row.owner.as_bytes(), owner_identity(RING.as_array()));
}

/// `test-vectors/policy/quantities.json`, the figure the policy circuit reaches.
#[test]
fn the_reserved_figure_matches_the_circuit_quantities() {
    fn identity(value: &serde_json::Value) -> [u8; 32] {
        let mut bytes = [0; 32];
        bytes[24..].copy_from_slice(&value.as_u64().expect("identity").to_be_bytes());
        bytes
    }
    let vector: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../test-vectors/policy/quantities.json"
    ))
    .expect("vector");
    for case in vector["cases"].as_array().expect("cases") {
        let notes = |key: &str| case[key].as_array().cloned().unwrap_or_default();
        let inputs: Vec<DebitInput> = notes("inputs")
            .iter()
            .map(|input| DebitInput {
                owner: identity(&input["owner"]),
                asset: identity(&input["asset"]),
                amount: input["amount"].as_u64().expect("amount"),
            })
            .collect();
        let outputs: Vec<DebitOutput> = notes("outputs")
            .iter()
            .map(|output| DebitOutput {
                ring: output["ring"].as_bool().expect("ring"),
                owner: identity(&output["owner"]),
                asset: identity(&output["asset"]),
                amount: output["amount"].as_u64().expect("amount"),
            })
            .collect();
        let public: Vec<PublicFlow> = notes("public")
            .iter()
            .map(|public| PublicFlow {
                mint: Address::new_from_array(identity(&public["asset"])),
                asset: identity(&public["asset"]),
                deposited: 0,
                withdrawn: public["gross"].as_u64().expect("gross").into(),
            })
            .collect();
        let rows = OwnerFlows {
            inputs: &inputs,
            outputs: &outputs,
            owner_signers: &[],
            public: &public,
            no_spend_owner: Member::from_bytes([0xee; 32]).expect("tag"),
        }
        .rows(|asset| Ok(Address::new_from_array(*asset)))
        .expect("rows");
        let reserved: Vec<([u8; 32], [u8; 32], u128)> = rows
            .iter()
            .filter(|row| row.flow.charged_outflow != 0)
            .map(|row| {
                (
                    *row.owner.as_bytes(),
                    row.flow.asset.to_bytes(),
                    row.flow.charged_outflow,
                )
            })
            .collect();
        let circuit: Vec<([u8; 32], [u8; 32], u128)> = notes("rows")
            .iter()
            .filter(|row| row["charged_outflow"].as_u64() != Some(0))
            .map(|row| {
                (
                    identity(&row["owner"]),
                    identity(&row["asset"]),
                    u128::from(row["charged_outflow"].as_u64().expect("charged outflow")),
                )
            })
            .collect();
        assert_eq!(reserved, circuit, "{}", case["name"]);
    }
}
