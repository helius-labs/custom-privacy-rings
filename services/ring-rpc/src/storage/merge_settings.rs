use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, QueryResult};
use solana_address::Address;
use zolana_ring_client::rpc::MergeSettings;

use crate::error::RingRpcError;
use ring_service_core::sql;

#[must_use]
pub(crate) struct SignedMergeSettings<'a> {
    pub ring: Address,
    pub owner: Address,
    pub settings: &'a MergeSettings,
    /// The request timestamp, a later one replaces the row.
    pub signed_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WalletMergeSettings {
    pub owner: Address,
    pub settings: MergeSettings,
}

/// A zero target stays stored, an older request never revives it.
pub(crate) struct MergeSettingsBook<'a> {
    db: &'a DatabaseConnection,
}

impl<'a> MergeSettingsBook<'a> {
    pub(crate) fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    /// `false` when the stored row was signed at or after `signed_at`.
    pub(crate) async fn save(&self, signed: SignedMergeSettings<'_>) -> Result<bool, RingRpcError> {
        let upsert = "INSERT INTO merge_settings \
                      (ring, owner, mint, target_note_count, min_average_amount, signed_at) \
                      VALUES ($1, $2, $3, $4, $5, $6) \
                      ON CONFLICT(ring, owner, mint) DO UPDATE SET \
                      target_note_count=excluded.target_note_count, \
                      min_average_amount=excluded.min_average_amount, signed_at=excluded.signed_at \
                      WHERE merge_settings.signed_at < excluded.signed_at";
        let values = vec![
            sql::bytes(signed.ring.as_ref()),
            sql::bytes(signed.owner.as_ref()),
            sql::bytes(signed.settings.mint.0.as_ref()),
            i64::from(signed.settings.target_note_count).into(),
            sql::bytes(&signed.settings.min_average_amount.to_be_bytes()),
            sql::integer(signed.signed_at)?,
        ];
        let result = self
            .db
            .execute(sql::statement(self.db, upsert, values))
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub(crate) async fn rings(&self) -> Result<Vec<Address>, RingRpcError> {
        let select = "SELECT DISTINCT ring FROM merge_settings WHERE target_note_count > 0";
        let rows = self
            .db
            .query_all(sql::statement(self.db, select, vec![]))
            .await?;
        Ok(rows
            .iter()
            .map(|row| sql::read_address(row, "ring"))
            .collect::<Result<_, _>>()?)
    }

    pub(crate) async fn active(
        &self,
        ring: Address,
    ) -> Result<Vec<WalletMergeSettings>, RingRpcError> {
        let select =
            "SELECT owner, mint, target_note_count, min_average_amount FROM merge_settings \
                      WHERE ring=$1 AND target_note_count > 0";
        let rows = self
            .db
            .query_all(sql::statement(
                self.db,
                select,
                vec![sql::bytes(ring.as_ref())],
            ))
            .await?;
        Ok(rows.iter().map(wallet_settings).collect::<Result<_, _>>()?)
    }
}

fn wallet_settings(row: &QueryResult) -> Result<WalletMergeSettings, DbErr> {
    let target = row.try_get::<i64>("", "target_note_count")?;
    Ok(WalletMergeSettings {
        owner: sql::read_address(row, "owner")?,
        settings: MergeSettings {
            mint: sql::read_address(row, "mint")?.into(),
            target_note_count: u32::try_from(target).map_err(|_| {
                DbErr::Type("merge settings target_note_count is malformed".to_owned())
            })?,
            min_average_amount: u64::from_be_bytes(sql::read_bytes(row, "min_average_amount")?),
        },
    })
}
