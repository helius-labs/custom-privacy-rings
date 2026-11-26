//! Ring addresses, the audit stream cipher with its record codecs, and the ring keys of a shielded keypair.

mod audit_records;
mod auditor_message;
mod blinding;
mod dummy_key;
mod error;
mod keys;
mod labels;
mod list_salt;
mod output_body;
pub mod ring_address;
mod slot_secret;
mod stream_cipher;

pub use audit_records::{
    decode_recipient_keys, encode_recipient_keys, AuditEnvelope, AuditEvidence, EntryRecord,
    MergeEnvelope, MergeRecord, OutputRecord, RecipientBodyForms, RecipientPayload, RecordKey,
    SealedLayout, RECORD_ZERO_KEY_BIT,
};
pub use auditor_message::AuditorMessageRead;
pub use blinding::ring_blinding_seed;
pub use dummy_key::{dummy_recipient_key, DUMMY_KEY_DOMAIN};
pub use error::RingKeyError;
pub use keys::{RingDeliveryRequest, RingKeys, RotatingRingKeys, ViewTagRequest, ViewTagStream};
pub use labels::{
    AUDIT_STREAM_LABEL, MERGE_OUTPUT_LABEL, MERGE_RECORD_LABEL, RECIPIENT_BODY_LABEL,
    REGISTER_KEY_LABEL, REPLY_LABEL,
};
pub use list_salt::{ListSaltHolder, LIST_SALT_LABEL};
pub use output_body::{
    encode_confidential_output, encode_ring_body_output, encrypted_output, ring_body_output,
};
pub use ring_address::{RingAddress, RingPaymentRequest};
pub use slot_secret::{bootstrap_slot_tag, DeliveredSlot, SlotSecret, StreamError};
pub use stream_cipher::{symmetric_apply_audit, RingStream};
