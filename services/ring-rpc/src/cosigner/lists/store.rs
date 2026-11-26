use std::{collections::HashSet, future::Future, sync::Arc};

use sea_orm::{ConnectionTrait, DbErr, QueryResult, Value};
use solana_address::Address;

use super::decision::{List, ListEntry, Lists, Subject, SubjectKind};
use crate::{database::Store, error::RingRpcError, sql};

pub trait ListStore: Send + Sync {
    /// The entries of `ring` that name one of `subjects`, with one allow entry of each closed kind.
    fn lists(
        &self,
        ring: Address,
        subjects: &[Subject],
    ) -> impl Future<Output = Result<Lists, RingRpcError>> + Send;
}

#[derive(Clone)]
pub struct StoredLists(pub Arc<Store>);

pub(crate) fn migration(blob: &str) -> String {
    format!(
        "CREATE TABLE cosign_list_entries (ring {blob} NOT NULL, list TEXT NOT NULL \
         CHECK(list IN ('allow','block','freeze')), kind TEXT NOT NULL \
         CHECK(kind IN ('owner','asset','destination')), subject {blob} NOT NULL, \
         PRIMARY KEY(ring, list, kind, subject))"
    )
}

fn stored_name<T: Copy>(
    all: [T; 3],
    name: fn(T) -> &'static str,
    stored: &str,
) -> Result<T, DbErr> {
    all.into_iter()
        .find(|value| name(*value) == stored)
        .ok_or_else(|| DbErr::Custom(format!("unknown list column value {stored}")))
}

impl ListEntry {
    fn read(row: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            list: stored_name(List::ALL, List::name, &row.try_get::<String>("", "list")?)?,
            subject: Subject {
                kind: stored_name(
                    SubjectKind::ALL,
                    SubjectKind::name,
                    &row.try_get::<String>("", "kind")?,
                )?,
                id: sql::read_bytes(row, "subject")?,
            },
        })
    }

    fn values(self, ring: Address) -> Vec<Value> {
        vec![
            sql::bytes(ring.as_ref()),
            Value::from(self.list.name()),
            Value::from(self.subject.kind.name()),
            sql::bytes(&self.subject.id),
        ]
    }
}

impl StoredLists {
    pub async fn set(&self, ring: Address, entry: ListEntry) -> Result<(), RingRpcError> {
        let insert = "INSERT INTO cosign_list_entries(ring,list,kind,subject) VALUES($1,$2,$3,$4) \
                      ON CONFLICT(ring,list,kind,subject) DO NOTHING";
        self.execute(insert, entry.values(ring)).await
    }

    pub async fn remove(&self, ring: Address, entry: ListEntry) -> Result<(), RingRpcError> {
        let delete = "DELETE FROM cosign_list_entries \
                      WHERE ring=$1 AND list=$2 AND kind=$3 AND subject=$4";
        self.execute(delete, entry.values(ring)).await
    }

    /// In `(subject, kind, list)` order after `after`, at most `limit`.
    pub async fn entries(
        &self,
        ring: Address,
        after: Option<ListEntry>,
        limit: u64,
    ) -> Result<Vec<ListEntry>, RingRpcError> {
        let limit = Value::from(i64::try_from(limit).unwrap_or(i64::MAX));
        let ring = sql::bytes(ring.as_ref());
        let order = "ORDER BY subject,kind,list LIMIT";
        let select = "SELECT list,kind,subject FROM cosign_list_entries WHERE ring=$1";
        let (sql, values) = match after {
            None => (format!("{select} {order} $2"), vec![ring, limit]),
            Some(entry) => (
                format!("{select} AND (subject,kind,list)>($2,$3,$4) {order} $5"),
                vec![
                    ring,
                    sql::bytes(&entry.subject.id),
                    Value::from(entry.subject.kind.name()),
                    Value::from(entry.list.name()),
                    limit,
                ],
            ),
        };
        Ok(self.read(&sql, values).await?)
    }

    async fn execute(&self, sql: &str, values: Vec<Value>) -> Result<(), RingRpcError> {
        let conn = self.0.connection();
        conn.execute(sql::statement(conn, sql, values)).await?;
        Ok(())
    }

    async fn read(&self, sql: &str, values: Vec<Value>) -> Result<Vec<ListEntry>, DbErr> {
        let conn = self.0.connection();
        conn.query_all(sql::statement(conn, sql, values))
            .await?
            .iter()
            .map(ListEntry::read)
            .collect()
    }
}

impl ListStore for StoredLists {
    async fn lists(&self, ring: Address, subjects: &[Subject]) -> Result<Lists, RingRpcError> {
        // One statement reads one snapshot, the first allow entry of a kind marks the kind closed.
        let first_allow = "list='allow' AND NOT EXISTS (SELECT 1 FROM cosign_list_entries earlier \
                           WHERE earlier.ring=entries.ring AND earlier.list='allow' \
                           AND earlier.kind=entries.kind AND earlier.subject<entries.subject)";
        let named = sql::placeholders(2, subjects.len());
        let matching = match subjects {
            [] => first_allow.to_owned(),
            _ => format!("subject IN ({named}) OR ({first_allow})"),
        };
        let select = format!(
            "SELECT list,kind,subject FROM cosign_list_entries entries WHERE ring=$1 AND ({matching})"
        );
        let values = std::iter::once(sql::bytes(ring.as_ref()))
            .chain(subjects.iter().map(|subject| sql::bytes(&subject.id)))
            .collect();
        let entries: HashSet<ListEntry> = self.read(&select, values).await?.into_iter().collect();
        Ok(Lists {
            allowing: entries
                .iter()
                .filter(|entry| entry.list == List::Allow)
                .map(|entry| entry.subject.kind)
                .collect(),
            entries,
        })
    }
}
