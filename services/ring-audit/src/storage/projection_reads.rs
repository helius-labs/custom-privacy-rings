//! Point reads of a ring's projection, where every miss of the bounded mirror lands.
//!
//! Each read sees committed rows only. The mirror never caches what these return, so a read racing
//! a commit cannot leave a stale entry behind.

use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, Value};
use solana_address::Address;

use crate::{
    frame_repairs::{RepairAttempt, MAX_REPAIRED_FRAMES},
    spend_machine::SpendRow,
    spent_index::{
        read_enrollment, read_frame, read_note, read_repair_attempt, read_spend_row, NoteSeal,
        ENROLLMENT_COLUMNS, NOTE_COLUMNS, ROW_COLUMNS,
    },
    tracking::HeldEnrollment,
    utxos::{CreatedIn, StoredUtxo},
};
use ring_service_core::sql::{self, KEYS_PER_STATEMENT};

/// The key an enrollment lookup names, each read through its own column.
#[derive(Clone, Copy, Debug)]
pub(crate) enum EnrollmentKey {
    Member,
    /// Tagged on disk, the column names no list key.
    ListKey,
}

#[must_use]
pub(crate) struct ProjectionReads<'a, C = DatabaseConnection> {
    pub db: &'a C,
    pub ring: Address,
    pub seal: &'a NoteSeal,
}

impl<'a, C> ProjectionReads<'a, C> {
    /// The same ring's reads on `db`, a snapshot then answers every read.
    pub(crate) fn at<'b, T>(&self, db: &'b T) -> ProjectionReads<'b, T>
    where
        'a: 'b,
    {
        ProjectionReads {
            db,
            ring: self.ring,
            seal: self.seal,
        }
    }
}

impl<C: ConnectionTrait> ProjectionReads<'_, C> {
    pub(crate) async fn due_repairs(
        &self,
        now: u64,
    ) -> Result<Vec<(CreatedIn, RepairAttempt)>, DbErr> {
        self.ring_query(
            "SELECT slot,tx_signature,event_index,attempts,retry_after FROM spend_unread_frames \
             WHERE ring=$1 AND retry_after<=$2 ORDER BY retry_after,slot,tx_signature,event_index \
             LIMIT $3",
            vec![sql::integer(now)?, sql::count(MAX_REPAIRED_FRAMES)?],
        )
        .all(self.db)
        .await?
        .iter()
        .map(|row| Ok((read_frame(row)?, read_repair_attempt(row)?)))
        .collect()
    }

    pub(crate) async fn notes(
        &self,
        commitments: &[[u8; 32]],
    ) -> Result<Vec<([u8; 32], StoredUtxo)>, DbErr> {
        let mut notes = Vec::new();
        for chunk in commitments.chunks(KEYS_PER_STATEMENT) {
            for row in self
                .keyed(
                    &format!("SELECT {NOTE_COLUMNS} FROM finalized_notes"),
                    "commitment",
                    chunk,
                )
                .await?
            {
                notes.push(read_note(&row, self.seal)?);
            }
        }
        Ok(notes)
    }

    /// A row found by tag counts only when its sealed body names the asked nullifier.
    pub(crate) async fn notes_by_nullifier(
        &self,
        nullifiers: &[[u8; 32]],
    ) -> Result<Vec<([u8; 32], StoredUtxo)>, DbErr> {
        let mut notes = Vec::new();
        for chunk in nullifiers.chunks(KEYS_PER_STATEMENT) {
            let tags = chunk
                .iter()
                .map(|nullifier| self.seal.tags.nullifier_tag(nullifier))
                .collect::<Result<Vec<_>, _>>()?;
            for row in self
                .keyed(
                    &format!("SELECT {NOTE_COLUMNS} FROM finalized_notes"),
                    "nullifier_tag",
                    &tags,
                )
                .await?
            {
                let (commitment, note) = read_note(&row, self.seal)?;
                if !note
                    .nullifier
                    .is_some_and(|nullifier| chunk.contains(&nullifier))
                {
                    return Err(DbErr::Custom("note row names another nullifier".into()));
                }
                notes.push((commitment, note));
            }
        }
        Ok(notes)
    }

    pub(crate) async fn rows(&self, nullifiers: &[[u8; 32]]) -> Result<Vec<SpendRow>, DbErr> {
        let mut rows = Vec::new();
        for chunk in nullifiers.chunks(KEYS_PER_STATEMENT) {
            for row in self
                .keyed(
                    &format!("SELECT {ROW_COLUMNS} FROM finalized_spent_nullifiers"),
                    "nullifier",
                    chunk,
                )
                .await?
            {
                rows.push(read_spend_row(&row, self.seal)?);
            }
        }
        Ok(rows)
    }

    /// A row found by a tag counts only when its sealed body names the asked key.
    pub(crate) async fn enrollments(
        &self,
        key: EnrollmentKey,
        values: &[[u8; 32]],
    ) -> Result<Vec<HeldEnrollment>, DbErr> {
        let tags = &self.seal.tags;
        let column = match key {
            EnrollmentKey::Member => "member",
            EnrollmentKey::ListKey => "list_key_tag",
        };
        let mut found = Vec::new();
        for chunk in values.chunks(KEYS_PER_STATEMENT) {
            let stored = chunk
                .iter()
                .map(|value| match key {
                    EnrollmentKey::Member => Ok(*value),
                    EnrollmentKey::ListKey => tags.list_key_tag(value),
                })
                .collect::<Result<Vec<_>, _>>()?;
            for row in self
                .keyed(
                    &format!("SELECT {ENROLLMENT_COLUMNS} FROM held_enrollments"),
                    column,
                    &stored,
                )
                .await?
            {
                let held = read_enrollment(&row, self.seal)?;
                if !chunk.contains(&key.of(&held)) {
                    return Err(DbErr::Custom("enrollment row names another key".into()));
                }
                found.push(held);
            }
        }
        Ok(found)
    }

    async fn keyed(
        &self,
        select: &str,
        column: &str,
        keys: &[[u8; 32]],
    ) -> Result<Vec<sea_orm::QueryResult>, DbErr> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        self.ring_query(
            &format!(
                "{select} WHERE ring=$1 AND {column} IN ({})",
                sql::placeholders(2, keys.len())
            ),
            keys.iter().map(|key| sql::bytes(key)).collect(),
        )
        .all(self.db)
        .await
    }

    pub(crate) fn ring_query(&self, query: &str, values: Vec<Value>) -> sql::Query {
        let mut bound = vec![sql::bytes(self.ring.as_ref())];
        bound.extend(values);
        sql::Query::new(query, bound)
    }
}

/// Every stored history row of one ring, for a test that compares stored states.
#[cfg(test)]
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct StoredRows {
    pub notes: std::collections::BTreeMap<[u8; 32], StoredUtxo>,
    pub rows: std::collections::BTreeMap<[u8; 32], SpendRow>,
    pub unread: std::collections::BTreeMap<CreatedIn, RepairAttempt>,
    pub faults: std::collections::BTreeSet<([u8; 32], CreatedIn)>,
}

#[cfg(test)]
impl<C: ConnectionTrait> ProjectionReads<'_, C> {
    pub(crate) async fn everything(&self) -> Result<StoredRows, DbErr> {
        use crate::spent_index::read_repair_attempt;
        let all = |select: String| self.ring_query(&select, Vec::new()).all(self.db);
        let mut stored = StoredRows::default();
        for row in all(format!(
            "SELECT {NOTE_COLUMNS} FROM finalized_notes WHERE ring=$1"
        ))
        .await?
        {
            let (commitment, note) = read_note(&row, self.seal)?;
            stored.notes.insert(commitment, note);
        }
        for row in all(format!(
            "SELECT {ROW_COLUMNS} FROM finalized_spent_nullifiers WHERE ring=$1"
        ))
        .await?
        {
            let row = read_spend_row(&row, self.seal)?;
            stored.rows.insert(row.nullifier, row);
        }
        for row in all("SELECT slot,tx_signature,event_index,attempts,retry_after FROM spend_unread_frames WHERE ring=$1".into()).await? {
            stored.unread.insert(read_frame(&row)?, read_repair_attempt(&row)?);
        }
        for row in all("SELECT nullifier,slot,tx_signature,event_index FROM spend_integrity_faults WHERE ring=$1".into()).await? {
            stored.faults.insert((sql::read_bytes(&row, "nullifier")?, read_frame(&row)?));
        }
        Ok(stored)
    }
}

impl EnrollmentKey {
    pub(crate) const fn of(self, held: &HeldEnrollment) -> [u8; 32] {
        match self {
            Self::Member => held.member,
            Self::ListKey => held.list_key,
        }
    }
}
