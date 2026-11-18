use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use solana_address::Address;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use zolana_ring_policy::Member;

use super::*;
use crate::{
    cosign::{CoSignFlows, CoSignRefusal},
    database::Store,
};

const RING: Address = Address::new_from_array([9; 32]);
const MINT: Address = Address::new_from_array([7; 32]);

fn request() -> CoSignRequest {
    CoSignRequest {
        ring: RING,
        message: VersionedMessage::Legacy(Message::default()),
        flows: CoSignFlows::Frame,
    }
}

fn owner(seed: u8) -> Member {
    Member::owner_tag(&[seed; 32]).expect("owner tag")
}

/// A rule the service does not know, as an operator adds one.
struct Recorded {
    verdict: Verdict,
    settled: Arc<Mutex<Vec<CoSignSettlement>>>,
}

#[async_trait]
impl CoSignRule for Recorded {
    async fn admit(&self, _operation: &Operation<'_>) -> Result<Verdict, RingRpcError> {
        Ok(self.verdict)
    }

    async fn settle(&self, settlement: CoSignSettlement) -> Result<(), RingRpcError> {
        self.settled.lock().expect("settled").push(settlement);
        Ok(())
    }
}

fn recorded(verdict: Verdict) -> (Recorded, Arc<Mutex<Vec<CoSignSettlement>>>) {
    let settled = Arc::new(Mutex::new(Vec::new()));
    let rule = Recorded {
        verdict,
        settled: settled.clone(),
    };
    (rule, settled)
}

struct Named(Parties);

impl PartySource for Named {
    async fn parties(
        &self,
        _ring: Address,
        _message: &VersionedMessage,
    ) -> Result<FrameParties, RingRpcError> {
        Ok(FrameParties::Transfer(self.0.clone()))
    }
}

struct DelegateFrame;

impl PartySource for DelegateFrame {
    async fn parties(
        &self,
        _ring: Address,
        _message: &VersionedMessage,
    ) -> Result<FrameParties, RingRpcError> {
        Ok(FrameParties::Delegate)
    }
}

fn payment() -> Parties {
    Parties {
        senders: vec![owner(1)],
        recipients: vec![owner(2)],
        destinations: Vec::new(),
        assets: vec![MINT],
    }
}

async fn lists(entries: &[(Address, ListEntry)]) -> StoredLists {
    let store = Store::open("sqlite::memory:", 1).await.expect("store");
    let lists = StoredLists(Arc::new(store));
    for (ring, entry) in entries {
        lists.set(*ring, *entry).await.expect("set");
    }
    lists
}

fn entry(list: List, subject: Subject) -> ListEntry {
    ListEntry { list, subject }
}

#[tokio::test]
async fn an_admitted_message_carries_the_signers_signature() {
    let signer = Keypair::new_from_array([5; 32]);
    let key = signer.pubkey();
    let (rule, _) = recorded(Verdict::Admitted);
    let co_signer = CoSigner::new(Arc::new(signer), rule);
    let request = request();
    let CoSignDecision::Signed(signature) = co_signer.decide(&request).await.expect("decision")
    else {
        panic!("refused");
    };
    assert!(signature.verify(key.as_array(), &request.message.serialize()));
}

#[tokio::test]
async fn a_later_refusal_releases_the_earlier_rule_and_signs_nothing() {
    let (reserving, released) = recorded(Verdict::Admitted);
    let (refusing, _) = recorded(Verdict::Refused(CoSignRefusal::Custom));
    let signer = Keypair::new_from_array([5; 32]);
    let request = request();
    let ticket = signer.sign_message(&request.message.serialize());
    let decision = CoSigner::new(Arc::new(signer), reserving.then(refusing))
        .decide(&request)
        .await
        .expect("decision");
    assert_eq!(decision, CoSignDecision::Refused(CoSignRefusal::Custom));
    assert_eq!(
        *released.lock().expect("released"),
        vec![CoSignSettlement::Failed(ticket)]
    );
}

#[tokio::test]
async fn an_earlier_refusal_stops_the_chain() {
    let (refusing, _) = recorded(Verdict::Refused(CoSignRefusal::Custom));
    let (later, settled) = recorded(Verdict::Refused(CoSignRefusal::OverLimit));
    let decision = CoSigner::new(Arc::new(Keypair::new()), refusing.then(later))
        .decide(&request())
        .await
        .expect("decision");
    assert_eq!(decision, CoSignDecision::Refused(CoSignRefusal::Custom));
    assert!(settled.lock().expect("settled").is_empty());
}

#[tokio::test]
async fn an_absent_rule_admits_and_a_settlement_reaches_every_rule() {
    let (first, first_settled) = recorded(Verdict::Admitted);
    let (last, last_settled) = recorded(Verdict::Admitted);
    let absent: Option<Arc<dyn CoSignRule>> = None;
    let co_signer = CoSigner::new(Arc::new(Keypair::new()), first.then(absent).then(last));
    let CoSignDecision::Signed(signature) = co_signer.decide(&request()).await.expect("decision")
    else {
        panic!("refused");
    };
    let confirmed = CoSignSettlement::Confirmed(signature);
    co_signer.settle(confirmed).await.expect("settle");
    for settled in [first_settled, last_settled] {
        assert_eq!(*settled.lock().expect("settled"), vec![confirmed]);
    }
}

async fn listed(entries: &[(Address, ListEntry)]) -> CoSignDecision {
    let rule = ListRule::new(Named(payment()), lists(entries).await);
    CoSigner::new(Arc::new(Keypair::new()), rule)
        .decide(&request())
        .await
        .expect("decision")
}

#[tokio::test]
async fn stored_lists_sign_or_refuse_by_list() {
    let sender = Subject::owner(owner(1));
    let recipient = Subject::owner(owner(2));
    assert!(matches!(listed(&[]).await, CoSignDecision::Signed(_)));
    for (list, subject, refusal) in [
        (List::Freeze, sender, CoSignRefusal::Frozen),
        (List::Block, recipient, CoSignRefusal::Blocked),
        (List::Block, Subject::asset(MINT), CoSignRefusal::Blocked),
        (List::Allow, sender, CoSignRefusal::NotAllowed),
    ] {
        assert_eq!(
            listed(&[(RING, entry(list, subject))]).await,
            CoSignDecision::Refused(refusal)
        );
    }
    // A frozen recipient still receives, an allow list that names both owners admits.
    let admitted: [&[(Address, ListEntry)]; 2] = [
        &[(RING, entry(List::Freeze, recipient))],
        &[
            (RING, entry(List::Allow, sender)),
            (RING, entry(List::Allow, recipient)),
        ],
    ];
    for entries in admitted {
        assert!(matches!(listed(entries).await, CoSignDecision::Signed(_)));
    }
}

#[tokio::test]
async fn another_rings_entry_and_a_removed_entry_refuse_nothing() {
    let other = Address::new_from_array([8; 32]);
    let block = entry(List::Block, Subject::owner(owner(1)));
    assert!(matches!(
        listed(&[(other, block)]).await,
        CoSignDecision::Signed(_)
    ));
    let lists = lists(&[(RING, block)]).await;
    assert_eq!(
        lists.entries(RING, None, 10).await.expect("entries"),
        [block]
    );
    lists.remove(RING, block).await.expect("remove");
    assert!(lists
        .entries(RING, None, 10)
        .await
        .expect("entries")
        .is_empty());
}

#[tokio::test]
async fn a_delegate_frame_passes_the_lists() {
    let block = entry(List::Block, Subject::owner(owner(1)));
    let rule = ListRule::new(DelegateFrame, lists(&[(RING, block)]).await);
    let decision = CoSigner::new(Arc::new(Keypair::new()), rule)
        .decide(&request())
        .await
        .expect("decision");
    assert!(matches!(decision, CoSignDecision::Signed(_)));
}

#[tokio::test]
async fn the_entries_page_in_order_after_a_cursor() {
    let stored = [
        entry(List::Block, Subject::asset(MINT)),
        entry(List::Freeze, Subject::asset(MINT)),
        entry(List::Block, Subject::owner(owner(1))),
    ];
    let lists = lists(&stored.map(|entry| (RING, entry))).await;
    let all = lists.entries(RING, None, 10).await.expect("entries");
    assert_eq!(all.len(), stored.len());

    let first = lists.entries(RING, None, 2).await.expect("first page");
    let rest = lists
        .entries(RING, first.last().copied(), 2)
        .await
        .expect("second page");
    assert_eq!([first, rest].concat(), all);
}
