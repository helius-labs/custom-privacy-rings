use custom_ring_interface::{tag, TransferApproval};
use solana_address::Address;
use solana_message::{compiled_instruction::CompiledInstruction, VersionedMessage};
use solana_sdk_ids::system_program;
use zolana_ring_client::{rpc::ApprovedTransfer, CustomRing};

use crate::cosign::{CoSignError, CoSignFlows, CoSignRequest, Sender};

const ADVANCE_NONCE_ACCOUNT: [u8; 4] = 4u32.to_le_bytes();

#[must_use]
pub(crate) struct SignerScope {
    pub ring: Address,
    pub signer: Address,
    pub approved_transfer: Option<ApprovedTransfer>,
}

/// Only `SignerScope::admit` builds one.
#[must_use]
pub(crate) struct ScopedMessage {
    ring: Address,
    message: VersionedMessage,
    approved_transfer: Option<ApprovedCall>,
}

/// A ring `Transfer` call inside its approval's digest, only `ApprovedCall::committed` builds one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedCall {
    ring: Address,
    approval: TransferApproval,
    data: Vec<u8>,
    accounts: Vec<Address>,
}

enum CoSigned {
    Transfer,
    Approval(TransferApproval),
}

impl SignerScope {
    /// The signature covers every instruction, the key signs read-only in one co-signer slot or one approval alone.
    pub fn admit(self, message: VersionedMessage) -> Result<ScopedMessage, CoSignError> {
        let VersionedMessage::V1(_) = message else {
            return Err(CoSignError::MessageScope);
        };
        let keys = message.static_account_keys();
        if message.instructions().iter().any(|instruction| {
            keys.get(usize::from(instruction.program_id_index)) == Some(&system_program::ID)
                && instruction.data.starts_with(&ADVANCE_NONCE_ACCOUNT)
        }) {
            return Err(CoSignError::MessageScope);
        }
        let header = message.header();
        let signed = usize::from(header.num_required_signatures);
        let read_only = signed.saturating_sub(usize::from(header.num_readonly_signed_accounts));
        let signer = keys
            .iter()
            .position(|key| *key == self.signer)
            .filter(|index| (read_only..signed).contains(index))
            .ok_or(CoSignError::MessageScope)?;
        let ring = CustomRing::new(self.ring);
        let place = |address: Address| keys.iter().position(|key| *key == address);
        let slot = CoSignerSlot {
            policy: place(ring.policy_pda()),
            record: place(ring.co_signer_pda(&self.signer)),
            signer,
        };
        let names = |instruction: &&CompiledInstruction| {
            usize::from(instruction.program_id_index) == signer
                || instruction
                    .accounts
                    .iter()
                    .any(|account| usize::from(*account) == signer)
        };
        let mut naming = message.instructions().iter().filter(names);
        let co_signed = match (naming.next(), naming.next()) {
            (Some(instruction), None)
                if keys.get(usize::from(instruction.program_id_index)) == Some(&self.ring) =>
            {
                slot.holds(instruction)
            }
            _ => None,
        }
        .ok_or(CoSignError::MessageScope)?;
        let approved_transfer = match co_signed {
            CoSigned::Transfer if self.approved_transfer.is_none() => None,
            CoSigned::Transfer => return Err(CoSignError::MessageScope),
            CoSigned::Approval(approval) => {
                let transfer = self.approved_transfer.ok_or(CoSignError::MessageScope)?;
                Some(ApprovedCall::committed(self.ring, approval, transfer)?)
            }
        };
        Ok(ScopedMessage {
            ring: self.ring,
            message,
            approved_transfer,
        })
    }
}

/// The key positions of one `[record, signer]` pair and its ring.
struct CoSignerSlot {
    policy: Option<usize>,
    record: Option<usize>,
    signer: usize,
}

impl CoSignerSlot {
    fn holds(&self, instruction: &CompiledInstruction) -> Option<CoSigned> {
        match TransferApproval::parse(&instruction.data) {
            Some(approval) => self
                .approves(instruction)
                .then_some(CoSigned::Approval(approval)),
            None => self.holds_alone(instruction).then_some(CoSigned::Transfer),
        }
    }

    /// A program forwards the accounts of its call, an approval names the signer for no other.
    fn approves(&self, instruction: &CompiledInstruction) -> bool {
        self.policy
            .zip(self.record)
            .is_some_and(|(policy, record)| {
                instruction.accounts.iter().copied().map(usize::from).eq([
                    policy,
                    record,
                    self.signer,
                ])
            })
    }

    /// The delegate rail demands no co-signer, a signature there would be the delegate's.
    fn holds_alone(&self, instruction: &CompiledInstruction) -> bool {
        let position = |key: usize| {
            let mut places = instruction
                .accounts
                .iter()
                .enumerate()
                .filter(move |(_, account)| usize::from(**account) == key);
            match (places.next(), places.next()) {
                (Some((place, _)), None) => Some(place),
                _ => None,
            }
        };
        let after_record = self
            .record
            .and_then(position)
            .zip(position(self.signer))
            .is_some_and(|(record, signer)| record + 1 == signer);
        after_record && instruction.data.first() != Some(&tag::AUTHORITY_TRANSFER)
    }
}

impl ScopedMessage {
    pub const fn message(&self) -> &VersionedMessage {
        &self.message
    }

    pub const fn approved_transfer(&self) -> Option<&ApprovedCall> {
        self.approved_transfer.as_ref()
    }

    pub fn into_request(self, sender: Sender) -> CoSignRequest {
        CoSignRequest {
            ring: self.ring,
            message: self.message,
            flows: CoSignFlows::Frame,
            approved_transfer: self.approved_transfer,
            sender,
        }
    }
}

impl ApprovedCall {
    /// The signature covers the approval's digest, a call outside it would pass as approved.
    pub(crate) fn committed(
        ring: Address,
        approval: TransferApproval,
        transfer: ApprovedTransfer,
    ) -> Result<Self, CoSignError> {
        let data = transfer.data.0;
        let accounts: Vec<Address> = transfer
            .accounts
            .into_iter()
            .map(|account| account.0)
            .collect();
        let digest = TransferApproval::digest(&ring, &data, &accounts)
            .map_err(|_| CoSignError::MessageScope)?;
        if digest != approval.digest || data.first() != Some(&tag::TRANSFER) {
            return Err(CoSignError::MessageScope);
        }
        Ok(Self {
            ring,
            approval,
            data,
            accounts,
        })
    }

    pub const fn ring(&self) -> Address {
        self.ring
    }

    pub const fn approval(&self) -> TransferApproval {
        self.approval
    }

    /// From the `TRANSFER` tag.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// In call order.
    pub fn accounts(&self) -> &[Address] {
        &self.accounts
    }
}

#[cfg(test)]
mod tests {
    use solana_instruction::{AccountMeta, Instruction};
    use spr_client::{compile_message, ComputeBudgetConfig};
    use zolana_ring_client::ApprovalEncode;

    use super::*;

    const RING: Address = Address::new_from_array([0x11; 32]);
    const PAYER: Address = Address::new_from_array([0x22; 32]);
    const SIGNER: Address = Address::new_from_array([0x33; 32]);
    const OTHER_PROGRAM: Address = Address::new_from_array([0x44; 32]);

    fn record() -> AccountMeta {
        AccountMeta::new_readonly(CustomRing::new(RING).co_signer_pda(&SIGNER), false)
    }

    fn instruction(program_id: Address, accounts: Vec<AccountMeta>) -> Instruction {
        Instruction {
            program_id,
            accounts,
            data: vec![tag::TRANSFER],
        }
    }

    fn admitted(fee_payer: Address, instructions: &[Instruction]) -> bool {
        admitted_with(fee_payer, instructions, None)
    }

    fn admitted_with(
        fee_payer: Address,
        instructions: &[Instruction],
        approved_transfer: Option<ApprovedTransfer>,
    ) -> bool {
        let message = compile_message(
            &fee_payer,
            instructions,
            solana_hash::Hash::default(),
            ComputeBudgetConfig::new(1),
        )
        .expect("message");
        SignerScope {
            ring: RING,
            signer: SIGNER,
            approved_transfer,
        }
        .admit(message)
        .is_ok()
    }

    #[test]
    fn the_signer_signs_read_only_in_one_co_signer_slot() {
        let signer = || AccountMeta::new_readonly(SIGNER, true);
        let payer = || AccountMeta::new(PAYER, true);
        let co_signed = instruction(RING, vec![payer(), record(), signer()]);
        let unrelated = instruction(OTHER_PROGRAM, vec![payer()]);
        assert!(admitted(PAYER, &[unrelated, co_signed.clone()]));

        let delegate_frame = Instruction {
            data: vec![tag::AUTHORITY_TRANSFER],
            ..co_signed.clone()
        };
        for refused in [
            vec![instruction(RING, vec![payer()])],
            vec![instruction(
                RING,
                vec![payer(), record(), AccountMeta::new(SIGNER, true)],
            )],
            vec![instruction(RING, vec![payer(), signer()])],
            vec![instruction(RING, vec![record(), payer(), signer()])],
            vec![instruction(RING, vec![record(), signer(), signer()])],
            vec![instruction(OTHER_PROGRAM, vec![record(), signer()])],
            vec![
                co_signed.clone(),
                instruction(OTHER_PROGRAM, vec![signer()]),
            ],
            vec![
                co_signed.clone(),
                instruction(
                    OTHER_PROGRAM,
                    vec![AccountMeta::new_readonly(SIGNER, false)],
                ),
            ],
            vec![co_signed.clone(), co_signed.clone()],
            vec![delegate_frame],
        ] {
            assert!(!admitted(PAYER, &refused));
        }
        assert!(!admitted(SIGNER, &[co_signed]));
    }

    fn called(data: Vec<u8>) -> ApprovedTransfer {
        ApprovedTransfer {
            data: data.into(),
            accounts: vec![PAYER.into(), record().pubkey.into()],
        }
    }

    fn committing(call: &ApprovedTransfer) -> TransferApproval {
        let accounts: Vec<Address> = call.accounts.iter().map(|account| account.0).collect();
        TransferApproval {
            position: 0,
            digest: TransferApproval::digest(&RING, &call.data.0, &accounts).expect("digest"),
        }
    }

    #[test]
    fn the_signer_approves_a_call_whose_program_never_receives_it() {
        let signer = || AccountMeta::new_readonly(SIGNER, true);
        let policy =
            |ring: Address| AccountMeta::new_readonly(CustomRing::new(ring).policy_pda(), false);
        let call = called(vec![tag::TRANSFER, 1]);
        let approval = |program_id: Address, accounts: Vec<AccountMeta>| Instruction {
            program_id,
            accounts,
            data: committing(&call).encode().to_vec(),
        };
        let approved = approval(RING, vec![policy(RING), record(), signer()]);
        let caller = |forwarded: Vec<AccountMeta>| {
            let ring = AccountMeta::new_readonly(RING, false);
            instruction(OTHER_PROGRAM, [vec![ring, record()], forwarded].concat())
        };
        let names =
            |instructions: &[Instruction]| admitted_with(PAYER, instructions, Some(call.clone()));
        assert!(names(&[caller(Vec::new()), approved.clone()]));

        for refused in [
            vec![caller(vec![signer()]), approved.clone()],
            vec![caller(Vec::new()), approved.clone(), approved.clone()],
            vec![approval(RING, vec![record(), policy(RING), signer()])],
            vec![approval(
                RING,
                vec![policy(OTHER_PROGRAM), record(), signer()],
            )],
            vec![approval(
                RING,
                vec![policy(RING), record(), signer(), record()],
            )],
            vec![approval(
                OTHER_PROGRAM,
                vec![policy(RING), record(), signer()],
            )],
        ] {
            assert!(!names(&refused));
        }
    }

    #[test]
    fn an_approval_admits_only_the_call_its_digest_commits_to() {
        let signer = AccountMeta::new_readonly(SIGNER, true);
        let policy = AccountMeta::new_readonly(CustomRing::new(RING).policy_pda(), false);
        let call = called(vec![tag::TRANSFER, 1]);
        let approving = |approval: TransferApproval| Instruction {
            program_id: RING,
            accounts: vec![policy.clone(), record(), signer.clone()],
            data: approval.encode().to_vec(),
        };
        let approved = [approving(committing(&call))];
        assert!(admitted_with(PAYER, &approved, Some(call.clone())));
        assert!(!admitted_with(PAYER, &approved, None));

        let mut other_data = call.clone();
        other_data.data.0.push(0);
        let mut other_accounts = call.clone();
        other_accounts.accounts.swap(0, 1);
        for named in [other_data, other_accounts] {
            assert!(!admitted_with(PAYER, &approved, Some(named)));
        }

        let delegate = called(vec![tag::AUTHORITY_TRANSFER, 1]);
        let delegated = [approving(committing(&delegate))];
        assert!(!admitted_with(PAYER, &delegated, Some(delegate)));

        // A message holding its own transfer takes no approved call.
        let own = instruction(RING, vec![AccountMeta::new(PAYER, true), record(), signer]);
        assert!(admitted(PAYER, std::slice::from_ref(&own)));
        assert!(!admitted_with(PAYER, &[own], Some(call)));
    }

    #[test]
    fn non_v1_messages_are_refused() {
        let signer = AccountMeta::new_readonly(SIGNER, true);
        let payer = AccountMeta::new(PAYER, true);
        let co_signed = instruction(RING, vec![payer, record(), signer]);
        let legacy = solana_message::Message::new(std::slice::from_ref(&co_signed), Some(&PAYER));
        let v0 = solana_message::v0::Message::try_compile(
            &PAYER,
            &[co_signed],
            &[],
            solana_hash::Hash::default(),
        )
        .expect("v0 message");
        for message in [VersionedMessage::Legacy(legacy), VersionedMessage::V0(v0)] {
            assert_eq!(
                SignerScope {
                    ring: RING,
                    signer: SIGNER,
                    approved_transfer: None,
                }
                .admit(message)
                .err(),
                Some(CoSignError::MessageScope)
            );
        }
    }

    #[test]
    fn advance_nonce_instructions_are_refused() {
        let signer = AccountMeta::new_readonly(SIGNER, true);
        let payer = AccountMeta::new(PAYER, true);
        let nonce_account = Address::new_from_array([0x55; 32]);
        let co_signed = instruction(RING, vec![payer.clone(), record(), signer]);
        let system_transfer = Instruction {
            program_id: system_program::ID,
            accounts: vec![payer, AccountMeta::new(nonce_account, false)],
            data: [2u32.to_le_bytes().as_slice(), &100u64.to_le_bytes()].concat(),
        };
        assert!(admitted(PAYER, &[system_transfer, co_signed.clone()]));

        for data in [vec![4, 0, 0, 0], vec![4, 0, 0, 0, 0xff]] {
            let advance = Instruction {
                program_id: system_program::ID,
                accounts: vec![
                    AccountMeta::new(nonce_account, false),
                    AccountMeta::new_readonly(
                        solana_sdk_ids::sysvar::recent_blockhashes::ID,
                        false,
                    ),
                    AccountMeta::new_readonly(PAYER, true),
                ],
                data,
            };
            assert!(!admitted(PAYER, &[advance.clone(), co_signed.clone()]));
            assert!(!admitted(PAYER, &[co_signed.clone(), advance.clone()]));

            let mut v1 = match compile_message(
                &PAYER,
                &[advance, co_signed.clone()],
                solana_hash::Hash::default(),
                ComputeBudgetConfig::new(1),
            )
            .expect("message")
            {
                VersionedMessage::V1(v1) => v1,
                other => panic!("expected v1, got {other:?}"),
            };
            v1.instructions.rotate_right(1);
            assert_eq!(
                SignerScope {
                    ring: RING,
                    signer: SIGNER,
                    approved_transfer: None,
                }
                .admit(VersionedMessage::V1(v1))
                .err(),
                Some(CoSignError::MessageScope)
            );
        }
    }
}
