//! `deploy`, the buffer writes and every other loader call are ours.

#[path = "deploy/buffer.rs"]
mod buffer;

use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, ensure, Context as _, Result};
use custom_ring_interface::{RingAccount, RingPolicy};
use sha2::{Digest, Sha256};
use solana_account::Account;
use solana_address::Address;
use solana_loader_v3_interface::{instruction as loader, state::UpgradeableLoaderState};
use solana_sbpf::elf_parser::{
    consts::{ELFOSABI_NONE, EM_BPF, EM_SBPF, ET_DYN, PF_X, PT_LOAD},
    Elf64,
};
use solana_system_interface::instruction as system;
use spr_client::Rpc;
use zolana_operator_custody::{OperatorSigner, SigningIdentity};
use zolana_ring_client::{CustomRing, ProgramData};

use crate::{
    line, policy::Pinned, release::RingProgram, submission::Operators, workspace::Workspace,
    Context, DeployArgs, Target,
};

/// The smallest growth of `ProgramData` the loader accepts.
const MIN_EXTEND_BYTES: usize = loader::MINIMUM_EXTEND_PROGRAM_BYTES as usize;
const USABLE_TIMEOUT: Duration = Duration::from_secs(60);
const SLOT_POLL: Duration = Duration::from_millis(400);
const LOADER: Address = Address::new_from_array(spr_interface::BPF_LOADER_UPGRADEABLE_ID);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProgramBinary {
    pub len: usize,
    pub sha256: [u8; 32],
}

struct ProgramFile {
    bytes: Vec<u8>,
    binary: ProgramBinary,
}

#[must_use]
struct ProgramUpload<'a> {
    authority: &'a dyn OperatorSigner,
    file: &'a ProgramFile,
}

#[must_use]
struct UploadBuffer<'a> {
    address: Address,
    seed: String,
    source: ProgramUpload<'a>,
}

/// An absent account is vacant with zero lamports.
enum ProgramAccount {
    Vacant { lamports: u64 },
    Deployed(ProgramData),
}

#[must_use]
enum Deployment {
    First {
        program_signer: SigningIdentity,
        lamports: u64,
    },
    Upgrade(ProgramData),
}

trait Extension {
    fn extension(&self, length: usize) -> Result<usize>;
}

pub fn run(ctx: &mut Context, args: DeployArgs) -> Result<()> {
    with_buffer(ctx, args, |buffer| {
        let outcome = buffer.send(ctx)?;
        let (program, authority) = (ctx.ring.program_id(), buffer.source.authority.address());
        println!("{outcome} {program} under {authority}");
        Ok(())
    })
}

pub fn close_buffer(ctx: &Context, args: DeployArgs) -> Result<()> {
    with_buffer(ctx, args, |buffer| buffer.refund(ctx))
}

fn with_buffer<T>(
    ctx: &Context,
    args: DeployArgs,
    act: impl FnOnce(&UploadBuffer<'_>) -> Result<T>,
) -> Result<T> {
    let workspace = Workspace::from_env()?;
    ensure!(
        workspace.is_none() || ctx.config.target == Target::Localnet,
        "workspace artifacts are only supported on localnet"
    );
    let program_so = match (args.program_so, workspace) {
        (Some(path), _) => ctx.path(&path),
        (None, Some(workspace)) => workspace.ring_program_so()?,
        (None, None) => {
            let program = RingProgram::from_lock()?;
            let release = format!("{} {}", program.tag, program.asset.name);
            line("release", release);
            program.ensure()?
        }
    };
    line("binary", program_so.display());
    let file = ProgramFile::load(&program_so)?;
    let authority = ctx.upgrade_authority_signer()?;
    let buffer = ProgramUpload {
        authority: authority.as_ref(),
        file: &file,
    }
    .bind(ctx.ring.program_id())?;
    line("buffer", buffer.address);
    act(&buffer)
}

impl UploadBuffer<'_> {
    fn send(&self, ctx: &Context) -> Result<&'static str> {
        let authority = self.source.authority;
        let binary = self.source.file.binary;
        let program = ctx.ring.program_id();
        let payer = authority.address();
        ensure!(payer != program, "program address cannot pay deploy fees");
        let outcome = match Deployment::of(ctx, payer)? {
            Deployment::First {
                program_signer,
                lamports,
            } => {
                self.fund_and_write(ctx, required_rent(ctx.rpc(), binary.len, None)?)?;
                let program_len = UpgradeableLoaderState::size_of_program();
                let rent = ctx
                    .rpc()
                    .get_minimum_balance_for_rent_exemption(program_len)?;
                let mut instructions = loader::deploy_with_max_program_len(
                    &payer,
                    &program,
                    &self.address,
                    &payer,
                    rent,
                    binary.len,
                    true,
                )?;
                // `create_account` refuses an address holding lamports.
                if lamports != 0 {
                    instructions.splice(
                        ..1,
                        [
                            system::transfer(&payer, &program, rent.saturating_sub(lamports)),
                            system::allocate(&program, u64::try_from(program_len)?),
                            system::assign(&program, &LOADER),
                        ],
                    );
                }
                let operators = Operators {
                    payer: authority,
                    signers: &[program_signer.as_ref()],
                };
                ctx.send_operator(operators, &instructions)?;
                "deployed"
            }
            Deployment::Upgrade(data) => {
                if binary.deployed_sha256(ctx)? == Some(binary.sha256) {
                    wait_until_usable(ctx.rpc(), program, data.slot)?;
                    return Ok("present");
                }
                let policy = ctx.rpc().get_account(ctx.ring.policy_pda())?;
                ensure_policy_compatible(program, policy.as_ref())?;
                self.fund_and_write(ctx, required_rent(ctx.rpc(), binary.len, Some(&data))?)?;
                let operators = Operators::solo(authority);
                if binary.len > data.capacity {
                    let bytes = u32::try_from(data.extension(binary.len)?)?;
                    let extend = loader::extend_program(&program, Some(&payer), bytes);
                    ctx.send_operator(operators, &[extend])?;
                }
                let upgrade = loader::upgrade(&program, &self.address, &payer, &payer, true);
                ctx.send_operator(operators, &[upgrade])?;
                "upgraded"
            }
        };
        ctx.rpc()
            .assert_executable(&program)
            .with_context(|| format!("program {program} is not executable after deploy"))?;
        binary.verify_deployed(ctx)?;
        let slot = program_data(ctx)?
            .with_context(|| not_program_data(ctx.ring))?
            .slot;
        wait_until_usable(ctx.rpc(), program, slot)?;
        Ok(outcome)
    }
}

impl Deployment {
    fn of(ctx: &Context, authority: Address) -> Result<Self> {
        Ok(match program_account(ctx.rpc(), ctx.ring)? {
            ProgramAccount::Vacant { lamports } => Self::First {
                program_signer: ctx.program_signer()?,
                lamports,
            },
            ProgramAccount::Deployed(data) => {
                check_upgrade_authority(ctx.ring.program_id(), &data, authority)?;
                Self::Upgrade(data)
            }
        })
    }
}

fn system_wallet(account: &Account) -> bool {
    account.owner == solana_system_interface::program::ID
        && account.data.is_empty()
        && !account.executable
}

pub(crate) fn program_data(ctx: &Context) -> Result<Option<ProgramData>> {
    Ok(match program_account(ctx.rpc(), ctx.ring)? {
        ProgramAccount::Deployed(data) => Some(data),
        ProgramAccount::Vacant { .. } => None,
    })
}

fn program_account<R: Rpc>(rpc: &R, ring: CustomRing) -> Result<ProgramAccount> {
    let Some(program) = rpc.get_account(ring.program_id())? else {
        return Ok(ProgramAccount::Vacant { lamports: 0 });
    };
    if system_wallet(&program) {
        return Ok(ProgramAccount::Vacant {
            lamports: program.lamports,
        });
    }
    let pda = ring.program_data_pda();
    ensure!(
        program.executable
            && program.owner == LOADER
            && program.data.len() == UpgradeableLoaderState::size_of_program()
            && wincode::deserialize::<UpgradeableLoaderState>(&program.data)?
                == (UpgradeableLoaderState::Program {
                    programdata_address: pda
                }),
        "invalid loader program account"
    );
    let account = rpc
        .get_account(pda)?
        .with_context(|| not_program_data(ring))?;
    ensure!(
        !account.executable
            && account.data.len() >= UpgradeableLoaderState::size_of_programdata_metadata(),
        "invalid ProgramData account"
    );
    let mut data =
        zolana_ring_client::program_data(&account).with_context(|| not_program_data(ring))?;
    data.upgrade_authority = data
        .upgrade_authority
        .filter(|authority| *authority != Address::default());
    Ok(ProgramAccount::Deployed(data))
}

/// Only the upgrade authority the operator manifest names may move the program.
pub(crate) fn check_upgrade_authority(
    program: Address,
    data: &ProgramData,
    expected: Address,
) -> Result<()> {
    let Some(authority) = data.upgrade_authority else {
        bail!("program {program} is immutable, it cannot be upgraded");
    };
    ensure!(
        authority == expected,
        "program {program} is upgradeable by {authority}, the operator manifest names {expected}"
    );
    Ok(())
}

fn not_program_data(ring: CustomRing) -> String {
    let address = ring.program_data_pda();
    format!("{address} is not a ProgramData account")
}

impl ProgramFile {
    fn load(path: &Path) -> Result<Self> {
        let bytes =
            std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
        let binary = ProgramBinary::checked(&bytes)?;
        Ok(Self { bytes, binary })
    }
}

impl ProgramBinary {
    pub(crate) fn read(path: &Path) -> Result<Self> {
        Ok(ProgramFile::load(path)?.binary)
    }

    fn checked(bytes: &[u8]) -> Result<Self> {
        ensure!(
            u64::try_from(UpgradeableLoaderState::size_of_programdata(bytes.len()))?
                <= solana_system_interface::MAX_PERMITTED_DATA_LENGTH,
            "program binary exceeds the loader account limit"
        );
        let binary = Self::parse(bytes)?;
        zolana_cli_common::release::verify_program_keys(
            bytes,
            zolana_ring_client::proving_key_hashes(),
        )?;
        Ok(binary)
    }

    fn parse(bytes: &[u8]) -> Result<Self> {
        let elf = Elf64::parse(bytes).context("invalid SBF binary")?;
        let header = elf.file_header();
        ensure!(
            header.e_ident.ei_osabi == ELFOSABI_NONE
                && header.e_type == ET_DYN
                && matches!(header.e_machine, EM_BPF | EM_SBPF)
                && elf.program_header_table().iter().any(|segment| {
                    segment.p_type == PT_LOAD
                        && segment.p_flags & PF_X != 0
                        && segment.p_filesz != 0
                }),
            "invalid SBF binary"
        );
        Ok(Self {
            len: bytes.len(),
            sha256: Sha256::digest(bytes).into(),
        })
    }

    /// Unused loader capacity must contain only zero bytes.
    fn deployed_sha256(&self, ctx: &Context) -> Result<Option<[u8; 32]>> {
        let account = ctx.rpc().get_account(ctx.ring.program_data_pda())?;
        let account = account.with_context(|| not_program_data(ctx.ring))?;
        Ok(deployed_bytes(&account.data, self.len).map(|deployed| Sha256::digest(deployed).into()))
    }

    pub(crate) fn verify_deployed(&self, ctx: &Context) -> Result<()> {
        let found = self.deployed_sha256(ctx)?;
        let found = found.with_context(|| not_program_data(ctx.ring))?;
        ensure!(
            found == self.sha256,
            "program {} holds {} on chain, expected {}",
            ctx.ring.program_id(),
            hex::encode(found),
            hex::encode(self.sha256)
        );
        Ok(())
    }
}

/// The current schema must decode the ring policy and reproduce its pinned policy hash.
fn ensure_policy_compatible(program: Address, policy: Option<&Account>) -> Result<()> {
    let Some(account) = policy.filter(|account| account.owner == program) else {
        return Ok(());
    };
    ensure!(
        account.data.len() == RingPolicy::SIZE,
        "program {program} keeps a ring policy of {} bytes, the release needs {}, deploy a fresh ring",
        account.data.len(),
        RingPolicy::SIZE
    );
    let policy = RingPolicy::decode(&account.owner, &program, &account.data);
    ensure!(
        policy.is_some_and(|policy| Pinned::of(Box::new(policy)).is_ok()),
        "program {program} has an incompatible ring policy or policy commitment, deploy a fresh ring"
    );
    Ok(())
}

impl Extension for ProgramData {
    fn extension(&self, length: usize) -> Result<usize> {
        let max = usize::try_from(solana_system_interface::MAX_PERMITTED_DATA_LENGTH)?
            - UpgradeableLoaderState::size_of_programdata_metadata();
        Ok(length
            .saturating_sub(self.capacity)
            .max(MIN_EXTEND_BYTES)
            .min(max.saturating_sub(self.capacity)))
    }
}

/// The loader drains the buffer before it funds a new ProgramData account.
fn required_rent<R: Rpc>(rpc: &R, so_len: usize, deployed: Option<&ProgramData>) -> Result<u64> {
    let rent = |bytes: usize| rpc.get_minimum_balance_for_rent_exemption(bytes);
    let buffer = rent(UpgradeableLoaderState::size_of_buffer(so_len))?;
    let held = match deployed {
        None => {
            let data = rent(UpgradeableLoaderState::size_of_programdata(so_len))?;
            buffer
                .max(data)
                .checked_add(rent(UpgradeableLoaderState::size_of_program())?)
        }
        Some(data) if so_len > data.capacity => {
            let grown = data.capacity + data.extension(so_len)?;
            let extra = rent(UpgradeableLoaderState::size_of_programdata(grown))?.saturating_sub(
                rent(UpgradeableLoaderState::size_of_programdata(data.capacity))?,
            );
            buffer.checked_add(extra)
        }
        Some(_) => Some(buffer),
    };
    held.context("deploy rent exceeds the lamport limit")
}

/// The loader refuses a program in the slot it was deployed in.
fn wait_until_usable<R: Rpc>(rpc: &R, program: Address, deployed_slot: u64) -> Result<()> {
    let started = Instant::now();
    while rpc.get_slot()? <= deployed_slot {
        ensure!(
            started.elapsed() <= USABLE_TIMEOUT,
            "program {program} deployed at slot {deployed_slot} stays unusable, the validator does not advance"
        );
        thread::sleep(SLOT_POLL);
    }
    Ok(())
}

/// The loader writes the binary as given, padded to the account's capacity.
fn deployed_bytes(program_data: &[u8], so_len: usize) -> Option<&[u8]> {
    let start = UpgradeableLoaderState::size_of_programdata_metadata();
    let end = start.checked_add(so_len)?;
    if so_len == 0 || program_data.get(end..)?.iter().any(|byte| *byte != 0) {
        return None;
    }
    program_data.get(start..end)
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, collections::HashMap};

    use spr_client::ClientError;

    use super::*;

    /// Rent as the cluster prices it, 3480 lamports per byte-year over two years.
    struct Rent;

    impl Rpc for Rent {
        fn get_minimum_balance_for_rent_exemption(
            &self,
            data_len: usize,
        ) -> Result<u64, ClientError> {
            Ok((128 + data_len as u64) * 3480 * 2)
        }
    }

    fn rent(data_len: usize) -> u64 {
        Rent.get_minimum_balance_for_rent_exemption(data_len)
            .expect("priced")
    }

    fn elf() -> Vec<u8> {
        let mut bytes = vec![0; 264];
        bytes[..7].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1]);
        for (offset, value) in [
            (16, ET_DYN),
            (18, EM_SBPF),
            (52, 64),
            (54, 56),
            (56, 1),
            (58, 64),
            (60, 2),
            (62, 1),
        ] {
            bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        for (offset, value) in [(20, 1u32), (64, PT_LOAD), (68, PF_X), (204, 3)] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        for (offset, value) in [
            (32, 64u64),
            (40, 136),
            (72, 120),
            (96, 8),
            (104, 8),
            (224, 128),
            (232, 1),
        ] {
            bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn binary_validation_requires_complete_sbf_tables_and_sections() {
        let bytes = elf();
        let binary = ProgramBinary::parse(&bytes).unwrap();
        assert_eq!(binary.len, bytes.len());
        let expected: [u8; 32] = Sha256::digest(&bytes).into();
        assert_eq!(binary.sha256, expected);
        for length in 0..bytes.len() {
            assert!(ProgramBinary::parse(&bytes[..length]).is_err());
        }
        for (offset, value) in [(18, 62u64), (40, u64::MAX), (96, u64::MAX)] {
            let mut invalid = bytes.clone();
            invalid[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
            assert!(ProgramBinary::parse(&invalid).is_err());
        }
        let mut invalid = bytes;
        invalid[68..72].fill(0);
        assert!(ProgramBinary::parse(&invalid).is_err());
    }

    #[test]
    fn deployed_bytes_skip_the_metadata_and_stop_at_the_binary_length() {
        let meta = UpgradeableLoaderState::size_of_programdata_metadata();
        let mut data = vec![0u8; meta];
        data.extend_from_slice(b"elf");
        data.extend_from_slice(&[0u8; 8]);
        assert_eq!(deployed_bytes(&data, 3), Some(&b"elf"[..]));
        assert_eq!(deployed_bytes(&data, 3 + 8), Some(&data[meta..]));
        assert_eq!(deployed_bytes(&data, 3 + 9), None);
        assert_eq!(deployed_bytes(&data[..meta - 1], 0), None);
        assert_eq!(deployed_bytes(&data, 0), None);
        data[meta + 3] = 1;
        assert_eq!(deployed_bytes(&data, 3), None);
    }

    #[test]
    fn only_the_recorded_upgrade_authority_may_move_the_program() {
        let check = |upgrade_authority| {
            let data = ProgramData {
                slot: 0,
                upgrade_authority,
                capacity: 0,
            };
            check_upgrade_authority(
                Address::new_from_array([1; 32]),
                &data,
                Address::new_from_array([2; 32]),
            )
        };
        assert!(check(Some(Address::new_from_array([2; 32]))).is_ok());
        let foreign = check(Some(Address::new_from_array([3; 32]))).expect_err("foreign");
        assert!(
            foreign.to_string().contains("is upgradeable by"),
            "{foreign}"
        );
        let immutable = check(None).expect_err("immutable");
        assert!(immutable
            .to_string()
            .ends_with("is immutable, it cannot be upgraded"));
    }

    #[test]
    fn an_incompatible_ring_policy_refuses_the_upgrade() {
        use custom_ring_interface::N_SOURCE_SLOTS;
        let program = Address::from([7u8; 32]);
        let foreign = Address::from([9u8; 32]);
        let account = |owner, data: &[u8]| Account {
            owner,
            data: data.to_vec(),
            ..Default::default()
        };
        assert!(ensure_policy_compatible(program, None).is_ok());
        let mut registry: Box<RingPolicy> = bytemuck::zeroed_box();
        registry.discriminator = custom_ring_interface::RING_POLICY;
        registry.auditor_pubkey[0] = custom_ring_interface::VIEWING_PUBKEY_TAG;
        let audit_only = account(program, bytemuck::bytes_of(&*registry));
        assert!(ensure_policy_compatible(program, Some(&audit_only)).is_ok());
        let rules = zolana_ring_policy::RuleTable::builder().build();
        registry
            .set_policy(custom_ring_interface::PolicyUpdate {
                sources: [bytemuck::Zeroable::zeroed(); N_SOURCE_SLOTS],
                rules: rules.encode(),
                slot: 0,
            })
            .unwrap();
        let pinned = account(program, bytemuck::bytes_of(&*registry));
        assert!(ensure_policy_compatible(program, Some(&pinned)).is_ok());
        let short = [0; RingPolicy::SIZE - 1];
        assert!(ensure_policy_compatible(program, Some(&account(foreign, &short))).is_ok());
        let refused =
            ensure_policy_compatible(program, Some(&account(program, &short))).expect_err("short");
        assert!(refused.to_string().contains(&format!(
            "of {} bytes, the release needs {}",
            RingPolicy::SIZE - 1,
            RingPolicy::SIZE
        )));
        registry.policy_hash[0] ^= 1;
        let drifted = ensure_policy_compatible(
            program,
            Some(&account(program, bytemuck::bytes_of(&*registry))),
        )
        .expect_err("drifted");
        assert!(drifted.to_string().contains("incompatible ring policy"));
    }

    #[test]
    fn a_first_deploy_reuses_buffer_rent() {
        let so_len = 300_000;
        assert_eq!(
            required_rent(&Rent, so_len, None).expect("priced"),
            rent(UpgradeableLoaderState::size_of_program())
                + rent(UpgradeableLoaderState::size_of_programdata(so_len))
        );
    }

    #[test]
    fn an_upgrade_pays_the_buffer_and_only_the_growth() {
        let deployed = ProgramData {
            upgrade_authority: None,
            capacity: 300_000,
            slot: 0,
        };
        let fits = required_rent(&Rent, deployed.capacity, Some(&deployed)).expect("priced");
        assert_eq!(
            fits,
            rent(UpgradeableLoaderState::size_of_buffer(deployed.capacity))
        );
        // One byte too big still extends by the loader's minimum.
        let grows = required_rent(&Rent, deployed.capacity + 1, Some(&deployed)).expect("priced");
        assert_eq!(
            grows - fits,
            (rent(MIN_EXTEND_BYTES) - rent(0)) + (rent(1) - rent(0))
        );
    }

    struct Accounts(HashMap<Address, Account>);

    impl Rpc for Accounts {
        fn get_account(&self, address: Address) -> Result<Option<Account>, ClientError> {
            Ok(self.0.get(&address).cloned())
        }
    }

    const RING: [u8; 32] = [7; 32];

    fn loader_account(state: &UpgradeableLoaderState, len: usize, executable: bool) -> Account {
        let mut data = wincode::serialize(state).expect("state");
        data.resize(len, 0);
        Account {
            lamports: 1,
            data,
            owner: LOADER,
            executable,
            ..Account::default()
        }
    }

    fn program_points_at(programdata_address: Address) -> Account {
        let state = UpgradeableLoaderState::Program {
            programdata_address,
        };
        loader_account(&state, UpgradeableLoaderState::size_of_program(), true)
    }

    fn program_data_held_by(upgrade_authority_address: Option<Address>) -> Account {
        let state = UpgradeableLoaderState::ProgramData {
            slot: 9,
            upgrade_authority_address,
        };
        loader_account(
            &state,
            UpgradeableLoaderState::size_of_programdata(64),
            false,
        )
    }

    fn decoded(program: Option<Account>, program_data: Option<Account>) -> Result<ProgramAccount> {
        let ring = CustomRing::new(Address::new_from_array(RING));
        let held = [
            (ring.program_id(), program),
            (ring.program_data_pda(), program_data),
        ];
        let accounts = held
            .into_iter()
            .filter_map(|(address, account)| Some((address, account?)))
            .collect();
        program_account(&Accounts(accounts), ring)
    }

    #[test]
    fn an_absent_address_or_a_system_wallet_takes_a_first_deploy() {
        let vacant = |program| match decoded(program, None).expect("vacant") {
            ProgramAccount::Vacant { lamports } => lamports,
            ProgramAccount::Deployed(_) => panic!("deployed"),
        };
        assert_eq!(vacant(None), 0);
        let wallet = Account {
            lamports: 5,
            ..Account::default()
        };
        assert_eq!(vacant(Some(wallet.clone())), 5);
        let allocated = Account {
            data: vec![0],
            ..wallet
        };
        let refused = decoded(Some(allocated), None).err().expect("refused");
        assert_eq!(refused.to_string(), "invalid loader program account");
    }

    #[test]
    fn a_deployed_program_decodes_its_slot_capacity_and_upgrade_authority() {
        let ring = CustomRing::new(Address::new_from_array(RING));
        let program = program_points_at(ring.program_data_pda());
        let authority = Address::new_from_array([2; 32]);
        for (stored, upgrade_authority) in [
            (Some(authority), Some(authority)),
            (Some(Address::default()), None),
            (None, None),
        ] {
            let deployed = decoded(Some(program.clone()), Some(program_data_held_by(stored)));
            let ProgramAccount::Deployed(data) = deployed.expect("deployed") else {
                panic!("vacant");
            };
            let expected = ProgramData {
                slot: 9,
                upgrade_authority,
                capacity: 64,
            };
            assert_eq!(data, expected);
        }
    }

    #[test]
    fn a_program_account_without_its_program_data_is_refused() {
        let ring = CustomRing::new(Address::new_from_array(RING));
        let program = program_points_at(ring.program_data_pda());
        let held = program_data_held_by(None);
        let not_program_data = not_program_data(ring);
        for (program, program_data, reason) in [
            (
                program_points_at(Address::new_from_array([9; 32])),
                Some(held.clone()),
                "invalid loader program account",
            ),
            (
                Account {
                    executable: false,
                    ..program.clone()
                },
                Some(held.clone()),
                "invalid loader program account",
            ),
            (program.clone(), None, not_program_data.as_str()),
            (
                program.clone(),
                Some(Account {
                    data: held.data[..13].to_vec(),
                    ..held.clone()
                }),
                "invalid ProgramData account",
            ),
            (
                program.clone(),
                Some(Account {
                    owner: Address::new_from_array([9; 32]),
                    ..held.clone()
                }),
                not_program_data.as_str(),
            ),
        ] {
            let refused = decoded(Some(program), program_data).err().expect("refused");
            assert_eq!(refused.to_string(), reason);
        }
    }

    #[test]
    fn an_extension_is_the_loader_minimum_capped_at_the_account_limit() {
        let limit = usize::try_from(solana_system_interface::MAX_PERMITTED_DATA_LENGTH)
            .expect("limit")
            - UpgradeableLoaderState::size_of_programdata_metadata();
        let extension = |capacity: usize, length: usize| {
            let deployed = ProgramData {
                slot: 0,
                upgrade_authority: None,
                capacity,
            };
            deployed.extension(length).expect("extension")
        };
        for (capacity, length, expected) in [
            (1_000, 1_001, MIN_EXTEND_BYTES),
            (1_000, 1_000 + MIN_EXTEND_BYTES, MIN_EXTEND_BYTES),
            (1_000, 1_001 + MIN_EXTEND_BYTES, MIN_EXTEND_BYTES + 1),
            (limit - MIN_EXTEND_BYTES, limit - 1, MIN_EXTEND_BYTES),
            (limit - 10, limit - 5, 10),
            (limit, limit, 0),
        ] {
            assert_eq!(extension(capacity, length), expected);
        }
    }

    #[test]
    fn rent_beyond_the_lamport_limit_is_refused() {
        struct Flat(u64);
        impl Rpc for Flat {
            fn get_minimum_balance_for_rent_exemption(
                &self,
                _data_len: usize,
            ) -> Result<u64, ClientError> {
                Ok(self.0)
            }
        }
        let half = u64::MAX / 2;
        assert_eq!(
            required_rent(&Flat(half), 1, None).expect("priced"),
            half * 2
        );
        let refused = required_rent(&Flat(half + 1), 1, None).expect_err("overflow");
        assert_eq!(refused.to_string(), "deploy rent exceeds the lamport limit");
    }

    #[test]
    fn a_deploy_is_usable_from_the_next_slot() {
        struct Advancing(Cell<u64>);
        impl Rpc for Advancing {
            fn get_slot(&self) -> Result<u64, ClientError> {
                let slot = self.0.get();
                self.0.set(slot + 1);
                Ok(slot)
            }
        }
        let rpc = Advancing(Cell::new(7));
        wait_until_usable(&rpc, Address::default(), 8).expect("usable");
        assert_eq!(rpc.0.get(), 10);
    }
}
