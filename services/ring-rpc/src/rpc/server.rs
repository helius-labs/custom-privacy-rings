use std::{
    future::Future,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};

use hyper::{header::CONTENT_TYPE, Method, StatusCode};
use jsonrpsee::{
    core::{BoxError, RegisterMethodError},
    server::{
        middleware::{
            http::{ProxyGetRequestError, ProxyGetRequestLayer},
            rpc::RpcServiceBuilder,
        },
        serve_with_graceful_shutdown, stop_channel, BatchRequestConfig, HttpBody, HttpRequest,
        HttpResponse, Methods, ServerBuilder, ServerConfig, ServerHandle,
    },
    types::ErrorObjectOwned,
    RpcModule,
};
use serde::{de::DeserializeOwned, Serialize};
use spr_rpc_http::{announce::Announcer, forward::UpstreamUrl, MethodPathLayer};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{watch, Semaphore},
};
use tower::Service;
use tower_http::cors::{AllowOrigin, CorsLayer};

use zolana_ring_client::rpc::{
    CoSignTransactionRequest, CreateAuditorKeyRequest, CreateAuditorKeyResponse,
    CreateRecordsRegistryRequest, EditCoSignListRequest, GetAuditedMergesResponse,
    GetAuditedUtxosRequest, GetCoSignListsRequest, GetDecryptedTransactionsRequest,
    GetDecryptedTransactionsResponse, GetDelegateNotesRequest, GetListSaltRequest,
    GetPendingListApprovalsRequest, GetRecordUpdateStatusRequest, GetRelayQuoteRequest,
    GetRelayStatusRequest, GetSpendCoverageRequest, GetWalletActivityResponse,
    GetWalletStatusRequest, PagedRead, PrepareAuthorityTransferRequest, PrepareRelayRequest,
    RefuseListApprovalRequest, RegisterEnrollmentRingRequest, RingState, RingStatusRequest,
    RingStatusResponse, SetMergeSettingsRequest, SetReaderGrantRequest, SignedVaultEnrollment,
    SubmitEnrollmentRequest, SubmitRecordUpdateRequest, SubmitRelayRequest, VaultEnrollmentRequest,
    CO_SIGN_TRANSACTION, CREATE_AUDITOR_KEY, CREATE_RECORDS_REGISTRY, EDIT_CO_SIGN_LIST,
    GET_AUDITED_UTXOS, GET_CO_SIGN_LISTS, GET_DELEGATE_NOTES, GET_LIST_SALT,
    GET_PENDING_LIST_APPROVALS, GET_RECORD_UPDATE_STATUS, GET_RELAY_QUOTE, GET_RELAY_STATUS,
    GET_SPEND_COVERAGE, GET_WALLET_STATUS, HEALTH, PREPARE_AUTHORITY_TRANSFER, PREPARE_RELAY,
    PREPARE_VAULT_ENROLLMENT, REFUSE_LIST_APPROVAL, REGISTER_ENROLLMENT_RING,
    RELEASE_DELEGATE_SNAPSHOT, RELEASE_WALLET_SNAPSHOT, RING_STATUS, SET_MERGE_SETTINGS,
    SET_READER_GRANT, SUBMIT_ENROLLMENT, SUBMIT_RECORD_UPDATE, SUBMIT_RELAY,
    SUBMIT_VAULT_ENROLLMENT,
};

use crate::{
    audit::{AuditRead, Page},
    client_identity::PeerAddress,
    client_rate::{ClientLayer, ClientLimits, ClientRate, ClientRateLimiter},
    error::RingRpcError,
    health::health,
    hub::{Hub, Lane, ReadinessStatus},
    origins::{OriginError, Origins},
    relay::Relay,
    upstream::TransactionSource,
};

const MAX_REQUEST_BODY_SIZE: u32 = 64 * 1024;
const MAX_RESPONSE_BODY_SIZE: u32 = 16 * 1024 * 1024;
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(50);
const IDLE_TIMEOUTS: IdleTimeouts = IdleTimeouts {
    first_request: Duration::from_secs(10),
    keep_alive: Duration::from_secs(60),
};
/// Bounds how long a refused client holds its socket.
const REFUSAL_TIMEOUT: Duration = Duration::from_secs(1);
const SERVER_FULL: &[u8] = b"HTTP/1.1 503 Service Unavailable\r\nconnection: close\r\n\
retry-after: 1\r\ncontent-length: 0\r\n\r\n";

/// How long a connection with no request in flight keeps its slot.
#[derive(Clone, Copy)]
struct IdleTimeouts {
    /// From the accept to the end of the first request's headers.
    first_request: Duration,
    /// From the end of a response to the next request.
    keep_alive: Duration,
}

/// Only a loopback bind keeps the decrypted audit data behind a TLS proxy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindPolicy {
    LoopbackOnly,
    InsecurePublic,
}

pub struct ServerOptions {
    pub bind: IpAddr,
    pub bind_policy: BindPolicy,
    pub port: u16,
    pub max_connections: u32,
    pub request_timeout: Duration,
    pub client_limits: ClientLimits,
    pub announce: Option<Announce>,
}

/// Has Photon forward the served methods here once its operator approves the service key.
pub struct Announce {
    pub registry: UpstreamUrl,
    /// The service URL Photon connects to.
    pub url: UpstreamUrl,
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("ring RPC must bind to a loopback address")]
    PublicBind,
    #[error(transparent)]
    Origin(#[from] OriginError),
    #[error("health proxy setup failed")]
    Proxy(#[from] ProxyGetRequestError),
    #[error("RPC listener setup failed")]
    Listener(#[from] std::io::Error),
    #[error("RPC method setup failed")]
    Method(#[from] RegisterMethodError),
}

pub async fn run_server<S: TransactionSource + 'static>(
    hub: Arc<Hub<S>>,
    options: ServerOptions,
) -> Result<ServerHandle, ServerError> {
    run_server_with_idle_timeouts(hub, options, IDLE_TIMEOUTS).await
}

async fn run_server_with_idle_timeouts<S: TransactionSource + 'static>(
    hub: Arc<Hub<S>>,
    options: ServerOptions,
    idle_timeouts: IdleTimeouts,
) -> Result<ServerHandle, ServerError> {
    if options.bind_policy == BindPolicy::LoopbackOnly && !options.bind.is_loopback() {
        return Err(ServerError::PublicBind);
    }
    let listener = TcpListener::bind(SocketAddr::from((options.bind, options.port))).await?;
    let limiter = Arc::new(ClientRateLimiter::new(options.client_limits.rates));
    let middleware = tower::ServiceBuilder::new()
        .layer(cors(hub.origins())?)
        .layer(tower::timeout::TimeoutLayer::new(options.request_timeout))
        .layer(tower::layer::layer_fn({
            let hub = hub.clone();
            move |inner| ReadyService {
                inner,
                hub: hub.clone(),
            }
        }))
        .layer(ClientLayer::new(options.client_limits.identity))
        .layer(ProxyGetRequestLayer::new([("/health", HEALTH)])?)
        .layer(MethodPathLayer {
            max_body_size: MAX_REQUEST_BODY_SIZE as usize,
        });
    let services = ServerBuilder::default()
        .set_config(
            ServerConfig::builder()
                // A WebSocket call runs outside the timeout layer and the CORS check.
                .http_only()
                .max_connections(options.max_connections)
                .max_request_body_size(MAX_REQUEST_BODY_SIZE)
                .max_response_body_size(MAX_RESPONSE_BODY_SIZE)
                .set_batch_request_config(BatchRequestConfig::Disabled)
                .build(),
        )
        .set_http_middleware(middleware)
        .set_rpc_middleware(
            RpcServiceBuilder::new().layer_fn(move |service| ClientRate {
                service,
                limiter: limiter.clone(),
            }),
        )
        .to_service_builder();
    let methods = Methods::from(rpc_module(hub.clone())?);
    let (stop, handle) = stop_channel();
    if let Some(Announce { registry, url }) = options.announce {
        let announcing = Announcer {
            registry,
            url,
            methods: methods.method_names().map(str::to_owned).collect(),
            pubkey: hub.service_pubkey(),
        }
        .spawn_with(move |message| {
            let hub = hub.clone();
            async move { hub.sign_attestation(&message).await }
        });
        let stop = stop.clone();
        tokio::spawn(async move {
            stop.shutdown().await;
            announcing.abort();
        });
    }
    // An owned accept loop, the jsonrpsee server hides the peer address from middleware.
    let slots = usize::try_from(options.max_connections.max(1)).unwrap_or(usize::MAX);
    let connections = Arc::new(Semaphore::new(slots));
    // As many refusals as connections answer at once, a flood past both is dropped unanswered.
    let refusals = Arc::new(Semaphore::new(slots));
    tokio::spawn(async move {
        loop {
            let (socket, peer) = tokio::select! {
                accepted = listener.accept() => match accepted {
                    Ok(accepted) => accepted,
                    Err(_) => {
                        tokio::select! {
                            () = tokio::time::sleep(ACCEPT_ERROR_BACKOFF) => continue,
                            () = stop.clone().shutdown() => break,
                        }
                    }
                },
                () = stop.clone().shutdown() => break,
            };
            let Ok(permit) = connections.clone().try_acquire_owned() else {
                if let Ok(refusal) = refusals.clone().try_acquire_owned() {
                    tokio::spawn(async move {
                        let _refusal = refusal;
                        let _ = tokio::time::timeout(REFUSAL_TIMEOUT, refuse(socket)).await;
                    });
                }
                continue;
            };
            let _ = socket.set_nodelay(true);
            let service = services.clone().build(methods.clone(), stop.clone());
            let (activity, idle) = ConnectionActivity::new(idle_timeouts);
            let connection =
                tower::service_fn(move |request: HttpRequest<hyper::body::Incoming>| {
                    let guard = activity.enter();
                    let mut request = request.map(HttpBody::new);
                    request.extensions_mut().insert(PeerAddress(peer));
                    let mut service = service.clone();
                    async move {
                        let _guard = guard;
                        service.call(request).await
                    }
                });
            let serving = tokio::spawn(serve_with_graceful_shutdown(
                socket,
                connection,
                stop.clone().shutdown(),
            ));
            tokio::spawn(async move {
                let _permit = permit;
                let mut serving = serving;
                tokio::select! {
                    _ = &mut serving => {}
                    () = wait_idle_deadline(idle) => serving.abort(),
                }
            });
        }
    });
    Ok(handle)
}

fn cors(origins: &Origins) -> Result<CorsLayer, OriginError> {
    if origins.is_empty() {
        return Ok(CorsLayer::new());
    }
    let allow = if origins.allows_any() {
        AllowOrigin::any()
    } else {
        AllowOrigin::list(origins.header_values()?)
    };
    Ok(CorsLayer::new()
        .allow_origin(allow)
        .allow_methods([Method::POST, Method::GET])
        .allow_headers([CONTENT_TYPE]))
}

pub fn rpc_module<S: TransactionSource + 'static>(
    hub: Arc<Hub<S>>,
) -> Result<RpcModule<Arc<Hub<S>>>, RegisterMethodError> {
    let relay = hub.relay().cloned();
    let mut module = RpcModule::new(hub);
    module.register_async_method(HEALTH, |_, hub, _| async move {
        Ok::<_, ErrorObjectOwned>(health(&hub))
    })?;
    method(
        &mut module,
        CREATE_AUDITOR_KEY,
        |hub, request: CreateAuditorKeyRequest| async move {
            let _slot = hub.admit(Lane::Authentication)?;
            let service = hub.service(Some(request.ring_program_id.0))?;
            let statement = service.auditor_key_statement(&request.auth).await?;
            let signature = hub.sign_attestation(&statement).await?;
            let (ring, auditor_pubkey) = (service.ring(), service.auditor_pubkey());
            Ok(CreateAuditorKeyResponse {
                ring_program_id: ring.into(),
                auditor_pubkey: auditor_pubkey.into(),
                auditor_view_tag: service.auditor_view_tag().into(),
                service_pubkey: hub.service_pubkey().into(),
                signature: signature.into(),
            })
        },
    )?;
    method(
        &mut module,
        RING_STATUS,
        |hub, request: RingStatusRequest| async move {
            let _slot = hub.admit(Lane::Public)?;
            let service = hub.service(Some(request.ring_program_id.0))?;
            let configured = service.configured_auditor().await?;
            let chain = service.chain_status().await?;
            let state = match configured {
                None => RingState::Uninitialized,
                Some(key) if key == service.auditor_pubkey() => RingState::Served,
                Some(_) => RingState::ForeignAuditor,
            };
            Ok(RingStatusResponse {
                ring_program_id: service.ring().into(),
                state,
                config_auditor_pubkey: configured.map(Into::into),
                service_pubkey: hub.service_pubkey().into(),
                activation: chain.activation,
            })
        },
    )?;
    for read in [
        PagedRead::DecryptedTransactions,
        PagedRead::WalletActivity,
        PagedRead::AuditedMerges,
    ] {
        method(&mut module, read.method(), move |hub, request| {
            paged_read(hub, request, read)
        })?;
    }
    method(
        &mut module,
        GET_AUDITED_UTXOS,
        |hub, request: GetAuditedUtxosRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            let service = hub.service(request.ring_program_id.map(|ring| ring.0))?;
            service.audited_utxos(&request).await
        },
    )?;
    method(
        &mut module,
        GET_WALLET_STATUS,
        |hub, request: GetWalletStatusRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            let service = hub.service(request.ring_program_id.map(|ring| ring.0))?;
            service.wallet_status(&request).await
        },
    )?;
    method(
        &mut module,
        RELEASE_WALLET_SNAPSHOT,
        |hub, request: GetWalletStatusRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            let service = hub.service(request.ring_program_id.map(|ring| ring.0))?;
            service.release_wallet_snapshot(&request).await
        },
    )?;
    method(
        &mut module,
        RELEASE_DELEGATE_SNAPSHOT,
        |hub, request: GetDelegateNotesRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            hub.release_delegate_snapshot(&request).await
        },
    )?;
    method(
        &mut module,
        GET_DELEGATE_NOTES,
        |hub, request: GetDelegateNotesRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            hub.delegate_notes(&request).await
        },
    )?;
    method(
        &mut module,
        GET_LIST_SALT,
        |hub, request: GetListSaltRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            hub.list_salt(&request).await
        },
    )?;
    method(
        &mut module,
        GET_SPEND_COVERAGE,
        |hub, request: GetSpendCoverageRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            let service = hub.service(Some(request.ring.0))?;
            service.spend_coverage(&request).await
        },
    )?;
    method(
        &mut module,
        SUBMIT_RECORD_UPDATE,
        |hub, request: SubmitRecordUpdateRequest| async move {
            let _slot = hub.admit(Lane::Authentication)?;
            hub.record_writer()?.submit(request).await
        },
    )?;
    method(
        &mut module,
        PREPARE_VAULT_ENROLLMENT,
        |hub, request: VaultEnrollmentRequest| async move {
            let _slot = hub.admit(Lane::Authentication)?;
            hub.enrollment_applier()?.prepare_vault(request).await
        },
    )?;
    method(
        &mut module,
        SUBMIT_VAULT_ENROLLMENT,
        |hub, request: SignedVaultEnrollment| async move {
            let _slot = hub.admit(Lane::Authentication)?;
            hub.enrollment_applier()?.submit_vault(request).await
        },
    )?;
    method(
        &mut module,
        SUBMIT_ENROLLMENT,
        |hub, request: SubmitEnrollmentRequest| async move {
            let _slot = hub.admit(Lane::Authentication)?;
            hub.enrollment_applier()?.submit(request).await
        },
    )?;
    method(
        &mut module,
        GET_RECORD_UPDATE_STATUS,
        |hub, request: GetRecordUpdateStatusRequest| async move {
            let _slot = hub.admit(Lane::Public)?;
            hub.record_writer()?.status(request).await
        },
    )?;
    method(
        &mut module,
        GET_PENDING_LIST_APPROVALS,
        |hub, request: GetPendingListApprovalsRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            hub.pending_list_approvals(&request).await
        },
    )?;
    method(
        &mut module,
        REFUSE_LIST_APPROVAL,
        |hub, request: RefuseListApprovalRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            hub.refuse_list_approval(&request).await
        },
    )?;
    method(
        &mut module,
        EDIT_CO_SIGN_LIST,
        |hub, request: EditCoSignListRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            hub.edit_co_sign_list(&request).await
        },
    )?;
    method(
        &mut module,
        GET_CO_SIGN_LISTS,
        |hub, request: GetCoSignListsRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            hub.co_sign_lists(&request).await
        },
    )?;
    method(
        &mut module,
        CREATE_RECORDS_REGISTRY,
        |hub, request: CreateRecordsRegistryRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            hub.create_records_registry(&request).await
        },
    )?;
    method(
        &mut module,
        REGISTER_ENROLLMENT_RING,
        |hub, request: RegisterEnrollmentRingRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            hub.register_enrollment_ring(&request).await
        },
    )?;
    method(
        &mut module,
        SET_READER_GRANT,
        |hub, request: SetReaderGrantRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            let service = hub.service(Some(request.ring.0))?;
            service.set_reader_grant(&request).await
        },
    )?;
    method(
        &mut module,
        SET_MERGE_SETTINGS,
        |hub, request: SetMergeSettingsRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            hub.set_merge_settings(&request).await
        },
    )?;
    method(
        &mut module,
        PREPARE_AUTHORITY_TRANSFER,
        |hub, request: PrepareAuthorityTransferRequest| async move {
            let _authentication = hub.admit(Lane::Authentication)?;
            hub.prepare_authority_transfer(&request).await
        },
    )?;
    if let Some(relay) = relay {
        register_relay_methods(&mut module, relay)?;
    }
    Ok(module)
}

fn register_relay_methods<S: TransactionSource + 'static>(
    module: &mut RpcModule<Arc<Hub<S>>>,
    relay: Arc<Relay>,
) -> Result<(), RegisterMethodError> {
    let quote_relay = relay.clone();
    method(
        module,
        GET_RELAY_QUOTE,
        move |hub, request: GetRelayQuoteRequest| {
            let relay = quote_relay.clone();
            async move {
                let _slot = hub.admit(Lane::Public)?;
                relay.quote(&request).await
            }
        },
    )?;
    let prepare_relay = relay.clone();
    method(
        module,
        PREPARE_RELAY,
        move |hub, request: PrepareRelayRequest| {
            let relay = prepare_relay.clone();
            async move {
                let _slot = hub.admit(Lane::Authentication)?;
                relay.prepare(&request).await
            }
        },
    )?;
    let submit_relay = relay.clone();
    method(
        module,
        SUBMIT_RELAY,
        move |hub, request: SubmitRelayRequest| {
            let relay = submit_relay.clone();
            async move {
                let _slot = hub.admit(Lane::Authentication)?;
                relay.submit(&request).await
            }
        },
    )?;
    let co_sign_relay = relay.clone();
    method(
        module,
        CO_SIGN_TRANSACTION,
        move |hub, request: CoSignTransactionRequest| {
            let relay = co_sign_relay.clone();
            async move {
                let _slot = hub.admit(Lane::Authentication)?;
                relay.co_sign_transaction(&request).await
            }
        },
    )?;
    method(
        module,
        GET_RELAY_STATUS,
        move |hub, request: GetRelayStatusRequest| {
            let relay = relay.clone();
            async move {
                let _slot = hub.admit(Lane::Public)?;
                relay.status(&request).await
            }
        },
    )?;
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(untagged)]
enum PagedResponse {
    Transactions(GetDecryptedTransactionsResponse),
    WalletActivity(GetWalletActivityResponse),
    Merges(GetAuditedMergesResponse),
}

async fn paged_read<S: TransactionSource>(
    hub: Arc<Hub<S>>,
    request: GetDecryptedTransactionsRequest,
    read: PagedRead,
) -> Result<PagedResponse, RingRpcError> {
    let authentication = hub.admit(Lane::Authentication)?;
    let service = hub.service(request.ring_program_id.map(|ring| ring.0))?;
    let page = Page::new(request.cursor, request.limit)?;
    let authorized = service
        .authorize(AuditRead {
            read,
            auth: &request.auth,
            page: &page,
            wallet: request.wallet.map(|wallet| wallet.0),
        })
        .await?;
    // Decrypting holds a read slot, the authentication slot is free again.
    drop(authentication);
    let _slot = hub.admit(Lane::Read)?;
    Ok(match read {
        PagedRead::DecryptedTransactions => {
            PagedResponse::Transactions(authorized.execute().await?)
        }
        PagedRead::WalletActivity => {
            PagedResponse::WalletActivity(authorized.execute_wallet_activity().await?)
        }
        PagedRead::AuditedMerges => PagedResponse::Merges(authorized.execute_merges().await?),
    })
}

/// Every handler error becomes a JSON-RPC error through one conversion.
fn method<C, P, T, F, Fut>(
    module: &mut RpcModule<Arc<C>>,
    name: &'static str,
    handler: F,
) -> Result<(), RegisterMethodError>
where
    C: Send + Sync + 'static,
    P: DeserializeOwned + Send + 'static,
    T: Serialize + Clone + Send + 'static,
    F: Fn(Arc<C>, P) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<T, RingRpcError>> + Send,
{
    module.register_async_method(name, move |params, context, _| {
        let handler = handler.clone();
        async move {
            let request = params.parse::<P>()?;
            handler(Arc::clone(&*context), request)
                .await
                .map_err(ErrorObjectOwned::from)
        }
    })?;
    Ok(())
}

/// Answers 503 and reads the request away, a close over unread bytes would reset the answer.
async fn refuse(mut socket: tokio::net::TcpStream) -> std::io::Result<()> {
    socket.write_all(SERVER_FULL).await?;
    socket.shutdown().await?;
    let mut unread = [0; 1024];
    while socket.read(&mut unread).await? != 0 {}
    Ok(())
}

struct ConnectionActivity {
    in_flight: AtomicUsize,
    deadline: watch::Sender<Option<tokio::time::Instant>>,
    keep_alive: Duration,
}

struct InFlightGuard {
    activity: Arc<ConnectionActivity>,
}

impl ConnectionActivity {
    /// A socket that sends nothing ends at `first_request`, an idle one at `keep_alive` past its
    /// last response.
    fn new(timeouts: IdleTimeouts) -> (Arc<Self>, watch::Receiver<Option<tokio::time::Instant>>) {
        let initial = tokio::time::Instant::now() + timeouts.first_request;
        let (deadline, receiver) = watch::channel(Some(initial));
        (
            Arc::new(Self {
                in_flight: AtomicUsize::new(0),
                deadline,
                keep_alive: timeouts.keep_alive,
            }),
            receiver,
        )
    }

    fn enter(self: &Arc<Self>) -> InFlightGuard {
        if self.in_flight.fetch_add(1, Ordering::AcqRel) == 0 {
            let _ = self.deadline.send(None);
        }
        InFlightGuard {
            activity: self.clone(),
        }
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if self.activity.in_flight.fetch_sub(1, Ordering::AcqRel) == 1 {
            let next = tokio::time::Instant::now() + self.activity.keep_alive;
            let _ = self.activity.deadline.send(Some(next));
        }
    }
}

async fn wait_idle_deadline(mut receiver: watch::Receiver<Option<tokio::time::Instant>>) {
    loop {
        let target = *receiver.borrow_and_update();
        match target {
            Some(at) => tokio::select! {
                () = tokio::time::sleep_until(at) => return,
                changed = receiver.changed() => {
                    if changed.is_err() {
                        std::future::pending::<()>().await;
                    }
                }
            },
            None => {
                if receiver.changed().await.is_err() {
                    std::future::pending::<()>().await;
                }
            }
        }
    }
}

struct ReadyService<S, Inner> {
    inner: Inner,
    hub: Arc<Hub<S>>,
}

impl<S, Inner: Clone> Clone for ReadyService<S, Inner> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            hub: self.hub.clone(),
        }
    }
}

impl<S, Inner, B> Service<HttpRequest<B>> for ReadyService<S, Inner>
where
    S: TransactionSource + 'static,
    Inner: Service<HttpRequest<B>, Response = HttpResponse>,
    Inner::Error: Into<BoxError> + 'static,
    Inner::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = HttpResponse;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, request: HttpRequest<B>) -> Self::Future {
        if request.method() == Method::GET && request.uri().path() == "/ready" {
            let hub = self.hub.clone();
            return Box::pin(async move {
                let (status, body) = match hub.readiness().await {
                    ReadinessStatus::Ready => (StatusCode::OK, "ready"),
                    ReadinessStatus::Unavailable(check) => (StatusCode::SERVICE_UNAVAILABLE, check),
                };
                let mut response = hyper::Response::new(HttpBody::from(body));
                *response.status_mut() = status;
                response.headers_mut().insert(
                    CONTENT_TYPE,
                    hyper::header::HeaderValue::from_static("text/plain; charset=utf-8"),
                );
                Ok(response)
            });
        }
        let future = self.inner.call(request);
        Box::pin(async move { future.await.map_err(Into::into) })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        net::Ipv4Addr,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use solana_address::Address;
    use solana_keypair::Keypair;
    use solana_signature::Signature;
    use spr_client::{ClientError, GetShieldedTransactionsByTagsResponse};
    use spr_keypair::X25519Keypair;
    use zolana_ring_client::RingIndexerError;
    use zolana_ring_client::{
        rpc::{HealthResponse, KeyMode, RingChainStatus, GET_DECRYPTED_TRANSACTIONS},
        AuditError, RingOrigin,
    };
    use zolana_ring_transaction::RingAssets;

    use spr_indexer_api::Hash;
    use zolana_ring_client::rpc::RecordUpdate;

    use crate::{
        client_identity::ClientIdentityPolicy,
        client_rate::{BucketRate, ClientRates, RateLimit, NETWORK_CLIENTS},
        config::RootSecret,
        error::RingRpcError,
        hub::ServiceIdentity,
        limits::{
            MAX_CONCURRENT_AUTHENTICATIONS, MAX_CONCURRENT_READS, MAX_REQUEST_UNITS_PER_SECOND,
        },
        origins::OriginPolicy,
        record_update::RecordUpdateRefusal,
        upstream::{
            MergeAuditRequest, RecordsTip, RingConfiguration, RingFrame, RingHistoryPage,
            TransactionPage,
        },
    };

    use super::*;

    const SERVED_RING: Address = Address::new_from_array([8; 32]);
    const TEST_GENESIS: [u8; 32] = [3; 32];

    struct TestSource {
        healthy: bool,
        delay: Duration,
        config: Option<RingConfiguration>,
        health_calls: Arc<AtomicUsize>,
    }

    impl TransactionSource for TestSource {
        async fn is_mint_delegate(
            &self,
            _mint: Address,
            _signer: Address,
        ) -> Result<bool, ClientError> {
            Ok(false)
        }

        async fn transactions_by_tag(
            &self,
            _request: TransactionPage,
        ) -> Result<GetShieldedTransactionsByTagsResponse, ClientError> {
            Err(ClientError::Rpc("unused source".to_owned()))
        }

        async fn ring_frame(
            &self,
            _frame: RingFrame,
        ) -> Result<Vec<spr_transaction::ShieldedTransaction>, ClientError> {
            Err(ClientError::Rpc("unused source".to_owned()))
        }

        async fn ring_history(
            &self,
            _request: RingHistoryPage,
        ) -> Result<GetShieldedTransactionsByTagsResponse, ClientError> {
            Err(ClientError::Rpc("unused source".to_owned()))
        }

        async fn merge_audit_page(
            &self,
            _request: MergeAuditRequest<'_>,
        ) -> Result<zolana_ring_client::MergeAuditPage, AuditError> {
            Err(AuditError::InvalidMemberRegistry)
        }

        async fn transaction_origin(
            &self,
            _signature: Signature,
            _event_index: u16,
            _ring: Address,
        ) -> Result<RingOrigin, AuditError> {
            Err(AuditError::Unavailable {
                signature: _signature,
                message: "unused source".to_owned(),
            })
        }

        async fn ring_config(
            &self,
            _ring: Address,
        ) -> Result<Option<RingConfiguration>, ClientError> {
            Ok(self.config)
        }

        async fn ring_chain_status(&self, _ring: Address) -> Result<RingChainStatus, ClientError> {
            Ok(RingChainStatus::default())
        }

        async fn slot(&self) -> Result<u64, ClientError> {
            Err(ClientError::Rpc("unused source".to_owned()))
        }

        async fn records_root(&self, _ring: Address) -> Result<RecordsTip, ClientError> {
            Ok(RecordsTip::NoPolicy)
        }

        async fn storage_member_openings(
            &self,
            _request: zolana_ring_indexer::rpc::GetStorageMemberOpeningsRequest,
        ) -> Result<zolana_ring_indexer::rpc::GetStorageMemberOpeningsResponse, RingIndexerError>
        {
            Err(ClientError::Rpc("unused source".to_owned()).into())
        }

        async fn storage_member_records(
            &self,
            _request: zolana_ring_indexer::rpc::GetStorageMemberRecordsRequest,
        ) -> Result<zolana_ring_indexer::rpc::GetStorageMemberRecordsResponse, RingIndexerError>
        {
            Err(ClientError::Rpc("unused source".to_owned()).into())
        }

        async fn upgrade_authority(
            &self,
            _program: Address,
        ) -> Result<Option<Address>, ClientError> {
            Ok(None)
        }

        fn health(&self) -> impl Future<Output = Result<(), ClientError>> + Send {
            let healthy = self.healthy;
            let delay = self.delay;
            let calls = self.health_calls.clone();
            async move {
                calls.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(delay).await;
                healthy
                    .then_some(())
                    .ok_or_else(|| ClientError::Rpc("unavailable".to_owned()))
            }
        }

        async fn genesis_hash(&self) -> Result<[u8; 32], ClientError> {
            Ok(TEST_GENESIS)
        }

        async fn tree_id(&self, _tree: Address) -> Result<u16, custom_ring_sdk::RingError> {
            Ok(1)
        }

        async fn ring_assets(&self) -> Result<RingAssets, ClientError> {
            Ok(RingAssets::new([]).unwrap())
        }
    }

    fn port() -> u16 {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("listener");
        listener.local_addr().expect("address").port()
    }

    fn hub(mut source: TestSource, origins: Origins) -> Arc<Hub<TestSource>> {
        let auditor = X25519Keypair::new();
        source.config = Some(RingConfiguration {
            auditor_pubkey: auditor.pubkey(),
            roles: custom_ring_interface::RingRoles {
                policy: Address::new_from_array([2; 32]),
                ..bytemuck::Zeroable::zeroed()
            },
            mode: custom_ring_interface::RingMode::Confidential,
            delegate: None,
            ring_list_tag: [0; 32],
            policy: bytemuck::Zeroable::zeroed(),
        });
        Arc::new(
            Hub::builder(source, identity(TEST_GENESIS))
                .with_origins(origins)
                .local(SERVED_RING, auditor)
                .expect("hub"),
        )
    }

    fn derived_hub(source: TestSource, genesis: [u8; 32]) -> Arc<Hub<TestSource>> {
        Arc::new(
            Hub::builder(source, identity(genesis))
                .derived(RootSecret::from_bytes([5; 32]).expect("root"))
                .expect("hub"),
        )
    }

    fn identity(genesis_hash: [u8; 32]) -> ServiceIdentity {
        ServiceIdentity {
            genesis_hash,
            service: Arc::new(Keypair::new()),
        }
    }

    fn source() -> TestSource {
        TestSource {
            healthy: true,
            delay: Duration::ZERO,
            config: None,
            health_calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn params<T: serde::Serialize>(value: T) -> serde_json::Map<String, serde_json::Value> {
        let serde_json::Value::Object(map) = serde_json::to_value(value).expect("JSON object")
        else {
            panic!("expected JSON object");
        };
        map
    }

    fn options(port: u16, timeout: Duration) -> ServerOptions {
        ServerOptions {
            bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
            bind_policy: BindPolicy::LoopbackOnly,
            port,
            max_connections: 8,
            request_timeout: timeout,
            client_limits: ClientLimits::default(),
            announce: None,
        }
    }

    #[tokio::test]
    async fn the_service_key_announces_the_served_methods_to_the_registry() {
        use spr_indexer_api::{AnnounceUpstreamRequest, AnnounceUpstreamResponse};

        let (announced, mut announcements) = tokio::sync::mpsc::unbounded_channel();
        let mut registry = RpcModule::new(announced);
        registry
            .register_method("announceUpstream", |params, announced, _| {
                let request = params.parse::<AnnounceUpstreamRequest>()?;
                announced.send(request).expect("receiver");
                Ok::<_, ErrorObjectOwned>(AnnounceUpstreamResponse { approved: false })
            })
            .expect("method");
        let server = ServerBuilder::default()
            .build("127.0.0.1:0")
            .await
            .expect("registry");
        let registry_url = format!("http://{}", server.local_addr().expect("address"));
        let registry = server.start(registry);

        let hub = hub(source(), Origins::default());
        let served = rpc_module(hub.clone()).expect("module");
        let handle = run_server(
            hub.clone(),
            ServerOptions {
                announce: Some(Announce {
                    registry: registry_url.parse().expect("registry URL"),
                    url: "http://ring:8785".parse().expect("public URL"),
                }),
                ..options(port(), Duration::from_secs(1))
            },
        )
        .await
        .expect("server");

        let AnnounceUpstreamRequest {
            pubkey,
            announcement,
            signature,
        } = announcements.recv().await.expect("announcement");
        assert_eq!(pubkey.0, hub.service_pubkey());
        assert!(signature
            .0
            .verify(pubkey.0.as_ref(), &announcement.message()));
        assert_eq!(announcement.url, "http://ring:8785");
        let mut expected = served.method_names().collect::<Vec<_>>();
        expected.sort_unstable();
        let mut methods = announcement.methods;
        methods.sort_unstable();
        assert_eq!(methods, expected);
        assert!(methods.iter().any(|method| method == RING_STATUS));
        assert!(!methods.iter().any(|method| method == SUBMIT_RELAY));
        for server in [handle, registry] {
            server.stop().expect("stop");
        }
    }

    /// A server granting each client a burst of two signed writes.
    async fn write_limited_server(identity: ClientIdentityPolicy) -> (ServerHandle, String) {
        let server_port = port();
        let handle = run_server(
            hub(source(), Origins::default()),
            ServerOptions {
                client_limits: ClientLimits {
                    rates: ClientRates {
                        signed_writes: BucketRate::new(1, 2),
                        ..ClientRates::default()
                    },
                    identity,
                },
                ..options(server_port, Duration::from_secs(1))
            },
        )
        .await
        .expect("server");
        (handle, format!("http://127.0.0.1:{server_port}"))
    }

    async fn submit_as(url: &str, client: &str) -> reqwest::Response {
        let update = SubmitRecordUpdateRequest {
            registry: Address::new_from_array([4; 32]).into(),
            update: RecordUpdate {
                genesis_hash: Hash(TEST_GENESIS),
                ring: SERVED_RING.into(),
                entries: Vec::new(),
                valid_until_slot: 1,
                awaited_lists: 0,
            },
            signer: Address::new_from_array([6; 32]).into(),
            signature: Signature::default().into(),
            unregistered_owners: Vec::new(),
        };
        reqwest::Client::new()
            .post(url)
            .header("x-forwarded-for", client)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": SUBMIT_RECORD_UPDATE,
                "params": update,
            }))
            .send()
            .await
            .expect("submit")
    }

    async fn error_code(response: reqwest::Response) -> serde_json::Value {
        let body: serde_json::Value = response.json().await.expect("JSON-RPC answer");
        body["error"]["code"].clone()
    }

    #[tokio::test]
    async fn a_client_past_its_signed_write_rate_waits_while_another_submits() {
        let proxy = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let (handle, url) =
            write_limited_server(ClientIdentityPolicy::default().with_trusted_proxies([proxy]))
                .await;
        let served = RecordUpdateRefusal::Disabled.code();

        for _ in 0..2 {
            assert_eq!(
                error_code(submit_as(&url, "198.51.100.1").await).await,
                served
            );
        }
        let refused = submit_as(&url, "198.51.100.1").await;
        assert_eq!(refused.status(), StatusCode::TOO_MANY_REQUESTS);
        let retry_after = refused
            .headers()
            .get("retry-after")
            .expect("retry after")
            .to_str()
            .expect("ascii")
            .parse::<u64>()
            .expect("seconds");
        assert!((1..=60).contains(&retry_after), "{retry_after}");
        let body: serde_json::Value = refused.json().await.expect("refusal");
        assert_eq!(body["error"]["code"], RateLimit::SignedWrites.code());
        assert!(
            body["error"]["data"]["retryAfterMs"]
                .as_u64()
                .expect("hint")
                > 0
        );

        let other = submit_as(&url, "198.51.100.2").await;
        assert_eq!(other.status(), StatusCode::OK);
        assert_eq!(error_code(other).await, served);
        handle.stop().expect("stop");
        handle.stopped().await;
    }

    #[tokio::test]
    async fn the_64s_of_one_ipv6_48_share_its_signed_write_rate() {
        let proxy = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let (handle, url) =
            write_limited_server(ClientIdentityPolicy::default().with_trusted_proxies([proxy]))
                .await;
        let served = RecordUpdateRefusal::Disabled.code();

        for subnet in 0..NETWORK_CLIENTS.get() {
            for _ in 0..2 {
                let client = format!("2001:db8:0:{subnet:x}::1");
                assert_eq!(error_code(submit_as(&url, &client).await).await, served);
            }
        }
        assert_eq!(
            error_code(submit_as(&url, "2001:db8:0:ffff::1").await).await,
            RateLimit::SignedWrites.code()
        );
        assert_eq!(
            error_code(submit_as(&url, "2001:db8:1::1").await).await,
            served
        );
        handle.stop().expect("stop");
        handle.stopped().await;
    }

    #[tokio::test]
    async fn a_forwarded_header_from_an_untrusted_peer_keeps_the_peer_identity() {
        let (handle, url) = write_limited_server(ClientIdentityPolicy::default()).await;

        for _ in 0..2 {
            submit_as(&url, "198.51.100.1").await;
        }
        assert_eq!(
            error_code(submit_as(&url, "198.51.100.2").await).await,
            RateLimit::SignedWrites.code()
        );
        handle.stop().expect("stop");
        handle.stopped().await;
    }

    #[tokio::test]
    async fn server_enforces_transport_boundaries() {
        let origins = OriginPolicy::new(vec!["http://localhost:3000".to_owned()])
            .with_relying_party_id("localhost".to_owned())
            .build()
            .expect("origins");
        let server_port = port();
        let handle = run_server(
            hub(source(), origins),
            options(server_port, Duration::from_secs(1)),
        )
        .await
        .expect("server");
        let url = format!("http://127.0.0.1:{server_port}");
        let client = reqwest::Client::new();

        let allowed = client
            .post(&url)
            .header("origin", "http://localhost:3000")
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": HEALTH,
            }))
            .send()
            .await
            .expect("allowed request");
        assert_eq!(
            allowed
                .headers()
                .get("access-control-allow-origin")
                .expect("CORS"),
            "http://localhost:3000"
        );

        let denied = client
            .post(&url)
            .header("origin", "https://evil.example")
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": HEALTH,
            }))
            .send()
            .await
            .expect("denied request");
        assert!(denied
            .headers()
            .get("access-control-allow-origin")
            .is_none());

        let batch = client
            .post(&url)
            .json(&serde_json::json!([{
                "jsonrpc": "2.0",
                "id": 1,
                "method": HEALTH,
            }]))
            .send()
            .await
            .expect("batch");
        let batch_body: serde_json::Value = batch.json().await.expect("batch body");
        assert_eq!(batch_body["error"]["code"], -32005);

        let oversized = client
            .post(&url)
            .body("a".repeat(MAX_REQUEST_BODY_SIZE as usize + 1))
            .send()
            .await
            .expect("oversized request");
        assert!(!oversized.status().is_success());

        let ready = client
            .get(format!("{url}/ready"))
            .send()
            .await
            .expect("ready");
        assert_eq!(ready.status(), StatusCode::OK);
        handle.stop().expect("stop");
        handle.stopped().await;
    }

    #[tokio::test]
    async fn a_path_runs_only_the_method_it_names() {
        let server_port = port();
        let handle = run_server(
            hub(source(), Origins::default()),
            options(server_port, Duration::from_secs(1)),
        )
        .await
        .expect("server");
        let url = format!("http://127.0.0.1:{server_port}");
        let client = reqwest::Client::new();
        let call = |path: &str, body: serde_json::Value| {
            let request = client.post(format!("{url}{path}")).json(&body);
            async move {
                request
                    .send()
                    .await
                    .expect("call")
                    .json::<serde_json::Value>()
                    .await
                    .expect("JSON-RPC answer")
            }
        };
        let named =
            |method: &str| serde_json::json!({ "jsonrpc": "2.0", "id": 7, "method": method });

        for path in ["/", "/health", "/health/", "/v1/zolana/ring/health"] {
            let answer = call(path, named(HEALTH)).await;
            assert_eq!(answer["result"]["mode"], "local", "{path}: {answer}");
            assert!(
                answer["result"]["servicePubkey"].is_string(),
                "{path}: {answer}"
            );
        }
        for (path, method) in [
            ("/health", GET_DECRYPTED_TRANSACTIONS),
            ("/v1/zolana/ring/health", GET_DECRYPTED_TRANSACTIONS),
            ("/v1/zolana/ring/ringStatus", GET_RELAY_QUOTE),
        ] {
            let answer = call(path, named(method)).await;
            assert_eq!(answer["error"]["code"], -32601, "{path} {method}");
            assert_eq!(answer["id"], 7, "{path} {method}");
        }
        // Not one call naming the path's method: a batch, an unnamed call,
        // or a body that is not JSON.
        for body in [
            serde_json::json!([named(HEALTH)]),
            serde_json::json!({ "jsonrpc": "2.0", "id": 1 }),
            serde_json::json!("health"),
        ] {
            let answer = call("/health", body.clone()).await;
            assert_eq!(answer["error"]["code"], -32601, "{body}");
        }
        let unparsed = client
            .post(format!("{url}/health"))
            .body("not json")
            .send()
            .await
            .expect("unparsed");
        let unparsed: serde_json::Value = unparsed.json().await.expect("refusal");
        assert_eq!(unparsed["error"]["code"], -32601);

        let probe = client
            .get(format!("{url}/health"))
            .send()
            .await
            .expect("health probe");
        assert_eq!(probe.status(), StatusCode::OK);
        handle.stop().expect("stop");
        handle.stopped().await;
    }

    #[tokio::test]
    async fn server_refuses_a_websocket_upgrade() {
        let server_port = port();
        let handle = run_server(
            hub(source(), Origins::default()),
            options(server_port, Duration::from_secs(1)),
        )
        .await
        .expect("server");
        let upgrade = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{server_port}/"))
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .send()
            .await
            .expect("upgrade response");
        assert_ne!(upgrade.status(), StatusCode::SWITCHING_PROTOCOLS);
        handle.stop().expect("stop");
        handle.stopped().await;
    }

    #[tokio::test]
    async fn server_rejects_public_bind_and_failed_readiness() {
        let public = run_server(
            hub(source(), Origins::default()),
            ServerOptions {
                bind: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                bind_policy: BindPolicy::LoopbackOnly,
                port: 0,
                max_connections: 1,
                request_timeout: Duration::from_secs(1),
                client_limits: ClientLimits::default(),
                announce: None,
            },
        )
        .await;
        assert!(matches!(public, Err(ServerError::PublicBind)));
        let insecure = run_server(
            hub(source(), Origins::default()),
            ServerOptions {
                bind: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                bind_policy: BindPolicy::InsecurePublic,
                port: 0,
                max_connections: 1,
                request_timeout: Duration::from_secs(1),
                client_limits: ClientLimits::default(),
                announce: None,
            },
        )
        .await
        .expect("public bind under the insecure policy");
        insecure.stop().expect("stop");

        let failed_port = port();
        let handle = run_server(
            hub(
                TestSource {
                    healthy: false,
                    ..source()
                },
                Origins::default(),
            ),
            options(failed_port, Duration::from_secs(1)),
        )
        .await
        .expect("server");
        let response = reqwest::get(format!("http://127.0.0.1:{failed_port}/ready"))
            .await
            .expect("ready response");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        handle.stop().expect("stop");
        handle.stopped().await;

        let timeout_port = port();
        let handle = run_server(
            hub(
                TestSource {
                    delay: Duration::from_millis(50),
                    ..source()
                },
                Origins::default(),
            ),
            options(timeout_port, Duration::from_millis(5)),
        )
        .await
        .expect("server");
        assert!(
            reqwest::get(format!("http://127.0.0.1:{timeout_port}/ready"))
                .await
                .is_err()
        );
        handle.stop().expect("stop");
        handle.stopped().await;
    }

    #[tokio::test]
    async fn health_and_status_answer_the_served_ring() {
        let module = rpc_module(hub(source(), Origins::default())).expect("module");
        let health: HealthResponse = module.call(HEALTH, [(); 0]).await.expect("health");
        assert_eq!(health.mode, KeyMode::Local);
        let status: RingStatusResponse = module
            .call(
                RING_STATUS,
                params(RingStatusRequest {
                    ring_program_id: SERVED_RING.to_bytes().into(),
                }),
            )
            .await
            .expect("status");
        assert_eq!(status.state, RingState::Served);
    }

    #[tokio::test]
    async fn readiness_probes_share_a_cached_result() {
        let source = source();
        let calls = source.health_calls.clone();
        let hub = hub(source, Origins::default());

        assert_eq!(hub.readiness().await, ReadinessStatus::Ready);
        assert_eq!(hub.readiness().await, ReadinessStatus::Ready);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn cancelled_readiness_waiters_leave_one_probe() {
        let source = TestSource {
            delay: Duration::from_millis(10),
            ..source()
        };
        let calls = source.health_calls.clone();
        let hub = hub(source, Origins::default());
        let waiter = tokio::spawn({
            let hub = hub.clone();
            async move { hub.readiness().await }
        });
        while calls.load(Ordering::Relaxed) == 0 {
            tokio::task::yield_now().await;
        }
        waiter.abort();

        assert_eq!(hub.readiness().await, ReadinessStatus::Ready);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    /// A decrypting page holds its slot for the whole page, and the gate
    /// refuses the next reader rather than queueing it behind them.
    #[test]
    fn concurrent_reads_are_capped_and_refused_rather_than_queued() {
        let hub = hub(source(), Origins::default());
        let held = (0..MAX_CONCURRENT_READS)
            .map(|_| hub.admit(Lane::Read).expect("read slot"))
            .collect::<Vec<_>>();

        assert!(matches!(hub.admit(Lane::Read), Err(RingRpcError::Busy)));
        drop(held);
        let _reopened = hub.admit(Lane::Read).expect("released slot");
    }

    #[test]
    fn authentication_attempts_do_not_consume_audit_capacity() {
        let hub = hub(source(), Origins::default());
        for _ in 0..MAX_REQUEST_UNITS_PER_SECOND {
            let _ = hub.admit(Lane::Authentication).expect("authentication");
        }
        assert!(matches!(
            hub.admit(Lane::Authentication),
            Err(RingRpcError::Busy)
        ));
        let _ = hub.admit(Lane::Read).expect("audit capacity");
    }

    #[test]
    fn concurrent_authentication_attempts_are_capped() {
        let hub = hub(source(), Origins::default());
        let held = (0..MAX_CONCURRENT_AUTHENTICATIONS)
            .map(|_| {
                hub.admit(Lane::Authentication)
                    .expect("authentication slot")
            })
            .collect::<Vec<_>>();

        assert!(matches!(
            hub.admit(Lane::Authentication),
            Err(RingRpcError::Busy)
        ));
        drop(held);
        let _reopened = hub.admit(Lane::Authentication).expect("released slot");
    }

    /// Every derived key is bound to the genesis hash captured at boot, so
    /// another cluster orphans every ring already registered.
    #[tokio::test]
    async fn derived_readiness_checks_the_cluster() {
        let matching = port();
        let handle = run_server(
            derived_hub(source(), TEST_GENESIS),
            options(matching, Duration::from_secs(1)),
        )
        .await
        .expect("server");
        let ready = reqwest::get(format!("http://127.0.0.1:{matching}/ready"))
            .await
            .expect("ready");
        assert_eq!(ready.status(), StatusCode::OK);
        handle.stop().expect("stop");
        handle.stopped().await;

        let repointed = port();
        let handle = run_server(
            derived_hub(source(), [1; 32]),
            options(repointed, Duration::from_secs(1)),
        )
        .await
        .expect("server");
        let response = reqwest::get(format!("http://127.0.0.1:{repointed}/ready"))
            .await
            .expect("ready");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.text().await.expect("body"), "unavailable cluster");
        handle.stop().expect("stop");
        handle.stopped().await;
    }

    const GET_HEALTH: &[u8] = b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n";

    /// One whole response, its status line first.
    async fn response(stream: &mut tokio::net::TcpStream) -> String {
        let mut read = Vec::new();
        loop {
            let text = String::from_utf8_lossy(&read).to_lowercase();
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .map_or(0, |length| length.trim().parse::<usize>().expect("length"));
                if body.len() >= length {
                    return text;
                }
            }
            let mut chunk = [0; 1024];
            match stream.read(&mut chunk).await.expect("response bytes") {
                0 => panic!("closed inside a response, read {text:?}"),
                count => read.extend_from_slice(&chunk[..count]),
            }
        }
    }

    /// Returns once the server closed the socket.
    async fn closed(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
        let mut read = Vec::new();
        let ended = tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut read));
        let _ = ended.await.expect("the server closes the socket");
        read
    }

    /// A freed slot serves the next connection, a full server answers 503 until then.
    async fn serves_again(addr: SocketAddr) {
        let served = async {
            loop {
                let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
                stream.write_all(GET_HEALTH).await.expect("request");
                if response(&mut stream).await.starts_with("http/1.1 200") {
                    return;
                }
                tokio::task::yield_now().await;
            }
        };
        tokio::time::timeout(Duration::from_secs(10), served)
            .await
            .expect("a slot frees");
    }

    #[tokio::test]
    async fn a_full_server_answers_503_and_serves_again_once_a_slot_frees() {
        let listen_port = port();
        let mut opts = options(listen_port, Duration::from_secs(2));
        opts.max_connections = 1;
        let handle = run_server(hub(source(), Origins::default()), opts)
            .await
            .expect("server");
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, listen_port));

        let mut held = tokio::net::TcpStream::connect(addr).await.expect("connect");
        held.write_all(GET_HEALTH).await.expect("request");
        assert!(response(&mut held).await.starts_with("http/1.1 200"));

        let mut excess = tokio::net::TcpStream::connect(addr).await.expect("connect");
        excess.write_all(GET_HEALTH).await.expect("request");
        let refused = String::from_utf8(closed(&mut excess).await).expect("text");
        assert!(
            refused.starts_with("HTTP/1.1 503 Service Unavailable\r\n"),
            "{refused:?}"
        );
        assert!(refused.contains("connection: close\r\n"));

        drop(held);
        serves_again(addr).await;
        handle.stop().expect("stop");
        handle.stopped().await;
    }

    /// The silent socket is the clock, its cut proves the first-request timeout passed.
    #[tokio::test]
    async fn a_silent_socket_is_cut_and_an_idle_keep_alive_connection_is_not() {
        let listen_port = port();
        let mut opts = options(listen_port, Duration::from_secs(2));
        opts.max_connections = 2;
        let timeouts = IdleTimeouts {
            first_request: Duration::from_millis(50),
            keep_alive: Duration::from_secs(3_600),
        };
        let handle =
            run_server_with_idle_timeouts(hub(source(), Origins::default()), opts, timeouts)
                .await
                .expect("server");
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, listen_port));

        let mut keep_alive = tokio::net::TcpStream::connect(addr).await.expect("connect");
        keep_alive.write_all(GET_HEALTH).await.expect("request");
        assert!(response(&mut keep_alive).await.starts_with("http/1.1 200"));

        for partial in [&b""[..], &b"POST / HTTP/1.1\r\nHost: localhost\r\n"[..]] {
            let mut silent = tokio::net::TcpStream::connect(addr).await.expect("connect");
            silent.write_all(partial).await.expect("partial request");
            assert!(closed(&mut silent).await.is_empty());
        }

        keep_alive
            .write_all(GET_HEALTH)
            .await
            .expect("second request");
        assert!(response(&mut keep_alive).await.starts_with("http/1.1 200"));
        serves_again(addr).await;
        handle.stop().expect("stop");
        handle.stopped().await;
    }

    #[tokio::test(start_paused = true)]
    async fn the_idle_deadline_moves_from_the_first_request_to_the_keep_alive_timeout() {
        let timeouts = IdleTimeouts {
            first_request: Duration::from_secs(10),
            keep_alive: Duration::from_secs(60),
        };
        let cut_within = |idle, wait: u64| async move {
            tokio::time::timeout(Duration::from_secs(wait), wait_idle_deadline(idle))
                .await
                .is_ok()
        };

        // 1. No request, the socket ends at the first-request timeout and no sooner.
        let (_activity, idle) = ConnectionActivity::new(timeouts);
        assert!(!cut_within(idle.clone(), 9).await);
        assert!(cut_within(idle, 1).await);

        // 2. A request in flight holds the connection past both timeouts.
        let (activity, idle) = ConnectionActivity::new(timeouts);
        let in_flight = activity.enter();
        assert!(!cut_within(idle.clone(), 3_600).await);

        // 3. Past its response the connection idles for the keep-alive timeout.
        drop(in_flight);
        assert!(!cut_within(idle.clone(), 59).await);
        assert!(cut_within(idle.clone(), 1).await);

        // 4. Each response starts the keep-alive timeout again.
        let (activity, idle) = ConnectionActivity::new(timeouts);
        drop(activity.enter());
        assert!(!cut_within(idle.clone(), 59).await);
        drop(activity.enter());
        assert!(!cut_within(idle.clone(), 59).await);
        assert!(cut_within(idle, 1).await);
    }
}
