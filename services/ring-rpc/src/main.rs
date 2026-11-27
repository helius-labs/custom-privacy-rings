use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use log::{info, warn};
use ring_audit::wallet_snapshots::WalletSnapshotLimits;
use ring_cosigner::cosign::CoSignerApproval;
use ring_relay::managed_merge::MANAGED_MERGE_INTERVAL;
use ring_relay::relay_reservations::RelayBudget;
use ring_relay::relay_services::RelayServicesInit;
use ring_relay::role_pins::RolePins;
use ring_service_core::database::{ServiceStore, Store, STORE_CONNECTIONS};
use ring_service_core::keys::KeySource;
use ring_service_core::origins::OriginPolicy;
use ring_service_core::upstream::{ChainSource, TransactionSource, Upstreams};
use ring_writer::enrollment_applier::EnrollmentApplierParts;
use ring_writer::record_writer::RECORD_FLUSH_INTERVAL;
use ring_writer::storage_chain::LiveStorageChain;
use zolana_ring_client::rpc::KeyMode;
use zolana_ring_rpc::{
    run_server, Announce, Cli, Command, Hub, ServeArgs, ServeKeys, ServerOptions, ServiceIdentity,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    match Cli::parse().command {
        Command::Serve(args) => serve(*args).await,
        Command::Provision(args) => print_json(&args.provision().await?),
        Command::SignerRef(args) => print_json(&args.resolve().await?),
    }
}

fn print_json(value: &impl serde::Serialize) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
}

async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let enrollment_ring_budget = args.enrollment_ring_budget();
    let enrollment_max_rings = args.enrollment_max_rings();
    let upstreams = Upstreams {
        indexer_url: &args.indexer_url,
        rpc_url: &args.rpc_url,
        timeout: args.upstream_timeout(),
    };
    let source = ChainSource::connect(upstreams)?;
    let genesis_hash = source
        .genesis_hash()
        .await
        .context("reading the cluster genesis hash")?;
    let ServeKeys {
        auditor,
        service,
        storage_authority,
        co_signer,
        grant_rows,
        fee_payer,
        enrollment_applier,
    } = args.read_keys(&genesis_hash).await?;
    // The role check, the enrollment writer, the record writer and its private writes share it.
    let chain = args
        .prover_url
        .as_deref()
        .map(|prover_url| LiveStorageChain::connect(upstreams, prover_url).map(Arc::new))
        .transpose()?;
    if let (KeySource::Local { ring, .. }, Some(chain)) = (&auditor, &chain) {
        RolePins {
            ring: *ring,
            storage_authority: storage_authority.as_ref().map(|signer| signer.address()),
            enrollment_writer: enrollment_applier.as_ref().map(|signer| signer.address()),
        }
        .check(chain.as_ref())
        .await?;
    }
    let options = ServerOptions {
        bind: args.bind,
        bind_policy: args.bind_policy(),
        port: args.port,
        max_connections: args.max_connections.get(),
        request_timeout: args.request_timeout(),
        client_limits: args.client_limits.client_limits(),
        announce: args
            .photon_registry_url
            .clone()
            .zip(args.public_url.clone())
            .map(|(registry, url)| Announce { registry, url }),
    };
    let store = match &args.store_url {
        Some(url) => {
            let store = Store::open(url, STORE_CONNECTIONS)
                .await
                .context("opening the service store")?;
            ServiceStore::Durable(match &args.replica_id {
                Some(name) => store.with_replica(name),
                None => store,
            })
        }
        None => ServiceStore::internal()
            .await
            .context("opening the internal store")?,
    };
    let relay_services = match (fee_payer, store.durable().cloned(), &args.prover_url) {
        (Some(fee_payer), Some(store), Some(prover_url)) => Some(
            RelayServicesInit {
                fee_payer,
                store,
                upstreams,
                prover_url,
            }
            .build()?,
        ),
        _ => None,
    };
    let origin_transport = args.origin_transport();
    let mut origin_policy = OriginPolicy::new(args.allow_origins).with_transport(origin_transport);
    if let Some(relying_party_id) = args.webauthn_rp_id {
        origin_policy = origin_policy.with_relying_party_id(relying_party_id);
    }
    let identity = ServiceIdentity {
        genesis_hash,
        service,
    };
    let relay_budget = RelayBudget {
        total_lamports: args.relay_budget_lamports,
        signer_lamports: args.relay_signer_budget_lamports,
    };
    let mut builder = Hub::builder(source, identity)
        .with_origins(origin_policy.build()?)
        .with_reader_grants(args.reader_grants)
        .with_snapshot_limits(WalletSnapshotLimits {
            store: args.wallet_snapshots,
            principal_wallet: args.wallet_snapshots_per_principal,
        })
        .with_relay_budget(relay_budget);
    builder = builder.with_store(store.clone());
    if let (Some(chain), Some(payer)) = (&chain, enrollment_applier) {
        let applier = Arc::new(
            EnrollmentApplierParts {
                chain: chain.clone(),
                payer,
                rings: args.enrollment_apply_rings.clone(),
                budget_lamports: args.enrollment_budget_lamports,
                ring_budget_lamports: enrollment_ring_budget,
                max_rings: enrollment_max_rings,
                store: store.store().clone(),
            }
            .applier()
            .await
            .context("loading the registered enrollment rings")?
            .with_vault_programs(args.enrollment_vault_programs.iter().copied()),
        );
        info!("enrollment writer {}", applier.payer());
        if args.enrollment_budget_lamports == 0 {
            warn!("enrollment writes off, set --enrollment-budget-lamports and fund the writer to turn them on");
        }
        tokio::spawn(applier.clone().run(RECORD_FLUSH_INTERVAL));
        builder = builder.with_enrollment_applier(applier);
    }
    if let (Some(chain), Some(authority)) = (&chain, storage_authority) {
        builder = builder.with_storage_chain(chain.clone(), authority);
    }
    if let Some(secret) = grant_rows {
        builder = builder.with_grant_rows_key(secret);
    }
    if let Some(signer) = co_signer {
        builder = builder
            .with_co_signer(signer)
            .with_co_signer_approval(CoSignerApproval::new(
                args.co_signer_approve.iter().copied(),
            ))
            .with_co_signer_exit_caps(args.co_signer_exit_caps.clone());
    }
    if let Some(services) = relay_services {
        builder = builder.with_relay(services);
    }
    let hub = Arc::new(match auditor {
        KeySource::Local { ring, auditor } => builder.local(ring, auditor).await?,
        KeySource::Derived(root) => builder.derived(root).await?,
    });
    info!("service pubkey {}", hub.service_pubkey());
    match hub.relay() {
        None => {
            warn!("relay off, it needs a fee payer and a relay budget")
        }
        Some(relay) => {
            info!("relay fee payer {}", relay.fee_payer());
            if relay_budget.is_set() {
                info!(
                    "relay budget {} lamports, {} per signer",
                    relay_budget.total_lamports, relay_budget.signer_lamports
                );
            } else {
                warn!(
                    "relay sends off, set --relay-budget-lamports and --relay-signer-budget-lamports and fund the fee payer to turn them on"
                );
            }
        }
    }
    if let Some(merge) = hub.managed_merge() {
        tokio::spawn(merge.run(MANAGED_MERGE_INTERVAL));
    }
    if let Some(co_signer) = hub.co_signer().signer() {
        info!("co-signer pubkey {co_signer}");
    }
    match (hub.mode(), hub.local_view_tag()) {
        (KeyMode::Local, Some(tag)) => info!(
            "ring-rpc listening on {}:{} for auditor tag {}",
            args.bind,
            args.port,
            hex::encode(tag)
        ),
        _ => info!(
            "ring-rpc listening on {}:{}, auditor keys derived per ring",
            args.bind, args.port
        ),
    }
    if let (Ok(writer), Some(chain)) = (hub.record_writer(), chain) {
        info!("storage authority {}", writer.authority());
        hub.serve_private_writes(chain)?;
    }
    hub.spawn_workers();
    let handle = run_server(hub, options).await?;
    tokio::signal::ctrl_c().await?;
    info!("stopping");
    handle.stop()?;
    handle.stopped().await;
    Ok(())
}
