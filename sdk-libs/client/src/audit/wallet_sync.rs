//! One sync of a ring-aware wallet, its core notes and the ring notes its tags find.

use std::collections::{BTreeSet, HashSet};

use solana_address::Address;
use spr_client::{
    Blocking, BlockingRpc, ClientError, IndexerRpcConfig, Rpc, UtxoScan, UtxoScanResult,
};
use spr_transaction::{
    OwnedUtxos, ShieldedKeys, ShieldedTransaction, SpendableDecryptionResult, WalletUtxo,
};
use zolana_ring_keys::RingKeys;
use zolana_ring_transaction::RingAssets;

use crate::{
    anonymous::{AnonymousSync, RecoveryStatus},
    anonymous_wallet::{AnonymousWallet, PendingNote},
    confidential::RingConfidentialScan,
    exposed_notes::verify_ring_owned,
    AnonymousError,
};

/// Zero-key notes stay apart, no automatic path selects them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RingWalletNotes {
    pub spendable: SpendableDecryptionResult,
    pub exposed_if_spent: Vec<WalletUtxo>,
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RingWalletSynced {
    Notes(RingWalletNotes),
    /// The wallet keeps its progress for the next sync.
    Paused,
}

/// Each sync resumes from the wallet's stored positions.
#[must_use]
pub struct RingWalletSync<'a, K: ?Sized> {
    keys: &'a K,
    assets: &'a RingAssets,
    config: IndexerRpcConfig,
    chain_assets: Option<&'a ChainAssets<'a>>,
    recovery: bool,
}

#[derive(Clone, Copy)]
enum Rail<'a> {
    /// The wallet's listed tags and the spends of its notes.
    Tags(&'a [ShieldedTransaction]),
    /// Every body of each watched ring's history, the recovery scan.
    History,
}

type ChainAssets<'a> = dyn Fn() -> Result<RingAssets, ClientError> + Sync + 'a;

struct Syncing<'s, 'a, K: ?Sized, I> {
    sync: &'s RingWalletSync<'a, K>,
    indexer: &'s Blocking<I>,
    wallet: &'s mut AnonymousWallet,
}

impl<'a, K: ShieldedKeys + RingKeys + ?Sized> RingWalletSync<'a, K> {
    pub fn new(keys: &'a K, assets: &'a RingAssets) -> Self {
        Self {
            keys,
            assets,
            config: IndexerRpcConfig::default(),
            chain_assets: None,
            recovery: false,
        }
    }

    /// Opens every body of each watched ring's history ahead of the tag read, a note the tag read misses is found here alone.
    pub fn with_recovery_scan(mut self) -> Self {
        self.recovery = true;
        self
    }

    pub fn with_indexer_config(mut self, config: IndexerRpcConfig) -> Self {
        self.config = config;
        self
    }

    /// Read only when a found note's mint is missing from the given assets.
    pub fn with_chain_assets(mut self, read: &'a ChainAssets<'a>) -> Self {
        self.chain_assets = Some(read);
        self
    }

    /// Blocking, for callers outside async code.
    pub fn scan<I: Rpc + ?Sized>(
        self,
        indexer: &I,
    ) -> Result<RingWalletScanned<'a, K>, ClientError> {
        let core = UtxoScan::new(self.keys, self.assets.registry())
            .with_indexer_config(self.config)
            .fetch(indexer)?;
        Ok(RingWalletScanned { sync: self, core })
    }
}

#[must_use]
pub struct RingWalletScanned<'a, K: ?Sized> {
    sync: RingWalletSync<'a, K>,
    core: UtxoScanResult,
}

impl<K: ShieldedKeys + RingKeys + ?Sized> RingWalletScanned<'_, K> {
    pub async fn sync<I: BlockingRpc>(
        self,
        indexer: &Blocking<I>,
        wallet: &mut AnonymousWallet,
    ) -> Result<RingWalletSynced, AnonymousError> {
        let Self { sync, core } = self;
        let mut syncing = Syncing {
            sync: &sync,
            indexer,
            wallet,
        };
        let mut unknown_mints = BTreeSet::new();
        // A paused recovery continues here, no other read moves while it is open.
        if sync.recovery || syncing.wallet.recovery_status() == RecoveryStatus::Paused {
            unknown_mints = syncing.synced(Rail::History).await?;
        }
        if syncing.wallet.recovery_status() == RecoveryStatus::Complete {
            let tags = Rail::Tags(&core.history.transactions);
            unknown_mints.extend(syncing.synced(tags).await?);
        }
        if wallet.recovery_status() == RecoveryStatus::Paused {
            return Ok(RingWalletSynced::Paused);
        }
        unknown_mints.extend(wallet.pending_notes().iter().map(PendingNote::mint));
        let held = wallet.decrypted();
        let verified = verify_ring_owned(sync.keys, &held)?;
        let found = verified
            .owned
            .spendable()
            .with_transaction_keys(sync.keys)?;
        let mut seen = HashSet::new();
        let utxos = core
            .spendable
            .utxos()
            .chain(found.utxos())
            .filter(|utxo| seen.insert(utxo.utxo_hash))
            .cloned()
            .collect();
        let mut spendable = OwnedUtxos {
            utxos,
            spent: held.spent_nullifiers,
            unknown_asset_ids: core.spendable.unknown_asset_ids,
            unknown_mints: core.spendable.unknown_mints,
        }
        .spendable();
        spendable.unknown_mints.extend(unknown_mints);
        Ok(RingWalletSynced::Notes(RingWalletNotes {
            spendable,
            exposed_if_spent: verified.exposed_if_spent,
        }))
    }
}

impl<K: ShieldedKeys + RingKeys + ?Sized, I: BlockingRpc> Syncing<'_, '_, K, I> {
    /// A mint registered after the registry read opens its notes under the chain's assets.
    async fn synced(&mut self, rail: Rail<'_>) -> Result<BTreeSet<Address>, AnonymousError> {
        let unknown_mints = self.absorb(rail, self.sync.assets).await?;
        let Some(chain_assets) = self.sync.chain_assets else {
            return Ok(unknown_mints);
        };
        if unknown_mints.is_empty() && self.wallet.pending_notes().is_empty() {
            return Ok(unknown_mints);
        }
        let chain = chain_assets()?;
        self.wallet.resolve_pending(&chain);
        // A merge output under an unregistered mint opens only on a read of its spend.
        if unknown_mints.is_empty() {
            return Ok(unknown_mints);
        }
        self.absorb(rail, &chain).await
    }

    async fn absorb(
        &mut self,
        rail: Rail<'_>,
        assets: &RingAssets,
    ) -> Result<BTreeSet<Address>, AnonymousError> {
        let keys = self.sync.keys;
        let unknown_mints = match rail {
            Rail::Tags(transactions) => {
                let scan = RingConfidentialScan::new(keys, assets)
                    .with_indexer_config(self.sync.config)
                    .sync(self.indexer, self.wallet, transactions)
                    .await?;
                let unknown_mints = scan.decrypted().unknown_mints.clone();
                self.wallet.absorb(scan)?;
                unknown_mints
            }
            Rail::History => {
                let scan = AnonymousSync::new(keys, assets)
                    .with_indexer_config(self.sync.config)
                    .fetch_wallet(self.indexer, self.wallet)
                    .await?;
                let unknown_mints = scan.decrypted().unknown_mints.clone();
                self.wallet.absorb(scan)?;
                unknown_mints
            }
        };
        Ok(unknown_mints)
    }
}
