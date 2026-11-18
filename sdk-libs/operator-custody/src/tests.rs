use aws_sdk_kms::{
    operation::{get_public_key::GetPublicKeyOutput, sign::SignOutput},
    primitives::Blob,
    types::{KeySpec, KeyUsageType, MessageType, SigningAlgorithmSpec},
    Client,
};
use aws_smithy_mocks::{mock, mock_client, RuleMode};
use ed25519_dalek::{pkcs8::EncodePublicKey, Signer, SigningKey};
use solana_address::Address;

use super::{CustodyError, Kms, SecretContext, SecretPurpose, SignerRef};

const ARN: &str = "arn:aws:kms:us-east-1:111122223333:key/abcfef01-2345-6789-abcd-ef0123456789";

#[tokio::test]
async fn signing_binds_the_pinned_address_and_raw_message() {
    let key = SigningKey::from_bytes(&[71; 32]);
    let public_key = key.verifying_key();
    let encoded = public_key.to_public_key_der().unwrap();
    let lookup = mock!(Client::get_public_key)
        .match_requests(|request| request.key_id() == Some(ARN))
        .then_output(move || {
            GetPublicKeyOutput::builder()
                .key_id(ARN)
                .key_spec(KeySpec::EccNistEdwards25519)
                .key_usage(KeyUsageType::SignVerify)
                .signing_algorithms(SigningAlgorithmSpec::Ed25519Sha512)
                .public_key(Blob::new(encoded.as_bytes()))
                .build()
        });
    let message = b"message".as_slice();
    let other_message = b"another message".as_slice();
    let signature = key.sign(message).to_bytes();
    let sign = mock!(Client::sign)
        .match_requests(move |request| {
            request.key_id() == Some(ARN)
                && request
                    .message()
                    .is_some_and(|blob| [message, other_message].contains(&blob.as_ref()))
                && request.message_type() == Some(&MessageType::Raw)
                && request.signing_algorithm() == Some(&SigningAlgorithmSpec::Ed25519Sha512)
        })
        .then_output(move || {
            SignOutput::builder()
                .key_id(ARN)
                .signing_algorithm(SigningAlgorithmSpec::Ed25519Sha512)
                .signature(Blob::new(signature))
                .build()
        });
    let kms = Kms(mock_client!(
        aws_sdk_kms,
        RuleMode::MatchAny,
        [&lookup, &sign]
    ));
    let mut identity = SignerRef {
        key_arn: ARN.into(),
        address: Address::default(),
    };
    assert!(matches!(
        kms.signer(&identity).await,
        Err(CustodyError::KeyMismatch)
    ));
    identity.address = Address::new_from_array(public_key.to_bytes());
    let signer = kms.signer(&identity).await.unwrap();
    assert_eq!(signer.sign(message).await.unwrap().as_ref(), signature);
    assert!(matches!(
        signer.sign(other_message).await,
        Err(CustodyError::Signature)
    ));
}

#[test]
fn wrapped_secrets_bind_deployment_cluster_role_and_ring() {
    let ring = Address::new_from_array([7; 32]);
    let genesis = [19; 32];
    for (purpose, name, bound_ring) in [
        (
            SecretPurpose::Auditor(ring),
            "auditor",
            Some(ring.to_string()),
        ),
        (SecretPurpose::AuditorRoot, "auditor-root", None),
        (SecretPurpose::GrantRows, "grant-rows", None),
    ] {
        let context = SecretContext {
            deployment: "operator-a",
            genesis_hash: &genesis,
            purpose,
        }
        .fields()
        .unwrap();
        assert_eq!(context["application"], "zolana-custom-rings-v1");
        assert_eq!(context["deployment"], "operator-a");
        assert_eq!(
            context["genesis"],
            Address::new_from_array(genesis).to_string()
        );
        assert_eq!(context["purpose"], name);
        assert_eq!(context.get("ring"), bound_ring.as_ref());
    }
}
