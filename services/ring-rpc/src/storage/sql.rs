//! The statement builder and the column codecs every storage module shares.

use sea_orm::{ConnectionTrait, DbErr, QueryResult, Statement, Value};
use solana_address::Address;

pub(crate) fn statement<C: ConnectionTrait>(
    conn: &C,
    sql: impl Into<String>,
    values: Vec<Value>,
) -> Statement {
    Statement::from_sql_and_values(conn.get_database_backend(), sql, values)
}

pub(crate) fn bytes(value: &[u8]) -> Value {
    value.to_vec().into()
}

/// A BIGINT column holds no value past `i64::MAX`.
pub(crate) fn integer(value: u64) -> Result<Value, DbErr> {
    i64::try_from(value)
        .map(Value::from)
        .map_err(|_| DbErr::Custom("value exceeds the store integer range".to_owned()))
}

pub(crate) fn count(value: usize) -> Result<Value, DbErr> {
    integer(u64::try_from(value).unwrap_or(u64::MAX))
}

/// `$first` onward, NULL for none, an empty list then matches no row.
pub(crate) fn placeholders(first: usize, count: usize) -> String {
    match count {
        0 => "NULL".to_owned(),
        _ => (first..first + count)
            .map(|index| format!("${index}"))
            .collect::<Vec<_>>()
            .join(","),
    }
}

pub(crate) fn read_bytes<const N: usize>(
    row: &QueryResult,
    column: &str,
) -> Result<[u8; N], DbErr> {
    <[u8; N]>::try_from(row.try_get::<Vec<u8>>("", column)?)
        .map_err(|_| DbErr::Custom(format!("column {column} has another length")))
}

pub(crate) fn read_integer(row: &QueryResult, column: &str) -> Result<u64, DbErr> {
    u64::try_from(row.try_get::<i64>("", column)?)
        .map_err(|_| DbErr::Custom(format!("column {column} is negative")))
}

pub(crate) fn read_address(row: &QueryResult, column: &str) -> Result<Address, DbErr> {
    read_bytes(row, column).map(Address::new_from_array)
}
