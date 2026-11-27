//! The in-proof owner rail, a relayer pays and the owner signs inside the proof.

use custom_ring_interface::{tag, CoSignScope, CustomRingError, TRANSFER_OUTPUT_CAPACITIES};
use solana_keypair::{Keypair, Signer};
use zolana_ring_client::FrameEncode;

use crate::{
    common::{reaches_verification, refused, set_co_signer, CoSignerSlot, Ring, Setup},
    transact_rules::{send, spp_list, Fixture},
};

const OWNER: [u8; 32] = [0x71; 32];

#[test]
fn a_relayer_pays_a_policy_transfer_its_owner_signs_in_the_proof() {
    let fixture = Fixture::new();
    let mut ring = Ring::new(Setup::Policy);
    let data = fixture.in_proof_owner(&OWNER, 2, 2, true);
    let spp = spp_list(&ring.payer.pubkey(), &data, &[]);
    reaches_verification(send(
        &mut ring,
        tag::TRANSFER,
        Vec::new(),
        data.encode().unwrap(),
        spp,
        &[],
    ));
}

#[test]
fn a_relayer_pays_an_audit_only_transfer_its_owner_signs_in_the_proof() {
    let fixture = Fixture::new();
    let mut ring = Ring::new(Setup::AuditOnly);
    let data = fixture.in_proof_owner(&OWNER, 2, 2, false);
    let spp = spp_list(&ring.payer.pubkey(), &data, &[]);
    reaches_verification(send(
        &mut ring,
        tag::TRANSFER,
        Vec::new(),
        data.encode().unwrap(),
        spp,
        &[],
    ));
}

#[test]
fn every_output_capacity_reaches_its_sender_key() {
    let fixture = Fixture::new();
    for outputs in TRANSFER_OUTPUT_CAPACITIES {
        let mut ring = Ring::new(Setup::AuditOnly);
        let data = fixture.in_proof_owner(&OWNER, 1, outputs as u8, false);
        let spp = spp_list(&ring.payer.pubkey(), &data, &[]);
        reaches_verification(send(
            &mut ring,
            tag::TRANSFER,
            Vec::new(),
            data.encode().unwrap(),
            spp,
            &[],
        ));
    }
}

/// The owner the proof names adds no second party as its own co-signer.
#[test]
fn an_in_proof_owner_co_signing_its_own_transfer_is_refused() {
    let fixture = Fixture::new();
    let mut ring = Ring::new(Setup::AuditOnly);
    let owner = Keypair::new();
    let other = Keypair::new();
    let owner_key = owner.pubkey().to_bytes();
    let data = fixture.in_proof_owner(&owner_key, 2, 2, false);
    let co_signed = |ring: &mut Ring, co_signer: &Keypair| {
        let spp = spp_list(&ring.payer.pubkey(), &data, &[]);
        send(
            ring,
            tag::TRANSFER,
            CoSignerSlot::own(co_signer.pubkey()).signed(),
            data.encode().unwrap(),
            spp,
            &[co_signer],
        )
    };
    for co_signer in [&owner, &other] {
        set_co_signer(
            &mut ring,
            &co_signer.pubkey(),
            CoSignScope::TRANSFERS.bits(),
        );
    }
    refused(
        co_signed(&mut ring, &owner),
        CustomRingError::CoSignerIsOwner,
    );
    reaches_verification(co_signed(&mut ring, &other));
}
