use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use solana_address::Address;
use spr_keypair::NullifierKey;
use spr_transaction::{Mint, WalletUtxo};
use zolana_ring_client::rpc::{
    MergeSettings, MergeSettingsAttestation, RelayId, SetMergeSettingsRequest,
    SetMergeSettingsResponse,
};
use zolana_ring_transaction::RingAssets;

use ring_service_core::{
    authorize::{PendingNonce, WalletCheck},
    keyed_tasks::{KeyedTasks, MAX_CONCURRENT_KEYED_TASKS},
    upstream::TransactionSource,
};
use zolana_ring_indexer::redact::redacted;

use ring_audit::{merge_selection::MergeTarget, shared::Shared};

use crate::{
    error::RingRelayError,
    merge_inputs::HeldInput,
    merge_settings::{MergeSettingsBook, SignedMergeSettings},
    relay::Relay,
    relays::RelayPhase,
};
pub use ring_audit::{audit::AuditService, utxos::SpendableNotes};

pub const MANAGED_MERGE_INTERVAL: Duration = Duration::from_secs(60);

/// The owner's nullifier secret lives only for the build.
#[must_use]
pub struct OwnerMerge<'a> {
    pub(crate) ring: Address,
    pub(crate) owner: Address,
    pub(crate) nullifier_key: &'a NullifierKey,
    pub(crate) inputs: Vec<WalletUtxo>,
}

#[must_use]
pub struct ManagedMergeInit<S> {
    pub shared: Arc<Shared<S>>,
    pub relay: Arc<Relay>,
}

pub struct ManagedMerge<S> {
    shared: Arc<Shared<S>>,
    relay: Arc<Relay>,
}

/// Every mint one owner settled on the ring.
struct OwnerSettings {
    owner: Address,
    settings: Vec<MergeSettings>,
}

struct RingPass<S> {
    service: AuditService<S>,
    assets: RingAssets,
    held: HashSet<[u8; 32]>,
}

impl<S> ManagedMergeInit<S> {
    pub fn build(self) -> ManagedMerge<S> {
        ManagedMerge {
            shared: self.shared,
            relay: self.relay,
        }
    }
}

impl<S: TransactionSource + 'static> ManagedMerge<S> {
    /// Every `interval`, the first at once, merges each wallet above its signed target.
    pub async fn run(self, interval: Duration) {
        let merge = Arc::new(self);
        let rings = KeyedTasks::new(MAX_CONCURRENT_KEYED_TASKS);
        let mut ticks = tokio::time::interval(interval);
        loop {
            ticks.tick().await;
            let due = match merge.due_rings().await {
                Ok(due) => due,
                Err(error) => {
                    log::warn!("managed merge failed because {}", redacted(&error));
                    continue;
                }
            };
            rings.spawn_idle(due, |ring| {
                let merge = merge.clone();
                async move {
                    merge.merge_ring_logged(ring).await;
                }
            });
        }
    }
}

impl<S: TransactionSource> ManagedMerge<S> {
    /// At most one merge for each wallet and mint, a failed ring or wallet only logs.
    pub async fn merge_due(&self) -> Result<Vec<RelayId>, RingRelayError> {
        let mut sent = Vec::new();
        for ring in self.due_rings().await? {
            sent.extend(self.merge_ring_logged(ring).await);
        }
        Ok(sent)
    }

    async fn due_rings(&self) -> Result<Vec<Address>, RingRelayError> {
        MergeSettingsBook::open(self.shared.database.durable())?
            .rings()
            .await
    }

    async fn merge_ring_logged(&self, ring: Address) -> Vec<RelayId> {
        let merged = match MergeSettingsBook::open(self.shared.database.durable()) {
            Ok(book) => self.merge_ring(&book, ring).await,
            Err(error) => Err(error),
        };
        merged.unwrap_or_else(|error| {
            log::warn!(
                "managed merge of ring {ring} failed because {}",
                redacted(&error)
            );
            Vec::new()
        })
    }

    /// An enrolled wallet's secret opens from its tracking row on every ring.
    async fn merge_ring(
        &self,
        book: &MergeSettingsBook<'_>,
        ring: Address,
    ) -> Result<Vec<RelayId>, RingRelayError> {
        let service = self.shared.service(Some(ring))?;
        let shared = &self.shared;
        if service.served_config().await?.is_none() {
            return Ok(Vec::new());
        }
        let pass = RingPass {
            service,
            assets: shared.due_assets().await,
            held: self.held_inputs(ring).await?,
        };
        let mut owners: BTreeMap<Address, Vec<MergeSettings>> = BTreeMap::new();
        for wallet in book.active(ring).await? {
            owners
                .entry(wallet.owner)
                .or_default()
                .push(wallet.settings);
        }
        let mut sent = Vec::new();
        for (owner, settings) in owners {
            match self
                .merge_owner(&pass, OwnerSettings { owner, settings })
                .await
            {
                Ok(merged) => sent.extend(merged),
                Err(error) => log::warn!(
                    "managed merge of {owner} on ring {ring} failed because {}",
                    redacted(&error)
                ),
            }
        }
        Ok(sent)
    }

    /// A failed merge frees its inputs, a confirmed one once the ring history spends them.
    async fn held_inputs(&self, ring: Address) -> Result<HashSet<[u8; 32]>, RingRelayError> {
        let synced = self.shared.store(ring);
        let inputs = self.relay.merge_inputs();
        let held_inputs = inputs.held(&ring).await?;
        let confirmed: Vec<[u8; 32]> = held_inputs
            .iter()
            .filter(|input| input.phase == RelayPhase::Confirmed)
            .map(|input| input.nullifier)
            .collect();
        let spent = synced.spend_rows(&confirmed).await?;
        let (settled, held): (Vec<HeldInput>, Vec<HeldInput>) =
            held_inputs
                .into_iter()
                .partition(|input| match input.phase {
                    RelayPhase::Pending => false,
                    RelayPhase::Confirmed => spent.contains_key(&input.nullifier),
                    RelayPhase::Failed => true,
                });
        let settled: Vec<[u8; 32]> = settled.iter().map(|input| input.nullifier).collect();
        inputs.release(&settled).await?;
        Ok(held.iter().map(|input| input.nullifier).collect())
    }

    async fn merge_owner(
        &self,
        pass: &RingPass<S>,
        OwnerSettings { owner, settings }: OwnerSettings,
    ) -> Result<Vec<RelayId>, RingRelayError> {
        let ring = pass.service.ring();
        // 1. A mint the registry does not know skips its own setting, the owner's other mints merge.
        let mut targets: Vec<(Mint, MergeTarget)> = Vec::new();
        for settings in &settings {
            let Some(target) = MergeTarget::of(settings) else {
                continue;
            };
            match pass.assets.registry().mint(&settings.mint.0) {
                Ok(mint) => targets.push((mint, target)),
                Err(error) => log::warn!(
                    "managed merge of {owner} in {} on ring {ring} failed because {}",
                    settings.mint.0,
                    redacted(&RingRelayError::from(error))
                ),
            }
        }
        if targets.is_empty() {
            return Ok(Vec::new());
        }
        let owner_key = owner.to_bytes();
        // 2. The secret opens to the nullifier key the registry record binds.
        let Some(nullifier_key) = pass.service.held_nullifier_key(&owner_key).await? else {
            return Ok(Vec::new());
        };
        // 3. Each candidate is unspent and held by no merge in the store.
        let mints: Vec<Mint> = targets.iter().map(|(mint, _)| *mint).collect();
        let plans: HashMap<Address, MergeTarget> = targets
            .iter()
            .map(|(mint, target)| (mint.asset, *target))
            .collect();
        let mut by_mint = self
            .shared
            .store(ring)
            .merge_plans(
                SpendableNotes {
                    ring,
                    owner_key,
                    nullifier_key: &nullifier_key,
                    mints: &mints,
                    held: &pass.held,
                },
                &plans,
            )
            .await?;
        let mut sent = Vec::new();
        for (mint, _) in targets {
            let Some(inputs) = by_mint.remove(&mint.asset) else {
                continue;
            };
            let merge = OwnerMerge {
                ring,
                owner,
                nullifier_key: &nullifier_key,
                inputs,
            };
            match self.relay.send_merge(merge).await {
                Ok(relay_id) => sent.push(relay_id),
                Err(error) => log::warn!(
                    "managed merge of {owner} in {} on ring {ring} failed because {}",
                    mint.asset,
                    redacted(&error)
                ),
            }
        }
        Ok(sent)
    }
}

/// A replay or a request not later than the stored one fails.
pub async fn set_merge_settings<S: TransactionSource>(
    shared: &Arc<Shared<S>>,
    request: &SetMergeSettingsRequest,
) -> Result<SetMergeSettingsResponse, RingRelayError> {
    let book = MergeSettingsBook::open(shared.database.durable())?;
    let ring = request.ring.0;
    let owner = request.owner.0;
    let service = shared.service(Some(ring))?;
    let attestation = MergeSettingsAttestation {
        target: service.read_target(),
        owner,
        settings: request.settings,
    };
    let now = shared.clock.now()?;
    let nonce = WalletCheck {
        wallet: owner,
        signature: &request.signature,
        timestamp: request.timestamp,
        nonce: &request.nonce,
        attestation: &attestation,
        now,
    }
    .decide()?;
    let pending = PendingNonce {
        ring,
        nonce,
        timestamp: request.timestamp,
    };
    shared
        .replay
        .authorize_once(pending, service.require_served_config())
        .await?;
    let saved = book
        .save(SignedMergeSettings {
            ring,
            owner,
            settings: &request.settings,
            signed_at: request.timestamp,
        })
        .await?;
    if !saved {
        return Err(RingRelayError::StaleMergeSettings);
    }
    Ok(request.settings)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use async_trait::async_trait;
    use custom_ring_interface::RingMode;
    use solana_hash::Hash;
    use solana_keypair::Keypair;
    use solana_signature::Signature;
    use solana_signer::Signer;
    use solana_transaction::versioned::VersionedTransaction;
    use spr_client::{ClientError, ComputeBudgetConfig, GetShieldedTransactionsByTagsResponse};
    use spr_keypair::X25519Keypair;
    use spr_transaction::{Data, Mint, Utxo};
    use zolana_ring_client::{
        rpc::{unix_now, MergeSettingsRequest, ReadTarget, RingChainStatus, SignedRequest},
        AuditError, CreatedUtxo, KnownUtxo, MergeAuditPage, RingIndexerError, RingOrigin,
        UtxoOwner,
    };
    use zolana_ring_indexer::rpc::{
        GetStorageMemberOpeningsRequest, GetStorageMemberOpeningsResponse,
        GetStorageMemberRecordsRequest, GetStorageMemberRecordsResponse,
    };

    use zolana_ring_indexer::tracking::{GetTrackingRecordsRequest, GetTrackingRecordsResponse};

    use super::*;
    use crate::{
        error::ServiceError,
        relay::{RelayParts, RelayPayload, SharedRings},
        relay_prover::{DelegateSeal, ProvenRelay, RelayParties, RelayProofRequest, RelayProver},
        relay_reservations::{RelayBudget, RelayReservations},
    };
    pub use ring_cosigner::{
        cosign::{ApproveClass, CoSignerApproval},
        cosign_limits::CoSignPolicyInit,
    };
    use ring_service_core::{
        clock::Clock,
        database::{ServiceStore, Store},
        keys::KeySource,
        landing::{signature_fees, ChainOutcome, ChainSubmit, LatestBlockhash},
        origins::Origins,
        upstream::{
            Finality, MergeAuditRequest, RecordsTip, RingConfiguration, RingFrame, RingHistoryPage,
            TrackingRoot, TransactionPage,
        },
    };
    use ring_writer::record_update::ReaderGrantStore;
    use zolana_ring_client::{rpc::ServiceTarget, MERGE_DEFAULT_INPUT_COUNT};

    use ring_audit::{
        owner_notes::OPENED_ROWS,
        shared::SharedInit,
        tracking::fixture::{Enrollment, TrackingLedger},
        tracking::HeldEnrollment,
        utxos::CreatedIn,
        wallet_snapshots::WalletSnapshotLimits,
    };

    pub(crate) const RING: Address = Address::new_from_array([8; 32]);
    pub(crate) const GENESIS: [u8; 32] = [3; 32];
    pub(crate) const MINT: Address = Address::new_from_array([6; 32]);
    const SECOND_MINT: Address = Address::new_from_array([0x16; 32]);
    pub(crate) fn fee_payer() -> Keypair {
        Keypair::new_from_array([5; 32])
    }
    const LAST_VALID_BLOCK_HEIGHT: u64 = 100;

    pub(crate) struct EscrowRingSource {
        config: RingConfiguration,
        /// Zero until a test commits one.
        pub(crate) ring_list_tag: Mutex<[u8; 32]>,
        pub(crate) origins: Mutex<HashMap<Signature, RingOrigin>>,
        pub(crate) tracking: TrackingLedger,
    }

    fn unused<T>() -> Result<T, ClientError> {
        Err(ClientError::Rpc("unused source".to_owned()))
    }

    impl TransactionSource for EscrowRingSource {
        async fn is_mint_delegate(
            &self,
            mint: Address,
            signer: Address,
        ) -> Result<bool, ClientError> {
            Ok(mint == MINT && signer == mint_delegate().pubkey())
        }

        async fn salted_list_writers(&self, _ring: Address) -> Result<Vec<Address>, ClientError> {
            Ok(vec![list_writer().pubkey()])
        }

        async fn transactions_by_tag(
            &self,
            _request: TransactionPage,
        ) -> Result<GetShieldedTransactionsByTagsResponse, ClientError> {
            unused()
        }

        async fn tracking_root(
            &self,
            _ring: Address,
            _finality: Finality,
        ) -> Result<Option<TrackingRoot>, ClientError> {
            Ok(self.tracking.root())
        }

        async fn tracking_records(
            &self,
            request: GetTrackingRecordsRequest,
        ) -> Result<GetTrackingRecordsResponse, RingIndexerError> {
            Ok(self.tracking.page(&request))
        }

        async fn ring_frame(
            &self,
            _frame: RingFrame,
        ) -> Result<Vec<spr_transaction::ShieldedTransaction>, ClientError> {
            unused()
        }

        async fn ring_history(
            &self,
            _request: RingHistoryPage,
        ) -> Result<GetShieldedTransactionsByTagsResponse, ClientError> {
            unused()
        }

        async fn merge_audit_page(
            &self,
            _request: MergeAuditRequest<'_>,
        ) -> Result<MergeAuditPage, AuditError> {
            Err(AuditError::InvalidMemberRegistry)
        }

        async fn transaction_origin(
            &self,
            signature: Signature,
            _event_index: u16,
            _ring: Address,
        ) -> Result<RingOrigin, AuditError> {
            let origins = self.origins.lock().expect("origins");
            origins
                .get(&signature)
                .cloned()
                .ok_or_else(|| AuditError::Unavailable {
                    signature,
                    message: "unused source".to_owned(),
                })
        }

        async fn ring_config(
            &self,
            _ring: Address,
        ) -> Result<Option<RingConfiguration>, ClientError> {
            let mut config = self.config;
            config.policy.ring_list_tag = *self.ring_list_tag.lock().unwrap();
            Ok(Some(config))
        }

        async fn ring_chain_status(&self, _ring: Address) -> Result<RingChainStatus, ClientError> {
            unused()
        }

        async fn slot(&self) -> Result<u64, ClientError> {
            unused()
        }

        async fn records_root(&self, _ring: Address) -> Result<RecordsTip, ClientError> {
            unused()
        }

        async fn storage_member_openings(
            &self,
            _request: GetStorageMemberOpeningsRequest,
        ) -> Result<GetStorageMemberOpeningsResponse, RingIndexerError> {
            Ok(unused()?)
        }

        async fn storage_member_records(
            &self,
            _request: GetStorageMemberRecordsRequest,
        ) -> Result<GetStorageMemberRecordsResponse, RingIndexerError> {
            Ok(unused()?)
        }

        async fn upgrade_authority(
            &self,
            _program: Address,
        ) -> Result<Option<Address>, ClientError> {
            unused()
        }

        async fn health(&self) -> Result<(), ClientError> {
            Ok(())
        }

        async fn genesis_hash(&self) -> Result<[u8; 32], ClientError> {
            Ok(GENESIS)
        }

        async fn tree_id(&self, _tree: Address) -> Result<u16, zolana_ring_client::RingError> {
            Ok(1)
        }

        async fn ring_assets(&self) -> Result<RingAssets, ClientError> {
            Ok(assets())
        }
    }

    /// Records each merge it proves, the instruction names the proof's turn so each message differs.
    #[derive(Default)]
    struct RecordingProver {
        owners: Mutex<Vec<(Address, usize)>>,
        tree_fees: Mutex<u64>,
    }

    #[async_trait]
    impl RelayProver for RecordingProver {
        async fn prove(
            &self,
            request: RelayProofRequest<'_>,
        ) -> Result<ProvenRelay, RingRelayError> {
            let RelayPayload::Merge(merge) = request.payload else {
                return Err(RingRelayError::Service(ServiceError::StateUnavailable));
            };
            let owner = merge.sender.signing_pubkey.as_ed25519().expect("owner");
            let mut owners = self.owners.lock().expect("owners");
            owners.push((Address::new_from_array(owner), merge.inputs.len()));
            Ok(ProvenRelay {
                instruction: solana_instruction::Instruction {
                    program_id: request.ring,
                    accounts: Vec::new(),
                    data: vec![u8::try_from(owners.len()).expect("proofs")],
                },
                budget: ComputeBudgetConfig::new(400_000),
                co_signers: Vec::new(),
                signers: Vec::new(),
                tree_fees: *self.tree_fees.lock().expect("tree fees"),
            })
        }

        async fn seal_delegate(
            &self,
            _seal: DelegateSeal<'_>,
            _parties: RelayParties,
        ) -> Result<zolana_ring_client::rpc::RelayProofInputs, RingRelayError> {
            Err(RingRelayError::Service(ServiceError::StateUnavailable))
        }
    }

    /// No outcome until a test sets one, a send fails while `down`.
    pub(crate) struct ScriptedChain {
        height: Mutex<u64>,
        pub(crate) outcome: Mutex<Option<ChainOutcome>>,
        sent: Mutex<usize>,
        pub(crate) down: Mutex<bool>,
    }

    impl Default for ScriptedChain {
        fn default() -> Self {
            Self {
                height: Mutex::new(1),
                outcome: Mutex::new(None),
                sent: Mutex::new(0),
                down: Mutex::new(false),
            }
        }
    }

    #[async_trait]
    impl ChainSubmit for ScriptedChain {
        async fn simulate(
            &self,
            _message: &solana_message::VersionedMessage,
        ) -> Result<ring_service_core::landing::Simulation, ServiceError> {
            Ok(ring_service_core::landing::Simulation::Passed { calls: Vec::new() })
        }

        async fn account_data(
            &self,
            _address: solana_address::Address,
        ) -> Result<Option<Vec<u8>>, ServiceError> {
            Ok(None)
        }

        async fn rent_exempt_lamports(&self, _data_len: usize) -> Result<u64, ServiceError> {
            Ok(0)
        }

        async fn slot(&self) -> Result<u64, ServiceError> {
            Ok(1)
        }

        async fn latest_blockhash(&self) -> Result<LatestBlockhash, ServiceError> {
            Ok((Hash::new_from_array([4; 32]), LAST_VALID_BLOCK_HEIGHT).into())
        }

        async fn block_height(&self) -> Result<u64, ServiceError> {
            Ok(*self.height.lock().expect("height"))
        }

        async fn send(&self, _transaction: &VersionedTransaction) -> Result<(), ServiceError> {
            if *self.down.lock().expect("down") {
                return Err(ServiceError::Upstream(ClientError::Rpc("down".to_owned())));
            }
            *self.sent.lock().expect("sent") += 1;
            Ok(())
        }

        async fn outcome(
            &self,
            _signature: Signature,
        ) -> Result<Option<ChainOutcome>, ServiceError> {
            Ok(self.outcome.lock().expect("outcome").clone())
        }
    }

    fn assets() -> RingAssets {
        RingAssets::new([Mint::new(MINT, 7), Mint::new(SECOND_MINT, 8)]).expect("assets")
    }

    /// The permanent delegate of `MINT` on the scripted chain.
    pub(crate) fn mint_delegate() -> Keypair {
        Keypair::new_from_array([24; 32])
    }

    pub(crate) fn list_writer() -> Keypair {
        Keypair::new_from_array([23; 32])
    }

    /// A zero lead byte as `RegisterKey` proves.
    pub(crate) const LIST_SALT: [u8; 32] = {
        let mut salt = [9; 32];
        salt[0] = 0;
        salt
    };

    /// `count` notes of 10 in `mint` under `key`.
    struct Holding<'a> {
        key: &'a NullifierKey,
        mint: Mint,
        count: u8,
    }

    const BUDGET: RelayBudget = RelayBudget {
        total_lamports: 1_000_000,
        signer_lamports: 1_000_000,
    };

    fn shared(
        source: EscrowRingSource,
        auditor: X25519Keypair,
        store: Store,
    ) -> Arc<Shared<EscrowRingSource>> {
        let init = SharedInit {
            source,
            genesis_hash: GENESIS,
            service: Keypair::new().pubkey(),
            keys: KeySource::Local {
                ring: RING,
                auditor,
            },
            clock: Clock::System,
            assets: None,
            origins: Origins::default(),
            store: ServiceStore::Durable(store),
            snapshot_limits: WalletSnapshotLimits::default(),
            reader_grants: ReaderGrantStore::Tree,
            grant_rows: None,
        };
        Arc::new(init.build().expect("shared"))
    }

    pub(crate) struct Fixture {
        pub(crate) shared: Arc<Shared<EscrowRingSource>>,
        pub(crate) relay: Option<Arc<Relay>>,
        prover: Arc<RecordingProver>,
        chain: Arc<ScriptedChain>,
    }

    impl Fixture {
        async fn new() -> Self {
            Self::on(Store::open("sqlite::memory:", 1).await.expect("store")).await
        }

        async fn on(store: Store) -> Self {
            Self::budgeted(store, BUDGET).await
        }

        async fn budgeted(store: Store, budget: RelayBudget) -> Self {
            let chain = Arc::new(ScriptedChain::default());
            let prover = Arc::new(RecordingProver::default());
            let parts = RelayParts {
                store,
                prover: prover.clone(),
                fee_payer: Arc::new(fee_payer()),
                chain: chain.clone(),
            };
            Self {
                chain,
                prover,
                ..Self::serving_within(None, Some(parts), budget).await
            }
        }

        pub(crate) async fn serving(delegate: Option<Address>, relay: Option<RelayParts>) -> Self {
            Self::serving_within(delegate, relay, BUDGET).await
        }

        async fn serving_within(
            delegate: Option<Address>,
            relay: Option<RelayParts>,
            budget: RelayBudget,
        ) -> Self {
            // One ring keeps one auditor key, a restart on the same store opens its rows.
            let auditor = X25519Keypair::from_bytes(&[0x5a; 32]);
            let source = EscrowRingSource {
                config: RingConfiguration {
                    auditor_pubkey: auditor.pubkey(),
                    mode: RingMode::Confidential,
                    policy: custom_ring_interface::RingPolicy {
                        roles: custom_ring_interface::RingRoles {
                            policy: Address::new_from_array([2; 32]),
                            ..bytemuck::Zeroable::zeroed()
                        },
                        delegate: delegate.unwrap_or_default(),
                        ..bytemuck::Zeroable::zeroed()
                    },
                },
                ring_list_tag: Mutex::default(),
                origins: Mutex::default(),
                tracking: TrackingLedger::default(),
            };
            let store = match &relay {
                Some(relay) => relay.store.clone(),
                None => Store::open("sqlite::memory:", 1).await.expect("store"),
            };
            let shared = shared(source, auditor, store.clone());
            let co_signer = CoSignPolicyInit {
                signer: Some(Arc::new(Keypair::new())),
                shared: shared.clone(),
                store: Some(store),
                approval: CoSignerApproval::new([ApproveClass::Transfers]),
            }
            .build()
            .expect("co-signer");
            let relay = relay.map(|parts| {
                Arc::new(
                    Relay::new(
                        parts,
                        ServiceTarget {
                            cluster: shared.genesis_hash,
                            service: shared.service,
                        },
                    )
                    .with_budget(budget)
                    .with_co_signer(co_signer)
                    .with_rings(Arc::new(SharedRings(shared.clone()))),
                )
            });
            Self {
                shared,
                relay,
                prover: Arc::default(),
                chain: Arc::default(),
            }
        }

        /// An applied enrollment of `wallet`, sealed to the served auditor.
        pub(crate) fn enroll(&self, wallet: &Keypair, key: &NullifierKey) -> HeldEnrollment {
            let auditor = self
                .shared
                .service(Some(RING))
                .expect("service")
                .auditor_pubkey();
            self.shared.source.tracking.enroll(Enrollment {
                owner: wallet.pubkey(),
                key,
                list_salt: LIST_SALT,
                auditor: &auditor,
            })
        }

        pub(crate) async fn escrow_notes(&self, wallet: &Keypair, notes: u8) -> NullifierKey {
            self.escrow_wallets(&[(wallet, notes)]).await.remove(0)
        }

        /// Held before any scan, a merge pass opens only held enrollments.
        pub(crate) async fn escrow_wallets(&self, wallets: &[(&Keypair, u8)]) -> Vec<NullifierKey> {
            let store = self.shared.store(RING);
            let mut keys = Vec::with_capacity(wallets.len());
            for (wallet, notes) in wallets {
                let key = NullifierKey::from_secret([wallet.pubkey().to_bytes()[0]; 31]);
                let held = self.enroll(wallet, &key);
                let admission = store
                    .enrollment_admission(vec![held])
                    .await
                    .expect("held enrollment");
                store.admit_enrollments(admission);
                self.record_notes(
                    wallet,
                    Holding {
                        key: &key,
                        mint: Mint::new(MINT, 7),
                        count: *notes,
                    },
                )
                .await;
                keys.push(key);
            }
            keys
        }

        async fn record_notes(&self, wallet: &Keypair, holding: Holding<'_>) {
            let Holding { key, mint, count } = holding;
            let store = self.shared.store(RING);
            let owner_key = wallet.pubkey().to_bytes();
            let nullifier_pk = key.pubkey().expect("nullifier pk");
            let asset = *assets().fields().next().expect("field");
            // A leading byte below the field modulus keeps every index a field element.
            let blinding = |index: u8| {
                let mut blinding = [1; 32];
                blinding[31] = index;
                blinding
            };
            // The delegate path rebuilds each opening's hash, a stand in commitment would fail it.
            let commitment = |index: u8| {
                Utxo {
                    owner: spr_keypair::PublicKey::from_ed25519(&owner_key),
                    asset: mint,
                    amount: 10,
                    blinding: blinding(index),
                    ring_program_id: Some(RING),
                    data: Data::default(),
                }
                .hash(&nullifier_pk, &[0; 32], &[0; 32], 1)
                .expect("commitment")
            };
            store
                .record(
                    CreatedIn {
                        slot: 1,
                        tx_signature: Signature::from([7; 64]),
                        event_index: 0,
                    },
                    (0..count).map(|index| CreatedUtxo {
                        commitment: commitment(index),
                        slot_index: u32::from(index),
                        leaf_index: u64::from(index),
                        utxo: KnownUtxo {
                            owner_key,
                            owner: UtxoOwner::Keyed { nullifier_pk },
                            asset: asset.0,
                            mint: Some(mint.asset),
                            amount: 10,
                            blinding: blinding(index),
                            data_hash: Some([0; 32]),
                            ring_data_hash: Some([0; 32]),
                            tree_id: 1,
                        },
                        ring_note: true,
                    }),
                    key,
                )
                .await;
        }

        async fn settle(&self, wallet: &Keypair, target_note_count: u32) {
            self.settle_mint(wallet, MINT, target_note_count).await;
        }

        async fn settle_mint(&self, wallet: &Keypair, mint: Address, target_note_count: u32) {
            let target = ReadTarget {
                cluster: GENESIS,
                service: self.shared.service,
                ring: RING,
            };
            let request: MergeSettingsRequest = SignedRequest::new(MergeSettingsAttestation {
                target,
                owner: wallet.pubkey(),
                settings: MergeSettings {
                    mint: mint.into(),
                    target_note_count,
                    min_average_amount: 0,
                },
            });
            let signed = request
                .at(unix_now().expect("clock"))
                .sign(wallet)
                .expect("signed settings");
            set_merge_settings(&self.shared, &signed)
                .await
                .expect("stored settings");
        }

        async fn merge_due(&self) -> Vec<RelayId> {
            ManagedMergeInit {
                shared: self.shared.clone(),
                relay: self.relay().clone(),
            }
            .build()
            .merge_due()
            .await
            .expect("merge pass")
        }

        fn relay(&self) -> &Arc<Relay> {
            self.relay.as_ref().expect("relay")
        }

        async fn track(&self) {
            self.relay().track().await.expect("tracked relays");
        }

        async fn held(&self) -> Vec<HeldInput> {
            self.relay()
                .merge_inputs()
                .held(&RING)
                .await
                .expect("held inputs")
        }
    }

    #[tokio::test]
    async fn a_wallet_above_its_target_count_is_merged() {
        let fixture = Fixture::new().await;
        let wallet = Keypair::new_from_array([11; 32]);
        fixture.escrow_notes(&wallet, 3).await;
        fixture.settle(&wallet, 1).await;

        assert_eq!(fixture.merge_due().await.len(), 1);
        assert_eq!(*fixture.chain.sent.lock().expect("sent"), 1);
        assert_eq!(
            *fixture.prover.owners.lock().expect("owners"),
            vec![(wallet.pubkey(), 3)]
        );
    }

    #[tokio::test]
    async fn a_wallet_without_settings_is_left_alone() {
        let fixture = Fixture::new().await;
        let settled = Keypair::new_from_array([11; 32]);
        let unsettled = Keypair::new_from_array([12; 32]);
        fixture.escrow_notes(&settled, 1).await;
        fixture.escrow_notes(&unsettled, 3).await;
        fixture.settle(&settled, 1).await;

        assert!(fixture.merge_due().await.is_empty());
        assert_eq!(*fixture.chain.sent.lock().expect("sent"), 0);
    }

    #[tokio::test]
    async fn a_dropped_merge_frees_its_notes_after_expiry() {
        let fixture = Fixture::new().await;
        let wallet = Keypair::new_from_array([11; 32]);
        fixture.escrow_notes(&wallet, 3).await;
        fixture.settle(&wallet, 1).await;
        assert_eq!(fixture.merge_due().await.len(), 1);
        assert!(fixture.merge_due().await.is_empty());

        *fixture.chain.height.lock().expect("height") = LAST_VALID_BLOCK_HEIGHT + 1;
        fixture.track().await;

        assert_eq!(fixture.merge_due().await.len(), 1);
        assert_eq!(*fixture.chain.sent.lock().expect("sent"), 2);
    }

    #[tokio::test]
    async fn a_confirmed_merge_holds_nothing_once_its_spend_syncs() {
        let fixture = Fixture::new().await;
        let wallet = Keypair::new_from_array([11; 32]);
        fixture.escrow_notes(&wallet, 3).await;
        fixture.settle(&wallet, 1).await;
        assert_eq!(fixture.merge_due().await.len(), 1);
        *fixture.chain.outcome.lock().expect("outcome") = Some(ChainOutcome::Confirmed { slot: 5 });
        fixture.track().await;
        // The backfill has not seen the spend, the notes still read as spendable.
        assert!(fixture.merge_due().await.is_empty());

        let spent: Vec<[u8; 32]> = fixture
            .held()
            .await
            .iter()
            .map(|input| input.nullifier)
            .collect();
        assert_eq!(spent.len(), 3);
        fixture.shared.store(RING).spend(spent).await;

        assert!(fixture.merge_due().await.is_empty());
        assert!(fixture.held().await.is_empty());
    }

    #[tokio::test]
    async fn a_restart_resends_the_stored_merge_and_builds_no_second_one() {
        let store = Store::open("sqlite::memory:", 1).await.expect("store");
        let before = Fixture::on(store.clone()).await;
        let wallet = Keypair::new_from_array([11; 32]);
        before.escrow_notes(&wallet, 3).await;
        before.settle(&wallet, 1).await;
        *before.chain.down.lock().expect("down") = true;
        assert!(before.merge_due().await.is_empty());

        let after = Fixture::on(store).await;
        after.escrow_notes(&wallet, 3).await;
        assert!(after.merge_due().await.is_empty());
        after.track().await;

        assert!(after.prover.owners.lock().expect("owners").is_empty());
        assert_eq!(*after.chain.sent.lock().expect("sent"), 1);
    }

    #[tokio::test]
    async fn a_restart_releases_orphaned_merge_inputs_after_finalized_expiry() {
        let store = Store::open("sqlite::memory:", 1).await.expect("store");
        let before = Fixture::on(store.clone()).await;
        let wallet = Keypair::new_from_array([11; 32]);
        before.escrow_notes(&wallet, 3).await;
        before.settle(&wallet, 1).await;
        assert_eq!(before.merge_due().await.len(), 1);
        drop(before);

        let after = Fixture::on(store).await;
        *after.chain.height.lock().expect("height") = LAST_VALID_BLOCK_HEIGHT;
        after.track().await;
        let held = after.held().await;
        assert_eq!(held.len(), 3);
        assert!(held.iter().all(|input| input.phase == RelayPhase::Pending));

        *after.chain.height.lock().expect("height") = LAST_VALID_BLOCK_HEIGHT + 1;
        after.track().await;
        let managed = ManagedMergeInit {
            shared: after.shared.clone(),
            relay: after.relay().clone(),
        }
        .build();
        assert!(managed
            .held_inputs(RING)
            .await
            .expect("held inputs")
            .is_empty());
        assert!(after.held().await.is_empty());
    }

    #[tokio::test]
    async fn all_mint_settings_share_two_owner_passes() {
        let fixture = Fixture::new().await;
        let wallet = Keypair::new_from_array([11; 32]);
        let key = fixture.escrow_notes(&wallet, 3).await;
        fixture
            .record_notes(
                &wallet,
                Holding {
                    key: &key,
                    mint: Mint::new(SECOND_MINT, 8),
                    count: 3,
                },
            )
            .await;
        fixture.settle(&wallet, 1).await;
        fixture.settle_mint(&wallet, SECOND_MINT, 1).await;

        OPENED_ROWS.with(|opened| opened.set(0));
        assert_eq!(fixture.merge_due().await.len(), 2);
        assert_eq!(OPENED_ROWS.with(std::cell::Cell::get), 12);
        assert_eq!(
            *fixture.prover.owners.lock().expect("owners"),
            vec![(wallet.pubkey(), 3), (wallet.pubkey(), 3)]
        );
    }

    /// The SDK pins what one keyed merge transaction carries, the plan stays inside it.
    #[tokio::test]
    async fn a_sixty_note_tree_merges_the_default_width_and_lands() {
        let fixture = Fixture::new().await;
        let wallet = Keypair::new_from_array([11; 32]);
        fixture.escrow_notes(&wallet, 60).await;
        fixture.settle(&wallet, 1).await;

        assert_eq!(fixture.merge_due().await.len(), 1);
        assert_eq!(
            *fixture.prover.owners.lock().expect("owners"),
            vec![(wallet.pubkey(), MERGE_DEFAULT_INPUT_COUNT)]
        );
        assert_eq!(fixture.held().await.len(), MERGE_DEFAULT_INPUT_COUNT);
        *fixture.chain.outcome.lock().expect("outcome") = Some(ChainOutcome::Confirmed { slot: 5 });
        fixture.track().await;
        assert!(fixture
            .held()
            .await
            .iter()
            .all(|input| input.phase == RelayPhase::Confirmed));
    }

    #[tokio::test]
    async fn an_unknown_mint_setting_skips_itself_and_the_valid_mint_merges() {
        let fixture = Fixture::new().await;
        let wallet = Keypair::new_from_array([11; 32]);
        fixture.escrow_notes(&wallet, 3).await;
        fixture
            .settle_mint(&wallet, Address::new_from_array([0x26; 32]), 1)
            .await;
        fixture.settle(&wallet, 1).await;

        assert_eq!(fixture.merge_due().await.len(), 1);
        assert_eq!(
            *fixture.prover.owners.lock().expect("owners"),
            vec![(wallet.pubkey(), 3)]
        );
    }

    #[tokio::test]
    async fn a_repeated_merge_settings_request_reads_replay() {
        use ring_service_core::authorize::Unauthorized;

        let fixture = Fixture::new().await;
        let wallet = Keypair::new_from_array([11; 32]);
        let signed = SignedRequest::new(MergeSettingsAttestation {
            target: ReadTarget {
                cluster: GENESIS,
                service: fixture.shared.service,
                ring: RING,
            },
            owner: wallet.pubkey(),
            settings: MergeSettings {
                mint: MINT.into(),
                target_note_count: 2,
                min_average_amount: 0,
            },
        })
        .at(unix_now().expect("clock"))
        .sign(&wallet)
        .expect("signed settings");
        set_merge_settings(&fixture.shared, &signed)
            .await
            .expect("first settings");
        let repeated = set_merge_settings(&fixture.shared, &signed).await;
        assert!(
            matches!(
                repeated,
                Err(RingRelayError::Service(ServiceError::Unauthorized(
                    Unauthorized::Replay
                )))
            ),
            "{repeated:?}"
        );
    }

    #[tokio::test]
    async fn set_merge_settings_for_unserved_ring_writes_no_settings_or_nonce() {
        use ring_service_core::authorize::Unauthorized;
        use sea_orm::{ConnectionTrait, Statement};

        let store = Store::open("sqlite::memory:", 1).await.expect("store");
        let foreign_auditor = X25519Keypair::new();
        let source = EscrowRingSource {
            config: RingConfiguration {
                auditor_pubkey: foreign_auditor.pubkey(),
                mode: RingMode::Confidential,
                policy: custom_ring_interface::RingPolicy {
                    roles: custom_ring_interface::RingRoles {
                        policy: Address::new_from_array([2; 32]),
                        ..bytemuck::Zeroable::zeroed()
                    },
                    ..bytemuck::Zeroable::zeroed()
                },
            },
            ring_list_tag: Mutex::new([0; 32]),
            origins: Mutex::new(HashMap::new()),
            tracking: TrackingLedger::default(),
        };
        let shared = shared(source, X25519Keypair::new(), store.clone());
        let wallet = Keypair::new_from_array([11; 32]);
        let signed = SignedRequest::new(MergeSettingsAttestation {
            target: ReadTarget {
                cluster: GENESIS,
                service: shared.service,
                ring: RING,
            },
            owner: wallet.pubkey(),
            settings: MergeSettings {
                mint: MINT.into(),
                target_note_count: 2,
                min_average_amount: 0,
            },
        })
        .at(unix_now().expect("clock"))
        .sign(&wallet)
        .expect("signed settings");

        let refused = set_merge_settings(&shared, &signed).await;
        assert!(
            matches!(
                refused,
                Err(RingRelayError::Service(ServiceError::Unauthorized(
                    Unauthorized::AuditorKeyMismatch
                )))
            ),
            "{refused:?}"
        );
        for table in ["merge_settings", "replay_nonces"] {
            let count: i64 = store
                .connection()
                .query_one(Statement::from_string(
                    store.connection().get_database_backend(),
                    format!("SELECT COUNT(*) AS count FROM {table}"),
                ))
                .await
                .expect("query")
                .expect("row")
                .try_get("", "count")
                .expect("count");
            assert_eq!(count, 0, "{table} must remain empty");
        }
    }

    #[tokio::test]
    async fn managed_merge_tree_fees_respect_the_relay_signer_budget() {
        let store = Store::open("sqlite::memory:", 1).await.expect("store");
        let fixture = Fixture::on(store.clone()).await;
        *fixture.prover.tree_fees.lock().expect("tree fees") = 1_000_001;
        let wallet = Keypair::new_from_array([11; 32]);
        fixture.escrow_notes(&wallet, 3).await;
        fixture.settle(&wallet, 1).await;

        assert!(fixture.merge_due().await.is_empty());
        assert_eq!(*fixture.chain.sent.lock().expect("sent"), 0);

        *fixture.prover.tree_fees.lock().expect("tree fees") = 10_000;
        let [relay_id] = fixture.merge_due().await.try_into().expect("one merge");
        assert_eq!(*fixture.chain.sent.lock().expect("sent"), 1);
        let held = RelayReservations::new(store.connection())
            .by_relay(&relay_id)
            .await
            .expect("read")
            .expect("row");
        assert_eq!(held.lamports, 10_000 + signature_fees(1));
    }

    /// A merge with no tree fee still spends the fee payer's signature fee.
    #[tokio::test]
    async fn a_managed_merge_sends_nothing_at_a_zero_budget() {
        let store = Store::open("sqlite::memory:", 1).await.expect("store");
        let fixture = Fixture::budgeted(store, RelayBudget::default()).await;
        let wallet = Keypair::new_from_array([11; 32]);
        fixture.escrow_notes(&wallet, 3).await;
        fixture.settle(&wallet, 1).await;

        assert!(fixture.merge_due().await.is_empty());
        assert_eq!(*fixture.chain.sent.lock().expect("sent"), 0);
        assert!(fixture.held().await.is_empty());
    }
}
