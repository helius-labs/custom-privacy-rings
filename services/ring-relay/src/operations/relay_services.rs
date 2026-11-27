use std::sync::Arc;

use zolana_operator_custody::SigningIdentity;

use ring_service_core::{database::Store, landing::SolanaChain, upstream::Upstreams};

use ring_writer::storage_chain::live_ring_client;

use crate::{error::RingRelayError, relay::RelayParts, relay_prover::SdkRelayProver};

/// The relay and the managed merge prove and pay through one client and fee payer.
#[must_use]
pub struct RelayServicesInit<'a> {
    pub fee_payer: SigningIdentity,
    pub store: Store,
    pub upstreams: Upstreams<'a>,
    pub prover_url: &'a str,
}

impl RelayServicesInit<'_> {
    pub fn build(self) -> Result<RelayParts, RingRelayError> {
        Ok(RelayParts {
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
        })
    }
}
