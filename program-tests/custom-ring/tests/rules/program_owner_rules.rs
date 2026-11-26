//! A program spends its PDA's ring notes through CPI, a co-signer approves the call from the top level.

use custom_ring_interface::{
    tag, CoSignScope, CustomRingError, InstructionTag, RingPolicy, RingRail, RingTransactData,
    RootRead, TransferApproval,
};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::{Keypair, Signer};
use zolana_ring_client::ApprovalEncode;
use zolana_ring_client::FrameEncode;

use crate::{
    common::{
        co_signer_record, policy_pda, reaches_verification, refused, set_co_signer,
        tracking_registry, transact, within_budget, Ring, Sent, Setup, PROGRAM,
    },
    transact_rules::{spp_list, Fixture},
};

/// Forwards any instruction under its `owner` PDA.
const FORWARDER: Address = Address::new_from_array([0x63; 32]);
/// `examples/enrollment-vault`, its manager moves the vault PDA's notes.
const VAULT_PROGRAM: Address = Address::new_from_array([0x64; 32]);
const INSTRUCTIONS_SYSVAR: Address =
    Address::from_str_const("Sysvar1nstructions1111111111111111111111111");

fn owner_pda(program: &Address) -> Address {
    Address::find_program_address(&[b"owner"], program).0
}

/// The ring transfer of `data` whose one owner slot names `owner`, unsigned at the top level.
fn pda_transfer(
    ring: &Ring,
    data: &RingTransactData<'_>,
    owner: Address,
    co_signers: Vec<AccountMeta>,
) -> Instruction {
    let policy: RingPolicy = ring.read(&policy_pda());
    let accounts = [
        vec![AccountMeta::new_readonly(policy_pda(), false)],
        ring.registries(RootRead::transfer(&policy, false)),
        co_signers,
        spp_list(
            &ring.payer.pubkey(),
            data,
            &[AccountMeta::new_readonly(owner, false)],
        ),
    ]
    .concat();
    transact(tag::TRANSFER, accounts, data.encode().unwrap())
}

/// `instruction` forwarded to the ring, the forwarder's PDA signing the inner call.
fn through_forwarder(mut instruction: Instruction) -> Instruction {
    instruction
        .accounts
        .push(AccountMeta::new_readonly(PROGRAM, false));
    instruction.program_id = FORWARDER;
    instruction
}

#[test]
fn a_pda_owner_reaches_the_ring_proof_only_through_its_program() {
    let fixture = Fixture::new();
    for (setup, rail, policy) in [
        (Setup::AuditOnly, RingRail::Confidential, false),
        (Setup::Policy, RingRail::Confidential, true),
        (Setup::Anonymous, RingRail::Anonymous, false),
    ] {
        let mut ring = Ring::new(setup);
        ring.install_owner_program(FORWARDER, "storage_cpi_owner.so");
        let data = match rail {
            RingRail::Anonymous => fixture.public_owners(2, 2, policy),
            _ => fixture.transfer(rail, 2, 2, policy),
        };
        let direct = pda_transfer(&ring, &data, owner_pda(&FORWARDER), Vec::new());
        // 1. No key signs for a PDA at the top level.
        refused(
            ring.send(direct.clone(), &[]),
            CustomRingError::MissingOwnerSigner,
        );
        // 2. The program's CPI signature fills the owner slot on either public rail.
        reaches_verification(ring.send(through_forwarder(direct), &[]));
    }
}

/// A hidden owner signs with an Ed25519 key inside the SPP proof, a PDA has none.
#[test]
fn a_pda_in_the_public_run_of_a_hidden_transfer_still_signs_through_cpi() {
    let fixture = Fixture::new();
    let mut ring = Ring::new(Setup::Anonymous);
    ring.install_owner_program(FORWARDER, "storage_cpi_owner.so");
    let data = fixture.transfer(RingRail::Anonymous, 2, 2, false);
    let transfer = pda_transfer(&ring, &data, owner_pda(&FORWARDER), Vec::new());
    reaches_verification(ring.send(through_forwarder(transfer), &[]));
}

/// A vault transfer on a ring whose transfers demand `co_signer`.
struct VaultTransfer {
    ring: Ring,
    manager: Keypair,
    co_signer: Keypair,
    /// The ring call the vault makes, its co-signer pair names the instructions sysvar.
    transfer: Instruction,
}

impl VaultTransfer {
    fn new() -> Self {
        let mut ring = Ring::new(Setup::AuditOnly);
        ring.install_owner_program(VAULT_PROGRAM, "enrollment_vault.so");
        let (manager, co_signer) = (Keypair::new(), Keypair::new());
        let vault = owner_pda(&VAULT_PROGRAM);
        ring.store(
            vault,
            VAULT_PROGRAM,
            [
                manager.pubkey().as_ref(),
                PROGRAM.as_ref(),
                tracking_registry().as_ref(),
            ]
            .concat(),
        );
        set_co_signer(
            &mut ring,
            &co_signer.pubkey(),
            CoSignScope::TRANSFERS.bits(),
        );
        let fixture = Fixture::new();
        let data = fixture.transfer(RingRail::Confidential, 2, 2, false);
        let pair = vec![
            AccountMeta::new_readonly(co_signer_record(&co_signer.pubkey()), false),
            AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR, false),
        ];
        let transfer = pda_transfer(&ring, &data, vault, pair);
        Self {
            ring,
            manager,
            co_signer,
            transfer,
        }
    }

    fn digest(&self) -> [u8; 32] {
        digest_of(&self.transfer)
    }

    fn approval(&self, position: u8, digest: [u8; 32]) -> Instruction {
        approval(
            &self.co_signer.pubkey(),
            TransferApproval { position, digest },
        )
    }

    /// The vault's instruction first, then `approvals` under the co-signer's signature.
    fn send(&mut self, approvals: &[Instruction]) -> Sent {
        let co_signer = self.co_signer.insecure_clone();
        self.send_signed(approvals, &[&co_signer])
    }

    /// The manager signs beside `signers`.
    fn send_signed(&mut self, approvals: &[Instruction], signers: &[&Keypair]) -> Sent {
        let mut accounts = vec![
            AccountMeta::new_readonly(self.manager.pubkey(), true),
            AccountMeta::new_readonly(owner_pda(&VAULT_PROGRAM), false),
            AccountMeta::new_readonly(PROGRAM, false),
        ];
        accounts.extend_from_slice(&self.transfer.accounts);
        let forwarded = Instruction {
            program_id: VAULT_PROGRAM,
            accounts,
            data: self.transfer.data.clone(),
        };
        let instructions = [&[forwarded][..], approvals].concat();
        let signers = [&[&self.manager][..], signers].concat();
        self.ring.send_all(&instructions, &signers)
    }
}

fn digest_of(transfer: &Instruction) -> [u8; 32] {
    TransferApproval::digest(
        &transfer.program_id,
        &transfer.data,
        transfer.accounts.iter().map(|account| &account.pubkey),
    )
    .unwrap()
}

/// `ApproveTransfer` under the record of `co_signer`, signed by it.
fn approval(co_signer: &Address, approval: TransferApproval) -> Instruction {
    Instruction {
        program_id: PROGRAM,
        accounts: vec![
            AccountMeta::new_readonly(policy_pda(), false),
            AccountMeta::new_readonly(co_signer_record(co_signer), false),
            AccountMeta::new_readonly(*co_signer, true),
        ],
        data: approval.encode().to_vec(),
    }
}

#[test]
fn a_top_level_approval_co_signs_the_transfer_a_program_forwards() {
    let mut vault = VaultTransfer::new();
    let approval = vault.approval(0, vault.digest());
    reaches_verification(vault.send(&[approval]));
}

#[test]
fn a_forwarded_transfer_without_an_approval_of_its_call_is_refused() {
    let mut vault = VaultTransfer::new();
    refused(
        vault.send_signed(&[], &[]),
        CustomRingError::MissingTransferApproval,
    );

    let mut other_frame = vault.transfer.clone();
    *other_frame.data.last_mut().unwrap() ^= 1;
    let mut reordered = vault.transfer.clone();
    let last = reordered.accounts.len() - 1;
    reordered.accounts.swap(last - 1, last);
    let mut other_ring = vault.transfer.clone();
    other_ring.program_id = Address::new_from_array([0x53; 32]);
    for other_call in [other_frame, reordered, other_ring] {
        let approval = vault.approval(0, digest_of(&other_call));
        refused(
            vault.send(&[approval]),
            CustomRingError::TransferApprovalOtherCall,
        );
    }

    let other_position = vault.approval(1, vault.digest());
    refused(
        vault.send(&[other_position]),
        CustomRingError::TransferApprovalOtherPosition,
    );
}

#[test]
fn an_approval_counts_only_under_the_signature_of_the_records_signer() {
    let mut vault = VaultTransfer::new();
    let approved = TransferApproval {
        position: 0,
        digest: vault.digest(),
    };
    let other = Keypair::new();
    let mut other_signer = approval(&vault.co_signer.pubkey(), approved);
    other_signer.accounts[TransferApproval::CO_SIGNER_ACCOUNT].pubkey = other.pubkey();
    refused(
        vault.send_signed(&[other_signer], &[&other]),
        CustomRingError::UnauthorizedCoSigner,
    );
    let mut unsigned = approval(&vault.co_signer.pubkey(), approved);
    unsigned.accounts[TransferApproval::CO_SIGNER_ACCOUNT].is_signer = false;
    refused(
        vault.send_signed(&[unsigned], &[]),
        CustomRingError::MissingCoSigner,
    );
}

#[test]
fn an_account_other_than_the_instructions_sysvar_must_sign() {
    let mut vault = VaultTransfer::new();
    let pair_signer = vault
        .transfer
        .accounts
        .iter()
        .position(|account| account.pubkey == INSTRUCTIONS_SYSVAR)
        .unwrap();
    vault.transfer.accounts[pair_signer].pubkey = Address::new_unique();
    let approval = vault.approval(0, vault.digest());
    refused(vault.send(&[approval]), CustomRingError::MissingCoSigner);
}

#[test]
fn an_approval_runs_only_at_the_top_level_under_its_co_signer() {
    let mut vault = VaultTransfer::new();
    vault
        .ring
        .install_owner_program(FORWARDER, "storage_cpi_owner.so");
    let co_signer = vault.co_signer.insecure_clone();
    let approved = TransferApproval {
        position: 0,
        digest: [7; 32],
    };
    let approve = approval(&co_signer.pubkey(), approved);
    within_budget(
        InstructionTag::ApproveTransfer,
        vault.ring.send(approve.clone(), &[&co_signer]),
    );
    refused(
        vault
            .ring
            .send(through_forwarder(approve.clone()), &[&co_signer]),
        CustomRingError::ApprovalNotTopLevel,
    );
    let mut unsigned = approve.clone();
    unsigned.accounts[TransferApproval::CO_SIGNER_ACCOUNT].is_signer = false;
    refused(
        vault.ring.send(unsigned, &[]),
        CustomRingError::MissingCoSigner,
    );
    let other = Keypair::new();
    let mut other_signer = approve;
    other_signer.accounts[TransferApproval::CO_SIGNER_ACCOUNT].pubkey = other.pubkey();
    refused(
        vault.ring.send(other_signer, &[&other]),
        CustomRingError::UnauthorizedCoSigner,
    );
}
