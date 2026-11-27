use custom_ring_interface::{
    cosign::ApprovalMask,
    storage::Revision,
    tag,
    transfer_cpi::{TransferCpiAccount, TransferFrame, TransferSignerCheck},
    CoSignScope, CoSigner, CoSignerCounts, CustomRingError, CustomRingProof, FrameExtensions,
    InputTrees, NamedRoots, PlainGroth16Proof, RailAuthorization, RingOutput, RingPolicy,
    RingTransactData, SlotBody, CO_SIGNER, RING_POLICY, RING_RECORD_LEN,
    RING_TRANSACT_ACCOUNT_PREFIX_LEN,
};
use solana_address::Address;
use solana_program_error::ProgramError;
use spr_interface::instruction::{
    instruction_data::transact::{CacheAccess, CacheWrite},
    InterfaceTransfer, TreeContext,
};
use zolana_ring_client::FrameEncode;

#[derive(Clone)]
struct MockAccount {
    address: Address,
    owner: Address,
    signer: bool,
    data: Vec<u8>,
}

impl MockAccount {
    fn plain(seed: u8, signer: bool) -> Self {
        Self {
            address: Address::new_from_array([seed; 32]),
            owner: Address::default(),
            signer,
            data: Vec::new(),
        }
    }

    fn owned(address: Address, owner: Address, data: &[u8]) -> Self {
        Self {
            address,
            owner,
            signer: false,
            data: data.to_vec(),
        }
    }
}

impl TransferCpiAccount for MockAccount {
    fn address(&self) -> &Address {
        &self.address
    }

    fn owner(&self) -> &Address {
        &self.owner
    }

    fn is_signer(&self) -> bool {
        self.signer
    }

    fn with_data<R>(&self, f: impl FnOnce(&[u8]) -> Option<R>) -> Option<R> {
        f(&self.data)
    }
}

fn policy() -> Box<RingPolicy> {
    let mut config: Box<RingPolicy> = bytemuck::zeroed_box();
    config.discriminator = RING_POLICY;
    config.auditor_pubkey[0] = 4;
    config.auditor_pubkey[1..].copy_from_slice(&[0xa1; 32]);
    config
}

#[test]
fn transfer_cpi_elevates_only_authorized_owner_and_co_signer_slots() {
    let proof = CustomRingProof {
        groth16: PlainGroth16Proof {
            proof_a: [0x10; 32],
            proof_b: [0x11; 64],
            proof_c: [0x12; 32],
        },
        commitment: [0x13; 32],
        commitment_pok: [0x14; 32],
    };
    let spp_proof = PlainGroth16Proof {
        proof_a: [0x21; 32],
        proof_b: [0x22; 64],
        proof_c: [0x23; 32],
    };
    let nullifiers = [[0x01; 32]];
    let private_tx_hash = [0x02; 32];
    let tx_viewing_u = [0x03; 32];
    let salt = [0x5c; 16];
    let output_hash = [0x04; 32];
    let record_bytes = [0x80; RING_RECORD_LEN];
    let envelope_buf = vec![0x93; 4096];
    let recipient_keys = [0x71; 64];
    let make_tx = || {
        let mut tx = RingTransactData {
            authorization: RailAuthorization::ConfidentialSigner,
            n_in: 1,
            n_out: 2,
            proof: &proof,
            tracking_revision: Revision::from(9),
            named_roots: NamedRoots::TrackingOnly,
            spl_withdrawal_owners: &[],
            approval: ApprovalMask::default(),
            no_spend: false,
            expiry_unix_ts: 1_800_000_000,
            tx_viewing_u: &tx_viewing_u,
            salt: &salt,
            extensions: FrameExtensions::default(),
            interface_transfers: Vec::new(),
            has_data: false,
            ring_data: false,
            entry: false,
            wide_owners: false,
            outputs: vec![
                RingOutput {
                    hash: &output_hash,
                    owner_tag: [0; 32],
                    body: SlotBody::Recipient(&record_bytes),
                    record: &record_bytes,
                };
                2
            ],
            replies: None,
            envelope: &[],
            recipient_keys: &recipient_keys,
            vouched: false,
            private_tx_hash: &private_tx_hash,
            spp_proof: &spp_proof,
            nullifiers: &nullifiers,
            input_trees: InputTrees::One(TreeContext {
                utxo_tree_root_index: 11,
                nullifier_tree_root_index: 12,
            }),
        };
        let len = tx.audit_stream().unwrap().envelope().len;
        tx.envelope = &envelope_buf[..len];
        tx
    };

    let ring_program = Address::new_from_array([0x11; 32]);
    let owner_pda = Address::new_from_array([0x22; 32]);
    let payer = Address::new_from_array([0x33; 32]);
    let ring = policy();

    let build_spp =
        |tx: &RingTransactData<'_>, owners: &[MockAccount], settlements: Vec<MockAccount>| {
            let mut spp = vec![MockAccount {
                address: payer,
                owner: Address::default(),
                signer: true,
                data: Vec::new(),
            }];
            for seed in 1..RING_TRANSACT_ACCOUNT_PREFIX_LEN as u8 {
                spp.push(MockAccount::plain(0x40 + seed, false));
            }
            for idx in 0..tx.input_trees.count() + tx.nullifiers.len() {
                spp.push(MockAccount::plain(0x50 + idx as u8, false));
            }
            spp.extend_from_slice(owners);
            spp.extend(settlements);
            if let Some(access) = tx.extensions.cache {
                if access.read_bitmap != 0 {
                    spp.push(MockAccount::plain(0x70, false));
                }
                if access.writes_cache() {
                    spp.push(MockAccount::plain(0x71, false));
                    spp.push(MockAccount::plain(0x72, true));
                }
            }
            spp
        };

    let framed = |tx: &RingTransactData<'_>| {
        let mut raw = vec![tag::TRANSFER];
        raw.extend(tx.encode().unwrap());
        raw
    };
    let check = |data: &[u8], accounts: &[MockAccount]| {
        TransferSignerCheck::signers(&ring_program, &owner_pda, accounts, |mode| {
            TransferFrame::parse(data, mode)
        })
    };

    let base_tx = make_tx();
    let base_data = framed(&base_tx);
    let ring_account = MockAccount::owned(
        Address::new_from_array([0x12; 32]),
        ring_program,
        bytemuck::bytes_of(&*ring),
    );
    let tracking_account = MockAccount::plain(0x13, false);
    let pda_slot = MockAccount {
        address: owner_pda,
        owner: Address::default(),
        signer: false,
        data: Vec::new(),
    };
    let mut accounts = vec![ring_account.clone(), tracking_account.clone()];
    let spp_offset = accounts.len();
    accounts.extend(build_spp(
        &base_tx,
        core::slice::from_ref(&pda_slot),
        Vec::new(),
    ));
    let owner_index = accounts.len() - 1;
    let flags = check(&base_data, &accounts).unwrap();
    for (idx, signs) in flags.iter().copied().enumerate() {
        assert_eq!(signs, idx == spp_offset || idx == owner_index, "slot {idx}");
    }

    let mut wrong_tag = base_data.clone();
    wrong_tag[0] = tag::AUTHORITY_TRANSFER;
    assert_eq!(
        check(&wrong_tag, &accounts),
        Err(CustomRingError::InvalidInstructionData.into())
    );

    let mut payer_forged = accounts.clone();
    payer_forged[spp_offset] = pda_slot.clone();
    assert_eq!(
        check(&base_data, &payer_forged),
        Err(CustomRingError::MissingFundingSigner.into())
    );
    let mut payer_signed = payer_forged;
    payer_signed[spp_offset].signer = true;
    assert!(check(&base_data, &payer_signed).is_ok());

    let mut prefix_forged = accounts.clone();
    prefix_forged[1] = pda_slot.clone();
    assert_eq!(
        check(&base_data, &prefix_forged),
        Err(ProgramError::InvalidAccountData)
    );

    let mut spl_deposit_tx = make_tx();
    spl_deposit_tx.interface_transfers = vec![InterfaceTransfer::SplDeposit {
        amount: 500,
        spl_interface_bump: 255,
    }];
    let spl_deposit_data = framed(&spl_deposit_tx);
    let mut spl_deposit_accounts = vec![ring_account.clone(), tracking_account.clone()];
    spl_deposit_accounts.extend(build_spp(
        &spl_deposit_tx,
        core::slice::from_ref(&pda_slot),
        vec![
            MockAccount::plain(0x81, false),
            MockAccount::plain(0x82, false),
            pda_slot.clone(),
            MockAccount::plain(0x83, false),
            MockAccount::plain(0x84, false),
        ],
    ));
    assert_eq!(
        check(&spl_deposit_data, &spl_deposit_accounts),
        Err(CustomRingError::MissingFundingSigner.into())
    );

    let mut sol_withdraw_tx = make_tx();
    sol_withdraw_tx.interface_transfers = vec![InterfaceTransfer::SolWithdrawal { amount: 500 }];
    let sol_withdraw_data = framed(&sol_withdraw_tx);
    let mut sol_withdraw_accounts = vec![ring_account.clone(), tracking_account.clone()];
    sol_withdraw_accounts.extend(build_spp(
        &sol_withdraw_tx,
        core::slice::from_ref(&pda_slot),
        vec![MockAccount::plain(0x85, false), pda_slot.clone()],
    ));
    let withdraw_flags = check(&sol_withdraw_data, &sol_withdraw_accounts).unwrap();
    assert!(!withdraw_flags[withdraw_flags.len() - 1]);

    let mut write_slots = CacheAccess::NO_WRITES;
    write_slots[0] = CacheWrite { output: 0, slot: 0 };
    let mut cached_tx = make_tx();
    cached_tx.extensions.cache = Some(CacheAccess {
        read_bitmap: 0,
        write_slots,
    });
    let cached_data = framed(&cached_tx);
    let mut cached_accounts = vec![ring_account, tracking_account.clone()];
    cached_accounts.extend(build_spp(
        &cached_tx,
        core::slice::from_ref(&pda_slot),
        Vec::new(),
    ));
    *cached_accounts.last_mut().unwrap() = pda_slot.clone();
    assert_eq!(
        check(&cached_data, &cached_accounts),
        Err(CustomRingError::MissingFundingSigner.into())
    );

    let mut cosigned_ring = policy();
    cosigned_ring.co_signers = CoSignerCounts::default()
        .added(CoSignScope::TRANSFERS)
        .unwrap();
    let cosigned_ring_account = MockAccount::owned(
        Address::new_from_array([0x12; 32]),
        ring_program,
        bytemuck::bytes_of(&*cosigned_ring),
    );
    let record_for = |signer: Address| {
        MockAccount::owned(
            Address::new_from_array([0x90; 32]),
            ring_program,
            bytemuck::bytes_of(&CoSigner {
                discriminator: CO_SIGNER,
                signer,
                scope: CoSignScope::TRANSFERS.bits(),
                bump: 254,
            }),
        )
    };
    let external_owner = MockAccount::plain(0x91, true);
    let mut cosign_ok = vec![
        cosigned_ring_account,
        tracking_account,
        record_for(owner_pda),
        pda_slot,
    ];
    cosign_ok.extend(build_spp(
        &base_tx,
        core::slice::from_ref(&external_owner),
        Vec::new(),
    ));
    let cosign_flags = check(&base_data, &cosign_ok).unwrap();
    assert!(cosign_flags[3]);
    assert!(!cosign_flags[2]);
}
