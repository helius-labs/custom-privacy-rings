//! The shared frame vectors through `parse` and `roots`.

use bytemuck::Zeroable;
use custom_ring_interface::{
    storage::Revision, tracking::TrackingRegistry, CustomRingError, LiveRoots, ParsedTransact,
    RingPolicy, RingRail, RingTransactData, SourceSlot,
};
use serde_json::Value;
use solana_address::Address;

const RAILS: [RingRail; 4] = [
    RingRail::Confidential,
    RingRail::Anonymous,
    RingRail::Delegate,
    RingRail::AnonymousDelegate,
];

fn vector(name: &str) -> Value {
    let file = format!(
        "{}/../../test-vectors/proof/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(file).expect("vector file")).expect("json")
}

/// The compact frame of `name` in the expansion vector and the rail it parses under.
fn expansion_case(name: &str) -> (RingRail, Vec<u8>) {
    let expansion = vector("ring-transact-expansion.json");
    let case = expansion["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|case| case["name"] == name)
        .expect("named case");
    let rail = RAILS
        .into_iter()
        .find(|rail| case["rail"] == rail.to_string())
        .expect("rail");
    let frame = hex::decode(case["compact"].as_str().expect("hex")).expect("hex");
    (rail, frame)
}

fn frames() -> Vec<Vec<u8>> {
    let frame = |hex: &Value| hex::decode(hex.as_str().expect("hex")).expect("hex");
    let expansion = vector("ring-transact-expansion.json");
    let approval_totals = vector("approval_totals_frame.json");
    let compact = expansion["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| frame(&case["compact"]));
    let with_totals = approval_totals["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .flat_map(|case| [frame(&case["base"]), frame(&case["frame"])]);
    compact.chain(with_totals).collect()
}

/// Both outcomes, where `parse` alone and `parse` then `ParsedTransact::new` differ in value or refusal.
fn disagreement(frame: &[u8], rail: RingRail) -> Option<String> {
    let parsed = RingTransactData::parse(frame, rail);
    let revalidated = parsed
        .clone()
        .and_then(|parsed| ParsedTransact::new(parsed.into_data()));
    (parsed != revalidated).then(|| {
        let (parsed, revalidated) = (parsed.map(drop), revalidated.map(drop));
        format!("{rail} parse {parsed:?}, revalidated {revalidated:?}")
    })
}

#[derive(Debug)]
enum Mutation {
    Truncated(usize),
    Changed { at: usize, value: u8 },
}

impl Mutation {
    fn all(len: usize) -> impl Iterator<Item = Self> {
        let changed =
            (0..len).flat_map(|at| (0..=u8::MAX).map(move |value| Self::Changed { at, value }));
        (0..len).map(Self::Truncated).chain(changed)
    }

    fn apply(&self, frame: &[u8]) -> Option<Vec<u8>> {
        match *self {
            Self::Truncated(len) => Some(frame[..len].to_vec()),
            Self::Changed { at, value } => (frame[at] != value).then(|| {
                let mut mutated = frame.to_vec();
                mutated[at] = value;
                mutated
            }),
        }
    }
}

#[test]
fn every_single_byte_change_and_truncation_parses_as_parse_and_validate() {
    let frames = frames();
    assert!(frames.iter().all(|frame| RAILS
        .iter()
        .any(|rail| RingTransactData::parse(frame, *rail).is_ok())));
    let disagreements: Vec<String> = std::thread::scope(|scope| {
        let sweeps: Vec<_> = frames
            .iter()
            .enumerate()
            .map(|(index, frame)| {
                scope.spawn(move || {
                    Mutation::all(frame.len())
                        .filter_map(|mutation| Some((mutation.apply(frame)?, mutation)))
                        .flat_map(|(mutated, mutation)| {
                            RAILS
                                .iter()
                                .filter_map(|rail| disagreement(&mutated, *rail))
                                .map(|found| format!("frame {index} {mutation:?} {found}"))
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        sweeps
            .into_iter()
            .flat_map(|sweep| sweep.join().expect("sweep"))
            .collect()
    });
    assert!(disagreements.is_empty(), "{disagreements:#?}");
}

/// A policy ring, its one source served by `curator` or by its own records.
fn policy_ring(curator: Option<Address>) -> RingPolicy {
    let mut ring = RingPolicy::zeroed();
    ring.policy_hash = [1; 32];
    ring.records = Address::new_from_array([2; 32]);
    ring.sources[0] = SourceSlot {
        list_id: 1,
        registry: curator.unwrap_or_default(),
    };
    ring
}

/// The first refusal of `roots` over registries that serve no root.
fn roots_refusal(frame: &[u8], rail: RingRail, ring: &RingPolicy) -> CustomRingError {
    let parsed = RingTransactData::parse(frame, rail).expect("the frame parses");
    let tracking = TrackingRegistry::zeroed();
    let live = LiveRoots {
        own: None,
        curator: None,
        tracking: &tracking,
        tracking_floor: Revision::default(),
        slot: 0,
    };
    parsed
        .data()
        .roots(ring, &live)
        .expect_err("no registry serves a root")
}

#[test]
fn a_curator_revision_names_the_ring_s_curator_exactly() {
    let curator = Some(Address::new_from_array([3; 32]));
    let (rail, with_curator) = expansion_case("confidential_policy");
    assert_eq!(
        roots_refusal(&with_curator, rail, &policy_ring(None)),
        CustomRingError::FlagNotAllowed
    );
    assert_eq!(
        roots_refusal(&with_curator, rail, &policy_ring(curator)),
        CustomRingError::RootNotServed
    );
    let (rail, without_curator) = expansion_case("anonymous_policy");
    assert_eq!(
        roots_refusal(&without_curator, rail, &policy_ring(curator)),
        CustomRingError::FlagNotAllowed
    );
    assert_eq!(
        roots_refusal(&without_curator, rail, &policy_ring(None)),
        CustomRingError::RootNotServed
    );
}

#[test]
fn a_delegate_frame_carries_no_approval_bit() {
    /// Byte 4, past the flags and the header.
    const APPROVAL_BYTE: usize = 4;
    let (rail, mut frame) = expansion_case("delegate_move");
    let ring = policy_ring(None);
    assert_eq!(
        roots_refusal(&frame, rail, &ring),
        CustomRingError::RootNotServed
    );
    frame[APPROVAL_BYTE] = 1;
    assert_eq!(
        roots_refusal(&frame, rail, &ring),
        CustomRingError::UnexpectedApproval
    );
}
