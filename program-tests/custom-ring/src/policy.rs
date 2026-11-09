//! Member list writes and governed transfers shared by the policy suites, and
//! the tables they pin.

use anyhow::{anyhow, ensure, Result};
use std::{
    thread::sleep,
    time::{Duration, Instant},
};

use custom_ring_cli::{wait_for, Probe, Zolana};
use custom_ring_interface::{
    mint_policy_write,
    storage::{Revision, StorageRegistry},
    storage_instruction::{EnrollIxData, StorageUpdateIxData, ENROLLMENT_BATCH},
    tracking::TrackingRegistry,
    RecordWriteData, RingPolicy, WriteRecordsIxData,
};
use custom_ring_sdk::{
    CustomRing, CustomRingTransfer, CustomRingTransferInput, DepositAsset, DepositRecipient,
    Enroll, EnrollBatch, EnrollmentRead, InitializeStorage, PrivateStorageWrite,
    ProvenRingTransfer, RingDeposit, RingDepositReceipt, RingError, SaltedOwnerWrite,
    StorageApplyProof, UpdateStorage,
};
use solana_address::Address;
use solana_keypair::Keypair;
use solana_signature::Signature;
use solana_signer::Signer;
use spr_client::{prover::indexed::ProofDataSource, AsyncProverClient, Rpc};
use spr_keypair::{ShieldedKeypair, X25519Keypair};
use spr_test_utils::test_validator_asserts::wait_for_merkle_proof;
use spr_transaction::{instructions::transact::ConfidentialTransaction, Utxo};
use zolana_ring_client::{
    records::{owner_record_key, storage_apply_proof_inputs},
    rpc::SubmitEnrollmentRequest,
    RegisteredKeys, RingIndexer, TrackingLeaf,
};
use zolana_ring_indexer::rpc::{GetStorageApplyProofInputsRequest, StorageWriteView};
use zolana_ring_keys::ListSaltHolder;
use zolana_ring_policy::{
    ListChange, ListId, ListSet, Member, MintPolicy, Rule, RuleTable, Subject,
};

use crate::shared::{
    custom_ring_program_id, send, setup_with_extra_rings, RegisterRing, TestEnv, Tier,
};

pub const DEPOSIT: u64 = 1_000_000_000;
pub const TRANSFER_AMOUNT: u64 = 250_000_000;

pub const EMPTY: RuleTable = RuleTable::empty();

pub const RELEASED: RuleTable = RuleTable::builder()
    .rule(Rule::require(Subject::OutputOwner, ListId::Allow))
    .rule(Rule::require(Subject::Sender, ListId::Allow))
    .rule(Rule::forbid(Subject::OutputOwner, ListId::Block))
    .rule(Rule::forbid(Subject::Sender, ListId::Frozen))
    .build();

pub const BLOCK_ONLY: RuleTable = RuleTable::builder()
    .rule(Rule::forbid(Subject::OutputOwner, ListId::Block))
    .build();

pub const LIMIT_CAP: u64 = 650_000_000;
pub const LIMIT_COSIGN_ABOVE: u64 = 300_000_000;

/// An output owner needs an Approval entry and no Block entry.
pub const APPROVED_AND_UNBLOCKED: RuleTable = RuleTable::builder()
    .rule(Rule::require(Subject::OutputOwner, ListId::Approval))
    .rule(Rule::forbid(Subject::OutputOwner, ListId::Block))
    .build();

/// The owner's public list record, the key every SDK reader names.
pub fn owner_member(owner: &ShieldedKeypair) -> Result<Member> {
    Ok(owner_record_key(&owner_tag(owner)?)?)
}

fn owner_tag(owner: &ShieldedKeypair) -> Result<[u8; 32]> {
    Ok(owner.signing_pubkey().confidential_view_tag()?)
}

pub fn policy_registry(ring: CustomRing, env: &TestEnv) -> Result<Box<RingPolicy>> {
    Ok(env.run(ring.require_policy(env.client.as_async()))?)
}

pub struct CuratedRings {
    pub env: TestEnv,
    pub curator: CustomRing,
    pub subscriber: CustomRing,
}

impl CuratedRings {
    /// The curator pins and registers under the payer, the subscriber stays unconfigured.
    pub fn setup() -> Result<Self> {
        let curator_program = Keypair::new().pubkey();
        let env = setup_with_extra_rings(&[curator_program])?;
        let curator = CustomRing::new(curator_program);
        RegisterRing {
            ring: curator,
            payer: &env.payer,
            auditor_pubkey: X25519Keypair::new().pubkey(),
            tier: Tier::policy(&BLOCK_ONLY),
        }
        .send(&env.client)?;
        Ok(Self {
            env,
            curator,
            subscriber: CustomRing::new(custom_ring_program_id()?),
        })
    }
}

/// Ring SOL notes of `owner`, each provable through the indexer on return.
pub struct RingNotes<'a> {
    pub ring: CustomRing,
    pub owner: &'a ShieldedKeypair,
    pub amount: u64,
    pub asset: DepositAsset,
    pub count: usize,
    pub env: &'a TestEnv,
}

impl RingNotes<'_> {
    pub fn deposit(self) -> Result<Vec<Utxo>> {
        let payer = self.owner;
        self.deposit_from(payer)
    }

    /// A separate depositor keeps an anonymous owner out of public signatures.
    pub fn deposit_from(self, payer: &(dyn Signer + Sync)) -> Result<Vec<Utxo>> {
        let recipient = DepositRecipient::Confidential(self.owner.shielded_address()?);
        self.deposit_to(recipient, payer)
    }

    /// Notes of the owner's hidden identity on an anonymous ring, each a payment its sync finds.
    pub fn deposit_hidden_from(self, payer: &(dyn Signer + Sync)) -> Result<Vec<Utxo>> {
        let recipient = DepositRecipient::Anonymous { keys: self.owner };
        self.deposit_to(recipient, payer)
    }

    fn deposit_to(
        self,
        recipient: DepositRecipient<'_>,
        payer: &(dyn Signer + Sync),
    ) -> Result<Vec<Utxo>> {
        (0..self.count)
            .map(|_| self.land(&self.deposit_with(recipient, payer)))
            .collect()
    }

    fn deposit_with<'d>(
        &self,
        recipient: DepositRecipient<'d>,
        payer: &'d (dyn Signer + Sync),
    ) -> RingDeposit<'d> {
        RingDeposit {
            ring: self.ring,
            payer,
            recipient,
            tree: self.env.tree,
            asset: self.asset,
            amount: self.amount,
            cosigner: None,
            // The salt the owner enrolled under, a salted list refuses an enrolled recipient with none.
            list_salt: self.owner.list_salt(&self.ring.program_id()).ok(),
        }
    }

    fn land(&self, deposit: &RingDeposit<'_>) -> Result<Utxo> {
        let env = self.env;
        let RingDepositReceipt { utxo, .. } = env.run(deposit.send(env.client.ring_client()))?;
        let tree_id = env.tree_id(env.tree)?;
        let leaf = utxo.hash(
            &self.owner.nullifier_key.pubkey()?,
            &[0; 32],
            &[0; 32],
            tree_id,
        )?;
        wait_for_merkle_proof(env.client.indexer(), env.tree, leaf);
        Ok(utxo)
    }
}

/// One 1-in 2-out ring transfer of `amount` to the recipient.
pub struct PolicyTransfer<'a> {
    pub ring: CustomRing,
    pub sender: &'a ShieldedKeypair,
    pub recipient: &'a ShieldedKeypair,
    pub note: Utxo,
    pub amount: u64,
    pub env: &'a TestEnv,
}

impl PolicyTransfer<'_> {
    pub fn prove(self, client: &Zolana) -> Result<ProvenRingTransfer, RingError> {
        let tree_id = self
            .env
            .run(custom_ring_sdk::tree_id(client.as_async(), self.env.tree))?;
        let hash = self.note.hash(
            &self.sender.nullifier_key.pubkey()?,
            &[0; 32],
            &[0; 32],
            tree_id,
        )?;
        let state = wait_for_merkle_proof(self.env.client.indexer(), self.env.tree, hash);
        let note = spr_test_utils::utxo::wallet(
            self.note,
            &self.sender.nullifier_key,
            tree_id,
            state.leaf_index,
            None,
            None,
        )
        .map_err(|error| spr_client::ClientError::Rpc(error.to_string()))?;
        let mut transfer = ConfidentialTransaction::new_with_ring(
            vec![note],
            self.sender.pubkey(),
            self.ring.program_id(),
        )?
        .with_output_tree_id(tree_id)?;
        transfer.transfer_sol(&self.recipient.shielded_address()?, self.amount)?;
        let transfer = CustomRingTransfer::new(CustomRingTransferInput {
            ring: self.ring,
            sender: self.sender,
            nullifier_key: Some(&self.sender.nullifier_key),
            transaction: transfer,
        });
        client.block_on(transfer.prove(client.ring_client()))
    }

    /// Refused before proving, the unreachable prover is never asked.
    pub fn expect_refusal(self) -> Result<()> {
        let unreachable = self.env.client_with(|| {
            AsyncProverClient::new("http://127.0.0.1:1".to_string())
                .with_proof_data_source(ProofDataSource::Client)
        });
        match self.prove(&unreachable) {
            Err(RingError::PolicyRuleUnsatisfied(_)) => Ok(()),
            Err(other) => Err(anyhow!("expected PolicyRuleUnsatisfied, got {other}")),
            Ok(_) => Err(anyhow!(
                "expected PolicyRuleUnsatisfied, the transfer was proven"
            )),
        }
    }

    /// Returns once photon serves the first output leaf, the next proof sees a synced index.
    pub fn land(self) -> Result<Signature> {
        let env = self.env;
        let payer = self.sender;
        let proven = self
            .prove(&env.client)
            .map_err(|error| anyhow!("transfer proof failed {error}"))?;
        let landed = proven
            .outputs
            .first()
            .ok_or_else(|| anyhow!("transfer output"))?
            .hash(env.tree_id(env.tree)?)?;
        let signature = send(env.client.rpc(), payer, &[proven.instruction()?])?;
        wait_for_merkle_proof(env.client.indexer(), env.tree, landed);
        Ok(signature)
    }
}

const INDEXING_TIMEOUT: Duration = Duration::from_secs(90);

/// Polls the indexer until `member`'s record in `ring` carries `list_id` as `present`.
pub fn wait_for_list(
    env: &TestEnv,
    ring: CustomRing,
    list_id: ListId,
    member: Member,
    present: bool,
) -> Result<()> {
    let deadline = Instant::now() + INDEXING_TIMEOUT;
    loop {
        match env.member_record(ring, member) {
            Ok(read) if read.listed(list_id) == present => return Ok(()),
            _ if Instant::now() > deadline => {
                return Err(anyhow!(
                    "{list_id:?} of {member:?} not indexed as {present}"
                ))
            }
            _ => sleep(Duration::from_millis(500)),
        }
    }
}

/// One authority list write, readable through the indexer on return.
pub struct RegistryWrite<'a> {
    pub ring: CustomRing,
    pub list_id: ListId,
    pub member: Member,
    pub present: bool,
    pub env: &'a TestEnv,
}

impl RegistryWrite<'_> {
    pub fn send(self) -> Result<()> {
        let env = self.env;
        let list = ListSet::single(self.list_id);
        let lists = match self.present {
            true => ListChange {
                set: list,
                clear: ListSet::EMPTY,
            },
            false => ListChange {
                set: ListSet::EMPTY,
                clear: list,
            },
        }
        .check()?;
        let write = RecordWriteData {
            member: *self.member.as_bytes(),
            lists: WriteRecordsIxData::new(lists),
            mint_policy: mint_policy_write(None),
        };
        StorageRecords {
            ring: self.ring,
            writes: vec![write],
            env,
        }
        .send()?;
        wait_for_list(env, self.ring, self.list_id, self.member, self.present)
    }
}

/// The owner's fields once the queue applies, its queued part unverified.
pub fn owner_enrollment(
    env: &TestEnv,
    ring: CustomRing,
    owner: Address,
) -> Result<Option<EnrollIxData>> {
    let tracking = env
        .run(ring.require_policy(env.client.as_async()))?
        .tracking;
    let enrolled = env.run(
        EnrollmentRead {
            tracking_registry: tracking,
            owner,
        }
        .read(env.client.as_async()),
    )?;
    Ok(enrolled.map(|enrolled| enrolled.ix))
}

/// The owner's leaf under the registry's newest root, its key and seal checked.
pub fn owner_leaf(env: &TestEnv, ring: CustomRing, owner: Address) -> Result<Option<TrackingLeaf>> {
    Ok(env.run(read_leaf(env, ring, owner))?)
}

async fn read_leaf(
    env: &TestEnv,
    ring: CustomRing,
    owner: Address,
) -> Result<Option<TrackingLeaf>, RingError> {
    let chain = env.client.as_async();
    let tracking = ring.require_policy(chain).await?.tracking;
    let owner = owner.to_bytes();
    let mut leaves = RegisteredKeys {
        registry: tracking,
        owners: &[owner],
    }
    .leaves(chain, chain)
    .await?;
    Ok(leaves.remove(&owner))
}

/// The slot the oldest kept root expires at, `None` while the history has room for a batch.
fn retained_root(env: &TestEnv, registry: Address) -> Result<Option<u64>> {
    let account =
        spr_test_utils::test_validator_asserts::fetch_account(env.client.rpc(), &registry)?;
    let tracking = TrackingRegistry::from_data(&account.data)
        .ok_or_else(|| anyhow!("{registry} holds no tracking registry"))?;
    let expiry = tracking.roots.last().map_or(0, |root| root.expires_at());
    Ok((env.client.rpc().get_slot()? < expiry).then_some(expiry))
}

/// Refuses a validator whose slot stops advancing.
fn wait_for_slot(env: &TestEnv, target: u64) -> Result<()> {
    let mut last = env.client.rpc().get_slot()?;
    let mut moved = Instant::now();
    while last < target {
        sleep(Duration::from_millis(500));
        let slot = env.client.rpc().get_slot()?;
        if slot > last {
            (last, moved) = (slot, Instant::now());
        } else if moved.elapsed() > INDEXING_TIMEOUT {
            return Err(anyhow!("the slot stalled at {last} before {target}"));
        }
    }
    Ok(())
}

/// Applied on return, each owner signs its write and the payer lands them as the ring's enrollment writer.
pub struct EnrollOwners<'a> {
    pub ring: CustomRing,
    pub owners: &'a [&'a ShieldedKeypair],
    pub env: &'a TestEnv,
}

impl EnrollOwners<'_> {
    pub fn send(self) -> Result<()> {
        let env = self.env;
        let registry = env
            .run(self.ring.require_policy(env.client.as_async()))?
            .tracking;
        let writes = self.signed_writes()?;
        for batch in writes.chunks(ENROLLMENT_BATCH) {
            // A batch evicting an unexpired root refuses, the writer waits out that root's grace.
            if let Some(expiry) = retained_root(env, registry)? {
                println!("RING_ROOT_RETAINED until slot {expiry}");
                wait_for_slot(env, expiry)?;
            }
            let proven = wait_for("the ring indexer to prove the batch", || {
                let proven = env.run(
                    EnrollBatch {
                        ring: self.ring,
                        writer: env.payer.pubkey(),
                        tracking_registry: registry,
                        writes: batch,
                    }
                    .prove(env.client.ring_client()),
                );
                match proven {
                    Ok(proven) => Ok(Probe::Ready(proven)),
                    Err(error) if error.is_projection_lag() => Ok(Probe::Retry(error.into())),
                    Err(error) => Err(error.into()),
                }
            })?;
            send(env.client.rpc(), &env.payer, &proven.instructions)?;
        }
        // The indexer serves every leaf at the owner's viewing key on return, a lagging read answers later.
        wait_for("the applied enrollments", || {
            for owner in self.owners {
                let current = match env.run(read_leaf(env, self.ring, owner.pubkey())) {
                    Ok(leaf) => {
                        leaf.is_some_and(|leaf| leaf.viewing_u == owner.viewing_pubkey().u())
                    }
                    Err(error) if error.is_projection_lag() => {
                        return Ok(Probe::Retry(error.into()))
                    }
                    Err(error) => return Err(error.into()),
                };
                if !current {
                    return Ok(Probe::NotYet);
                }
            }
            Ok(Probe::Ready(()))
        })
    }

    /// One batch of every owner, a split batch would select a narrower apply key.
    pub fn send_in_one_batch(self) -> Result<()> {
        ensure!(
            self.owners.len() <= ENROLLMENT_BATCH,
            "{} owners exceed one enrollment batch",
            self.owners.len()
        );
        self.send()
    }

    /// `Enroll::prove` reads each leaf, an enrolled owner gets a rotation with no proof.
    fn signed_writes(&self) -> Result<Vec<SubmitEnrollmentRequest>> {
        let env = self.env;
        self.owners
            .iter()
            .map(|owner| {
                let list_salt = owner.list_salt(&self.ring.program_id())?;
                let enrollment = wait_for("the ring indexer to serve the owner's leaf", || {
                    let proven = env.run(
                        Enroll {
                            ring: self.ring,
                            owner: owner.pubkey(),
                            nullifier_key: &owner.nullifier_key,
                            viewing_pubkey: owner.viewing_pubkey(),
                            list_salt,
                        }
                        .prove(env.client.ring_client()),
                    );
                    match proven {
                        Ok(proven) => Ok(Probe::Ready(proven)),
                        Err(error) if error.is_projection_lag() => Ok(Probe::Retry(error.into())),
                        Err(error) => Err(error.into()),
                    }
                })?;
                Ok(enrollment.sign(*owner)?)
            })
            .collect()
    }
}

/// A keyed note needs its owner's tracking row on every ring.
pub fn enroll_actors(env: &TestEnv, ring: CustomRing) -> Result<()> {
    EnrollOwners {
        ring,
        owners: &[&env.sender.keypair, &env.recipient.keypair],
        env,
    }
    .send()
}

/// Sets `PrivateBlock` on each enrolled owner's list key, every write private.
pub struct PrivateBlockWrites<'a> {
    pub ring: CustomRing,
    pub owners: &'a [&'a ShieldedKeypair],
    pub env: &'a TestEnv,
}

impl PrivateBlockWrites<'_> {
    pub fn send(self) -> Result<Signature> {
        let Self { ring, owners, env } = self;
        let program = ring.program_id();
        let mut writes = Vec::with_capacity(owners.len());
        let mut salted = Vec::with_capacity(owners.len());
        for (index, owner) in owners.iter().enumerate() {
            let list_salt = owner.list_salt(&program)?;
            writes.push(RecordWriteData {
                member: *Member::salted(&Member::owner_tag(&owner_tag(owner)?)?, &list_salt)?
                    .as_bytes(),
                lists: WriteRecordsIxData::new(
                    ListChange {
                        set: ListSet::single(ListId::PrivateBlock),
                        clear: ListSet::EMPTY,
                    }
                    .check()?,
                ),
                mint_policy: mint_policy_write(None),
            });
            salted.push(SaltedOwnerWrite {
                write_index: u8::try_from(index)?,
                owner: owner.pubkey(),
                list_salt,
            });
        }
        StorageRecords { ring, writes, env }.send_with_owners(&salted)
    }
}

/// Needs the payer as storage authority of the ring's records.
pub struct StorageRecords<'a> {
    pub ring: CustomRing,
    pub writes: Vec<RecordWriteData>,
    pub env: &'a TestEnv,
}

impl StorageRecords<'_> {
    /// Replaces every mint setting of `member`.
    pub fn mint_policy(
        ring: CustomRing,
        member: Member,
        policy: MintPolicy,
        env: &TestEnv,
    ) -> StorageRecords<'_> {
        StorageRecords {
            ring,
            writes: vec![RecordWriteData {
                member: *member.as_bytes(),
                mint_policy: mint_policy_write(Some(policy)),
                ..RecordWriteData::default()
            }],
            env,
        }
    }

    pub fn send(self) -> Result<Signature> {
        self.send_with_owners(&[])
    }

    /// Salted owners make the batch private, its proof opens each owner's tracking row.
    fn send_with_owners(self, owners: &[SaltedOwnerWrite]) -> Result<Signature> {
        let env = self.env;
        let authority = env.payer.pubkey();
        let registry = InitializeStorage::records(self.ring, authority).registry();
        let writes = self
            .writes
            .iter()
            .map(|write| Ok(StorageWriteView::from(&write.storage_entry()?)))
            .collect::<Result<Vec<_>>>()?;
        let request = GetStorageApplyProofInputsRequest {
            registry: registry.into(),
            writes,
        };
        // The indexer can serve inputs at a root the chain has already moved past.
        let (response, inputs, state) = wait_for("the storage apply proof inputs", || {
            let response = match env
                .client
                .indexer()
                .get_storage_apply_proof_inputs(request.clone())
            {
                Ok(response) => response,
                Err(error) => {
                    let error = RingError::from(error);
                    return match error.is_projection_lag() {
                        true => Ok(Probe::Retry(error.into())),
                        false => Err(error.into()),
                    };
                }
            };
            let inputs = storage_apply_proof_inputs(&response)?;
            let account =
                spr_test_utils::test_validator_asserts::fetch_account(env.client.rpc(), &registry)?;
            let state = StorageRegistry::decode(&account.owner, &account.data)
                .ok_or_else(|| anyhow::anyhow!("the ring's records are not a storage registry"))?;
            Ok(
                match inputs.old_root == state.newest().hash
                    && inputs.old_next_index == state.next_index()
                {
                    true => Probe::Ready((response, inputs, state)),
                    false => Probe::NotYet,
                },
            )
        })?;
        let (apply, tracking_revision) = match owners.is_empty() {
            true => (
                env.run(
                    StorageApplyProof {
                        inputs: &inputs,
                        registry: &state,
                        asset_writes: &[],
                    }
                    .prove(env.client.ring_client()),
                )?,
                None,
            ),
            false => {
                let proven = env.run(
                    PrivateStorageWrite {
                        ring: self.ring,
                        inputs: &inputs,
                        registry: &state,
                        asset_writes: &[],
                        owners,
                    }
                    .prove(env.client.ring_client()),
                )?;
                (proven.apply, Some(proven.tracking_revision))
            }
        };
        let update = UpdateStorage {
            authority,
            registry,
            tracking_registry: state.tracking_registry,
            data: StorageUpdateIxData {
                revision: Revision::from(response.revision),
                writes: self.writes,
                asset_writes: Vec::new(),
                tracking_revision,
                apply: &apply,
            },
        };
        let instruction = update.clone().instruction()?;
        let mut altered_apply = apply;
        altered_apply.new_root[31] ^= 1;
        let altered = UpdateStorage {
            data: StorageUpdateIxData {
                apply: &altered_apply,
                ..update.data
            },
            ..update
        }
        .instruction()?;
        let before =
            spr_test_utils::test_validator_asserts::fetch_account(env.client.rpc(), &registry)?;
        let rejection =
            crate::shared::send_expecting_rejection(env.client.rpc(), &env.payer, altered)?;
        spr_program_test::Rejection::custom(
            custom_ring_interface::storage::StorageError::ProofVerificationFailed as u32,
        )
        .at(0)
        .assert_client(&rejection);
        spr_test_utils::test_validator_asserts::assert_account_unchanged(
            env.client.rpc(),
            &registry,
            &before,
        )?;
        let signature = send(env.client.rpc(), &env.payer, &[instruction])?;
        println!(
            "RING_APPLY_KEY writes={} mutation_refused=true settled=true",
            inputs.writes.len()
        );
        Ok(signature)
    }
}
