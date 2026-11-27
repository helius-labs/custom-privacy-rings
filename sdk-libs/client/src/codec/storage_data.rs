//! The writers and readers of storage instruction data a client builds and an indexer replays.

use custom_ring_interface::{
    instruction::list_and_mint_entry,
    storage::StorageError,
    storage_instruction::{
        event_kind, parse_writes, storage_write_count, tag, ApplyEnrollmentsRequest,
        EnrollmentsAppliedEvent, ParsedStorageUpdate, RecordWriteClassification, StorageEvent,
        StorageUpdateIxData,
    },
    CustomRingError, RecordWrite, RecordWriteData, WriteRecordsIxData,
};
use zolana_ring_policy::{ListWrite, Writer};

pub trait StorageUpdateCodec<'a>: Sized {
    fn parse(data: &'a [u8]) -> Result<Self, StorageError>;

    fn encode(&self) -> Result<Vec<u8>, StorageError>;

    fn private_writes(&self) -> Result<u16, StorageError>;

    /// The leaf writes the apply proof hashes, a mint settings tag above 1 refused.
    fn entries(&self) -> impl Iterator<Item = Result<RecordWrite, CustomRingError>> + '_;
}

impl<'a> StorageUpdateCodec<'a> for StorageUpdateIxData<'a> {
    fn parse(data: &'a [u8]) -> Result<Self, StorageError> {
        ParsedStorageUpdate::parse(data).map(|parsed| parsed.data)
    }

    fn encode(&self) -> Result<Vec<u8>, StorageError> {
        let count = storage_write_count(self.writes.len())?;
        if (self.private_writes()? != 0) != self.tracking_revision.is_some() {
            return Err(StorageError::InvalidTrackingRegistry);
        }
        let asset_count =
            u8::try_from(self.asset_writes.len()).map_err(|_| StorageError::InvalidAssetWrite)?;
        let mut data = Vec::new();
        data.extend_from_slice(&self.revision.0);
        data.push(count);
        data.extend_from_slice(bytemuck::cast_slice(self.writes));
        data.push(asset_count);
        data.extend_from_slice(bytemuck::cast_slice(self.asset_writes));
        if let Some(revision) = self.tracking_revision {
            data.extend_from_slice(&revision.0);
        }
        data.extend_from_slice(bytemuck::bytes_of(self.apply));
        Ok(data)
    }

    fn private_writes(&self) -> Result<u16, StorageError> {
        RecordWriteClassification {
            entries: self.entries(),
            asset_writes: self.asset_writes,
        }
        .private_writes()
    }

    fn entries(&self) -> impl Iterator<Item = Result<RecordWrite, CustomRingError>> + '_ {
        self.writes.iter().map(RecordWriteData::storage_entry)
    }
}

pub trait EnrollmentRequestEncode {
    fn encode(&self) -> Vec<u8>;
}

impl EnrollmentRequestEncode for ApplyEnrollmentsRequest<'_> {
    fn encode(&self) -> Vec<u8> {
        let mut data = bytemuck::bytes_of(self.ix).to_vec();
        self.writes
            .iter()
            .for_each(|write| write.encode_into(&mut data));
        data
    }
}

pub trait StorageEventDecode<'a>: Sized {
    /// `None` unless the tag, the kind, the header and exactly its `count` writes match.
    fn decode(data: &'a [u8]) -> Option<Self>;
}

impl<'a> StorageEventDecode<'a> for StorageEvent<'a> {
    fn decode(data: &'a [u8]) -> Option<Self> {
        let ([tag::EMIT_EVENT, event_kind::ENROLLMENTS_APPLIED], body) =
            data.split_first_chunk::<2>()?
        else {
            return None;
        };
        let (header, writes) =
            body.split_at_checked(core::mem::size_of::<EnrollmentsAppliedEvent>())?;
        let header: EnrollmentsAppliedEvent = bytemuck::try_pod_read_unaligned(header).ok()?;
        Some(Self {
            header,
            writes: parse_writes(header.count, writes).ok()?,
        })
    }
}

pub trait ListWriteData: Sized {
    fn new(write: ListWrite) -> Self;

    /// The entry of `writer` changing `member`'s lists, only the lists `writer` owns.
    fn entry(&self, member: [u8; 32], writer: Writer) -> Result<RecordWrite, CustomRingError>;
}

impl ListWriteData for WriteRecordsIxData {
    fn new(write: ListWrite) -> Self {
        Self {
            mask: write.mask().to_le_bytes(),
            value: write.value().to_le_bytes(),
        }
    }

    fn entry(&self, member: [u8; 32], writer: Writer) -> Result<RecordWrite, CustomRingError> {
        self.entry_within(member, writer.writable())
    }
}

pub trait AuthorityWriteEntry {
    fn entry(&self) -> Result<RecordWrite, CustomRingError>;
}

impl AuthorityWriteEntry for RecordWriteData {
    fn entry(&self) -> Result<RecordWrite, CustomRingError> {
        list_and_mint_entry(
            self.member,
            &self.lists,
            &self.mint_policy,
            Writer::Authority.writable(),
        )
    }
}
