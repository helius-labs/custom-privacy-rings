use hkdf::hmac::{Hmac, Mac};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseTransaction, TransactionTrait};
use sha2::Sha256;
use spr_keypair::KeypairError;
use zeroize::Zeroizing;
use zolana_ring_client::rpc::ReaderGrantAction;

use ring_service_core::{database::Store, keys::GRANT_ROW_KEY_INFO, sql};

use crate::{error::RingAuditError, grants::GrantQuery, row_cipher::RowCipher};

const ACTION_AND_TIMESTAMP_LEN: usize = 1 + 8;

pub struct GrantRows {
    store: Store,
    key: GrantRowKey,
}

pub(crate) struct GrantRowKey {
    cipher: RowCipher,
    lookup: Zeroizing<[u8; 32]>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub(crate) struct GrantRow {
    pub action: ReaderGrantAction,
    pub timestamp: u64,
}

struct ScopedGrant {
    scope: [u8; 32],
    row: GrantRow,
}

impl GrantRowKey {
    pub(crate) fn derive(secret: &[u8]) -> Result<Self, RingAuditError> {
        let mut okm = Zeroizing::new([0u8; 64]);
        hkdf::Hkdf::<Sha256>::new(None, secret)
            .expand(GRANT_ROW_KEY_INFO, okm.as_mut_slice())
            .map_err(|_| KeypairError::Hkdf)?;
        // Disjoint halves, the cipher key never keys the lookup.
        let (cipher, lookup) = okm.split_first_chunk().ok_or(KeypairError::Hkdf)?;
        Ok(Self {
            cipher: RowCipher::new(cipher),
            lookup: Zeroizing::new(lookup.try_into().map_err(|_| KeypairError::Hkdf)?),
        })
    }

    fn tag(&self, fields: &[&[u8]]) -> Result<[u8; 32], RingAuditError> {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(self.lookup.as_slice())
            .map_err(|_| KeypairError::Hkdf)?;
        for field in fields {
            mac.update(field);
        }
        Ok(mac.finalize().into_bytes().into())
    }

    fn scope(&self, query: GrantQuery) -> Result<[u8; 32], RingAuditError> {
        let reader = self.tag(&[&query.reader.to_bytes()])?;
        self.tag(&[
            &reader,
            query.ring.as_ref(),
            query.wallet.as_ref().map_or(&[], AsRef::as_ref),
        ])
    }

    fn open(&self, scope: &[u8; 32], sealed: &[u8]) -> Result<GrantRow, RingAuditError> {
        let bytes = self
            .cipher
            .open(scope, sealed)
            .map_err(|_| RingAuditError::InvalidGrantRow)?;
        GrantRow::decode(&bytes)
    }
}

impl GrantRow {
    fn encode(self) -> [u8; ACTION_AND_TIMESTAMP_LEN] {
        let mut bytes = [0; ACTION_AND_TIMESTAMP_LEN];
        bytes[0] = u8::from(self.action == ReaderGrantAction::Grant);
        bytes[1..].copy_from_slice(&self.timestamp.to_le_bytes());
        bytes
    }

    fn decode(bytes: &[u8]) -> Result<Self, RingAuditError> {
        let (action, timestamp) = bytes.split_first().ok_or(RingAuditError::InvalidGrantRow)?;
        let action = match action {
            0 => ReaderGrantAction::Revoke,
            1 => ReaderGrantAction::Grant,
            _ => return Err(RingAuditError::InvalidGrantRow),
        };
        Ok(Self {
            action,
            timestamp: u64::from_le_bytes(
                timestamp
                    .try_into()
                    .map_err(|_| RingAuditError::InvalidGrantRow)?,
            ),
        })
    }
}

impl GrantRows {
    pub(crate) async fn open(store: Store, key: GrantRowKey) -> Result<Self, RingAuditError> {
        let rows = Self { store, key };
        while rows.import_batch().await? {}
        Ok(rows)
    }

    pub(crate) async fn write(
        &self,
        query: GrantQuery,
        row: GrantRow,
    ) -> Result<ReaderGrantAction, RingAuditError> {
        let transaction = self.store.connection().begin().await?;
        let grant = ScopedGrant {
            scope: self.key.scope(query)?,
            row,
        };
        let action = self.replace(&transaction, grant).await?;
        transaction.commit().await?;
        Ok(action)
    }

    pub(crate) async fn grants(&self, query: GrantQuery) -> Result<bool, RingAuditError> {
        let scope = self.key.scope(query)?;
        let ring = match query.wallet {
            Some(_) => self.key.scope(GrantQuery {
                wallet: None,
                ..query
            })?,
            None => scope,
        };
        let rows = sql::Query::new(
            "SELECT scope, sealed FROM grants WHERE scope IN ($1, $2)",
            vec![sql::bytes(&scope), sql::bytes(&ring)],
        )
        .all(self.store.connection())
        .await?;
        let mut granted = false;
        for row in rows {
            let scope = sql::read_bytes(&row, "scope")?;
            let sealed: Vec<u8> = row.try_get("", "sealed")?;
            granted |= self.key.open(&scope, &sealed)?.action == ReaderGrantAction::Grant;
        }
        Ok(granted)
    }

    async fn replace(
        &self,
        tx: &DatabaseTransaction,
        grant: ScopedGrant,
    ) -> Result<ReaderGrantAction, RingAuditError> {
        let ScopedGrant { scope, row } = grant;
        let sealed = self
            .key
            .cipher
            .seal(&scope, &row.encode())
            .map_err(|_| RingAuditError::InvalidGrantRow)?;
        // The scope lock orders writes with equal signed timestamps.
        let held = sql::Query::new(
            "INSERT INTO grants(scope, sealed) VALUES($1, $2) ON CONFLICT(scope) DO UPDATE SET sealed=grants.sealed RETURNING sealed",
            vec![sql::bytes(&scope), sql::bytes(&sealed)],
        ).one(tx).await?.ok_or(RingAuditError::InvalidGrantRow)?;
        let current = self
            .key
            .open(&scope, &held.try_get::<Vec<u8>>("", "sealed")?)?;
        if current.timestamp > row.timestamp || current == row {
            return Ok(current.action);
        }
        sql::Query::new(
            "UPDATE grants SET sealed=$1 WHERE scope=$2",
            vec![sealed.into(), sql::bytes(&scope)],
        )
        .execute(tx)
        .await?;
        Ok(row.action)
    }

    async fn import_batch(&self) -> Result<bool, RingAuditError> {
        let tx = self.store.connection().begin().await?;
        if tx.get_database_backend() == DatabaseBackend::Postgres {
            tx.execute_unprepared("LOCK TABLE legacy_grants IN EXCLUSIVE MODE")
                .await?;
        }
        let batch = sql::Query::new(
            "DELETE FROM legacy_grants WHERE id IN (SELECT id FROM legacy_grants ORDER BY id LIMIT $1) RETURNING id, reader, sealed",
            vec![sql::count(sql::KEYS_PER_STATEMENT)?],
        ).all(&tx).await?;
        let more = batch.len() == sql::KEYS_PER_STATEMENT;
        let mut latest = std::collections::BTreeMap::new();
        for row in batch {
            let id = sql::read_integer(&row, "id")?;
            let reader = sql::read_bytes::<32>(&row, "reader")?;
            let sealed: Vec<u8> = row.try_get("", "sealed")?;
            let body = self
                .key
                .cipher
                .open(&reader, &sealed)
                .map_err(|_| RingAuditError::InvalidGrantRow)?;
            let (scope, value) = body
                .split_last_chunk::<ACTION_AND_TIMESTAMP_LEN>()
                .ok_or(RingAuditError::InvalidGrantRow)?;
            if !matches!(scope.len(), 32 | 64) {
                return Err(RingAuditError::InvalidGrantRow);
            }
            let scope = self.key.tag(&[&reader, scope])?;
            let grant = GrantRow::decode(value)?;
            let entry = latest.entry(scope).or_insert((grant, id));
            if (grant.timestamp, id) > (entry.0.timestamp, entry.1) {
                *entry = (grant, id);
            }
        }
        for (scope, (row, _)) in latest {
            self.replace(&tx, ScopedGrant { scope, row }).await?;
        }
        tx.commit().await?;
        Ok(more)
    }
}

#[cfg(test)]
mod tests {
    use solana_address::Address;
    use solana_signer::Signer;
    use spr_keypair::X25519Keypair;
    use zolana_ring_client::ReaderKey;

    use super::*;
    use crate::spent_index::NoteSeal;

    const SECRET: [u8; 32] = [0x41; 32];
    const RING: Address = Address::new_from_array([8; 32]);
    /// A stored ring-wide grant of `reader(1)` at second 7 under `SECRET`.
    const SEALED_GRANT: [u8; 69] = [
        99, 17, 137, 154, 200, 43, 165, 11, 161, 109, 15, 108, 93, 215, 26, 145, 73, 131, 167, 0,
        172, 201, 153, 97, 195, 217, 174, 0, 195, 110, 216, 29, 66, 129, 111, 177, 3, 204, 98, 151,
        68, 193, 128, 38, 225, 222, 146, 106, 3, 73, 198, 102, 70, 84, 63, 14, 88, 61, 21, 27, 114,
        168, 40, 7, 47, 253, 216, 55, 80,
    ];

    fn reader(seed: u8) -> ReaderKey {
        ReaderKey::ed25519(solana_keypair::Keypair::new_from_array([seed; 32]).pubkey())
            .expect("reader")
    }

    fn query() -> GrantQuery {
        GrantQuery {
            ring: RING,
            reader: reader(1),
            wallet: None,
        }
    }

    const fn grant(timestamp: u64) -> GrantRow {
        GrantRow {
            action: ReaderGrantAction::Grant,
            timestamp,
        }
    }

    const fn revoke(timestamp: u64) -> GrantRow {
        GrantRow {
            action: ReaderGrantAction::Revoke,
            timestamp,
        }
    }

    async fn legacy(store: &Store, tag: [u8; 32], sealed: &[u8]) {
        sql::Query::new(
            "INSERT INTO legacy_grants(reader, sealed) VALUES ($1, $2)",
            vec![sql::bytes(&tag), sql::bytes(sealed)],
        )
        .execute(store.connection())
        .await
        .expect("legacy row");
    }

    fn key() -> GrantRowKey {
        GrantRowKey::derive(&SECRET).expect("key")
    }

    #[tokio::test]
    async fn a_corrupt_legacy_batch_refuses_startup_and_keeps_its_rows() {
        let store = Store::in_memory().await.expect("store");
        let tag = key().tag(&[&reader(1).to_bytes()]).expect("tag");
        legacy(&store, tag, &SEALED_GRANT).await;
        for corrupt in [&SEALED_GRANT[..], &SEALED_GRANT[..11]] {
            let other = key().tag(&[&reader(2).to_bytes()]).expect("other reader");
            legacy(&store, other, corrupt).await;
            assert!(matches!(
                GrantRows::open(store.clone(), key()).await,
                Err(RingAuditError::InvalidGrantRow)
            ));
            assert_eq!(count(&store, "legacy_grants").await, 2);
            assert_eq!(count(&store, "grants").await, 0);
            sql::Query::new(
                "DELETE FROM legacy_grants WHERE reader=$1",
                vec![sql::bytes(&other)],
            )
            .execute(store.connection())
            .await
            .expect("repair");
        }
        let rows = GrantRows::open(store.clone(), key()).await.expect("reopen");
        assert!(rows.grants(query()).await.expect("original grant"));
        assert_eq!(count(&store, "legacy_grants").await, 0);
        assert_eq!(count(&store, "grants").await, 1);
    }

    async fn count(store: &Store, table: &str) -> i64 {
        sql::Query::new(format!("SELECT COUNT(*) AS n FROM {table}"), vec![])
            .one(store.connection())
            .await
            .expect("query")
            .expect("row")
            .try_get("", "n")
            .expect("count")
    }

    async fn concurrent_grants(url: &str) {
        let first = Store::open(url, 2).await.expect("first store");
        let second = Store::open(url, 2).await.expect("second store");
        let tag = key().tag(&[&reader(1).to_bytes()]).expect("tag");
        let wallet = GrantQuery {
            wallet: Some(Address::new_from_array([22; 32])),
            ..query()
        };
        let other = GrantQuery {
            ring: Address::new_from_array([4; 32]),
            ..wallet
        };
        let wallet_body = [
            RING.as_ref(),
            wallet.wallet.as_ref().expect("wallet").as_ref(),
            &grant(8).encode(),
        ]
        .concat();
        let wallet_sealed = key()
            .cipher
            .seal(&tag, &wallet_body)
            .expect("legacy wallet");
        let page = first.connection().begin().await.expect("transaction");
        for _ in 0..sql::KEYS_PER_STATEMENT {
            sql::Query::new(
                "INSERT INTO legacy_grants(reader, sealed) VALUES($1, $2)",
                vec![sql::bytes(&tag), sql::bytes(&SEALED_GRANT)],
            )
            .execute(&page)
            .await
            .expect("old ring grants");
        }
        page.commit().await.expect("commit");
        let later = [RING.as_ref(), &revoke(7).encode()].concat();
        legacy(
            &first,
            tag,
            &key().cipher.seal(&tag, &later).expect("same timestamp"),
        )
        .await;
        legacy(&first, tag, &wallet_sealed).await;
        let (a, b) = tokio::join!(
            GrantRows::open(first.clone(), key()),
            GrantRows::open(second, key())
        );
        let (a, b) = (a.expect("first boot"), b.expect("concurrent boot"));
        assert_eq!(count(&first, "legacy_grants").await, 0);
        assert_eq!(count(&first, "grants").await, 2);
        assert!(!a.grants(query()).await.expect("later revoke wins"));
        assert!(b.grants(wallet).await.expect("own wallet"));
        assert!(!b.grants(other).await.expect("other ring"));
        let (older, newer) = tokio::join!(a.write(wallet, grant(20)), b.write(wallet, revoke(21)));
        older.expect("older");
        assert_eq!(newer.expect("newer"), ReaderGrantAction::Revoke);
        assert!(!a.grants(wallet).await.expect("newest"));
        assert_eq!(
            a.write(wallet, grant(19)).await.expect("stale"),
            ReaderGrantAction::Revoke
        );
        assert_eq!(
            a.write(wallet, grant(21)).await.expect("tie"),
            ReaderGrantAction::Grant
        );
        assert!(b.grants(wallet).await.expect("later tie wins"));
        a.write(query(), grant(30)).await.expect("ring grant");
        b.write(wallet, revoke(40)).await.expect("wallet revoke");
        assert!(a.grants(wallet).await.expect("ring wide"));
        assert_eq!(count(&first, "grants").await, 2);
        b.write(other, grant(10)).await.expect("other ring");
        let columns = sql::Query::new("SELECT * FROM grants LIMIT 1", vec![])
            .one(first.connection())
            .await
            .expect("query")
            .expect("grant")
            .column_names();
        assert_eq!(columns, ["scope", "sealed"]);
        sql::Query::new(
            "UPDATE grants SET sealed=(SELECT sealed FROM grants WHERE scope=$1) WHERE scope=$2",
            vec![
                sql::bytes(&key().scope(wallet).expect("wallet scope")),
                sql::bytes(&key().scope(other).expect("other scope")),
            ],
        )
        .execute(first.connection())
        .await
        .expect("corrupt unrelated scope");
        assert!(matches!(
            b.grants(other).await,
            Err(RingAuditError::InvalidGrantRow)
        ));
        assert!(a
            .grants(wallet)
            .await
            .expect("unrelated ciphertext stays unopened"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn encrypted_grants_compact_across_replicas_on_sqlite() {
        let path = ring_service_core::database::fixture::StoreDirectory::new("grant-scopes");
        concurrent_grants(&path.url()).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "needs RING_RPC_TEST_POSTGRES_URL"]
    async fn encrypted_grants_compact_across_replicas_on_postgres() {
        let url = std::env::var("RING_RPC_TEST_POSTGRES_URL").expect("postgres url");
        let store = Store::open(&url, 1).await.expect("postgres");
        for table in ["grants", "legacy_grants"] {
            sql::Query::new(format!("DELETE FROM {table}"), vec![])
                .execute(store.connection())
                .await
                .expect("clear");
        }
        concurrent_grants(&url).await;
    }

    #[test]
    fn grant_actions_and_timestamps_have_one_encoding() {
        for state in [grant(0), revoke(u64::MAX)] {
            let bytes = state.encode();
            assert_eq!(GrantRow::decode(&bytes).expect("valid"), state);
            assert!(GrantRow::decode(&bytes[1..]).is_err());
            assert!(GrantRow::decode(&[bytes.as_slice(), &[0]].concat()).is_err());
            let mut invalid = bytes;
            invalid[0] = 2;
            assert!(GrantRow::decode(&invalid).is_err());
        }
    }

    #[test]
    fn grant_and_note_keys_differ_under_one_secret() {
        let grants = GrantRowKey::derive(&SECRET).expect("key");
        let notes = NoteSeal::derive(&X25519Keypair::from_bytes(&SECRET), RING).expect("seal");
        let row_key = [0x22; 32];
        let sealed = notes.seal(&row_key, b"row").expect("sealed");
        let aad = [RING.as_ref(), &row_key].concat();

        assert_eq!(notes.open(&row_key, &sealed).expect("opens"), b"row");
        assert!(grants.cipher.open(&aad, &sealed).is_err());
    }
}
