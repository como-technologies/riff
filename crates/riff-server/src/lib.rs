//! `riff-server`: the central service that sessions connect to.
//!
//! # Design
//!
//! ```text
//!  HTTP handlers ──▶ Engine ──lock──▶ State    see [`engine`], [`state`]
//!        │             │
//!        │             └─ the writer ──▶ wakes channel ──▶ GET /v1/watch streams
//!        │                           └─▶ tail channel  ──▶ GET /v1/tail streams
//!        └──lock──▶ Tokens (Mutex)      see [`token`]
//! ```
//!
//! - One process holds all state in memory, behind one mutex. The
//!   [`engine::Engine`] owns the state and its lock: no handler locks
//!   the state (01M3WRD8WJ2JF9077PRDX04T9A). The engine holds the lock
//!   for a short time and does no I/O under it.
//! - Each call that changes the state of the log is a command. One
//!   handler, [`engine::command`], serves each, and
//!   [`engine::Engine::dispatch`] is its one path. A call that changes
//!   only the presence is a signal ([`engine::Engine::signal`]). Each
//!   other call is a query: it reads the written copy.
//! - [`state::State`] does not know about HTTP or clocks. The engine
//!   passes the time in, so tests control it.
//! - The writer sends the wakes and the `tail` event of a message to
//!   two broadcast channels, after the write of its record. Each open
//!   stream filters the channel for its own session or thread.
//! - Each stream starts with the comment `: ready`, see [`opened`].
//! - A watch stream then gives the wake from [`state::State::missed`],
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
//! - `POST /v1/revoke` ends each sign-in of a person. The admins are
//!   the owner and a setting ([`auth::Config::admins`]). Each admin is
//!   named by verified email (R210).
//! - `POST /v1/invite`, `/v1/remove` and `/v1/members` change and show
//!   who may join the riff. `POST /v1/admin` lets the owner make a
//!   person an admin, or an admin a member again. `POST /v1/owner`
//!   lets the owner pass the owner role. `POST /v1/owner/take` lets an
//!   admin ask for it, and `POST /v1/owner/deny` lets the owner keep
//!   it. See "Owner and members" in [`token`].
//! - A task looks for idle workers each [`idle::CHECK_EVERY`], and asks
//!   each idle worker past the limit to stop. See [`idle`].
//! - A task looks at the owner role each [`owner::Timing::tick`]: it
//!   grants a request whose time ended, and checks the owner. The server
//!   posts the note of each change of the role itself. See [`owner`].
//! - Each route with a `me` acts only as the [`auth::SignedIn`] caller
//!   of its token: the same user and the same session ID, or 403
//!   (R104). A person token acts only as the person. A session token
//!   acts only as its session.
//! - A token exchange also swaps a person access token for a session
//!   pair (R19, see [`token`]).
//! - With sign-in, a post needs a signature from the device key of its
//!   token ([`auth::SignedIn::check_post`]). The message keeps the
//!   signature. `read` and each `tail` event give the keys of the live
//!   sign-ins of each sender ([`token::Tokens::keys`]), so the reader
//!   verifies each message (see [`riff_core::signed`]). Without sign-in,
//!   the server keeps no signature and gives no keys (R201).
//! - A riff with no sign-in ([`auth::Config::trusted`]) marks
//!   each `read` reply and each `tail` event as trusted. Its reader
//!   counts each of its messages as verified (R211, R212).
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
//! - A riff with no sign-in listens only on a loopback address, unless
//!   it gets `--insecure`. See [`listen`].
//!
//! - Each change of the state is a record in the log (see [`state`] and
//!   [`log`]). A command puts its entry in the queue of the engine, and
//!   gets its reply after the writer is done with the entry. Its wakes
//!   and its `tail` events go out after the write too. A command that
//!   makes no record, and a command that is refused, wait as each
//!   command does (01M3WRD933ESXF33WDEDFCRFB8).
//! - The writer is one task. It takes each entry in the queue into one
//!   chunk, writes it outside the lock of the state, and then the engine
//!   finishes each command of the chunk
//!   ([`engine::Engine::finish`]). When a write fails for good, the
//!   server stops for good: each waiting call gets 503, and `main`
//!   exits.
//! - The first start of a riff sends the command `make_riff`
//!   (01M3WRD99M99PNGP8ME50KC6WS). A store with the objects of a
//!   riff-server from before the log, and no log, is refused
//!   (01M3WRD9DYJWVN1QRBAC3ZVVZD).
//! - [`Service::load`] loads the newest checkpoint of a [`store::Store`],
//!   and replays the log after it (R30). A timer writes a checkpoint, and
//!   deletes the old checkpoints and the chunks that no kept checkpoint
//!   needs (see [`checkpoint`]).
//! - A timer forgets each session with no sign of life for
//!   [`state::SESSION_EXPIRY`], each [`FORGET_EVERY`].
//!   [`Service::new`] keeps its log in memory, and saves nothing (R34).
//!   [`Service::save`] waits until the queue is written, and saves the
//!   token store; `main` calls it on SIGTERM (R129).
//! - The token store is the object [`store::SIGN_INS`]. The server
//!   writes it when it changed, at most one time each
//!   [`auth::Config::save_every`] (R127). The server knows the
//!   [`store::Version`] of the object. Each write names it, so a write
//!   over the changes of another instance fails (R141). A sign-in, a
//!   revoke and a change of the people get their reply only after the
//!   write (R128). When that write fails, the reply is 503, and a task
//!   writes the store again.
//! - A refresh and a new session token get their reply before the write
//!   (01M3TFG527M04TA7ESM970X3B8). A session token changes nothing in
//!   the object (01M3WFVAB44T8EP4QZD4KS7DRF). So after a crash, the object can be
//!   one generation behind, and [`token::Tokens::refresh`] takes the next
//!   generation as good. While the last write failed, a refresh first
//!   writes the store again, and gets 503 when that write fails too.
//! - A server with a store loads first, then takes the [`lease`], and
//!   keeps reading it (see [`Service::load`]). `main` opens the port
//!   only after that ([`listen::Port`]). A gate replies 503 to each call
//!   while the server does not serve (R139). The server saves only while
//!   it holds the lease (R155).
//! - A server that reads another ID in the lease, or whose save finds
//!   another version, or whose chunk has the name of another chunk, stops
//!   for good (R140, R141): each stream closes, each call gets 503, and
//!   it saves nothing more.
//!   [`Service::stopped`] tells `main`, which exits after
//!   [`lease::Timing::exit_after`].
//! - A server refuses each proof issued before it started to serve
//!   (R142).
//! - `GET /v1/server` gives the facts of the instance, for `riff server`
//!   (01M3TJWJ12WEDCXW3W0529KRP2): if it serves, its last error, the
//!   log, the checkpoint, its sizes and its start. The route is outside
//!   the gate and the build check, so it answers also while each other
//!   call gets 503, and to a `riff` of each version. The server counts
//!   the facts that the state does not hold: the writer counts the
//!   chunks and the failed tries, and each error that the server logs
//!   stays as the last error.
//! - Each log line is JSON with a `severity` (see [`logline`]). The
//!   tools of the log run with no server (see [`tools`]).
//!
//! The wire protocol is in [`riff_core::wire`].
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
pub mod checkpoint;
pub mod engine;
pub mod gcs;
pub mod idle;
pub mod lease;
pub mod listen;
pub mod log;
pub mod logline;
pub mod oidc;
pub mod owner;
pub mod state;
pub mod store;
pub mod token;
pub mod tools;
pub mod trace;

use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{Form, FromRef, FromRequestParts, Query, Request, State as AxumState};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use futures::{Stream, StreamExt};
use riff_core::build::{self, Build, Mismatch};
use riff_core::dpop;
use riff_core::name::{SessionUri, ThreadName, Who};
use riff_core::selector::Selector;
use riff_core::wire::{
    ACCESS_TOKEN_TYPE, AdminSet, Alive, AliveReply, Call, CheckpointFacts, Claim, DenyOwner, End,
    FactError, ID_TOKEN_TYPE, Idle, IdleQuery, Invite, Invited, Join, Keys, Kind, Lead, Leave,
    MeReply, Members, MembersReply, OwnerAsked, OwnerDenied, OwnerPassed, PassOwner, Pause, Person,
    Post, Read, ReadReply, Register, Release, ReleaseFor, Remove, Removed, ResourceMetadata,
    Resume, Revoke, Revoked, RiffOwner, RiffQuery, RiffReply, ServerFacts, ServerMetadata,
    SetAdmin, SetIdle, SetStatus, SignInConfig, Start, TOKEN_EXCHANGE, TakeOwner, Threads,
    ThreadsReply, TokenError, TokenReply, TokenRequest, WhoReply, WhoRequest,
};
use serde::Deserialize;
use tokio::time::MissedTickBehavior;
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::{Config, Refusal, Replay, SignedIn};
use crate::engine::{Admitted, Engine, Failed, People, command};
use crate::lease::Lease;
use crate::owner::{Check, Checks};
use crate::state::{Announce, Role, Signal, State, may_read};
use crate::store::{Memory, SIGN_INS, Store, StoreError, Version};
use crate::token::{NO_OWNER, OwnerChange, Tokens, Took};
use crate::trace::{DeniedCode, Named};

/// The least time between two writes of the token store (R127): the
/// default of [`auth::Config::save_every`].
pub const SAVE_EVERY: Duration = Duration::from_secs(1);

/// The server looks for sessions to forget this often
/// ([`state::State::forget_expired`]).
pub const FORGET_EVERY: Duration = Duration::from_secs(60 * 60);

type Shared = Arc<Server>;
type Reply<T> = Result<Json<T>, (StatusCode, String)>;

/// Loads the token store, with the version of its object. A store with
/// no token store gives an empty one.
async fn load_tokens(store: &dyn Store) -> Result<(Tokens, Option<Version>), StoreError> {
    let Some(loaded) = store.load(SIGN_INS).await? else {
        return Ok((Tokens::default(), None));
    };
    let tokens = Tokens::from_bytes(&loaded.bytes, Instant::now(), SystemTime::now())
        .map_err(|e| StoreError::not_valid(store, SIGN_INS, e))?;
    Ok((tokens, Some(loaded.version)))
}

/// Refuses a store of a riff-server from before the log
/// (01M3WRD9DYJWVN1QRBAC3ZVVZD): it has the old objects `sessions`,
/// `tokens` or `threads/`, and no log. This build does not read them:
/// go-live (#341) imports them.
async fn refuse_old_objects(store: &dyn Store) -> Result<(), StoreError> {
    let has_log = !store.list(log::LOG).await?.is_empty()
        || !store.list(checkpoint::CHECKPOINT).await?.is_empty();
    if has_log {
        return Ok(());
    }
    let mut old = Vec::new();
    for name in ["sessions", "tokens"] {
        if store.load(name).await?.is_some() {
            old.push(format!("`{name}`"));
        }
    }
    if !store.list("threads/").await?.is_empty() {
        old.push("`threads/`".to_owned());
    }
    if old.is_empty() {
        return Ok(());
    }
    Err(StoreError::Failed(format!(
        "the store has the objects of a riff-server from before the log ({}), and no log. \
         This build does not import them, so it does not start. Use the release that \
         imports them, or an empty store.",
        old.join(", ")
    )))
}

/// The roles and the keys of the people, for the engine. Until E3
/// (#393), they come from the token store.
struct Roles {
    tokens: Arc<Mutex<Tokens>>,
    /// The admins of the settings (R210).
    admins: Vec<String>,
    needs_sign_in: bool,
    trusted: bool,
}

impl Roles {
    fn tokens(&self) -> MutexGuard<'_, Tokens> {
        self.tokens
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

impl People for Roles {
    fn needs_sign_in(&self) -> bool {
        self.needs_sign_in
    }

    fn trusted(&self) -> bool {
        self.trusted
    }

    fn role(&self, user: &str) -> Role {
        if self.trusted {
            return Role::Admin;
        }
        let tokens = self.tokens();
        if tokens.is_owner(user) {
            Role::Owner
        } else if tokens.is_admin(user, &self.admins) {
            Role::Admin
        } else {
            Role::Member
        }
    }

    fn keys(&self, user: &str) -> Vec<String> {
        self.tokens().keys(user, Instant::now())
    }
}

struct Server {
    config: Config,
    /// The command engine. It owns the state and its lock.
    engine: Engine,
    tokens: Arc<Mutex<Tokens>>,
    /// The number of changes to the token store.
    tokens_changes: AtomicU64,
    /// The number of changes to the token store that are saved.
    tokens_saved: AtomicU64,
    /// True while the last write of the token store failed
    /// (01M3TFG527M04TA7ESM970X3B8).
    tokens_failed: AtomicBool,
    replay: Mutex<Replay>,
    http: reqwest::Client,
    saved: Option<Saved>,
    gate: Gate,
    /// The store of the log.
    log: Arc<dyn Store>,
    /// The checkpoints of this server.
    checkpoints: Mutex<Checkpoints>,
    /// What `GET /v1/server` tells about this instance.
    facts: Mutex<Facts>,
}

/// The last checkpoint, and why the server writes none.
struct Checkpoints {
    /// The position of the last checkpoint, or of the start of the log.
    position: u64,
    /// The time of the last checkpoint, or of the start.
    at: Instant,
    /// Why this server writes no checkpoint (01M3TBZBQDF0ES4KM54FJQF6Z8).
    blocked: Option<String>,
    /// The newest checkpoint, for the facts.
    newest: Option<CheckpointFacts>,
}

impl Checkpoints {
    fn new(blocked: Option<String>, newest: Option<CheckpointFacts>) -> Self {
        if let Some(why) = &blocked {
            tracing::warn!("this server writes no checkpoint: {why}");
        }
        Checkpoints {
            position: newest.as_ref().map_or(0, |c| c.position),
            at: Instant::now(),
            blocked,
            newest,
        }
    }
}

/// The facts of this instance that only the server counts
/// (01M3TJWJ12WEDCXW3W0529KRP2). The state and the token store give the
/// other facts.
#[derive(Default)]
struct Facts {
    /// The start time of the instance, in milliseconds since the Unix
    /// epoch.
    started_at_ms: u64,
    /// How long the load and the replay took.
    replay: Duration,
    last_error: Option<FactError>,
    chunk_written_at_ms: Option<u64>,
    chunk_write: Option<Duration>,
    /// The number of failed tries of a chunk write.
    write_errors: u64,
    /// The number of records that this build skipped.
    skipped: u64,
    /// The number of chunks in the store.
    chunks: u64,
    /// Why the server stopped for good.
    stopped: Option<String>,
}

impl Facts {
    /// The facts of an instance that starts now.
    fn start() -> Self {
        Facts {
            started_at_ms: now_ms(),
            ..Facts::default()
        }
    }
}

impl Drop for Server {
    /// Wakes the writer, so that its task ends.
    fn drop(&mut self) {
        self.engine.queued().notify_one();
    }
}

impl FromRef<Shared> for Engine {
    fn from_ref(server: &Shared) -> Engine {
        server.engine.clone()
    }
}

/// When a server serves. A server with no lease serves until it stops.
struct Gate {
    /// With a lease: the server serves until this time (R139).
    until: Mutex<Option<Instant>>,
    /// True once the server stopped for good (R140).
    stopped: tokio::sync::watch::Sender<bool>,
    /// True once the server shuts down: it takes no call, but it still
    /// saves (R129).
    closing: AtomicBool,
}

/// Where a server saves its token store.
struct Saved {
    store: Arc<dyn Store>,
    /// The last write of the token store. The lock lets only one write
    /// run at a time.
    written: tokio::sync::Mutex<Written>,
}

/// The last write of the token store.
#[derive(Default)]
struct Written {
    /// The version of the object that the server knows (R141).
    version: Option<Version>,
    /// The start of the last write (R127).
    at: Option<tokio::time::Instant>,
}

impl Server {
    fn tokens(&self) -> MutexGuard<'_, Tokens> {
        // A panic while the lock is held leaves plain data behind; keep going.
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
        Ok(SignedIn {
            who,
            jkt: proof.jkt,
        })
    }

    /// The keys of the live sign-ins of each user (R199).
    fn keys<'a>(&self, users: impl IntoIterator<Item = &'a str>) -> Keys {
        let tokens = self.tokens();
        let now = Instant::now();
        users
            .into_iter()
            .map(|user| (user.to_owned(), tokens.keys(user, now)))
            .collect()
    }

    fn replay(&self) -> MutexGuard<'_, Replay> {
        self.replay
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// The token store for a change. The change is counted, and
    /// [`Server::save_tokens_since`] saves it.
    fn tokens_change(&self) -> MutexGuard<'_, Tokens> {
        let tokens = self.tokens();
        self.tokens_changes.fetch_add(1, Ordering::SeqCst);
        tokens
    }

    /// True when a change after the first `mark` changes is not saved.
    fn tokens_unsaved(&self, mark: u64) -> bool {
        let saved = self.tokens_saved.load(Ordering::SeqCst).max(mark);
        self.tokens_changes.load(Ordering::SeqCst) > saved
    }

    /// Writes the token store, when a change after the first `mark`
    /// changes is not saved (R128). It waits until the last write is
    /// [`Config::save_every`] old (R127). A server with no store does
    /// nothing.
    async fn save_tokens_since(&self, mark: u64) -> Result<(), StoreError> {
        let Some(saved) = &self.saved else {
            return Ok(());
        };
        if !self.tokens_unsaved(mark) {
            return Ok(());
        }
        let mut written = saved.written.lock().await;
        // A write that ran while this call waited for the lock can hold
        // the change already.
        if !self.tokens_unsaved(mark) {
            return Ok(());
        }
        self.save_tokens(saved, &mut written).await
    }

    async fn save_tokens(&self, saved: &Saved, written: &mut Written) -> Result<(), StoreError> {
        // At most one write each `save_every` (R127). Each call that
        // waits for the lock finds its change in this write.
        if let Some(at) = written.at {
            tokio::time::sleep_until(at + self.config.save_every).await;
        }
        let (bytes, changes) = {
            let tokens = self.tokens();
            let changes = self.tokens_changes.load(Ordering::SeqCst);
            (tokens.to_bytes(Instant::now(), SystemTime::now()), changes)
        };
        if !self.leased() {
            return Err(StoreError::Failed("the server does not serve now".into()));
        }
        written.at = Some(tokio::time::Instant::now());
        let result = saved.store.save(SIGN_INS, bytes, written.version).await;
        self.tokens_failed.store(result.is_err(), Ordering::SeqCst);
        let version = match result {
            Ok(version) => version,
            Err(error) => {
                if let StoreError::Conflict(_) = error {
                    self.stop(&error.to_string());
                }
                return Err(error);
            }
        };
        written.version = Some(version);
        self.tokens_saved.fetch_max(changes, Ordering::SeqCst);
        Ok(())
    }

    /// Lets a refresh go on, which gets its reply before the write of
    /// the token store (01M3TFG527M04TA7ESM970X3B8). While the last write
    /// failed, it writes the store again first, and refuses when that
    /// write fails too. So the saved store does not fall more and more
    /// generations behind.
    async fn tokens_written(&self) -> Result<(), TokenError> {
        if !self.tokens_failed.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.save_tokens_since(0).await.map_err(|error| {
            self.error(format!("the token store was not saved: {error}"));
            no(UNAVAILABLE)
        })
    }

    fn until(&self) -> MutexGuard<'_, Option<Instant>> {
        self.gate
            .until
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// True while the server holds the lease and has not stopped, so it
    /// may save (R139, R140).
    fn leased(&self) -> bool {
        !self.is_stopped() && self.until().is_none_or(|until| Instant::now() < until)
    }

    /// True while the server takes calls: it holds the lease, and it
    /// does not shut down.
    fn serving(&self) -> bool {
        self.leased() && !self.gate.closing.load(Ordering::SeqCst)
    }

    fn is_stopped(&self) -> bool {
        *self.gate.stopped.borrow()
    }

    /// Stops for good (R140). Each command that waits for the writer
    /// fails.
    fn stop(&self, why: &str) {
        if !self.gate.stopped.send_replace(true) {
            tracing::warn!("stopped for good: {why}");
            self.facts().stopped = Some(why.to_owned());
        }
        self.engine.stop();
    }

    fn facts(&self) -> MutexGuard<'_, Facts> {
        self.facts
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Logs an error, and keeps it as the last error for the facts
    /// (01M3TJWJ12WEDCXW3W0529KRP2).
    fn error(&self, message: String) {
        tracing::error!("{message}");
        self.facts().last_error = Some(FactError {
            message,
            at_ms: now_ms(),
        });
    }

    /// Why the server replies 503 to each call, or `None` while it
    /// serves.
    fn not_serving(&self) -> Option<String> {
        if let Some(why) = &self.facts().stopped {
            return Some(format!("it stopped for good: {why}"));
        }
        if self.gate.closing.load(Ordering::SeqCst) {
            return Some("it shuts down".into());
        }
        (!self.leased()).then(|| "its last read of the lease is too old".into())
    }

    /// The facts of this instance, for `GET /v1/server`
    /// (01M3TJWJ12WEDCXW3W0529KRP2).
    fn server_facts(&self) -> ServerFacts {
        let not_serving = self.not_serving();
        let sign_ins = self.tokens().chains() as u64;
        let (position, counts) = self
            .engine
            .read(|state| (state.written_position(), state.counts()));
        let (checkpoint, no_checkpoint) = {
            let marks = self.checkpoints();
            (marks.newest.clone(), marks.blocked.clone())
        };
        let facts = self.facts();
        let ms = |d: Duration| u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
        ServerFacts {
            not_serving,
            last_error: facts.last_error.clone(),
            position,
            chunk_written_at_ms: facts.chunk_written_at_ms,
            chunk_write_ms: facts.chunk_write.map(ms),
            write_errors: facts.write_errors,
            skipped_records: facts.skipped,
            checkpoint,
            no_checkpoint,
            chunks: facts.chunks,
            sessions: counts.sessions,
            cursors: counts.cursors,
            threads: counts.threads,
            sign_ins,
            memory_bytes: memory_bytes(),
            started_at_ms: facts.started_at_ms,
            replay_ms: ms(facts.replay),
            now_ms: now_ms(),
        }
    }

    /// Ends when the server stops for good.
    fn stopping(&self) -> impl Future<Output = ()> + use<> {
        let mut stopped = self.gate.stopped.subscribe();
        async move {
            // An error means that the server is gone, so it stopped too.
            let _ = stopped.wait_for(|stopped| *stopped).await;
        }
    }

    /// Writes a checkpoint when one is due, then prunes. See
    /// [`Service::keep_checkpoints`].
    async fn checkpoint(&self, settings: &checkpoint::Settings) {
        let (last, at) = {
            let marks = self.checkpoints();
            if marks.blocked.is_some() {
                return;
            }
            (marks.position, marks.at)
        };
        let snapshot = self.engine.read(|state| {
            let came = state.written_position().saturating_sub(last);
            let due = came >= settings.every_records || at.elapsed() >= settings.every;
            (came > 0 && due).then(|| state.snapshot(Instant::now(), now_ms()))
        });
        let Some(snapshot) = snapshot else {
            return;
        };
        let position = snapshot.position;
        let checkpoint = checkpoint::Checkpoint::new(&settings.build, now_ms(), snapshot);
        match checkpoint::write(&*self.log, &checkpoint).await {
            Ok(name) => {
                tracing::info!(position, "wrote the checkpoint {name}");
                let mut marks = self.checkpoints();
                marks.position = position;
                marks.at = Instant::now();
                marks.newest = Some(CheckpointFacts {
                    position,
                    written_at_ms: checkpoint.written_at_ms,
                    build: checkpoint.build,
                });
            }
            Err(error) => {
                self.error(format!(
                    "the checkpoint at position {position} was not written: {error}"
                ));
                return;
            }
        }
        match checkpoint::prune(&*self.log, settings, now_ms()).await {
            Ok(pruned) => {
                tracing::info!(
                    checkpoints = pruned.checkpoints.len(),
                    chunks = pruned.chunks.len(),
                    "deleted the old checkpoints and chunks"
                );
                let mut facts = self.facts();
                facts.chunks = facts.chunks.saturating_sub(pruned.chunks.len() as u64);
            }
            Err(error) => tracing::warn!("the delete of old checkpoints failed: {error}"),
        }
    }

    fn checkpoints(&self) -> MutexGuard<'_, Checkpoints> {
        self.checkpoints
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// One look at the owner role at `now` (see [`owner`]): it grants a
    /// request whose time ended, then checks the owner when a check is
    /// due. It warns the owner one check before the owner is gone.
    /// Returns the change that it made.
    async fn owner_tick(&self, checks: &mut Checks, now: Instant) -> Option<OwnerChange> {
        let timing = &self.config.owner_role;
        if self.tokens().is_due(now) {
            return self.tokens_change().owner_due(now);
        }
        if !checks.due(now) {
            return None;
        }
        let owner = {
            let tokens = self.tokens();
            let others = !tokens.roles(&self.config.admins).1.is_empty();
            match tokens.owner() {
                Some(email) if others => {
                    Some((email.to_owned(), tokens.owner_user().map(str::to_owned)))
                }
                _ => None,
            }
        };
        // Only a riff with an owner and another admin checks the owner.
        let Some((email, user)) = owner else {
            checks.reset(now, timing);
            return None;
        };
        let since = checks.since(timing);
        let seen = user
            .as_deref()
            .is_some_and(|user| self.engine.read(|state| state.present(user, since, now)));
        match checks.record(now, seen, timing) {
            Check::Seen | Check::Missed => None,
            Check::Warn => {
                self.warn_owner(&owner::warn_news(&email, timing), user.as_deref())
                    .await;
                None
            }
            Check::Gone => self.tokens_change().owner_gone(),
        }
    }

    /// A note or a message of the server, for [`Server::announce_each`].
    fn news(thread: Option<ThreadName>, to: Vec<Selector>, body: &str, kind: Kind) -> Announce {
        Announce {
            thread,
            to,
            body: body.to_owned(),
            kind,
            at_ms: now_ms(),
        }
    }

    /// Sends each post of the server: one command `announce` for each
    /// (01M3N7K4BC1RPZKQ1XNDTBRPGF). They go to the engine together, so
    /// they wait for one write. A post that did not go is only logged.
    async fn announce_each(&self, posts: Vec<Announce>) {
        let sent = posts.into_iter().map(|post| self.engine.announce(post));
        for result in futures::future::join_all(sent).await {
            if let Err(failed) = result {
                tracing::warn!("a note of the server did not go: {}", failed.text());
            }
        }
    }

    /// Warns the owner that the next check can drop the owner
    /// (01M3Q546335NBTKG5BHQ27QC93): a note to each session of `user` in
    /// the thread of each repository, and one line in the chat.
    async fn warn_owner(&self, news: &str, user: Option<&str>) {
        tracing::info!("{news}");
        let mut posts = Vec::new();
        if let Some(user) = user {
            for thread in self.engine.read(State::repositories) {
                let to = vec![Selector {
                    user: Some(user.to_owned()),
                    ..Selector::default()
                }];
                posts.push(Server::news(Some(thread), to, news, Kind::Note));
            }
        }
        let chat = ThreadName::chat();
        posts.push(Server::news(Some(chat), Vec::new(), news, Kind::Message));
        self.announce_each(posts).await;
    }

    /// Saves a change of the owner role that no person made, and posts
    /// its note (01M3N7K4DVHSF7AQ402F14J26Z). When the riff has no owner
    /// now, it also asks each admin for a volunteer.
    async fn owner_changed(&self, change: &OwnerChange) {
        if let Err(error) = self.save_tokens_since(0).await {
            self.error(format!("the token store was not saved: {error}"));
        }
        let news = owner::change_news(change, &self.config.owner_role);
        tracing::info!("{news}");
        let admins = match change {
            OwnerChange::Gone { owner: None, .. } => self.tokens().admin_users(&self.config.admins),
            _ => Vec::new(),
        };
        self.announce(&news, &admins).await;
    }

    /// Asks each idle worker past the limit to stop, and posts a note to
    /// the lead of its user for each (see [`idle`]).
    async fn stop_idle_workers(&self) {
        let settings = self.engine.read(State::idle);
        let mut posts = Vec::new();
        for stopping in self.engine.stop_idle_workers() {
            let news = idle::news(&stopping, &settings);
            tracing::info!("{news}");
            let Some(thread) = stopping.worker.default_thread() else {
                continue;
            };
            let lead = Selector::lead(stopping.worker.who().user(), &thread.to_string());
            posts.push(Server::news(Some(thread), vec![lead], &news, Kind::Note));
        }
        self.announce_each(posts).await;
    }

    /// Posts a note of the server to the thread of each repository of
    /// the riff. It sends the same text to each live lead of `users` as a
    /// direct message (see [`owner`]).
    async fn announce(&self, news: &str, users: &[String]) {
        let now = Instant::now();
        let (repositories, leads) = self.engine.read(|state| {
            let leads: Vec<Who> = users
                .iter()
                .flat_map(|user| state.live_leads(user, now))
                .collect();
            (state.repositories(), leads)
        });
        let mut posts = Vec::new();
        for thread in repositories {
            let to = vec![Selector {
                repo: Some(thread.to_string()),
                ..Selector::default()
            }];
            posts.push(Server::news(Some(thread), to, news, Kind::Note));
        }
        let direct = owner::to_lead(news);
        for lead in leads {
            let Some(id) = lead.session() else {
                continue;
            };
            let to = vec![Selector::session(id)];
            posts.push(Server::news(None, to, &direct, Kind::Message));
        }
        self.announce_each(posts).await;
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
/// let pair = service
///     .tokens()
///     .sign_in("mike@comotechnologies.io", "jkt", Instant::now())
///     .unwrap();
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
    /// A new server with these settings. It keeps its log in memory, and
    /// saves nothing (R34).
    pub fn new(config: Config) -> Self {
        Service::build(
            config,
            State::with_writer(Instant::now(), now_ms()),
            Tokens::default(),
            oidc::client(oidc::FETCH_TIMEOUT),
            None,
            Arc::new(Memory::default()),
            None,
            now_ms() / 1000,
            Checkpoints::new(None, None),
            Facts::start(),
        )
    }

    /// A server with the state that `store` holds
    /// (01M3THEE08ZKV8WGHDSVWV69ZE).
    ///
    /// ```mermaid
    /// flowchart TD
    ///     L[load the sign-ins and the newest checkpoint,<br/>replay the log after it] -->|fails| X[error: no lease.<br/>The old instance serves on]
    ///     L -->|works| T[take the lease, wait]
    ///     T --> C[apply the chunks that came since the load,<br/>list the checkpoints again,<br/>load the sign-ins again]
    ///     C --> S[serve from the next whole second]
    /// ```
    ///
    /// - It loads first, with no lease. So a build that cannot load
    ///   changes nothing in the store, and the old instance serves on.
    /// - Then it takes the lease and waits [`lease::Timing::wait`]. The
    ///   old instance writes until it reads the new lease. So the new
    ///   instance then applies the chunks that came since its load, and
    ///   loads the sign-ins again.
    /// - The old instance can also write a checkpoint in that time. So
    ///   the new instance lists the checkpoints again. When a checkpoint
    ///   came, it takes the position of the newest one, and writes no
    ///   checkpoint when that one comes from a later version or does not
    ///   read (01M3TJWJC08ZR5TWA1Y9CDE0QM).
    /// - It serves from the next whole second (R138, R142). It writes
    ///   each change to the log of `store`, and saves the token store
    ///   within [`Config::save_every`], while the service lives (R30).
    /// - It fails when another instance took the lease during the wait,
    ///   or when the log does not read (see [`log::replay`]).
    /// - The claim timer of each session starts at the load (R125).
    ///
    /// ```
    /// # #[tokio::main] async fn main() -> Result<(), riff_server::store::StoreError> {
    /// use std::sync::Arc;
    /// use std::time::Duration;
    /// use riff_server::Service;
    /// use riff_server::auth::Config;
    /// use riff_server::store::{LEASE, Memory, Store};
    ///
    /// let mut config = Config::default();
    /// config.lease.wait = Duration::from_millis(10);
    /// let store = Memory::default();
    /// let old = Service::load(config.clone(), Arc::new(store.clone())).await?;
    /// // The first start of a riff writes the record of `make_riff` as
    /// // the first chunk. Wait for that write.
    /// old.save().await?;
    ///
    /// // A build that cannot load takes no lease.
    /// let lease = store.load(LEASE).await?.unwrap().bytes;
    /// store.save("log/00000000000000000002.jsonl", b"not a chunk".to_vec(), None).await?;
    /// assert!(Service::load(config.clone(), Arc::new(store.clone())).await.is_err());
    /// assert_eq!(store.load(LEASE).await?.unwrap().bytes, lease);
    ///
    /// // A deploy: a new server on the same store.
    /// store.delete("log/00000000000000000002.jsonl").await?;
    /// let new = Service::load(config, Arc::new(store)).await?;
    /// old.stopped().await;
    /// # Ok(()) }
    /// ```
    pub async fn load(config: Config, store: Arc<dyn Store>) -> Result<Self, StoreError> {
        // The build of the HTTP client blocks. Build it now, so that it
        // does not use up the serve time after the lease read (R139).
        let http = oidc::client(oidc::FETCH_TIMEOUT);
        // The load, with no lease. An error here leaves the store as it
        // is, and the old instance serves on.
        let mut facts = Facts::start();
        let replayed = Instant::now();
        refuse_old_objects(&*store).await?;
        load_tokens(&*store).await?;
        let build = config.checkpoint.build.clone();
        let found = checkpoint::load(&*store, &build).await?;
        let mut newest = found.checkpoint.as_ref().map(checkpoint_facts);
        let mut blocked = found.blocked;
        let snapshot = found.checkpoint.map(|c| c.state);
        let from = snapshot.as_ref().map_or(0, |s| s.position);
        let log::Replayed {
            records,
            last,
            skipped,
            skips,
            ..
        } = log::replay_after(&*store, from).await?;
        let count = records.len();
        let mut state = State::load(snapshot, records, Instant::now(), now_ms());
        state.continue_after(last);
        facts.replay = replayed.elapsed();
        tracing::info!(
            checkpoint = from,
            records = count,
            position = state.position(),
            "loaded the checkpoint and replayed the log after it in {:?}",
            facts.replay
        );
        let lease = Lease::take(store.clone()).await?;
        tracing::info!(
            "took the lease as {}; waiting {:?} for the old instance",
            lease.id(),
            config.lease.wait
        );
        tokio::time::sleep(config.lease.wait).await;
        // Serve from the next whole second, and refuse each proof issued
        // before it (R142). See the lease module for the argument.
        tokio::time::sleep(Duration::from_millis(1000 - now_ms() % 1000)).await;
        let start = now_ms() / 1000;
        // The old instance wrote until it read the new lease.
        let since = log::replay_after(&*store, state.position()).await?;
        tracing::info!(
            records = since.records.len(),
            position = since.last,
            "applied the records that came since the load"
        );
        state.catch_up(since.records);
        state.continue_after(since.last);
        let skipped = skipped.or(since.skipped);
        facts.skipped = skips + since.skips;
        facts.chunks = since.chunks;
        // The old instance can also write a checkpoint until it reads
        // the new lease. This build writes none past a checkpoint of a
        // later version (01M3TJWJC08ZR5TWA1Y9CDE0QM).
        let now = checkpoint::names(&*store).await?.pop();
        if now.map(|(_, _, name)| name) != found.newest {
            let again = checkpoint::load(&*store, &build).await?;
            tracing::info!("a checkpoint came since the load");
            newest = again.checkpoint.as_ref().map(checkpoint_facts);
            blocked = again.blocked;
        }
        let (tokens, version) = load_tokens(&*store).await?;
        let written = Written { version, at: None };
        let blocked = blocked.or_else(|| {
            skipped.map(|position| format!("this build skipped the record at position {position}"))
        });
        let checkpoints = Checkpoints::new(blocked, newest);
        let asked = Instant::now();
        if !lease.held().await? {
            return Err(StoreError::Conflict(store::LEASE.into()));
        }
        let saved = Saved {
            store: store.clone(),
            written: tokio::sync::Mutex::new(written),
        };
        let until = asked + config.lease.valid_for;
        let service = Service::build(
            config,
            state,
            tokens,
            http,
            Some(saved),
            store,
            Some(until),
            start,
            checkpoints,
            facts,
        );
        // Save the tokens once, so that a new riff ID stays
        // (01M3JNVBPMZ1K9WX7Q7DP6Y0DH).
        drop(service.0.tokens_change());
        if let Err(error) = service.0.save_tokens_since(0).await {
            service
                .0
                .error(format!("the token store was not saved: {error}"));
        }
        service.keep_lease(lease);
        service.save_each_second();
        Ok(service)
    }

    /// `log` is the store of the log. `until` is the end of the first
    /// serve time of a server with a lease. `start` is the second when it
    /// starts to serve.
    #[allow(clippy::too_many_arguments)]
    fn build(
        config: Config,
        state: State,
        tokens: Tokens,
        http: reqwest::Client,
        saved: Option<Saved>,
        log: Arc<dyn Store>,
        until: Option<Instant>,
        start: u64,
        checkpoints: Checkpoints,
        facts: Facts,
    ) -> Self {
        let mut tokens = tokens;
        if let Some(owner) = &config.owner {
            tokens.name_owner(owner);
        }
        let mut replay = Replay::default();
        replay.refuse_before(start);
        // A log with no record is the first start of a riff
        // (01M3WRD99M99PNGP8ME50KC6WS).
        let first = state.position() == 0;
        let tokens = Arc::new(Mutex::new(tokens));
        let roles = Roles {
            tokens: tokens.clone(),
            admins: config.admins.clone(),
            needs_sign_in: config.require_sign_in,
            trusted: config.trusted(),
        };
        let engine = Engine::new(state, roles);
        if first {
            engine.make_riff();
        }
        let service = Service(Arc::new(Server {
            config,
            engine,
            tokens,
            tokens_changes: AtomicU64::new(0),
            tokens_saved: AtomicU64::new(0),
            tokens_failed: AtomicBool::new(false),
            replay: Mutex::new(replay),
            http,
            saved,
            gate: Gate {
                until: Mutex::new(until),
                stopped: tokio::sync::watch::Sender::new(false),
                closing: AtomicBool::new(false),
            },
            log,
            checkpoints: Mutex::new(checkpoints),
            facts: Mutex::new(facts),
        }));
        service.write_log();
        service.keep_checkpoints();
        service.forget_sessions();
        service.watch_owner();
        service.watch_idle_workers();
        service
    }

    /// Starts the writer: the task that writes the records of each entry
    /// in the queue of the engine as a chunk of the log (see [`log`]).
    /// After each chunk, the engine finishes each command of the chunk
    /// ([`Engine::finish`], 01M3WRD90WBBCWTDGVQCBR6MNT): it applies the
    /// records to the written state, sends the wakes, and tells each
    /// call. When a chunk fails for good, it stops the server for good.
    /// A server with no async runtime, for example in a doc test, starts
    /// no task. The task ends when the service ends.
    fn write_log(&self) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let server = Arc::downgrade(&self.0);
        let queued = self.0.engine.queued();
        runtime.spawn(async move {
            loop {
                let Some(s) = server.upgrade() else {
                    break;
                };
                if s.is_stopped() {
                    break;
                }
                // An instance writes only while it may serve (R155).
                if !s.leased() {
                    drop(s);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                let Some(chunk) = s.engine.take() else {
                    drop(s);
                    queued.notified().await;
                    continue;
                };
                let records = chunk.records();
                let log = s.log.clone();
                let timing = s.config.log;
                let began = Instant::now();
                let failed = AtomicU64::new(0);
                // A chunk of commands with no record needs no object:
                // the write gives its proof at once.
                let wrote =
                    log::write_counted(&*log, &records, &timing, || s.leased(), &failed).await;
                s.facts().write_errors += failed.into_inner();
                match wrote {
                    Ok(written) => {
                        s.engine.finish(chunk, written);
                        if !records.is_empty() {
                            let mut facts = s.facts();
                            facts.chunks += 1;
                            facts.chunk_written_at_ms = Some(now_ms());
                            facts.chunk_write = Some(began.elapsed());
                        }
                    }
                    Err(error) => {
                        // The calls of the chunk fail: the server stopped.
                        // Each command of the chunk gets the line `failed`.
                        s.engine.fail(chunk);
                        let first = records.first().map_or(0, |record| record.position);
                        let chunk = log::chunk_name(first);
                        s.error(format!(
                            "the write of the chunk {chunk} failed for good: {error}"
                        ));
                        s.stop(&format!("the write of the chunk {chunk} failed"));
                        break;
                    }
                }
            }
        });
    }

    /// Starts the task that writes a checkpoint each
    /// [`checkpoint::Settings::every_records`] records, or each
    /// [`checkpoint::Settings::every`] when records came, while the server
    /// serves. It takes a snapshot of the written state under the lock,
    /// and encodes and writes it outside the lock. Then it deletes the old
    /// checkpoints and the chunks that no kept checkpoint needs. A server
    /// with no async runtime starts no task. The task ends when the
    /// service ends.
    fn keep_checkpoints(&self) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let server = Arc::downgrade(&self.0);
        let settings = self.0.config.checkpoint.clone();
        runtime.spawn(async move {
            let mut tick = tokio::time::interval(settings.check_every);
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                let Some(s) = server.upgrade() else {
                    break;
                };
                if s.is_stopped() {
                    break;
                }
                if s.leased() {
                    s.checkpoint(&settings).await;
                }
            }
        });
    }

    /// Starts the task that forgets each session with no sign of life for
    /// [`state::SESSION_EXPIRY`], each [`FORGET_EVERY`]. A server with no
    /// async runtime starts no task. The task ends when the service ends.
    fn forget_sessions(&self) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let server = Arc::downgrade(&self.0);
        runtime.spawn(async move {
            let mut tick = tokio::time::interval(FORGET_EVERY);
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                let Some(s) = server.upgrade() else {
                    break;
                };
                if s.is_stopped() {
                    break;
                }
                if s.serving() {
                    match s.engine.forget().await {
                        Ok(0) => {}
                        Ok(forgotten) => tracing::info!(
                            sessions = forgotten,
                            "forgot the sessions with no sign of life"
                        ),
                        Err(failed) => {
                            tracing::warn!("the sessions were not forgotten: {}", failed.text());
                        }
                    }
                }
            }
        });
    }

    /// Starts the task that looks at the owner role each
    /// [`owner::Timing::tick`]: it grants a request whose time ended, and
    /// checks the owner (see [`owner`]). A server with no async runtime,
    /// for example in a doc test, starts no task. The task ends when the
    /// service ends.
    fn watch_owner(&self) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let server = Arc::downgrade(&self.0);
        let timing = self.0.config.owner_role;
        runtime.spawn(async move {
            let mut tick = tokio::time::interval(timing.tick());
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            let mut checks = Checks::new(Instant::now(), &timing);
            loop {
                tick.tick().await;
                let Some(server) = server.upgrade() else {
                    break;
                };
                if server.is_stopped() {
                    break;
                }
                if !server.serving() {
                    continue;
                }
                if let Some(change) = server.owner_tick(&mut checks, Instant::now()).await {
                    server.owner_changed(&change).await;
                }
            }
        });
    }

    /// Starts the task that asks the idle workers past the limit to stop,
    /// each [`idle::CHECK_EVERY`] (see [`idle`]). A server with no async
    /// runtime starts no task. The task ends when the service ends.
    fn watch_idle_workers(&self) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let server = Arc::downgrade(&self.0);
        runtime.spawn(async move {
            let mut tick = tokio::time::interval(idle::CHECK_EVERY);
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                let Some(server) = server.upgrade() else {
                    break;
                };
                if server.is_stopped() {
                    break;
                }
                if server.serving() {
                    server.stop_idle_workers().await;
                }
            }
        });
    }

    /// Waits until each record in the queue is written, then saves the
    /// token store when it changed. A server with no store saves no
    /// token store. A server that does not hold the lease now, or that
    /// stopped, saves nothing (R140, R155). A save that finds another
    /// version stops the server for good (R141).
    pub async fn save(&self) -> Result<(), StoreError> {
        if !self.0.leased() {
            return Ok(());
        }
        if self.0.engine.settle().await.is_err() {
            return Err(StoreError::Failed(
                "the server stopped before it wrote the log".into(),
            ));
        }
        let Some(saved) = &self.0.saved else {
            return Ok(());
        };
        let mut written = saved.written.lock().await;
        if !self.0.leased() || !self.0.tokens_unsaved(0) {
            return Ok(());
        }
        self.0.save_tokens(saved, &mut written).await
    }

    /// Stops taking calls, then saves each unsaved change (R129). The
    /// gate replies 503 from now on. `main` calls it on SIGTERM.
    pub async fn shutdown(&self) -> Result<(), StoreError> {
        self.0.gate.closing.store(true, Ordering::SeqCst);
        self.save().await
    }

    /// Ends when the server stops for good (R140, R141).
    pub async fn stopped(&self) {
        self.0.stopping().await;
    }

    /// Starts the task that reads the lease each
    /// [`lease::Timing::read_every`] (R139). The task ends when the
    /// server stops or ends.
    fn keep_lease(&self, lease: Lease) {
        let server = Arc::downgrade(&self.0);
        let timing = self.0.config.lease;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(timing.read_every);
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                let Some(server) = server.upgrade() else {
                    break;
                };
                if server.is_stopped() {
                    break;
                }
                let asked = Instant::now();
                match lease.held().await {
                    Ok(true) => *server.until() = Some(asked + timing.valid_for),
                    Ok(false) => {
                        server.stop("another instance holds the lease");
                        break;
                    }
                    Err(error) => tracing::warn!("the lease read failed: {error}"),
                }
            }
        });
    }

    /// Starts the task that saves the changed token store each
    /// [`Config::save_every`]. The task ends when the service ends.
    fn save_each_second(&self) {
        let server = Arc::downgrade(&self.0);
        let every = self.0.config.save_every;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(every);
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                let Some(server) = server.upgrade() else {
                    break;
                };
                if server.is_stopped() {
                    break;
                }
                if let Err(error) = server.save_tokens_since(0).await {
                    server.error(format!("the token store was not saved: {error}"));
                }
            }
        });
    }

    /// The HTTP routes of this server.
    pub fn router(&self) -> Router {
        // ANCHOR: routes
        // The commands: one line for each, and one handler for all
        // (01M3WRD8TBDPA4JNEZY6J4N2EX).
        let commands = Router::new()
            .route(Register::PATH, post(command::<Register>))
            .route(Start::PATH, post(command::<Start>))
            .route(End::PATH, post(command::<End>))
            .route(Join::PATH, post(command::<Join>))
            .route(Leave::PATH, post(command::<Leave>))
            .route(Post::PATH, post(command::<Post>))
            .route(Claim::PATH, post(command::<Claim>))
            .route(Release::PATH, post(command::<Release>))
            .route(ReleaseFor::PATH, post(command::<ReleaseFor>))
            .route(Lead::PATH, post(command::<Lead>))
            .route(Pause::PATH, post(command::<Pause>))
            .route(Resume::PATH, post(command::<Resume>));
        // `set_idle` has a router of its own: in a riff with sign-in,
        // its route always has the token check.
        let set_idle = Router::new().route(SetIdle::PATH, post(command::<SetIdle>));
        // ANCHOR_END: routes
        // The signals and the queries.
        let mut routes = commands
            .route(SetStatus::PATH, post(status))
            .route(Alive::PATH, post(alive))
            .route(WhoRequest::PATH, post(who))
            .route(Threads::PATH, post(threads))
            .route(Read::PATH, post(read))
            .route(RiffQuery::PATH, post(riff))
            .route("/v1/me", get(me))
            .route("/v1/watch", get(watch))
            .route("/v1/tail", get(tail_thread));
        let guard = || middleware::from_fn_with_state(self.0.clone(), require_token);
        // The settings of idle workers: the query, and the command
        // `set_idle` on a path of its own (01M3WRD9BSBKS9TN66H29TGTBV).
        let idle = Router::new()
            .route(IdleQuery::PATH, post(idle_workers))
            .merge(set_idle);
        // A riff with sign-in knows who changes the idle workers
        // (01M3Q5A0TF9K49V8Z1ZY9NDF74): there, the routes of the idle
        // workers have the token check.
        let mut admin_routes = Router::new();
        if self.0.config.trusted() {
            routes = routes.merge(idle);
        } else {
            admin_routes = admin_routes.merge(idle);
        }
        if self.0.config.require_sign_in {
            routes = routes.route_layer(guard());
        }
        let admin_routes = admin_routes
            .route("/v1/revoke", post(revoke))
            .route("/v1/invite", post(invite))
            .route("/v1/remove", post(remove))
            .route("/v1/members", post(members))
            .route("/v1/admin", post(admin))
            .route("/v1/owner", post(pass_owner))
            .route("/v1/owner/take", post(take_owner))
            .route("/v1/owner/deny", post(deny_owner))
            .route_layer(guard());
        let mut facts = Router::new().route("/v1/server", get(server_facts));
        if self.0.config.require_sign_in {
            facts = facts.route_layer(guard());
        }
        routes
            .merge(admin_routes)
            .route_layer(middleware::from_fn(check_build))
            // `riff login` and a refresh work with each version
            // (01M3MX4V43SF2XFCZWANHD19WV).
            .route(auth::TOKEN_PATH, post(token))
            .route("/v1/sign-in", get(sign_in_config))
            .route(auth::RESOURCE_METADATA_PATH, get(resource_metadata))
            .route(auth::SERVER_METADATA_PATH, get(server_metadata))
            .layer(middleware::from_fn_with_state(self.0.clone(), gate))
            // The facts answer also while the gate replies 503, and to a
            // `riff` of each version (01M3TJWJ12WEDCXW3W0529KRP2).
            .merge(facts)
            .layer(middleware::from_fn(stamp_build))
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

/// The proof of the token layer in a request, when the route has the
/// token check.
struct Proof {
    signed_in: Option<SignedIn>,
    /// The path of the call, for the line of a refusal.
    path: String,
}

#[cfg(test)]
impl Proof {
    /// The proof of a call with no token, for a test that calls a
    /// handler.
    fn none() -> Proof {
        Proof {
            signed_in: None,
            path: String::new(),
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Proof {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Proof, Infallible> {
        Ok(Proof {
            signed_in: parts.extensions.get::<SignedIn>().cloned(),
            path: parts.uri.path().to_owned(),
        })
    }
}

/// Admits the caller of a signal or of a query that acts as `me`
/// ([`Engine::admit`]). A refusal gives the line `denied`
/// (RID_DENIED).
fn admit(s: &Server, proof: &Proof, me: &SessionUri) -> Result<Admitted, Failed> {
    s.engine
        .admit(proof.signed_in.as_ref(), me)
        .inspect_err(|failed| failed.trace_denied(&proof.path, Some(me)))
}

/// A keep-alive: a sign of life that is not a call (R204). It is a
/// signal.
async fn alive(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<Alive>,
) -> Reply<AliveReply> {
    let caller = admit(&s, &proof, &r.me)?;
    Ok(Json(s.engine.signal(&caller, Signal::Alive).await?))
}

/// Sets the status of a session. It is a signal: it makes no record,
/// and it does not wait for the writer (01M3WRD97EZJK3AABXECXEY133).
async fn status(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<SetStatus>,
) -> Reply<()> {
    let caller = admit(&s, &proof, &r.me)?;
    r.status.check().map_err(bad_request)?;
    let status = Signal::Status {
        status: r.status,
        at_ms: now_ms(),
    };
    s.engine.signal(&caller, status).await?;
    Ok(Json(()))
}

async fn who(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<WhoRequest>,
) -> Reply<WhoReply> {
    let (owner, members) = if s.config.trusted() {
        (RiffOwner::NoSignIn, Vec::new())
    } else {
        let tokens = s.tokens();
        (tokens.riff_owner(), tokens.people(&s.config.admins))
    };
    let caller = admit(&s, &proof, &r.me)?;
    let reply = s
        .engine
        .query(&caller, |state| {
            let now = Instant::now();
            let now_ms = now_ms();
            let people = members
                .into_iter()
                .map(|(user, role)| {
                    let seen = state.seen(&user, now, now_ms);
                    Person {
                        live: seen.is_some_and(|(live, _)| live),
                        seen_secs: seen.map(|(_, secs)| secs),
                        user,
                        role,
                    }
                })
                .collect();
            WhoReply {
                sessions: state.who(now, now_ms, r.all),
                owner,
                people,
            }
        })
        .await?;
    Ok(Json(reply))
}

/// Only the session of the caller, and the build of the server, for
/// the status line (01M3T5GFVS8NMA992KHZN4VE17). It changes nothing,
/// so it is not a call of the session.
async fn me(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Query(q): Query<WatchQuery>,
) -> Reply<MeReply> {
    let caller = admit(&s, &proof, &q.uri)?;
    let session = s.engine.peek(&caller, |state| {
        state.me(q.uri.who(), Instant::now(), now_ms())
    })?;
    Ok(Json(MeReply {
        session,
        build: build::VERSION.into(),
    }))
}

/// The facts of this instance, for `riff server`
/// (01M3TJWJ12WEDCXW3W0529KRP2). It changes nothing, and it is not a
/// call of a session.
async fn server_facts(AxumState(s): AxumState<Shared>) -> Json<ServerFacts> {
    Json(s.server_facts())
}

async fn threads(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<Threads>,
) -> Reply<ThreadsReply> {
    let caller = admit(&s, &proof, &r.me)?;
    let threads = s
        .engine
        .query(&caller, |state| {
            state.threads_of(r.me.who(), Instant::now())
        })
        .await?;
    Ok(Json(ThreadsReply { threads }))
}

/// Gives the messages. With sign-in, the reply holds the keys of each
/// sender, so the reader can verify each message (R199). A riff with
/// no sign-in marks the reply as trusted (R211). A direct thread of two
/// other sessions is not found ([`state::may_read`]). The read moves
/// the cursor of the caller: a signal.
async fn read(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<Read>,
) -> Reply<ReadReply> {
    let signed = proof.signed_in.is_some();
    let caller = admit(&s, &proof, &r.me)?;
    let (page, read) = s
        .engine
        .query(&caller, |state| {
            state.page(r.me.who(), &r.thread, r.all, r.after, state::PAGE)
        })
        .await?
        .map_err(not_found)?;
    if let Some(read) = read {
        s.engine.signal(&caller, read).await?;
    }
    let keys = if signed {
        s.keys(page.messages.iter().map(|m| m.from.who().user()))
    } else {
        Keys::new()
    };
    Ok(Json(ReadReply {
        messages: page.messages,
        next: page.next,
        keys,
        trusted: s.config.trusted(),
    }))
}

/// Reads the state of the riff: a query (01M3WRD9BSBKS9TN66H29TGTBV).
/// The commands `pause` and `resume` change it.
async fn riff(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<RiffQuery>,
) -> Reply<RiffReply> {
    let caller = admit(&s, &proof, &r.me)?;
    let state = s.engine.query(&caller, State::riff_state).await?;
    Ok(Json(RiffReply {
        state,
        changed: false,
    }))
}

/// Reads the settings of idle workers: a query
/// (01M3Q5A0TF9K49V8Z1ZY9NDF74). The command `set_idle` changes them.
async fn idle_workers(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<IdleQuery>,
) -> Reply<Idle> {
    let caller = admit(&s, &proof, &r.me)?;
    Ok(Json(s.engine.query(&caller, State::idle).await?))
}

/// The OAuth error code of a 503 of the token endpoint.
const UNAVAILABLE: &str = "temporarily_unavailable";

/// The OAuth 2.1 token endpoint: swaps a refresh token for a new pair.
/// A sign-in gets its reply after the write of the token store (R128).
/// A refresh and a session token do not wait for it
/// (01M3TFG527M04TA7ESM970X3B8).
async fn token(
    AxumState(s): AxumState<Shared>,
    headers: HeaderMap,
    Form(r): Form<TokenRequest>,
) -> impl IntoResponse {
    let no_store = || [(header::CACHE_CONTROL, "no-store")];
    let refuse = |error: TokenError| {
        let status = match error.error.as_str() {
            UNAVAILABLE => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::BAD_REQUEST,
        };
        (status, no_store(), Json(error)).into_response()
    };
    if r.resource
        .as_ref()
        .is_some_and(|resource| !s.config.is_resource(resource))
    {
        return refuse(no("invalid_target"));
    }
    let Ok(proof) = s.proof(&headers, "POST", auth::TOKEN_PATH, None) else {
        return refuse(no("invalid_dpop_proof"));
    };
    let reply = match r.grant_type.as_str() {
        "refresh_token" => refresh(&s, &r, &proof).await,
        TOKEN_EXCHANGE => match r.subject_token_type.as_deref() {
            Some(ID_TOKEN_TYPE) => {
                let mark = s.tokens_changes.load(Ordering::SeqCst);
                let reply = exchange(&s, &r, &proof).await;
                match s.save_tokens_since(mark).await {
                    Ok(()) => reply,
                    Err(error) => {
                        tracing::error!("the token store was not saved: {error}");
                        Err(no(UNAVAILABLE))
                    }
                }
            }
            Some(ACCESS_TOKEN_TYPE) => for_session(&s, &r, &proof).await,
            _ => Err(no("invalid_request")),
        },
        _ => Err(no("unsupported_grant_type")),
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
    Extension(SignedIn { who: caller, .. }): Extension<SignedIn>,
    Json(r): Json<Revoke>,
) -> Reply<Revoked> {
    let caller = caller.user().to_owned();
    // Names compare trimmed and in lower case, as at sign-in (R111).
    let user = r
        .user
        .map_or_else(|| caller.clone(), |u| u.trim().to_lowercase());
    if user != caller && !s.tokens().is_admin(&caller, &s.config.admins) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("{caller} is not an admin; only an admin revokes another person"),
        ));
    }
    let mark = s.tokens_changes.load(Ordering::SeqCst);
    let sign_ins = s.tokens_change().revoke_user(&user);
    s.save_tokens_since(mark).await.map_err(|error| {
        tracing::error!("the token store was not saved: {error}");
        (StatusCode::SERVICE_UNAVAILABLE, error.to_string())
    })?;
    tracing::info!(%caller, %user, sign_ins, "revoked");
    Ok(Json(Revoked { user, sign_ins }))
}

/// Refuses a caller who is not an admin, with 403.
fn admin_only(s: &Server, caller: &Who, what: &str) -> Result<(), (StatusCode, String)> {
    let caller = caller.user();
    if s.tokens().is_admin(caller, &s.config.admins) {
        return Ok(());
    }
    Err((
        StatusCode::FORBIDDEN,
        format!("{caller} is not an admin; only an admin can {what}"),
    ))
}

/// Saves the token store after a change, or replies 503.
async fn saved(s: &Server, mark: u64) -> Result<(), (StatusCode, String)> {
    s.save_tokens_since(mark).await.map_err(|error| {
        tracing::error!("the token store was not saved: {error}");
        (StatusCode::SERVICE_UNAVAILABLE, error.to_string())
    })
}

/// Adds a member. Only an admin can.
async fn invite(
    AxumState(s): AxumState<Shared>,
    Extension(SignedIn { who: caller, .. }): Extension<SignedIn>,
    Json(r): Json<Invite>,
) -> Reply<Invited> {
    admin_only(&s, &caller, "invite a person")?;
    let mark = s.tokens_changes.load(Ordering::SeqCst);
    let email = s
        .tokens_change()
        .invite(&r.email)
        .map_err(|e| bad_request(e.to_string()))?;
    saved(&s, mark).await?;
    tracing::info!(%caller, %email, "invited");
    let address = s.config.public_url.clone();
    Ok(Json(Invited { email, address }))
}

/// Removes a member and ends each sign-in of that person. Only an admin
/// can.
async fn remove(
    AxumState(s): AxumState<Shared>,
    Extension(SignedIn { who: caller, .. }): Extension<SignedIn>,
    Json(r): Json<Remove>,
) -> Reply<Removed> {
    admin_only(&s, &caller, "remove a person")?;
    let mark = s.tokens_changes.load(Ordering::SeqCst);
    let (email, sign_ins) = s.tokens_change().remove(&r.email).map_err(bad_request)?;
    saved(&s, mark).await?;
    tracing::info!(%caller, %email, sign_ins, "removed");
    Ok(Json(Removed { email, sign_ins }))
}

/// Makes a person an admin, or an admin a member again. Only the owner
/// can.
async fn admin(
    AxumState(s): AxumState<Shared>,
    Extension(SignedIn { who: caller, .. }): Extension<SignedIn>,
    Json(r): Json<SetAdmin>,
) -> Reply<AdminSet> {
    owner_only(&s, &caller, "adds or removes an admin")?;
    let mark = s.tokens_changes.load(Ordering::SeqCst);
    let email = if r.admin {
        s.tokens_change()
            .add_admin(&r.email)
            .map_err(|e| bad_request(e.to_string()))?
    } else {
        s.tokens_change()
            .remove_admin(&r.email)
            .map_err(bad_request)?
    };
    saved(&s, mark).await?;
    tracing::info!(%caller, %email, admin = r.admin, "admin set");
    Ok(Json(AdminSet {
        email,
        admin: r.admin,
    }))
}

/// Refuses a caller who is not the owner, with 403. On a riff with no
/// owner, the text names `riff owner --take` (01M3Q63NNC6SC03BFCG80M7B4D).
fn owner_only(s: &Server, caller: &Who, what: &str) -> Result<(), (StatusCode, String)> {
    let tokens = s.tokens();
    if tokens.owner().is_none() {
        return Err((StatusCode::FORBIDDEN, NO_OWNER.into()));
    }
    if tokens.is_owner(caller.user()) {
        return Ok(());
    }
    Err((
        StatusCode::FORBIDDEN,
        format!("{} is not the owner; only the owner {what}", caller.user()),
    ))
}

/// An admin asks for the owner role (01M3N7K3ZAZFGABN7032AYJWEM). The
/// server posts the note, and tells each live lead of the owner. The
/// owner gets a reply that names the owner as the admin, with no note
/// (01M3WRJAFS6W3J2ZRJ6XSW3SB5).
async fn take_owner(
    AxumState(s): AxumState<Shared>,
    Extension(SignedIn { who: caller, .. }): Extension<SignedIn>,
    Json(TakeOwner {}): Json<TakeOwner>,
) -> Reply<OwnerAsked> {
    admin_only(&s, &caller, "take the owner role")?;
    // The owner is the owner already: no change of the store, no note
    // and no message.
    let already = |owner: String| OwnerAsked {
        admin: owner.clone(),
        owner: Some(owner),
        answer_secs: 0,
    };
    if s.tokens().is_owner(caller.user()) {
        let owner = s.tokens().owner().unwrap_or_default().to_owned();
        return Ok(Json(already(owner)));
    }
    let answer = s.config.owner_role.answer;
    let mark = s.tokens_changes.load(Ordering::SeqCst);
    let took = s
        .tokens_change()
        .take_owner(caller.user(), &s.config.admins, answer, Instant::now())
        .map_err(|why| (StatusCode::CONFLICT, why))?;
    saved(&s, mark).await?;
    let user = caller.user();
    let (reply, news, tell) = match took {
        Took::Already { owner } => return Ok(Json(already(owner))),
        Took::Owner { owner } => {
            let news = owner::took_news(user, &owner);
            let reply = OwnerAsked {
                admin: owner,
                owner: None,
                answer_secs: 0,
            };
            (reply, news, None)
        }
        Took::Asked { owner, admin } => {
            let news = owner::asked_news(user, &admin, &owner, answer);
            let tell = s.tokens().user_of_email(&owner).map(str::to_owned);
            let reply = OwnerAsked {
                admin,
                owner: Some(owner),
                answer_secs: answer.as_secs(),
            };
            (reply, news, tell)
        }
    };
    tracing::info!(%caller, "{news}");
    s.announce(&news, tell.as_slice()).await;
    Ok(Json(reply))
}

/// The owner keeps the owner role that an admin asks for
/// (01M3N7K41N03P26BEFFNX5617K). The server posts the note, and tells
/// each live lead of the admin.
async fn deny_owner(
    AxumState(s): AxumState<Shared>,
    Extension(SignedIn { who: caller, .. }): Extension<SignedIn>,
    Json(DenyOwner {}): Json<DenyOwner>,
) -> Reply<OwnerDenied> {
    owner_only(&s, &caller, "denies the owner role")?;
    let mark = s.tokens_changes.load(Ordering::SeqCst);
    let (owner, admin) = {
        let mut tokens = s.tokens_change();
        let admin = tokens
            .deny_owner(caller.user())
            .map_err(|why| (StatusCode::CONFLICT, why))?;
        (tokens.owner().unwrap_or_default().to_owned(), admin)
    };
    saved(&s, mark).await?;
    let news = owner::denied_news(caller.user(), &owner, &admin);
    tracing::info!(%caller, "{news}");
    let tell = s.tokens().user_of_email(&admin).map(str::to_owned);
    s.announce(&news, tell.as_slice()).await;
    Ok(Json(OwnerDenied { owner, admin }))
}

/// Passes the owner role to a member or an admin. Only the owner can.
/// It ends a request for the owner role that waits.
async fn pass_owner(
    AxumState(s): AxumState<Shared>,
    Extension(SignedIn { who: caller, .. }): Extension<SignedIn>,
    Json(r): Json<PassOwner>,
) -> Reply<OwnerPassed> {
    owner_only(&s, &caller, "passes the owner role")?;
    let mark = s.tokens_changes.load(Ordering::SeqCst);
    let (owner, admin) = {
        let mut tokens = s.tokens_change();
        let admin = tokens.owner().unwrap_or_default().to_owned();
        let owner = tokens
            .pass_owner(&r.email, &s.config.admins)
            .map_err(bad_request)?;
        (owner, admin)
    };
    saved(&s, mark).await?;
    tracing::info!(%caller, %owner, "owner passed");
    Ok(Json(OwnerPassed { owner, admin }))
}

/// Shows who may join the riff: each person once, with the highest
/// role.
async fn members(
    AxumState(s): AxumState<Shared>,
    Extension(_): Extension<SignedIn>,
    Json(Members {}): Json<Members>,
) -> Json<MembersReply> {
    let (owner, admins, members) = s.tokens().roles(&s.config.admins);
    Json(MembersReply {
        owner,
        admins,
        members,
        allowed_domains: s
            .config
            .provider
            .as_ref()
            .map(|p| p.allowed_domains.clone())
            .unwrap_or_default(),
    })
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
    let refusal = match checked {
        Ok(user) => {
            request.extensions_mut().insert(user);
            return next.run(request).await;
        }
        Err(refusal) => refusal,
    };
    let challenge = s.config.challenge(refusal.as_ref());
    let code = match &refusal {
        None => DeniedCode::NoToken,
        Some(refusal) if refusal.code == Refusal::TOKEN => DeniedCode::BadToken,
        Some(_) => DeniedCode::BadProof,
    };
    // The server reads the body only now, after the refusal, to name
    // the caller in the line (RID_DENIED).
    let named = Named::in_request(request).await;
    trace::denied(&path, named.as_ref(), code);
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, challenge)],
    )
        .into_response()
}

/// Refuses a call from a `riff` of a version that this server cannot
/// talk to, or that names no build (01M3MX1E65XGWDZ062PQ9YXQ5T), with
/// the line `denied` and the code `old_build` (RID_DENIED). A `riff`
/// of the line of this server, or of the line before, goes on
/// (01M3MX1DYY6AVDW946NR0B9T2C, 01M3MX1E1EY1M7JGNCN6FCEVQK). The OAuth
/// metadata, `/v1/token` and `/v1/sign-in` stay open to each client
/// (01M3MX4V43SF2XFCZWANHD19WV).
async fn check_build(request: Request, next: Next) -> Response {
    let this = Build::this();
    let riff = Build::from_header(request.headers().get(build::HEADER).map(|v| v.as_bytes()));
    if riff.as_ref().is_some_and(|r| build::compatible(r, &this)) {
        return next.run(request).await;
    }
    let mismatch = Mismatch {
        riff,
        server: Some(this),
        seen: None,
    };
    let path = request.uri().path().to_owned();
    let named = Named::in_request(request).await;
    trace::denied(&path, named.as_ref(), DeniedCode::OldBuild);
    (StatusCode::CONFLICT, mismatch.to_string()).into_response()
}

/// Names the build of this server in each reply
/// (01M3JEE7P46GWXR1BD4Q1TTSGN).
async fn stamp_build(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    if let Ok(value) = header::HeaderValue::from_str(&Build::this().to_string()) {
        response.headers_mut().insert(build::HEADER, value);
    }
    response
}

/// Replies 503 to each call while the server does not serve (R139,
/// R140).
async fn gate(AxumState(s): AxumState<Shared>, request: Request, next: Next) -> Response {
    if s.serving() {
        return next.run(request).await;
    }
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::RETRY_AFTER, "1")],
        "riff-server does not serve now. Try again.",
    )
        .into_response()
}

async fn resource_metadata(AxumState(s): AxumState<Shared>) -> Json<ResourceMetadata> {
    Json(s.config.resource_metadata())
}

async fn server_metadata(AxumState(s): AxumState<Shared>) -> Json<ServerMetadata> {
    Json(s.config.server_metadata())
}

/// An OAuth error reply with only its code.
fn no(error: &str) -> TokenError {
    TokenError {
        error: error.into(),
        ..TokenError::default()
    }
}

/// Swaps a refresh token for a new pair.
async fn refresh(
    s: &Server,
    r: &TokenRequest,
    proof: &dpop::Proof,
) -> Result<TokenReply, TokenError> {
    let token = r.refresh_token.as_deref().unwrap_or_default();
    if !s.tokens().knows_refresh(token, &proof.jkt) {
        return Err(no("invalid_grant"));
    }
    s.tokens_written().await?;
    s.first_use(proof).map_err(|_| no("invalid_dpop_proof"))?;
    s.tokens_change()
        .refresh(token, &proof.jkt, Instant::now())
        .map_err(|_| no("invalid_grant"))
}

/// Swaps an ID token of the provider for a first pair of riff tokens.
async fn exchange(
    s: &Server,
    r: &TokenRequest,
    proof: &dpop::Proof,
) -> Result<TokenReply, TokenError> {
    let provider = s
        .config
        .provider
        .as_ref()
        .ok_or_else(|| no("unsupported_grant_type"))?;
    let id_token = r
        .subject_token
        .as_ref()
        .ok_or_else(|| no("invalid_request"))?;
    let identity = provider.sign_in(&s.http, id_token).await.map_err(|e| {
        tracing::info!("sign-in refused: {e}");
        no("invalid_grant")
    })?;
    s.first_use(proof).map_err(|_| no("invalid_dpop_proof"))?;
    let pair = s
        .tokens_change()
        .admit(
            &identity.email,
            identity.allowed_domain,
            &s.config.admins,
            &proof.jkt,
            Instant::now(),
        )
        .map_err(|e| {
            tracing::info!("sign-in refused for {}: {e}", identity.email);
            // The person is not a member, another email holds the USER
            // (R209), or the email gives no USER (R208). Each refuses the
            // person, who must read why.
            TokenError {
                error: "access_denied".into(),
                error_description: Some(e.to_string()),
            }
        })?;
    tracing::info!("{} signed in as {}", identity.email, pair.user);
    Ok(pair)
}

/// Swaps a person access token for a session access token (R19). The
/// token store keeps nothing of a session token, so the swap counts no
/// change of it (01M3WFVAB44T8EP4QZD4KS7DRF).
async fn for_session(
    s: &Server,
    r: &TokenRequest,
    proof: &dpop::Proof,
) -> Result<TokenReply, TokenError> {
    let (Some(token), Some(session)) = (&r.subject_token, &r.session) else {
        return Err(no("invalid_request"));
    };
    let now = Instant::now();
    if s.tokens().caller(token, &proof.jkt, now).is_err() {
        return Err(no("invalid_grant"));
    }
    s.tokens_written().await?;
    s.first_use(proof).map_err(|_| no("invalid_dpop_proof"))?;
    s.tokens()
        .for_session(token, &proof.jkt, session, now)
        .map_err(|_| no("invalid_grant"))
}

/// Names the sign-in provider, for `riff login`.
async fn sign_in_config(AxumState(s): AxumState<Shared>) -> Reply<SignInConfig> {
    match &s.config.provider {
        Some(provider) => Ok(Json(provider.config(s.tokens().riff_id()))),
        None => Err(not_found("this riff-server has no sign-in provider".into())),
    }
}

#[derive(Deserialize)]
struct WatchQuery {
    uri: SessionUri,
}

/// Streams the wakes for one session. The open stream is no sign of life
/// (01M3WG240PNMQYZ7TX6Z7ZF6M9). It gives only the wakes of the session, in threads
/// that it may read ([`state::may_read`]). The start and the end of the
/// stream are signals.
async fn watch(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Query(q): Query<WatchQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let rx = s.engine.wakes();
    let caller = admit(&s, &proof, &q.uri)?;
    s.engine.signal(&caller, Signal::WatchStarted).await?;
    let guard = WatchGuard {
        engine: s.engine.clone(),
        who: q.uri.who().clone(),
    };
    let missed = s.engine.read(|state| state.missed(q.uri.who()));
    let live = BroadcastStream::new(rx).filter_map(move |event| {
        let _alive = &guard;
        let wake = match event {
            Ok((to, wake)) if to == guard.who && may_read(&to, &wake.thread) => Some(wake),
            _ => None,
        };
        std::future::ready(wake)
    });
    let stream = futures::stream::iter(missed)
        .chain(live)
        .filter_map(|wake| std::future::ready(Event::default().json_data(wake).ok().map(Ok)))
        .take_until(s.stopping());
    Ok(Sse::new(opened(stream)).keep_alive(KeepAlive::default()))
}

/// Marks the session as stopped when its watch stream closes.
struct WatchGuard {
    engine: Engine,
    who: Who,
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        self.engine.watch_ended(&self.who);
    }
}

#[derive(Deserialize)]
struct TailQuery {
    /// The caller: a session, or a person.
    uri: SessionUri,
    thread: ThreadName,
}

/// Streams each new message in one thread (R27). The caller acts as its
/// token, and gets no direct thread of two other sessions: 404
/// ([`state::may_read`]).
async fn tail_thread(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Query(q): Query<TailQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let caller = admit(&s, &proof, &q.uri)?;
    s.engine.peek(&caller, |_| ())?;
    if !may_read(q.uri.who(), &q.thread) {
        return Err(not_found(format!("no thread named {}", q.thread)));
    }
    let stream = BroadcastStream::new(s.engine.tail()).filter_map(move |event| {
        let event = match event {
            Ok(tailed) if tailed.thread == q.thread => Event::default().json_data(tailed).ok(),
            _ => None,
        };
        std::future::ready(event.map(Ok))
    });
    let stream = stream.take_until(s.stopping());
    Ok(Sse::new(opened(stream)).keep_alive(KeepAlive::default()))
}

/// Puts the comment `: ready` first in an event stream, so that the
/// stream sends its first bytes when it opens
/// (01M3QA6TDF6FB5PH8E5V7HCYDQ). A front end such as Cloud Run holds a
/// reply until its first body byte. Without the comment, the connect
/// waits for the first keep-alive, 15 seconds. A client reads only the
/// `data:` lines, so it skips the comment.
///
/// ```
/// use futures::StreamExt;
///
/// let events = riff_server::opened(futures::stream::empty());
/// assert_eq!(futures::executor::block_on(events.count()), 1);
/// ```
pub fn opened<S>(stream: S) -> impl Stream<Item = Result<Event, Infallible>>
where
    S: Stream<Item = Result<Event, Infallible>>,
{
    futures::stream::once(std::future::ready(Ok(Event::default().comment("ready")))).chain(stream)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// The position, the time and the build of a checkpoint, for the facts.
fn checkpoint_facts(checkpoint: &checkpoint::Checkpoint) -> CheckpointFacts {
    CheckpointFacts {
        position: checkpoint.state.position,
        written_at_ms: checkpoint.written_at_ms,
        build: checkpoint.build.clone(),
    }
}

/// The memory that this process uses now, in bytes: the `VmRSS` line of
/// `/proc/self/status`. `None` on a system with no such file.
fn memory_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find_map(|l| l.strip_prefix("VmRSS:"))?;
    let kb: u64 = line.trim().strip_suffix("kB")?.trim().parse().ok()?;
    Some(kb * 1024)
}

fn bad_request(message: String) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message)
}

fn not_found(message: String) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Routed;
    use crate::store::Memory;
    use futures::future::BoxFuture;
    use riff_core::record::{Change, Line};
    use std::time::Duration;
    use tokio::time::sleep;

    /// A memory store whose chunk writes can wait, or fail.
    #[derive(Default)]
    struct Gated {
        store: Memory,
        /// True: each chunk write waits until it is false.
        hold: AtomicBool,
        open: tokio::sync::Notify,
        /// The number of chunk writes that started.
        tries: AtomicU64,
        /// Each chunk write fails with this error.
        fail: Mutex<Option<StoreError>>,
        /// An object that another instance saves at the next write of
        /// the lease: its name and its bytes.
        at_lease: Mutex<Option<(String, Vec<u8>)>>,
    }

    impl Gated {
        fn release(&self) {
            self.hold.store(false, Ordering::SeqCst);
            self.open.notify_waiters();
        }

        /// Waits until `n` chunk writes started.
        async fn tried(&self, n: u64) {
            while self.tries.load(Ordering::SeqCst) < n {
                sleep(Duration::from_millis(1)).await;
            }
        }

        /// The records of each chunk, in order.
        async fn chunks(&self) -> Vec<Vec<Change>> {
            let mut chunks = Vec::new();
            for name in self.store.list(log::LOG).await.unwrap() {
                let bytes = self.store.load(&name).await.unwrap().unwrap().bytes;
                let (_, lines) = log::decode(&bytes).unwrap();
                let changes = lines
                    .into_iter()
                    .map(|line| match line {
                        Line::Record(record) => record.change,
                        Line::Unknown { .. } => panic!("a known kind"),
                    })
                    .collect();
                chunks.push(changes);
            }
            chunks
        }
    }

    impl Store for Gated {
        fn load<'a>(
            &'a self,
            name: &'a str,
        ) -> BoxFuture<'a, Result<Option<store::Loaded>, StoreError>> {
            self.store.load(name)
        }

        fn list<'a>(&'a self, prefix: &'a str) -> BoxFuture<'a, Result<Vec<String>, StoreError>> {
            self.store.list(prefix)
        }

        fn save<'a>(
            &'a self,
            name: &'a str,
            bytes: Vec<u8>,
            known: Option<Version>,
        ) -> BoxFuture<'a, Result<Version, StoreError>> {
            Box::pin(async move {
                if name.starts_with(log::LOG) {
                    self.tries.fetch_add(1, Ordering::SeqCst);
                    while self.hold.load(Ordering::SeqCst) {
                        let open = self.open.notified();
                        if !self.hold.load(Ordering::SeqCst) {
                            break;
                        }
                        open.await;
                    }
                    let fail = self.fail.lock().unwrap().clone();
                    if let Some(error) = fail {
                        return Err(error);
                    }
                }
                let other = self
                    .at_lease
                    .lock()
                    .unwrap()
                    .take_if(|_| name == store::LEASE);
                if let Some((other, bytes)) = other {
                    self.store.save(&other, bytes, None).await?;
                }
                self.store.save(name, bytes, known).await
            })
        }

        fn delete<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<(), StoreError>> {
            self.store.delete(name)
        }
    }

    fn mike() -> SessionUri {
        "riff://mike@pangolin/como-technologies/riff?session=a"
            .parse()
            .unwrap()
    }

    fn brett() -> SessionUri {
        "riff://brett@heron/como-technologies/riff?session=b"
            .parse()
            .unwrap()
    }

    fn config() -> Config {
        let mut config = Config::default();
        config.lease.wait = Duration::from_millis(10);
        config
    }

    /// A config that writes a checkpoint each `every_records` records,
    /// as the build `build`.
    fn with_checkpoints(every_records: u64, build: &str) -> Config {
        let mut config = config();
        config.checkpoint.every_records = every_records;
        config.checkpoint.check_every = Duration::from_millis(50);
        config.checkpoint.build = build.into();
        config
    }

    /// The names of the checkpoints in `store`.
    async fn checkpoints(store: &Gated) -> Vec<String> {
        store.store.list(checkpoint::CHECKPOINT).await.unwrap()
    }

    /// Sends a command as a client with no token does: through the one
    /// path. The future owns its engine, so a test can spawn it, and
    /// drop it.
    fn send<C: Routed>(
        service: &Service,
        command: C,
    ) -> impl Future<Output = Result<<C as Call>::Reply, Failed>> + use<C> {
        let engine = service.0.engine.clone();
        async move {
            let call = engine.authenticate(None, command)?;
            engine.dispatch(call).await
        }
    }

    /// Posts `n` messages of mike, and waits until they are written.
    async fn post_n(service: &Service, n: usize, prefix: &str) {
        for i in 0..n {
            send(service, post_body(&mike(), &format!("{prefix}{i}")))
                .await
                .unwrap();
        }
    }

    /// The state that the log gives in `service`, as a state with no
    /// sessions.
    fn log_state(service: &Service) -> State {
        let snapshot = service
            .0
            .engine
            .read(|state| state.snapshot(Instant::now(), 0));
        State::load(Some(snapshot), [], Instant::now(), 0)
    }

    /// The position of the last record of `service`: in the queue, or
    /// written.
    fn position(service: &Service) -> u64 {
        service.0.engine.read(State::position)
    }

    fn claim_of(me: &SessionUri, item: &str) -> Claim {
        Claim {
            me: me.clone(),
            thread: mike().default_thread().unwrap(),
            item: item.into(),
        }
    }

    /// A server on `store` with mike and brett in a running riff.
    async fn running(store: Arc<Gated>) -> Service {
        running_with(config(), store).await
    }

    /// A server with `config` on `store`, with mike and brett in a
    /// running riff.
    async fn running_with(config: Config, store: Arc<Gated>) -> Service {
        let service = Service::load(config, store).await.unwrap();
        for me in [mike(), brett()] {
            let register = Register { me, worker: false };
            send(&service, register).await.unwrap();
        }
        send(&service, Resume { me: mike() }).await.unwrap();
        service
    }

    fn post_body(me: &SessionUri, body: &str) -> Post {
        Post::new(me, me.default_thread(), vec![], body)
    }

    async fn claims(service: &Service, me: &SessionUri) -> Vec<String> {
        let request = WhoRequest {
            me: me.clone(),
            all: false,
        };
        let who = who(AxumState(service.0.clone()), Proof::none(), Json(request))
            .await
            .unwrap();
        who.sessions
            .iter()
            .find(|s| s.uri.who() == mike().who())
            .map(|s| s.uri.claims().to_vec())
            .unwrap_or_default()
    }

    #[tokio::test(start_paused = true)]
    async fn two_posts_that_wait_for_a_write_go_in_one_chunk() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let before = store.chunks().await.len();
        store.hold.store(true, Ordering::SeqCst);
        let first = tokio::spawn(send(&service, post_body(&mike(), "one")));
        store.tried(before as u64 + 1).await;
        let second = tokio::spawn(send(&service, post_body(&mike(), "two")));
        let third = tokio::spawn(send(&service, post_body(&brett(), "three")));
        sleep(Duration::from_millis(50)).await;
        assert!(!first.is_finished(), "a reply comes after the write");
        store.release();
        for task in [first, second, third] {
            let _ = task.await.unwrap().unwrap();
        }
        let chunks = store.chunks().await;
        let posts = |chunk: &Vec<Change>| {
            chunk
                .iter()
                .filter(|c| matches!(c, Change::Posted(_)))
                .count()
        };
        assert_eq!(chunks.len(), before + 2);
        assert_eq!(posts(&chunks[before]), 1);
        assert_eq!(posts(&chunks[before + 1]), 2, "{:?}", chunks[before + 1]);
    }

    #[tokio::test(start_paused = true)]
    async fn who_shows_a_claim_only_after_its_write() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let tries = store.tries.load(Ordering::SeqCst);
        store.hold.store(true, Ordering::SeqCst);
        let task = tokio::spawn(send(&service, claim_of(&mike(), "issue-7")));
        store.tried(tries + 1).await;
        assert!(claims(&service, &brett()).await.is_empty());
        store.release();
        let reply = task.await.unwrap().unwrap();
        assert_eq!(reply.holder.claims(), ["issue-7"]);
        assert_eq!(claims(&service, &brett()).await, ["issue-7"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_second_claim_waits_for_the_write_of_the_first() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let tries = store.tries.load(Ordering::SeqCst);
        store.hold.store(true, Ordering::SeqCst);
        let first = tokio::spawn(send(&service, claim_of(&mike(), "issue-7")));
        store.tried(tries + 1).await;
        // A retry of the client: it makes no record, and its reply comes
        // after the write of the queued claim.
        let second = tokio::spawn(send(&service, claim_of(&mike(), "issue-7")));
        sleep(Duration::from_millis(50)).await;
        assert!(!second.is_finished(), "no reply before the write");
        store.release();
        first.await.unwrap().unwrap();
        let reply = second.await.unwrap().unwrap();
        assert_eq!(reply.holder.claims(), ["issue-7"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_skipped_record_at_the_end_of_the_log_keeps_its_position() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let last = position(&service);
        drop(service);
        // A later build wrote a record of a kind that this build does not
        // know.
        let later = format!(
            "{{\"format\":1,\"first\":{n}}}\n{{\"position\":{n},\"written_at_ms\":0,\"change\":{{\"reacted\":{{}}}}}}\n",
            n = last + 1
        );
        store
            .store
            .save(&log::chunk_name(last + 1), later.into_bytes(), None)
            .await
            .unwrap();

        let next = Service::load(config(), store.clone()).await.unwrap();
        assert_eq!(position(&next), last + 1);
        send(&next, post_body(&mike(), "after")).await.unwrap();
        drop(next);
        let again = Service::load(config(), store).await.unwrap();
        let read = Read {
            after: None,
            me: brett(),
            thread: mike().default_thread().unwrap(),
            all: true,
        };
        assert_eq!(read_messages(&again, read).await, ["after"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_new_server_starts_from_the_checkpoint_and_the_chunks_after_it() {
        let store = Arc::new(Gated::default());
        let config = with_checkpoints(5, "0.8.0");
        let service = running_with(config.clone(), store.clone()).await;
        post_n(&service, 12, "m").await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(!checkpoints(&store).await.is_empty());
        post_n(&service, 2, "late").await;
        let before = log_state(&service);
        drop(service);

        // The delete took the first chunks, so a full replay from the
        // start is not possible: the start uses the checkpoint.
        assert!(log::replay(&store.store).await.is_err());
        let next = Service::load(config, store.clone()).await.unwrap();
        assert!(next.0.engine.read(|state| state.same_log_state(&before)));
        let read = Read {
            after: None,
            me: brett(),
            thread: mike().default_thread().unwrap(),
            all: true,
        };
        let messages = read_messages(&next, read).await;
        assert_eq!(messages.len(), 14);
        assert_eq!(messages.last().unwrap(), "late1");
    }

    #[tokio::test(start_paused = true)]
    async fn new_build_old_build_new_build_loses_no_record() {
        let store = Arc::new(Gated::default());
        let new = with_checkpoints(3, "0.9.0");
        let service = running_with(new.clone(), store.clone()).await;
        post_n(&service, 4, "new").await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let written = checkpoints(&store).await;
        assert!(!written.is_empty());
        drop(service);

        // A rollback: the old build writes records, but no checkpoint
        // past the checkpoint of the later version.
        let old = Service::load(with_checkpoints(3, "0.8.0"), store.clone())
            .await
            .unwrap();
        post_n(&old, 6, "old").await;
        send(&old, claim_of(&brett(), "issue-9")).await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(checkpoints(&store).await, written);
        let before = log_state(&old);
        drop(old);

        // The new build again: each record of the old build is there.
        let again = Service::load(new, store.clone()).await.unwrap();
        assert!(again.0.engine.read(|state| state.same_log_state(&before)));
        let claims = again
            .0
            .engine
            .read(|state| state.uri(brett().who(), Instant::now()).claims().to_vec());
        assert_eq!(claims, ["issue-9"]);
        // The new build writes checkpoints again.
        post_n(&again, 3, "again").await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_ne!(checkpoints(&store).await, written);
    }

    #[tokio::test(start_paused = true)]
    async fn a_build_that_skipped_a_record_writes_no_checkpoint_past_it() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let last = position(&service);
        drop(service);
        let later = format!(
            "{{\"format\":1,\"first\":{n}}}\n{{\"position\":{n},\"written_at_ms\":0,\"change\":{{\"reacted\":{{}}}}}}\n",
            n = last + 1
        );
        store
            .store
            .save(&log::chunk_name(last + 1), later.into_bytes(), None)
            .await
            .unwrap();

        let next = Service::load(with_checkpoints(1, "0.8.0"), store.clone())
            .await
            .unwrap();
        post_n(&next, 3, "m").await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(checkpoints(&store).await.is_empty());
        // `riff server` says why (01M3TJWJ12WEDCXW3W0529KRP2).
        let facts = next.0.server_facts();
        assert_eq!(facts.skipped_records, 1);
        assert_eq!(
            facts.no_checkpoint,
            Some(format!(
                "this build skipped the record at position {}",
                last + 1
            ))
        );
    }

    /// A rollback: the later build writes a checkpoint after the load of
    /// the older build, and before the older build takes the lease
    /// (01M3TJWJC08ZR5TWA1Y9CDE0QM).
    #[tokio::test(start_paused = true)]
    async fn a_checkpoint_of_a_later_build_at_the_lease_write_blocks_the_older_build() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        post_n(&service, 2, "new").await;
        let snapshot = service
            .0
            .engine
            .read(|state| state.snapshot(Instant::now(), 0));
        drop(service);
        assert!(checkpoints(&store).await.is_empty());
        let later = checkpoint::Checkpoint::new("0.9.0", 5, snapshot);
        let name = checkpoint::name(later.state.position, later.written_at_ms);
        *store.at_lease.lock().unwrap() = Some((name.clone(), checkpoint::encode(&later)));

        let old = Service::load(with_checkpoints(3, "0.8.0"), store.clone())
            .await
            .unwrap();
        assert_eq!(checkpoints(&store).await, std::slice::from_ref(&name));
        post_n(&old, 6, "old").await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        // The older build wrote no checkpoint past it.
        assert_eq!(checkpoints(&store).await, [name]);
        let facts = old.0.server_facts();
        let why = facts.no_checkpoint.unwrap();
        assert!(why.contains("comes from the later version 0.9.0"), "{why}");
        let newest = facts.checkpoint.unwrap();
        assert_eq!(
            (newest.build.as_str(), newest.position),
            ("0.9.0", later.state.position)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn me_shows_a_claim_only_after_its_write() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let tries = store.tries.load(Ordering::SeqCst);
        store.hold.store(true, Ordering::SeqCst);
        let task = tokio::spawn(send(&service, claim_of(&mike(), "issue-7")));
        store.tried(tries + 1).await;
        let claims = || async {
            let query = Query(WatchQuery { uri: mike() });
            let reply = me(AxumState(service.0.clone()), Proof::none(), query).await.unwrap();
            let session = reply.session.clone().expect("the server knows mike");
            session.uri.claims().to_vec()
        };
        assert!(claims().await.is_empty(), "no claim before the write");
        store.release();
        task.await.unwrap().unwrap();
        assert_eq!(claims().await, ["issue-7"]);
    }

    #[tokio::test(start_paused = true)]
    async fn the_facts_show_the_log_the_checkpoint_and_the_counts() {
        let store = Arc::new(Gated::default());
        let service = running_with(with_checkpoints(3, "0.8.0"), store.clone()).await;
        post_n(&service, 4, "m").await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        post_n(&service, 1, "late").await;

        let facts = service.0.server_facts();
        assert_eq!((&facts.not_serving, &facts.last_error), (&None, &None));
        assert_eq!(
            facts.position,
            service.0.engine.read(State::written_position)
        );
        assert!(facts.position > 4);
        assert!(
            facts
                .chunk_written_at_ms
                .is_some_and(|at| at <= facts.now_ms)
        );
        assert!(facts.chunk_write_ms.is_some());
        assert_eq!((facts.write_errors, facts.skipped_records), (0, 0));
        let newest = facts.checkpoint.expect("a checkpoint");
        assert_eq!(newest.build, "0.8.0");
        assert!(newest.position < facts.position && newest.written_at_ms <= facts.now_ms);
        assert_eq!(facts.no_checkpoint, None);
        let chunks = store.store.list(log::LOG).await.unwrap().len();
        assert_eq!(facts.chunks, chunks as u64);
        assert_eq!((facts.sessions, facts.threads), (2, 1));
        assert_eq!(facts.sign_ins, 0);
        assert!(facts.memory_bytes.is_none_or(|bytes| bytes > 0));
        assert!(facts.started_at_ms > 0 && facts.started_at_ms <= facts.now_ms);

        // A new instance counts the chunks of the store, and has the
        // checkpoint of the old one.
        drop(service);
        let next = Service::load(with_checkpoints(3, "0.8.0"), store.clone())
            .await
            .unwrap();
        let facts = next.0.server_facts();
        assert_eq!(facts.chunks, chunks as u64);
        assert_eq!(facts.checkpoint, Some(newest));
        assert_eq!(facts.chunk_written_at_ms, None);
    }

    #[tokio::test(start_paused = true)]
    async fn the_facts_show_the_last_error_and_why_the_server_does_not_serve() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        *store.fail.lock().unwrap() = Some(StoreError::Failed("503 from the bucket".into()));
        let lost = send(&service, post_body(&mike(), "lost")).await;
        assert_eq!(lost.unwrap_err(), Failed::Stopped);
        service.stopped().await;
        let facts = service.0.server_facts();
        let why = facts.not_serving.unwrap();
        assert!(why.starts_with("it stopped for good: the write of the chunk"));
        let error = facts.last_error.unwrap();
        assert!(error.message.contains("failed for good"), "{error:?}");
        assert!(error.message.contains("503 from the bucket"), "{error:?}");
        assert!(facts.write_errors > 3, "{}", facts.write_errors);
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_write_stops_the_instance_and_the_next_replays_without_it() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        send(&service, post_body(&mike(), "kept")).await.unwrap();
        *store.fail.lock().unwrap() = Some(StoreError::Failed("503 from the bucket".into()));
        let started = tokio::time::Instant::now();
        let lost = send(&service, post_body(&mike(), "lost"))
            .await
            .err()
            .unwrap();
        assert_eq!(lost.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(started.elapsed() >= Duration::from_secs(9), "tried again");
        service.stopped().await;
        assert!(store.tries.load(Ordering::SeqCst) > 3);

        *store.fail.lock().unwrap() = None;
        let next = Service::load(config(), store.clone()).await.unwrap();
        let read = Read {
            after: None,
            me: brett(),
            thread: mike().default_thread().unwrap(),
            all: true,
        };
        let messages = read_messages(&next, read).await;
        assert_eq!(messages, ["kept"]);
        // The next instance writes its own chunk after the kept ones.
        send(&next, post_body(&mike(), "new")).await.unwrap();
    }

    async fn read_messages(service: &Service, request: Read) -> Vec<String> {
        let reply = read(AxumState(service.0.clone()), Proof::none(), Json(request))
            .await
            .unwrap();
        reply.messages.iter().map(|m| m.body.clone()).collect()
    }

    #[tokio::test(start_paused = true)]
    async fn a_chunk_that_another_instance_wrote_stops_the_instance_at_once() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let next = position(&service) + 1;
        store
            .store
            .save(&log::chunk_name(next), b"other".to_vec(), None)
            .await
            .unwrap();
        let started = tokio::time::Instant::now();
        let lost = send(&service, post_body(&mike(), "lost"))
            .await
            .err()
            .unwrap();
        assert_eq!(lost.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(started.elapsed() < Duration::from_secs(1), "no retry");
        service.stopped().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_wake_goes_out_after_the_write() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let mut wakes = service.0.engine.wakes();
        let tries = store.tries.load(Ordering::SeqCst);
        store.hold.store(true, Ordering::SeqCst);
        let to = vec![Selector::session("b")];
        let post = Post::new(&mike(), mike().default_thread(), to, "wake up");
        let task = tokio::spawn(send(&service, post));
        store.tried(tries + 1).await;
        sleep(Duration::from_millis(50)).await;
        assert!(wakes.try_recv().is_err(), "no wake before the write");
        store.release();
        let posted = task.await.unwrap().unwrap();
        assert_eq!(posted.woken[0].who(), brett().who());
        let (to, _) = wakes.recv().await.unwrap();
        assert_eq!(&to, brett().who());
    }

    #[tokio::test(start_paused = true)]
    async fn the_writer_ends_with_the_service() {
        let store = Arc::new(Gated::default());
        let service = Service::load(config(), store.clone()).await.unwrap();
        let server = Arc::downgrade(&service.0);
        drop(service);
        sleep(Duration::from_secs(2)).await;
        assert!(server.upgrade().is_none());
    }

    #[tokio::test]
    async fn a_server_with_no_store_keeps_its_log_in_memory() {
        let service = Service::default();
        let me: SessionUri = "riff://mike@pangolin/-?session=a#x".parse().unwrap();
        let register = Register {
            me: me.clone(),
            worker: false,
        };
        send(&service, register).await.unwrap();
        let post = Post::new(&me, Some("t".parse().unwrap()), vec![], "hi");
        send(&service, post).await.unwrap();
        assert!(service.save().await.is_ok());
        let chunks = service.0.log.list(log::LOG).await.unwrap();
        assert!(!chunks.is_empty());
        // The pause of `make_riff`, the join of the thread, the message.
        assert_eq!(position(&service), 3);
    }

    /// The claims of mike, as `who` shows them: the written copy.
    async fn written_claims(service: &Service) -> Vec<String> {
        claims(service, &brett()).await
    }

    /// A call that the client drops loses only its reply: the writer
    /// finishes the command (01M3WRD90WBBCWTDGVQCBR6MNT).
    #[tokio::test(start_paused = true)]
    async fn a_dropped_call_is_in_the_written_copy_and_its_wake_goes_out() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let mut wakes = service.0.engine.wakes();
        let tries = store.tries.load(Ordering::SeqCst);
        store.hold.store(true, Ordering::SeqCst);
        let to = vec![Selector::session("b")];
        let post = Post::new(&mike(), mike().default_thread(), to, "wake up");
        let posting = tokio::spawn(send(&service, post));
        store.tried(tries + 1).await;
        // This claim waits in the queue, behind the chunk of the post.
        let claiming = tokio::spawn(send(&service, claim_of(&mike(), "issue-7")));
        sleep(Duration::from_millis(50)).await;

        // The client drops the two calls while the write waits.
        posting.abort();
        claiming.abort();
        assert!(posting.await.unwrap_err().is_cancelled());
        assert!(claiming.await.unwrap_err().is_cancelled());
        assert!(wakes.try_recv().is_err(), "no wake before the write");
        assert!(written_claims(&service).await.is_empty());

        store.release();
        service.save().await.unwrap();
        // The records are in the written copy.
        assert_eq!(written_claims(&service).await, ["issue-7"]);
        let read = Read {
            after: None,
            me: brett(),
            thread: mike().default_thread().unwrap(),
            all: true,
        };
        assert_eq!(read_messages(&service, read).await, ["wake up"]);
        // The wake went out.
        let (to, wake) = wakes.recv().await.unwrap();
        assert_eq!(&to, brett().who());
        assert_eq!(wake.from.who(), mike().who());
    }

    /// The writer applies the records of a chunk in the order of their
    /// positions, so the written copy is the replay of the log
    /// (01M3WRD90WBBCWTDGVQCBR6MNT).
    #[tokio::test(start_paused = true)]
    async fn a_claim_and_a_release_in_one_chunk_give_the_replay_of_the_log() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let before = store.chunks().await.len();
        let tries = store.tries.load(Ordering::SeqCst);
        store.hold.store(true, Ordering::SeqCst);
        let first = tokio::spawn(send(&service, post_body(&mike(), "one")));
        store.tried(tries + 1).await;
        let release = Release {
            me: mike(),
            thread: mike().default_thread().unwrap(),
            item: "issue-7".into(),
        };
        let claim = tokio::spawn(send(&service, claim_of(&mike(), "issue-7")));
        let release = tokio::spawn(send(&service, release));
        sleep(Duration::from_millis(50)).await;
        store.release();
        first.await.unwrap().unwrap();
        claim.await.unwrap().unwrap();
        release.await.unwrap().unwrap();

        // The claim and the release are in one chunk.
        let chunks = store.chunks().await;
        assert_eq!(chunks.len(), before + 2);
        let kinds: Vec<&str> = chunks[before + 1]
            .iter()
            .map(|change| match change {
                Change::Claimed(_) => "claimed",
                Change::Released(_) => "released",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["claimed", "released"]);

        // The written copy is the replay of the log.
        let records = log::replay(&store.store).await.unwrap().records;
        let replayed = State::replay(records, Instant::now(), 0);
        assert!(
            service
                .0
                .engine
                .read(|state| state.same_log_state(&replayed))
        );
        assert!(written_claims(&service).await.is_empty());
    }

    /// A refused command has an entry in the queue, and waits as each
    /// command does (01M3WRD933ESXF33WDEDFCRFB8). So no refusal tells of
    /// a change that is not in the log.
    #[tokio::test(start_paused = true)]
    async fn a_refusal_as_held_comes_only_after_the_write_of_the_claim_that_holds() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let before = store.chunks().await.len();
        let tries = store.tries.load(Ordering::SeqCst);
        store.hold.store(true, Ordering::SeqCst);
        let first = tokio::spawn(send(&service, claim_of(&mike(), "issue-7")));
        store.tried(tries + 1).await;
        // The claim of mike is only in the pending copy. It refuses brett.
        let second = tokio::spawn(send(&service, claim_of(&brett(), "issue-7")));
        sleep(Duration::from_millis(50)).await;
        assert!(!second.is_finished(), "no refusal before the write");
        assert!(written_claims(&service).await.is_empty());

        store.release();
        first.await.unwrap().unwrap();
        let failed = second.await.unwrap().unwrap_err();
        assert_eq!(failed.status(), StatusCode::CONFLICT);
        let Failed::Refused(refused) = failed else {
            panic!("not a refusal: {failed:?}");
        };
        assert_eq!(refused.code, state::Code::Held);
        assert_eq!(
            refused.reason,
            "mike@pangolin:riff (a) holds issue-7 in como-technologies/riff."
        );
        assert_eq!(written_claims(&service).await, ["issue-7"]);
        // The refused claim made no record.
        let chunks = store.chunks().await;
        assert_eq!(chunks.len(), before + 1);
        assert_eq!(chunks[before].len(), 1);
    }

    /// A status is a signal (01M3WRD97EZJK3AABXECXEY133): it makes no
    /// entry in the queue, so it does not wait for the writer.
    #[tokio::test(start_paused = true)]
    async fn a_status_makes_no_entry_and_does_not_wait_for_the_writer() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let tries = store.tries.load(Ordering::SeqCst);
        store.hold.store(true, Ordering::SeqCst);
        let posting = tokio::spawn(send(&service, post_body(&mike(), "one")));
        store.tried(tries + 1).await;
        let last = position(&service);

        let step = riff_core::wire::Status {
            step: "the tests run".into(),
            blocked: None,
        };
        let set = SetStatus {
            me: brett(),
            status: step.clone(),
        };
        // The reply comes while the write of the post waits.
        let _ = status(AxumState(service.0.clone()), Proof::none(), Json(set))
            .await
            .unwrap();
        assert!(!posting.is_finished());
        assert_eq!(position(&service), last);
        assert!(service.0.engine.take().is_none(), "no entry in the queue");
        let shown = service.0.engine.read(|state| {
            let me = state.me(brett().who(), Instant::now(), now_ms());
            me.unwrap().status.unwrap().status
        });
        assert_eq!(shown, step);

        store.release();
        posting.await.unwrap().unwrap();
    }

    /// A signal of a session that the state does not know first sends
    /// `register` through the one path, and waits for its write
    /// (01M3WRD97EZJK3AABXECXEY133).
    #[tokio::test(start_paused = true)]
    async fn a_signal_of_a_new_session_registers_it_first() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let before = store.chunks().await.len();
        let new: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=c#api"
            .parse()
            .unwrap();
        let reply = alive(
            AxumState(service.0.clone()),
            Proof::none(),
            Json(Alive { me: new.clone() }),
        )
        .await
        .unwrap();
        assert!(!reply.stop);
        // The join of the repository thread is written.
        let chunks = store.chunks().await;
        assert_eq!(chunks.len(), before + 1);
        assert!(matches!(chunks[before][..], [Change::JoinedThread(_)]));
        let known = service
            .0
            .engine
            .read(|state| state.me(new.who(), Instant::now(), now_ms()));
        assert_eq!(known.unwrap().uri.place(), new.place());
    }

    /// The signal of a `register` sets the place and the worker mark of
    /// the session, also when the command makes no record
    /// (01M3WRD97EZJK3AABXECXEY133).
    #[tokio::test(start_paused = true)]
    async fn a_register_sets_the_place_and_the_worker_mark() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let last = position(&service);
        let moved: SessionUri = "riff://brett@heron/como-technologies/riff?session=b#api"
            .parse()
            .unwrap();
        let register = Register {
            me: moved.clone(),
            worker: true,
        };
        send(&service, register).await.unwrap();
        assert_eq!(position(&service), last, "no record");
        let shown = service
            .0
            .engine
            .read(|state| state.me(brett().who(), Instant::now(), now_ms()))
            .unwrap();
        assert!(shown.worker);
        assert_eq!(shown.uri.place(), moved.place());
    }

    /// A worker is never the lead: `permits` refuses the command with
    /// the code `not_allowed`. The engine adds the worker mark of the
    /// caller under the lock (01M3WRD959DYNZHDKP5ZT9Q1C7).
    #[tokio::test(start_paused = true)]
    async fn a_lead_call_of_a_worker_is_refused_as_not_allowed() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let worker: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w1"
            .parse()
            .unwrap();
        let register = Register {
            me: worker.clone(),
            worker: true,
        };
        send(&service, register).await.unwrap();
        let failed = send(&service, Lead { me: worker }).await.unwrap_err();
        assert_eq!(failed.status(), StatusCode::FORBIDDEN);
        let Failed::Refused(refused) = failed else {
            panic!("not a refusal: {failed:?}");
        };
        assert_eq!(refused.code, state::Code::NotAllowed);
        assert!(refused.reason.contains("a worker cannot be the lead"));
    }

    /// The first start of a riff sends `make_riff`: the first record of
    /// the log pauses the riff. A later start adds no record
    /// (01M3WRD99M99PNGP8ME50KC6WS).
    #[tokio::test(start_paused = true)]
    async fn the_first_start_of_a_riff_sends_make_riff() {
        let store = Arc::new(Gated::default());
        let service = Service::load(config(), store.clone()).await.unwrap();
        service.save().await.unwrap();
        let paused = Change::RiffStateSet(riff_core::record::RiffStateSet {
            state: riff_core::wire::RiffState::Paused,
        });
        assert_eq!(store.chunks().await, [[paused]]);
        drop(service);

        let next = Service::load(config(), store.clone()).await.unwrap();
        next.save().await.unwrap();
        assert_eq!(store.chunks().await.len(), 1);
        assert_eq!(position(&next), 1);
    }

    /// A store of a riff-server from before the log is refused, and the
    /// error says why (01M3WRD9DYJWVN1QRBAC3ZVVZD). The server takes no
    /// lease.
    #[tokio::test(start_paused = true)]
    async fn a_store_with_the_old_objects_and_no_log_is_refused() {
        for name in ["sessions", "tokens", "threads/acme%2Fapp"] {
            let store = Memory::default();
            store.save(name, b"{}".to_vec(), None).await.unwrap();
            let loaded = Service::load(config(), Arc::new(store.clone())).await;
            let error = loaded.err().expect("the load fails").to_string();
            assert!(error.contains("from before the log"), "{error}");
            assert!(error.contains("no log"), "{error}");
            assert!(store.load(store::LEASE).await.unwrap().is_none());
        }

        // A store with a log starts, also with an old object next to it.
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        drop(service);
        store
            .store
            .save("sessions", b"{}".to_vec(), None)
            .await
            .unwrap();
        assert!(Service::load(config(), store).await.is_ok());
    }
}
