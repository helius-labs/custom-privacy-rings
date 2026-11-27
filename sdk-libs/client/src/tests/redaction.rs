use std::{collections::BTreeSet, fmt::Debug};

use solana_address::Address;
use solana_signature::Signature;
use spr_indexer_api::Hash;
use spr_keypair::ViewingKey;
use spr_transaction::{DecryptionResult, Mint, OwnedUtxos, SpendableDecryptionResult};
use zeroize::Zeroizing;

use crate::{
    anonymous::RecoveryUpdate,
    rpc::{
        DecryptedKnownUtxo, DecryptedUtxo, ProofInputBytes, TxViewingSecret, WalletNote,
        WalletNoteStatus,
    },
    tests::support::{keypair, redacts, ring_utxo},
    AnonymousWallet, AuditedMergeOutput, AuditedUtxo, AuditorRecovery, KnownUtxo,
    MergeRecordOutput, PendingNote, RingConfidentialNotes, RingFullScan, RingOwnedUtxos,
    RingWalletNotes, RingWalletSynced, ScanCommit, SlotOpening, UtxoOwner, ViewingHistory,
};

const SECRET: [u8; 32] = [0xab; 32];

fn hides(printed: &str, field: &str, secret: &impl Debug) {
    assert!(redacts(printed, &[field]), "{printed}");
    assert!(!printed.contains(&format!("{secret:?}")), "{printed}");
}

fn address(byte: u8) -> Address {
    Address::new_from_array([byte; 32])
}

#[test]
fn audited_and_known_utxos_print_no_blinding() {
    let audited = AuditedUtxo {
        owner_key: [1; 32],
        owner: UtxoOwner::Keyed {
            nullifier_pk: [2; 32],
        },
        asset: [3; 32],
        mint: Some(address(4)),
        amount: 7,
        blinding: SECRET,
        data_hash: Some([5; 32]),
        ring_data_hash: Some([6; 32]),
    };
    hides(&format!("{audited:?}"), "blinding", &SECRET);
    let known = KnownUtxo {
        owner_key: audited.owner_key,
        owner: audited.owner,
        asset: audited.asset,
        mint: audited.mint,
        amount: audited.amount,
        blinding: SECRET,
        data_hash: audited.data_hash,
        ring_data_hash: audited.ring_data_hash,
        tree_id: 1,
    };
    hides(&format!("{known:?}"), "blinding", &SECRET);
}

#[test]
fn a_slot_opening_prints_no_blinding() {
    let opening = SlotOpening {
        slot: 1,
        owner: address(1).into(),
        nullifier_pk: Hash([2; 32]),
        mint: address(3).into(),
        amount: 7,
        blinding: Hash(SECRET),
        tree_id: 0,
        data_hash: Hash([5; 32]),
        ring_data_hash: Hash([6; 32]),
        tag: Some(Hash([8; 32])),
    };
    hides(&format!("{opening:?}"), "blinding", &Hash(SECRET));
}

#[test]
fn merge_outputs_print_no_blinding() {
    let record = MergeRecordOutput {
        owner_key: [1; 32],
        nullifier_pk: [2; 32],
        mint: address(3),
        amount: 7,
        blinding: SECRET,
        real_nullifiers: vec![[4; 32]],
    };
    hides(&format!("{record:?}"), "blinding", &SECRET);
    let audited = AuditedMergeOutput {
        commitment: [5; 32],
        slot: 1,
        tx_signature: Signature::default(),
        event_index: 0,
        tree_id: 0,
        leaf_index: 2,
        first_nullifier: [6; 32],
        ring_data_hash: [7; 32],
        owner_key: record.owner_key,
        owner: UtxoOwner::Unresolved,
        asset: [8; 32],
        mint: record.mint,
        amount: record.amount,
        real_nullifiers: record.real_nullifiers.clone(),
        blinding: SECRET,
    };
    hides(&format!("{audited:?}"), "blinding", &SECRET);
}

#[test]
fn decrypted_utxos_and_wallet_notes_print_no_blinding() {
    let decrypted = DecryptedUtxo {
        owner_key: address(1).into(),
        nullifier_pk: Some(Hash([2; 32])),
        asset: Hash([3; 32]),
        mint: Some(address(4).into()),
        amount: 7,
        blinding: Hash(SECRET),
        data_hash: Some(Hash([5; 32])),
        ring_data_hash: Some(Hash([6; 32])),
    };
    hides(&format!("{decrypted:?}"), "blinding", &Hash(SECRET));
    let known = DecryptedKnownUtxo {
        owner_key: decrypted.owner_key,
        nullifier_pk: decrypted.nullifier_pk.clone(),
        asset: decrypted.asset.clone(),
        mint: decrypted.mint,
        amount: decrypted.amount,
        blinding: Hash(SECRET),
        data_hash: decrypted.data_hash.clone(),
        ring_data_hash: decrypted.ring_data_hash.clone(),
        tree_id: 1,
    };
    hides(&format!("{known:?}"), "blinding", &Hash(SECRET));
    let note = WalletNote {
        utxo_hash: Hash([1; 32]),
        status: WalletNoteStatus::Unspent,
        nullifier_pk: decrypted.nullifier_pk.clone(),
        asset: decrypted.asset.clone(),
        mint: decrypted.mint,
        amount: decrypted.amount,
        blinding: Hash(SECRET),
        data_hash: decrypted.data_hash.clone(),
        ring_data_hash: decrypted.ring_data_hash.clone(),
        tree_id: 0,
        leaf_index: 2,
        slot: 1,
        tx_signature: Signature::default().into(),
        event_index: 0,
        slot_index: 0,
    };
    hides(&format!("{note:?}"), "blinding", &Hash(SECRET));
}

#[test]
fn a_viewing_secret_and_proof_input_bytes_print_no_byte() {
    let key = ViewingKey::from_bytes(&[9; 32]);
    assert_eq!(
        format!("{:?}", TxViewingSecret::from(&key)),
        "TxViewingSecret(..)"
    );
    assert_eq!(
        format!(
            "{:?}",
            ProofInputBytes::new(Zeroizing::new(SECRET.to_vec()))
        ),
        "ProofInputBytes(..)"
    );
}

#[test]
fn wallet_note_holders_print_no_blinding() {
    let owner = keypair(1);
    let note = ring_utxo(&owner, Mint::SOL, 7, 0);
    let blinding = format!("{:?}", note.utxo.blinding);
    let notes = vec![note.clone()];
    let owned = OwnedUtxos {
        utxos: notes.clone(),
        ..OwnedUtxos::default()
    };
    let ring_owned = RingOwnedUtxos {
        owned: owned.clone(),
        exposed_if_spent: notes.clone(),
        spent_exposed: [note.nullifier].into(),
    };
    let history = ViewingHistory::new(owner.viewing_pubkey(), 0);
    let mut wallet = AnonymousWallet::new(history.clone());
    wallet
        .absorb(ScanCommit {
            recovery: RecoveryUpdate::Keep,
            starting_history: history.clone(),
            history,
            decrypted: DecryptionResult {
                utxos: notes.clone(),
                ..DecryptionResult::default()
            },
            pending: Vec::new(),
            claims: Vec::new(),
            retried: BTreeSet::new(),
            unresolved: BTreeSet::new(),
        })
        .unwrap();
    assert_eq!(wallet.notes(), notes.as_slice());
    let printed = [
        format!("{:?}", PendingNote::new(note)),
        format!("{wallet:?}"),
        format!("{ring_owned:?}"),
        format!(
            "{:?}",
            RingFullScan {
                owned: ring_owned,
                pending: Vec::new(),
                auditor_recovery: AuditorRecovery::default(),
            }
        ),
        format!(
            "{:?}",
            RingConfidentialNotes {
                owned,
                exposed_if_spent: notes.clone(),
            }
        ),
        format!(
            "{:?}",
            RingWalletSynced::Notes(RingWalletNotes {
                spendable: SpendableDecryptionResult {
                    utxos_with_data: notes.clone(),
                    ..SpendableDecryptionResult::default()
                },
                exposed_if_spent: notes,
            })
        ),
    ];
    for printed in printed {
        assert!(!printed.contains(&blinding), "{printed}");
    }
}
