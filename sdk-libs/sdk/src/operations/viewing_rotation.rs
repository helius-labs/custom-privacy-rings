//! Hands the ring's enrollment writer a rotated viewing key for the owner's leaf, the key senders read once its batch lands.
//!
//! Read after `zolana_ring_client::ViewingRotation`, whose stored keys and history come first.

use solana_address::Address;
use spr_client::AsyncRpc;
use spr_transaction::ShieldedKeys;
use zolana_ring_client::ViewingHistory;
use zolana_ring_keys::RotatingRingKeys;

use crate::client::{RingClient, RingIndexerBackend};
use crate::{
    enrollment::{EnrollmentRead, OwnerEnrollment},
    CustomRing, RingError,
};

#[cfg(test)]
#[path = "../tests/operations/viewing_rotation.rs"]
mod tests;

#[must_use]
pub struct RotateViewingKey<'a> {
    pub ring: CustomRing,
    pub owner: Address,
    pub keys: &'a RotatingRingKeys,
    pub history: &'a ViewingHistory,
}

impl RotateViewingKey<'_> {
    /// The rotation the owner signs for the writer, `None` when the leaf already holds the key.
    pub async fn prepare<R: AsyncRpc, I: RingIndexerBackend>(
        self,
        client: &RingClient<R, I>,
    ) -> Result<Option<OwnerEnrollment>, RingError> {
        // 1. No sync reads a key the history lacks, publishing one strands every later payment.
        let address = self.keys.address()?;
        if self.history.current().viewing_pubkey() != address.viewing_pubkey {
            return Err(RingError::ViewingHistoryNotCurrent);
        }
        // 2. A leaf key the wallet never held is another copy's, publishing over it strands its payments.
        let owner = self.owner;
        let tracking_registry = self
            .ring
            .require_config(client.zolana())
            .await?
            .account()
            .tracking;
        let enrolled = EnrollmentRead {
            tracking_registry,
            owner,
        }
        .read(client.zolana())
        .await?
        .ok_or(RingError::RotationUnenrolled { owner })?;
        if enrolled.ix.nullifier_pk != address.nullifier_pubkey {
            return Err(RingError::AlreadyEnrolled { owner });
        }
        if !self
            .keys
            .viewing_public_keys()
            .iter()
            .any(|key| key.u() == enrolled.ix.viewing_u)
        {
            return Err(RingError::ViewingKeyNotHeld { owner });
        }
        let viewing_u = address.viewing_pubkey.u();
        Ok(
            (enrolled.ix.viewing_u != viewing_u).then(|| OwnerEnrollment {
                ring: self.ring,
                tracking_registry,
                owner,
                change: enrolled.rotation(viewing_u),
            }),
        )
    }
}
