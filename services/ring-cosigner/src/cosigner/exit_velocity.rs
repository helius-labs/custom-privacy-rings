//! An example rule. It keeps what each owner moves out of the ring of an asset at or below a cap
//! in a window of slots.
//!
//! An exit is an output to the default ring and a withdrawal is a public leg. The frame read
//! gives `TransferRead::leaving`, what leaves the ring of each asset as the sum of its exit
//! outputs and its public withdrawal legs, and each owner's `AssetFlow::charged_outflow`, the
//! amount `zolana_ring_policy::owner_debits` computes. `owner_debits` returns neither an exit nor
//! a withdrawal to an owner, so an owner's part of what leaves lies in its charged outflow. The
//! rule counts the smaller of the two for each owner, and a frame that pays only inside the ring
//! counts nothing. The read gives no exit to one owner, so in a frame of several paying owners
//! each one counts up to what leaves of the asset, and no owner counts less than its part. What a
//! deposit of the same frame funds past the owners' notes counts for no owner.
//!
//! A window starts at the slot of the admission, a landing does not move it. The reservations
//! stay in memory. After a restart every owner has its full cap again, and replicas do not share
//! a count. A failed settlement takes back one admission of its message and the last one frees
//! the reservation, a confirmation keeps it. The rule reads no ring history, so a wallet's own
//! send counts until its window passes, and one transfer under a second blockhash counts twice.
//!
//! The service's chain does not hold the rule. One `then` adds it ahead of the window rule, the
//! window rule runs last.
//!
//! ```ignore
//! CoSigner::new(key, list_rule.then(ExitVelocity::new(limits, caps)).then(window))
//! ```

use std::{
    collections::BTreeMap,
    num::NonZeroU64,
    sync::{Mutex, MutexGuard, PoisonError},
};

use async_trait::async_trait;
use solana_address::Address;
use zolana_ring_policy::Member;

use super::rule::{CoSignRule, Operation, Verdict};
use crate::{
    cosign::{CoSignFlows, CoSignRefusal, CoSignSettlement, Ticket},
    cosign_limits::{AssetLimitSource, FrameRead, TransferRead},
    error::RingCosignerError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitCap {
    pub asset: Address,
    /// In the asset's base units for each owner in each ring, zero lets none out.
    pub amount: u64,
    /// A zero window counts the checked frame alone.
    pub window_slots: u64,
}

pub struct ExitVelocity<L> {
    source: L,
    caps: Vec<ExitCap>,
    reservations: Mutex<Reservations>,
}

#[derive(Default)]
struct Reservations {
    /// A slot read behind it counts at it.
    newest_slot: u64,
    held: BTreeMap<Ticket, Reservation>,
}

struct Reservation {
    ring: Address,
    slot: u64,
    state: State,
    exits: Vec<Exit>,
}

enum State {
    /// Each failure takes one admission back, the last one frees the reservation.
    Held { admissions: NonZeroU64 },
    /// It counts until its window passes.
    Confirmed,
}

/// What one owner counts of one asset in one frame.
struct Exit {
    owner: Member,
    asset: Address,
    amount: u128,
}

#[must_use]
struct ExitDecision<'a> {
    caps: &'a [ExitCap],
    ring: Address,
    slot: u64,
    exits: &'a [Exit],
    held: &'a BTreeMap<Ticket, Reservation>,
}

impl<L: AssetLimitSource> ExitVelocity<L> {
    pub fn new(source: L, caps: impl IntoIterator<Item = ExitCap>) -> Self {
        Self {
            source,
            caps: caps.into_iter().collect(),
            reservations: Mutex::default(),
        }
    }
}

#[async_trait]
impl<L: AssetLimitSource> CoSignRule for ExitVelocity<L> {
    async fn admit(&self, operation: &Operation<'_>) -> Result<Verdict, RingCosignerError> {
        // 1. Only a frame read names what leaves and whose notes pay. A deposit spends no note, a
        // merge returns its inputs to their owner, and no cap reads a delegate move.
        let read = match operation.flows {
            CoSignFlows::Published(_) => return Ok(Verdict::Admitted),
            CoSignFlows::Frame => match self
                .source
                .transfer_flows(operation.ring, operation.frame_source())
                .await?
            {
                FrameRead::Delegate => return Ok(Verdict::Admitted),
                FrameRead::Transfer(read) => read,
            },
        };
        // 2. A frame with no amount a cap reads holds nothing.
        let exits = self.exits(&read);
        if exits.is_empty() {
            return Ok(Verdict::Admitted);
        }
        // 3. One slot is read ahead of the lock, no lock spans a network call. A slot read behind
        // the newest one counts at the newest.
        let slot = self.source.current_slot().await?;
        let mut reservations = self.reservations();
        reservations.newest_slot = reservations.newest_slot.max(slot);
        let slot = reservations.newest_slot;
        // 4. A repeat signs under its reservation and counts once. It holds the reservation once
        // more, a later rule that fails the repeat takes back that admission alone while the
        // first signature is out.
        if reservations.readmit(&operation.ticket) {
            return Ok(Verdict::Admitted);
        }
        // 5. No cap reads a reservation past the longest window.
        let longest_window = self
            .caps
            .iter()
            .map(|cap| cap.window_slots)
            .max()
            .unwrap_or_default();
        reservations
            .held
            .retain(|_, reservation| reservation.age(slot) < longest_window);
        // 6. The decision and the reservation hold one lock, and a refusal reserves nothing.
        let verdict = ExitDecision {
            caps: &self.caps,
            ring: operation.ring,
            slot,
            exits: &exits,
            held: &reservations.held,
        }
        .decide();
        if verdict == Verdict::Admitted {
            let reservation = Reservation {
                ring: operation.ring,
                slot,
                state: State::Held {
                    admissions: NonZeroU64::MIN,
                },
                exits,
            };
            reservations.held.insert(operation.ticket, reservation);
        }
        Ok(verdict)
    }

    async fn settle(&self, settlement: CoSignSettlement) -> Result<(), RingCosignerError> {
        let mut reservations = self.reservations();
        match settlement {
            CoSignSettlement::Confirmed(ticket) => reservations.confirm(&ticket),
            CoSignSettlement::Failed(ticket) => reservations.take_back(&ticket),
        }
        Ok(())
    }
}

impl<L> ExitVelocity<L> {
    /// Each owner's charged outflow of a capped asset, up to what leaves the ring of the asset.
    fn exits(&self, read: &TransferRead) -> Vec<Exit> {
        read.flows
            .iter()
            .filter(|flow| self.caps.iter().any(|cap| cap.asset == flow.flow.asset))
            .filter_map(|flow| {
                let leaving = read.leaving.get(&flow.flow.asset)?;
                let amount = flow.flow.charged_outflow.min(*leaving);
                (amount != 0).then_some(Exit {
                    owner: flow.owner,
                    asset: flow.flow.asset,
                    amount,
                })
            })
            .collect()
    }

    fn reservations(&self) -> MutexGuard<'_, Reservations> {
        self.reservations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl Reservations {
    /// Whether the message holds a reservation, held once more from here on.
    fn readmit(&mut self, ticket: &Ticket) -> bool {
        let Some(repeated) = self.held.get_mut(ticket) else {
            return false;
        };
        if let State::Held { admissions } = &mut repeated.state {
            *admissions = admissions.saturating_add(1);
        }
        true
    }

    fn confirm(&mut self, ticket: &Ticket) {
        if let Some(confirmed) = self.held.get_mut(ticket) {
            confirmed.state = State::Confirmed;
        }
    }

    fn take_back(&mut self, ticket: &Ticket) {
        let Some(failed) = self.held.get_mut(ticket) else {
            return;
        };
        let State::Held { admissions } = &mut failed.state else {
            return;
        };
        match NonZeroU64::new(admissions.get() - 1) {
            Some(left) => *admissions = left,
            None => {
                self.held.remove(ticket);
            }
        }
    }
}

impl ExitDecision<'_> {
    fn decide(self) -> Verdict {
        let within = self.caps.iter().all(|cap| {
            self.exits
                .iter()
                .filter(|exit| exit.asset == cap.asset)
                .all(|exit| {
                    self.total(cap, exit.owner)
                        .is_some_and(|total| total <= u128::from(cap.amount))
                })
        });
        match within {
            true => Verdict::Admitted,
            false => Verdict::Refused(CoSignRefusal::OverLimit),
        }
    }

    /// With the frame's own exits, `None` past a u128 and so past every cap.
    fn total(&self, cap: &ExitCap, owner: Member) -> Option<u128> {
        self.held
            .values()
            .filter(|reservation| {
                reservation.ring == self.ring && reservation.age(self.slot) < cap.window_slots
            })
            .flat_map(|reservation| &reservation.exits)
            .chain(self.exits)
            .filter(|exit| exit.owner == owner && exit.asset == cap.asset)
            .try_fold(0u128, |total, exit| total.checked_add(exit.amount))
    }
}

impl Reservation {
    const fn age(&self, slot: u64) -> u64 {
        slot.saturating_sub(self.slot)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };

    use custom_ring_interface::CoSignScope;
    use ring_audit::utxos::SpendRead;
    use solana_keypair::Keypair;
    use solana_message::{Message, VersionedMessage};
    use solana_signer::Signer;
    use zolana_ring_policy::{DebitInput, DebitOutput, MintPolicy};

    use super::*;
    use crate::{
        cosign::{
            AssetFlow, CoSignDecision, CoSignPolicy, CoSignRequest, FrameSource, OwnerFlow, Sender,
        },
        cosigner::CoSigner,
        transfer_debits::{OwnerFlows, PublicFlow},
    };

    const RING: Address = Address::new_from_array([9; 32]);
    const SOL: Address = Address::new_from_array([3; 32]);
    const SOL_FIELD: [u8; 32] = [3; 32];
    const USDC: Address = Address::new_from_array([4; 32]);
    const UNCAPPED: Address = Address::new_from_array([5; 32]);
    const CAP: u64 = 100;
    const WINDOW: u64 = 50;
    const LONG_WINDOW: u64 = 2 * WINDOW;
    const START_SLOT: u64 = 1_000;

    /// The slot and the frame move under a running rule.
    struct Frames {
        slot: AtomicU64,
        frame: Mutex<FrameRead>,
    }

    impl AssetLimitSource for Frames {
        async fn current_slot(&self) -> Result<u64, RingCosignerError> {
            Ok(self.slot.load(Ordering::Relaxed))
        }

        async fn asset_policies(
            &self,
            _ring: Address,
            _assets: &[Address],
        ) -> Result<Vec<Option<MintPolicy>>, RingCosignerError> {
            unreachable!("the rule reads no asset record")
        }

        async fn co_signer_scope(
            &self,
            _ring: Address,
            _signer: Address,
        ) -> Result<CoSignScope, RingCosignerError> {
            unreachable!("the rule reads no co-signer record")
        }

        async fn transfer_flows(
            &self,
            _ring: Address,
            _frame: FrameSource<'_>,
        ) -> Result<FrameRead, RingCosignerError> {
            Ok(self.frame.lock().expect("frame").clone())
        }

        async fn spend_read(
            &self,
            _ring: Address,
            _nullifiers: &[[u8; 32]],
        ) -> Result<SpendRead, RingCosignerError> {
            unreachable!("the rule reads no ring history")
        }
    }

    struct Exits {
        co_signer: CoSigner<ExitVelocity<Frames>>,
        signer: Address,
    }

    impl Exits {
        fn open() -> Self {
            Self::capped(
                [(SOL, WINDOW), (USDC, LONG_WINDOW)].map(|(asset, window_slots)| ExitCap {
                    asset,
                    amount: CAP,
                    window_slots,
                }),
            )
        }

        fn capped(caps: impl IntoIterator<Item = ExitCap>) -> Self {
            let signer = Keypair::new_from_array([5; 32]);
            let frames = Frames {
                slot: AtomicU64::new(START_SLOT),
                frame: Mutex::new(FrameRead::Delegate),
            };
            Self {
                signer: signer.pubkey(),
                co_signer: CoSigner::new(Arc::new(signer), ExitVelocity::new(frames, caps)),
            }
        }

        fn at(&self, slot: u64) {
            self.co_signer
                .rule
                .source
                .slot
                .store(slot, Ordering::Relaxed);
        }

        /// The decision on `request` while the frame read returns `frame`.
        async fn decide(&self, request: &CoSignRequest, frame: FrameRead) -> CoSignDecision {
            *self.co_signer.rule.source.frame.lock().expect("frame") = frame;
            self.co_signer.decide(request).await.expect("decision")
        }

        async fn admits(&self, nonce: u8, frame: FrameRead) -> Ticket {
            let request = request(nonce);
            let decision = self.decide(&request, frame).await;
            assert!(
                matches!(decision, CoSignDecision::Signed(_)),
                "{decision:?}"
            );
            Ticket::of(&self.signer, &request.message)
        }

        async fn signs(&self, nonce: u8, flows: Vec<OwnerFlow>) -> Ticket {
            self.admits(nonce, exiting(flows)).await
        }

        async fn refuses(&self, nonce: u8, flows: Vec<OwnerFlow>) {
            assert_eq!(
                self.decide(&request(nonce), exiting(flows)).await,
                CoSignDecision::Refused(CoSignRefusal::OverLimit)
            );
        }

        async fn settle(&self, settlement: CoSignSettlement) {
            self.co_signer.settle(settlement).await.expect("settle");
        }

        fn held(&self) -> usize {
            self.co_signer.rule.reservations().held.len()
        }
    }

    /// A frame as its openings, read as the frame read computes its rows and what leaves.
    #[derive(Default)]
    struct Opened {
        inputs: Vec<DebitInput>,
        outputs: Vec<DebitOutput>,
        public: Vec<PublicFlow>,
    }

    impl Opened {
        fn read(self) -> FrameRead {
            let moved = OwnerFlows {
                inputs: &self.inputs,
                outputs: &self.outputs,
                owner_signers: &[],
                public: &self.public,
                no_spend_owner: Member::from_bytes([0xee; 32]).expect("tag"),
            };
            let leaving = moved.leaving(|_| Ok(SOL)).expect("leaving");
            transfer(moved.rows(|_| Ok(SOL)).expect("rows"), leaving)
        }

        /// The frame takes `counted` of owner 7's cap and no unit more.
        async fn counts(self, counted: u64) {
            let exits = Exits::open();
            exits.admits(1, self.read()).await;
            exits.signs(2, vec![exit(7, (CAP - counted).into())]).await;
            exits.refuses(3, vec![exit(7, 1)]).await;
        }
    }

    fn request(nonce: u8) -> CoSignRequest {
        CoSignRequest {
            ring: RING,
            message: VersionedMessage::Legacy(Message {
                account_keys: vec![Address::new_from_array([nonce; 32])],
                ..Message::default()
            }),
            flows: CoSignFlows::Frame,
            approved_transfer: None,
            sender: Sender::Relay,
        }
    }

    fn transfer(flows: Vec<OwnerFlow>, leaving: BTreeMap<Address, u128>) -> FrameRead {
        FrameRead::Transfer(TransferRead {
            flows,
            leaving,
            spends_note: true,
            approval_bits: None,
            co_signers: Vec::new(),
            nullifiers: Vec::new(),
        })
    }

    /// A read whose rows all leave the ring.
    fn exiting(flows: Vec<OwnerFlow>) -> FrameRead {
        let mut leaving = BTreeMap::<Address, u128>::new();
        for flow in flows.iter().filter(|flow| flow.flow.charged_outflow != 0) {
            let total = leaving.entry(flow.flow.asset).or_default();
            *total = total.saturating_add(flow.flow.charged_outflow);
        }
        transfer(flows, leaving)
    }

    /// A row of SOL whose charged outflow is its debit.
    fn exit(owner: u8, amount: u128) -> OwnerFlow {
        OwnerFlow {
            owner: Member::from_bytes([owner; 32]).expect("member"),
            flow: AssetFlow {
                asset: SOL,
                debit: amount,
                charged_outflow: amount,
                deposited: 0,
                withdrawn: 0,
            },
        }
    }

    fn in_asset(asset: Address, row: OwnerFlow) -> OwnerFlow {
        OwnerFlow {
            flow: AssetFlow { asset, ..row.flow },
            ..row
        }
    }

    fn spent(owner: u8, amount: u64) -> DebitInput {
        DebitInput {
            owner: [owner; 32],
            asset: SOL_FIELD,
            amount,
        }
    }

    fn paid(owner: u8, amount: u64) -> DebitOutput {
        DebitOutput {
            ring: true,
            owner: [owner; 32],
            asset: SOL_FIELD,
            amount,
        }
    }

    fn exited(owner: u8, amount: u64) -> DebitOutput {
        DebitOutput {
            ring: false,
            ..paid(owner, amount)
        }
    }

    fn withdrawn(amount: u128) -> PublicFlow {
        PublicFlow {
            mint: SOL,
            asset: SOL_FIELD,
            deposited: 0,
            withdrawn: amount,
        }
    }

    #[tokio::test]
    async fn an_owner_exits_up_to_the_cap_and_no_unit_past_it() {
        let exits = Exits::open();
        exits.signs(1, vec![exit(7, 60)]).await;
        exits
            .refuses(2, vec![exit(8, CAP.into()), exit(7, 41)])
            .await;
        exits.signs(3, vec![exit(8, CAP.into()), exit(7, 40)]).await;
        exits.refuses(4, vec![exit(7, 1)]).await;
        exits.refuses(5, vec![exit(7, u128::MAX)]).await;
    }

    #[tokio::test]
    async fn a_failure_frees_the_reservation_and_a_confirmation_keeps_it() {
        let exits = Exits::open();
        let failed = exits.signs(1, vec![exit(7, CAP.into())]).await;
        exits.refuses(2, vec![exit(7, 1)]).await;
        exits.settle(CoSignSettlement::Failed(failed)).await;

        let confirmed = exits.signs(3, vec![exit(7, CAP.into())]).await;
        exits.settle(CoSignSettlement::Confirmed(confirmed)).await;
        exits.settle(CoSignSettlement::Failed(confirmed)).await;
        exits.refuses(4, vec![exit(7, 1)]).await;
    }

    #[tokio::test]
    async fn a_repeat_counts_once_and_each_failure_takes_one_admission_back() {
        let exits = Exits::open();
        let repeated = exits.signs(1, vec![exit(7, 60)]).await;
        exits.signs(1, vec![exit(7, 60)]).await;
        exits.signs(2, vec![exit(7, 40)]).await;

        exits.settle(CoSignSettlement::Failed(repeated)).await;
        exits.refuses(3, vec![exit(7, 1)]).await;
        exits.settle(CoSignSettlement::Failed(repeated)).await;
        exits.signs(4, vec![exit(7, 60)]).await;
    }

    #[tokio::test]
    async fn an_exit_counts_until_its_own_window_passes_and_frees_past_the_longest() {
        let exits = Exits::open();
        let both = vec![exit(7, CAP.into()), in_asset(USDC, exit(7, CAP.into()))];
        exits.signs(1, both).await;
        exits.at(START_SLOT + WINDOW - 1);
        exits.refuses(2, vec![exit(7, 1)]).await;
        exits.at(START_SLOT + WINDOW);
        exits.refuses(3, vec![in_asset(USDC, exit(7, 1))]).await;
        exits.signs(4, vec![exit(7, CAP.into())]).await;

        exits.at(START_SLOT + LONG_WINDOW);
        exits
            .signs(5, vec![in_asset(USDC, exit(7, CAP.into()))])
            .await;
        assert_eq!(exits.held(), 2);
    }

    #[tokio::test]
    async fn a_slot_read_behind_the_newest_counts_at_the_newest() {
        let exits = Exits::open();
        exits.at(START_SLOT + 5);
        let freed = exits.signs(1, vec![exit(7, 1)]).await;
        exits.settle(CoSignSettlement::Failed(freed)).await;
        exits.at(START_SLOT);
        exits.signs(2, vec![exit(7, CAP.into())]).await;
        exits.at(START_SLOT + WINDOW);
        exits.refuses(3, vec![exit(7, 1)]).await;
        exits.at(START_SLOT + WINDOW + 5);
        exits.signs(4, vec![exit(7, CAP.into())]).await;
    }

    #[tokio::test]
    async fn each_owner_asset_and_ring_has_its_own_budget() {
        let exits = Exits::open();
        exits.signs(1, vec![exit(7, CAP.into())]).await;
        exits.refuses(2, vec![exit(7, 1)]).await;
        exits.signs(3, vec![exit(8, CAP.into())]).await;
        exits
            .signs(4, vec![in_asset(USDC, exit(7, CAP.into()))])
            .await;

        let other_ring = CoSignRequest {
            ring: Address::new_from_array([8; 32]),
            ..request(5)
        };
        assert!(matches!(
            exits
                .decide(&other_ring, exiting(vec![exit(7, CAP.into())]))
                .await,
            CoSignDecision::Signed(_)
        ));
    }

    #[tokio::test]
    async fn every_cap_of_an_asset_binds_and_a_zero_window_counts_the_frame_alone() {
        let exits = Exits::capped([
            ExitCap {
                asset: SOL,
                amount: CAP,
                window_slots: 0,
            },
            ExitCap {
                asset: SOL,
                amount: 150,
                window_slots: LONG_WINDOW,
            },
        ]);
        exits.signs(1, vec![exit(7, CAP.into())]).await;
        exits.signs(2, vec![exit(7, 50)]).await;
        exits.refuses(3, vec![exit(7, 1)]).await;
        exits.refuses(4, vec![exit(8, u128::from(CAP) + 1)]).await;
    }

    #[tokio::test]
    async fn a_payment_inside_the_ring_counts_nothing() {
        let exits = Exits::open();
        let payment = Opened {
            inputs: vec![spent(7, 30)],
            outputs: vec![paid(8, 30)],
            ..Opened::default()
        };
        exits.admits(1, payment.read()).await;
        assert_eq!(exits.held(), 0);
        exits.signs(2, vec![exit(7, CAP.into())]).await;
    }

    #[tokio::test]
    async fn an_exit_counts() {
        let frame = Opened {
            inputs: vec![spent(7, 30)],
            outputs: vec![exited(9, 30)],
            ..Opened::default()
        };
        frame.counts(30).await;
    }

    #[tokio::test]
    async fn a_public_withdrawal_counts() {
        let withdrawal = Opened {
            inputs: vec![spent(7, 30)],
            public: vec![withdrawn(30)],
            ..Opened::default()
        };
        withdrawal.counts(30).await;
    }

    #[tokio::test]
    async fn a_payment_beside_an_exit_counts_the_exit_alone() {
        let both = Opened {
            inputs: vec![spent(7, 100)],
            outputs: vec![paid(8, 70), exited(9, 30)],
            ..Opened::default()
        };
        both.counts(30).await;
    }

    /// Owner 7 spends 10 and owner 8 spends 50, 20 and 10 exit and 30 stay in the ring.
    #[tokio::test]
    async fn each_paying_owner_of_a_frame_counts_up_to_what_leaves() {
        let exits = Exits::open();
        let shared = Opened {
            inputs: vec![spent(7, 10), spent(8, 50)],
            outputs: vec![exited(1, 20), exited(2, 10), paid(3, 30)],
            ..Opened::default()
        };
        exits.admits(1, shared.read()).await;
        exits.signs(2, vec![exit(7, 90)]).await;
        exits.refuses(3, vec![exit(7, 1)]).await;
        exits.signs(4, vec![exit(8, 70)]).await;
        exits.refuses(5, vec![exit(8, 1)]).await;
    }

    /// The owner spends 100 and takes 100 back while a deposit funds an exit of 100, its debit is
    /// zero.
    #[tokio::test]
    async fn an_exit_beside_a_returned_spend_counts_by_its_charge() {
        let returned = Opened {
            inputs: vec![spent(7, 100)],
            outputs: vec![paid(7, 100), exited(8, 100)],
            public: vec![PublicFlow {
                deposited: 100,
                ..withdrawn(0)
            }],
        };
        let frame = returned.read();
        let FrameRead::Transfer(read) = &frame else {
            panic!("a transfer, {frame:?}");
        };
        assert_eq!(
            (read.flows[0].flow.debit, read.flows[0].flow.charged_outflow),
            (0, 100)
        );

        let exits = Exits::open();
        exits.admits(1, frame).await;
        exits.refuses(2, vec![exit(7, 1)]).await;
    }

    #[tokio::test]
    async fn what_no_cap_reads_passes_and_holds_nothing() {
        let exits = Exits::open();
        let named = CoSignRequest {
            flows: CoSignFlows::Published(vec![exit(7, u128::from(CAP) + 1)]),
            ..request(1)
        };
        let uncapped = exiting(vec![in_asset(UNCAPPED, exit(7, u128::MAX))]);
        let public_leg_alone = Opened {
            public: vec![withdrawn(30)],
            ..Opened::default()
        };
        for (asked, frame) in [
            (named, FrameRead::Delegate),
            (request(2), FrameRead::Delegate),
            (request(3), uncapped),
            (request(4), public_leg_alone.read()),
        ] {
            let decision = exits.decide(&asked, frame).await;
            assert!(
                matches!(decision, CoSignDecision::Signed(_)),
                "{decision:?}"
            );
        }
        assert_eq!(exits.held(), 0);
        exits.signs(5, vec![exit(7, CAP.into())]).await;
    }
}
