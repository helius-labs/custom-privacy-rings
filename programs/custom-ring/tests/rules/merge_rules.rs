//! Merge refusals ahead of the record proof and the forward.

use bytemuck::Zeroable;
use custom_ring_interface::{
    tag, CoSignScope, CustomRingError, RootRead, MERGE_ENVELOPE_LEN, MERGE_RECORD_LEN,
    RING_MERGE_PAYER_INDEX, RING_MERGE_RING_CONFIG_INDEX,
};
use solana_address::Address;
use solana_bn254::compression::prelude::alt_bn128_g2_compress_be;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::{Keypair, Signer};
use spr_interface::instruction::instruction_data::merge_transact::{
    MergeOutputEncryption, MergeProof, MergeTransactIxData,
};
use zolana_ring_client::CustomRingMergeIxData;
use zolana_ring_client::SealedMergeRecord;

use crate::{
    common::{
        auditor_pubkey, off_curve_g2, on_curve_g2, policy_pda, reaches_verification, refused,
        ring_auth, set_co_signer, Ring, Setup, PROGRAM,
    },
    transact_rules::SPP,
};

const RINGS: [Setup; 3] = [Setup::AuditOnly, Setup::Policy, Setup::Anonymous];
/// The ring account and the tracking registry lead the forwarded list.
const MERGE_SPP: usize = 2;

pub(crate) fn merge_data() -> CustomRingMergeIxData {
    CustomRingMergeIxData {
        tracking_revision: 0.into(),
        output_ring_data_hash: [8; 32],
        sealed: SealedMergeRecord {
            proof: Zeroable::zeroed(),
            eph_pk: auditor_pubkey(),
            envelope: [4; MERGE_ENVELOPE_LEN],
            record: [3; MERGE_RECORD_LEN],
        },
        merge: MergeTransactIxData {
            expiry_unix_ts: 1_800_000_000,
            proof: MergeProof {
                b: on_curve_g2(),
                ..MergeProof::zeroed()
            },
            output_utxo_hash: [1; 32],
            output_encryption: MergeOutputEncryption::PoseidonCtrV1 {
                salt: [2; 16],
                ciphertext: [[3; 32]; 3],
            },
            private_tx_hash: [4; 32],
            nullifiers: vec![[5; 32]; 8],
            utxo_tree_root_index: 0,
            nullifier_tree_root_index: 0,
            cache_slot: None,
        },
    }
}

pub(crate) fn merge_instruction(ring: &Ring, data: &CustomRingMergeIxData) -> Instruction {
    let mut spp: Vec<AccountMeta> = (0..RING_MERGE_PAYER_INDEX)
        .map(|_| AccountMeta::new(Address::new_unique(), false))
        .collect();
    spp[RING_MERGE_RING_CONFIG_INDEX] = AccountMeta::new(ring_auth(), false);
    spp.push(AccountMeta::new(ring.payer.pubkey(), true));
    spp.push(AccountMeta::new_readonly(SPP, false));
    let mut accounts = vec![AccountMeta::new_readonly(policy_pda(), false)];
    accounts.extend(ring.registries(RootRead::merge()));
    accounts.extend(spp);
    Instruction {
        program_id: PROGRAM,
        accounts,
        data: [&[tag::MERGE][..], &data.serialize().unwrap()].concat(),
    }
}

fn ring_merge(ring: &Ring) -> Instruction {
    merge_instruction(ring, &merge_data())
}

#[test]
fn a_nonzero_merge_binds_a_served_tracking_root_on_every_ring() {
    for setup in RINGS {
        let mut ring = Ring::new(setup);
        let mut data = merge_data();
        data.tracking_revision = 1.into();
        let stale = merge_instruction(&ring, &data);
        refused(ring.send(stale, &[]), CustomRingError::RootNotServed);
        let served = ring_merge(&ring);
        reaches_verification(ring.send(served, &[]));
    }
}

#[test]
fn a_ciphertext_element_past_the_field_is_refused() {
    let mut ring = Ring::new(Setup::AuditOnly);
    let mut data = merge_data();
    data.merge.output_encryption = MergeOutputEncryption::PoseidonCtrV1 {
        salt: [2; 16],
        ciphertext: [[3; 32], [0xff; 32], [3; 32]],
    };
    let instruction = merge_instruction(&ring, &data);
    refused(ring.send(instruction, &[]), CustomRingError::HashingFailed);
}

#[test]
fn a_merge_reads_only_the_pinned_tracking_registry() {
    let mut ring = Ring::new(Setup::AuditOnly);
    let mut instruction = ring_merge(&ring);
    instruction.accounts[1] = AccountMeta::new_readonly(Address::new_unique(), false);
    refused(
        ring.send(instruction, &[]),
        CustomRingError::UnboundRegistry,
    );
}

/// A frozen owner keeps its merge under any co-signer record.
#[test]
fn a_transfer_scoped_co_signer_demands_nothing_of_a_merge() {
    let mut ring = Ring::new(Setup::AuditOnly);
    set_co_signer(
        &mut ring,
        &Keypair::new().pubkey(),
        CoSignScope::TRANSFERS.bits(),
    );
    let instruction = ring_merge(&ring);
    reaches_verification(ring.send(instruction, &[]));
}

#[test]
fn an_anonymous_ring_merge_may_carry_ring_data() {
    let mut ring = Ring::new(Setup::Anonymous);
    let instruction = ring_merge(&ring);
    reaches_verification(ring.send(instruction, &[]));
}

#[test]
fn an_ephemeral_key_off_its_tag_is_refused() {
    let mut ring = Ring::new(Setup::AuditOnly);
    let mut data = merge_data();
    data.sealed.eph_pk[0] = 0;
    let instruction = merge_instruction(&ring, &data);
    refused(
        ring.send(instruction, &[]),
        CustomRingError::InvalidMergeDisclosure,
    );
}

#[test]
fn a_merge_proof_point_off_the_curve_is_refused_before_the_forward() {
    let mut ring = Ring::new(Setup::AuditOnly);
    let mut instruction = ring_merge(&ring);
    let compressed = alt_bn128_g2_compress_be(&on_curve_g2()).unwrap();
    let at = instruction
        .data
        .windows(compressed.len())
        .position(|window| window == compressed)
        .expect("the merge carries b compressed");
    instruction.data[at..at + compressed.len()].copy_from_slice(&off_curve_g2());
    refused(
        ring.send(instruction, &[]),
        CustomRingError::InvalidSppProofPoint,
    );
}

#[test]
fn a_merge_naming_the_ring_pda_past_its_slot_is_refused() {
    let mut ring = Ring::new(Setup::AuditOnly);
    let mut instruction = ring_merge(&ring);
    instruction
        .accounts
        .push(AccountMeta::new(ring_auth(), false));
    refused(
        ring.send(instruction, &[]),
        CustomRingError::RingAuthOutOfSlot,
    );
}

#[test]
fn a_merge_forward_holds_the_ring_pda_and_a_signing_payer() {
    let mut ring = Ring::new(Setup::AuditOnly);
    for (index, error) in [
        (
            RING_MERGE_RING_CONFIG_INDEX,
            CustomRingError::MissingRingAuth,
        ),
        (
            RING_MERGE_PAYER_INDEX,
            CustomRingError::MissingFundingSigner,
        ),
    ] {
        let mut instruction = ring_merge(&ring);
        instruction.accounts[MERGE_SPP + index] = AccountMeta::new(Address::new_unique(), false);
        refused(ring.send(instruction, &[]), error);
    }
}

#[test]
fn a_forward_past_the_cpi_account_cap_is_refused() {
    let mut ring = Ring::new(Setup::AuditOnly);
    ring.deploy_stand_in(SPP);
    let mut instruction = ring_merge(&ring);
    instruction.accounts.extend(std::iter::repeat_n(
        AccountMeta::new_readonly(SPP, false),
        128,
    ));
    // The record proof precedes the forward, the cap holds the forward alone.
    reaches_verification(ring.send(instruction, &[]));
}

#[test]
fn a_merge_without_the_shielded_pool_program_refuses_before_verification() {
    let mut ring = Ring::new(Setup::AuditOnly);
    let mut instruction = ring_merge(&ring);
    instruction.accounts.retain(|meta| meta.pubkey != SPP);
    refused(
        ring.send(instruction, &[]),
        CustomRingError::InvalidShieldedPoolProgram,
    );
}
