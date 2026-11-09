//! The enrollment queue as successful `Enroll` instructions wrote it, and the viewing key of each leaf version.
//!
//! The instruction data is the record, the program emits no event. A row keeps every field an
//! `ApplyEnrollments` rebuilds its entry hash from, so the projection never reads an account. A leaf
//! version keeps the viewing key its `viewing_hash` commits to and the sequence of the entry that
//! inserted it, the source of the seal an auditor opens.

use anyhow::Result;
use custom_ring_interface::{
    storage::StorageError, storage_instruction::EnrollIxData, tracking::TrackingEntry,
};
use sea_orm::{ConnectionTrait, QueryResult, Value};
use solana_address::Address;

use super::{bytes, field, int, uint, TrackingStore};

/// One successful `Enroll` at its queue sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct QueuedEnrollment {
    pub sequence: u64,
    pub owner: [u8; 32],
    /// The `Enroll` carried a `RegisterKey` proof.
    pub proven: bool,
    pub ix: EnrollIxData,
}

/// `head` is the oldest sequence no apply consumed, `tail` the one the next `Enroll` takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct QueueBounds {
    pub head: u64,
    pub tail: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LeafKey {
    pub viewing_u: [u8; 32],
    /// The inserting entry's, a rotation keeps it.
    pub sequence: u64,
}

/// What one applied entry left on its owner's leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LeafStamp {
    pub member: [u8; 32],
    pub viewing_u: [u8; 32],
    /// Set on the insert alone, an update keeps the inserting sequence.
    pub inserted: Option<u64>,
}

impl QueuedEnrollment {
    /// The entry the program hashed into the queue.
    pub fn entry(&self) -> Result<TrackingEntry, StorageError> {
        TrackingEntry::from_enroll(&Address::new_from_array(self.owner), &self.ix, self.proven)
    }
}

const QUEUE_READ: &str = "SELECT sequence, owner, proven, nullifier_pk, list_tag, viewing_u, \
     eph_u, ciphertext FROM tracking_queue";

impl<C: ConnectionTrait> TrackingStore<'_, C> {
    pub async fn queue_bounds(&self) -> Result<QueueBounds> {
        let sql = "SELECT CAST(COALESCE(MAX(sequence) + 1, 0) AS BIGINT) AS tail, \
                   CAST(COALESCE(MIN(CASE WHEN applied_slot IS NULL THEN sequence END), \
                   MAX(sequence) + 1, 0) AS BIGINT) AS head \
                   FROM tracking_queue WHERE registry=$1";
        let row = self.tree.query_one(sql, vec![]).await?;
        let Some(row) = row else {
            return Ok(QueueBounds::default());
        };
        Ok(QueueBounds {
            head: uint(&row, "head")?,
            tail: uint(&row, "tail")?,
        })
    }

    /// Ascending from `from`, applied entries included.
    pub async fn queued(&self, from: u64, limit: u64) -> Result<Vec<QueuedEnrollment>> {
        let sql =
            format!("{QUEUE_READ} WHERE registry=$1 AND sequence>=$2 ORDER BY sequence LIMIT $3");
        let rows = self
            .tree
            .query_all(&sql, vec![int(from)?, int(limit)?])
            .await?;
        rows.iter().map(queued).collect()
    }

    pub async fn queued_at(&self, sequence: u64) -> Result<Option<QueuedEnrollment>> {
        let sql = format!("{QUEUE_READ} WHERE registry=$1 AND sequence=$2");
        let row = self.tree.query_one(&sql, vec![int(sequence)?]).await?;
        row.as_ref().map(queued).transpose()
    }

    pub async fn enqueue(&self, queued: &QueuedEnrollment, slot: u64) -> Result<()> {
        let entry = queued.entry()?;
        let sql = "INSERT INTO tracking_queue(registry,sequence,owner,member,proven,nullifier_pk,\
                   ct_hash,list_tag,viewing_u,eph_u,ciphertext,slot) \
                   VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)";
        let ix = &queued.ix;
        let values = vec![
            int(queued.sequence)?,
            bytes(&queued.owner),
            bytes(&entry.member),
            int(u64::from(queued.proven))?,
            bytes(&ix.nullifier_pk),
            bytes(&entry.ct_hash),
            bytes(&ix.list_tag),
            bytes(&ix.viewing_u),
            bytes(&ix.eph_u),
            bytes(&ix.ciphertext),
            int(slot)?,
        ];
        self.tree.exec(sql, values).await
    }

    /// Consumes `count` entries from `head`.
    pub async fn mark_applied(&self, head: u64, count: u64, slot: u64) -> Result<()> {
        let sql = "UPDATE tracking_queue SET applied_slot=$4 \
                   WHERE registry=$1 AND sequence>=$2 AND sequence<$3";
        let end = head.saturating_add(count);
        self.tree
            .exec(sql, vec![int(head)?, int(end)?, int(slot)?])
            .await
    }

    /// Every leaf version saved at `revision` takes its predecessor's key, then each stamp overrides its owner's.
    pub async fn stamp_leaves(&self, revision: u64, stamps: &[LeafStamp]) -> Result<()> {
        let previous = |column: &str| {
            format!(
                "(SELECT p.{column} FROM tracking_records p WHERE p.program=tracking_records.program \
                 AND p.member=tracking_records.member AND p.applied<tracking_records.applied \
                 ORDER BY p.applied DESC LIMIT 1)"
            )
        };
        let sql = format!(
            "UPDATE tracking_records SET viewing_u={}, sequence={} WHERE program=$1 AND applied=$2",
            previous("viewing_u"),
            previous("sequence"),
        );
        self.tree.exec(&sql, vec![int(revision)?]).await?;
        let sql = "UPDATE tracking_records SET viewing_u=$3, sequence=COALESCE($4, sequence) \
                   WHERE program=$1 AND applied=$2 AND member=$5";
        for stamp in stamps {
            let inserted = stamp.inserted.map(i64::try_from).transpose()?;
            let values = vec![
                int(revision)?,
                bytes(&stamp.viewing_u),
                Value::from(inserted),
                bytes(&stamp.member),
            ];
            self.tree.exec(sql, values).await?;
        }
        Ok(())
    }

    /// The key of `member`'s newest leaf version at or below `version`.
    pub async fn leaf_key(&self, member: &[u8; 32], version: u64) -> Result<Option<LeafKey>> {
        let sql =
            "SELECT viewing_u, sequence FROM tracking_records WHERE program=$1 AND member=$2 \
                   AND applied<=$3 ORDER BY applied DESC LIMIT 1";
        let row = self
            .tree
            .query_one(sql, vec![bytes(member), int(version)?])
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(LeafKey {
            viewing_u: field(&row, "viewing_u")?,
            sequence: uint(&row, "sequence")?,
        }))
    }
}

/// Queue rows carry no revision a registry undo can read.
pub(super) async fn rollback_slot<C: ConnectionTrait>(conn: &C, slot: u64) -> Result<()> {
    for sql in [
        "DELETE FROM tracking_queue WHERE slot=$1",
        "UPDATE tracking_queue SET applied_slot=NULL WHERE applied_slot=$1",
    ] {
        super::exec(conn, sql, vec![int(slot)?]).await?;
    }
    Ok(())
}

fn queued(row: &QueryResult) -> Result<QueuedEnrollment> {
    Ok(QueuedEnrollment {
        sequence: uint(row, "sequence")?,
        owner: field(row, "owner")?,
        proven: uint(row, "proven")? != 0,
        ix: EnrollIxData {
            nullifier_pk: field(row, "nullifier_pk")?,
            list_tag: field(row, "list_tag")?,
            viewing_u: field(row, "viewing_u")?,
            eph_u: field(row, "eph_u")?,
            ciphertext: field(row, "ciphertext")?,
        },
    })
}

#[cfg(test)]
mod tests {
    use custom_ring_interface::REGISTER_KEY_CIPHERTEXT_LEN;

    use super::*;
    use crate::tests::database;

    const REGISTRY: [u8; 32] = [7; 32];

    fn enrollment(sequence: u64) -> QueuedEnrollment {
        QueuedEnrollment {
            sequence,
            owner: [sequence as u8 + 1; 32],
            proven: true,
            ix: EnrollIxData {
                nullifier_pk: [1; 32],
                list_tag: [3; 32],
                viewing_u: [9; 32],
                eph_u: [4; 32],
                ciphertext: [5; REGISTER_KEY_CIPHERTEXT_LEN],
            },
        }
    }

    #[tokio::test]
    async fn the_head_passes_applied_entries_and_a_rollback_restores_it() {
        let db = database().await;
        let store = TrackingStore::new(&db, REGISTRY);
        assert_eq!(store.queue_bounds().await.unwrap(), QueueBounds::default());
        for sequence in 0..3 {
            store.enqueue(&enrollment(sequence), 10).await.unwrap();
        }
        TrackingStore::new(&db, [8; 32])
            .enqueue(&enrollment(0), 10)
            .await
            .unwrap();
        store.mark_applied(0, 2, 11).await.unwrap();
        let bounds = |head, tail| QueueBounds { head, tail };
        assert_eq!(store.queue_bounds().await.unwrap(), bounds(2, 3));
        store.mark_applied(2, 1, 12).await.unwrap();
        assert_eq!(store.queue_bounds().await.unwrap(), bounds(3, 3));
        assert_eq!(store.queued(1, 8).await.unwrap(), [1, 2].map(enrollment));
        rollback_slot(&db, 12).await.unwrap();
        rollback_slot(&db, 11).await.unwrap();
        assert_eq!(store.queue_bounds().await.unwrap(), bounds(0, 3));
        rollback_slot(&db, 10).await.unwrap();
        assert_eq!(store.queue_bounds().await.unwrap(), QueueBounds::default());
    }
}
