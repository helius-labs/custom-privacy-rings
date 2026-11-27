use std::collections::{BTreeMap, BTreeSet, HashMap};

use sea_orm::{ConnectionTrait, DatabaseTransaction, DbErr, QueryResult, Value};
use solana_address::Address;

use crate::{
    cosign::{AssetFlow, CoSignError, CoSignSettlement, OwnerFlow, Ticket},
    error::RingCosignerError,
};
use ring_service_core::sql::{self, placeholders, read_address, read_bytes, read_integer};

#[path = "cosign_window_index.rs"]
mod window_index;

use window_index::{
    range_totals, Amounts, ChangeKind, Point, PointChange, Scope, WindowRange, EPOCH_SLOTS,
};

const MEASURE_CEILING: u128 = 1 << 64;
const RESERVATION_BATCH: usize = 64;
const PRUNE_NODES: usize = 64;
pub(crate) const MAX_USAGE_FLOWS: usize =
    spr_interface::MAX_TRANSACT_INPUTS + spr_interface::MAX_INTERFACE_TRANSFERS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Measure {
    Outflow,
    Volume,
    Deposited,
    Withdrawn,
    Exiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReservationState {
    Held,
    Confirmed,
    Released,
}

#[derive(Clone, Copy)]
pub(crate) struct AssetScope<'a> {
    pub ring: &'a Address,
    pub assets: &'a [Address],
}

#[derive(Clone, Copy)]
pub(crate) struct AdmissionSlot<'a> {
    pub scope: AssetScope<'a>,
    pub slot: u64,
}

pub(crate) struct SettlementScope {
    ring: Address,
    assets: Vec<Address>,
}

#[derive(Clone, Copy)]
pub(crate) struct WindowSum<'a> {
    pub ring: &'a Address,
    pub asset: &'a Address,
    pub measure: Measure,
    pub slot: u64,
    pub window_slots: u64,
}

pub(crate) struct Reserve<'a> {
    pub ticket: &'a Ticket,
    pub ring: &'a Address,
    pub slot: u64,
    pub flows: &'a [OwnerFlow],
}

pub(crate) struct UsageLedger<'a, C> {
    pub conn: &'a C,
}

/// A reservation whose message the wallet sends, the sweep learns its end from the ring history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WalletSend {
    pub ticket: Ticket,
    pub ring: Address,
    /// No block past it lands a message of the reservation.
    pub last_valid_block_height: u64,
    /// A finalized slot past `last_valid_block_height`, `None` until the sweep reads one.
    pub expired_slot: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WalletInputs {
    pub nullifiers: Vec<[u8; 32]>,
    pub funding_accounts: BTreeSet<Address>,
}

pub(crate) struct TrackSend<'a> {
    pub ticket: &'a Ticket,
    pub ring: &'a Address,
    pub last_valid_block_height: u64,
    pub inputs: &'a WalletInputs,
}

struct Reservation {
    ring: Address,
    owner: [u8; 32],
    asset: Address,
    slot: u64,
    state: ReservationState,
    amounts: Amounts,
}

struct AssetFilter {
    sql: String,
    values: Vec<Value>,
}

impl Measure {
    pub fn of(self, flow: &AssetFlow) -> Option<u128> {
        Amounts::from(flow).measure(self)
    }
}

impl ReservationState {
    fn read(value: &str) -> Result<Self, DbErr> {
        match value {
            "held" => Ok(Self::Held),
            "confirmed" => Ok(Self::Confirmed),
            "released" => Ok(Self::Released),
            _ => Err(DbErr::Custom("invalid usage state".into())),
        }
    }

    const fn stored(self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::Confirmed => "confirmed",
            Self::Released => "released",
        }
    }
}

impl AssetScope<'_> {
    fn filter(self) -> AssetFilter {
        let mut values = vec![address_value(self.ring)];
        values.extend(self.assets.iter().map(address_value));
        AssetFilter {
            sql: format!(
                "ring=$1 AND asset IN ({})",
                placeholders(2, self.assets.len())
            ),
            values,
        }
    }
}

impl SettlementScope {
    pub(crate) fn as_scope(&self) -> AssetScope<'_> {
        AssetScope {
            ring: &self.ring,
            assets: &self.assets,
        }
    }
}

impl<C: ConnectionTrait> UsageLedger<'_, C> {
    pub(crate) async fn reservation_state(
        &self,
        ticket: &Ticket,
    ) -> Result<Option<ReservationState>, DbErr> {
        let row = sql::Query::new(
            "SELECT state FROM cosign_usage WHERE signature=$1 LIMIT 1",
            vec![ticket_value(ticket)],
        )
        .one(self.conn)
        .await?;
        row.map(|row| ReservationState::read(&row.try_get::<String>("", "state")?))
            .transpose()
    }

    pub(crate) async fn settlement_scope(
        &self,
        ticket: &Ticket,
    ) -> Result<Option<SettlementScope>, DbErr> {
        let rows = self.reservations(ticket).await?;
        let Some(first) = rows.first() else {
            return Ok(None);
        };
        if rows
            .iter()
            .any(|row| row.ring != first.ring || row.state != first.state)
        {
            return Err(DbErr::Custom("inconsistent usage reservation".into()));
        }
        Ok(Some(SettlementScope {
            ring: first.ring,
            assets: rows
                .iter()
                .map(|row| row.asset)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        }))
    }

    /// The newest slot a reservation of the scope holds, zero before the first.
    pub(crate) async fn watermark(&self, scope: AssetScope<'_>) -> Result<u64, DbErr> {
        Ok(self
            .watermarks(scope)
            .await?
            .into_values()
            .max()
            .unwrap_or_default())
    }

    /// Whether the reservation holds exactly what `flows` moves.
    pub async fn holds(&self, ticket: &Ticket, flows: &[OwnerFlow]) -> Result<bool, DbErr> {
        let mut held: Vec<_> = self
            .reservations(ticket)
            .await?
            .iter()
            .map(|row| (row.owner, row.asset, row.amounts))
            .collect();
        let mut moved: Vec<_> = flows
            .iter()
            .map(|flow| {
                (
                    *flow.owner.as_bytes(),
                    flow.flow.asset,
                    Amounts::from(&flow.flow).clipped(),
                )
            })
            .collect();
        held.sort_unstable_by_key(|(owner, asset, _)| (*owner, *asset));
        moved.sort_unstable_by_key(|(owner, asset, _)| (*owner, *asset));
        Ok(held == moved)
    }

    /// The live reservations holding one of `nullifiers`.
    pub(crate) async fn input_holders(
        &self,
        ring: &Address,
        nullifiers: &[[u8; 32]],
    ) -> Result<Vec<WalletSend>, DbErr> {
        let sql = format!(
            "SELECT {SEND_COLUMNS} FROM cosign_wallet_sends WHERE ticket IN \
             (SELECT ticket FROM cosign_wallet_inputs WHERE ring=$1 AND nullifier IN ({}))",
            placeholders(2, nullifiers.len())
        );
        let mut values = vec![address_value(ring)];
        values.extend(nullifiers.iter().map(|nullifier| sql::bytes(nullifier)));
        sql::Query::new(sql, values)
            .all(self.conn)
            .await?
            .iter()
            .map(WalletSend::read)
            .collect()
    }

    /// Ticket order past `after`, one bounded page of the sweep.
    pub(crate) async fn wallet_sends(
        &self,
        after: Option<&Ticket>,
        limit: u64,
    ) -> Result<Vec<WalletSend>, DbErr> {
        let (filter, mut values) = match after {
            Some(ticket) => ("WHERE ticket>$1", vec![ticket_value(ticket)]),
            None => ("", Vec::new()),
        };
        values.push(sql::integer(limit)?);
        let sql = format!(
            "SELECT {SEND_COLUMNS} FROM cosign_wallet_sends {filter} ORDER BY ticket LIMIT ${}",
            values.len()
        );
        sql::Query::new(sql, values)
            .all(self.conn)
            .await?
            .iter()
            .map(WalletSend::read)
            .collect()
    }

    pub(crate) async fn send_nullifiers(
        &self,
        ticket: &Ticket,
    ) -> Result<BTreeSet<[u8; 32]>, DbErr> {
        sql::Query::new(
            "SELECT nullifier FROM cosign_wallet_inputs WHERE ticket=$1",
            vec![ticket_value(ticket)],
        )
        .all(self.conn)
        .await?
        .iter()
        .map(|row| read_bytes(row, "nullifier"))
        .collect()
    }

    pub(crate) async fn funding_accounts(
        &self,
        ticket: &Ticket,
    ) -> Result<BTreeSet<Address>, DbErr> {
        sql::Query::new(
            "SELECT account FROM cosign_wallet_funding_accounts WHERE ticket=$1",
            vec![ticket_value(ticket)],
        )
        .all(self.conn)
        .await?
        .iter()
        .map(|row| read_address(row, "account"))
        .collect()
    }

    /// Keeps the first slot it reads, a later tick never moves the proof of expiry.
    pub(crate) async fn mark_expired(&self, ticket: &Ticket, slot: u64) -> Result<(), DbErr> {
        sql::Query::new("UPDATE cosign_wallet_sends SET expired_slot=$2 WHERE ticket=$1 AND expired_slot IS NULL", vec![ticket_value(ticket), sql::integer(slot)?]).execute(self.conn)
            .await?;
        Ok(())
    }

    pub async fn ring_total(&self, sum: WindowSum<'_>) -> Result<u128, DbErr> {
        let ranges = [sum.range(None)];
        let totals = range_totals(self.conn, &ranges).await?;
        totals
            .first()
            .copied()
            .ok_or_else(|| DbErr::Custom("missing usage total".into()))?
            .measure(sum.measure)
            .ok_or_else(|| DbErr::Custom("usage measure exceeds its counter range".into()))
    }

    pub(crate) async fn owner_totals(
        &self,
        sum: WindowSum<'_>,
        owners: &[[u8; 32]],
    ) -> Result<HashMap<[u8; 32], u128>, DbErr> {
        let ranges: Vec<_> = owners.iter().map(|owner| sum.range(Some(*owner))).collect();
        owners
            .iter()
            .copied()
            .zip(range_totals(self.conn, &ranges).await?)
            .map(|(owner, amounts)| {
                amounts
                    .measure(sum.measure)
                    .map(|total| (owner, total))
                    .ok_or_else(|| DbErr::Custom("usage measure exceeds its counter range".into()))
            })
            .collect()
    }

    async fn watermarks(&self, scope: AssetScope<'_>) -> Result<BTreeMap<Address, u64>, DbErr> {
        if scope.assets.is_empty() {
            return Ok(BTreeMap::new());
        }
        let filter = scope.filter();
        let sql = format!(
            "SELECT asset,last_slot FROM cosign_admission WHERE {}",
            filter.sql
        );
        let mut slots = BTreeMap::new();
        for row in sql::Query::new(&sql, filter.values).all(self.conn).await? {
            let slot = read_integer(&row, "last_slot")?;
            slots.insert(read_address(&row, "asset")?, slot);
        }
        if slots.len() != scope.assets.len() {
            return Err(DbErr::Custom("missing usage admission lock".into()));
        }
        Ok(slots)
    }

    async fn reservations(&self, ticket: &Ticket) -> Result<Vec<Reservation>, DbErr> {
        let sql = format!(
            "SELECT ring,owner,asset,slot,state,outflow_hi,outflow_lo,deposited_hi,deposited_lo,withdrawn_hi,withdrawn_lo,exiting_hi,exiting_lo FROM cosign_usage WHERE signature=$1 LIMIT {}",
            MAX_USAGE_FLOWS + 1
        );
        let rows = sql::Query::new(&sql, vec![ticket_value(ticket)])
            .all(self.conn)
            .await?;
        if rows.len() > MAX_USAGE_FLOWS {
            return Err(DbErr::Custom(
                "usage reservation exceeds its flow bound".into(),
            ));
        }
        rows.iter().map(Reservation::read).collect()
    }
}

impl UsageLedger<'_, DatabaseTransaction> {
    pub async fn admit(&self, scope: AssetScope<'_>) -> Result<(), DbErr> {
        let sql = "INSERT INTO cosign_admission(ring,asset) VALUES($1,$2) ON CONFLICT(ring,asset) DO UPDATE SET asset=excluded.asset";
        for asset in scope.assets.iter().collect::<BTreeSet<_>>() {
            sql::Query::new(sql, vec![address_value(scope.ring), address_value(asset)])
                .execute(self.conn)
                .await?;
        }
        Ok(())
    }

    pub async fn track(&self, send: TrackSend<'_>) -> Result<(), RingCosignerError> {
        sql::Query::new(
            "INSERT INTO cosign_wallet_sends(ticket,ring,last_valid_block_height) VALUES($1,$2,$3)",
            vec![
                ticket_value(send.ticket),
                address_value(send.ring),
                sql::integer(send.last_valid_block_height)?,
            ],
        )
        .execute(self.conn)
        .await?;
        for account in &send.inputs.funding_accounts {
            let claimed = sql::Query::new(
                "INSERT INTO cosign_wallet_funding_accounts(ring,account,ticket) VALUES($1,$2,$3) ON CONFLICT(ring,account) DO NOTHING",
                vec![address_value(send.ring), address_value(account), ticket_value(send.ticket)],
            )
            .execute(self.conn)
            .await?;
            if claimed == 0 {
                return Err(CoSignError::InputReserved.into());
            }
        }
        for nullifier in &send.inputs.nullifiers {
            sql::Query::new(
                "INSERT INTO cosign_wallet_inputs(ring,nullifier,ticket) VALUES($1,$2,$3)",
                vec![
                    address_value(send.ring),
                    sql::bytes(nullifier),
                    ticket_value(send.ticket),
                ],
            )
            .execute(self.conn)
            .await?;
        }
        Ok(())
    }

    /// False once a newer message of the transfer or another sweep took the row.
    pub(crate) async fn untrack(&self, ticket: &Ticket) -> Result<bool, DbErr> {
        let removed = sql::Query::new(
            "DELETE FROM cosign_wallet_sends WHERE ticket=$1",
            vec![ticket_value(ticket)],
        )
        .execute(self.conn)
        .await?;
        sql::Query::new(
            "DELETE FROM cosign_wallet_inputs WHERE ticket=$1",
            vec![ticket_value(ticket)],
        )
        .execute(self.conn)
        .await?;
        Ok(removed != 0)
    }

    pub async fn reserve(&self, reserve: Reserve<'_>) -> Result<(), DbErr> {
        if reserve.flows.len() > MAX_USAGE_FLOWS {
            return Err(DbErr::Custom(
                "usage reservation exceeds its flow bound".into(),
            ));
        }
        let rows: Vec<_> = reserve
            .flows
            .iter()
            .map(|flow| Reservation {
                ring: *reserve.ring,
                owner: *flow.owner.as_bytes(),
                asset: flow.flow.asset,
                slot: reserve.slot,
                state: ReservationState::Held,
                amounts: Amounts::from(&flow.flow).clipped(),
            })
            .collect();
        let assets: Vec<_> = rows
            .iter()
            .map(|row| row.asset)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let admission = AdmissionSlot {
            scope: AssetScope {
                ring: reserve.ring,
                assets: &assets,
            },
            slot: reserve.slot,
        };
        self.advance(admission).await?;
        for batch in rows.chunks(RESERVATION_BATCH) {
            let mut values = Vec::new();
            let mut slots = Vec::new();
            for row in batch {
                let start = values.len() + 1;
                slots.push(format!("({})", placeholders(start, 14)));
                values.extend([
                    ticket_value(reserve.ticket),
                    address_value(&row.asset),
                    address_value(&row.ring),
                    sql::bytes(&row.owner),
                    sql::integer(row.slot)?,
                ]);
                for amount in [
                    row.amounts.outflow,
                    row.amounts.deposited,
                    row.amounts.withdrawn,
                    row.amounts.exiting,
                ] {
                    values.extend(halves(amount)?);
                }
                values.push(Value::from(ReservationState::Held.stored()));
            }
            let sql = format!(
                "INSERT INTO cosign_usage(signature,asset,ring,owner,slot,outflow_hi,outflow_lo,deposited_hi,deposited_lo,withdrawn_hi,withdrawn_lo,exiting_hi,exiting_lo,state) VALUES {}",
                slots.join(",")
            );
            sql::Query::new(&sql, values).execute(self.conn).await?;
        }
        PointChange {
            points: rows.iter().flat_map(Reservation::points).collect(),
            kind: ChangeKind::Add,
        }
        .apply(self.conn)
        .await?;
        self.prune(admission.scope, admission.slot).await
    }

    pub async fn settle(&self, settlement: CoSignSettlement) -> Result<(), DbErr> {
        let (ticket, next) = match settlement {
            CoSignSettlement::Confirmed(ticket) => (ticket, ReservationState::Confirmed),
            CoSignSettlement::Failed(ticket) => (ticket, ReservationState::Released),
        };
        let rows = self.reservations(&ticket).await?;
        let Some(first) = rows.first() else {
            return Ok(());
        };
        if rows
            .iter()
            .any(|row| row.ring != first.ring || row.state != first.state)
        {
            return Err(DbErr::Custom("inconsistent usage reservation".into()));
        }
        match first.state {
            ReservationState::Confirmed => return Ok(()),
            ReservationState::Released => {
                return if next == ReservationState::Released {
                    Ok(())
                } else {
                    Err(DbErr::Custom(
                        "a released usage reservation cannot confirm".into(),
                    ))
                };
            }
            ReservationState::Held => {}
        }
        if next == ReservationState::Released {
            let assets: Vec<_> = rows
                .iter()
                .map(|row| row.asset)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let watermarks = self
                .watermarks(AssetScope {
                    ring: &first.ring,
                    assets: &assets,
                })
                .await?;
            let mut points = Vec::new();
            for row in &rows {
                let watermark = watermarks
                    .get(&row.asset)
                    .ok_or_else(|| DbErr::Custom("missing usage admission lock".into()))?;
                if row.slot / EPOCH_SLOTS >= (watermark / EPOCH_SLOTS).saturating_sub(1) {
                    points.extend(row.points());
                }
            }
            PointChange {
                points,
                kind: ChangeKind::Remove,
            }
            .apply(self.conn)
            .await?;
        }
        let sql = "UPDATE cosign_usage SET state=$2 WHERE signature=$1 AND state=$3";
        let changed = sql::Query::new(
            sql,
            vec![
                ticket_value(&ticket),
                Value::from(next.stored()),
                Value::from(ReservationState::Held.stored()),
            ],
        )
        .execute(self.conn)
        .await?;
        if changed != rows.len() as u64 {
            return Err(DbErr::Custom(
                "usage reservation changed during settlement".into(),
            ));
        }
        Ok(())
    }

    async fn advance(&self, admission: AdmissionSlot<'_>) -> Result<(), DbErr> {
        if admission.scope.assets.is_empty() {
            return Ok(());
        }
        let mut filter = admission.scope.filter();
        filter.values.push(sql::integer(admission.slot)?);
        let parameter = filter.values.len();
        let sql = format!("UPDATE cosign_admission SET last_slot=${parameter} WHERE {} AND last_slot<=${parameter}", filter.sql);
        let changed = sql::Query::new(&sql, filter.values)
            .execute(self.conn)
            .await?;
        if changed != admission.scope.assets.len() as u64 {
            return Err(DbErr::Custom("usage admission slot regressed".into()));
        }
        Ok(())
    }

    async fn prune(&self, scope: AssetScope<'_>, slot: u64) -> Result<(), DbErr> {
        let minimum_epoch = (slot / EPOCH_SLOTS).saturating_sub(1);
        if minimum_epoch == 0 {
            return Ok(());
        }
        let sql = format!("DELETE FROM cosign_totals WHERE (ring,asset,epoch,scope_kind,owner,node) IN (SELECT ring,asset,epoch,scope_kind,owner,node FROM cosign_totals WHERE ring=$1 AND asset=$2 AND epoch<$3 ORDER BY epoch,scope_kind,owner,node LIMIT {PRUNE_NODES})");
        for asset in scope.assets {
            sql::Query::new(
                &sql,
                vec![
                    address_value(scope.ring),
                    address_value(asset),
                    sql::integer(minimum_epoch)?,
                ],
            )
            .execute(self.conn)
            .await?;
        }
        Ok(())
    }
}

impl WindowSum<'_> {
    fn range(self, owner: Option<[u8; 32]>) -> WindowRange {
        WindowRange {
            scope: Scope {
                ring: *self.ring,
                asset: *self.asset,
                owner,
            },
            slot: self.slot,
            window_slots: self.window_slots,
        }
    }
}

impl Reservation {
    fn read(row: &QueryResult) -> Result<Self, DbErr> {
        let amount = |name| {
            let high = u128::from(read_integer(row, &format!("{name}_hi"))?);
            let low = u128::from(read_integer(row, &format!("{name}_lo"))?);
            let amount = (high << 32) + low;
            if low > u128::from(u32::MAX) || amount > MEASURE_CEILING {
                return Err(DbErr::Custom("invalid usage contribution".into()));
            }
            Ok(amount)
        };
        Ok(Self {
            ring: read_address(row, "ring")?,
            owner: read_bytes(row, "owner")?,
            asset: read_address(row, "asset")?,
            slot: read_integer(row, "slot")?,
            state: ReservationState::read(&row.try_get::<String>("", "state")?)?,
            amounts: Amounts {
                outflow: amount("outflow")?,
                deposited: amount("deposited")?,
                withdrawn: amount("withdrawn")?,
                exiting: amount("exiting")?,
            },
        })
    }

    fn points(&self) -> [Point; 2] {
        [
            Point {
                scope: Scope {
                    ring: self.ring,
                    asset: self.asset,
                    owner: None,
                },
                slot: self.slot,
                amounts: self.amounts,
            },
            Point {
                scope: Scope {
                    ring: self.ring,
                    asset: self.asset,
                    owner: Some(self.owner),
                },
                slot: self.slot,
                amounts: self.amounts.owner(),
            },
        ]
    }
}

fn halves(amount: u128) -> Result<[Value; 2], DbErr> {
    let stored = |half: u128| sql::integer(u64::try_from(half).unwrap_or(u64::MAX));
    Ok([
        stored(amount >> 32)?,
        stored(amount & u128::from(u32::MAX))?,
    ])
}

impl WalletSend {
    fn read(row: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            ticket: Ticket::from_bytes(read_bytes(row, "ticket")?),
            ring: read_address(row, "ring")?,
            last_valid_block_height: read_integer(row, "last_valid_block_height")?,
            expired_slot: sql::read_optional_integer(row, "expired_slot")?,
        })
    }
}

const SEND_COLUMNS: &str = "ticket,ring,last_valid_block_height,expired_slot";

fn ticket_value(ticket: &Ticket) -> Value {
    sql::bytes(ticket.as_bytes())
}

fn address_value(address: &Address) -> Value {
    sql::bytes(address.as_ref())
}
