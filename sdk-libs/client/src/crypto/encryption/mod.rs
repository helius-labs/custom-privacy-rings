//! Mirrors `prover/circuits/shared/envelope.go`, streams keyed by `Poseidon(CR_S, dh, eph_pk, auditor_pk)`.

use custom_ring_interface::{
    domain::{self, REGISTER_KEY_LABEL},
    REGISTER_KEY_CIPHERTEXT_LEN,
};
use spr_hasher::{
    primitives::{pack_be, right_align},
    Hasher, Poseidon,
};
use spr_keypair::{NullifierKey, X25519Keypair, X25519Pubkey};
use zeroize::Zeroizing;
use zolana_ring_keys::{symmetric_apply_audit, RingStream};

use crate::AuditError;

type Result<T> = core::result::Result<T, AuditError>;

mod merge_record;
pub use merge_record::{
    open_merge_record, MergeRecordSeal, SealedMergeCiphertexts, SealedMergeOpening,
};

/// The auditor key's u-coordinate, the tag the auditor scans for.
pub fn auditor_view_tag(pk: &X25519Pubkey) -> [u8; 32] {
    pk.u()
}

/// Binds the X25519 output to both keys that produced it, in the order `envelope.go` hashes.
pub(crate) fn shared_secret(
    dh: &[u8; 32],
    ephemeral: &X25519Pubkey,
    auditor: &X25519Pubkey,
) -> Result<Zeroizing<[u8; 32]>> {
    let [dh0, dh1] = pack_be(dh);
    let [eph0, eph1] = pack_be(ephemeral.as_bytes());
    let [aud0, aud1] = pack_be(auditor.as_bytes());
    Ok(Zeroizing::new(Poseidon::hashv(&[
        &domain::field(domain::AUDIT_SHARED_SECRET),
        &dh0,
        &dh1,
        &eph0,
        &eph1,
        &aud0,
        &aud1,
    ])?))
}

/// A resend repeats the transfer's key, the salt keeps its keystream apart.
pub(crate) fn salted_stream(
    secret: &[u8; 32],
    salt: &[u8; 16],
    label: &[u8; 9],
) -> Result<RingStream> {
    let salted = Zeroizing::new(Poseidon::hashv(&[secret, &right_align(salt)])?);
    Ok(RingStream::new(&salted, label)?)
}

pub(crate) fn sealing_secret(
    ephemeral: &X25519Keypair,
    auditor: &X25519Pubkey,
) -> Result<Zeroizing<[u8; 32]>> {
    shared_secret(&*ephemeral.ecdh(auditor)?, &ephemeral.pubkey(), auditor)
}

pub(crate) fn opening_secret(
    auditor: &X25519Keypair,
    ephemeral: &X25519Pubkey,
) -> Result<Zeroizing<[u8; 32]>> {
    shared_secret(&*auditor.ecdh(ephemeral)?, ephemeral, &auditor.pubkey())
}

/// A nullifier key and list salt sealed to the ring auditor, plaintext bytes 0 and 32 are the pads the circuit pins.
pub struct NullifierKeyEnvelope {
    pub ephemeral_sk: Zeroizing<[u8; 32]>,
    pub sealed: SealedNullifierKey,
    pub nullifier_pk: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SealedNullifierKey {
    pub eph_pk: X25519Pubkey,
    pub ciphertext: [u8; REGISTER_KEY_CIPHERTEXT_LEN],
}

impl NullifierKeyEnvelope {
    pub fn new(
        nullifier_key: &NullifierKey,
        list_salt: &[u8; 32],
        auditor_pk: &X25519Pubkey,
    ) -> Result<Self> {
        Self::seal(nullifier_key, list_salt, auditor_pk, X25519Keypair::new())
    }

    fn seal(
        nullifier_key: &NullifierKey,
        list_salt: &[u8; 32],
        auditor_pk: &X25519Pubkey,
        ephemeral: X25519Keypair,
    ) -> Result<Self> {
        let mut ciphertext = Zeroizing::new([0u8; REGISTER_KEY_CIPHERTEXT_LEN]);
        let (secret, salt) = ciphertext.split_at_mut(32);
        secret.copy_from_slice(&*Zeroizing::new(right_align(&nullifier_key.secret())));
        salt.copy_from_slice(list_salt);
        symmetric_apply_audit(
            &*sealing_secret(&ephemeral, auditor_pk)?,
            REGISTER_KEY_LABEL,
            &mut *ciphertext,
        )?;
        Ok(Self {
            ephemeral_sk: ephemeral.secret_bytes(),
            sealed: SealedNullifierKey {
                eph_pk: ephemeral.pubkey(),
                ciphertext: *ciphertext,
            },
            nullifier_pk: nullifier_key.pubkey()?,
        })
    }
}

impl SealedNullifierKey {
    /// The pad alone does not authenticate the key against its registry record.
    pub fn open(&self, auditor: &X25519Keypair) -> Result<NullifierKey> {
        let plaintext = self.plaintext(auditor)?;
        let secret: &[u8; 31] = plaintext[1..32]
            .try_into()
            .map_err(|_| AuditError::NullifierPad)?;
        Ok(NullifierKey::from_secret(*secret))
    }

    pub fn open_list_salt(&self, auditor: &X25519Keypair) -> Result<[u8; 32]> {
        let plaintext = self.plaintext(auditor)?;
        plaintext[32..]
            .try_into()
            .map_err(|_| AuditError::NullifierPad)
    }

    fn plaintext(
        &self,
        auditor: &X25519Keypair,
    ) -> Result<Zeroizing<[u8; REGISTER_KEY_CIPHERTEXT_LEN]>> {
        let mut plaintext = Zeroizing::new(self.ciphertext);
        symmetric_apply_audit(
            &*opening_secret(auditor, &self.eph_pk)?,
            REGISTER_KEY_LABEL,
            &mut *plaintext,
        )?;
        if plaintext[0] != 0 || plaintext[32] != 0 {
            return Err(AuditError::NullifierPad);
        }
        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes<const N: usize>(value: &str) -> [u8; N] {
        let decoded = hex::decode(value).expect("hex");
        decoded.try_into().expect("byte length")
    }

    #[test]
    fn shared_secret_matches_the_circuit_vector() {
        let diffie_hellman_x =
            bytes("464b33b122dc1346a6012199742d7e14b2ee6436589c454a579fdda24feec92c");
        let ephemeral_key = X25519Pubkey::from_bytes(bytes(
            "048ea960e4e1b72cdec9a6e7d86ae28cd04edb4b5c07266d3c32d9133c8ebaa966",
        ))
        .expect("ephemeral key");
        let auditor_key = X25519Pubkey::from_bytes(bytes(
            "04708c2645c705baaf44f2ab947574174f9280df58be6fc2f6f2e8881b78a3115a",
        ))
        .expect("auditor key");
        let secret =
            shared_secret(&diffie_hellman_x, &ephemeral_key, &auditor_key).expect("shared secret");
        assert_eq!(
            *secret,
            bytes("2590b2b949a2f87b9201e4d065167ea9282c4bd06a32e888e24e33c5bddae54f")
        );
    }

    #[test]
    fn shared_secret_binds_both_keys_and_the_ecdh_value() {
        let auditor_key = X25519Keypair::new().pubkey();
        let other_auditor_key = X25519Keypair::new().pubkey();
        let ephemeral_key = X25519Keypair::new().pubkey();
        let other_ephemeral_key = X25519Keypair::new().pubkey();
        let derive = |dh, eph, auditor| shared_secret(dh, eph, auditor).expect("shared secret");
        let secret = derive(&[7u8; 32], &ephemeral_key, &auditor_key);
        assert_ne!(secret, derive(&[8u8; 32], &ephemeral_key, &auditor_key));
        assert_ne!(
            secret,
            derive(&[7u8; 32], &other_ephemeral_key, &auditor_key)
        );
        assert_ne!(
            secret,
            derive(&[7u8; 32], &ephemeral_key, &other_auditor_key)
        );
    }

    const LIST_SALT: [u8; 32] = {
        let mut salt = [0x09; 32];
        salt[0] = 0;
        salt
    };

    #[test]
    fn nullifier_key_envelope_round_trips_through_the_auditor() {
        let auditor = X25519Keypair::new();
        let nullifier_key = NullifierKey::from_secret([7u8; 31]);
        let envelope = NullifierKeyEnvelope::new(&nullifier_key, &LIST_SALT, &auditor.pubkey())
            .expect("encrypt");
        let recovered = envelope.sealed.open(&auditor).expect("decrypt");
        assert_eq!(
            envelope.sealed.open_list_salt(&auditor).expect("salt"),
            LIST_SALT
        );
        assert_eq!(
            recovered.secret().as_slice(),
            nullifier_key.secret().as_slice()
        );
        assert_eq!(
            envelope.nullifier_pk,
            nullifier_key.pubkey().expect("pubkey")
        );
    }

    #[test]
    fn a_stranger_auditor_cannot_recover_the_nullifier_key() {
        let auditor = X25519Keypair::new();
        let stranger = X25519Keypair::new();
        let nullifier_key = NullifierKey::from_secret([9u8; 31]);
        let envelope = NullifierKeyEnvelope::new(&nullifier_key, &LIST_SALT, &auditor.pubkey())
            .expect("encrypt");
        match envelope.sealed.open(&stranger) {
            Err(AuditError::NullifierPad) => {}
            Ok(recovered) => {
                assert_ne!(
                    recovered.secret().as_slice(),
                    nullifier_key.secret().as_slice()
                )
            }
            Err(other) => panic!("unexpected error {other}"),
        }
    }

    /// The `disclosure_vectors.rs` auditor scalar, the TypeScript mirror pins the same values.
    const AUDITOR_SK: &str = "01323130373635343b3a39383f3e3d3c23222120272625242b2a29282f2e2d2c";
    const EPHEMERAL_SK: &str = "011013121514171619181b1a1d1c1f1e010003020504070609080b0a0d0c0f0e";
    const NULLIFIER_SECRET: &str = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
    const NULLIFIER_PK: &str = "2d1faf6cf358763421511eb637adf7b6609443d38edc4ed2b042dfbf834b03f5";
    const EPH_PK: &str = "043632fe89228ea5215eea45a56704f569e3929868a8573363d015e39609f0eb63";
    const CIPHERTEXT: &str = "4be6ab2544799bd0c018ff495f59e8205d6fe9d007f74e44ec739529c0b27f25d23f6f5b30bde013afd5a02f1913cb4c354c8529e12a01ad583bb6f72692d99c";

    #[test]
    fn the_sealed_key_matches_the_pinned_vector() {
        let auditor = X25519Keypair::from_bytes(&bytes(AUDITOR_SK));
        let nullifier_key = NullifierKey::from_secret(bytes(NULLIFIER_SECRET));
        let ephemeral = X25519Keypair::from_bytes(&bytes(EPHEMERAL_SK));
        let envelope =
            NullifierKeyEnvelope::seal(&nullifier_key, &LIST_SALT, &auditor.pubkey(), ephemeral)
                .expect("seal");
        assert_eq!(envelope.nullifier_pk, bytes(NULLIFIER_PK));
        assert_eq!(envelope.sealed.eph_pk.as_bytes(), &bytes::<33>(EPH_PK));
        assert_eq!(envelope.sealed.ciphertext, bytes(CIPHERTEXT));
        assert_eq!(
            envelope
                .sealed
                .open(&auditor)
                .expect("open")
                .secret()
                .as_slice(),
            &bytes::<31>(NULLIFIER_SECRET)
        );
    }
}
