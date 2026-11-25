//! A program's call into the ring's `Transfer` under its own PDA.
//!
//! The ring takes an owner by its signature alone, a PDA signs only through its program's call.
//! `RingTransferCpi` raises that one PDA to a signer and forwards no tag but `TRANSFER`.
//!
//! Read after `program-libs/interface/src/instructions/transfer_approval.rs`. Read before
//! `examples/enrollment-vault/src/lib.rs`.

use spr_interface::instruction::InterfaceTransfer;

use crate::{tag, CustomRingError, ParsedTransact, RingMode, RingRail, RingTransactData};

#[cfg(any(feature = "cpi", test))]
use {
    crate::{
        cosign::{CoSignOperation, MAX_CO_SIGNERS},
        funder_account_position, CoSignScope, CoSigner, InputTrees, RingAccount, RingPolicy,
        RootRead, MAX_TRANSFER_ACCOUNTS, RING_TRANSACT_ACCOUNT_PREFIX_LEN, TRANSACT_PAYER_INDEX,
    },
    arrayvec::ArrayVec,
    solana_address::Address,
    solana_program_error::ProgramError,
    spr_interface::instruction::instruction_data::settlement_accounts,
};

#[cfg(any(feature = "cpi", test))]
const INSTRUCTIONS_SYSVAR_ID: Address =
    Address::from_str_const("Sysvar1nstructions1111111111111111111111111");

/// What a calling program binds its own proof to ahead of the call.
pub struct TransferFrame<'a>(ParsedTransact<'a>);

impl<'a> TransferFrame<'a> {
    /// `data` from the `TRANSFER` tag.
    pub fn parse(data: &'a [u8], mode: RingMode) -> Result<Self, CustomRingError> {
        let frame = data
            .strip_prefix(&[tag::TRANSFER])
            .ok_or(CustomRingError::InvalidInstructionData)?;
        RingTransactData::parse(frame, RingRail::transfer(mode)).map(Self)
    }

    pub fn private_tx_hash(&self) -> &'a [u8; 32] {
        self.0.data().private_tx_hash
    }

    pub fn interface_transfers(&self) -> &[InterfaceTransfer] {
        &self.0.data().interface_transfers
    }
}

#[cfg(any(feature = "cpi", test))]
trait TransferCpiAccount {
    fn address(&self) -> &Address;
    fn owner(&self) -> &Address;
    fn is_signer(&self) -> bool;
    fn with_data<R>(&self, f: impl FnOnce(&[u8]) -> Option<R>) -> Option<R>;
}

#[cfg(any(feature = "cpi", test))]
#[must_use]
struct TransferSignerCheck<'a> {
    ring_program: &'a Address,
    owner: &'a Address,
    data: &'a [u8],
}

#[cfg(any(feature = "cpi", test))]
impl TransferSignerCheck<'_> {
    #[inline(never)]
    fn signers<A: TransferCpiAccount>(
        self,
        accounts: &[A],
    ) -> Result<ArrayVec<bool, MAX_TRANSFER_ACCOUNTS>, ProgramError> {
        if self.data.first() != Some(&tag::TRANSFER) {
            return Err(CustomRingError::InvalidInstructionData.into());
        }
        if accounts.len() > MAX_TRANSFER_ACCOUNTS {
            return Err(CustomRingError::TooManyAccounts.into());
        }
        let ring_account = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
        let (frame, registries_len, mut requirement) =
            read_ring_account::<RingPolicy, _, _>(self.ring_program, ring_account, |ring| {
                let mode = ring.mode()?;
                let frame = TransferFrame::parse(self.data, mode)?;
                let tx = frame.0.data();
                let read = RootRead::transfer(ring, false);
                let registries_len = usize::from(ring.own_registry(read).is_some())
                    + usize::from(ring.curator_registry(read).is_some())
                    + 1;
                let public_legs =
                    tx.interface_transfers
                        .iter()
                        .fold(CoSignScope::NONE, |classes, transfer| {
                            classes
                                | match transfer.is_deposit() {
                                    true => CoSignScope::DEPOSITS,
                                    false => CoSignScope::WITHDRAWALS,
                                }
                        });
                let requirement = CoSignOperation {
                    spends_note: !tx.no_spend,
                    public_legs,
                    approval: tx.approval,
                }
                .demand()
                .requirement(ring);
                Ok((frame, registries_len, requirement))
            })?;

        let mut flags = ArrayVec::<bool, MAX_TRANSFER_ACCOUNTS>::new();
        for account in accounts {
            flags
                .try_push(account.is_signer())
                .map_err(|_| CustomRingError::TooManyAccounts)?;
        }

        let tx = frame.0.data();
        let co_signers_start = 1 + registries_len;
        let prefix = accounts
            .get(..co_signers_start)
            .ok_or(ProgramError::NotEnoughAccountKeys)?;
        // Duplicate CPI accounts share signer privilege across the inner instruction.
        if prefix
            .iter()
            .any(|account| account.address() == self.owner && !account.is_signer())
        {
            return Err(ProgramError::InvalidAccountData);
        }

        let mut cursor = co_signers_start;
        for _ in 0..MAX_CO_SIGNERS {
            let Some(current) = requirement else {
                break;
            };
            let [record, signer] = accounts
                .get(cursor..cursor + 2)
                .ok_or(ProgramError::NotEnoughAccountKeys)?
            else {
                return Err(ProgramError::NotEnoughAccountKeys);
            };
            if record.address() == self.owner && !record.is_signer() {
                return Err(CustomRingError::InvalidCoSignerRecord.into());
            }
            let cover = read_ring_account::<CoSigner, _, _>(self.ring_program, record, |stored| {
                current.cover(stored)
            })?;
            if signer.address() == self.owner {
                if cover.signer != *self.owner || signer.address() == &INSTRUCTIONS_SYSVAR_ID {
                    return Err(CustomRingError::UnauthorizedCoSigner.into());
                }
                flags[cursor + 1] = true;
            }
            requirement = cover.rest;
            cursor += 2;
        }

        let spp = accounts
            .get(cursor..)
            .ok_or(ProgramError::NotEnoughAccountKeys)?;
        let window = settlement_accounts(&tx.interface_transfers, tx.circuit(), spp)
            .ok_or(CustomRingError::InvalidSettlementAccounts)?;
        let owners = tx.owner_signers(spp, window.len())?;
        let trees = match tx.input_trees {
            InputTrees::One(_) => 1,
            InputTrees::Two { .. } => 2,
        };
        let owners_start = RING_TRANSACT_ACCOUNT_PREFIX_LEN + trees + tx.nullifiers.len();
        let owners_end = owners_start + owners.len();
        let before_owners = spp
            .get(..owners_start)
            .ok_or(CustomRingError::InvalidSettlementAccounts)?;

        for (index, account) in before_owners.iter().enumerate() {
            if account.address() == self.owner && !account.is_signer() {
                return Err(match index {
                    TRANSACT_PAYER_INDEX => CustomRingError::MissingFundingSigner.into(),
                    _ => ProgramError::InvalidAccountData,
                });
            }
        }

        for offset in owners_start..owners_end {
            if spp[offset].address() == self.owner {
                flags[cursor + offset] = true;
            }
        }

        let mut groups = window;
        for transfer in &tx.interface_transfers {
            let (group, rest) = groups
                .split_at_checked(transfer.settlement_account_count())
                .ok_or(CustomRingError::InvalidSettlementAccounts)?;
            groups = rest;
            let funder = funder_account_position(transfer);
            for (position, account) in group.iter().enumerate() {
                if account.address() == self.owner && !account.is_signer() {
                    if funder == Some(position) {
                        return Err(CustomRingError::MissingFundingSigner.into());
                    }
                    if !matches!(
                        (transfer, position),
                        (InterfaceTransfer::SolWithdrawal { .. }, 1)
                    ) {
                        return Err(ProgramError::InvalidAccountData);
                    }
                }
            }
        }

        let cache = spp
            .get(owners_end + window.len()..)
            .ok_or(CustomRingError::InvalidSettlementAccounts)?;
        for account in cache {
            if account.address() == self.owner && !account.is_signer() {
                return Err(CustomRingError::MissingFundingSigner.into());
            }
        }

        Ok(flags)
    }
}

#[cfg(any(feature = "cpi", test))]
fn read_ring_account<T: RingAccount, A: TransferCpiAccount, R>(
    ring_program: &Address,
    account: &A,
    f: impl FnOnce(&T) -> Result<R, CustomRingError>,
) -> Result<R, CustomRingError> {
    if account.owner() != ring_program {
        return Err(T::INVALID);
    }
    account
        .with_data(|bytes| T::from_data(bytes).map(f))
        .ok_or(T::INVALID)?
}

#[cfg(feature = "cpi")]
pub use cpi::RingTransferCpi;

#[cfg(feature = "cpi")]
mod cpi {
    use arrayvec::ArrayVec;
    use pinocchio::{
        cpi::{invoke_signed_with_bounds, Signer, MAX_STATIC_CPI_ACCOUNTS},
        instruction::{InstructionAccount, InstructionView},
        AccountView, Address, ProgramResult,
    };

    use super::{TransferCpiAccount, TransferSignerCheck};
    use crate::CustomRingError;

    impl TransferCpiAccount for AccountView {
        fn address(&self) -> &Address {
            AccountView::address(self)
        }

        fn owner(&self) -> &Address {
            AccountView::owner(self)
        }

        fn is_signer(&self) -> bool {
            AccountView::is_signer(self)
        }

        fn with_data<R>(&self, f: impl FnOnce(&[u8]) -> Option<R>) -> Option<R> {
            let bytes = self.try_borrow().ok()?;
            f(&bytes)
        }
    }

    #[must_use]
    pub struct RingTransferCpi<'a> {
        /// Fixed by the caller's signer seeds or its own state, never taken from the caller's accounts alone.
        pub ring_program: &'a Address,
        /// The PDA the signer seeds derive.
        pub owner: &'a Address,
        /// The transfer's list in call order.
        pub accounts: &'a [AccountView],
        /// From the `TRANSFER` tag.
        pub data: &'a [u8],
    }

    impl RingTransferCpi<'_> {
        /// Signs only in note owner or cosigner slots, never as payer or deposit funder.
        #[inline(never)]
        pub fn invoke_signed(self, signer: Signer<'_, '_>) -> ProgramResult {
            if self.accounts.len() > MAX_STATIC_CPI_ACCOUNTS {
                return Err(CustomRingError::TooManyAccounts.into());
            }
            let signers = TransferSignerCheck {
                ring_program: self.ring_program,
                owner: self.owner,
                data: self.data,
            }
            .signers(self.accounts)?;
            let mut metas = ArrayVec::<InstructionAccount, MAX_STATIC_CPI_ACCOUNTS>::new();
            for (account, signs) in self.accounts.iter().zip(signers) {
                metas
                    .try_push(InstructionAccount::new(
                        account.address(),
                        account.is_writable(),
                        signs,
                    ))
                    .map_err(|_| CustomRingError::TooManyAccounts)?;
            }
            let instruction = InstructionView {
                program_id: self.ring_program,
                accounts: &metas,
                data: self.data,
            };
            invoke(&instruction, self.accounts, signer)
        }
    }

    /// Its own frame, the metas and pinocchio's account array together pass one SBF frame.
    #[inline(never)]
    fn invoke(
        instruction: &InstructionView,
        accounts: &[AccountView],
        signer: Signer<'_, '_>,
    ) -> ProgramResult {
        invoke_signed_with_bounds::<MAX_STATIC_CPI_ACCOUNTS, _>(instruction, accounts, &[signer])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cosign::ApprovalMask, storage::Revision, CoSignerCounts, CustomRingProof, FrameExtensions,
        PlainGroth16Proof, RingOutput, SlotBody, CO_SIGNER, RING_POLICY, RING_RECORD_LEN,
    };
    use spr_interface::instruction::{
        instruction_data::transact::{CacheAccess, CacheWrite},
        TreeContext,
    };

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
                rail: RingRail::Confidential,
                n_in: 1,
                n_out: 2,
                proof: &proof,
                tracking_revision: Revision::from(9),
                own_revision: None,
                policy: None,
                spl_withdrawal_owners: &[],
                approval: ApprovalMask::default(),
                no_spend: false,
                public_owners: false,
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
                sender: None,
                envelope: &[],
                recipient_keys: &recipient_keys,
                vouched: false,
                private_tx_hash: &private_tx_hash,
                anonymous: None,
                in_proof_owner: None,
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
                let trees = match tx.input_trees {
                    InputTrees::One(_) => 1,
                    InputTrees::Two { .. } => 2,
                };
                for idx in 0..trees + tx.nullifiers.len() {
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
            TransferSignerCheck {
                ring_program: &ring_program,
                owner: &owner_pda,
                data,
            }
            .signers(accounts)
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
        sol_withdraw_tx.interface_transfers =
            vec![InterfaceTransfer::SolWithdrawal { amount: 500 }];
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
}
