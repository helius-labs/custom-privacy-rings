//! Readers and writers of ring output bodies in SPP's encrypted output framing.

use custom_ring_interface::{
    output::{OutputPayload, RING_BODY_SCHEME_TAG, RING_SENDER_BUNDLE_SCHEME_TAG},
    CustomRingError, RING_RECORD_LEN,
};
use spr_event::OutputDataEncoding;

pub fn encode_confidential_output(body: &[u8]) -> Result<Vec<u8>, CustomRingError> {
    OutputPayload::Confidential(body).encode()
}

pub fn encode_ring_body_output(body: &[u8], reply: &[u8]) -> Result<Vec<u8>, CustomRingError> {
    OutputPayload::Recipient { body, reply }.encode()
}

/// `body || reply` of a well-formed ring body output, `None` otherwise.
pub fn ring_body_output(data: &[u8]) -> Option<(&[u8], &[u8])> {
    let payload = encrypted_output_body(data, RING_BODY_SCHEME_TAG)?;
    let (body_len, rest) = payload.split_first_chunk()?;
    rest.split_at_checked(usize::from(u16::from_le_bytes(*body_len)))
        .filter(|(body, _)| body.len() >= RING_RECORD_LEN)
}

pub fn ring_sender_bundle_output_body(data: &[u8]) -> Option<&[u8]> {
    encrypted_output_body(data, RING_SENDER_BUNDLE_SCHEME_TAG)
}

fn encrypted_output_body(data: &[u8], scheme: u8) -> Option<&[u8]> {
    encrypted_output(data)
        .filter(|(scheme_byte, _)| *scheme_byte == scheme)
        .map(|(_, payload)| payload)
}

/// The scheme byte and payload of `Encrypted(len || scheme || payload)`.
pub fn encrypted_output(data: &[u8]) -> Option<(u8, &[u8])> {
    let (&encoding_tag, rest) = data.split_first()?;
    if encoding_tag != OutputDataEncoding::ENCRYPTED_TAG {
        return None;
    }
    let (len_bytes, body) = rest.split_first_chunk::<4>()?;
    let body_len = usize::try_from(u32::from_le_bytes(*len_bytes)).ok()?;
    let (&scheme_byte, payload) = body.split_first()?;
    (body_len == body.len()).then_some((scheme_byte, payload))
}
