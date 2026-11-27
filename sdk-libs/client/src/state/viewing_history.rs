//! The anonymous rail's wallet state, spec `ViewingKeyEntry` per retained viewing key.

use std::{collections::BTreeMap, ops::Range};

use serde::{Deserialize, Serialize};
use solana_address::Address;
use spr_keypair::{viewing_key::ViewTag, x25519::X25519_PUBKEY_LEN, X25519Pubkey};
use zolana_ring_keys::{RingKeys, ViewTagRequest, ViewTagStream};

use crate::{exactly_one, AnonymousError};

/// How far past its counter a stream is read, zolana's `DEFAULT_TAG_WINDOW`.
pub const TAG_WINDOW: u64 = 64;

/// `scanned` never passes `next`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamCounter {
    /// The index the next request or send takes.
    next: u64,
    /// Past the highest index a sync found.
    scanned: u64,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ViewingKeyEntry {
    viewing_pubkey: X25519Pubkey,
    created_at: i64,
    tx: StreamCounter,
    request: StreamCounter,
    /// `after_slot` per ring, `None` reads from the first indexed block.
    ring_positions: BTreeMap<Address, Option<u64>>,
    /// `after_slot` of the tag read, `None` reads from the first indexed block.
    tag_position: Option<u64>,
}

/// The sender tag a wallet's history gives one transfer, a payment carries its own request tag.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferTags {
    viewing_pubkey: X25519Pubkey,
    sender: ViewTag,
}

/// Every retained viewing key, oldest first, the last one current.
#[derive(Clone, PartialEq, Eq)]
pub struct ViewingHistory {
    entries: Vec<ViewingKeyEntry>,
}

/// `history` is advanced past `tag`.
#[must_use]
#[derive(Clone, Debug)]
pub struct AdvancedTag<T> {
    pub tag: T,
    pub history: ViewingHistory,
}

impl StreamCounter {
    /// Every index taken and [`TAG_WINDOW`] past them, `zolana/docs/spec.md` First Time Sync Wallet.
    pub(crate) fn listed(&self) -> Range<u64> {
        0..self.next.saturating_add(TAG_WINDOW)
    }

    /// The indices an earlier sync read through its key's tag position.
    pub(crate) fn read_before(&self) -> Range<u64> {
        match self.scanned {
            0 => 0..0,
            scanned => 0..scanned.saturating_add(TAG_WINDOW),
        }
    }

    fn mint(&mut self) -> Result<u64, AnonymousError> {
        let index = self.next;
        self.next = index
            .checked_add(1)
            .ok_or(AnonymousError::CounterOverflow)?;
        Ok(index)
    }

    fn found(&mut self, index: u64) {
        self.scanned = self.scanned.max(index.saturating_add(1));
        self.next = self.next.max(self.scanned);
    }
}

impl TransferTags {
    /// The key the sender tag derives from.
    pub fn viewing_pubkey(&self) -> X25519Pubkey {
        self.viewing_pubkey
    }

    /// On the first slot of a frame sealed to the wallet's own key.
    pub fn sender(&self) -> ViewTag {
        self.sender
    }
}

impl ViewingKeyEntry {
    pub fn new(viewing_pubkey: X25519Pubkey, created_at: i64) -> Self {
        Self {
            viewing_pubkey,
            created_at,
            tx: StreamCounter::default(),
            request: StreamCounter::default(),
            ring_positions: BTreeMap::new(),
            tag_position: None,
        }
    }

    pub fn viewing_pubkey(&self) -> X25519Pubkey {
        self.viewing_pubkey
    }

    pub fn created_at(&self) -> i64 {
        self.created_at
    }

    pub fn tx_count(&self) -> u64 {
        self.tx.next
    }

    pub fn request_count(&self) -> u64 {
        self.request.next
    }

    pub(crate) fn streams(&self) -> [(ViewTagStream, StreamCounter); 2] {
        [
            (ViewTagStream::Sender, self.tx),
            (ViewTagStream::Request, self.request),
        ]
    }

    pub(crate) fn found(&mut self, stream: ViewTagStream, index: u64) {
        match stream {
            ViewTagStream::Sender => self.tx.found(index),
            ViewTagStream::Request => self.request.found(index),
        }
    }

    pub fn ring_positions(&self) -> &BTreeMap<Address, Option<u64>> {
        &self.ring_positions
    }

    pub fn tag_position(&self) -> Option<u64> {
        self.tag_position
    }

    /// Only a terminal page's `context.slot` reaches here.
    pub(crate) fn set_ring_position(&mut self, ring: Address, after_slot: u64) {
        self.ring_positions.insert(ring, Some(after_slot));
    }

    /// Only a terminal page's `context.slot` reaches here.
    pub(crate) fn set_tag_position(&mut self, after_slot: u64) {
        self.tag_position = Some(after_slot);
    }

    /// A stale history lowers no counter, its next selection would take a published index again.
    fn progress_matches(&self, stored: &Self) -> bool {
        self.tx.next >= stored.tx.next
            && self.request.next >= stored.request.next
            && self.tx.scanned == stored.tx.scanned
            && self.request.scanned == stored.request.scanned
            && self.tag_position == stored.tag_position
            && stored
                .ring_positions
                .iter()
                .all(|(ring, after_slot)| self.ring_positions.get(ring) == Some(after_slot))
            && self.ring_positions.iter().all(|(ring, after_slot)| {
                stored.ring_positions.contains_key(ring) || after_slot.is_none()
            })
    }
}

zolana_ring_keys::redacted_debug!(ViewingKeyEntry {
    shown: [created_at, tx],
    hidden: [viewing_pubkey, request, ring_positions, tag_position],
});

impl ViewingHistory {
    /// A wallet whose one viewing key never paid or was paid.
    pub fn new(viewing_pubkey: X25519Pubkey, created_at: i64) -> Self {
        Self {
            entries: vec![ViewingKeyEntry::new(viewing_pubkey, created_at)],
        }
    }

    /// The rotated key becomes current, the earlier keys keep reading what was sent to them.
    pub fn with_rotated_key(
        mut self,
        viewing_pubkey: X25519Pubkey,
        created_at: i64,
    ) -> Result<Self, AnonymousError> {
        let current = self.current();
        if created_at <= current.created_at || self.entry(&viewing_pubkey).is_some() {
            return Err(AnonymousError::RotatedKeyNotNewer);
        }
        let entry = self.unread_entry(viewing_pubkey, created_at);
        self.entries.push(entry);
        Ok(self)
    }

    /// A key the wallet held before its current one, the current key stays last.
    pub fn with_imported_key(
        mut self,
        viewing_pubkey: X25519Pubkey,
        created_at: i64,
    ) -> Result<Self, AnonymousError> {
        let taken = self
            .entries
            .iter()
            .any(|entry| entry.created_at == created_at);
        if created_at >= self.current().created_at || taken || self.entry(&viewing_pubkey).is_some()
        {
            return Err(AnonymousError::ImportedKeyNotOlder);
        }
        let entry = self.unread_entry(viewing_pubkey, created_at);
        let at = self
            .entries
            .partition_point(|entry| entry.created_at < created_at);
        self.entries.insert(at, entry);
        Ok(self)
    }

    /// Existing ring positions remain unchanged.
    pub fn with_ring(mut self, ring: Address, after_slot: Option<u64>) -> Self {
        for entry in &mut self.entries {
            entry.ring_positions.entry(ring).or_insert(after_slot);
        }
        self
    }

    /// Every ring the wallet reads is unread under an added key, its next sync reads their history.
    fn unread_entry(&self, viewing_pubkey: X25519Pubkey, created_at: i64) -> ViewingKeyEntry {
        ViewingKeyEntry {
            ring_positions: self
                .entries
                .iter()
                .flat_map(|entry| entry.ring_positions.keys())
                .map(|ring| (*ring, None))
                .collect(),
            ..ViewingKeyEntry::new(viewing_pubkey, created_at)
        }
    }

    pub fn entries(&self) -> &[ViewingKeyEntry] {
        &self.entries
    }

    pub fn current(&self) -> &ViewingKeyEntry {
        // Every constructor and `from_json` holds at least one entry.
        &self.entries[self.entries.len() - 1]
    }

    pub fn entry(&self, viewing_pubkey: &X25519Pubkey) -> Option<&ViewingKeyEntry> {
        self.entries
            .iter()
            .find(|entry| entry.viewing_pubkey == *viewing_pubkey)
    }

    pub(crate) fn entries_mut(&mut self) -> &mut [ViewingKeyEntry] {
        &mut self.entries
    }

    /// The sender tag of one transfer under the current key. A caller stores the returned
    /// history before it builds the transfer, two transfers under one index would publish one tag.
    pub fn select<K: RingKeys + ?Sized>(
        &self,
        keys: &K,
    ) -> Result<AdvancedTag<TransferTags>, AnonymousError> {
        let viewing_pubkey = self.current().viewing_pubkey;
        let AdvancedTag {
            tag: sender,
            history,
        } = self.minted(keys, ViewTagStream::Sender)?;
        Ok(AdvancedTag {
            tag: TransferTags {
                viewing_pubkey,
                sender,
            },
            history,
        })
    }

    /// The tag of the current key's next payment request.
    pub fn request_tag<K: RingKeys + ?Sized>(
        &self,
        keys: &K,
    ) -> Result<AdvancedTag<ViewTag>, AnonymousError> {
        self.minted(keys, ViewTagStream::Request)
    }

    fn minted<K: RingKeys + ?Sized>(
        &self,
        keys: &K,
        stream: ViewTagStream,
    ) -> Result<AdvancedTag<ViewTag>, AnonymousError> {
        let mut history = self.clone();
        let current = history.current_mut();
        let viewing_pubkey = current.viewing_pubkey;
        let counter = match stream {
            ViewTagStream::Sender => &mut current.tx,
            ViewTagStream::Request => &mut current.request,
        };
        let index = counter.mint()?;
        let tags = exactly_one(keys.view_tags(&[ViewTagRequest {
            viewing_pubkey,
            stream,
            indices: index..index.saturating_add(1),
        }])?)?;
        Ok(AdvancedTag {
            tag: exactly_one(tags)?,
            history,
        })
    }

    /// At least `count` requests were minted under the current key, a wallet restored from its seed alone states it.
    #[must_use]
    pub fn with_request_count(mut self, count: u64) -> Self {
        let request = &mut self.current_mut().request;
        request.next = request.next.max(count);
        self
    }

    fn current_mut(&mut self) -> &mut ViewingKeyEntry {
        let last = self.entries.len() - 1;
        &mut self.entries[last]
    }

    /// Whether every stored key's progress is unmoved, an added key starts with none.
    pub(crate) fn progress_matches(&self, stored: &Self) -> bool {
        stored.entries.iter().all(|stored| {
            self.entry(&stored.viewing_pubkey)
                .is_some_and(|entry| entry.progress_matches(stored))
        }) && self.entries.iter().all(|entry| {
            stored.entry(&entry.viewing_pubkey).is_some()
                || (entry.tx.scanned == 0
                    && entry.request.scanned == 0
                    && entry.tag_position.is_none()
                    && entry.ring_positions.values().all(Option::is_none))
        })
    }
}

zolana_ring_keys::redacted_debug!(ViewingHistory { shown: [entries] });

/// The persisted form, keys in hex.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct HistoryJson {
    viewing_history: Vec<EntryJson>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct EntryJson {
    viewing_pubkey: String,
    created_at: i64,
    tx: CounterJson,
    request: CounterJson,
    ring_positions: Vec<RingPositionJson>,
    tag_position: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RingPositionJson {
    ring: String,
    after_slot: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CounterJson {
    next: u64,
    scanned: u64,
}

impl From<StreamCounter> for CounterJson {
    fn from(counter: StreamCounter) -> Self {
        Self {
            next: counter.next,
            scanned: counter.scanned,
        }
    }
}

impl TryFrom<CounterJson> for StreamCounter {
    type Error = AnonymousError;

    fn try_from(stored: CounterJson) -> Result<Self, AnonymousError> {
        if stored.scanned > stored.next {
            return Err(invalid("a scan watermark passes its next index"));
        }
        Ok(Self {
            next: stored.next,
            scanned: stored.scanned,
        })
    }
}

/// Stored only inside `AnonymousWallet`, its positions and watermarks cover the wallet's notes.
pub(crate) mod stored {
    use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

    use super::{HistoryJson, ViewingHistory};

    pub(crate) fn serialize<S: Serializer>(
        history: &ViewingHistory,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        HistoryJson::from(history).serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<ViewingHistory, D::Error> {
        ViewingHistory::try_from(HistoryJson::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl From<&ViewingHistory> for HistoryJson {
    fn from(history: &ViewingHistory) -> Self {
        Self {
            viewing_history: history
                .entries
                .iter()
                .map(|entry| EntryJson {
                    viewing_pubkey: hex::encode(entry.viewing_pubkey.as_bytes()),
                    created_at: entry.created_at,
                    tx: entry.tx.into(),
                    request: entry.request.into(),
                    ring_positions: entry
                        .ring_positions
                        .iter()
                        .map(|(ring, after_slot)| RingPositionJson {
                            ring: ring.to_string(),
                            after_slot: *after_slot,
                        })
                        .collect(),
                    tag_position: entry.tag_position,
                })
                .collect(),
        }
    }
}

impl TryFrom<HistoryJson> for ViewingHistory {
    type Error = AnonymousError;

    fn try_from(stored: HistoryJson) -> Result<Self, AnonymousError> {
        let entries = stored
            .viewing_history
            .into_iter()
            .map(entry_of)
            .collect::<Result<Vec<_>, _>>()?;
        for (at, entry) in entries.iter().enumerate() {
            if at > 0 && entries[at - 1].created_at >= entry.created_at
                || entries[..at]
                    .iter()
                    .any(|earlier| earlier.viewing_pubkey == entry.viewing_pubkey)
            {
                return Err(invalid("viewing keys repeat or are not oldest first"));
            }
        }
        if entries.is_empty() {
            return Err(invalid("holds no viewing key"));
        }
        Ok(Self { entries })
    }
}

fn entry_of(stored: EntryJson) -> Result<ViewingKeyEntry, AnonymousError> {
    let mut ring_positions = BTreeMap::new();
    for position in stored.ring_positions {
        let ring: Address = position
            .ring
            .parse()
            .map_err(|_| invalid("ringPositions holds a ring that is not an address"))?;
        if ring_positions.insert(ring, position.after_slot).is_some() {
            return Err(invalid("ringPositions repeats a ring"));
        }
    }
    Ok(ViewingKeyEntry {
        viewing_pubkey: pubkey("viewingPubkey", &stored.viewing_pubkey)?,
        created_at: stored.created_at,
        tx: stored.tx.try_into()?,
        request: stored.request.try_into()?,
        ring_positions,
        tag_position: stored.tag_position,
    })
}

/// An error names the field alone.
fn pubkey(name: &str, text: &str) -> Result<X25519Pubkey, AnonymousError> {
    hex::decode(text)
        .ok()
        .and_then(|bytes| <[u8; X25519_PUBKEY_LEN]>::try_from(bytes).ok())
        .and_then(|bytes| X25519Pubkey::from_bytes(bytes).ok())
        .ok_or_else(|| {
            invalid(&format!(
                "{name} holds a key that is not a tagged X25519 key"
            ))
        })
}

fn invalid(reason: &str) -> AnonymousError {
    AnonymousError::ViewingHistory(reason.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use custom_ring_interface::stream::slot_tag;
    use spr_keypair::{ShieldedKeypair, ViewingKey};

    use super::*;

    const PASSES_NEXT: &str =
        "viewing history does not parse because a scan watermark passes its next index";

    fn to_stored(history: &ViewingHistory) -> serde_json::Value {
        stored::serialize(history, serde_json::value::Serializer).unwrap()
    }

    fn refusal(document: serde_json::Value) -> String {
        stored::deserialize(document).unwrap_err().to_string()
    }

    #[test]
    fn each_selection_takes_the_next_sender_index() {
        let wallet = ShieldedKeypair::new_ed25519().unwrap();
        let fresh = ViewingHistory::new(wallet.viewing_pubkey(), 0);
        let first = fresh.select(&wallet).unwrap();
        let second = first.history.select(&wallet).unwrap();
        for (index, selected) in [&first, &second].into_iter().enumerate() {
            assert_eq!(
                selected.tag.sender(),
                slot_tag(wallet.get_sender_view_tag(index as u64).unwrap())
            );
            assert_eq!(selected.history.current().tx_count(), index as u64 + 1);
        }
        assert_eq!(
            fresh.current().tx_count(),
            0,
            "a selection moves no stored counter"
        );
    }

    #[test]
    fn a_stale_history_lowers_no_counter() {
        let wallet = ShieldedKeypair::new_ed25519().unwrap();
        let fresh = ViewingHistory::new(wallet.viewing_pubkey(), 0);
        let selected = fresh.select(&wallet).unwrap().history;
        assert!(selected.progress_matches(&fresh));
        assert!(!fresh.progress_matches(&selected));
        let minted_from_stale = fresh.request_tag(&wallet).unwrap().history;
        assert!(!minted_from_stale.progress_matches(&selected));
    }

    #[test]
    fn requests_mint_past_the_tag_window_each_index_once() {
        let wallet = ShieldedKeypair::new_ed25519().unwrap();
        let viewing_pubkey = wallet.viewing_pubkey();
        let mut history = ViewingHistory::new(viewing_pubkey, 0);
        let mut minted = Vec::new();
        for _ in 0..200 {
            let advanced = history.request_tag(&wallet).unwrap();
            minted.push(advanced.tag);
            history = advanced.history;
        }
        assert_eq!(history.current().request_count(), 200);
        assert_eq!(minted.iter().collect::<BTreeSet<_>>().len(), 200);
        let stream = wallet
            .view_tags(&[ViewTagRequest {
                viewing_pubkey,
                stream: ViewTagStream::Request,
                indices: 0..200,
            }])
            .unwrap();
        assert_eq!(stream, [minted]);

        let last = ViewingHistory::new(viewing_pubkey, 0)
            .with_request_count(u64::MAX - 1)
            .request_tag(&wallet)
            .unwrap()
            .history;
        assert_eq!(last.current().request_count(), u64::MAX);
        assert!(matches!(
            last.request_tag(&wallet),
            Err(AnonymousError::CounterOverflow)
        ));
    }

    #[test]
    fn a_stated_request_count_raises_the_counter_and_never_lowers_it() {
        let wallet = ShieldedKeypair::new_ed25519().unwrap();
        let stated = ViewingHistory::new(wallet.viewing_pubkey(), 0).with_request_count(70);
        assert_eq!(stated.current().request_count(), 70);
        assert_eq!(stated.current().tx_count(), 0);
        for lower in [0, 69, 70] {
            assert_eq!(stated.clone().with_request_count(lower), stated);
        }
        let minted = stated.request_tag(&wallet).unwrap();
        assert_eq!(
            minted.tag[24..],
            wallet.get_recipient_request_view_tag(70).unwrap()[24..]
        );
        assert_eq!(minted.history.current().request_count(), 71);
    }

    #[test]
    fn a_stream_lists_a_window_past_its_counter_and_reads_before_only_past_a_found_index() {
        let history = ViewingHistory::new(ViewingKey::new().pubkey(), 0).with_request_count(5);
        let mut entry = history.current().clone();
        assert_eq!(entry.request.listed(), 0..69);
        assert!(entry.request.read_before().is_empty());
        entry.found(ViewTagStream::Request, 2);
        assert_eq!(entry.request.listed(), 0..69);
        assert_eq!(entry.request.read_before(), 0..67);
        entry.found(ViewTagStream::Request, 9);
        assert_eq!(entry.request.listed(), 0..74);
        assert_eq!(entry.request.read_before(), 0..74);
        assert_eq!(entry.tx.listed(), 0..64);
    }

    #[test]
    fn a_stored_watermark_past_its_next_index_is_refused() {
        let history = ViewingHistory::new(ViewingKey::new().pubkey(), 0);
        let mut document = to_stored(&history);
        document["viewingHistory"][0]["tx"]["scanned"] = 1.into();
        assert_eq!(refusal(document), PASSES_NEXT);
    }

    #[test]
    fn a_stored_document_names_each_streams_counters_and_holds_the_request_watermark() {
        let mut history = ViewingHistory::new(ViewingKey::new().pubkey(), 0);
        history.entries_mut()[0].tx = StreamCounter {
            next: 5,
            scanned: 3,
        };
        history.entries_mut()[0].request = StreamCounter {
            next: 9,
            scanned: 7,
        };
        let mut document = to_stored(&history);
        let entry = &document["viewingHistory"][0];
        assert_eq!(entry["tx"], serde_json::json!({ "next": 5, "scanned": 3 }));
        assert_eq!(
            entry["request"],
            serde_json::json!({ "next": 9, "scanned": 7 })
        );
        assert_eq!(stored::deserialize(document.clone()).unwrap(), history);
        document["viewingHistory"][0]["request"]["scanned"] = 9.into();
        assert!(stored::deserialize(document.clone()).is_ok());
        document["viewingHistory"][0]["request"]["scanned"] = 10.into();
        assert_eq!(refusal(document), PASSES_NEXT);
    }

    #[test]
    fn a_stored_history_holds_at_least_one_key_each_once_and_oldest_first() {
        let history = ViewingHistory::new(ViewingKey::new().pubkey(), 10)
            .with_rotated_key(ViewingKey::new().pubkey(), 20)
            .unwrap();
        let document = to_stored(&history);
        let out_of_order =
            "viewing history does not parse because viewing keys repeat or are not oldest first";
        let mut empty = document.clone();
        empty["viewingHistory"] = serde_json::json!([]);
        assert_eq!(
            refusal(empty),
            "viewing history does not parse because holds no viewing key"
        );
        let mut repeated = document.clone();
        repeated["viewingHistory"][1]["viewingPubkey"] =
            document["viewingHistory"][0]["viewingPubkey"].clone();
        assert_eq!(refusal(repeated), out_of_order);
        for created_at in [10, 5] {
            let mut unordered = document.clone();
            unordered["viewingHistory"][1]["createdAt"] = created_at.into();
            assert_eq!(refusal(unordered), out_of_order);
        }
    }

    #[test]
    fn debug_prints_no_request_count_or_scan_position() {
        let ring = Address::new_from_array([0x77; 32]);
        let mut history = ViewingHistory::new(ViewingKey::new().pubkey(), 3)
            .with_request_count(4_242_424_242)
            .with_ring(ring, Some(6_464_646_464));
        history.entries_mut()[0].set_tag_position(5_353_535_353);
        let printed = format!("{history:?} {:?}", history.current());
        for hidden in [
            "4242424242".to_owned(),
            "5353535353".to_owned(),
            "6464646464".to_owned(),
            ring.to_string(),
        ] {
            assert!(!printed.contains(&hidden), "{printed}");
        }
    }

    #[test]
    fn an_added_key_reads_every_watched_ring_from_its_start() {
        let ring = Address::new_from_array([7; 32]);
        let synced = ViewingHistory::new(ViewingKey::new().pubkey(), 10).with_ring(ring, Some(40));
        let imported = ViewingKey::new().pubkey();
        let rotated = ViewingKey::new().pubkey();
        let added = synced
            .with_imported_key(imported, 5)
            .unwrap()
            .with_rotated_key(rotated, 20)
            .unwrap();
        assert_eq!(
            added.entry(&imported).unwrap().ring_positions()[&ring],
            None
        );
        assert_eq!(added.current().viewing_pubkey(), rotated);
        assert_eq!(added.current().ring_positions()[&ring], None);
        assert_eq!(added.entries()[1].ring_positions()[&ring], Some(40));
    }

    #[test]
    fn watching_a_ring_fills_absent_positions_and_preserves_existing_ones() {
        let existing = Address::new_from_array([7; 32]);
        let added = Address::new_from_array([8; 32]);
        let history = ViewingHistory::new(ViewingKey::new().pubkey(), 10)
            .with_ring(existing, Some(40))
            .with_rotated_key(ViewingKey::new().pubkey(), 20)
            .unwrap()
            .with_ring(existing, Some(90))
            .with_ring(added, Some(60));
        assert_eq!(history.entries()[0].ring_positions()[&existing], Some(40));
        assert_eq!(history.entries()[1].ring_positions()[&existing], None);
        assert!(history
            .entries()
            .iter()
            .all(|entry| entry.ring_positions()[&added] == Some(60)));
    }

    #[test]
    fn an_import_newer_than_the_current_key_or_repeated_is_refused() {
        let current = ViewingKey::new().pubkey();
        let history = ViewingHistory::new(current, 10);
        for (key, created_at) in [
            (ViewingKey::new().pubkey(), 11),
            (ViewingKey::new().pubkey(), 10),
            (current, 5),
        ] {
            assert!(matches!(
                history.clone().with_imported_key(key, created_at),
                Err(AnonymousError::ImportedKeyNotOlder)
            ));
        }
    }
}
