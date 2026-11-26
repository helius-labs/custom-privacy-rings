use std::{
    net::SocketAddr,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use anyhow::Result;
use futures::future::{ready, Either, Ready};
use hyper::{header::CONTENT_TYPE, Method, StatusCode};
use jsonrpsee::{
    core::TEN_MB_SIZE_BYTES,
    server::{HttpBody, HttpRequest, HttpResponse, ServerBuilder, ServerConfig, ServerHandle},
    types::ErrorObjectOwned,
    RpcModule,
};
use sea_orm::DatabaseConnection;
use spr_indexer_api::RpcMethod;
use spr_rpc_http::{
    forward::{ForwardLayer, ForwardLimits, Resolve, UpstreamUrl},
    MethodPathLayer,
};
use tower::Service;
use tower_http::cors::{Any, CorsLayer};
use zolana_ring_indexer::{
    rpc::method::{
        GetStorageApplyProofInputs, GetStorageMemberOpenings, GetStorageMemberRecords,
        GetStorageRecordPool,
    },
    tracking::method::{
        GetTrackingApplyProofInputs, GetTrackingOpenings, GetTrackingRecordPool, GetTrackingRecords,
    },
};

use crate::{
    api::{self, CachedChain},
    indexer::Readiness,
    rpc::SolanaRpc,
};

/// Served here, every other call goes to Photon.
pub const RING_METHODS: [&str; 8] = [
    GetStorageMemberOpenings::NAME,
    GetStorageMemberRecords::NAME,
    GetStorageRecordPool::NAME,
    GetStorageApplyProofInputs::NAME,
    GetTrackingOpenings::NAME,
    GetTrackingRecords::NAME,
    GetTrackingRecordPool::NAME,
    GetTrackingApplyProofInputs::NAME,
];

const MAX_BODY_BYTES: u32 = TEN_MB_SIZE_BYTES;
const PHOTON_TIMEOUT: Duration = Duration::from_secs(90);

struct Photon(UpstreamUrl);

impl Resolve for Photon {
    fn upstream(&self, method: Option<&str>) -> Option<UpstreamUrl> {
        (!method.is_some_and(|method| RING_METHODS.contains(&method))).then(|| self.0.clone())
    }
}

pub(crate) struct Server {
    pub db: Arc<DatabaseConnection>,
    pub rpc: Arc<SolanaRpc>,
    pub photon: UpstreamUrl,
    pub readiness: Readiness,
    pub max_connections: u32,
}

impl Server {
    pub async fn start(self, address: SocketAddr) -> Result<(SocketAddr, ServerHandle)> {
        let Self {
            db,
            rpc,
            photon,
            readiness,
            max_connections,
        } = self;
        let cors = CorsLayer::new()
            .allow_methods([Method::POST, Method::GET])
            .allow_origin(Any)
            .allow_headers([CONTENT_TYPE]);
        // Below the forwarder, forwarded calls keep Photon's own path check.
        let middleware = tower::ServiceBuilder::new()
            .layer(cors)
            .layer(tower::layer::layer_fn(move |inner| Probes {
                inner,
                readiness: readiness.clone(),
            }))
            .layer(ForwardLayer::new(
                Photon(photon),
                ForwardLimits {
                    request_bytes: MAX_BODY_BYTES as usize,
                    timeout: PHOTON_TIMEOUT,
                    concurrent: max_connections as usize,
                },
            )?)
            .layer(MethodPathLayer {
                max_body_size: MAX_BODY_BYTES as usize,
            });
        let server = ServerBuilder::default()
            .set_config(
                ServerConfig::builder()
                    .http_only()
                    .max_connections(max_connections)
                    .max_request_body_size(MAX_BODY_BYTES)
                    .build(),
            )
            .set_http_middleware(middleware)
            .build(address)
            .await?;
        Ok((
            server.local_addr()?,
            server.start(ring_methods(Lookups {
                db,
                chain: CachedChain::new(rpc),
            })?),
        ))
    }
}

struct Lookups {
    db: Arc<DatabaseConnection>,
    chain: CachedChain,
}

macro_rules! register {
    ($module:ident, $($method:ty => $read:path,)+) => {
        $($module.register_async_method(<$method>::NAME, |params, lookups, _| async move {
            $read(&lookups.db, &lookups.chain, params.parse()?)
                .await
                .map_err(ErrorObjectOwned::from)
        })?;)+
    };
}

fn ring_methods(lookups: Lookups) -> Result<RpcModule<Lookups>> {
    let mut module = RpcModule::new(lookups);
    register!(
        module,
        GetStorageMemberOpenings => api::storage_member_openings,
        GetStorageMemberRecords => api::storage_member_records,
        GetStorageRecordPool => api::storage_record_pool,
        GetStorageApplyProofInputs => api::storage_apply_proof_inputs,
        GetTrackingOpenings => api::tracking_openings,
        GetTrackingRecords => api::tracking_records,
        GetTrackingRecordPool => api::tracking_record_pool,
        GetTrackingApplyProofInputs => api::tracking_apply_proof_inputs,
    );
    Ok(module)
}

#[derive(Clone)]
struct Probes<Inner> {
    inner: Inner,
    readiness: Readiness,
}

impl<Inner> Service<HttpRequest> for Probes<Inner>
where
    Inner: Service<HttpRequest, Response = HttpResponse>,
{
    type Response = HttpResponse;
    type Error = Inner::Error;
    type Future = Either<Ready<Result<HttpResponse, Inner::Error>>, Inner::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: HttpRequest) -> Self::Future {
        let probe = (request.method() == Method::GET)
            .then(|| match request.uri().path() {
                "/health" => Some((StatusCode::OK, "ok")),
                "/ready" if self.readiness.is_ready() => Some((StatusCode::OK, "ready")),
                "/ready" => Some((StatusCode::SERVICE_UNAVAILABLE, "catching up")),
                _ => None,
            })
            .flatten();
        match probe {
            Some((status, body)) => {
                let mut response = HttpResponse::new(HttpBody::from(body));
                *response.status_mut() = status;
                Either::Left(ready(Ok(response)))
            }
            None => Either::Right(self.inner.call(request)),
        }
    }
}

#[cfg(test)]
mod tests;
