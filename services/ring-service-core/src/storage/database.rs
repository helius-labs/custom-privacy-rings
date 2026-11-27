use std::time::Duration;

use sea_orm::{
    sqlx::{
        sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
        ConnectOptions as _,
    },
    ConnectOptions, Database, DatabaseConnection, DbErr, RuntimeErr, SqlxSqliteConnector,
};
use sea_orm_migration::MigratorTrait;

use sha2::{Digest, Sha256};

use crate::{error::ServiceError, migration::Migrator};

/// One flush at a time, the rest serve submits and status reads.
pub const STORE_CONNECTIONS: u32 = 4;
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

/// `replica` survives a restart, `instance` is drawn per open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeaseHolder {
    pub replica: [u8; 32],
    pub instance: [u8; 16],
}

/// Migrated when opened, clones share one pool.
#[derive(Clone)]
pub struct Store {
    db: DatabaseConnection,
    holder: LeaseHolder,
}

#[derive(Clone)]
pub enum ServiceStore {
    /// `--store-url`, the one store that turns on the parts it gates.
    Durable(Store),
    /// Lost at exit, it turns on nothing.
    Internal(Store),
}

impl Store {
    /// `url` is a `postgres://` or `sqlite://` URL.
    pub async fn open(url: &str, max_connections: u32) -> Result<Self, ServiceError> {
        let db = if url.starts_with("sqlite:") && !is_memory_url(url) {
            let options = url
                .parse::<SqliteConnectOptions>()
                .map_err(|error| DbErr::Conn(RuntimeErr::SqlxError(error)))?
                // Readers run beside the writer.
                .journal_mode(SqliteJournalMode::Wal)
                .busy_timeout(SQLITE_BUSY_TIMEOUT)
                .disable_statement_logging();
            let pool = SqlitePoolOptions::new()
                .max_connections(max_connections)
                .acquire_timeout(POOL_ACQUIRE_TIMEOUT)
                .connect_with(options)
                .await
                .map_err(|error| DbErr::Conn(RuntimeErr::SqlxError(error)))?;
            SqlxSqliteConnector::from_sqlx_sqlite_pool(pool)
        } else {
            Database::connect(
                ConnectOptions::new(url)
                    .max_connections(max_connections)
                    .acquire_timeout(POOL_ACQUIRE_TIMEOUT)
                    .sqlx_logging(false)
                    .to_owned(),
            )
            .await?
        };
        Self::migrated(db).await
    }

    pub async fn in_memory() -> Result<Self, ServiceError> {
        let options = "sqlite::memory:"
            .parse::<SqliteConnectOptions>()
            .map_err(|error| DbErr::Conn(RuntimeErr::SqlxError(error)))?
            .disable_statement_logging();
        // One connection for the process life, each connection to the URL opens another empty database.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .acquire_timeout(POOL_ACQUIRE_TIMEOUT)
            .connect_with(options)
            .await
            .map_err(|error| DbErr::Conn(RuntimeErr::SqlxError(error)))?;
        Self::migrated(SqlxSqliteConnector::from_sqlx_sqlite_pool(pool)).await
    }

    async fn migrated(db: DatabaseConnection) -> Result<Self, ServiceError> {
        Migrator::up(&db, None).await?;
        Ok(Self {
            db,
            holder: LeaseHolder {
                replica: rand::random(),
                instance: rand::random(),
            },
        })
    }

    /// A stable name lets a restart take its own lease at once, an unnamed store waits out its expiry.
    #[must_use]
    pub fn with_replica(mut self, name: &str) -> Self {
        self.holder.replica = Sha256::digest(name.as_bytes()).into();
        self
    }

    pub fn connection(&self) -> &DatabaseConnection {
        &self.db
    }

    pub const fn lease_holder(&self) -> LeaseHolder {
        self.holder
    }
}

impl ServiceStore {
    pub async fn internal() -> Result<Self, ServiceError> {
        Store::in_memory().await.map(Self::Internal)
    }

    pub const fn store(&self) -> &Store {
        match self {
            Self::Durable(store) | Self::Internal(store) => store,
        }
    }

    pub const fn durable(&self) -> Option<&Store> {
        match self {
            Self::Durable(store) => Some(store),
            Self::Internal(_) => None,
        }
    }
}

/// SQLite keeps a memory database only while a pool connection holds it.
pub fn is_memory_url(url: &str) -> bool {
    url.starts_with("sqlite:") && (url.contains(":memory:") || url.contains("mode=memory"))
}

#[cfg(any(test, feature = "test-fixtures"))]
pub mod fixture {
    use std::path::PathBuf;

    /// One directory per store, a removal takes the WAL files with the database.
    pub struct StoreDirectory(PathBuf);

    impl StoreDirectory {
        /// `name` is unique among the stores of one test process.
        pub fn new(name: &str) -> Self {
            let directory =
                std::env::temp_dir().join(format!("ring-store-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&directory);
            std::fs::create_dir_all(&directory).expect("store directory");
            Self(directory)
        }

        pub fn url(&self) -> String {
            format!(
                "sqlite://{}?mode=rwc",
                self.0.join("store.sqlite").display()
            )
        }
    }

    impl Drop for StoreDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_internal_store_keeps_one_database_across_acquisitions() {
        let store = Store::in_memory().await.expect("store");
        let writer = store.clone();
        tokio::spawn(async move {
            sql::Query::new(
                "INSERT INTO replay_nonces(ring,nonce,signed_at) VALUES($1,$2,$3)",
                vec![sql::bytes(&[1; 32]), sql::bytes(&[2; 32]), 3_i64.into()],
            )
            .execute(writer.connection())
            .await
            .expect("insert");
        })
        .await
        .expect("writer task");

        let count = tokio::spawn(async move {
            sql::Query::new("SELECT COUNT(*) AS rows FROM replay_nonces", Vec::new())
                .one(store.connection())
                .await
                .expect("select")
                .expect("row")
                .try_get::<i64>("", "rows")
                .expect("count")
        })
        .await
        .expect("reader task");
        assert_eq!(count, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_fresh_file_migrates_through_a_pool_and_reopens() {
        for attempt in 0..32 {
            let directory = fixture::StoreDirectory::new(&format!("fresh-{attempt}"));
            Store::open(&directory.url(), STORE_CONNECTIONS)
                .await
                .expect("fresh");
            Store::open(&directory.url(), STORE_CONNECTIONS)
                .await
                .expect("restart");
        }
    }
}
