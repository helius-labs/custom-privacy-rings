use sea_orm::DatabaseBackend;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct MergeScan;

#[async_trait::async_trait]
impl MigrationTrait for MergeScan {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        let blob = match manager.get_database_backend() {
            DatabaseBackend::Postgres => "BYTEA",
            _ => "BLOB",
        };
        db.execute_unprepared(&format!(
            "CREATE TABLE IF NOT EXISTS merge_scan (ring {blob} NOT NULL PRIMARY KEY, after_owner {blob} NOT NULL, after_mint {blob} NOT NULL)"
        )).await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS merge_settings_active ON merge_settings(ring, owner, mint) WHERE target_note_count > 0"
        ).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP INDEX merge_settings_active")
            .await?;
        db.execute_unprepared("DROP TABLE merge_scan").await?;
        Ok(())
    }
}
