use std::collections::HashMap;

use aws_sdk_kms::{primitives::Blob, types::DataKeySpec};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use solana_address::Address;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::{check_key_arn, CustodyError, Kms};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WrappedSecret {
    pub key_arn: String,
    pub ciphertext: String,
}

pub enum SecretPurpose {
    Auditor(Address),
    AuditorRoot,
    GrantRows,
}

pub struct SecretContext<'a> {
    pub deployment: &'a str,
    pub genesis_hash: &'a [u8; 32],
    pub purpose: SecretPurpose,
}

impl SecretContext<'_> {
    pub(super) fn fields(self) -> Result<HashMap<String, String>, CustodyError> {
        if self.deployment.trim().is_empty() {
            return Err(CustodyError::Deployment);
        }
        let mut fields = HashMap::from([
            ("application".into(), "zolana-custom-rings-v1".into()),
            ("deployment".into(), self.deployment.into()),
            (
                "genesis".into(),
                Address::new_from_array(*self.genesis_hash).to_string(),
            ),
        ]);
        let purpose = match self.purpose {
            SecretPurpose::Auditor(ring) => {
                fields.insert("ring".into(), ring.to_string());
                "auditor"
            }
            SecretPurpose::AuditorRoot => "auditor-root",
            SecretPurpose::GrantRows => "grant-rows",
        };
        fields.insert("purpose".into(), purpose.into());
        Ok(fields)
    }
}

impl Kms {
    pub async fn provision(
        &self,
        key_arn: &str,
        context: SecretContext<'_>,
    ) -> Result<WrappedSecret, CustodyError> {
        check_key_arn(key_arn)?;
        let reply = self
            .0
            .generate_data_key_without_plaintext()
            .key_id(key_arn)
            .key_spec(DataKeySpec::Aes256)
            .set_encryption_context(Some(context.fields()?))
            .send()
            .await
            .map_err(CustodyError::service)?;
        if reply.key_id() != Some(key_arn) {
            return Err(CustodyError::KeyMismatch);
        }
        let blob = reply
            .ciphertext_blob()
            .filter(|blob| !blob.as_ref().is_empty())
            .ok_or(CustodyError::Secret)?;
        Ok(WrappedSecret {
            key_arn: key_arn.into(),
            ciphertext: STANDARD.encode(blob.as_ref()),
        })
    }

    pub async fn open(
        &self,
        secret: &WrappedSecret,
        context: SecretContext<'_>,
    ) -> Result<Zeroizing<[u8; 32]>, CustodyError> {
        check_key_arn(&secret.key_arn)?;
        let ciphertext = STANDARD
            .decode(&secret.ciphertext)
            .map_err(|_| CustodyError::Secret)?;
        let reply = self
            .0
            .decrypt()
            .key_id(&secret.key_arn)
            .ciphertext_blob(Blob::new(ciphertext))
            .set_encryption_context(Some(context.fields()?))
            .send()
            .await
            .map_err(CustodyError::service)?;
        let correct_key = reply.key_id() == Some(secret.key_arn.as_str());
        let plaintext = Zeroizing::new(reply.plaintext.ok_or(CustodyError::Secret)?.into_inner());
        if !correct_key || plaintext.len() != 32 || bool::from(plaintext.as_slice().ct_eq(&[0; 32]))
        {
            return Err(CustodyError::Secret);
        }
        let mut bytes = Zeroizing::new([0; 32]);
        bytes.copy_from_slice(&plaintext);
        Ok(bytes)
    }
}
