//! Pins `test-vectors/proof/transfer-layout.json`, the account windows of a ring transfer, for the SDK, Ring RPC and the TS SDK.

mod frames;

use core::ops::Range;
use zolana_ring_client::CoSignerPairs;
use zolana_ring_client::FrameEncode;
use zolana_ring_client::SppExpansion;

use custom_ring_interface::{
    cosign::CoSignRequirement, tag, CoSignScope, CoSigner, CoSignerCounts, CustomRingError,
    RingPolicy, RingRail, RingTransactData, RootRead, TransferLayout, TransferPrefix, CO_SIGNER,
    RING_POLICY,
};
use frames::Fixture;
use serde_json::{json, Value};
use solana_address::Address;
use spr_instruction::instruction::{
    RingAuthorityTransact, RingTransact, TransactInterfaceTransferAccounts,
    TransactSolTransferAccounts, TransactSplWithdrawalAccounts,
};
use spr_interface::{
    instruction::{instruction_data::transact::CacheAccess, InterfaceTransfer, TransactProof},
    pda,
};
use zolana_ring_client::KnownPrefix;

const VECTORS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../test-vectors/proof/transfer-layout.json"
);

const RING_PROGRAM: Address = Address::new_from_array([0x11; 32]);
const INSTRUCTIONS_SYSVAR: Address =
    Address::from_str_const("Sysvar1nstructions1111111111111111111111111");

fn address(seed: u8) -> Address {
    Address::new_from_array([seed; 32])
}

fn ring(rail: RingRail) -> Box<RingPolicy> {
    let mut ring: Box<RingPolicy> = bytemuck::zeroed_box();
    ring.discriminator = RING_POLICY;
    ring.mode = match rail.anonymous_frame() {
        true => 2,
        false => 0,
    };
    ring.auditor_pubkey[0] = 4;
    ring.tracking = address(0x21);
    ring
}

#[derive(Clone, Copy)]
struct AccountMeta {
    pubkey: Address,
    is_signer: bool,
    is_writable: bool,
}

impl AccountMeta {
    fn new_readonly(pubkey: Address, is_signer: bool) -> Self {
        Self {
            pubkey,
            is_signer,
            is_writable: false,
        }
    }
}

/// A signing co-signer, or the program form's instructions sysvar.
#[derive(Clone, Copy)]
enum Pair {
    Signed,
    Approved,
}

struct Case<'a> {
    name: &'static str,
    frame: RingTransactData<'a>,
    ring: Box<RingPolicy>,
    pairs: Vec<Pair>,
    /// The signer of an authority frame.
    delegate: Option<Address>,
    owners: Vec<Address>,
    legs: Vec<TransactInterfaceTransferAccounts>,
    cache: Option<Address>,
}

fn sol(seed: u8) -> TransactInterfaceTransferAccounts {
    TransactInterfaceTransferAccounts::Sol(TransactSolTransferAccounts {
        recipient: address(seed),
    })
}

fn confidential(fixture: &Fixture) -> RingTransactData<'_> {
    let mut frame = fixture.base(RingRail::Confidential, 2, 2);
    frame.outputs = vec![fixture.recipient(0), fixture.recipient(1)];
    fixture.with_audit_stream(frame)
}

fn cases(fixture: &Fixture) -> Vec<Case<'_>> {
    let plain = |rail: RingRail| Case {
        name: "",
        frame: confidential(fixture),
        ring: ring(rail),
        pairs: Vec::new(),
        delegate: None,
        owners: vec![address(0x51), address(0x52)],
        legs: Vec::new(),
        cache: None,
    };
    let co_signed = |pair: Pair| {
        let mut case = plain(RingRail::Confidential);
        case.ring.co_signers = CoSignerCounts::default()
            .added(CoSignScope::TRANSFERS)
            .unwrap();
        case.pairs = vec![pair];
        case
    };
    let mut policy = plain(RingRail::Confidential);
    policy.frame = fixture.confidential_policy();
    policy.ring.policy_hash = [1; 32];
    policy.ring.records = address(0x22);
    policy.ring.sources[0].registry = address(0x23);
    policy.pairs = vec![Pair::Signed];
    policy.legs = vec![
        sol(0x61),
        TransactInterfaceTransferAccounts::SplDeposit(
            spr_instruction::instruction::TransactSplDepositAccounts {
                mint: address(0x62),
                spl_interface: address(0x63),
                token_authority: address(0x64),
                user_token_account: address(0x65),
                token_program: address(0x66),
            },
        ),
    ];
    let mut legs = plain(RingRail::Confidential);
    legs.frame.interface_transfers = vec![
        InterfaceTransfer::SolDeposit { amount: 5 },
        InterfaceTransfer::SplWithdrawal {
            amount: 7,
            spl_interface_bump: 255,
        },
    ];
    legs.legs = vec![
        sol(0x61),
        TransactInterfaceTransferAccounts::SplWithdrawal(TransactSplWithdrawalAccounts {
            mint: address(0x62),
            spl_interface: address(0x63),
            user_token_account: address(0x65),
            token_program: address(0x66),
        }),
    ];
    let mut cached = plain(RingRail::Confidential);
    cached.frame.extensions.cache = Some(CacheAccess {
        read_bitmap: 1,
        write_slots: CacheAccess::NO_WRITES,
    });
    cached.cache = Some(address(0x71));
    let mut public_owners = plain(RingRail::Anonymous);
    public_owners.frame = fixture.anonymous_public_owners();
    let delegate = |mint: bool| {
        let mut case = plain(RingRail::Delegate);
        case.frame = fixture.delegate();
        case.owners = Vec::new();
        case.ring.delegate = address(0x31);
        case.delegate = Some(match mint {
            true => address(0x32),
            false => address(0x31),
        });
        case
    };
    [
        ("confidential signer", plain(RingRail::Confidential)),
        (
            "confidential signer with a co-signer",
            co_signed(Pair::Signed),
        ),
        (
            "program form with an approved co-signer",
            co_signed(Pair::Approved),
        ),
        ("anonymous public owners", public_owners),
        ("ring delegate", delegate(false)),
        ("mint permanent delegate", delegate(true)),
        ("deposit and withdrawal legs", legs),
        ("cached selector", cached),
        ("policy roots, a co-signer and two trees", policy),
    ]
    .into_iter()
    .map(|(name, case)| Case { name, ..case })
    .collect()
}

impl Case<'_> {
    fn delegate_mint(&self) -> bool {
        self.delegate
            .is_some_and(|signer| self.ring.delegate() != Some(&signer))
    }

    /// The ring prefix, then SPP's own builder for the forwarded list.
    fn accounts(&self) -> Vec<AccountMeta> {
        let delegate_rail = self.frame.rail.is_delegate();
        let mut accounts = vec![AccountMeta::new_readonly(address(0x20), false)];
        accounts.extend(
            self.ring
                .root_registries(RootRead::transfer(&self.ring, delegate_rail))
                .map(|registry| AccountMeta::new_readonly(registry, false)),
        );
        for (index, pair) in self.pairs.iter().enumerate() {
            accounts.push(AccountMeta::new_readonly(
                address(0x40 + index as u8),
                false,
            ));
            accounts.push(match pair {
                Pair::Signed => AccountMeta::new_readonly(address(0x48 + index as u8), true),
                Pair::Approved => AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR, false),
            });
        }
        if let Some(signer) = self.delegate {
            accounts.push(AccountMeta::new_readonly(signer, true));
            if self.delegate_mint() {
                accounts.push(AccountMeta::new_readonly(address(0x33), false));
            }
        }
        let data = self
            .frame
            .transact_ix_data(&self.ring, TransactProof::zeroed())
            .unwrap();
        let input_trees = (0..self.frame.input_trees.count())
            .map(|tree| address(0x80 + tree as u8))
            .collect();
        let spp = match delegate_rail {
            true => {
                RingAuthorityTransact {
                    payer: address(0x50),
                    input_trees,
                    output_tree: address(0x80),
                    ring_program_id: RING_PROGRAM,
                    interface_transfer_accounts: self.legs.clone(),
                    data,
                }
                .instruction()
                .accounts
            }
            false => {
                let transact = RingTransact {
                    payer: address(0x50),
                    input_trees,
                    output_tree: address(0x80),
                    ring_program_id: RING_PROGRAM,
                    owner_signers: self.owners.clone(),
                    interface_transfer_accounts: self.legs.clone(),
                    data,
                };
                match self.cache {
                    Some(cache) => transact.instruction_with_cache_read(cache).accounts,
                    None => transact.instruction().accounts,
                }
            }
        };
        accounts.extend(spp.into_iter().map(|meta| AccountMeta {
            pubkey: meta.pubkey,
            is_signer: meta.is_signer,
            is_writable: meta.is_writable,
        }));
        accounts
    }

    fn layout(&self, accounts: usize) -> Result<TransferLayout, CustomRingError> {
        TransferLayout::new(
            &self.frame,
            &self.ring,
            accounts,
            &mut KnownPrefix {
                co_signer_pairs: self.pairs.len(),
                delegate_mint: self.delegate_mint(),
            },
        )
    }
}

/// Covers the demand with one record of every class per pair, as the program reads its records.
struct CoveringRecords;

impl TransferPrefix for CoveringRecords {
    type Error = CustomRingError;

    fn co_signer_pairs(
        &mut self,
        _: usize,
        requirement: Option<CoSignRequirement>,
    ) -> Result<usize, CustomRingError> {
        CoSignRequirement::pairs(requirement, |pair, current| {
            current.cover(&CoSigner {
                discriminator: CO_SIGNER,
                signer: address(0x48 + pair as u8),
                scope: CoSignScope::ALL.bits(),
                bump: 0,
            })
        })
    }

    fn delegate_mint(&mut self, _: usize) -> Result<bool, CustomRingError> {
        Ok(false)
    }
}

fn span(range: &Range<usize>) -> Value {
    json!([range.start, range.end])
}

fn compute_vector() -> Value {
    let fixture = Fixture::new();
    let cases: Vec<Value> = cases(&fixture)
        .iter()
        .map(|case| {
            let accounts = case.accounts();
            let layout = case.layout(accounts.len()).unwrap();
            let ring_tag = match case.frame.rail.is_delegate() {
                true => tag::AUTHORITY_TRANSFER,
                false => tag::TRANSFER,
            };
            json!({
                "name": case.name,
                "rail": case.frame.rail.to_string(),
                "data": hex::encode([&[ring_tag][..], &case.frame.encode().unwrap()].concat()),
                "ring_policy": hex::encode(bytemuck::bytes_of(&*case.ring)),
                "co_signer_pairs": case.pairs.len(),
                "delegate_mint": case.delegate_mint(),
                "accounts": accounts.iter().map(|meta| json!({
                    "address": meta.pubkey.to_string(),
                    "signer": meta.is_signer,
                    "writable": meta.is_writable,
                })).collect::<Vec<_>>(),
                "windows": {
                    "ring": TransferLayout::RING,
                    "registries": span(&layout.registries),
                    "co_signers": span(&layout.co_signers),
                    "delegate": layout.delegate,
                    "mint": layout.mint,
                    "spp": layout.spp,
                    "payer": layout.payer(),
                    "ring_config": layout.ring_config(),
                    "trees": span(&layout.trees),
                    "nullifiers": span(&layout.nullifiers),
                    "owners": span(&layout.owners),
                    "settlement": span(&layout.settlement),
                    "legs": layout.legs(&case.frame.interface_transfers).map(|leg| json!({
                        "accounts": span(&leg.accounts),
                        "funder": leg.funder,
                    })).collect::<Vec<_>>(),
                    "cache": span(&layout.cache),
                },
            })
        })
        .collect();
    json!({
        "description": "The account windows of a ring Transfer or AuthorityTransfer, Rust TransferLayout in program-libs/interface/src/instructions/transfer_layout.rs. data is the instruction data from its tag, ring_policy the RingPolicy account, accounts the list in order. A window is [start, end) over accounts, a slot one index, null when the list has none. co_signers holds [record, signer] pairs, the program form names the instructions sysvar as signer. owners is SPP's owner signer run. Each leg names its settlement group and the index of its deposit funder. Produced by sdk-libs/client/tests/transfer_layout.rs.",
        "cases": cases,
    })
}

#[test]
fn the_transfer_layout_is_the_shared_vector() {
    let committed: Value =
        serde_json::from_str(&std::fs::read_to_string(VECTORS).expect("vector file"))
            .expect("vector json");
    assert_eq!(committed, compute_vector());
}

#[test]
#[ignore = "rewrites test-vectors/proof/transfer-layout.json"]
fn write_transfer_layout_vector() {
    let json = serde_json::to_string_pretty(&compute_vector()).expect("json");
    std::fs::write(VECTORS, json + "\n").expect("write vector file");
}

/// SPP's builders place each account, the windows must find it there.
#[test]
fn every_window_holds_the_accounts_the_spp_builder_placed() {
    let fixture = Fixture::new();
    let ring_auth = pda::ring_auth(&RING_PROGRAM).0;
    for case in cases(&fixture) {
        let accounts = case.accounts();
        let layout = case.layout(accounts.len()).unwrap();
        let keys = |range: &Range<usize>| -> Vec<Address> {
            accounts[range.clone()]
                .iter()
                .map(|meta| meta.pubkey)
                .collect()
        };
        assert_eq!(
            accounts[layout.payer()].pubkey,
            address(0x50),
            "{}",
            case.name
        );
        assert_eq!(
            accounts[layout.ring_config()].pubkey,
            ring_auth,
            "{}",
            case.name
        );
        assert_eq!(
            layout.delegate.map(|slot| accounts[slot].pubkey),
            case.delegate
        );
        assert_eq!(layout.mint.is_some(), case.delegate_mint(), "{}", case.name);
        assert_eq!(layout.co_signer_pairs().count(), case.pairs.len());
        assert!(keys(&layout.trees)
            .iter()
            .all(|tree| tree.as_array()[0] & 0xf0 == 0x80));
        assert_eq!(layout.nullifiers.len(), case.frame.nullifiers.len());
        assert_eq!(keys(&layout.owners), case.owners, "{}", case.name);
        assert!(accounts[layout.owners.clone()]
            .iter()
            .all(|meta| meta.is_signer));
        assert_eq!(
            keys(&layout.cache),
            Vec::from_iter(case.cache),
            "{}",
            case.name
        );
        let legs: Vec<_> = layout.legs(&case.frame.interface_transfers).collect();
        assert_eq!(legs.len(), case.legs.len(), "{}", case.name);
        for (leg, placed) in legs.iter().zip(&case.legs) {
            let group = keys(&leg.accounts);
            match placed {
                TransactInterfaceTransferAccounts::Sol(sol) => {
                    assert!(group.contains(&sol.recipient))
                }
                TransactInterfaceTransferAccounts::SplDeposit(spl) => {
                    assert_eq!(
                        leg.funder.map(|slot| accounts[slot].pubkey),
                        Some(spl.token_authority)
                    );
                    assert!(group.contains(&spl.mint));
                }
                TransactInterfaceTransferAccounts::SplWithdrawal(spl) => {
                    assert_eq!(leg.funder, None);
                    assert!(group.contains(&spl.user_token_account));
                }
            }
            assert_eq!(
                leg.funder.is_some(),
                leg.transfer.is_deposit(),
                "{}",
                case.name
            );
            assert!(leg.funder.is_none_or(|slot| accounts[slot].is_signer));
        }
        assert_eq!(
            legs.last()
                .map_or(layout.settlement.start, |leg| leg.accounts.end),
            layout.settlement.end
        );

        // The records the program reads and the located SPP window give the same map.
        if !case.frame.rail.is_delegate() {
            let covered = TransferLayout::new(
                &case.frame,
                &case.ring,
                accounts.len(),
                &mut CoveringRecords,
            );
            assert_eq!(covered.as_ref(), Ok(&layout), "{}", case.name);
        }
        let mut located = KnownPrefix::ending_at(&case.frame, &case.ring, layout.spp).unwrap();
        assert_eq!(
            TransferLayout::new(&case.frame, &case.ring, accounts.len(), &mut located),
            Ok(layout.clone()),
            "{}",
            case.name
        );

        // A list one account short of the frame's counts has no owner run to give up.
        let shortest = accounts.len() - layout.owners.len();
        assert_eq!(
            case.layout(shortest).map(|layout| layout.owners.len()),
            Ok(0)
        );
        assert_eq!(
            case.layout(shortest - 1),
            Err(CustomRingError::InvalidSettlementAccounts),
            "{}",
            case.name
        );
    }
}

/// No account sits between two windows or past the last.
#[test]
fn the_windows_of_every_frame_tile_the_list() {
    let fixture = Fixture::new();
    for (name, frame) in fixture.cases() {
        let ring = ring(frame.rail);
        for owners in [0, 3] {
            let mut prefix = KnownPrefix {
                co_signer_pairs: 2,
                delegate_mint: frame.rail.is_delegate(),
            };
            let shortest = TransferLayout::new(&frame, &ring, usize::MAX, &mut prefix)
                .unwrap()
                .nullifiers
                .end
                + frame
                    .interface_transfers
                    .iter()
                    .map(|transfer| transfer.settlement_account_count())
                    .sum::<usize>()
                + frame.cache_accounts();
            let accounts = shortest + owners;
            let layout = TransferLayout::new(&frame, &ring, accounts, &mut prefix).unwrap();
            let delegate = usize::from(frame.rail.is_delegate());
            assert_eq!(layout.registries.start, TransferLayout::RING + 1, "{name}");
            assert_eq!(layout.co_signers.start, layout.registries.end, "{name}");
            assert_eq!(
                layout.delegate.is_some(),
                frame.rail.is_delegate(),
                "{name}"
            );
            assert_eq!(layout.spp, layout.co_signers.end + 2 * delegate, "{name}");
            assert!(layout.payer() < layout.ring_config(), "{name}");
            assert_eq!(layout.trees.start, layout.ring_config() + 1, "{name}");
            assert_eq!(layout.nullifiers.start, layout.trees.end, "{name}");
            assert_eq!(
                layout.owners,
                layout.nullifiers.end..layout.settlement.start
            );
            assert_eq!(layout.owners.len(), owners, "{name}");
            assert_eq!(layout.cache, layout.settlement.end..accounts, "{name}");
        }
    }
}

#[test]
fn a_prefix_no_pair_count_fits_is_refused() {
    let fixture = Fixture::new();
    let frame = confidential(&fixture);
    let ring = ring(RingRail::Confidential);
    let ending_at = |frame: &RingTransactData<'_>, spp| KnownPrefix::ending_at(frame, &ring, spp);
    assert_eq!(
        ending_at(&frame, 2).map(|prefix| prefix.co_signer_pairs),
        Ok(0)
    );
    assert_eq!(
        ending_at(&frame, 4).map(|prefix| prefix.co_signer_pairs),
        Ok(1)
    );
    for spp in [0, 1, 3] {
        assert_eq!(
            ending_at(&frame, spp),
            Err(CustomRingError::InvalidSettlementAccounts)
        );
    }
    let delegate = fixture.delegate();
    assert_eq!(
        ending_at(&delegate, 3).map(|prefix| prefix.delegate_mint),
        Ok(false)
    );
    assert_eq!(
        ending_at(&delegate, 4).map(|prefix| prefix.delegate_mint),
        Ok(true)
    );
    for spp in [2, 5] {
        assert_eq!(
            ending_at(&delegate, spp),
            Err(CustomRingError::InvalidSettlementAccounts)
        );
    }
}
