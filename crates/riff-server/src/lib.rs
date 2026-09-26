//! `riff-server`: the central service that sessions connect to.
//!
//! # Design
//!
//! ```text
//!  HTTP handlers ──lock──▶ State (Mutex)       see [`state`]
//!        │
//!        ├─ Delivery ──▶ wakes channel ──▶ GET /v1/watch streams
//!        │           └─▶ tail channel  ──▶ GET /v1/tail streams
//!        └──lock──▶ Tokens (Mutex)      see [`token`]
//! ```
//!
//! - One process holds all state in memory, behind one mutex. Each
//!   handler holds the lock for a short time and does no I/O under it.
//! - [`state::State`] does not know about HTTP or clocks. Handlers pass
//!   the time in, so tests control it.
//! - A post returns a [`state::Delivery`]. The handler sends its wakes
//!   and its tail event to two broadcast channels. Each open stream
//!   filters the channel for its own session or thread.
//! - A watch stream starts with the wake from [`state::State::missed`],
//!   if there is one. It subscribes to the wakes channel first, so no
//!   wake falls in the gap.
//! - A watch stream owns a guard. When the stream closes, the guard marks
//!   the session as stopped. That starts the claim grace period.
//! - A stream that falls more than 1024 events behind skips the events
//!   that it missed. A skipped wake is lost; the message stays in its
//!   thread.
//!
//! - `POST /v1/token` swaps a refresh token for a new pair. The token
//!   store has its own lock, so a refresh never waits for the state.
//! - `POST /v1/revoke` ends each sign-in of a person. The admins are a
//!   setting ([`auth::Config::admins`]).
//! - Each route with a `me` acts only as the [`auth::SignedIn`] caller
//!   of its token: the same user and the same session ID, or 403
//!   (R104). A person token acts only as the person. A session token
//!   acts only as its session.
//! - A token exchange also swaps a person access token for a session
//!   pair (R19, see [`token`]).
//! - A layer checks the access token and its DPoP proof. It guards
//!   `/v1/revoke` always, and each other `/v1` route except `/v1/token`
//!   with [`auth::Config::require_sign_in`]. It puts the
//!   [`auth::SignedIn`] user in the request. See [`auth`] for the OAuth
//!   rules.
//! - `POST /v1/token` also swaps an ID token of the sign-in provider for
//!   a first pair (see [`oidc`]). `GET /v1/sign-in` names the provider.
//!   A server with no provider ([`auth::Config::provider`]) refuses both.
//!   Each grant needs a DPoP proof; the first pair binds the sign-in to
//!   its key.
//!
//! The server keeps state only in memory. The wire protocol is in
//! [`riff_core::wire`].
//!
//! # Example
//!
//! ```no_run
//! # async fn run() -> std::io::Result<()> {
//! let listener = tokio::net::TcpListener::bind("127.0.0.1:7878").await?;
//! axum::serve(listener, riff_server::router()).await
//! # }
//! ```

pub mod auth;
pub mod oidc;
pub mod state;
pub mod token;

use std::convert::Infallible;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{Form, Query, Request, State as AxumState};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use futures::{Stream, StreamExt};
use riff_core::dpop;
use riff_core::name::{SessionUri, ThreadName, Who};
use riff_core::wire::{
    ACCESS_TOKEN_TYPE, Claim, ClaimReply, ID_TOKEN_TYPE, Membership, Post, Posted, Read, ReadReply,
    Register, ResourceMetadata, Revoke, Revoked, ServerMetadata, SignInConfig, TOKEN_EXCHANGE,
    Tailed, Threads, ThreadsReply, TokenError, TokenReply, TokenRequest, Wake, WhoReply,
    WhoRequest,
};
use serde::Deserialize;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::{Config, Refusal, Replay, SignedIn};
use crate::state::{Delivery, State};
use crate::token::Tokens;

/// Events that a slow stream may miss before it drops them.
const EVENT_BUFFER: usize = 1024;

type Shared = Arc<Server>;
type Reply<T> = Result<Json<T>, (StatusCode, String)>;

struct Server {
    config: Config,
    state: Mutex<State>,
    tokens: Mutex<Tokens>,
    replay: Mutex<Replay>,
    wakes: broadcast::Sender<(Who, Wake)>,
    tail: broadcast::Sender<Tailed>,
    http: reqwest::Client,
}

impl Server {
    fn state(&self) -> MutexGuard<'_, State> {
        // A panic while the lock is held leaves plain data behind; keep going.
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn tokens(&self) -> MutexGuard<'_, Tokens> {
        self.tokens
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Checks the `DPoP` header of a request to `path` (R18). `token` is
    /// the access token that the request carries, if any. The caller
    /// checks the credential of the request, and then calls
    /// [`Server::first_use`].
    fn proof(
        &self,
        headers: &HeaderMap,
        method: &str,
        path: &str,
        token: Option<&str>,
    ) -> Result<dpop::Proof, Refusal> {
        let mut values = headers.get_all("dpop").iter();
        let (Some(value), None) = (values.next(), values.next()) else {
            return Err(Refusal::proof("send one DPoP header"));
        };
        let value = value.to_str().map_err(Refusal::proof)?;
        let now = now_ms() / 1000;
        let url = self.config.url(path);
        dpop::verify(value, method, &url, token, now).map_err(Refusal::proof)
    }

    /// Refuses a proof that came before. Only a request with a valid
    /// credential gets here, so a caller without one cannot fill the
    /// store (R115).
    fn first_use(&self, proof: &dpop::Proof) -> Result<(), Refusal> {
        let now = now_ms() / 1000;
        if !self.replay().first_use(&proof.jti, proof.iat, now) {
            return Err(Refusal::proof("the proof was used before"));
        }
        Ok(())
    }

    /// Finds the user of a request from its access token and its proof.
    /// `Err(None)` means that the request has no token.
    fn authenticate(
        &self,
        headers: &HeaderMap,
        method: &str,
        path: &str,
    ) -> Result<SignedIn, Option<Refusal>> {
        let token = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(auth::dpop_token)
            .ok_or(None)?;
        let proof = self.proof(headers, method, path, Some(token))?;
        let who = self
            .tokens()
            .caller(token, &proof.jkt, Instant::now())
            .map_err(Refusal::token)?;
        self.first_use(&proof)?;
        Ok(SignedIn(who))
    }

    fn replay(&self) -> MutexGuard<'_, Replay> {
        self.replay
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn deliver(&self, delivery: Delivery) {
        // A send fails only when nobody listens. That is not an error.
        for wake in delivery.wakes {
            let _ = self.wakes.send(wake);
        }
        let _ = self.tail.send(delivery.tailed);
    }
}

/// One `riff-server`: its state, its tokens and its routes. Clones
/// share the same server.
///
/// ```
/// use std::time::Instant;
/// use riff_server::Service;
///
/// let service = Service::default();
/// assert!(!service.config().require_sign_in);
/// let pair = service.tokens().sign_in("mike", "jkt", Instant::now()).unwrap();
/// let router = service.router();
/// # let _ = (pair, router);
/// ```
#[derive(Clone)]
pub struct Service(Shared);

impl Default for Service {
    fn default() -> Self {
        Service::new(Config::default())
    }
}

impl Service {
    /// A new server with these settings.
    pub fn new(config: Config) -> Self {
        let (wakes, _) = broadcast::channel(EVENT_BUFFER);
        let (tail, _) = broadcast::channel(EVENT_BUFFER);
        Service(Arc::new(Server {
            config,
            state: Mutex::new(State::default()),
            tokens: Mutex::new(Tokens::default()),
            replay: Mutex::new(Replay::default()),
            wakes,
            tail,
            http: oidc::client(oidc::FETCH_TIMEOUT),
        }))
    }

    /// The HTTP routes of this server.
    pub fn router(&self) -> Router {
        let mut routes = Router::new()
            .route("/v1/register", post(register))
            .route("/v1/who", post(who))
            .route("/v1/threads", post(threads))
            .route("/v1/join", post(join))
            .route("/v1/leave", post(leave))
            .route("/v1/post", post(post_message))
            .route("/v1/read", post(read))
            .route("/v1/claim", post(claim))
            .route("/v1/release", post(release))
            .route("/v1/watch", get(watch))
            .route("/v1/tail", get(tail_thread));
        let guard = || middleware::from_fn_with_state(self.0.clone(), require_token);
        if self.0.config.require_sign_in {
            routes = routes.route_layer(guard());
        }
        let revoke = Router::new()
            .route("/v1/revoke", post(revoke))
            .route_layer(guard());
        routes
            .merge(revoke)
            .route(auth::TOKEN_PATH, post(token))
            .route("/v1/sign-in", get(sign_in_config))
            .route(auth::RESOURCE_METADATA_PATH, get(resource_metadata))
            .route(auth::SERVER_METADATA_PATH, get(server_metadata))
            .with_state(self.0.clone())
    }

    /// The settings of this server.
    pub fn config(&self) -> &Config {
        &self.0.config
    }

    /// The token store of this server.
    pub fn tokens(&self) -> MutexGuard<'_, Tokens> {
        self.0.tokens()
    }

    /// The number of proof IDs that this server keeps (R114).
    pub fn proofs_kept(&self) -> usize {
        self.0.replay().len()
    }
}

/// The HTTP routes of a new `riff-server`.
pub fn router() -> Router {
    Service::default().router()
}

async fn register(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Register>,
) -> Reply<()> {
    acts_as(caller, &r.me)?;
    s.state().register(&r.me, Instant::now());
    Ok(Json(()))
}

async fn who(AxumState(s): AxumState<Shared>, Json(_): Json<WhoRequest>) -> Reply<WhoReply> {
    Ok(Json(WhoReply {
        sessions: s.state().who(),
    }))
}

async fn threads(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Threads>,
) -> Reply<ThreadsReply> {
    acts_as(caller, &r.me)?;
    Ok(Json(ThreadsReply {
        threads: s.state().threads(&r.me, Instant::now()),
    }))
}

async fn join(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Membership>,
) -> Reply<()> {
    acts_as(caller, &r.me)?;
    s.state().join(&r.me, &r.thread, Instant::now());
    Ok(Json(()))
}

async fn leave(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Membership>,
) -> Reply<()> {
    acts_as(caller, &r.me)?;
    s.state().leave(&r.me, &r.thread, Instant::now());
    Ok(Json(()))
}

async fn post_message(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Post>,
) -> Reply<Posted> {
    acts_as(caller, &r.me)?;
    let delivery = s
        .state()
        .post(&r.me, r.thread, r.to, r.body, Instant::now(), now_ms())
        .map_err(bad_request)?;
    Ok(Json(posted(&s, delivery)))
}

async fn read(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Read>,
) -> Reply<ReadReply> {
    acts_as(caller, &r.me)?;
    let messages = s
        .state()
        .read(&r.me, &r.thread, r.all, Instant::now())
        .map_err(not_found)?;
    Ok(Json(ReadReply { messages }))
}

async fn claim(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Claim>,
) -> Reply<ClaimReply> {
    acts_as(caller, &r.me)?;
    let reply = s
        .state()
        .claim(&r.me, &r.thread, &r.item, Instant::now())
        .map_err(bad_request)?;
    Ok(Json(reply))
}

async fn release(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Claim>,
) -> Reply<()> {
    acts_as(caller, &r.me)?;
    s.state()
        .release(&r.me, &r.thread, &r.item, Instant::now())
        .map_err(bad_request)?;
    Ok(Json(()))
}

/// The OAuth 2.1 token endpoint: swaps a refresh token for a new pair.
async fn token(
    AxumState(s): AxumState<Shared>,
    headers: HeaderMap,
    Form(r): Form<TokenRequest>,
) -> impl IntoResponse {
    let no_store = || [(header::CACHE_CONTROL, "no-store")];
    let refuse = |error: &str| {
        let error = TokenError {
            error: error.into(),
        };
        (StatusCode::BAD_REQUEST, no_store(), Json(error)).into_response()
    };
    if r.resource
        .as_ref()
        .is_some_and(|resource| !s.config.is_resource(resource))
    {
        return refuse("invalid_target");
    }
    let Ok(proof) = s.proof(&headers, "POST", auth::TOKEN_PATH, None) else {
        return refuse("invalid_dpop_proof");
    };
    let reply = match r.grant_type.as_str() {
        "refresh_token" => refresh(&s, &r, &proof),
        TOKEN_EXCHANGE => match r.subject_token_type.as_deref() {
            Some(ID_TOKEN_TYPE) => exchange(&s, &r, &proof).await,
            Some(ACCESS_TOKEN_TYPE) => for_session(&s, &r, &proof),
            _ => Err("invalid_request"),
        },
        _ => Err("unsupported_grant_type"),
    };
    match reply {
        Ok(pair) => (no_store(), Json(pair)).into_response(),
        Err(error) => refuse(error),
    }
}

/// Ends each sign-in of a person. The caller is the user of the access
/// token. Only an admin names another person.
async fn revoke(
    AxumState(s): AxumState<Shared>,
    Extension(SignedIn(caller)): Extension<SignedIn>,
    Json(r): Json<Revoke>,
) -> Reply<Revoked> {
    let caller = caller.user().to_owned();
    // Names compare trimmed and in lower case, as at sign-in (R111).
    let user = r
        .user
        .map_or_else(|| caller.clone(), |u| u.trim().to_lowercase());
    let admin = s
        .config
        .admins
        .iter()
        .any(|a| a.trim().to_lowercase() == caller);
    if user != caller && !admin {
        return Err((
            StatusCode::FORBIDDEN,
            format!("{caller} is not an admin; only an admin revokes another person"),
        ));
    }
    let sign_ins = s.tokens().revoke_user(&user);
    tracing::info!(%caller, %user, sign_ins, "revoked");
    Ok(Json(Revoked { user, sign_ins }))
}

/// Lets a request through only with a live access token in the
/// `Authorization` header and a proof from its device key. Else it
/// replies 401 with a challenge.
async fn require_token(
    AxumState(s): AxumState<Shared>,
    mut request: Request,
    next: Next,
) -> Response {
    let method = request.method().as_str().to_owned();
    let path = request.uri().path().to_owned();
    let checked = s.authenticate(request.headers(), &method, &path);
    let challenge = match checked {
        Ok(user) => {
            request.extensions_mut().insert(user);
            return next.run(request).await;
        }
        Err(refusal) => s.config.challenge(refusal.as_ref()),
    };
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, challenge)],
    )
        .into_response()
}

async fn resource_metadata(AxumState(s): AxumState<Shared>) -> Json<ResourceMetadata> {
    Json(s.config.resource_metadata())
}

async fn server_metadata(AxumState(s): AxumState<Shared>) -> Json<ServerMetadata> {
    Json(s.config.server_metadata())
}

/// Swaps a refresh token for a new pair.
fn refresh(s: &Server, r: &TokenRequest, proof: &dpop::Proof) -> Result<TokenReply, &'static str> {
    let token = r.refresh_token.as_deref().unwrap_or_default();
    if !s.tokens().knows_refresh(token, &proof.jkt) {
        return Err("invalid_grant");
    }
    s.first_use(proof).map_err(|_| "invalid_dpop_proof")?;
    s.tokens()
        .refresh(token, &proof.jkt, Instant::now())
        .map_err(|_| "invalid_grant")
}

/// Swaps an ID token of the provider for a first pair of riff tokens.
async fn exchange(
    s: &Server,
    r: &TokenRequest,
    proof: &dpop::Proof,
) -> Result<TokenReply, &'static str> {
    let provider = s.config.provider.as_ref().ok_or("unsupported_grant_type")?;
    let id_token = r.subject_token.as_ref().ok_or("invalid_request")?;
    let identity = provider.sign_in(&s.http, id_token).await.map_err(|e| {
        tracing::info!("sign-in refused: {e}");
        "invalid_grant"
    })?;
    s.first_use(proof).map_err(|_| "invalid_dpop_proof")?;
    tracing::info!("{} signed in as {}", identity.email, identity.user);
    s.tokens()
        .sign_in(&identity.user, &proof.jkt, Instant::now())
        .map_err(|_| "invalid_grant")
}

/// Swaps a person access token for a session pair (R19).
fn for_session(
    s: &Server,
    r: &TokenRequest,
    proof: &dpop::Proof,
) -> Result<TokenReply, &'static str> {
    let (Some(token), Some(session)) = (&r.subject_token, &r.session) else {
        return Err("invalid_request");
    };
    let now = Instant::now();
    if s.tokens().caller(token, &proof.jkt, now).is_err() {
        return Err("invalid_grant");
    }
    s.first_use(proof).map_err(|_| "invalid_dpop_proof")?;
    s.tokens()
        .for_session(token, &proof.jkt, session, now)
        .map_err(|_| "invalid_grant")
}

/// Refuses a request that acts as another user or session than its
/// token (R104). Without a token check, each request passes.
fn acts_as(
    caller: Option<Extension<SignedIn>>,
    me: &SessionUri,
) -> Result<(), (StatusCode, String)> {
    match caller {
        Some(Extension(caller)) => caller
            .may_act_as(me.who())
            .map_err(|message| (StatusCode::FORBIDDEN, message)),
        None => Ok(()),
    }
}

/// Names the sign-in provider, for `riff login`.
async fn sign_in_config(AxumState(s): AxumState<Shared>) -> Reply<SignInConfig> {
    match &s.config.provider {
        Some(provider) => Ok(Json(provider.config())),
        None => Err(not_found("this riff-server has no sign-in provider".into())),
    }
}

#[derive(Deserialize)]
struct WatchQuery {
    uri: SessionUri,
}

/// Streams the wakes for one session. The session is live while the
/// stream is open.
async fn watch(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Query(q): Query<WatchQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    acts_as(caller, &q.uri)?;
    let rx = s.wakes.subscribe();
    let missed = {
        let mut state = s.state();
        let now = Instant::now();
        state.watch_started(&q.uri, now);
        state.missed(q.uri.who())
    };
    let guard = WatchGuard {
        server: s.clone(),
        who: q.uri.who().clone(),
    };
    let live = BroadcastStream::new(rx).filter_map(move |event| {
        let _alive = &guard;
        let wake = match event {
            Ok((to, wake)) if to == guard.who => Some(wake),
            _ => None,
        };
        std::future::ready(wake)
    });
    let stream = futures::stream::iter(missed)
        .chain(live)
        .filter_map(|wake| std::future::ready(Event::default().json_data(wake).ok().map(Ok)));
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Marks the session as stopped when its watch stream closes.
struct WatchGuard {
    server: Shared,
    who: Who,
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        self.server.state().watch_ended(&self.who, Instant::now());
    }
}

#[derive(Deserialize)]
struct TailQuery {
    thread: ThreadName,
}

/// Streams each new message in one thread (R27).
async fn tail_thread(
    AxumState(s): AxumState<Shared>,
    Query(q): Query<TailQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(s.tail.subscribe()).filter_map(move |event| {
        let event = match event {
            Ok(tailed) if tailed.thread == q.thread => Event::default().json_data(tailed).ok(),
            _ => None,
        };
        std::future::ready(event.map(Ok))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn posted(s: &Server, delivery: Delivery) -> Posted {
    let reply = Posted {
        thread: delivery.tailed.thread.clone(),
        seq: delivery.tailed.message.seq,
        woken: delivery.woken.clone(),
        unmatched: delivery.unmatched.clone(),
    };
    s.deliver(delivery);
    reply
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn bad_request(message: String) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message)
}

fn not_found(message: String) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, message)
}
