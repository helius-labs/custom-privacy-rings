use sea_orm::DatabaseBackend;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct WalletFunders;

#[async_trait::async_trait]
impl MigrationTrait for WalletFunders {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let blob = match manager.get_database_backend() {
            DatabaseBackend::Postgres => "BYTEA",
            _ => "BLOB",
        };
        let db = manager.get_connection();
        db.execute_unprepared(&format!(
            "CREATE TABLE IF NOT EXISTS cosign_wallet_funding_accounts (ring {blob} NOT NULL, account {blob} NOT NULL, ticket {blob} NOT NULL REFERENCES cosign_wallet_sends(ticket) ON DELETE CASCADE, PRIMARY KEY(ring, account))"
        )).await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS cosign_wallet_funding_accounts_ticket ON cosign_wallet_funding_accounts(ticket)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE cosign_wallet_funding_accounts")
            .await?;
        Ok(())
    }
}

pub(super) fn statements(blob: &str) -> [String; 6] {
    [
        format!("CREATE TABLE cosign_admission (ring {blob} NOT NULL, asset {blob} NOT NULL, last_slot BIGINT NOT NULL DEFAULT 0, PRIMARY KEY(ring, asset))"),
        format!("CREATE TABLE cosign_usage (signature {blob} NOT NULL, ring {blob} NOT NULL, owner {blob} NOT NULL, asset {blob} NOT NULL, slot BIGINT NOT NULL, outflow_hi BIGINT NOT NULL, outflow_lo BIGINT NOT NULL, deposited_hi BIGINT NOT NULL, deposited_lo BIGINT NOT NULL, withdrawn_hi BIGINT NOT NULL, withdrawn_lo BIGINT NOT NULL, exiting_hi BIGINT NOT NULL, exiting_lo BIGINT NOT NULL, state TEXT NOT NULL CHECK(state IN ('held','confirmed','released')), PRIMARY KEY(signature, owner, asset))"),
        format!("CREATE TABLE cosign_totals (ring {blob} NOT NULL, asset {blob} NOT NULL, epoch BIGINT NOT NULL, scope_kind BIGINT NOT NULL, owner {blob} NOT NULL, node BIGINT NOT NULL, outflow {blob} NOT NULL, deposited {blob} NOT NULL, withdrawn {blob} NOT NULL, exiting {blob} NOT NULL, PRIMARY KEY(ring, asset, epoch, scope_kind, owner, node))"),
        format!("CREATE TABLE cosign_wallet_sends (ticket {blob} NOT NULL PRIMARY KEY, ring {blob} NOT NULL, last_valid_block_height BIGINT NOT NULL, expired_slot BIGINT)"),
        format!("CREATE TABLE cosign_wallet_inputs (ring {blob} NOT NULL, nullifier {blob} NOT NULL, ticket {blob} NOT NULL, PRIMARY KEY(ring, nullifier))"),
        "CREATE INDEX cosign_wallet_inputs_ticket ON cosign_wallet_inputs(ticket)".to_owned(),
    ]
}

pub(super) fn list_entries(blob: &str) -> String {
    format!(
        "CREATE TABLE cosign_list_entries (ring {blob} NOT NULL, list TEXT NOT NULL \
         CHECK(list IN ('allow','block','freeze')), kind TEXT NOT NULL \
         CHECK(kind IN ('owner','asset','destination')), subject {blob} NOT NULL, \
         PRIMARY KEY(ring, list, kind, subject))"
    )
}
