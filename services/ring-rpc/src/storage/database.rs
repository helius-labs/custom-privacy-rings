use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseBackend, DatabaseConnection};
use sea_orm_migration::MigratorTrait;

use sha2::{Digest, Sha256};

use crate::{error::RingRpcError, migration::Migrator, projection_lease::LeaseHolder};

/// One flush at a time, the rest serve submits and status reads.
pub const STORE_CONNECTIONS: u32 = 4;

/// Migrated when opened, clones share one pool.
#[derive(Clone)]
pub struct Store {
    db: DatabaseConnection,
    holder: LeaseHolder,
}

impl Store {
    /// `url` is a `postgres://` or `sqlite://` URL.
    pub async fn open(url: &str, max_connections: u32) -> Result<Self, RingRpcError> {
        let db = Database::connect(
            ConnectOptions::new(url)
                .max_connections(max_connections)
                .sqlx_logging(false)
                .to_owned(),
        )
        .await?;
        if db.get_database_backend() == DatabaseBackend::Sqlite {
            // Readers run beside the writer.
            db.execute_unprepared("PRAGMA journal_mode=WAL").await?;
        }
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

    pub(crate) const fn lease_holder(&self) -> LeaseHolder {
        self.holder
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_fresh_file_migrates_through_a_pool_and_reopens() {
        let directory = std::env::temp_dir().join(format!("store-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("directory");
        for attempt in 0..32 {
            let path = directory.join(format!("{attempt}.sqlite"));
            let url = format!("sqlite://{}?mode=rwc", path.display());
            Store::open(&url, STORE_CONNECTIONS).await.expect("fresh");
            Store::open(&url, STORE_CONNECTIONS).await.expect("restart");
        }
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[tokio::test]
    async fn a_store_from_before_the_enrollment_rings_upgrades() {
        let directory = std::env::temp_dir().join(format!("store-upgrade-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("directory");
        let url = format!(
            "sqlite://{}?mode=rwc",
            directory.join("store.sqlite").display()
        );
        let first = Database::connect(url.as_str()).await.expect("connect");
        Migrator::up(&first, Some(1))
            .await
            .expect("first migration");
        let writer = solana_address::Address::new_from_array([9; 32]);
        let ring = solana_address::Address::new_from_array([13; 32]);
        let store = Store::open(&url, 1).await.expect("upgraded");
        let book = crate::enrollment_rings::EnrollmentRingBook::new(store.connection(), writer);
        book.insert(ring).await.expect("insert");
        assert_eq!(book.rings().await.expect("rings"), [ring]);
        let _ = std::fs::remove_dir_all(&directory);
    }
}
