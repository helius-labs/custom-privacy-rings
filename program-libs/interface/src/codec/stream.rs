//! The short slot tag a frame carries and the full tag SPP indexes.
//!
//! Read after `program-libs/interface/src/codec/output.rs`. Read before `program-libs/interface/src/proof/key_catalog.rs`.

/// The bytes of a ring slot tag a frame carries, a false match costs one trial open.
pub const SLOT_TAG_LEN: usize = 8;

/// A ring slot's tag reaches the indexer alone, its leading bytes stay zero and off the frame.
pub fn slot_tag(tag: [u8; 32]) -> [u8; 32] {
    unframed_slot_tag(&framed_slot_tag(&tag))
}

pub fn framed_slot_tag(tag: &[u8; 32]) -> [u8; SLOT_TAG_LEN] {
    let mut framed = [0; SLOT_TAG_LEN];
    framed.copy_from_slice(&tag[32 - SLOT_TAG_LEN..]);
    framed
}

pub fn unframed_slot_tag(framed: &[u8; SLOT_TAG_LEN]) -> [u8; 32] {
    let mut tag = [0; 32];
    tag[32 - SLOT_TAG_LEN..].copy_from_slice(framed);
    tag
}
