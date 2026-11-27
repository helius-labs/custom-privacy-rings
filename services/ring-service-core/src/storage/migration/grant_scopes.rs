use sea_orm::DatabaseBackend;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct GrantScopes;

#[async_trait::async_trait]
impl MigrationTrait for GrantScopes {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        if !manager.has_table("legacy_grants").await? {
            db.execute_unprepared("ALTER TABLE grants RENAME TO legacy_grants")
                .await?;
        }
        let blob = match manager.get_database_backend() {
            DatabaseBackend::Postgres => "BYTEA",
            _ => "BLOB",
        };
        db.execute_unprepared(&format!(
            "CREATE TABLE IF NOT EXISTS grants (scope {blob} NOT NULL PRIMARY KEY, sealed {blob} NOT NULL)"
        )).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        if db
            .query_one(sea_orm::Statement::from_string(
                manager.get_database_backend(),
                "SELECT 1 FROM grants LIMIT 1",
            ))
            .await?
            .is_some()
        {
            return Err(DbErr::Migration(
                "restore the grant database from its backup".into(),
            ));
        }
        db.execute_unprepared("DROP TABLE grants").await?;
        db.execute_unprepared("ALTER TABLE legacy_grants RENAME TO grants")
            .await?;
        Ok(())
    }
}
