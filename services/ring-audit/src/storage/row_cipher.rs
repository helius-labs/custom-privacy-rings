use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
    Aes256Gcm, Error, Nonce,
};

const NONCE_LEN: usize = 12;

/// A stored row is its random nonce followed by the AES-256-GCM ciphertext.
pub(crate) struct RowCipher(Aes256Gcm);

impl RowCipher {
    pub(crate) fn new(key: &[u8; 32]) -> Self {
        Self(Aes256Gcm::new(key.into()))
    }

    pub(crate) fn seal(&self, aad: &[u8], body: &[u8]) -> Result<Vec<u8>, Error> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let sealed = self.0.encrypt(&nonce, Payload { msg: body, aad })?;
        Ok([nonce.as_slice(), &sealed].concat())
    }

    pub(crate) fn open(&self, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, Error> {
        let (nonce, ciphertext) = sealed.split_at_checked(NONCE_LEN).ok_or(Error)?;
        self.0.decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
    }
}
