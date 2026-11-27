use sea_orm::{DatabaseConnection, DbErr, QueryResult, TransactionTrait};
use solana_address::Address;
use zolana_ring_client::rpc::MergeSettings;

use crate::error::RingRelayError;
use ring_service_core::{database::Store, sql};

const SETTINGS_PER_PASS: usize = 64;

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
    pub(crate) fn open(store: Option<&'a Store>) -> Result<Self, RingRelayError> {
        let store = store.ok_or(RingRelayError::MergeSettingsUnavailable)?;
        Ok(Self {
            db: store.connection(),
        })
    }

    /// `false` when the stored row was signed at or after `signed_at`.
    pub(crate) async fn save(
        &self,
        signed: SignedMergeSettings<'_>,
    ) -> Result<bool, RingRelayError> {
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
        Ok(sql::Query::new(upsert, values).execute(self.db).await? == 1)
    }

    pub(crate) async fn rings(&self) -> Result<Vec<Address>, RingRelayError> {
        let select = "WITH RECURSIVE active(ring) AS (
            SELECT (SELECT ring FROM merge_settings WHERE target_note_count > 0 ORDER BY ring LIMIT 1)
            UNION ALL SELECT (SELECT s.ring FROM merge_settings s
                WHERE s.target_note_count > 0 AND s.ring > active.ring ORDER BY s.ring LIMIT 1)
            FROM active WHERE active.ring IS NOT NULL)
            SELECT ring FROM active WHERE ring IS NOT NULL";
        let rows = sql::Query::new(select, vec![]).all(self.db).await?;
        Ok(rows
            .iter()
            .map(|row| sql::read_address(row, "ring"))
            .collect::<Result<_, _>>()?)
    }

    pub(crate) async fn next_page(
        &self,
        ring: Address,
    ) -> Result<Vec<WalletMergeSettings>, RingRelayError> {
        let tx = self.db.begin().await?;
        // Empty cursor bytes sort before the zero address.
        sql::Query::new(
            "INSERT INTO merge_scan(ring, after_owner, after_mint) VALUES($1,$2,$2) ON CONFLICT(ring) DO UPDATE SET ring=excluded.ring",
            vec![sql::bytes(ring.as_ref()), sql::bytes(&[])],
        ).execute(&tx).await?;
        let select = "SELECT owner, mint, target_note_count, min_average_amount FROM merge_settings
            WHERE ring=$1 AND target_note_count > 0
            AND (owner, mint) > (SELECT after_owner, after_mint FROM merge_scan WHERE ring=$1)
            ORDER BY owner, mint LIMIT $2";
        let rows = sql::Query::new(
            select,
            vec![
                sql::bytes(ring.as_ref()),
                sql::count(SETTINGS_PER_PASS + 1)?,
            ],
        )
        .all(&tx)
        .await?;
        let page = rows
            .iter()
            .take(SETTINGS_PER_PASS)
            .map(wallet_settings)
            .collect::<Result<Vec<_>, _>>()?;
        let next = page.last().filter(|_| rows.len() > SETTINGS_PER_PASS);
        sql::Query::new(
            "UPDATE merge_scan SET after_owner=$2, after_mint=$3 WHERE ring=$1",
            vec![
                sql::bytes(ring.as_ref()),
                sql::bytes(next.map_or(&[], |setting| setting.owner.as_ref())),
                sql::bytes(next.map_or(&[], |setting| setting.settings.mint.0.as_ref())),
            ],
        )
        .execute(&tx)
        .await?;
        tx.commit().await?;
        Ok(page)
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

#[cfg(test)]
mod tests {
    use super::*;

    const RING: Address = Address::new_from_array([0x44; 32]);
    const OTHER_RING: Address = Address::new_from_array([0x45; 32]);

    fn settings(owner: u8, mint: u8) -> WalletMergeSettings {
        WalletMergeSettings {
            owner: Address::new_from_array([owner; 32]),
            settings: MergeSettings {
                mint: Address::new_from_array([mint; 32]).into(),
                target_note_count: 1,
                min_average_amount: 0,
            },
        }
    }

    async fn check_pages(url: &str) {
        let first = Store::open(url, 2).await.expect("first");
        let second = Store::open(url, 2).await.expect("second");
        let a = MergeSettingsBook::open(Some(&first)).expect("book");
        let b = MergeSettingsBook::open(Some(&second)).expect("book");
        let mut active = Vec::new();
        for i in 0..=2 * SETTINGS_PER_PASS {
            let item = if i < SETTINGS_PER_PASS + 1 {
                settings(0, u8::try_from(i).expect("mint"))
            } else {
                settings(1, u8::try_from(i).expect("mint"))
            };
            a.save(SignedMergeSettings {
                ring: RING,
                owner: item.owner,
                settings: &item.settings,
                signed_at: 1,
            })
            .await
            .expect("setting");
            active.push(item);
        }
        let other = settings(0, 0);
        a.save(SignedMergeSettings {
            ring: OTHER_RING,
            owner: other.owner,
            settings: &other.settings,
            signed_at: 1,
        })
        .await
        .expect("other ring");
        assert_eq!(a.rings().await.expect("active rings"), [RING, OTHER_RING]);

        let (p, q) = tokio::join!(a.next_page(RING), b.next_page(RING));
        let mut pages = [p.expect("first page"), q.expect("concurrent page")];
        pages.sort_by_key(|page| page[0].settings.mint.0);
        assert_eq!(pages[0], active[..SETTINGS_PER_PASS]);
        assert_eq!(pages[1], active[SETTINGS_PER_PASS..2 * SETTINGS_PER_PASS]);
        assert_eq!(b.next_page(OTHER_RING).await.expect("other ring"), [other]);
        drop((first, second));

        let reopened = Store::open(url, 2).await.expect("restart");
        let book = MergeSettingsBook::open(Some(&reopened)).expect("book");
        assert_eq!(
            book.next_page(RING).await.expect("resume"),
            active[2 * SETTINGS_PER_PASS..]
        );
        let disabled = MergeSettings {
            target_note_count: 0,
            ..active[0].settings
        };
        book.save(SignedMergeSettings {
            ring: RING,
            owner: active[0].owner,
            settings: &disabled,
            signed_at: 2,
        })
        .await
        .expect("disable");
        assert_eq!(
            book.next_page(RING).await.expect("retry"),
            active[1..=SETTINGS_PER_PASS]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn merge_settings_pages_resume_on_sqlite() {
        let path = ring_service_core::database::fixture::StoreDirectory::new("merge-scan");
        check_pages(&path.url()).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "needs RING_RPC_TEST_POSTGRES_URL"]
    async fn merge_settings_pages_resume_on_postgres() {
        let url = std::env::var("RING_RPC_TEST_POSTGRES_URL").expect("postgres url");
        let store = Store::open(&url, 2).await.expect("store");
        for table in ["merge_settings", "merge_scan"] {
            sql::Query::new(format!("DELETE FROM {table}"), vec![])
                .execute(store.connection())
                .await
                .expect("clear");
        }
        check_pages(&url).await;
    }
}
