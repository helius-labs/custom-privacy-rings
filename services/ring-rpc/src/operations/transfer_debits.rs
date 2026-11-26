//! Each owner's outflow of the transfer a co-signer signs, read from the frame and the note index.

use std::collections::HashMap;

use custom_ring_interface::{
    funder_account_position, settlement_mints, tag, InstructionTag, ParsedTransact, PublicLeg,
    PublicSlotError, PublicSlots, RingRail, RingTransactData, TransferApproval,
    RING_TRANSACT_RING_CONFIG_INDEX,
};
use custom_ring_sdk::CustomRing;
use solana_address::Address;
use solana_message::VersionedMessage;
use solana_sdk_ids::sysvar;
use spr_hasher::primitives::hash_bytes;
use spr_interface::SOL_ASSET_FIELD;
use spr_interface::{
    instruction::{instruction_data::settlement_accounts, InterfaceTransfer},
    pda, PROGRAM_ID_PUBKEY,
};
use spr_transaction::{instructions::transact::SettlementTransfer, SOL_MINT};
use zolana_ring_client::{AuditedTransaction, FramedRow, OutputOpening, TransactionAudit};
use zolana_ring_policy::{
    owner_debits, DebitInput, DebitOutput, DebitTransfer, Member, PublicGross,
};
use zolana_ring_transaction::RingAssets;

use crate::{
    cosign::{AssetFlow, CoSignError, FrameSource, OwnerFlow},
    cosign_limits::{FrameRead, TransferRead},
    cosigner::{FrameParties, Parties},
    error::RingRpcError,
    hub::Shared,
    upstream::TransactionSource,
    utxos::{AuditStore, StoredUtxo},
};

/// The output tree's position in SPP's transact account list.
const TRANSACT_OUTPUT_TREE_INDEX: usize = 1;

/// A ring whose auditor key the service does not hold refuses the limited co-signer.
#[must_use]
pub(crate) struct TransferDebits<'a, S> {
    pub shared: &'a std::sync::Arc<Shared<S>>,
    pub ring: Address,
    pub source: FrameSource<'a>,
}

/// The one ring transfer the co-signer's signature admits.
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
struct RingFrame<'m> {
    tag: u8,
    data: &'m [u8],
    accounts: Vec<Address>,
    /// Empty where the message's own instruction is the transfer.
    approvers: Vec<Approver>,
}

#[derive(Debug, PartialEq, Eq)]
struct Approver {
    record: Address,
    signer: Address,
}

/// An instruction able to reach the ring or SPP, a CPI target sits in the invoking accounts.
struct Reaching<'m> {
    position: usize,
    program: Address,
    accounts: Vec<Address>,
    data: &'m [u8],
}

enum Carried<'m> {
    Instruction(Reaching<'m>),
    /// The program at the approved position calls it.
    Approved {
        approval: TransferApproval,
        approvers: Vec<Approver>,
    },
}

impl<'m> Carried<'m> {
    /// Refuses a second reaching instruction beside the transfer or the approved caller.
    fn decode(message: &'m VersionedMessage, ring: Address) -> Result<Self, RingRpcError> {
        // Version 1 carries no lookup table, one could hide the ring from the check.
        let VersionedMessage::V1(_) = message else {
            return Err(scope());
        };
        let keys = message.static_account_keys();
        let key = |index: u8| keys.get(usize::from(index)).copied().ok_or_else(scope);
        let policy = CustomRing::new(ring).policy_pda();
        let mut approvals: Vec<(TransferApproval, Approver)> = Vec::new();
        let mut holding = None;
        for (position, instruction) in message.instructions().iter().enumerate() {
            let reaching = Reaching {
                position,
                program: key(instruction.program_id_index)?,
                accounts: instruction
                    .accounts
                    .iter()
                    .map(|index| key(*index))
                    .collect::<Result<_, _>>()?,
                data: &instruction.data,
            };
            if !reaching.names(ring) && !reaching.names(PROGRAM_ID_PUBKEY) {
                continue;
            }
            let approval = (reaching.program == ring)
                .then(|| TransferApproval::parse(reaching.data))
                .flatten();
            match approval {
                Some(approval) => {
                    let [named, record, signer] = reaching.accounts[..] else {
                        return Err(scope());
                    };
                    if named != policy {
                        return Err(scope());
                    }
                    approvals.push((approval, Approver { record, signer }));
                }
                None if holding.replace(reaching).is_some() => return Err(scope()),
                None => {}
            }
        }
        let held = holding.ok_or_else(scope)?;
        let Some((approval, _)) = approvals.first() else {
            return Ok(Self::Instruction(held));
        };
        let approval = *approval;
        let calls_ring = held.program != ring
            && held.program != PROGRAM_ID_PUBKEY
            && held.accounts.contains(&ring);
        if !calls_ring
            || usize::from(approval.position) != held.position
            || approvals.iter().any(|(other, _)| *other != approval)
        {
            return Err(scope());
        }
        Ok(Self::Approved {
            approval,
            approvers: approvals
                .into_iter()
                .map(|(_, approver)| approver)
                .collect(),
        })
    }
}

impl Reaching<'_> {
    fn names(&self, address: Address) -> bool {
        self.program == address || self.accounts.contains(&address)
    }
}

/// One asset's public legs, a deposit joins the volume and a withdrawal stays gross.
pub(super) struct PublicFlow {
    pub(super) mint: Address,
    /// The field the statement's gross and every note of the mint hash under.
    pub(super) asset: [u8; 32],
    pub(super) deposited: u128,
    pub(super) withdrawn: u128,
}

/// Unknown input refuses, the wallet retries once the auditor ingested the note.
const fn unavailable() -> RingRpcError {
    RingRpcError::CoSign(CoSignError::LimitsUnavailable)
}

const fn scope() -> RingRpcError {
    RingRpcError::CoSign(CoSignError::MessageScope)
}

impl<S: TransactionSource + 'static> TransferDebits<'_, S> {
    pub async fn flows(self) -> Result<FrameRead, RingRpcError> {
        let frame = RingFrame::of(self.source, self.ring)?;
        let co_signers = frame.co_signers(self.ring);
        let Some(opened) = self.open(&frame).await? else {
            return Ok(FrameRead::Delegate);
        };
        let notes = real_input_notes(&self.shared.store(self.ring), &opened.audited).await?;
        let read = FrameFlows {
            audited: &opened.audited,
            notes: &notes,
            owner_signers: opened.owner_signers,
            public: opened.public,
            assets: &self.shared.cached_assets().await,
            ring: self.ring,
        }
        .read(opened.approval_bits)?;
        Ok(FrameRead::Transfer(TransferRead { co_signers, ..read }))
    }

    /// Waits for no input note the signer run already answers for.
    pub async fn parties(self) -> Result<FrameParties, RingRpcError> {
        let frame = RingFrame::of(self.source, self.ring)?;
        let Some(opened) = self.open(&frame).await? else {
            return Ok(FrameParties::Delegate);
        };
        let notes = real_input_notes(&self.shared.store(self.ring), &opened.audited).await?;
        let flows = FrameFlows {
            audited: &opened.audited,
            notes: &notes,
            owner_signers: opened.owner_signers,
            public: opened.public,
            assets: &self.shared.cached_assets().await,
            ring: self.ring,
        };
        let spent = match flows.inputs() {
            Ok(spent) => spent,
            // The proof binds every owner of such a frame into the signer run.
            Err(_) if !opened.hidden_sender => flows.entries()?,
            Err(unknown) => return Err(unknown),
        };
        let outputs = flows.outputs()?;
        let senders = flows
            .owner_signers
            .iter()
            .copied()
            .chain(spent.iter().map(|input| input.owner))
            .map(member)
            .collect::<Result<_, _>>()?;
        let moved = outputs
            .iter()
            .map(|output| output.asset)
            .chain(spent.iter().map(|input| input.asset))
            .map(|asset| flows.mint_of(&asset))
            .chain(flows.public.iter().map(|public| Ok(public.mint)))
            .collect::<Result<_, _>>()?;
        Ok(FrameParties::Transfer(Parties {
            senders,
            recipients: outputs
                .iter()
                .map(|output| member(output.owner))
                .collect::<Result<_, _>>()?,
            destinations: opened.withdrawal_accounts,
            assets: moved,
        }))
    }

    /// `None` on the delegate rail.
    async fn open(&self, frame: &RingFrame<'_>) -> Result<Option<OpenedFrame>, RingRpcError> {
        let service = self.shared.service(Some(self.ring))?;
        let config = service.served_config().await?.ok_or_else(unavailable)?;
        if frame.tag == tag::AUTHORITY_TRANSFER {
            return Ok(None);
        }
        let rail = InstructionTag::try_from(frame.tag)
            .and_then(|tag| RingRail::of(tag, config.mode))
            .map_err(|_| unavailable())?;
        let parsed = RingTransactData::parse(frame.data, rail).map_err(|_| unavailable())?;
        let spp = frame.spp_accounts(self.ring)?;
        let tree = *spp
            .get(TRANSACT_OUTPUT_TREE_INDEX)
            .ok_or_else(unavailable)?;
        let output_tree_id = self
            .shared
            .source
            .tree_id(tree)
            .await
            .map_err(|_| unavailable())?;
        let row = FramedRow {
            parsed: &parsed,
            ring: self.ring,
            auditor: &service.auditor_keypair().pubkey(),
            output_tree_id,
        }
        .row()
        .map_err(|_| unavailable())?;
        let audited = TransactionAudit::new(&row, parsed.audit_stream())
            .run(service.auditor_keypair())
            .map_err(|_| unavailable())?;
        let public = public_flows(&parsed, spp)?;
        let window = settlement_accounts(
            &parsed.data().interface_transfers,
            parsed.data().circuit(),
            spp,
        )
        .ok_or_else(unavailable)?;
        let owner_signers = parsed
            .data()
            .owner_signers(spp, window.len())
            .map_err(|_| unavailable())?
            .iter()
            .map(|owner| identity(owner.as_array()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(OpenedFrame {
            audited,
            owner_signers,
            withdrawal_accounts: withdrawal_accounts(&parsed.data().interface_transfers, window)?,
            public,
            approval_bits: parsed.data().approval.bits(),
            hidden_sender: rail != RingRail::Confidential
                || parsed.data().in_proof_owner.is_some()
                || parsed.data().anonymous.is_some(),
        }))
    }
}

/// A frame the auditor key opened, before any input note is read.
struct OpenedFrame {
    audited: AuditedTransaction,
    owner_signers: Vec<[u8; 32]>,
    public: Vec<PublicFlow>,
    /// The settlement group of each withdrawing leg.
    withdrawal_accounts: Vec<Address>,
    approval_bits: u8,
    /// An input owner signs inside the proof or stays hidden.
    hidden_sender: bool,
}

impl<'m> RingFrame<'m> {
    /// The signature covers the whole message, debits derive from all of it or it refuses.
    fn of(source: FrameSource<'m>, ring: Address) -> Result<Self, RingRpcError> {
        match Carried::decode(source.message, ring)? {
            Carried::Instruction(transfer) => {
                let (tag, data) = transfer.data.split_first().ok_or_else(scope)?;
                if source.approved_transfer.is_some()
                    || transfer.program != ring
                    || !matches!(*tag, tag::TRANSFER | tag::AUTHORITY_TRANSFER)
                {
                    return Err(scope());
                }
                Ok(Self {
                    tag: *tag,
                    data,
                    accounts: transfer.accounts,
                    approvers: Vec::new(),
                })
            }
            Carried::Approved {
                approval,
                approvers,
            } => {
                let call = source.approved_transfer.ok_or_else(scope)?;
                // A call verified under another message's approval is not the frame's.
                if call.approval() != approval {
                    return Err(scope());
                }
                let (tag, data) = call.data().split_first().ok_or_else(scope)?;
                Ok(Self {
                    tag: *tag,
                    data,
                    accounts: call.accounts().to_vec(),
                    approvers,
                })
            }
        }
    }

    /// In account order, a `[record, signer]` pair or the approver of a `[record, sysvar]` pair.
    fn co_signers(&self, ring: Address) -> Vec<Address> {
        let ring = CustomRing::new(ring);
        self.accounts
            .windows(2)
            .filter_map(|pair| {
                let signer = match pair[1] == sysvar::instructions::ID {
                    true => {
                        self.approvers
                            .iter()
                            .find(|approver| approver.record == pair[0])?
                            .signer
                    }
                    false => pair[1],
                };
                (pair[0] == ring.co_signer_pda(&signer)).then_some(signer)
            })
            .collect()
    }

    /// The ring's signed config opens SPP's list past the ring's own accounts.
    fn spp_accounts(&self, ring: Address) -> Result<&[Address], RingRpcError> {
        let ring_auth = pda::ring_auth(&ring).0;
        self.accounts
            .iter()
            .position(|account| *account == ring_auth)
            .and_then(|position| position.checked_sub(RING_TRANSACT_RING_CONFIG_INDEX))
            .and_then(|start| self.accounts.get(start..))
            .ok_or_else(unavailable)
    }
}

fn public_flows(
    parsed: &ParsedTransact<'_>,
    spp: &[Address],
) -> Result<Vec<PublicFlow>, RingRpcError> {
    let data = parsed.data();
    let window = settlement_accounts(&data.interface_transfers, data.circuit(), spp)
        .ok_or_else(unavailable)?;
    let named = settlement_mints(&data.interface_transfers, window)
        .map(|mint| {
            mint.map(Option::<&Address>::copied)
                .map_err(|_| unavailable())
        })
        .collect::<Result<Vec<_>, RingRpcError>>()?;
    fold_public_flows(data.interface_transfers.iter().copied().zip(named))
}

fn withdrawal_accounts(
    transfers: &[InterfaceTransfer],
    window: &[Address],
) -> Result<Vec<Address>, RingRpcError> {
    let mut rest = window;
    let mut accounts = Vec::new();
    for transfer in transfers {
        let (group, later) = rest
            .split_at_checked(transfer.settlement_account_count())
            .ok_or_else(unavailable)?;
        rest = later;
        if funder_account_position(transfer).is_none() {
            accounts.extend_from_slice(group);
        }
    }
    Ok(accounts)
}

/// The rows of a relayed deposit, the frame the relay proves from these legs carries the same.
pub(crate) fn deposit_flows(
    ring: Address,
    transfers: &[SettlementTransfer],
) -> Result<Vec<OwnerFlow>, RingRpcError> {
    let public = fold_public_flows(transfers.iter().map(|transfer| {
        let mint = match transfer {
            SettlementTransfer::Spl { mint, .. } => Some(*mint),
            SettlementTransfer::Sol { .. } => None,
        };
        (transfer.interface_transfer(), mint)
    }))?;
    OwnerFlows {
        inputs: &[],
        outputs: &[],
        owner_signers: &[],
        public: &public,
        no_spend_owner: Member::owner_tag(ring.as_array()).map_err(|_| unavailable())?,
    }
    .rows(|asset| {
        public
            .iter()
            .find(|flow| flow.asset == *asset)
            .map(|flow| flow.mint)
            .ok_or_else(unavailable)
    })
}

/// The program's fold bounds each net to a u64 magnitude, a gross past `u64::MAX` stays exact.
fn fold_public_flows(
    named_legs: impl ExactSizeIterator<Item = (InterfaceTransfer, Option<Address>)>,
) -> Result<Vec<PublicFlow>, RingRpcError> {
    // 1. Each leg names its mint, the first leg of a mint opens its slot as the fold orders them.
    let mut mints: Vec<(Address, [u8; 32])> = Vec::new();
    let mut legs = Vec::with_capacity(named_legs.len());
    for (transfer, named) in named_legs {
        let mint = named.unwrap_or(SOL_MINT);
        if !mints.iter().any(|(known, _)| *known == mint) {
            let asset = named.map_or(Ok(SOL_ASSET_FIELD), |mint| {
                hash_bytes(mint.as_array()).map_err(|_| unavailable())
            })?;
            mints.push((mint, asset));
        }
        legs.push(Ok(PublicLeg {
            transfer,
            mint: named.map(|mint| mint.to_bytes()),
        }));
    }
    // 2. The canonical fold refuses a net the program refuses, its gross is the withdrawal sum.
    let slots = PublicSlots::fold::<PublicSlotError>(legs).map_err(|_| unavailable())?;
    if slots.len() != mints.len() {
        return Err(unavailable());
    }
    // 3. Deposits are the gross plus the signed net, the net counts a deposit positive.
    mints
        .into_iter()
        .zip(slots.gross().into_iter().zip(slots.amounts()))
        .map(|((mint, asset), (withdrawn, net))| {
            let deposited = i128::try_from(withdrawn)
                .ok()
                .and_then(|gross| gross.checked_add(net))
                .and_then(|deposited| u128::try_from(deposited).ok())
                .ok_or_else(unavailable)?;
            Ok(PublicFlow {
                mint,
                asset,
                deposited,
                withdrawn,
            })
        })
        .collect()
}

/// A public leg has no owner on the frame, it joins the first sender or a no-spend frame's ring tag.
pub(super) struct OwnerFlows<'a> {
    pub(super) inputs: &'a [DebitInput],
    pub(super) outputs: &'a [DebitOutput],
    pub(super) owner_signers: &'a [[u8; 32]],
    pub(super) public: &'a [PublicFlow],
    /// Only ring caps read a public leg, the tag answers for no owner cap.
    pub(super) no_spend_owner: Member,
}

impl OwnerFlows<'_> {
    /// One row per nonzero debit or public leg, each owner and asset once.
    pub(super) fn rows(
        self,
        mint_of: impl Fn(&[u8; 32]) -> Result<Address, RingRpcError>,
    ) -> Result<Vec<OwnerFlow>, RingRpcError> {
        // Only an input owner holds a nonzero debit, the proof binds each to the rail's senders.
        let senders: Vec<[u8; 32]> = self
            .inputs
            .iter()
            .map(|input| input.owner)
            .chain(self.owner_signers.iter().copied())
            .collect();
        let public_gross: Vec<PublicGross> = self
            .public
            .iter()
            .map(|public| PublicGross {
                asset: public.asset,
                gross: public.withdrawn,
            })
            .collect();
        let assets: Vec<[u8; 32]> = self
            .outputs
            .iter()
            .map(|output| output.asset)
            .chain(self.inputs.iter().map(|input| input.asset))
            .chain(public_gross.iter().map(|public| public.asset))
            .collect();
        let mut flows = owner_debits(&DebitTransfer {
            senders: &senders,
            assets: &assets,
            inputs: self.inputs,
            outputs: self.outputs,
            public_gross: &public_gross,
        })
        .iter()
        .map(|debit| {
            Ok(OwnerFlow {
                owner: member(debit.owner)?,
                flow: AssetFlow {
                    asset: mint_of(&debit.asset)?,
                    debit: debit.debit,
                    charged_outflow: debit.charged_outflow,
                    deposited: 0,
                    withdrawn: 0,
                },
            })
        })
        .collect::<Result<Vec<_>, RingRpcError>>()?;
        for public in self.public {
            let owner = senders
                .first()
                .map_or(Ok(self.no_spend_owner), |sender| member(*sender))?;
            let joined = flows
                .iter()
                .position(|flow| flow.flow.asset == public.mint && flow.owner == owner);
            let row = match joined {
                Some(row) => row,
                None => {
                    flows.push(OwnerFlow {
                        owner,
                        flow: AssetFlow {
                            asset: public.mint,
                            debit: 0,
                            charged_outflow: 0,
                            deposited: 0,
                            withdrawn: 0,
                        },
                    });
                    flows.len() - 1
                }
            };
            flows[row].flow.deposited = public.deposited;
            flows[row].flow.withdrawn = public.withdrawn;
        }
        Ok(flows)
    }
}

/// The note of each published nullifier the index holds, the rest spend no note it opened.
type InputNotes = HashMap<[u8; 32], ([u8; 32], StoredUtxo)>;

/// Read before the flow math, a note the cache evicted answers from storage.
async fn real_input_notes(
    store: &AuditStore,
    audited: &AuditedTransaction,
) -> Result<InputNotes, RingRpcError> {
    store.notes_of_nullifiers(&audited.nullifiers).await
}

struct FrameFlows<'a> {
    audited: &'a AuditedTransaction,
    notes: &'a InputNotes,
    /// The public run a public-funded transfer screens, it spends no note.
    owner_signers: Vec<[u8; 32]>,
    public: Vec<PublicFlow>,
    assets: &'a RingAssets,
    ring: Address,
}

impl FrameFlows<'_> {
    fn read(&self, approval_bits: u8) -> Result<TransferRead, RingRpcError> {
        let inputs = self.inputs()?;
        Ok(TransferRead {
            flows: OwnerFlows {
                inputs: &inputs,
                outputs: &self.outputs()?,
                owner_signers: &self.owner_signers,
                public: &self.public,
                no_spend_owner: Member::owner_tag(self.ring.as_array())
                    .map_err(|_| unavailable())?,
            }
            .rows(|asset| self.mint_of(asset))?,
            spends_note: !inputs.is_empty(),
            approval_bits: Some(approval_bits),
            co_signers: Vec::new(),
        })
    }

    /// The proof's `R` must equal the notes the index names plus the entry records, one note the
    /// index cannot name refuses the read.
    fn inputs(&self) -> Result<Vec<DebitInput>, RingRpcError> {
        let audited = self.audited;
        let mut inputs = Vec::new();
        for nullifier in &audited.nullifiers {
            if let Some((_, note)) = self.notes.get(nullifier) {
                inputs.push(DebitInput {
                    owner: identity(&note.utxo.owner_key)?,
                    asset: note.utxo.asset,
                    amount: note.utxo.amount,
                });
            }
        }
        inputs.extend(self.entries()?);
        match inputs.len() == usize::from(audited.real_inputs) {
            true => Ok(inputs),
            false => Err(unavailable()),
        }
    }

    /// The frame's proven entry evidence, a default-ring note holds no index row.
    fn entries(&self) -> Result<Vec<DebitInput>, RingRpcError> {
        self.audited
            .entries
            .iter()
            .map(|entry| {
                Ok(DebitInput {
                    owner: identity(&entry.owner_key)?,
                    asset: entry.asset,
                    amount: entry.amount,
                })
            })
            .collect()
    }

    /// A slot with no proven record could return funds, the outflow stays unknown.
    fn outputs(&self) -> Result<Vec<DebitOutput>, RingRpcError> {
        if self.audited.unopened().is_some() {
            return Err(unavailable());
        }
        let mut outputs = Vec::new();
        for output in &self.audited.outputs {
            let (ring, utxo) = match &output.opening {
                OutputOpening::Dummy => continue,
                OutputOpening::Ring { utxo, .. } => (true, utxo),
                OutputOpening::Exit(utxo) => (false, utxo),
            };
            outputs.push(DebitOutput {
                ring,
                owner: identity(&utxo.owner_key)?,
                asset: utxo.asset,
                amount: utxo.amount,
            });
        }
        Ok(outputs)
    }

    fn mint_of(&self, asset: &[u8; 32]) -> Result<Address, RingRpcError> {
        self.assets
            .by_field(asset)
            .map(|mint| mint.asset)
            .ok_or_else(unavailable)
    }
}

fn identity(owner_key: &[u8; 32]) -> Result<[u8; 32], RingRpcError> {
    Member::owner_tag(owner_key)
        .map(|member| *member.as_bytes())
        .map_err(|_| unavailable())
}

fn member(owner: [u8; 32]) -> Result<Member, RingRpcError> {
    Member::from_bytes(owner).map_err(|_| unavailable())
}

#[cfg(test)]
#[path = "frame_flows_tests.rs"]
mod frame_flows_tests;

#[cfg(test)]
mod tests {
    use solana_instruction::{AccountMeta, Instruction};
    use spr_client::{compile_message, ComputeBudgetConfig};
    use zolana_ring_client::rpc::ApprovedTransfer;

    use super::*;
    use crate::cosigner::ApprovedCall;

    const RING: Address = Address::new_from_array([0x11; 32]);
    const PAYER: Address = Address::new_from_array([0x22; 32]);
    const OTHER_PROGRAM: Address = Address::new_from_array([0x33; 32]);

    fn ring_instruction(tag: u8) -> Instruction {
        Instruction {
            program_id: RING,
            accounts: vec![AccountMeta::new(PAYER, true)],
            data: vec![tag, 1, 2],
        }
    }

    fn message(instructions: &[Instruction]) -> VersionedMessage {
        compile_message(
            &PAYER,
            instructions,
            solana_hash::Hash::default(),
            ComputeBudgetConfig::new(1),
        )
        .expect("message")
    }

    fn direct(message: &VersionedMessage) -> FrameSource<'_> {
        FrameSource {
            message,
            approved_transfer: None,
        }
    }

    fn refuses_scope(instructions: &[Instruction]) {
        assert!(matches!(
            RingFrame::of(direct(&message(instructions)), RING).err(),
            Some(RingRpcError::CoSign(CoSignError::MessageScope))
        ));
    }

    /// A gross past `u64::MAX` under a valid net passes exact, an aggregate the program refuses refuses here.
    #[test]
    fn a_gross_past_u64_passes_and_an_invalid_aggregate_refuses() {
        let deposit = |amount| InterfaceTransfer::SolDeposit { amount };
        let withdrawal = |amount| InterfaceTransfer::SolWithdrawal { amount };
        let fold = |transfers: &[InterfaceTransfer]| {
            fold_public_flows(transfers.iter().map(|transfer| (*transfer, None)))
        };
        let past_u64 = u128::from(u64::MAX) + 1;
        for (transfers, deposited, withdrawn) in [
            (
                [withdrawal(u64::MAX), deposit(u64::MAX), withdrawal(1)],
                u128::from(u64::MAX),
                past_u64,
            ),
            (
                [deposit(u64::MAX), withdrawal(u64::MAX), deposit(1)],
                past_u64,
                u128::from(u64::MAX),
            ),
        ] {
            let flows = fold(&transfers).expect("a valid net");
            assert_eq!(flows.len(), 1);
            assert_eq!(
                (flows[0].deposited, flows[0].withdrawn),
                (deposited, withdrawn)
            );
        }
        for invalid in [
            vec![withdrawal(u64::MAX), withdrawal(1)],
            vec![deposit(5), withdrawal(5)],
        ] {
            assert!(matches!(
                fold(&invalid),
                Err(RingRpcError::CoSign(CoSignError::LimitsUnavailable))
            ));
        }
    }

    const VECTOR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test-vectors/rpc/owner_debits.json"
    );

    /// Two rows of one owner aggregate, an exit leaves, a credit never moves, a public-funded transfer joins its signer.
    fn vector_cases() -> serde_json::Value {
        let field = |byte: u8| hex::encode(right_aligned(byte));
        let input = |owner: u8, asset: u8, amount: u64| serde_json::json!({"owner": field(owner), "asset": field(asset), "amount": amount});
        let output = |ring: bool, owner: u8, asset: u8, amount: u64| serde_json::json!({"ring": ring, "owner": field(owner), "asset": field(asset), "amount": amount});
        let public = |asset: u8, deposited: u64, withdrawn: u64| serde_json::json!({"mint": hex::encode(mint_of_field(asset)), "asset": field(asset), "deposited": deposited, "withdrawn": withdrawn});
        serde_json::json!([
            {"name": "one owner keeps change", "inputs": [input(1, 7, 30), input(1, 7, 20)], "outputs": [output(true, 2, 7, 15), output(true, 1, 7, 35)], "ownerSigners": [field(1)], "public": []},
            {"name": "an exit to the owner leaves the ring", "inputs": [input(1, 7, 30)], "outputs": [output(false, 1, 7, 30)], "ownerSigners": [field(1)], "public": [public(7, 0, 30)]},
            {"name": "a credit of another owner never moves", "inputs": [input(1, 7, 30), input(2, 7, 10)], "outputs": [output(true, 2, 7, 25), output(true, 3, 7, 15)], "ownerSigners": [field(1), field(2)], "public": []},
            {"name": "two assets of two owners", "inputs": [input(1, 7, 30), input(2, 8, 10)], "outputs": [output(true, 3, 7, 30), output(true, 3, 8, 10)], "ownerSigners": [field(1), field(2)], "public": []},
            {"name": "a public funded transfer joins its signer", "inputs": [], "outputs": [output(true, 3, 7, 40)], "ownerSigners": [field(1)], "public": [public(7, 40, 0)]}
        ])
    }

    fn right_aligned(byte: u8) -> [u8; 32] {
        let mut field = [0; 32];
        field[31] = byte;
        field
    }

    fn mint_of_field(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn bytes32(value: &serde_json::Value) -> [u8; 32] {
        hex::decode(value.as_str().expect("hex"))
            .expect("hex")
            .try_into()
            .expect("32 bytes")
    }

    fn rows_of(case: &serde_json::Value) -> Vec<serde_json::Value> {
        let inputs: Vec<DebitInput> = case["inputs"]
            .as_array()
            .expect("inputs")
            .iter()
            .map(|input| DebitInput {
                owner: bytes32(&input["owner"]),
                asset: bytes32(&input["asset"]),
                amount: input["amount"].as_u64().expect("amount"),
            })
            .collect();
        let outputs: Vec<DebitOutput> = case["outputs"]
            .as_array()
            .expect("outputs")
            .iter()
            .map(|output| DebitOutput {
                ring: output["ring"].as_bool().expect("ring"),
                owner: bytes32(&output["owner"]),
                asset: bytes32(&output["asset"]),
                amount: output["amount"].as_u64().expect("amount"),
            })
            .collect();
        let owner_signers: Vec<[u8; 32]> = case["ownerSigners"]
            .as_array()
            .expect("signers")
            .iter()
            .map(bytes32)
            .collect();
        let public: Vec<PublicFlow> = case["public"]
            .as_array()
            .expect("public")
            .iter()
            .map(|flow| PublicFlow {
                mint: Address::new_from_array(bytes32(&flow["mint"])),
                asset: bytes32(&flow["asset"]),
                deposited: flow["deposited"].as_u64().expect("deposited").into(),
                withdrawn: flow["withdrawn"].as_u64().expect("withdrawn").into(),
            })
            .collect();
        OwnerFlows {
            inputs: &inputs,
            outputs: &outputs,
            owner_signers: &owner_signers,
            public: &public,
            no_spend_owner: Member::from_bytes([0xee; 32]).expect("tag"),
        }
        .rows(|asset| Ok(Address::new_from_array([asset[31]; 32])))
        .expect("rows")
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "owner": hex::encode(row.owner.as_bytes()),
                "mint": hex::encode(row.flow.asset.to_bytes()),
                "debit": row.flow.charged_outflow,
                "deposited": row.flow.deposited,
                "withdrawn": row.flow.withdrawn,
            })
        })
        .collect()
    }

    /// `UPDATE_RING_VECTORS=1` rewrites the file.
    #[test]
    fn the_owner_debits_vector_holds() {
        let cases: Vec<serde_json::Value> = vector_cases()
            .as_array()
            .expect("cases")
            .iter()
            .map(|case| {
                let mut case = case.clone();
                case["flows"] = serde_json::Value::Array(rows_of(&case));
                case
            })
            .collect();
        let built = serde_json::json!({ "cases": cases });
        if std::env::var_os("UPDATE_RING_VECTORS").is_some() {
            let pretty = serde_json::to_string_pretty(&built).expect("json");
            std::fs::write(VECTOR, pretty + "\n").expect("vector");
        }
        let pinned: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(VECTOR).expect("vector")).expect("json");
        assert_eq!(built, pinned);
    }

    #[test]
    fn a_deposit_leg_names_no_withdrawal_account() {
        let deposit = InterfaceTransfer::SolDeposit { amount: 1 };
        let withdrawal = InterfaceTransfer::SolWithdrawal { amount: 1 };
        let legs = [deposit, withdrawal, deposit];
        let window: Vec<Address> = (1u8..)
            .map(|seed| Address::new_from_array([seed; 32]))
            .take(legs.iter().map(|leg| leg.settlement_account_count()).sum())
            .collect();
        let deposited = deposit.settlement_account_count();
        let withdrawn = withdrawal.settlement_account_count();
        assert_eq!(
            withdrawal_accounts(&legs, &window).expect("groups"),
            window[deposited..deposited + withdrawn]
        );
        assert!(withdrawal_accounts(&legs, &window[1..]).is_err());
    }

    #[test]
    fn one_transfer_alone_is_the_frame() {
        let transfer = ring_instruction(tag::TRANSFER);
        let unrelated = Instruction {
            program_id: OTHER_PROGRAM,
            accounts: vec![AccountMeta::new(PAYER, true)],
            data: vec![9],
        };
        for instructions in [
            vec![transfer.clone()],
            vec![unrelated.clone(), transfer.clone()],
            vec![transfer.clone(), unrelated],
        ] {
            let message = message(&instructions);
            let frame = RingFrame::of(direct(&message), RING).expect("one transfer");
            assert_eq!(frame.tag, tag::TRANSFER);
            assert_eq!(frame.data, [1, 2]);
        }
    }

    const CO_SIGNER: Address = Address::new_from_array([0x55; 32]);

    fn record() -> AccountMeta {
        AccountMeta::new_readonly(CustomRing::new(RING).co_signer_pda(&CO_SIGNER), false)
    }

    /// The transfer with `second` after the co-signer's record.
    fn co_signed_transfer(second: AccountMeta) -> Instruction {
        Instruction {
            accounts: vec![AccountMeta::new(PAYER, true), record(), second],
            ..ring_instruction(tag::TRANSFER)
        }
    }

    fn called_transfer() -> Instruction {
        co_signed_transfer(AccountMeta::new_readonly(sysvar::instructions::ID, false))
    }

    fn committing(position: u8, transfer: &Instruction) -> TransferApproval {
        let digest = TransferApproval::digest(
            &RING,
            &transfer.data,
            transfer.accounts.iter().map(|account| &account.pubkey),
        )
        .expect("digest");
        TransferApproval { position, digest }
    }

    fn approved(position: u8, transfer: &Instruction) -> ApprovedCall {
        let named = ApprovedTransfer {
            data: transfer.data.clone().into(),
            accounts: transfer
                .accounts
                .iter()
                .map(|account| account.pubkey.into())
                .collect(),
        };
        ApprovedCall::committed(RING, committing(position, transfer), named).expect("committed")
    }

    fn approval(position: u8, transfer: &Instruction) -> Instruction {
        Instruction {
            program_id: RING,
            accounts: vec![
                AccountMeta::new_readonly(CustomRing::new(RING).policy_pda(), false),
                record(),
                AccountMeta::new_readonly(CO_SIGNER, true),
            ],
            data: committing(position, transfer).encode().to_vec(),
        }
    }

    /// A program able to call `target`.
    fn caller(target: Address) -> Instruction {
        Instruction {
            program_id: OTHER_PROGRAM,
            accounts: vec![
                AccountMeta::new(PAYER, true),
                AccountMeta::new_readonly(target, false),
            ],
            data: vec![9],
        }
    }

    fn approved_frame<'m>(
        message: &'m VersionedMessage,
        approved_transfer: Option<&'m ApprovedCall>,
    ) -> Result<RingFrame<'m>, RingRpcError> {
        let source = FrameSource {
            message,
            approved_transfer,
        };
        RingFrame::of(source, RING)
    }

    #[test]
    fn an_approved_call_reads_as_the_frame_of_its_direct_form() {
        let signed = co_signed_transfer(AccountMeta::new_readonly(CO_SIGNER, true));
        let signed_message = message(std::slice::from_ref(&signed));
        let direct_frame = RingFrame::of(direct(&signed_message), RING).expect("direct");

        let transfer = called_transfer();
        let call = approved(1, &transfer);
        let second_approval = approval(1, &transfer);
        let wrapped = message(&[approval(1, &transfer), caller(RING), second_approval]);
        let frame = approved_frame(&wrapped, Some(&call)).expect("approved");

        assert_eq!(
            (frame.tag, frame.data),
            (direct_frame.tag, direct_frame.data)
        );
        assert_eq!(frame.accounts[..2], direct_frame.accounts[..2]);
        assert_eq!(frame.co_signers(RING), [CO_SIGNER]);
        assert_eq!(direct_frame.co_signers(RING), [CO_SIGNER]);
    }

    #[test]
    fn an_approval_admits_only_the_call_it_commits_to_at_its_position() {
        let transfer = called_transfer();
        let call = approved(0, &transfer);
        let refused = |instructions: &[Instruction], call: Option<&ApprovedCall>| {
            assert!(matches!(
                approved_frame(&message(instructions), call).err(),
                Some(RingRpcError::CoSign(CoSignError::MessageScope))
            ));
        };
        let wrapped = [caller(RING), approval(0, &transfer)];
        refused(&wrapped, None);

        let other_transfer = Instruction {
            data: vec![tag::TRANSFER, 3],
            ..transfer.clone()
        };
        let other_digest = approval(0, &other_transfer);
        // A call verified under another approval is not the message's.
        refused(&wrapped, Some(&approved(0, &other_transfer)));
        refused(&wrapped, Some(&approved(1, &transfer)));
        refused(
            &[wrapped[0].clone(), wrapped[1].clone(), other_digest],
            Some(&call),
        );
        // Neither a program without the ring nor the ring's own instruction is the caller.
        refused(
            &[caller(PROGRAM_ID_PUBKEY), approval(0, &transfer)],
            Some(&call),
        );
        refused(&[transfer.clone(), approval(0, &transfer)], Some(&call));
        let unrelated = Instruction {
            accounts: vec![AccountMeta::new(PAYER, true)],
            ..caller(RING)
        };
        refused(
            &[unrelated, caller(RING), approval(0, &transfer)],
            Some(&call),
        );
        refused(
            &[caller(RING), caller(RING), approval(0, &transfer)],
            Some(&call),
        );
        // A message holding its own transfer takes no approved call.
        refused(std::slice::from_ref(&transfer), Some(&call));
    }

    #[test]
    fn a_second_transfer_refuses_in_either_order() {
        let transfer = ring_instruction(tag::TRANSFER);
        let authority = ring_instruction(tag::AUTHORITY_TRANSFER);
        refuses_scope(&[transfer.clone(), transfer.clone()]);
        refuses_scope(&[transfer.clone(), authority.clone()]);
        refuses_scope(&[authority, transfer]);
    }

    #[test]
    fn any_further_ring_or_spp_reach_refuses() {
        let transfer = ring_instruction(tag::TRANSFER);
        let merge = ring_instruction(tag::MERGE);
        refuses_scope(&[transfer.clone(), merge.clone()]);
        refuses_scope(&[merge, transfer.clone()]);
        let direct_spp = Instruction {
            program_id: PROGRAM_ID_PUBKEY,
            accounts: vec![AccountMeta::new(PAYER, true)],
            data: vec![0],
        };
        refuses_scope(&[transfer.clone(), direct_spp.clone()]);
        refuses_scope(&[direct_spp, transfer.clone()]);
        // A wrapper can invoke only what its accounts name.
        let wrapper = |target: Address| Instruction {
            program_id: OTHER_PROGRAM,
            accounts: vec![AccountMeta::new_readonly(target, false)],
            data: vec![],
        };
        refuses_scope(&[transfer.clone(), wrapper(RING)]);
        refuses_scope(&[wrapper(PROGRAM_ID_PUBKEY), transfer]);
    }

    /// A wrapper whose program reaches `hidden` through a lookup table, an index past the static keys.
    fn v0_with_a_table(hidden_slot: u8) -> VersionedMessage {
        use solana_message::{
            compiled_instruction::CompiledInstruction,
            v0::{self, MessageAddressTableLookup},
            MessageHeader,
        };
        let transfer = ring_instruction(tag::TRANSFER);
        VersionedMessage::V0(v0::Message {
            header: MessageHeader {
                num_required_signatures: 1,
                num_readonly_signed_accounts: 0,
                num_readonly_unsigned_accounts: 2,
            },
            account_keys: vec![PAYER, RING, OTHER_PROGRAM],
            recent_blockhash: solana_hash::Hash::default(),
            instructions: vec![
                CompiledInstruction {
                    program_id_index: 1,
                    accounts: vec![0],
                    data: transfer.data,
                },
                CompiledInstruction {
                    program_id_index: 2,
                    accounts: vec![3],
                    data: vec![],
                },
            ],
            address_table_lookups: vec![MessageAddressTableLookup {
                account_key: Address::new_from_array([0x44; 32]),
                writable_indexes: vec![],
                readonly_indexes: vec![hidden_slot],
            }],
        })
    }

    #[test]
    fn a_message_that_can_hide_an_account_refuses() {
        for hidden_slot in [0, 1] {
            assert!(matches!(
                RingFrame::of(direct(&v0_with_a_table(hidden_slot)), RING).err(),
                Some(RingRpcError::CoSign(CoSignError::MessageScope))
            ));
        }
        let legacy = VersionedMessage::Legacy(solana_message::legacy::Message::new(
            &[ring_instruction(tag::TRANSFER)],
            Some(&PAYER),
        ));
        assert!(matches!(
            RingFrame::of(direct(&legacy), RING).err(),
            Some(RingRpcError::CoSign(CoSignError::MessageScope))
        ));
    }

    #[test]
    fn an_index_outside_the_account_list_refuses_and_never_reads_unrelated() {
        let unrelated = Instruction {
            program_id: OTHER_PROGRAM,
            accounts: vec![AccountMeta::new(PAYER, true)],
            data: vec![9],
        };
        let mut wrapper_account = message(&[ring_instruction(tag::TRANSFER), unrelated.clone()]);
        let VersionedMessage::V1(held) = &mut wrapper_account else {
            panic!("version 1");
        };
        held.instructions[1].accounts.push(200);
        assert!(matches!(
            RingFrame::of(direct(&wrapper_account), RING).err(),
            Some(RingRpcError::CoSign(CoSignError::MessageScope))
        ));
        let mut wrapper_program = message(&[ring_instruction(tag::TRANSFER), unrelated]);
        let VersionedMessage::V1(held) = &mut wrapper_program else {
            panic!("version 1");
        };
        held.instructions[1].program_id_index = 200;
        assert!(matches!(
            RingFrame::of(direct(&wrapper_program), RING).err(),
            Some(RingRpcError::CoSign(CoSignError::MessageScope))
        ));
    }

    #[test]
    fn a_message_with_no_transfer_refuses() {
        refuses_scope(&[ring_instruction(tag::MERGE)]);
        let unrelated = Instruction {
            program_id: OTHER_PROGRAM,
            accounts: vec![AccountMeta::new(PAYER, true)],
            data: vec![9],
        };
        refuses_scope(&[unrelated]);
    }
}
