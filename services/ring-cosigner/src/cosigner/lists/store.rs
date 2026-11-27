use std::{collections::HashSet, sync::Arc};

use sea_orm::{DbErr, QueryResult, Value};
use solana_address::Address;

use super::decision::{List, ListEntry, Lists, Subject, SubjectKind};
use ring_service_core::{database::Store, sql};

use crate::error::RingCosignerError;

#[derive(Clone)]
pub(crate) struct StoredLists(pub Arc<Store>);

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
    pub(crate) async fn set(
        &self,
        ring: Address,
        entry: ListEntry,
    ) -> Result<(), RingCosignerError> {
        let insert = "INSERT INTO cosign_list_entries(ring,list,kind,subject) VALUES($1,$2,$3,$4) \
                      ON CONFLICT(ring,list,kind,subject) DO NOTHING";
        self.execute(insert, entry.values(ring)).await
    }

    pub(crate) async fn remove(
        &self,
        ring: Address,
        entry: ListEntry,
    ) -> Result<(), RingCosignerError> {
        let delete = "DELETE FROM cosign_list_entries \
                      WHERE ring=$1 AND list=$2 AND kind=$3 AND subject=$4";
        self.execute(delete, entry.values(ring)).await
    }

    /// In `(subject, kind, list)` order after `after`, at most `limit`.
    pub(crate) async fn entries(
        &self,
        ring: Address,
        after: Option<ListEntry>,
        limit: u64,
    ) -> Result<Vec<ListEntry>, RingCosignerError> {
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

    async fn execute(&self, sql: &str, values: Vec<Value>) -> Result<(), RingCosignerError> {
        let conn = self.0.connection();
        sql::Query::new(sql, values).execute(conn).await?;
        Ok(())
    }

    async fn read(&self, sql: &str, values: Vec<Value>) -> Result<Vec<ListEntry>, DbErr> {
        let conn = self.0.connection();
        sql::Query::new(sql, values)
            .all(conn)
            .await?
            .iter()
            .map(ListEntry::read)
            .collect()
    }
}

impl StoredLists {
    /// The entries of `ring` that name one of `subjects`, with one allow entry of each closed kind.
    pub(crate) async fn lists(
        &self,
        ring: Address,
        subjects: &[Subject],
    ) -> Result<Lists, RingCosignerError> {
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
