use sea_orm::{ConnectionTrait, DbErr};
use solana_address::Address;
use zolana_ring_client::rpc::RelayId;

use crate::relays::RelayPhase;
use ring_service_core::sql;

/// One row per nullifier, a second merge of the same note fails its insert.
pub(crate) struct MergeInputs<'a, C> {
    pub conn: &'a C,
}

#[must_use]
pub(crate) struct HoldInputs<'a> {
    pub ring: &'a Address,
    pub relay_id: &'a RelayId,
    pub nullifiers: &'a [[u8; 32]],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeldInput {
    pub nullifier: [u8; 32],
    pub phase: RelayPhase,
}

impl<C: ConnectionTrait> MergeInputs<'_, C> {
    pub async fn hold(&self, hold: HoldInputs<'_>) -> Result<(), DbErr> {
        let insert = "INSERT INTO merge_inputs(nullifier,ring,relay_id) VALUES($1,$2,$3)";
        for nullifier in hold.nullifiers {
            let values = vec![
                sql::bytes(nullifier),
                sql::bytes(hold.ring.as_ref()),
                sql::bytes(&hold.relay_id.0 .0),
            ];
            self.conn
                .execute(sql::statement(self.conn, insert, values))
                .await?;
        }
        Ok(())
    }

    pub async fn held(&self, ring: &Address) -> Result<Vec<HeldInput>, DbErr> {
        let select = "SELECT merge_inputs.nullifier, relay_relays.status FROM merge_inputs \
                      JOIN relay_relays ON relay_relays.relay_id = merge_inputs.relay_id \
                      WHERE merge_inputs.ring=$1";
        let values = vec![sql::bytes(ring.as_ref())];
        self.conn
            .query_all(sql::statement(self.conn, select, values))
            .await?
            .iter()
            .map(|row| {
                let status: String = row.try_get("", "status")?;
                Ok(HeldInput {
                    nullifier: sql::read_bytes(row, "nullifier")?,
                    phase: RelayPhase::of_status(&status)?,
                })
            })
            .collect()
    }

    pub async fn release(&self, nullifiers: &[[u8; 32]]) -> Result<(), DbErr> {
        let delete = "DELETE FROM merge_inputs WHERE nullifier=$1";
        for nullifier in nullifiers {
            let values = vec![sql::bytes(nullifier)];
            self.conn
                .execute(sql::statement(self.conn, delete, values))
                .await?;
        }
        Ok(())
    }
}
