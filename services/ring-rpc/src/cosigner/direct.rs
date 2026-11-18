use solana_message::VersionedMessage;
use zolana_ring_client::rpc::{CoSignTransactionRequest, CoSignTransactionResponse};

use super::scope::SignerScope;
use crate::{
    cosign::{CoSignDecision, CoSignError, CoSignFlows, CoSignPolicy, CoSignRequest},
    error::RingRpcError,
    landing::{ChainSubmit, Simulation},
    relay::RelayError,
};

#[must_use]
pub(crate) struct DirectCoSign<'a> {
    pub chain: &'a dyn ChainSubmit,
    pub policy: &'a dyn CoSignPolicy,
}

impl DirectCoSign<'_> {
    pub async fn sign(
        self,
        request: &CoSignTransactionRequest,
    ) -> Result<CoSignTransactionResponse, RingRpcError> {
        let signer = self.policy.signer().ok_or(CoSignError::Disabled)?;
        if request.message.0.len() > solana_message::v1::MAX_TRANSACTION_SIZE {
            return Err(CoSignError::MessageScope.into());
        }
        let message: VersionedMessage =
            wincode::deserialize(&request.message.0).map_err(|_| CoSignError::MessageScope)?;
        let ring = request.ring.0;
        let message = SignerScope { ring, signer }.admit(message)?.into_message();
        // No rule verifies the ring proof, an unproven frame would hold a window reservation.
        if let Simulation::Refused { reason } = self.chain.simulate(&message).await? {
            return Err(RelayError::SendRefused { reason }.into());
        }
        let request = CoSignRequest {
            ring,
            message,
            flows: CoSignFlows::Frame,
        };
        match self.policy.decide(&request).await? {
            CoSignDecision::Signed(signature) => Ok(CoSignTransactionResponse {
                signer: signer.into(),
                signature: signature.into(),
            }),
            CoSignDecision::Refused(refusal) => Err(CoSignError::from(refusal).into()),
        }
    }
}
