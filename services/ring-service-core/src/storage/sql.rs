//! The statement builder and the column codecs every storage module shares.

use sea_orm::{ConnectionTrait, DbErr, QueryResult, Statement, Value};
use solana_address::Address;

/// Keys bound into one statement, under every backend's parameter ceiling.
pub const KEYS_PER_STATEMENT: usize = 256;

/// Built for the backend of the connection that runs it.
#[must_use]
pub struct Query {
    sql: String,
    values: Vec<Value>,
}

impl Query {
    pub fn new(sql: impl Into<String>, values: Vec<Value>) -> Self {
        Self {
            sql: sql.into(),
            values,
        }
    }

    pub async fn execute(self, conn: &impl ConnectionTrait) -> Result<u64, DbErr> {
        Ok(conn.execute(self.statement(conn)).await?.rows_affected())
    }

    pub async fn one(self, conn: &impl ConnectionTrait) -> Result<Option<QueryResult>, DbErr> {
        conn.query_one(self.statement(conn)).await
    }

    pub async fn all(self, conn: &impl ConnectionTrait) -> Result<Vec<QueryResult>, DbErr> {
        conn.query_all(self.statement(conn)).await
    }

    fn statement(self, conn: &impl ConnectionTrait) -> Statement {
        statement(conn, self.sql, self.values)
    }
}

pub fn statement<C: ConnectionTrait>(
    conn: &C,
    sql: impl Into<String>,
    values: Vec<Value>,
) -> Statement {
    Statement::from_sql_and_values(conn.get_database_backend(), sql, values)
}

pub fn bytes(value: &[u8]) -> Value {
    value.to_vec().into()
}

/// A BIGINT column holds no value past `i64::MAX`.
pub fn integer(value: u64) -> Result<Value, DbErr> {
    i64::try_from(value)
        .map(Value::from)
        .map_err(|_| DbErr::Custom("value exceeds the store integer range".to_owned()))
}

pub fn count(value: usize) -> Result<Value, DbErr> {
    integer(u64::try_from(value).unwrap_or(u64::MAX))
}

/// `$first` onward, NULL for none, an empty list then matches no row.
pub fn placeholders(first: usize, count: usize) -> String {
    match count {
        0 => "NULL".to_owned(),
        _ => (first..first + count)
            .map(|index| format!("${index}"))
            .collect::<Vec<_>>()
            .join(","),
    }
}

pub fn read_bytes<const N: usize>(row: &QueryResult, column: &str) -> Result<[u8; N], DbErr> {
    <[u8; N]>::try_from(row.try_get::<Vec<u8>>("", column)?)
        .map_err(|_| DbErr::Custom(format!("column {column} has another length")))
}

pub fn read_integer(row: &QueryResult, column: &str) -> Result<u64, DbErr> {
    u64::try_from(row.try_get::<i64>("", column)?)
        .map_err(|_| DbErr::Custom(format!("column {column} is negative")))
}

pub fn read_optional_integer(row: &QueryResult, column: &str) -> Result<Option<u64>, DbErr> {
    row.try_get::<Option<i64>>("", column)?
        .map(u64::try_from)
        .transpose()
        .map_err(|_| DbErr::Custom(format!("column {column} is negative")))
}

pub fn read_address(row: &QueryResult, column: &str) -> Result<Address, DbErr> {
    read_bytes(row, column).map(Address::new_from_array)
}
