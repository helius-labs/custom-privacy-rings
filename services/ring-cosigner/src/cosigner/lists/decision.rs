use std::collections::HashSet;

use solana_address::Address;
pub use zolana_ring_client::rpc::{CoSignList as List, CoSignSubjectKind as SubjectKind};
use zolana_ring_policy::Member;

use crate::{cosign::CoSignRefusal, cosigner::rule::Verdict};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Subject {
    pub(crate) kind: SubjectKind,
    pub(crate) id: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListEntry {
    pub(crate) list: List,
    pub(crate) subject: Subject,
}

/// Who and what one operation moves, as the frame the co-signer signs states it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parties {
    pub(crate) senders: Vec<Member>,
    pub(crate) recipients: Vec<Member>,
    /// Every account of a public withdrawal leg.
    pub(crate) destinations: Vec<Address>,
    pub(crate) assets: Vec<Address>,
}

/// The entries of one ring that name a subject of the operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lists {
    pub(crate) entries: HashSet<ListEntry>,
    /// The kinds that hold an allow entry for any subject.
    pub(crate) allowing: HashSet<SubjectKind>,
}

#[must_use]
pub struct ListDecision<'a> {
    pub(crate) parties: &'a Parties,
    pub(crate) lists: &'a Lists,
}

impl Subject {
    pub(crate) const fn owner(owner: Member) -> Self {
        Self {
            kind: SubjectKind::Owner,
            id: *owner.as_bytes(),
        }
    }

    pub(crate) const fn asset(mint: Address) -> Self {
        Self {
            kind: SubjectKind::Asset,
            id: mint.to_bytes(),
        }
    }

    pub(crate) const fn destination(account: Address) -> Self {
        Self {
            kind: SubjectKind::Destination,
            id: account.to_bytes(),
        }
    }
}

impl Parties {
    /// What a freeze stops.
    fn outgoing(&self) -> impl Iterator<Item = Subject> + '_ {
        let senders = self.senders.iter().copied().map(Subject::owner);
        senders.chain(self.assets.iter().copied().map(Subject::asset))
    }

    pub(crate) fn subjects(&self) -> impl Iterator<Item = Subject> + '_ {
        let recipients = self.recipients.iter().copied().map(Subject::owner);
        let destinations = self.destinations.iter().copied().map(Subject::destination);
        self.outgoing().chain(recipients).chain(destinations)
    }
}

impl ListDecision<'_> {
    pub(crate) fn evaluate(self) -> Verdict {
        let listed = |list, subject| self.lists.entries.contains(&ListEntry { list, subject });
        let parties = self.parties;
        if parties
            .outgoing()
            .any(|subject| listed(List::Freeze, subject))
        {
            return Verdict::Refused(CoSignRefusal::Frozen);
        }
        if parties
            .subjects()
            .any(|subject| listed(List::Block, subject))
        {
            return Verdict::Refused(CoSignRefusal::Blocked);
        }
        let unlisted = |subject: Subject| {
            self.lists.allowing.contains(&subject.kind) && !listed(List::Allow, subject)
        };
        match parties.subjects().any(unlisted) {
            true => Verdict::Refused(CoSignRefusal::NotAllowed),
            false => Verdict::Admitted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner(seed: u8) -> Member {
        Member::owner_tag(&[seed; 32]).expect("owner tag")
    }

    const MINT: Address = Address::new_from_array([7; 32]);
    const ACCOUNT: Address = Address::new_from_array([8; 32]);

    fn payment() -> Parties {
        Parties {
            senders: vec![owner(1)],
            recipients: vec![owner(2)],
            destinations: vec![ACCOUNT],
            assets: vec![MINT],
        }
    }

    fn verdict(entries: &[(List, Subject)]) -> Verdict {
        let lists = Lists {
            entries: entries
                .iter()
                .map(|(list, subject)| ListEntry {
                    list: *list,
                    subject: *subject,
                })
                .collect(),
            allowing: entries
                .iter()
                .filter(|(list, _)| *list == List::Allow)
                .map(|(_, subject)| subject.kind)
                .collect(),
        };
        ListDecision {
            parties: &payment(),
            lists: &lists,
        }
        .evaluate()
    }

    #[test]
    fn empty_lists_admit() {
        assert_eq!(verdict(&[]), Verdict::Admitted);
    }

    #[test]
    fn a_freeze_stops_the_sender_and_the_asset_and_spares_the_recipient() {
        let frozen = Verdict::Refused(CoSignRefusal::Frozen);
        assert_eq!(verdict(&[(List::Freeze, Subject::owner(owner(1)))]), frozen);
        assert_eq!(verdict(&[(List::Freeze, Subject::asset(MINT))]), frozen);
        assert_eq!(
            verdict(&[(List::Freeze, Subject::owner(owner(2)))]),
            Verdict::Admitted
        );
    }

    #[test]
    fn a_block_stops_every_named_subject() {
        for subject in [
            Subject::owner(owner(1)),
            Subject::owner(owner(2)),
            Subject::asset(MINT),
            Subject::destination(ACCOUNT),
        ] {
            assert_eq!(
                verdict(&[(List::Block, subject)]),
                Verdict::Refused(CoSignRefusal::Blocked)
            );
        }
    }

    #[test]
    fn an_allow_entry_closes_its_kind_to_the_rest() {
        let sender = (List::Allow, Subject::owner(owner(1)));
        let recipient = (List::Allow, Subject::owner(owner(2)));
        assert_eq!(
            verdict(&[sender]),
            Verdict::Refused(CoSignRefusal::NotAllowed)
        );
        assert_eq!(verdict(&[sender, recipient]), Verdict::Admitted);
        assert_eq!(
            verdict(&[sender, recipient, (List::Block, Subject::asset(MINT))]),
            Verdict::Refused(CoSignRefusal::Blocked)
        );
    }
}
