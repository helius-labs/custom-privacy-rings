//! Command-line arguments and subcommands.

use super::*;

#[derive(Debug, Parser)]
#[command(name = "zolana-ring", about = "Operate a custom ring")]
pub struct Cli {
    /// Path to ring.toml.
    #[arg(long, default_value = RING_TOML, global = true)]
    pub config: PathBuf,
    /// Cluster for one command instead of the one ring.toml records.
    #[arg(long, global = true, env = "RING_TARGET")]
    pub target: Option<Target>,
    /// A curator catalogue file or URL instead of the bundled one.
    #[arg(long, global = true, env = "RING_CATALOGUE")]
    pub catalogue: Option<String>,
    /// Require the prover to attest to the TEE policy in FILE, bare or as a {"deployment": ...}
    /// pin file.
    #[arg(
        long,
        value_name = "FILE",
        global = true,
        env = "RING_PROVER_TEE_POLICY"
    )]
    pub prover_tee_policy: Option<PathBuf>,
    /// Anonymous rings require a fee payer with a different address from the owner.
    #[arg(
        long,
        value_name = "FILE",
        global = true,
        env = "RING_FEE_PAYER_KEYPAIR"
    )]
    pub fee_payer_keypair: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create `ring.toml` with the program address from the operator manifest.
    New(NewArgs),
    /// Recorded answers and on-chain state.
    Status,
    /// Record the cluster the ring acts on in ring.toml.
    Target { target: Target },
    /// Print one service URL of the active target.
    Url { service: Service },
    /// Record devnet in ring.toml and probe the deployed services.
    Devnet,
    /// Record localnet in ring.toml and start the validator, Photon, the prover and the ring rpc.
    #[command(alias = "dev")]
    Localnet(LocalnetArgs),
    /// Deploy the released program under the authority, or upgrade it in place.
    Deploy(DeployArgs),
    /// Create the config with the auditor key, pin the policy and register the ring with SPP.
    Init(InitArgs),
    /// Deploy, init, check the ring rpc, grant the authority and transact.
    Pipeline(ProgramArgs),
    /// Two ring deposits and one custom-ring transfer, read back from the ring RPC.
    Transact(TransactArgs),
    /// Pay recipients inside the ring from fresh deposits or from the sender's notes.
    Transfer(TransferArgs),
    /// Deposit into the sender's ring balance, every deposit carries its audit proof.
    Deposit(DepositArgs),
    /// Consolidate existing notes of one asset into one note.
    Merge(MergeArgs),
    /// Confirm the ring RPC in `ring.toml` is up and holds the ring's auditor key.
    RpcCheck,
    /// Transfer or renounce the program's upgrade authority.
    #[command(subcommand)]
    Authority(AuthorityCommand),
    /// Grant or revoke reads on the ring RPC.
    #[command(subcommand)]
    Reader(ReaderCommand),
    /// Set, clear or show the ring's co-signer.
    #[command(subcommand)]
    Cosigner(CoSignerCommand),
    /// Set or show the permanent delegate, or move a note as the delegate.
    #[command(subcommand)]
    Delegate(DelegateCommand),
    /// Enroll the member's nullifier key in the ring's tracking registry, apply pending enrollments, or rotate its viewing key.
    #[command(subcommand)]
    Key(KeyCommand),
    /// List the wallet updates pending list approval, or approve one.
    #[command(subcommand)]
    ListApproval(ListApprovalCommand),
    /// Read and write the list bits of member records.
    #[command(subcommand)]
    List(ListCommand),
    /// Read, check and replace the pinned rule table.
    #[command(subcommand)]
    Policy(PolicyCommand),
    /// Print the auditor's X25519 public key in hex.
    AuditorKey,
    /// Open the ring's transactions with the auditor key, read from the indexer.
    Audit(AuditArgs),
    /// Mint a `zolana:` payment request for the member on an anonymous ring.
    Address(AddressArgs),
    /// Print the sender's notes per mint, zero-key notes apart since spending one names the wallet.
    Balance(BalanceArgs),
    /// List the replies on the member's notes, each an unauthenticated claim, or confirm one.
    ReplyClaims(ReplyClaimsArgs),
}

#[derive(Debug, Subcommand)]
pub enum ListCommand {
    /// Set the list bit of each member's record.
    Add(EntryArgs),
    /// Clear the list bit of each member's record.
    Clear(EntryArgs),
    /// Print whether each member's record carries the list.
    Show(EntryArgs),
    /// Point the list at the ring's own registry or a curator ring's.
    SetSource {
        #[arg(value_enum)]
        list_id: ListName,
        /// Curator ring program id or catalogue name.
        #[arg(long, conflicts_with = "own", required_unless_present = "own")]
        curator: Option<String>,
        /// The ring's own registry.
        #[arg(long)]
        own: bool,
    },
    /// Set a list the sender writes on its own record.
    Join {
        #[arg(value_enum)]
        list: MemberList,
    },
    /// Clear a list the sender writes on its own record.
    Leave {
        #[arg(value_enum)]
        list: MemberList,
    },
}

#[derive(Debug, Args)]
pub struct EntryArgs {
    #[arg(value_enum)]
    pub list_id: ListName,
    #[command(flatten)]
    pub members: MemberArgs,
}

#[derive(Debug, Subcommand)]
pub enum PolicyCommand {
    /// The pinned rows, hash, generation and sources.
    Show,
    /// Compare ring.toml with the chain, exits non-zero on a difference.
    Check,
    /// Replace the pinned table with ring.toml under the policy authority.
    Set {
        /// Confirms the replacement, proofs built against the old table are refused from now on.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum CoSignerCommand {
    /// Create or rescope a co-signer's record, flags absent means the `[cosigner]` table of ring.toml.
    Set {
        /// The Solana key that signs beside the sender.
        #[arg(long)]
        signer: Option<Address>,
        /// Operation classes the co-signer gates.
        #[arg(long, value_delimiter = ',', requires = "signer")]
        scope: Vec<CoSignClass>,
        /// `<mint>=<amount>` per mint, `sol` for the native token, withdrawals above it need the co-signer.
        #[arg(long, requires = "signer")]
        threshold: Vec<ThresholdSpec>,
    },
    /// Close the co-signer's record, the rent returns to the authority.
    Clear {
        /// The record's key, absent means the `[cosigner]` key of ring.toml.
        #[arg(long)]
        signer: Option<Address>,
    },
    /// The classes some record covers and the named record, absent means the `[cosigner]` key.
    Show {
        #[arg(long)]
        signer: Option<Address>,
    },
}

#[derive(Debug, Subcommand)]
pub enum DelegateCommand {
    /// Set the permanent delegate from the operator manifest.
    Set,
    Show,
    /// Move a source member's ring balance to shielded addresses over the delegate rail.
    Move(Box<DelegateMoveArgs>),
}

#[derive(Debug, Args)]
pub struct DelegateMoveArgs {
    /// Each recipient's base58 shielded address, every one receives the amount.
    #[arg(required = true, num_args = 1..)]
    pub to: Vec<ShieldedAddress>,
    /// The source member's base58 shielded address, its ring notes fund the move.
    #[arg(long)]
    pub source: ShieldedAddress,
    #[arg(long, default_value_t = DEFAULT_TRANSACT_AMOUNT)]
    pub amount: u64,
    /// The mint moved, `sol` by default.
    #[arg(long)]
    pub mint: Option<Mint>,
}

/// The co-signer of a ring whose scope covers the operation.
#[derive(Debug, Default, Args)]
pub struct CosignerArg {
    /// Request a signature from the Ring RPC co-signer with the given public key.
    #[arg(long)]
    pub cosigner: Option<Address>,
}

#[derive(Debug, Subcommand)]
pub enum ListApprovalCommand {
    /// Print each update pending list approval with the lists it awaits.
    Pending,
    /// Set every list the wallet's pending updates await in one update that lands them.
    Approve { wallet: Address },
}

#[derive(Debug, Subcommand)]
pub enum KeyCommand {
    /// Seal the member's nullifier key to the ring auditor and hand the signed enrollment to ring rpc, `transfer` does it on first use.
    Register,
    /// Replace the member's viewing key in its user record and tracking leaf, earlier keys stay held for old notes.
    Rotate,
}

#[derive(Debug, Subcommand)]
pub enum ReaderCommand {
    /// A base58 Solana key or the 66-hex P-256 key of a passkey.
    Grant { reader: ReaderKey },
    /// Clear the `reader` list of the reader's record.
    Revoke { reader: ReaderKey },
}

#[derive(Debug, Subcommand)]
pub enum AuthorityCommand {
    /// Transfer the program upgrade authority to the signer named in a JSON file.
    Transfer {
        new_signer: PathBuf,
        /// Confirms the new key, nobody else can hand the program back.
        #[arg(long)]
        yes: bool,
    },
    /// Transfer one ring role to the KMS signer named in a JSON file.
    UpdateRole { role: RoleName, new_signer: PathBuf },
    /// Stop every deposit, transfer and merge of the ring.
    Pause,
    /// Open the ring again after a pause.
    Resume,
    /// Make the program immutable, irreversible.
    Renounce {
        /// Confirms the irreversible step.
        #[arg(long)]
        yes: bool,
        /// The binary the deployment must match, the released ring program by default.
        #[arg(long)]
        program_so: Option<PathBuf>,
    },
}

#[derive(Debug, Args)]
pub struct NewArgs {
    /// Ring name in kebab-case, also the directory name, asked when absent.
    pub name: Option<String>,
    /// Parent directory the ring is created in.
    #[arg(long, default_value = ".")]
    pub dest: PathBuf,
    /// Answer every question with its default.
    #[arg(long)]
    pub silent: bool,
    /// Load the KMS signer references from a JSON file.
    #[arg(long)]
    pub operator_manifest: PathBuf,
    /// A `ring.toml` or any toml file with a `[policy]` table, replaces the policy questions.
    #[arg(long)]
    pub policy_from: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct LocalnetArgs {
    /// Record the target and print the URLs, start nothing.
    #[arg(long)]
    pub no_start: bool,
}

#[derive(Debug, Args)]
pub struct AuditArgs {
    /// Continue from the cursor an earlier run printed.
    #[arg(long)]
    pub cursor: Option<String>,
}

#[derive(Debug, Args)]
pub struct AddressArgs {
    /// The member's Solana keypair, `keys/sender-keypair.json` by default.
    #[arg(long)]
    pub keypair: Option<PathBuf>,
    /// Requested amount in base units, unauthenticated like the mint and label.
    #[arg(long)]
    pub amount: Option<u64>,
    #[arg(long)]
    pub mint: Option<Mint>,
    #[arg(long)]
    pub label: Option<String>,
}

#[derive(Debug, Args)]
pub struct ReplyClaimsArgs {
    /// The member's Solana keypair, `keys/sender-keypair.json` by default.
    #[arg(long)]
    pub keypair: Option<PathBuf>,
    /// Trust the sender a listed claim names, by its viewing key in hex.
    #[arg(long)]
    pub confirm: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProgramArgs {
    /// A local binary instead of the released ring program.
    #[arg(long)]
    pub program_so: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct DeployArgs {
    #[command(flatten)]
    pub program: ProgramArgs,
    /// Refund the upload buffer for the selected program binary.
    #[arg(long)]
    pub close_buffer: bool,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Hex tagged X25519 auditor key, created by the ring RPC and written here when absent.
    #[arg(long, default_value = AUDITOR_PUBKEY_FILE)]
    pub auditor_pubkey_file: PathBuf,
    /// Accept the ring RPC's auditor key without a pinned service key in ring.toml.
    #[arg(long)]
    pub trust_ring_rpc: bool,
}

#[derive(Debug, Args)]
pub struct TransactArgs {
    /// Lamports per deposit and per payment to the recipient.
    #[arg(long, default_value_t = DEFAULT_TRANSACT_AMOUNT)]
    pub amount: u64,
    #[command(flatten)]
    pub cosigner: CosignerArg,
    /// Use `in-proof` on a confidential ring to authorize the spend inside the proof.
    #[arg(long, value_enum, default_value_t = OwnerRail::Signer)]
    pub owner_rail: OwnerRail,
    /// Localnet only, the authority lists both demo parties on the ring's own allow list.
    #[arg(long)]
    pub enrol_allow: bool,
}

#[derive(Debug, Args)]
pub struct TransferArgs {
    /// Each recipient, a base58 shielded address on a confidential ring, a `zolana:` request
    /// on an anonymous one, or a shielded address with `--default-ring`.
    #[arg(required = true, num_args = 1..)]
    pub to: Vec<Recipient>,
    /// Base units each recipient receives, a request's own amount or the default when absent.
    #[arg(long)]
    pub amount: Option<u64>,
    /// Registered mint, a request's own mint or `sol` when absent.
    #[arg(long)]
    pub mint: Option<Mint>,
    /// Payer's SPL funding account, its ATA by default.
    #[arg(long)]
    pub token_account: Option<Address>,
    /// `deposit` funds the payments with two fresh deposits, `held` spends the sender's notes
    /// of the ring, and of the default ring on a confidential ring.
    #[arg(long, value_enum, default_value_t = Funding::Deposit)]
    pub funding: Funding,
    /// Spending default-ring notes publishes the hidden owner's key, `allow` lets `--funding held`
    /// spend them in place of ring notes that fall short.
    #[arg(long, value_enum, default_value_t = EntrySpendName::Refuse)]
    pub entry_spend: EntrySpendName,
    /// The fewest output slots, dummies fill what the payments and the change leave.
    #[arg(long)]
    pub outputs: Option<usize>,
    /// Attach the sender's address to each anonymous payment, an unauthenticated claim the recipient must confirm.
    #[arg(long)]
    pub reply: bool,
    /// An anonymous ring pays each shielded recipient a default-ring note, the value leaves the ring.
    #[arg(long)]
    pub default_ring: bool,
    #[command(flatten)]
    pub cosigner: CosignerArg,
    /// Use `in-proof` on a confidential ring to authorize the spend inside the proof.
    #[arg(long, value_enum, default_value_t = OwnerRail::Signer)]
    pub owner_rail: OwnerRail,
    /// Wallet that signs in public and spends its zero-key notes, each note's sender learns the spend.
    #[arg(long, value_name = "WALLET")]
    pub spend_public: Option<Address>,
}

#[derive(Debug, Args)]
pub struct BalanceArgs {
    /// Registered mint, `sol` by default.
    #[arg(long)]
    pub mint: Option<Mint>,
}

#[derive(Debug, Args)]
pub struct DepositArgs {
    /// Base units the wallet fee payer deposits into the sender's ring balance.
    #[arg(
        long,
        default_value_t = DEFAULT_TRANSACT_AMOUNT,
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub amount: u64,
    /// Registered mint, `sol` by default.
    #[arg(long)]
    pub mint: Option<Mint>,
    /// Payer's SPL funding account, its ATA by default.
    #[arg(long)]
    pub token_account: Option<Address>,
    #[command(flatten)]
    pub cosigner: CosignerArg,
}

#[derive(Debug, Args)]
pub struct MergeArgs {
    /// Mint to merge, `sol` by default.
    #[arg(long)]
    pub mint: Option<Mint>,
    /// Maximum number of notes to merge, the transaction size may carry fewer.
    #[arg(
        long,
        default_value_t = MERGE_DEFAULT_INPUT_COUNT as u64,
        value_parser = clap::value_parser!(u64).range(2..=custom_ring_sdk::MAX_RING_MERGE_INPUTS as u64)
    )]
    pub count: u64,
    #[command(flatten)]
    pub cosigner: CosignerArg,
}

/// The arguments a command defaults to, what the pipeline runs each step with.
pub fn defaults<T: Args + FromArgMatches>() -> T {
    let command = T::augment_args(clap::Command::new("defaults"));
    T::from_arg_matches(&command.get_matches_from(["defaults"]))
        .expect("every argument has a default")
}
