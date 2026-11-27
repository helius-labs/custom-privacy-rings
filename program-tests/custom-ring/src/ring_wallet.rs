//! A ring wallet kept in memory, synced by exact tag as the cli syncs its stored one.

use anyhow::Result;
use spr_client::{IndexerRpcConfig, Rpc};
use spr_keypair::ShieldedKeypair;
use spr_transaction::{SpendableDecryptionResult, WalletUtxo};
use zolana_ring_client::{
    AnonymousError, AnonymousWallet, RingWalletSync, RingWalletSynced, TransferTags, ViewingHistory,
};
use zolana_ring_transaction::RingAssets;

use crate::shared::TestEnv;

pub struct RingWallet<'a> {
    keys: &'a ShieldedKeypair,
    assets: RingAssets,
    wallet: AnonymousWallet,
    spendable: SpendableDecryptionResult,
}

impl<'a> RingWallet<'a> {
    /// A wallet that never paid or was paid, its history from the keypair's viewing key.
    pub fn new(keys: &'a ShieldedKeypair, assets: RingAssets) -> Self {
        Self {
            keys,
            assets,
            wallet: AnonymousWallet::new(ViewingHistory::new(keys.viewing_pubkey(), 0)),
            spendable: SpendableDecryptionResult::default(),
        }
    }

    pub fn history(&self) -> &ViewingHistory {
        self.wallet.history()
    }

    pub fn sync(&mut self, env: &TestEnv) -> Result<()> {
        let config = IndexerRpcConfig::at_slot(env.client.rpc().get_slot()?);
        let scanned = RingWalletSync::new(self.keys, &self.assets)
            .with_indexer_config(config)
            .scan(env.client.indexer())?;
        let indexer = env.client.as_async().indexer();
        let origin = zolana_ring_client::FinalizedRpcOrigin::new(
            env.client.rpc().client().get_inner_client(),
        );
        let sources = zolana_ring_client::RingWalletSources {
            indexer,
            origin: &origin,
        };
        let synced = env
            .client
            .block_on(async { anyhow::Ok(scanned.sync(sources, &mut self.wallet).await?) })?;
        match synced {
            RingWalletSynced::Notes(notes) => {
                self.spendable = notes.spendable;
                Ok(())
            }
            RingWalletSynced::Paused => Err(AnonymousError::SyncIncomplete.into()),
        }
    }

    pub fn unspent(&self) -> impl Iterator<Item = &WalletUtxo> {
        self.spendable.utxos()
    }

    pub fn is_spent(&self, utxo: &WalletUtxo) -> bool {
        !self.unspent().any(|held| held.utxo_hash == utxo.utxo_hash)
    }
}

pub fn fresh_wallet_tags(sender: &ShieldedKeypair) -> Result<TransferTags> {
    Ok(ViewingHistory::new(sender.viewing_pubkey(), 0)
        .select(sender)?
        .tag)
}
