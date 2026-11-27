mod frames;

use custom_ring_interface::{
    shape_flag, transact_flag, CustomRingError, ParsedTransact, RailAuthorization, Replies,
    RingRail, RingTransactData, MIN_REPLY_LEN,
};
use frames::Fixture;
use zolana_ring_client::FrameEncode;

const RAILS: [RingRail; 4] = [
    RingRail::Confidential,
    RingRail::Anonymous,
    RingRail::Delegate,
    RingRail::AnonymousDelegate,
];

#[test]
fn checked_frames_accept_the_supported_authorization_modes() {
    let fixture = Fixture::new();
    for (name, data) in fixture.cases() {
        assert!(ParsedTransact::new(data).is_ok(), "{name}");
    }
}

/// A frame with none of the three flag bits parses under the confidential and both delegate rails.
#[test]
fn each_authorization_encodes_its_flags_and_parses_back_under_its_rail() {
    let fixture = Fixture::new();
    let flag = Err(CustomRingError::FlagNotAllowed);
    let length = Err(CustomRingError::RingTransactLength);
    let own = |data: &RingTransactData<'_>| Ok(data.authorization);
    let (signer, in_proof, hidden, public, delegate, anonymous_delegate) = (
        fixture.confidential_policy(),
        fixture.confidential_in_proof_owner(),
        fixture.anonymous(),
        fixture.anonymous_public_owners(),
        fixture.delegate(),
        fixture.anonymous_delegate(),
    );
    let unflagged = [
        Ok(RailAuthorization::ConfidentialSigner),
        length,
        Ok(RailAuthorization::Delegate),
        Ok(RailAuthorization::AnonymousDelegate),
    ];
    for (data, bits, by_rail) in [
        (&signer, [false; 3], [own(&signer), length, flag, flag]),
        (
            &in_proof,
            [true, false, false],
            [own(&in_proof), flag, flag, flag],
        ),
        (
            &hidden,
            [false, false, true],
            [flag, own(&hidden), flag, flag],
        ),
        (
            &public,
            [false, true, false],
            [flag, own(&public), flag, flag],
        ),
        (&delegate, [false; 3], unflagged),
        (&anonymous_delegate, [false; 3], unflagged),
    ] {
        let compact = data.encode().unwrap();
        assert_eq!(
            [
                compact[transact_flag::BYTE] & transact_flag::OWNER_IN_PROOF != 0,
                compact[1] & shape_flag::PUBLIC_OWNERS != 0,
                compact[1] & shape_flag::DEFAULT_OWNER_TAG != 0,
            ],
            bits
        );
        let parsed = RAILS.map(|rail| {
            RingTransactData::parse(&compact, rail).map(|parsed| parsed.data().authorization)
        });
        assert_eq!(parsed, by_rail, "{}", data.rail());
    }
}

#[test]
fn reply_construction_rejects_zero_and_short_chunks() {
    for len in [0, 1, (MIN_REPLY_LEN - 1) as u16] {
        for region in [&[][..], &[0; MIN_REPLY_LEN][..]] {
            assert_eq!(Replies::new(len, region), Err(CustomRingError::ReplyLength));
        }
    }
}

#[test]
fn reply_construction_rejects_incomplete_chunks() {
    let region = [0; MIN_REPLY_LEN * 2 + 1];
    for len in [MIN_REPLY_LEN - 1, MIN_REPLY_LEN + 1, region.len()] {
        assert_eq!(
            Replies::new(MIN_REPLY_LEN as u16, &region[..len]),
            Err(CustomRingError::ReplyLength)
        );
    }
}

#[test]
fn checked_frames_require_one_reply_per_recipient_body() {
    let fixture = Fixture::new();
    let valid = fixture.anonymous_replies(MIN_REPLY_LEN as u16);
    assert!(ParsedTransact::new(valid.clone()).is_ok());
    let region = [0; MIN_REPLY_LEN * 4];
    for chunks in [0, 1, 2, 4] {
        let mut data = valid.clone();
        data.replies =
            Some(Replies::new(MIN_REPLY_LEN as u16, &region[..MIN_REPLY_LEN * chunks]).unwrap());
        assert_eq!(ParsedTransact::new(data), Err(CustomRingError::ReplyLength));
    }
}

#[test]
fn checked_frames_keep_reply_chunks_on_their_recipient_slots() {
    let fixture = Fixture::new();
    let data = fixture.anonymous_replies(MIN_REPLY_LEN as u16);
    let replies = data.replies.unwrap();
    assert_eq!(usize::from(replies.byte_len()), MIN_REPLY_LEN);
    let parsed = ParsedTransact::new(data).unwrap();
    let slots: Vec<_> = parsed.data().slot_replies().collect();
    assert_eq!(slots[0], &replies.region()[..MIN_REPLY_LEN]);
    assert_eq!(
        slots[1],
        &replies.region()[MIN_REPLY_LEN..2 * MIN_REPLY_LEN]
    );
    assert!(slots[2].is_empty());
    assert_eq!(slots[3], &replies.region()[2 * MIN_REPLY_LEN..]);
}
