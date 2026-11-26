//! The stream labels, each keeps one keystream apart from every other under the same secret.

pub const AUDIT_STREAM_LABEL: &[u8; 9] = b"CRING/adt";
pub const RECIPIENT_BODY_LABEL: &[u8; 9] = b"CRING/rcp";
pub const REPLY_LABEL: &[u8; 9] = b"CRING/rpl";
pub const SENDER_BUNDLE_LABEL: &[u8; 9] = b"CRING/snd";
pub const REGISTER_KEY_LABEL: &[u8; 9] = b"CRING/nfk";
pub const MERGE_RECORD_LABEL: &[u8; 9] = b"CRING/mrg";
/// Keeps a merge record's keystream apart from a transfer record's under the same key pair.
pub const MERGE_OUTPUT_LABEL: &[u8; 9] = b"CRING/mro";
