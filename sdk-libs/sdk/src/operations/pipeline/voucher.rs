//! The voucher of one transfer, mirror `prover/circuits/policy/voucher.go`.

use spr_hasher::primitives::solana_owner_identity;
use spr_keypair::{PublicKey, SigningKey};
use zolana_ring_client::VouchMessage;

use crate::{request::VoucherParams, RingError};

/// Signs one transfer's vouch digest after the transfer is fixed, the key stays with its wallet.
pub trait VouchSigner: Sync {
    fn public_key(&self) -> PublicKey;

    /// Ed25519 over the 32 digest bytes with no header.
    fn sign_vouch(&self, digest: &[u8; 32]) -> Result<[u8; 64], RingError>;
}

impl VouchSigner for SigningKey {
    fn public_key(&self) -> PublicKey {
        self.pubkey()
    }

    fn sign_vouch(&self, digest: &[u8; 32]) -> Result<[u8; 64], RingError> {
        Ok(self.sign_message(digest)?)
    }
}

/// The Ed25519 key the auditor stream seals.
#[derive(Clone, Copy)]
pub(super) struct VoucherKey(pub [u8; 32]);

/// A signer under the key it named once, the member screened, the key sealed and the key proven are that key.
#[derive(Clone, Copy)]
pub(super) struct Voucher<'a> {
    signer: &'a dyn VouchSigner,
    public_key: PublicKey,
    pub key: VoucherKey,
}

impl<'a> Voucher<'a> {
    pub fn of(signer: &'a dyn VouchSigner) -> Result<Self, RingError> {
        let public_key = signer.public_key();
        Ok(Self {
            signer,
            public_key,
            key: VoucherKey(public_key.as_ed25519()?),
        })
    }

    /// The member the voucher's records sit under.
    pub fn identity(&self) -> Result<[u8; 32], RingError> {
        Ok(solana_owner_identity(&self.key.0)?)
    }

    fn sign(&self, digest: &[u8; 32]) -> Result<VoucherParams, RingError> {
        let signature = self.signer.sign_vouch(digest)?;
        if !self.public_key.verify_message(digest, &signature) {
            return Err(RingError::VoucherSignatureMismatch);
        }
        Ok(VoucherParams {
            public_key: self.key.0,
            signature,
        })
    }
}

pub(super) struct Vouch<'a> {
    pub voucher: Option<Voucher<'a>>,
    pub message: VouchMessage<'a>,
}

impl Vouch<'_> {
    /// The circuit verifies a signature on every proof, a fresh key signs where nobody vouches.
    pub fn sign(self) -> Result<VoucherParams, RingError> {
        let digest = self.message.digest()?;
        match self.voucher {
            Some(voucher) => voucher.sign(&digest),
            None => Voucher::of(&SigningKey::new_ed25519())?.sign(&digest),
        }
    }
}
