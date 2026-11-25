use std::sync::Arc;

use zolana_operator_custody::SigningIdentity;

use crate::{
    database::Store, error::RingRpcError, escrow_merge::RingSdkMergeBuilder, landing::SolanaChain,
    managed_merge::MergeBuilder, relay::RelayParts, relay_prover::SdkRelayProver,
    storage_chain::live_ring_client, upstream::Upstreams,
};

/// The relay and the managed merge pay through one fee payer.
#[must_use]
pub struct RelayServices {
    pub(crate) parts: RelayParts,
    pub(crate) merge: MergeParts,
}

pub(crate) struct MergeParts {
    pub(crate) builder: Arc<dyn MergeBuilder>,
}

#[must_use]
pub struct RelayServicesInit<'a> {
    pub fee_payer: SigningIdentity,
    pub store: Store,
    pub upstreams: Upstreams<'a>,
    pub prover_url: &'a str,
}

impl RelayServicesInit<'_> {
    pub fn build(self) -> Result<RelayServices, RingRpcError> {
        let builder = Arc::new(RingSdkMergeBuilder {
            client: live_ring_client(self.upstreams, self.prover_url)?,
            payer: self.fee_payer.address(),
        });
        Ok(RelayServices {
            parts: RelayParts {
                store: self.store,
                prover: Arc::new(SdkRelayProver::new(live_ring_client(
                    self.upstreams,
                    self.prover_url,
                )?)),
                fee_payer: self.fee_payer,
                chain: Arc::new(SolanaChain::with_timeout(
                    self.upstreams.rpc_url,
                    self.upstreams.timeout,
                )),
            },
            merge: MergeParts { builder },
        })
    }
}
