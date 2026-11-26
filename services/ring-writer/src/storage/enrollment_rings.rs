use sea_orm::{ConnectionTrait, DatabaseConnection};
use solana_address::Address;

use crate::error::WriterError;
use ring_service_core::sql;

/// The rings each enrollment writer serves through `registerEnrollmentRing`.
pub(crate) struct EnrollmentRingBook<'a> {
    db: &'a DatabaseConnection,
    writer: Address,
}

impl<'a> EnrollmentRingBook<'a> {
    pub(crate) fn new(db: &'a DatabaseConnection, writer: Address) -> Self {
        Self { db, writer }
    }

    pub(crate) async fn insert(&self, ring: Address) -> Result<(), WriterError> {
        let insert = "INSERT INTO enrollment_rings (writer, ring) VALUES ($1, $2) \
                      ON CONFLICT(writer, ring) DO NOTHING";
        let values = vec![sql::bytes(self.writer.as_ref()), sql::bytes(ring.as_ref())];
        self.db
            .execute(sql::statement(self.db, insert, values))
            .await?;
        Ok(())
    }

    pub async fn remove(&self, ring: Address) -> Result<(), WriterError> {
        let delete = "DELETE FROM enrollment_rings WHERE writer=$1 AND ring=$2";
        let values = vec![sql::bytes(self.writer.as_ref()), sql::bytes(ring.as_ref())];
        self.db
            .execute(sql::statement(self.db, delete, values))
            .await?;
        Ok(())
    }

    pub async fn rings(&self) -> Result<Vec<Address>, WriterError> {
        let select = "SELECT ring FROM enrollment_rings WHERE writer=$1";
        let values = vec![sql::bytes(self.writer.as_ref())];
        let rows = self
            .db
            .query_all(sql::statement(self.db, select, values))
            .await?;
        Ok(rows
            .iter()
            .map(|row| sql::read_address(row, "ring"))
            .collect::<Result<_, _>>()?)
    }
}
