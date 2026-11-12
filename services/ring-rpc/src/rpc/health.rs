use zolana_ring_client::rpc::{HealthResponse, ReaderGrantMode};

use crate::{grants::Grants, hub::Hub, upstream::TransactionSource};

pub(crate) fn health<S: TransactionSource>(hub: &Hub<S>) -> HealthResponse {
    let relay = hub.relay();
    let writer = hub.record_writer().ok();
    let enrollments = hub.enrollment_applier().ok();
    HealthResponse {
        mode: hub.mode(),
        service_pubkey: hub.service_pubkey().into(),
        co_signer_pubkey: hub.co_signer().signer().map(Into::into),
        storage_authority_pubkey: writer.map(|writer| writer.authority().into()),
        fee_payer_pubkey: relay.map(|relay| relay.fee_payer().into()),
        enrollment_writer_pubkey: enrollments.map(|applier| applier.payer().into()),
        relay: relay.is_some(),
        record_writer: writer.is_some(),
        enrollment_writes: enrollments.is_some_and(|applier| applier.writes()),
        reader_grants: match hub.shared().grants {
            Grants::Tree => ReaderGrantMode::Tree,
            Grants::Database(_) => ReaderGrantMode::Database,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use serde_json::json;
    use solana_keypair::Keypair;
    use solana_signer::Signer;

    use super::*;
    use crate::{
        config::{GrantRowsSecret, ReaderGrantStore, RootSecret},
        database::Store,
        enrollment_applier::{EnrollmentApplier, EnrollmentApplierParts},
        hub::{HubBuilder, ServiceIdentity},
        relay_services::RelayServicesInit,
        storage_chain::LiveStorageChain,
        upstream::{ChainSource, Upstreams},
    };

    const UNREACHABLE: &str = "http://127.0.0.1:1";
    const UPSTREAMS: Upstreams<'static> = Upstreams {
        indexer_url: UNREACHABLE,
        rpc_url: UNREACHABLE,
        timeout: Duration::from_secs(1),
    };

    fn key(byte: u8) -> Keypair {
        Keypair::new_from_array([byte; 32])
    }

    fn chain() -> Arc<LiveStorageChain> {
        Arc::new(LiveStorageChain::connect(UPSTREAMS, UNREACHABLE).expect("chain"))
    }

    fn builder() -> HubBuilder<ChainSource> {
        let identity = ServiceIdentity {
            genesis_hash: [1; 32],
            service: key(1),
        };
        Hub::builder(ChainSource::connect(UPSTREAMS).expect("source"), identity)
    }

    async fn enrollment_writer(budget_lamports: u64) -> Arc<EnrollmentApplier> {
        Arc::new(
            EnrollmentApplierParts {
                chain: chain(),
                payer: key(5),
                rings: Vec::new(),
                budget_lamports,
                ring_budget_lamports: budget_lamports,
                max_rings: 1,
                store: None,
            }
            .applier()
            .await
            .expect("applier"),
        )
    }

    fn body(builder: HubBuilder<ChainSource>) -> serde_json::Value {
        let hub = builder
            .derived(RootSecret::from_bytes([7; 32]).expect("root"))
            .expect("hub");
        serde_json::to_value(health(&hub)).expect("health body")
    }

    #[tokio::test]
    async fn health_names_each_configured_role_key_and_no_secret() {
        let store = Store::open("sqlite::memory:", 1).await.expect("store");
        let relay = RelayServicesInit {
            fee_payer: key(4),
            store: store.clone(),
            rpc_url: UNREACHABLE,
            indexer_url: UNREACHABLE,
            prover_url: UNREACHABLE,
        }
        .build();
        let full = builder()
            .with_store(store)
            .with_storage_chain(chain(), key(2))
            .with_co_signer(key(3))
            .with_reader_grants(ReaderGrantStore::Database)
            .with_grant_rows_key(GrantRowsSecret::from_bytes([6; 32]))
            .with_relay_services(relay)
            .with_enrollment_applier(enrollment_writer(1).await);
        assert_eq!(
            body(full),
            json!({
                "mode": "derived",
                "servicePubkey": key(1).pubkey().to_string(),
                "storageAuthorityPubkey": key(2).pubkey().to_string(),
                "coSignerPubkey": key(3).pubkey().to_string(),
                "feePayerPubkey": key(4).pubkey().to_string(),
                "enrollmentWriterPubkey": key(5).pubkey().to_string(),
                "relay": true,
                "recordWriter": true,
                "enrollmentWrites": true,
                "readerGrants": "database",
            })
        );

        let unfunded = body(builder().with_enrollment_applier(enrollment_writer(0).await));
        assert_eq!(
            unfunded["enrollmentWriterPubkey"],
            json!(key(5).pubkey().to_string())
        );
        assert_eq!(unfunded["enrollmentWrites"], json!(false));

        assert_eq!(
            body(builder()),
            json!({
                "mode": "derived",
                "servicePubkey": key(1).pubkey().to_string(),
                "enrollmentWriterPubkey": null,
                "relay": false,
                "recordWriter": false,
                "enrollmentWrites": false,
                "readerGrants": "tree",
            })
        );
    }
}
