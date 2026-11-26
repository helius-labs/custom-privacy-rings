//! The program circuit, run natively, commits the transfer the ring SDK seals.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use custom_ring_interface::{NewRing, RingMode, RingRoles, RING_POLICY_PDA_SEED};
use solana_account::Account;
use solana_address::Address;
use spr_client::{
    AsyncProverClient, AsyncRpc, AsyncSprClient, AsyncWitnessReader, ClientError, IndexerRpcConfig,
    InputWitnesses,
};
use spr_hasher::primitives::hash_bytes;
use spr_interface::{
    shape::Shape,
    state::{discriminator::RING_CONFIG, RingConfig},
    RING_AUTH_PDA_SEED, SHIELDED_POOL_PROGRAM_ID,
};
use spr_keypair::{
    random_blinding, NullifierKey, ShieldedAddress, ShieldedKeypair, SigningKey, X25519Keypair,
};
use spr_program::{
    hasher::{data_hash, DataHasher},
    program::PdaOwner,
    proof::compressed_proof,
    to_bytes,
    verifier::VerifyError,
    CompressedProof, InputRecords, ProgramOwner, ProgramProofInputs,
};
use spr_transaction::{
    instructions::transact::{ConfidentialTransaction, FinalizedTransaction, SppProofOutputUtxo},
    utxo::SppProofInputUtxo,
    LocalShieldedKeys, Mint, WalletUtxo,
};
use zk_ring_program::{
    circuits::{
        Counter, Create, CreatePrivateInputs, OwnerPublicInputs, Update, UpdatePrivateInputs,
    },
    client::owner,
    verifier::{create, update},
};
use zolana_ring_client::{
    AnonymousPayment, ApplicationData, AsyncRingIndexer, CustomRing, CustomRingTransfer,
    CustomRingTransferInput, NoteData, ProgramCommitment, ProgramPlan, PublicOwnerTransfer,
    RingClient, RingError,
};
use zolana_ring_keys::RingAddress;

const TREE_ID: u16 = 3;
const HELD: u64 = 10;
const SPL: Mint = Mint::new(Address::new_from_array([9; 32]), 2);
/// A slot past the three the update circuit emits, the spare one stays padding.
const SHAPE: Shape = Shape::new(2, 4);
const RING_PROGRAM: Address = Address::new_from_array([42; 32]);
const PAYER: Address = Address::new_from_array([7; 32]);

/// The two accounts a plan and a staging read.
#[derive(Clone)]
struct Chain(HashMap<Address, Account>);

impl Chain {
    fn ring(mode: RingMode) -> Self {
        let ring = CustomRing::new(RING_PROGRAM);
        let policy = NewRing {
            bump: Address::find_program_address(&[RING_POLICY_PDA_SEED], &RING_PROGRAM).1,
            mode,
            roles: RingRoles {
                admin: Address::new_from_array([0xa1; 32]),
                policy: Address::new_from_array([0xa2; 32]),
                pauser: Address::new_from_array([0xa3; 32]),
                cosign_admin: Address::new_from_array([0xa4; 32]),
                enrollment_writer: Address::new_from_array([0xa5; 32]),
            },
            auditor_pubkey: *X25519Keypair::from_bytes(&[5; 32]).pubkey().as_bytes(),
            created_slot: 0,
            records: Address::default(),
            tracking: Address::new_from_array([0x5b; 32]),
        }
        .account();
        let registration = RingConfig {
            discriminator: RING_CONFIG,
            authority: Address::new_from_array([0xa1; 32]),
            program_id: RING_PROGRAM,
            ring_authority_transact_is_enabled: 1,
            paused: 0,
            activated: 1,
            bump: Address::find_program_address(&[RING_AUTH_PDA_SEED], &RING_PROGRAM).1,
        };
        let account = |owner: Address, data: &[u8]| Account {
            lamports: 1,
            data: data.to_vec(),
            owner,
            ..Account::default()
        };
        Self(HashMap::from([
            (
                ring.policy_pda(),
                account(RING_PROGRAM, bytemuck::bytes_of(&*policy)),
            ),
            (
                ring.ring_auth_pda(),
                account(
                    Address::new_from_array(SHIELDED_POOL_PROGRAM_ID),
                    bytemuck::bytes_of(&registration),
                ),
            ),
        ]))
    }

    fn client(self) -> RingClient<Chain, Chain> {
        RingClient::new(
            AsyncSprClient::new(self.clone(), self, AsyncProverClient::local()),
            AsyncProverClient::local(),
        )
    }
}

#[async_trait]
impl AsyncRpc for Chain {
    async fn get_account(&self, address: Address) -> Result<Option<Account>, ClientError> {
        Ok(self.0.get(&address).cloned())
    }
}

#[async_trait]
impl AsyncRingIndexer for Chain {}

impl AsyncWitnessReader for Chain {
    fn input_witnesses(
        &self,
        _inputs: &[&SppProofInputUtxo],
        _dummy_nullifiers: &[[u8; 32]],
        _config: Option<IndexerRpcConfig>,
    ) -> impl std::future::Future<Output = Result<InputWitnesses, ClientError>> + Send {
        std::future::ready(Err(ClientError::NoInputs))
    }
}

/// The program's PDA beside the viewing key its notes seal to.
struct Program {
    owner: ProgramOwner,
    address: ShieldedAddress,
    keys: LocalShieldedKeys,
    nullifier_key: NullifierKey,
}

impl Program {
    fn new() -> Self {
        let viewer = keypair(1);
        let owner = owner(&RING_PROGRAM);
        let address = owner.address(viewer.viewing_pubkey()).unwrap();
        let keys = LocalShieldedKeys::new(
            address,
            vec![viewer.viewing_key.clone()],
            ProgramOwner::nullifier_key(),
        )
        .unwrap();
        Self {
            owner,
            address,
            keys,
            nullifier_key: ProgramOwner::nullifier_key(),
        }
    }

    fn note(&self, asset: Mint, output: SppProofOutputUtxo, leaf_index: u64) -> WalletUtxo {
        let output = SppProofOutputUtxo {
            owner_address: Some(self.address),
            asset,
            blinding: random_blinding(),
            ring_program_id: Some(RING_PROGRAM),
            ..output
        };
        let mut note = self.owner.input(&output, TREE_ID, leaf_index).unwrap();
        note.latest_tree_id = Some(TREE_ID);
        note
    }
}

fn keypair(seed: u8) -> ShieldedKeypair {
    ShieldedKeypair::from_keypair(SigningKey::from_ed25519_bytes(&[seed; 32])).unwrap()
}

fn state_output(counter: &Counter) -> SppProofOutputUtxo {
    SppProofOutputUtxo::default().with_utxo_data(
        borsh::to_vec(counter).unwrap(),
        data_hash(&DataHasher::hash(counter).unwrap()).unwrap(),
    )
}

#[derive(Clone, Copy, Debug)]
enum Step {
    /// The first state note beside the held note, unchanged.
    Create,
    /// The counter one step on and `paid` of the held note to the recipient.
    Update { paid: u64 },
}

impl Step {
    const fn paid(self) -> u64 {
        match self {
            Self::Create => 0,
            Self::Update { paid } => paid,
        }
    }

    const fn artifact(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update { .. } => "update",
        }
    }

    /// The public inputs as `process_instruction` derives them.
    fn verify(
        self,
        proof: &CompressedProof,
        owner: &ProgramOwner,
        private_tx_hash: &[u8; 32],
    ) -> Result<(), VerifyError> {
        let owner = *PdaOwner::new(owner.pda()).unwrap().owner_hash();
        let ring = hash_bytes(RING_PROGRAM.as_array()).unwrap();
        match self {
            Self::Create => create::Verifier {
                proof,
                public_inputs: create::PublicInputs { owner, ring },
                private_tx_hash,
                transfers: &[],
            }
            .verify(),
            Self::Update { .. } => update::Verifier {
                proof,
                public_inputs: update::PublicInputs { owner, ring },
                private_tx_hash,
                transfers: &[],
            }
            .verify(),
        }
    }
}

struct Case {
    /// A signer seal adds no zero change, there the client declares the one the circuit outputs.
    change_declared: bool,
    /// The circuit outputs the state ahead of the payment.
    payment_first: bool,
    program: Program,
    recipient: ShieldedAddress,
    mint: Mint,
    state: WalletUtxo,
    token: WalletUtxo,
    step: Step,
}

impl Case {
    fn new(mode: RingMode, mint: Mint, step: Step) -> Self {
        let program = Program::new();
        Self {
            change_declared: mode == RingMode::Confidential && step.paid() == HELD,
            payment_first: false,
            recipient: keypair(2).shielded_address().unwrap(),
            mint,
            state: program.note(mint, state_output(&Counter { count: 4 }), 0),
            token: program.note(
                mint,
                SppProofOutputUtxo {
                    amount: HELD,
                    ..Default::default()
                },
                1,
            ),
            program,
            step,
        }
    }

    fn pay(
        &self,
        to: ShieldedAddress,
        amount: u64,
        output: SppProofOutputUtxo,
    ) -> AnonymousPayment {
        AnonymousPayment {
            to: RingAddress {
                recipient: to,
                ring_program_id: RING_PROGRAM,
                list_salt: [7; 32],
            },
            asset: self.mint,
            amount,
            reply: None,
            data: NoteData {
                ring_data_hash: None,
                application: output.data_hash.map(|hash| ApplicationData {
                    hash,
                    bytes: output.data.utxo_data().unwrap().to_vec(),
                }),
            },
        }
    }

    /// The ring transfer of the step, the SDK leads with the change on both rails.
    fn transfer(&self) -> PublicOwnerTransfer<'_> {
        let (inputs, count) = match self.step {
            Step::Create => (vec![self.token.clone()], 0),
            Step::Update { .. } => (vec![self.token.clone(), self.state.clone()], 5),
        };
        let mut transaction =
            ConfidentialTransaction::new_with_ring(inputs, PAYER, RING_PROGRAM).unwrap();
        if self.change_declared {
            transaction
                .add_output_utxo(SppProofOutputUtxo {
                    owner_address: Some(self.program.address),
                    asset: self.mint,
                    amount: HELD - self.step.paid(),
                    ring_program_id: Some(RING_PROGRAM),
                    ..Default::default()
                })
                .unwrap();
        }
        let state = self.pay(self.program.address, 0, state_output(&Counter { count }));
        let paid = match self.step.paid() {
            0 => None,
            paid => Some(self.pay(self.recipient, paid, SppProofOutputUtxo::default())),
        };
        let payments: Vec<_> = match self.payment_first {
            true => paid.into_iter().chain([state]).collect(),
            false => [state].into_iter().chain(paid).collect(),
        };
        payments.into_iter().fold(
            CustomRingTransfer::new(CustomRingTransferInput {
                ring: CustomRing::new(RING_PROGRAM),
                sender: &self.program.keys,
                nullifier_key: Some(&self.program.nullifier_key),
                transaction,
            })
            .spend_public(*self.program.owner.pda())
            .with_shape(SHAPE)
            .unwrap(),
            |transfer, payment| transfer.with_payment(payment).unwrap(),
        )
    }

    /// The program's own run over the plan's seed and output tree.
    fn commit(&self, seed: [u8; 32], plan: &ProgramPlan) -> Committed {
        let mut records = InputRecords::new();
        let tx_context = InputRecords::context(&seed, Some(plan.output_tree_id)).unwrap();
        let tokens = records.utxos(std::slice::from_ref(&self.token)).unwrap();
        let public = OwnerPublicInputs {
            owner: *records.address(&self.program.address).unwrap().owner(),
            ring: records.ring(RING_PROGRAM).unwrap(),
        };
        let (checked, proof_inputs) = match self.step {
            Step::Create => {
                let create = Create {
                    private: CreatePrivateInputs { tx_context, tokens },
                    public,
                };
                let checked = spr_program::checked(|| create.execute()).unwrap();
                let proof_inputs = ProgramProofInputs::new(&create, *checked.public_hash());
                (checked, proof_inputs)
            }
            Step::Update { paid } => {
                let update = Update {
                    private: UpdatePrivateInputs {
                        tx_context,
                        state: records.utxo(&self.state).unwrap(),
                        counter: Counter { count: 4 },
                        tokens,
                        recipient: records.address(&self.recipient).unwrap(),
                        amount: paid,
                    },
                    public,
                };
                let checked = spr_program::checked(|| update.execute()).unwrap();
                let proof_inputs = ProgramProofInputs::new(&update, *checked.public_hash());
                (checked, proof_inputs)
            }
        };
        let mut transaction = records
            .transaction(&checked, &self.program.address, PAYER)
            .unwrap();
        transaction
            .pad_utxos_with_empty_outputs(plan.shape, &self.program.address)
            .unwrap();
        let finalized = transaction.finalize(&self.program.address).unwrap();
        records.check_commitment(&checked, &finalized).unwrap();
        Committed {
            commitment: ProgramCommitment {
                salt: plan.salt,
                private_tx_hash: to_bytes(checked.private_tx_hash()),
                transaction_hash: to_bytes(checked.transaction_hash()),
            },
            finalized,
            proof_inputs,
        }
    }
}

struct Committed {
    commitment: ProgramCommitment,
    finalized: FinalizedTransaction,
    proof_inputs: ProgramProofInputs,
}

fn slot_hashes(outputs: &[SppProofOutputUtxo], tree_id: u16) -> Vec<[u8; 32]> {
    outputs
        .iter()
        .filter(|output| !output.is_dummy())
        .map(|output| output.hash(tree_id).unwrap())
        .collect()
}

const MODES: [RingMode; 2] = [RingMode::Confidential, RingMode::Anonymous];

#[tokio::test]
async fn the_program_circuits_commit_the_sealed_ring_transfer_on_both_rails() {
    let steps = [
        Step::Create,
        Step::Update { paid: 4 },
        Step::Update { paid: HELD },
        Step::Update { paid: 0 },
    ];
    for mode in MODES {
        for mint in [Mint::SOL, SPL] {
            // A declared change is the frame's change, the spare slot of `SHAPE` takes no second one.
            for (step, declared) in steps
                .into_iter()
                .flat_map(|step| [(step, false), (step, true)])
            {
                let seals_alone = Case::new(mode, mint, step);
                let case = Case {
                    change_declared: declared || seals_alone.change_declared,
                    ..seals_alone
                };
                let client = Chain::ring(mode).client();
                let plan = case.transfer().plan_for_program(&client).await.unwrap();
                let Committed {
                    commitment,
                    finalized,
                    ..
                } = case.commit(plan.blinding_seed, &plan);
                let label = format!("{mode:?} {} {step:?} declared {declared}", mint.asset);

                assert_eq!(
                    finalized.first_nullifier().unwrap(),
                    plan.first_nullifier,
                    "{label}"
                );
                assert_eq!(
                    slot_hashes(finalized.output_utxos(), finalized.output_tree_id()),
                    slot_hashes(&plan.outputs, plan.output_tree_id),
                    "{label}"
                );
                assert_eq!(commitment.private_tx_hash, plan.private_tx_hash, "{label}");
                assert_eq!(
                    commitment.transaction_hash, plan.transaction_hash,
                    "{label}"
                );
                assert!(plan.check(&commitment).is_ok(), "{label}");
            }
        }
    }
}

#[tokio::test]
async fn a_commitment_under_another_seed_or_payment_order_is_refused() {
    let refused = |plan: &ProgramPlan, commitment: ProgramCommitment| {
        matches!(
            plan.check(&commitment),
            Err(RingError::ProgramCommitmentMismatch { committed, staged })
                if committed == commitment.private_tx_hash && staged == plan.private_tx_hash
        )
    };
    for mode in MODES {
        for step in [Step::Create, Step::Update { paid: 4 }] {
            let case = Case::new(mode, Mint::SOL, step);
            let client = Chain::ring(mode).client();
            let plan = case.transfer().plan_for_program(&client).await.unwrap();
            let commitment = case.commit(random_blinding(), &plan).commitment;
            assert!(refused(&plan, commitment), "{mode:?} {step:?}");
        }
    }

    let amounts = |outputs: &[SppProofOutputUtxo]| -> Vec<u64> {
        outputs.iter().map(|output| output.amount).collect()
    };
    for mode in MODES {
        let swapped = Case {
            payment_first: true,
            ..Case::new(mode, Mint::SOL, Step::Update { paid: 4 })
        };
        let client = Chain::ring(mode).client();
        let plan = swapped.transfer().plan_for_program(&client).await.unwrap();
        assert_eq!(amounts(&plan.outputs), [HELD - 4, 4, 0, 0], "{mode:?}");
        let Committed {
            commitment,
            finalized,
            ..
        } = swapped.commit(plan.blinding_seed, &plan);
        assert_eq!(
            amounts(finalized.output_utxos()),
            [HELD - 4, 0, 4, 0],
            "{mode:?}"
        );
        assert!(refused(&plan, commitment), "{mode:?}");
    }
}

/// Each proof under its own test key, the hash from the sealed ring frame.
#[tokio::test]
async fn the_program_proofs_verify_against_the_sealed_frame_hash() {
    let artifacts = std::env::var_os("ZOLANA_ZK_ARTIFACTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/zk"))
        .join("zk-ring-program");
    for mode in MODES {
        for step in [Step::Create, Step::Update { paid: 4 }] {
            let case = Case::new(mode, Mint::SOL, step);
            let client = Chain::ring(mode).client();
            let plan = case.transfer().plan_for_program(&client).await.unwrap();
            let proof_inputs = case.commit(plan.blinding_seed, &plan).proof_inputs;
            let proven = xark::prover::circuit_at(artifacts.join(step.artifact()))
                .prove(proof_inputs)
                .unwrap();
            let proof = compressed_proof(&proven.proof).unwrap();
            let verify = |hash: &[u8; 32]| step.verify(&proof, &case.program.owner, hash);
            assert_eq!(verify(&plan.private_tx_hash), Ok(()), "{mode:?} {step:?}");
            assert_eq!(
                verify(&[1; 32]),
                Err(VerifyError::Proof),
                "{mode:?} {step:?}"
            );
        }
    }
}
