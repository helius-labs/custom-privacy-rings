//! A member's viewing history, notes and scan positions between runs, and the scans that read them.
//!
//! Every ring mode keeps one ledger per identity and syncs it by the same tag read.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{ensure, Context as _, Result};
use solana_address::Address;
use spr_client::{BlockingIndexer, BlockingRpc, IndexerRpcConfig, Rpc};
use spr_keypair::{viewing_key::ViewTag, X25519Pubkey};
use spr_transaction::SpendableDecryptionResult;
use zolana_ring_client::{
    AnonymousError, AnonymousWallet, RecoveryStatus, RingWalletNotes, RingWalletSync,
    RingWalletSynced, TransferTags, ViewingHistory,
};
use zolana_ring_keys::RotatingRingKeys;
use zolana_ring_transaction::RingAssets;
use zolana_secret_file::{create_new, Visibility};

use crate::{file, Context, Zolana};

/// Under the `keys/` that `new` writes into `.gitignore`.
pub const ANONYMOUS_RING_DIR: &str = "keys/anonymous-ring";

/// One wallet's notes, spends and scan progress in one document, shared by every ring it holds notes in.
pub struct AnonymousWalletStore {
    path: PathBuf,
    viewing_pubkey: X25519Pubkey,
}

#[must_use]
pub struct StoreLocation<'a> {
    pub dir: &'a Path,
    pub viewing_pubkey: X25519Pubkey,
}

#[must_use]
pub struct SelectedTags {
    store: AnonymousWalletStore,
    tags: TransferTags,
}

#[must_use]
pub struct WalletScan<'a> {
    pub keys: &'a RotatingRingKeys,
    pub assets: &'a RingAssets,
}

#[must_use]
struct ScanAt<'a> {
    sync: RingWalletSync<'a, RotatingRingKeys>,
    store: &'a AnonymousWalletStore,
    /// The ring a recovery scan reads the history of.
    watch: Option<Address>,
}

/// Held while a store's file changes, any part file found under it is a crashed run's.
#[must_use]
struct StoreLock {
    _file: fs::File,
}

impl StoreLocation<'_> {
    pub fn open(self) -> AnonymousWalletStore {
        let viewing_pubkey = self.viewing_pubkey;
        AnonymousWalletStore {
            path: self
                .dir
                .join(format!("{}.json", hex::encode(viewing_pubkey.as_bytes()))),
            viewing_pubkey,
        }
    }
}

impl AnonymousWalletStore {
    /// A wallet that never stored one starts at its current key.
    pub fn load(&self) -> Result<AnonymousWallet> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(AnonymousWallet::new(ViewingHistory::new(
                    self.viewing_pubkey,
                    unix_now()?,
                )))
            }
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {}", self.path.display()))
            }
        };
        let wallet = AnonymousWallet::from_json(&text)
            .with_context(|| format!("cannot parse {}", self.path.display()))?;
        ensure!(
            wallet.history().entry(&self.viewing_pubkey).is_some(),
            "{} holds no entry of the wallet's viewing key",
            self.path.display()
        );
        Ok(wallet)
    }

    /// Applies `change` under the store's lock, its one replace stores everything it changed.
    pub fn update<T>(&self, change: impl FnOnce(&mut AnonymousWallet) -> Result<T>) -> Result<T> {
        let lock = self.lock()?;
        let stored = self.load()?;
        let mut wallet = stored.clone();
        let outcome = change(&mut wallet)?;
        if wallet != stored || !self.path.exists() {
            lock.replace(&self.path, wallet.to_json())?;
        }
        Ok(outcome)
    }

    /// The wallet's sender tag for one transfer, nothing is stored until [`SelectedTags::confirm`].
    pub fn select(self, keys: &RotatingRingKeys) -> Result<SelectedTags> {
        let wallet = self.load()?;
        // A history in recovery takes no counter, refused ahead of the transfer.
        ensure!(
            matches!(wallet.recovery_status(), RecoveryStatus::Complete),
            "the wallet is in a recovery scan, finish it before a send"
        );
        let tags = wallet.history().select(keys)?;
        Ok(SelectedTags { store: self, tags })
    }

    fn lock(&self) -> Result<StoreLock> {
        let path = self.path.with_extension("lock");
        if let Some(dir) = path.parent() {
            file::create_dir_all(dir)?;
        }
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .with_context(|| format!("cannot open {}", path.display()))?;
        lock.lock()
            .with_context(|| format!("cannot lock {}", path.display()))?;
        Ok(StoreLock { _file: lock })
    }
}

impl SelectedTags {
    pub fn tags(&self) -> &TransferTags {
        &self.tags
    }

    /// The stored counter moves only when `published`, the confirmed frame's slot tags, holds the sender tag.
    pub fn confirm(&self, published: &[ViewTag]) -> Result<()> {
        self.store.update(|wallet| {
            let advanced = wallet
                .history()
                .clone()
                .with_published(&self.tags, published);
            Ok(wallet.update_history(advanced)?)
        })
    }
}

impl WalletScan<'_> {
    pub fn fetch(self, ctx: &Context) -> Result<SpendableDecryptionResult> {
        Ok(self.fetch_notes(ctx)?.spendable)
    }

    pub fn fetch_notes(self, ctx: &Context) -> Result<RingWalletNotes> {
        self.scan(ctx, None)
    }

    /// Also opens every body of the ring's history under each held key, it finds a note no listed tag names.
    pub fn recover_notes(self, ctx: &Context) -> Result<RingWalletNotes> {
        self.scan(ctx, Some(ctx.ring.program_id()))
    }

    fn scan(self, ctx: &Context, watch: Option<Address>) -> Result<RingWalletNotes> {
        let store = ctx.anonymous_wallet_store(self.keys.identity_viewing_pubkey());
        let rpc = ctx.rpc();
        let chain_assets = || crate::assets::ring_assets(rpc);
        let sync = RingWalletSync::new(self.keys, self.assets)
            .with_indexer_config(IndexerRpcConfig::at_slot(rpc.get_slot()?))
            .with_chain_assets(&chain_assets);
        ScanAt {
            sync: match watch {
                Some(_) => sync.with_recovery_scan(),
                None => sync,
            },
            store: &store,
            watch,
        }
        .fetch(&ctx.zolana)
    }
}

impl ScanAt<'_> {
    /// Each sync resumes from the stored positions, one replace stores what the reads covered.
    fn fetch<R: BlockingRpc, I: BlockingIndexer>(
        self,
        zolana: &Zolana<R, I>,
    ) -> Result<RingWalletNotes> {
        let scanned = self.sync.scan(zolana.indexer())?;
        let synced = self.store.update(|wallet| {
            if let Some(ring) = self.watch {
                let watched = wallet.history().clone().with_ring(ring, None);
                wallet.update_history(watched)?;
            }
            zolana.block_on(async {
                let indexer = zolana.as_async().indexer();
                Ok(scanned.sync(indexer, wallet).await?)
            })
        })?;
        match synced {
            RingWalletSynced::Notes(notes) => Ok(notes),
            RingWalletSynced::Paused => Err(AnonymousError::SyncIncomplete.into()),
        }
    }
}

impl StoreLock {
    /// Owner-only, a crash leaves the previous contents whole.
    fn replace(&self, path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
        let part = path.with_added_extension("part");
        let _ = fs::remove_file(&part);
        create_new(&part, contents.as_ref(), Visibility::Secret)
            .with_context(|| format!("cannot write {}", part.display()))?;
        fs::rename(&part, path)
            .inspect_err(|_| {
                let _ = fs::remove_file(&part);
            })
            .with_context(|| format!("cannot write {}", path.display()))
    }
}

impl Context {
    pub fn anonymous_wallet_store(&self, viewing_pubkey: X25519Pubkey) -> AnonymousWalletStore {
        StoreLocation {
            dir: &self.path(Path::new(ANONYMOUS_RING_DIR)),
            viewing_pubkey,
        }
        .open()
    }
}

pub(crate) fn unix_now() -> Result<i64> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("the system clock is before the Unix epoch")?
        .as_secs();
    Ok(i64::try_from(seconds)?)
}

#[cfg(test)]
pub(crate) mod fixtures {
    use solana_address::Address;
    use spr_keypair::SigningKey;

    use spr_keypair::ShieldedKeypair;

    use super::*;

    pub(crate) const RING: Address = Address::new_from_array([4; 32]);

    pub(crate) fn keypair(seed: u8) -> ShieldedKeypair {
        ShieldedKeypair::from_keypair(SigningKey::from_ed25519_bytes(&[seed; 32])).unwrap()
    }

    /// A keypair with a store no other test shares.
    pub(crate) struct Member {
        pub(crate) keys: ShieldedKeypair,
        pub(crate) store: AnonymousWalletStore,
        dir: PathBuf,
    }

    impl Member {
        pub(crate) fn new(seed: u8, test: &str) -> Self {
            let keys = keypair(seed);
            let dir =
                std::env::temp_dir().join(format!("ring-wallet-{test}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            let store = StoreLocation {
                dir: &dir,
                viewing_pubkey: keys.viewing_pubkey(),
            }
            .open();
            Self { keys, store, dir }
        }

        pub(crate) fn state_file(&self) -> PathBuf {
            self.store.path.clone()
        }
    }

    impl Drop for Member {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use zolana_ring_client::AnonymousError;

    use super::{
        fixtures::{keypair, Member},
        *,
    };

    #[test]
    fn a_new_store_starts_a_history_at_the_wallets_viewing_key() {
        let member = Member::new(1, "fresh");
        let fresh = member.store.load().unwrap();
        let history = fresh.history();
        assert_eq!(history.entries().len(), 1);
        assert_eq!(
            history.current().viewing_pubkey(),
            member.keys.viewing_pubkey()
        );
        assert_eq!(history.current().tx_count(), 0);
        assert!(!member.state_file().exists(), "a load writes nothing");
    }

    #[test]
    fn a_foreign_or_malformed_store_is_refused() {
        let member = Member::new(1, "foreign");
        let path = member.state_file();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let other = ViewingHistory::new(keypair(2).viewing_pubkey(), 1);
        fs::write(&path, AnonymousWallet::new(other).to_json()).unwrap();
        assert!(member.store.load().is_err(), "another viewing key");
        fs::write(&path, r#"{"ring":"x"}"#).unwrap();
        let refused = member.store.load().unwrap_err();
        assert!(matches!(
            refused.downcast_ref::<AnonymousError>(),
            Some(AnonymousError::WalletDocument(_))
        ));
    }

    #[test]
    fn only_a_frame_that_carried_the_sender_tag_moves_the_stored_counter() {
        let member = Member::new(1, "confirm");
        let keys = RotatingRingKeys::new(&member.keys).unwrap();
        let store = AnonymousWalletStore {
            path: member.state_file(),
            viewing_pubkey: member.keys.viewing_pubkey(),
        };
        let selected = store.select(&keys).unwrap();
        let tx_count = || member.store.load().unwrap().history().current().tx_count();
        selected.confirm(&[]).unwrap();
        selected.confirm(&[[9; 32]]).unwrap();
        assert_eq!(tx_count(), 0, "no tag of the selection");
        selected.confirm(&[selected.tags().sender()]).unwrap();
        assert_eq!(tx_count(), 1);
    }

    #[test]
    fn concurrent_updates_keep_every_counter_step() {
        let member = Member::new(1, "concurrent");
        let (store, keys) = (&member.store, &member.keys);
        std::thread::scope(|scope| {
            for _ in 0..16 {
                scope.spawn(move || {
                    store
                        .update(|wallet| {
                            let minted = wallet.history().request_tag(keys)?.history;
                            wallet.update_history(minted)?;
                            Ok(())
                        })
                        .unwrap()
                });
            }
        });
        let stored = store.load().unwrap();
        assert_eq!(stored.history().current().request_count(), 16);
    }

    #[test]
    fn a_part_left_by_a_crashed_run_does_not_block_the_next_write() {
        let member = Member::new(1, "crashed-part");
        let path = member.state_file();
        let part = path.with_added_extension("part");
        let lock = member.store.lock().unwrap();
        fs::write(&part, b"torn").unwrap();
        lock.replace(&path, b"whole").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"whole");
        assert!(!part.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }
}

#[cfg(test)]
mod scan_tests;
