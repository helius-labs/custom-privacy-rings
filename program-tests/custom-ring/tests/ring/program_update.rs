//! An Xark program creates and updates its data note on a ring through approved CPI, with real proofs.

use std::{path::Path, sync::Arc};

use custom_ring_interface::TransferApproval;
use custom_ring_test_validator::{ring_rpc::CoSigningRingRpc, shared::CoSigned};
use ring_cosigner::cosign::{ApproveClass, CoSignError, CoSignerApproval};
use ring_writer::record_update::MAX_LIMIT_WINDOW_SLOTS;
use solana_message::VersionedMessage;
use solana_signer::null_signer::NullSigner;
use solana_transaction::versioned::VersionedTransaction;
use spr_program::{
    hasher::{data_hash, DataHasher},
    proof::compressed_proof,
    to_bytes, InputRecords, ProgramProofInputs,
};
use zk_ring_program::{
    circuits::{
        Counter, Create, CreatePrivateInputs, OwnerPublicInputs, Update, UpdatePrivateInputs,
    },
    client::{owner, ProvenInstruction},
};
use zolana_ring_client::{
    rpc::CoSignTransactionRequest, ApplicationData, ApprovalEncode, NoteData, ProgramCommitment,
};
use zolana_ring_policy::AssetLimits;

use super::*;

const HELD: u64 = 2_000_000_000;
const PAID: u64 = 500_000_000;
/// An outflow above it demands the co-signer.
const COSIGN_ABOVE: u64 = 100_000_000;
/// Ring RPC co-signs no more of one sender inside its window.
const SENDER_CAP: u64 = 1_000_000_000;
/// No slot of a run leaves the window.
const SENDER_WINDOW_SLOTS: u64 = MAX_LIMIT_WINDOW_SLOTS;
const CHANGE_SLOT: usize = 0;
const STATE_SLOT: usize = 1;

/// One call of the program, a create without `state`.
#[derive(Clone, Copy)]
struct ProgramStep<'a> {
    ring: CustomRing,
    keys: &'a LocalShieldedKeys,
    zero_key: &'a NullifierKey,
    token: &'a WalletUtxo,
    state: Option<(&'a WalletUtxo, Counter)>,
    recipient: ShieldedAddress,
    paid: u64,
    co_signer: Address,
}

struct ProvenStep {
    transfer: ProvenRingTransfer,
    /// Each approval, then the program's call.
    instructions: Vec<Instruction>,
}

impl ProgramStep<'_> {
    fn next(&self) -> Counter {
        Counter {
            count: self.state.map_or(0, |(_, counter)| counter.count + 1),
        }
    }

    fn payment(
        &self,
        to: ShieldedAddress,
        amount: u64,
        application: Option<ApplicationData>,
    ) -> zolana_ring_client::AnonymousPayment {
        zolana_ring_client::AnonymousPayment {
            to: RingAddress {
                recipient: to,
                ring_program_id: self.ring.program_id(),
                list_salt: [0; 32],
            },
            asset: Mint::SOL,
            amount,
            reply: None,
            data: NoteData {
                ring_data_hash: None,
                application,
            },
            request_tag: None,
        }
    }

    /// The change leads the outputs as the circuits order them.
    fn transfer(&self, payer: Address) -> Result<zolana_ring_client::PublicOwnerTransfer<'_>> {
        let ring_program = self.ring.program_id();
        let address = self.keys.address()?;
        let inputs = std::iter::once(self.token)
            .chain(self.state.map(|(state, _)| state))
            .cloned()
            .collect();
        let mut transaction = ConfidentialTransaction::new_with_ring(inputs, payer, ring_program)?;
        transaction.add_output_utxo(SppProofOutputUtxo {
            owner_address: Some(address),
            asset: Mint::SOL,
            amount: self.token.utxo.amount - self.paid,
            ring_program_id: Some(ring_program),
            ..Default::default()
        })?;
        let next = self.next();
        let transfer = CustomRingTransfer::new(CustomRingTransferInput {
            ring: self.ring,
            sender: self.keys,
            nullifier_key: Some(self.zero_key),
            transaction,
        })
        .spend_public(*owner(&ring_program).pda())
        .with_payment(self.payment(
            address,
            0,
            Some(ApplicationData {
                hash: data_hash(&DataHasher::hash(&next)?)?,
                bytes: borsh::to_vec(&next)?,
            }),
        ))?
        .with_co_signer(self.co_signer);
        Ok(match self.paid {
            0 => transfer,
            paid => transfer.with_payment(self.payment(self.recipient, paid, None))?,
        })
    }

    /// The program proof over the plan, then the ring proof staged under its commitment.
    fn prove(&self, env: &TestEnv, payer: Address) -> Result<ProvenStep> {
        let plan = env.run(
            self.transfer(payer)?
                .plan_for_program(env.client.ring_client()),
        )?;
        let mut records = InputRecords::new();
        let tx_context = InputRecords::context(&plan.blinding_seed, Some(plan.output_tree_id))?;
        let tokens = records.utxos(std::slice::from_ref(self.token))?;
        let public = OwnerPublicInputs {
            owner: *records.address(&self.keys.address()?)?.owner(),
            ring: records.ring(self.ring.program_id())?,
        };
        let (artifact, checked, proof_inputs) = match self.state {
            None => {
                let create = Create {
                    private: CreatePrivateInputs { tx_context, tokens },
                    public,
                };
                let checked = spr_program::checked(|| create.execute())?;
                let inputs = ProgramProofInputs::new(&create, *checked.public_hash());
                ("create", checked, inputs)
            }
            Some((state, counter)) => {
                let update = Update {
                    private: UpdatePrivateInputs {
                        tx_context,
                        state: records.utxo(state)?,
                        counter,
                        tokens,
                        recipient: records.address(&self.recipient)?,
                        amount: self.paid,
                    },
                    public,
                };
                let checked = spr_program::checked(|| update.execute())?;
                let inputs = ProgramProofInputs::new(&update, *checked.public_hash());
                ("update", checked, inputs)
            }
        };
        let commitment = ProgramCommitment {
            salt: plan.salt,
            private_tx_hash: to_bytes(checked.private_tx_hash()),
            transaction_hash: to_bytes(checked.transaction_hash()),
        };
        plan.check(&commitment)?;
        let artifacts = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/zk/zk-ring-program"
        );
        let proven =
            xark::prover::circuit_at(Path::new(artifacts).join(artifact)).prove(proof_inputs)?;
        let proof = compressed_proof(&proven.proof).map_err(anyhow::Error::msg)?;
        let transfer = env.run(
            self.transfer(payer)?
                .with_program(commitment)
                .prove(env.client.ring_client()),
        )?;
        let call = match self.state {
            None => ProvenInstruction::create(proof, transfer.instruction()?),
            Some(_) => ProvenInstruction::update(proof, transfer.instruction()?),
        };
        // The approvals lead, the program's call sits at their count.
        let position = u8::try_from(transfer.co_signers.len())?;
        let mut instructions = transfer.approve_transfer(position)?;
        instructions.push(call.instruction());
        Ok(ProvenStep {
            transfer,
            instructions,
        })
    }
}

impl ProvenStep {
    fn message(&self, env: &TestEnv, payer: Address) -> Result<VersionedMessage> {
        let (blockhash, _) = env.client.rpc().get_latest_blockhash()?;
        Ok(compile_message(
            &payer,
            &self.instructions,
            blockhash,
            ComputeBudgetConfig::new(compute_unit_limit(InstructionTag::Transfer)),
        )?)
    }

    fn co_sign_request(
        &self,
        ring: CustomRing,
        transaction: &VersionedTransaction,
    ) -> Result<CoSignTransactionRequest> {
        Ok(CoSignTransactionRequest {
            ring: ring.program_id().into(),
            transaction: wincode::serialize(transaction)?.into(),
            approved_transfer: Some(self.transfer.approved_transfer()?),
        })
    }

    /// Ring RPC signs each demanded approval, a call that demands none goes out under the payer alone.
    fn land(
        &self,
        env: &TestEnv,
        ring: CustomRing,
        ring_rpc: &CoSigningRingRpc,
        payer: &dyn Signer,
    ) -> Result<Signature> {
        if self.transfer.co_signers.is_empty() {
            return send(env.client.rpc(), payer, &self.instructions);
        }
        CoSignAsk {
            env,
            ring,
            ring_rpc,
            payer,
        }
        .co_sign_once_read(self)?
        .send(env.client.rpc())
    }
}

/// The payer signs ahead of Ring RPC, whose slot stays zero.
struct Presigning<'a> {
    payer: &'a dyn Signer,
    co_signer: Address,
}

impl Presigning<'_> {
    fn transaction(&self, message: VersionedMessage) -> Result<VersionedTransaction> {
        let unsigned = NullSigner::new(&self.co_signer);
        Ok(sign_transaction(message, &[self.payer, &unsigned])?)
    }
}

/// Each attempt presigns a message over a fresh blockhash, a long wait for Ring RPC never expires it.
struct CoSignAsk<'a> {
    env: &'a TestEnv,
    ring: CustomRing,
    ring_rpc: &'a CoSigningRingRpc,
    payer: &'a dyn Signer,
}

impl<'a> CoSignAsk<'a> {
    /// Ring RPC refuses an input it has not ingested yet and a client above its rate, the wallet asks again.
    fn co_sign_once_read(&self, step: &ProvenStep) -> Result<CoSigned<'a>> {
        let retried = [
            (CoSignError::LimitsUnavailable as i32).to_string(),
            "429 Too Many Requests".to_owned(),
        ];
        let co_signer = self.ring_rpc.co_signer;
        wait_for("Ring RPC to read the inputs", || {
            let message = step.message(self.env, self.payer.pubkey())?;
            let presigned = Presigning {
                payer: self.payer,
                co_signer,
            }
            .transaction(message.clone())?;
            match self
                .ring_rpc
                .co_sign(step.co_sign_request(self.ring, &presigned)?)
            {
                Ok(co_signed) => Ok(Probe::Ready(CoSigned {
                    payer: self.payer,
                    message,
                    co_signer,
                    co_signature: co_signed.signature.0,
                })),
                Err(refusal)
                    if retried
                        .iter()
                        .any(|sign| refusal.to_string().contains(sign)) =>
                {
                    Ok(Probe::Retry(refusal.into()))
                }
                Err(refusal) => Err(refusal.into()),
            }
        })
    }
}

/// A ring that demands Ring RPC's co-signature, the program's counter created then updated once.
struct Updated {
    env: TestEnv,
    ring: CustomRing,
    auditor: X25519Keypair,
    co_signer: Keypair,
    ring_rpc: CoSigningRingRpc,
    keys: LocalShieldedKeys,
    zero_key: NullifierKey,
    token: WalletUtxo,
    state: WalletUtxo,
    signature: Signature,
    outputs: Vec<SppProofOutputUtxo>,
}

impl Updated {
    fn new() -> Result<Self> {
        let env =
            setup_with_programs(&[(zk_ring_program::ID, "target/deploy/zk_ring_program.so")])?;
        let rpc = env.client.rpc();
        let ring = CustomRing::new(custom_ring_program_id()?);
        let auditor = X25519Keypair::new();
        RegisterRing {
            ring,
            payer: &env.payer,
            auditor_pubkey: auditor.pubkey(),
            tier: Tier::policy(&EMPTY),
        }
        .send(&env.client)?;
        enroll_actors(&env, ring)?;

        // 1. An outflow above the threshold demands the co-signer Ring RPC holds.
        let co_signer = Keypair::new();
        let ring_rpc = RingRpcSpec {
            env: &env,
            ring: ring.program_id(),
            auditor: auditor.clone(),
        }
        .serve_co_signer(
            Arc::new(co_signer.insecure_clone()),
            CoSignerApproval::new([ApproveClass::Outflow]),
        )?;
        let cosign_admin = RingRoleKeys::of(&env.payer).cosign_admin;
        SendAs {
            rpc,
            payer: &env.payer,
            holder: &cosign_admin,
        }
        .send(&[zolana_ring_client::SetCoSigner {
            ring,
            payer: env.payer.pubkey(),
            authority: cosign_admin.pubkey(),
            signer: ring_rpc.co_signer,
            scope: CoSignScope::APPROVE_OUTFLOW,
        }
        .instruction()?])?;
        StorageRecords::mint_policy(
            ring,
            Member::asset(&Address::new_from_array(
                custom_ring_interface::SOL_ASSET_ADDRESS,
            ))?,
            zolana_ring_policy::MintPolicy {
                cosign_above: COSIGN_ABOVE,
                limits: AssetLimits {
                    sender_cap: SENDER_CAP,
                    sender_window_slots: SENDER_WINDOW_SLOTS,
                    ..AssetLimits::EMPTY
                },
                ..Default::default()
            },
            &env,
        )
        .send()?;

        // 2. A wallet deposits to the program's PDA, the program then creates its counter.
        let keys = pda_keys(owner(&ring.program_id()).pda(), &env.recipient.keypair)?;
        let zero_key = NullifierKey::from_secret([0; 31]);
        let address = keys.address()?;
        let payer = &env.sender.keypair;
        let deposit = RingDeposit {
            ring,
            payer,
            recipient: DepositRecipient::Confidential(address),
            tree: env.tree,
            asset: DepositAsset::Sol,
            amount: HELD,
            cosigner: None,
            list_salt: None,
            tags: None,
        };
        let RingDepositReceipt { utxo, .. } = env.run(deposit.send(env.client.ring_client()))?;
        let token = pda_note(&env, utxo, None)?;
        let step = ProgramStep {
            ring,
            keys: &keys,
            zero_key: &zero_key,
            token: &token,
            state: None,
            recipient: payer.shielded_address()?,
            paid: 0,
            co_signer: ring_rpc.co_signer,
        };
        let created = step.prove(&env, payer.pubkey())?;
        ensure!(
            created.transfer.co_signers.is_empty(),
            "a create moves nothing out"
        );
        created.land(&env, ring, &ring_rpc, payer)?;
        let token = pda_output(&env, &created.transfer.outputs[CHANGE_SLOT], &address)?;
        let state = pda_output(&env, &created.transfer.outputs[STATE_SLOT], &address)?;

        // 3. Ring RPC approves the exact call of the update, the program forwards the frame.
        let updated = ProgramStep {
            token: &token,
            state: Some((&state, Counter { count: 0 })),
            paid: PAID,
            ..step
        }
        .prove(&env, payer.pubkey())?;
        ensure!(
            updated.transfer.co_signers == [ring_rpc.co_signer],
            "the update asks the co-signer"
        );
        let size = spr_client::transaction_size(
            &payer.pubkey(),
            &updated.instructions,
            ComputeBudgetConfig::new(compute_unit_limit(InstructionTag::Transfer)),
        )?;
        ensure!(
            size.fits(),
            "the approved update has {size:?} of {MAX_TRANSACTION_SIZE} bytes"
        );
        let signature = updated.land(&env, ring, &ring_rpc, payer)?;
        assert_transaction_compute_units(
            rpc,
            &signature,
            "Xark program update, approved CPI",
            u64::from(compute_unit_limit(InstructionTag::Transfer)),
        )?;
        let outputs = updated.transfer.outputs;
        Ok(Self {
            token: pda_output(&env, &outputs[CHANGE_SLOT], &address)?,
            state: pda_output(&env, &outputs[STATE_SLOT], &address)?,
            env,
            ring,
            auditor,
            co_signer,
            ring_rpc,
            keys,
            zero_key,
            signature,
            outputs,
        })
    }

    /// The second update, `paid` to the sender wallet.
    fn step(&self, paid: u64) -> Result<ProgramStep<'_>> {
        Ok(ProgramStep {
            ring: self.ring,
            keys: &self.keys,
            zero_key: &self.zero_key,
            token: &self.token,
            state: Some((&self.state, Counter { count: 1 })),
            recipient: self.env.sender.keypair.shielded_address()?,
            paid,
            co_signer: self.ring_rpc.co_signer,
        })
    }
}

#[test]
fn a_program_updates_its_ring_note_under_a_ring_rpc_approval() -> Result<()> {
    let updated = Updated::new()?;
    let env = &updated.env;
    let payer = &env.sender.keypair;
    let ask = CoSignAsk {
        env,
        ring: updated.ring,
        ring_rpc: &updated.ring_rpc,
        payer,
    };

    // 1. An approval covers the committed frame alone, on chain and at Ring RPC.
    let approved = updated.step(PAID / 2)?.prove(env, payer.pubkey())?;
    let other = updated.step(PAID / 2 - 1)?.prove(env, payer.pubkey())?;
    let (_, approvals) = approved
        .instructions
        .split_last()
        .context("the program's call")?;
    let swapped = ProvenStep {
        instructions: approvals
            .iter()
            .chain(other.instructions.last())
            .cloned()
            .collect(),
        transfer: approved.transfer,
    };
    let refusal = ask
        .co_sign_once_read(&swapped)
        .err()
        .context("a co-signature over another call")?;
    ensure!(
        refusal.to_string().contains(&format!(
            "{:#x}",
            CustomRingError::TransferApprovalOtherCall as u32
        )),
        "{refusal}"
    );
    let refused = |message: VersionedMessage| -> Result<spr_client::ClientError> {
        let transaction = sign_transaction(message, &[payer, &updated.co_signer])?;
        match env
            .client
            .rpc()
            .client()
            .send_and_confirm_transaction(&transaction)
        {
            Ok(signature) => bail!("the refused frame landed as {signature}"),
            Err(source) => Ok(spr_client::ClientError::SolanaRpcTransaction {
                operation: "send v1",
                source,
            }),
        }
    };
    Rejection::custom(CustomRingError::TransferApprovalOtherCall as u32)
        .at(u8::try_from(approvals.len())?)
        .assert_client(&refused(swapped.message(env, payer.pubkey())?)?);

    // 2. Ring RPC signs no outflow that takes the sender above its window cap.
    let above = updated.step(SENDER_CAP)?.prove(env, payer.pubkey())?;
    let refusal = ask
        .co_sign_once_read(&above)
        .err()
        .context("a co-signature above the cap")?;
    ensure!(
        refusal
            .to_string()
            .contains(&(CoSignError::OverLimit as i32).to_string()),
        "{refusal}"
    );

    // 3. The program signs for the PDA of the called ring alone, the approval covers the call as sent.
    let other_ring = owner(&Address::new_from_array([0x52; 32]));
    let own = *owner(&updated.ring.program_id()).pda();
    let ProvenStep {
        transfer,
        mut instructions,
    } = updated.step(PAID / 2)?.prove(env, payer.pubkey())?;
    let mut call = instructions.pop().context("the program's call")?;
    let slot = call
        .accounts
        .iter_mut()
        .find(|account| account.pubkey == own)
        .context("the owner slot")?;
    slot.pubkey = *other_ring.pda();
    let (ring_program, forwarded) = call.accounts.split_first().context("the ring program")?;
    let approval = TransferApproval {
        position: u8::try_from(instructions.len())?,
        digest: TransferApproval::digest(
            &ring_program.pubkey,
            &transfer.instruction()?.data,
            forwarded.iter().map(|account| &account.pubkey),
        )?,
    };
    for approve in &mut instructions {
        approve.data = approval.encode().into();
    }
    let position = approval.position;
    instructions.push(call);
    let foreign = ProvenStep {
        transfer,
        instructions,
    };
    Rejection::custom(CustomRingError::MissingOwnerSigner as u32)
        .at(position)
        .assert_client(&refused(foreign.message(env, payer.pubkey())?)?);
    Ok(())
}

#[test]
fn the_auditor_checks_the_updated_note_commitment_without_its_payload() -> Result<()> {
    let updated = Updated::new()?;
    let env = &updated.env;
    let audited = AuditLookup {
        scan: RingScan::new(updated.ring.program_id(), updated.auditor.pubkey())
            .with_mode(RingMode::Confidential),
        auditor: &updated.auditor,
        signature: updated.signature,
    }
    .run(env)?;
    let opened = audited
        .outputs
        .iter()
        .find(|output| usize::try_from(output.slot_index) == Ok(STATE_SLOT))
        .and_then(|output| output.opening.utxo())
        .context("the state slot opens")?;
    let next = data_hash(&DataHasher::hash(&Counter { count: 1 })?)?;
    ensure!(
        opened.data_hash == Some(next),
        "the record proves the counter"
    );
    // The commitment from the record's fields alone, no data bytes.
    let commitment = Utxo {
        owner: updated.keys.address()?.signing_pubkey,
        asset: env.assets.mint(&opened.mint.context("a recorded mint")?)?,
        amount: opened.amount,
        blinding: opened.blinding,
        ring_program_id: Some(updated.ring.program_id()),
        data: Data::default(),
    }
    .hash(
        &opened.owner.nullifier_pk().context("a keyed owner")?,
        &next,
        &opened.ring_data_hash.unwrap_or_default(),
        env.tree_id(env.tree)?,
    )?;
    ensure!(
        opened.owner_key == owner(&updated.ring.program_id()).pda().to_bytes(),
        "the record names the program's PDA"
    );
    ensure!(
        commitment == updated.outputs[STATE_SLOT].hash(env.tree_id(env.tree)?)?,
        "the record rebuilds the note's commitment"
    );
    Ok(())
}
