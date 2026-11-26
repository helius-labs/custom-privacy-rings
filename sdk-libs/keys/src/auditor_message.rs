//! The reader of the auditor message a transfer publishes.

use custom_ring_interface::{
    AuditStreamLayout, AuditorMessage, RecordKind, TAGGED_X25519_KEY_LEN, VIEWING_PUBKEY_TAG,
};

pub trait AuditorMessageRead<'a>: Sized {
    /// `None` unless the key is tagged and every length is that of `layout` and `exits`.
    fn parse(data: &'a [u8], layout: AuditStreamLayout, exits: usize) -> Option<Self>;

    /// Indexed among the exits, not the slots.
    fn exit_record(&self, exit: usize) -> Option<&'a [u8]>;
}

impl<'a> AuditorMessageRead<'a> for AuditorMessage<'a> {
    fn parse(data: &'a [u8], layout: AuditStreamLayout, exits: usize) -> Option<Self> {
        layout.checked().ok()?;
        let (eph_pk, rest) = data.split_first_chunk::<TAGGED_X25519_KEY_LEN>()?;
        let (envelope, rest) = rest.split_at_checked(layout.envelope().len)?;
        let (recipient_keys, exit_records) = rest.split_at_checked(layout.keys_len())?;
        let exit_record_len = layout.records.published_len(RecordKind::Exit);
        (eph_pk[0] == VIEWING_PUBKEY_TAG
            && exit_records.len() == exits.checked_mul(exit_record_len)?)
        .then_some(Self {
            eph_pk,
            envelope,
            recipient_keys,
            exit_records,
            exit_record_len,
        })
    }

    fn exit_record(&self, exit: usize) -> Option<&'a [u8]> {
        self.exit_records
            .chunks_exact(self.exit_record_len)
            .nth(exit)
    }
}
