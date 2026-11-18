//! Loaded ring context and transaction landing.

#[path = "operator.rs"]
mod operator;

use zolana_operator_custody::{signed_transaction, Kms, OperatorSigner};
use zolana_ring_client::rpc::RECORD_UPDATE_SLOTS;

use super::*;

pub struct Context {
    pub config_path: PathBuf,
    /// The directory of ring.toml, relative key paths resolve against it.
    pub root: PathBuf,
    pub config: RingConfig,
    pub ring: CustomRing,
    pub zolana: Zolana,
    /// The `--catalogue` override, `None` for the bundled file.
    pub catalogue: Option<String>,
    ask: RefCell<Box<dyn Ask>>,
    operators: OnceCell<config::OperatorManifest>,
    kms: OnceCell<Kms>,
    fee_payer_keypair: Option<PathBuf>,
    fee_payer: OnceCell<Keypair>,
}

#[must_use]
pub struct ContextSetup {
    pub config_path: PathBuf,
    pub config: RingConfig,
    pub ask: Box<dyn Ask>,
    pub catalogue: Option<String>,
    pub prover_tee_policy: Option<TeePolicy>,
    pub fee_payer_keypair: Option<PathBuf>,
}

pub enum Probe<T> {
    Ready(T),
    NotYet,
    Retry(anyhow::Error),
}

impl ContextSetup {
    pub fn open(self) -> Result<Context> {
        let ContextSetup {
            config_path,
            config,
            ask,
            catalogue,
            prover_tee_policy,
            fee_payer_keypair,
        } = self;
        let root = config_root(&config_path);
        let urls = config.urls();
        let zolana = ZolanaServices {
            rpc: SolanaRpc::new(urls.rpc.clone()),
            indexer: SprIndexer::new(&urls.indexer),
            prover: urls.prover_client(&root, prover_tee_policy.clone())?,
            ring_prover: urls.prover_client(&root, prover_tee_policy)?,
        }
        .connect();
        Ok(Context {
            ring: CustomRing::new(config.program_id.0),
            config_path,
            root,
            config,
            zolana,
            catalogue,
            ask: RefCell::new(ask),
            operators: OnceCell::new(),
            kms: OnceCell::new(),
            fee_payer_keypair,
            fee_payer: OnceCell::new(),
        })
    }
}

impl Context {
    /// Relative to the ring directory, an absolute or `~` path as given.
    pub fn path(&self, path: &Path) -> PathBuf {
        resolve(&self.root, path)
    }

    pub fn rpc(&self) -> &SolanaRpc {
        self.zolana.rpc()
    }

    pub fn chain(&self) -> &Chain {
        self.zolana.as_async()
    }

    pub fn ring_client(
        &self,
    ) -> &custom_ring_sdk::RingClient<Blocking<SolanaRpc>, Blocking<SprIndexer>> {
        self.zolana.ring_client()
    }

    pub fn block<T, E: From<BlockingInsideRuntime>>(
        &self,
        future: impl std::future::Future<Output = Result<T, E>>,
    ) -> Result<T, E> {
        self.zolana.block_on(future)
    }

    pub fn ring_config(&self) -> Result<CustomRingConfig> {
        self.block(self.ring.config(self.chain()))?
            .context("the ring has no config, run `zolana-ring init` first")
    }

    pub(crate) fn member_payer(&self, mode: RingMode, owner: Address) -> Result<&Keypair> {
        let payer = match self.fee_payer.get() {
            Some(payer) => payer,
            None => {
                let payer = match &self.fee_payer_keypair {
                    Some(path) => file::read_keypair(&config::expand_tilde(&self.path(path))?)?,
                    None if mode == RingMode::Anonymous => {
                        bail!("an anonymous ring requires --fee-payer-keypair")
                    }
                    None => self.sender()?,
                };
                self.fee_payer.get_or_init(|| payer)
            }
        };
        ensure!(
            mode != RingMode::Anonymous || payer.pubkey() != owner,
            "the fee payer {owner} is the hidden owner, paying would publish it"
        );
        Ok(payer)
    }

    /// Localnet airdrops what the step spends, devnet waits at the faucet.
    pub fn fund(&self, who: Address, required: u64) -> Result<()> {
        match self.config.target {
            Target::Localnet => {
                if self.rpc().get_balance(who)? < required.max(LOCALNET_BALANCE / 2) {
                    SolanaRpc::new(self.config.urls().rpc.clone())
                        .airdrop(&who, required.max(LOCALNET_BALANCE))?;
                }
                Ok(())
            }
            Target::Devnet => {
                fund::wait_for_balance(self.rpc(), &mut **self.ask.borrow_mut(), who, required)
            }
        }
    }

    /// No airdrop, for a step that must not move the operator's funds itself.
    pub fn require_balance(&self, who: Address, required: u64) -> Result<()> {
        let balance = self.rpc().get_balance(who)?;
        ensure!(
            balance >= required,
            "{who} holds {balance} lamports, {required} are needed"
        );
        Ok(())
    }

    /// Kept between runs, earlier change stays spendable with it.
    pub fn sender(&self) -> Result<Keypair> {
        file::read_or_create_keypair(&self.path(Path::new(SENDER_KEYPAIR_FILE)))
    }

    /// `signers[0]` pays, each ring instruction brings its own compute budget.
    pub fn send(&self, signers: &[&dyn Signer], instructions: &[Instruction]) -> Result<Signature> {
        let payer = signers.first().context("a transaction needs a payer")?;
        self.rpc()
            .create_and_send_transaction(
                instructions,
                payer.pubkey(),
                signers,
                self.budget(instructions),
            )
            .map_err(transaction_failure)
    }

    pub fn send_operator(
        &self,
        signers: &[&dyn OperatorSigner],
        instructions: &[Instruction],
    ) -> Result<Signature> {
        let payer = signers.first().context("a transaction needs a payer")?;
        let (blockhash, _) = self.rpc().get_latest_blockhash()?;
        let message = spr_client::rpc::compile_message(
            &payer.address(),
            instructions,
            blockhash,
            self.budget(instructions),
        )?;
        let transaction = self.block(async {
            Ok::<_, anyhow::Error>(signed_transaction(&message, signers).await?)
        })?;
        self.rpc()
            .process_transaction(transaction)
            .map_err(transaction_failure)
    }

    pub fn wait_for_indexed(&self, signature: Signature) -> Result<()> {
        wait_for_indexed_transaction(&self.zolana, signature)
    }

    pub fn interactive(&self) -> bool {
        self.ask.borrow().interactive()
    }

    pub fn ask_yes(&self, prompt: &str, default: bool) -> Result<bool> {
        self.ask.borrow_mut().confirm(prompt, default)
    }

    /// `--yes` skips the question, without a terminal `needs_yes` is the error.
    pub fn confirm(
        &self,
        yes: bool,
        prompt: &str,
        needs_yes: impl Display,
        declined: impl Display,
    ) -> Result<()> {
        if yes {
            return Ok(());
        }
        ensure!(self.interactive(), "{needs_yes}");
        ensure!(self.ask_yes(prompt, false)?, "{declined}");
        Ok(())
    }

    pub fn ring_rpc(&self) -> RingRpcClient {
        RingRpcClient::new(&self.config.urls().ring_rpc)
    }

    pub fn record_update_target(&self) -> Result<custom_ring_sdk::RecordUpdateTarget> {
        let policy = self
            .block(
                self.ring
                    .read::<custom_ring_interface::RingPolicy>(self.chain()),
            )?
            .context("the ring has no policy account, run `zolana-ring init` first")?;
        Ok(custom_ring_sdk::RecordUpdateTarget {
            genesis_hash: self.rpc().genesis_hash()?,
            ring: self.ring,
            registry: policy.records,
            valid_until_slot: self.rpc().get_slot()?.saturating_add(RECORD_UPDATE_SLOTS),
        })
    }

    pub fn ring_rpc_target(&self) -> Result<zolana_ring_client::rpc::ReadTarget> {
        let service = self.ring_rpc().service_pubkey()?;
        if let Trust::Pinned(pinned) = self.trust(Trust::Unpinned)? {
            ensure!(
                pinned == service,
                "the ring RPC reports service key {service}, expected {pinned}"
            );
        }
        Ok(zolana_ring_client::rpc::ReadTarget {
            cluster: self.rpc().genesis_hash()?,
            service,
            ring: self.ring.program_id(),
        })
    }

    pub fn trust(&self, unpinned: Trust) -> Result<Trust> {
        if let Some(key) = &self.config.urls().ring_rpc_pubkey {
            return key
                .parse()
                .map(Trust::Pinned)
                .with_context(|| format!("ring_rpc_pubkey {key} is not a base58 key"));
        }
        Ok(self
            .local_rpc_roles()?
            .map_or(unpinned, |roles| Trust::Pinned(roles.service.address)))
    }

    fn budget(&self, instructions: &[Instruction]) -> ComputeBudgetConfig {
        let units = instructions
            .iter()
            .map(|instruction| self.compute_units(instruction))
            .fold(0u32, u32::saturating_add)
            .min(MAX_COMPUTE_UNITS);
        ComputeBudgetConfig::new(units)
    }

    fn compute_units(&self, instruction: &Instruction) -> u32 {
        instruction
            .data
            .first()
            .filter(|_| instruction.program_id == self.ring.program_id())
            .and_then(|tag| InstructionTag::try_from(*tag).ok())
            .map_or(
                ComputeBudgetConfig::for_instruction_count(1).cu_limit,
                compute_unit_limit,
            )
    }
}

fn transaction_failure(error: ClientError) -> anyhow::Error {
    match hint(&error) {
        Some(hint) => anyhow::Error::new(error).context(hint),
        None => error.into(),
    }
}

pub(crate) fn config_root(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

pub(crate) fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() || path.starts_with("~") {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}
