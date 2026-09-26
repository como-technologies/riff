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
//! - `POST /v1/revoke` ends each sign-in of a person. It checks the
//!   token itself. The admins are a setting ([`auth::Config::admins`]).
//! - With [`auth::Config::require_sign_in`], a layer checks the access
//!   token of each other `/v1` route. See [`auth`] for the OAuth rules.
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
use axum::{Json, Router};
use futures::{Stream, StreamExt};
use riff_core::name::{SessionUri, ThreadName, Who};
use riff_core::wire::{
    Claim, ClaimReply, Membership, Post, Posted, Read, ReadReply, Register, ResourceMetadata,
    Revoke, Revoked, ServerMetadata, Tailed, Threads, ThreadsReply, TokenError, TokenRequest, Wake,
    WhoReply, WhoRequest,
};
use serde::Deserialize;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::{Config, SignedIn};
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
    wakes: broadcast::Sender<(Who, Wake)>,
    tail: broadcast::Sender<Tailed>,
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
/// let pair = service.tokens().sign_in("mike", Instant::now()).unwrap();
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
            wakes,
            tail,
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
        if self.0.config.require_sign_in {
            routes = routes.route_layer(middleware::from_fn_with_state(
                self.0.clone(),
                require_token,
            ));
        }
        routes
            .route(auth::TOKEN_PATH, post(token))
            .route("/v1/revoke", post(revoke))
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
}

/// The HTTP routes of a new `riff-server`.
pub fn router() -> Router {
    Service::default().router()
}

async fn register(AxumState(s): AxumState<Shared>, Json(r): Json<Register>) -> Reply<()> {
    s.state().register(&r.me, Instant::now());
    Ok(Json(()))
}

async fn who(AxumState(s): AxumState<Shared>, Json(_): Json<WhoRequest>) -> Reply<WhoReply> {
    Ok(Json(WhoReply {
        sessions: s.state().who(),
    }))
}

async fn threads(AxumState(s): AxumState<Shared>, Json(r): Json<Threads>) -> Reply<ThreadsReply> {
    Ok(Json(ThreadsReply {
        threads: s.state().threads(&r.me, Instant::now()),
    }))
}

async fn join(AxumState(s): AxumState<Shared>, Json(r): Json<Membership>) -> Reply<()> {
    s.state().join(&r.me, &r.thread, Instant::now());
    Ok(Json(()))
}

async fn leave(AxumState(s): AxumState<Shared>, Json(r): Json<Membership>) -> Reply<()> {
    s.state().leave(&r.me, &r.thread, Instant::now());
    Ok(Json(()))
}

async fn post_message(AxumState(s): AxumState<Shared>, Json(r): Json<Post>) -> Reply<Posted> {
    let delivery = s
        .state()
        .post(&r.me, r.thread, r.to, r.body, Instant::now(), now_ms())
        .map_err(bad_request)?;
    Ok(Json(posted(&s, delivery)))
}

async fn read(AxumState(s): AxumState<Shared>, Json(r): Json<Read>) -> Reply<ReadReply> {
    let messages = s
        .state()
        .read(&r.me, &r.thread, r.all, Instant::now())
        .map_err(not_found)?;
    Ok(Json(ReadReply { messages }))
}

async fn claim(AxumState(s): AxumState<Shared>, Json(r): Json<Claim>) -> Reply<ClaimReply> {
    let reply = s
        .state()
        .claim(&r.me, &r.thread, &r.item, Instant::now())
        .map_err(bad_request)?;
    Ok(Json(reply))
}

async fn release(AxumState(s): AxumState<Shared>, Json(r): Json<Claim>) -> Reply<()> {
    s.state()
        .release(&r.me, &r.thread, &r.item, Instant::now())
        .map_err(bad_request)?;
    Ok(Json(()))
}

/// The OAuth 2.1 token endpoint: swaps a refresh token for a new pair.
async fn token(AxumState(s): AxumState<Shared>, Form(r): Form<TokenRequest>) -> impl IntoResponse {
    let no_store = || [(header::CACHE_CONTROL, "no-store")];
    let refuse = |error: &str| {
        let error = TokenError {
            error: error.into(),
        };
        (StatusCode::BAD_REQUEST, no_store(), Json(error)).into_response()
    };
    if r.grant_type != "refresh_token" {
        return refuse("unsupported_grant_type");
    }
    if r.resource
        .is_some_and(|resource| !s.config.is_resource(&resource))
    {
        return refuse("invalid_target");
    }
    match s.tokens().refresh(&r.refresh_token, Instant::now()) {
        Ok(pair) => (no_store(), Json(pair)).into_response(),
        Err(_) => refuse("invalid_grant"),
    }
}

/// Ends each sign-in of a person. The caller is the user of the bearer
/// token. Only an admin names another person.
async fn revoke(
    AxumState(s): AxumState<Shared>,
    headers: HeaderMap,
    Json(r): Json<Revoke>,
) -> Reply<Revoked> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(auth::bearer)
        .ok_or((StatusCode::UNAUTHORIZED, "send a bearer token".to_owned()))?;
    let mut tokens = s.tokens();
    let caller = tokens
        .check(token, Instant::now())
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?
        .to_owned();
    let user = r.user.unwrap_or_else(|| caller.clone());
    if user != caller && !s.config.admins.contains(&caller) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("{caller} is not an admin; only an admin revokes another person"),
        ));
    }
    let sign_ins = tokens.revoke_user(&user);
    tracing::info!(%caller, %user, sign_ins, "revoked");
    Ok(Json(Revoked { user, sign_ins }))
}

/// Lets a request through only with a live access token in the
/// `Authorization` header. Else it replies 401 with a challenge.
async fn require_token(
    AxumState(s): AxumState<Shared>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(auth::bearer);
    let checked = token.map(|token| {
        s.tokens()
            .check(token, Instant::now())
            .map(|user| SignedIn(user.to_owned()))
    });
    let challenge = match checked {
        Some(Ok(user)) => {
            request.extensions_mut().insert(user);
            return next.run(request).await;
        }
        Some(Err(refused)) => s.config.challenge(Some(&refused.to_string())),
        None => s.config.challenge(None),
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

#[derive(Deserialize)]
struct WatchQuery {
    uri: SessionUri,
}

/// Streams the wakes for one session. The session is live while the
/// stream is open.
async fn watch(
    AxumState(s): AxumState<Shared>,
    Query(q): Query<WatchQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
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
    Sse::new(stream).keep_alive(KeepAlive::default())
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
