use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, QueryResult, Statement, Value};
use solana_address::Address;

use crate::error::RingRpcError;

/// The rings each enrollment writer serves through `registerEnrollmentRing`.
pub(crate) struct EnrollmentRingBook<'a> {
    db: &'a DatabaseConnection,
    writer: Address,
}

impl<'a> EnrollmentRingBook<'a> {
    pub(crate) fn new(db: &'a DatabaseConnection, writer: Address) -> Self {
        Self { db, writer }
    }

    pub(crate) async fn insert(&self, ring: Address) -> Result<(), RingRpcError> {
        let sql = "INSERT INTO enrollment_rings (writer, ring) VALUES ($1, $2) \
                   ON CONFLICT(writer, ring) DO NOTHING";
        self.db
            .execute(self.statement(sql, vec![address(&self.writer), address(&ring)]))
            .await?;
        Ok(())
    }

    pub(crate) async fn remove(&self, ring: Address) -> Result<(), RingRpcError> {
        let sql = "DELETE FROM enrollment_rings WHERE writer=$1 AND ring=$2";
        self.db
            .execute(self.statement(sql, vec![address(&self.writer), address(&ring)]))
            .await?;
        Ok(())
    }

    pub(crate) async fn rings(&self) -> Result<Vec<Address>, RingRpcError> {
        let sql = "SELECT ring FROM enrollment_rings WHERE writer=$1";
        let rows = self
            .db
            .query_all(self.statement(sql, vec![address(&self.writer)]))
            .await?;
        Ok(rows.iter().map(read_ring).collect::<Result<_, _>>()?)
    }

    fn statement(&self, sql: &str, values: Vec<Value>) -> Statement {
        Statement::from_sql_and_values(self.db.get_database_backend(), sql, values)
    }
}

fn read_ring(row: &QueryResult) -> Result<Address, DbErr> {
    let bytes: [u8; 32] = row
        .try_get::<Vec<u8>>("", "ring")?
        .try_into()
        .map_err(|_| DbErr::Type("enrollment ring is malformed".to_owned()))?;
    Ok(Address::new_from_array(bytes))
}

fn address(value: &Address) -> Value {
    value.to_bytes().to_vec().into()
}
