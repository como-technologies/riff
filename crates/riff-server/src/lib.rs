//! `riff-server`: the central service that sessions connect to.
//!
//! # Design
//!
//! ```text
//!  HTTP handlers ──▶ Engine ──lock──▶ State    see [`engine`], [`state`]
//!        │             │
//!        │             └─ the writer ──▶ wakes channel ──▶ GET /v1/watch streams
//!        │                           └─▶ tail channel  ──▶ GET /v1/tail streams
//!        └──lock──▶ Tokens (Mutex)      see [`token`]: only the sign-ins
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
//! - The people are state of the log (01M3XA875QZ584JBGA37853PWX): who
//!   may join the riff, the roles, and the request for the owner role.
//!   Each change of them is a command, with the one handler: see
//!   [`state::people`]. `POST /v1/revoke` ends each sign-in of a
//!   person. The admins are the owner, the admins that the owner made,
//!   and a setting ([`auth::Config::admins`]). Each admin is named by
//!   verified email (R210). `POST /v1/invite` and `/v1/remove` change
//!   who may join the riff, and `/v1/members` shows it. `POST
//!   /v1/admin` lets the owner make a person an admin, or an admin a
//!   member again. `POST /v1/owner` lets the owner pass the owner
//!   role. `POST /v1/owner/take` lets an admin ask for it, and `POST
//!   /v1/owner/deny` lets the owner keep it.
//! - `POST /v1/log` gives the records of one repository to the owner
//!   and the admins, for `riff audit` (01M3ZWRC11R5M9V1KTF05P240W). It
//!   reads the log from the store, and a post has only its mark
//!   (01M3ZWRC3XBFN8FJDGE8XWZ5EA).
//! - A sign-in has two steps (01M3XA877YZQ649SWB5TN60V5P): the command
//!   `admit` through the engine, which decides if the person may join,
//!   and then the start of the chain in the token store. The end of
//!   the sign-ins of a removed person is an effect of the writer, and
//!   a load drops each sign-in from before the last removal of its
//!   person (01M3XA87A9GGFA89RQXWSKY0V6).
//! - A task looks for idle workers each [`idle::CHECK_EVERY`], and asks
//!   each idle worker past the limit to stop. See [`idle`].
//! - A task looks at the owner role each [`owner::Timing::tick`]: it
//!   sends the command `grant_owner` for a request whose time ended,
//!   and checks the owner: the command `end_owner` ends the role of an
//!   owner who is gone. The note of each change of the role is in the
//!   chunk of its command. See [`owner`].
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
//!   (01M3WRD99M99PNGP8ME50KC6WS). A start on a store with the objects
//!   of a riff-server from before the log, and no log, is the import of
//!   go-live (01M3Z8MRDZEKTXSKZTDTDSCZ3W): see [`import`].
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
//!   writes it when it changed, at most one time each write window
//!   (R127). One lock holds each write: the writes of each sign-in, of
//!   each refresh and of the task of each second wait for the same
//!   window. The window is [`auth::Config::save_every`]. A store that
//!   is busy ([`store::StoreError::Busy`], a 429 of Cloud Storage)
//!   doubles the window, to at most [`BUSY_WINDOW_MOST`] times
//!   `save_every`. A good write sets it back
//!   (01M3ZZQ9TRG9385GRQGM79RCXX). The server knows the
//!   [`store::Version`] of the object. Each write names it, so a write
//!   over the changes of another instance fails (R141). A sign-in gets
//!   its reply only after the write (R128). When that write fails, the
//!   reply is 503, and a task writes the store again. A revoke and a
//!   change of the people get their reply after the write of their
//!   records in the log.
//! - A refresh and a new session token get their reply before the write
//!   (01M3TFG527M04TA7ESM970X3B8). A session token changes nothing in
//!   the object (01M3WFVAB44T8EP4QZD4KS7DRF). So after a crash, the object can be
//!   one generation behind, and [`token::Tokens::refresh`] takes the next
//!   generation as good. While the last write failed, a refresh first
//!   writes the store again, and gets 503 when that write fails too.
//!   While the store is busy, a refresh writes nothing: it goes on
//!   while the last good write is less than one window old, and gets
//!   503 after that (01M3ZZQCEKEYK9CE8MGPM80Z2P).
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
pub mod forge;
pub mod gcs;
pub mod idle;
pub mod import;
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
use riff_core::record::Record;
use riff_core::selector::Selector;
use riff_core::wire::{
    ACCESS_TOKEN_TYPE, Alive, AliveReply, BlockedLook, BlockedLookReply, Call, CheckpointFacts,
    Claim, DenyOwner, End, FactError, Free, Hold, ID_TOKEN_TYPE, Idle, IdleQuery, Invite,
    ItemFacts, Join, Keys, Kind, Lead, Leave, LogQuery, LogReply, MeReply, Members, MembersReply,
    PassOwner, Pause, Person, PlanOff, PlanReply, PlanSeen, PlanShow, Post, Read, ReadReply,
    Register, Release, ReleaseFor, Remove, ResourceMetadata, Resume, Revoke, RiffOwner, RiffQuery,
    RiffReply, ServerFacts, ServerMetadata, SetAdmin, SetBlocked, SetIdle, SetPlan, SetStatus,
    SetStep, SignInConfig, Start, TOKEN_EXCHANGE, TakeOwner, Threads, ThreadsReply, TokenError,
    TokenReply, TokenRequest, Unanswered, WhoReply, WhoRequest,
};
use serde::Deserialize;
use tokio::time::MissedTickBehavior;
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::{Config, Refusal, Replay, SignedIn};
use crate::engine::{Admitted, Authenticated, Engine, Failed, Routed, SignIns, command};
use crate::import::Old;
use crate::lease::Lease;
use crate::oidc::Identity;
use crate::owner::{Check, Checks};
use crate::state::{
    Announce, Code, OwnerChange, Refused, Role, Settings, Signal, Snapshot, State, may_read,
};
use crate::store::{Memory, SIGN_INS, Store, StoreError, Version};
use crate::token::Tokens;
use crate::trace::DeniedCode;
use riff_core::forge::TokenRole;
use riff_core::wire::{
    ForgeAllow, ForgeCheck, ForgeCheckReply, ForgeCreate, ForgeCreateReply, ForgeCreated,
    ForgeCreatedReply, ForgeInstall, ForgeInstallReply, ForgeToken, ForgeTokenReply,
};

/// The least time between two writes of the token store (R127): the
/// default of [`auth::Config::save_every`].
pub const SAVE_EVERY: Duration = Duration::from_secs(1);

/// The write window of the token store grows to at most this many
/// [`auth::Config::save_every`] while the store is busy
/// (01M3ZZQ9TRG9385GRQGM79RCXX).
pub const BUSY_WINDOW_MOST: u32 = 32;

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

/// Reads the objects of a riff-server from before the log, when the
/// store has them and no log: the import of go-live
/// (01M3Z8MRDZEKTXSKZTDTDSCZ3W). The load reads each object before the
/// lease: an object that does not read stops the start, and changes
/// nothing. It reads them again after the wait for the old instance,
/// and imports from that read (01M3ZCDNQY2G9ET537B6SBYCBB).
async fn read_old_objects(store: &dyn Store) -> Result<Option<Old>, StoreError> {
    let Some(old) = Old::read(store).await? else {
        return Ok(None);
    };
    if let Some(bytes) = old.tokens() {
        Tokens::import(bytes, 0, Instant::now(), SystemTime::now())
            .map_err(|e| StoreError::not_valid(store, import::TOKENS, e))?;
    }
    Ok(Some(old))
}

/// The sign-ins of the riff, for the engine: the token store.
struct SignInStore {
    tokens: Arc<Mutex<Tokens>>,
    /// The number of changes to the token store.
    changes: Arc<AtomicU64>,
    needs_sign_in: bool,
    trusted: bool,
}

impl SignInStore {
    fn tokens(&self) -> MutexGuard<'_, Tokens> {
        self.tokens
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

impl SignIns for SignInStore {
    fn needs_sign_in(&self) -> bool {
        self.needs_sign_in
    }

    fn trusted(&self) -> bool {
        self.trusted
    }

    fn keys(&self, user: &str) -> Vec<String> {
        self.tokens().keys(user, Instant::now())
    }

    /// Ends the sign-ins of `user` from before `position`, and keeps
    /// `position` for `user`, in one step under the lock of the store
    /// (01M3XA87A9GGFA89RQXWSKY0V6). The change is counted, so the
    /// server saves the store.
    fn end(&self, user: &str, position: u64) -> usize {
        let ended = self.tokens().end(user, position);
        self.changes.fetch_add(1, Ordering::SeqCst);
        ended
    }
}

struct Server {
    config: Config,
    /// The command engine. It owns the state and its lock.
    engine: Engine,
    tokens: Arc<Mutex<Tokens>>,
    /// The number of changes to the token store.
    tokens_changes: Arc<AtomicU64>,
    /// The number of changes to the token store that are saved.
    tokens_saved: AtomicU64,
    /// How the last writes of the token store went.
    tokens_writes: Mutex<TokenWrites>,
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
    /// The forge tokens of the sessions (#628).
    forge: forge::Forge,
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
    /// True once the server stopped because its lease ended by its age
    /// (01M3X5TPBMF81TDVZ7Q4NVXBQX).
    lease_ended: AtomicBool,
    /// True once the server shuts down: it takes no call, but it still
    /// saves (R129).
    closing: AtomicBool,
}

/// Where a server saves its token store.
struct Saved {
    store: Arc<dyn Store>,
    /// The lease of this instance.
    lease: Arc<Lease>,
    /// The last write of the token store. The lock lets only one write
    /// run at a time.
    written: tokio::sync::Mutex<Written>,
}

/// How the last writes of the token store went, for the window of the
/// next write and for a refresh (01M3TFG527M04TA7ESM970X3B8,
/// 01M3ZZQ9TRG9385GRQGM79RCXX).
#[derive(Clone, Copy, Debug)]
struct TokenWrites {
    /// How the last write failed. `None` after a good write.
    failed: Option<WriteFailed>,
    /// The start of the last good write, or the load of the store.
    good: tokio::time::Instant,
    /// The least time from the start of one write to the start of the
    /// next.
    window: Duration,
}

/// How a write of the token store failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WriteFailed {
    /// The object is over the rate limit of the store
    /// ([`StoreError::Busy`]).
    Busy,
    /// Each other failure.
    Failed,
}

impl TokenWrites {
    /// The writes of a store that holds each change now.
    fn new(every: Duration) -> Self {
        TokenWrites {
            failed: None,
            good: tokio::time::Instant::now(),
            window: every,
        }
    }

    /// Takes the result of the write that started at `started`. A good
    /// write sets the window back to `every`. A busy store doubles the
    /// window, to at most [`BUSY_WINDOW_MOST`] times `every`.
    fn after<T>(
        &mut self,
        result: &Result<T, StoreError>,
        started: tokio::time::Instant,
        every: Duration,
    ) {
        match result {
            Ok(_) => {
                *self = TokenWrites {
                    failed: None,
                    good: started,
                    window: every,
                };
            }
            Err(StoreError::Busy(_)) => {
                self.failed = Some(WriteFailed::Busy);
                self.window = (self.window * 2).min(every * BUSY_WINDOW_MOST);
            }
            Err(_) => self.failed = Some(WriteFailed::Failed),
        }
    }

    /// True while the last good write is less than one window old at
    /// `now`.
    fn fresh(&self, now: tokio::time::Instant) -> bool {
        now.saturating_duration_since(self.good) < self.window
    }
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
        let (who, started) = self
            .tokens()
            .signed_in(token, &proof.jkt, Instant::now())
            .map_err(Refusal::token)?;
        self.first_use(&proof)?;
        Ok(SignedIn {
            who,
            jkt: proof.jkt,
            started,
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

    /// Signs in the person of `identity` on the device key `jkt`: the
    /// one path of the handler `exchange` and of [`Service::admit`]
    /// (01M3XA877YZQ649SWB5TN60V5P). First the command `admit` goes
    /// through the engine: the people of the log decide, and the first
    /// sign-in of a person makes its records. Then the token store
    /// starts the chain, with the position of the log at the check
    /// (01M3XA87A9GGFA89RQXWSKY0V6). The caller saves the token store.
    ///
    /// A stop between the two steps leaves the records, and no chain.
    /// The person signs in again, and `admit` then makes no record.
    async fn sign_in(&self, identity: &Identity, jkt: &str) -> Result<TokenReply, Failed> {
        let admitted = self.engine.sign_in(identity).await?;
        self.tokens_change()
            .start(&admitted.user, jkt, admitted.position, Instant::now())
            .map_err(|ended| Failed::Refused(Refused::new(Code::NotAllowed, ended.to_string())))
    }

    /// Who may join the riff: the one path of the handler `members` and
    /// of [`Service::members`]. It reads the people of the written
    /// copy. Each person shows once, with the highest role
    /// (01M3MN157X8N9QKER1AJEPEJVX).
    fn members(&self) -> MembersReply {
        MembersReply {
            allowed_domains: self
                .config
                .provider
                .as_ref()
                .map(|p| p.allowed_domains.clone())
                .unwrap_or_default(),
            ..self.engine.read(|state| state.people().members())
        }
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
        // At most one write each window: `save_every`, or more while
        // the store is busy (R127, 01M3ZZQ9TRG9385GRQGM79RCXX). Each call
        // that waits for the lock finds its change in this write.
        if let Some(at) = written.at {
            let window = self.token_writes().window;
            tokio::time::sleep_until(at + window).await;
        }
        let (bytes, changes) = {
            let tokens = self.tokens();
            let changes = self.tokens_changes.load(Ordering::SeqCst);
            (tokens.to_bytes(Instant::now(), SystemTime::now()), changes)
        };
        if !self.leased() {
            return Err(StoreError::Failed("the server does not serve now".into()));
        }
        let started = tokio::time::Instant::now();
        written.at = Some(started);
        let result = saved.store.save(SIGN_INS, bytes, written.version).await;
        self.token_writes()
            .after(&result, started, self.config.save_every);
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

    fn token_writes(&self) -> MutexGuard<'_, TokenWrites> {
        self.tokens_writes
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Lets a refresh go on, which gets its reply before the write of
    /// the token store (01M3TFG527M04TA7ESM970X3B8). While the last write
    /// failed, it writes the store again first, and refuses when that
    /// write fails too. So the saved store does not fall more and more
    /// generations behind.
    ///
    /// While the store is busy, a refresh does not write: a write adds
    /// to the load of the object. The refresh goes on while the last
    /// good write is less than one window old, and gets 503 after that.
    /// The task of [`Service::save_each_second`] writes the store again
    /// after the window (01M3ZZQCEKEYK9CE8MGPM80Z2P).
    async fn tokens_written(&self) -> Result<(), TokenError> {
        let writes = *self.token_writes();
        match writes.failed {
            None => return Ok(()),
            Some(WriteFailed::Busy) if writes.fresh(tokio::time::Instant::now()) => {
                return Ok(());
            }
            Some(WriteFailed::Busy) => return Err(no(UNAVAILABLE)),
            Some(WriteFailed::Failed) => {}
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
        self.engine.stop(why);
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
        if let Some(snapshot) = snapshot {
            self.write_checkpoint(settings, snapshot, true).await;
        }
    }

    /// Writes a checkpoint at a stop, also when no record came since the
    /// last one. So the memory of the presence stays: the last call and
    /// the status of each session (01M4264028A3KVDK10PPERHM0C). It does
    /// not prune: a tool of the log can run at once after the stop, and
    /// it finds the chunks of before. The next checkpoint prunes.
    async fn last_checkpoint(&self) {
        if self.checkpoints().blocked.is_some() {
            return;
        }
        let snapshot = self
            .engine
            .read(|state| state.snapshot(Instant::now(), now_ms()));
        let settings = self.config.checkpoint.clone();
        self.write_checkpoint(&settings, snapshot, false).await;
    }

    /// Writes the checkpoint of `snapshot`, then prunes when `prune`.
    async fn write_checkpoint(
        &self,
        settings: &checkpoint::Settings,
        snapshot: Snapshot,
        prune: bool,
    ) {
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
        if !prune {
            return;
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

    /// The import of go-live (01M3Z8MRDZEKTXSKZTDTDSCZ3W), after the
    /// lease and before the port opens. See [`import`] for the steps.
    ///
    /// - The token store takes the sign-ins of the old `tokens` object,
    ///   at the position of the end of the import
    ///   (01M3Z8MRGWWA0CNZ003D67H6R4), and the server saves it. This
    ///   save comes before the log. So a server that stops between the
    ///   two steps finds no log at its next start, and imports again:
    ///   no stop loses the sign-ins.
    /// - The command `import` writes the records of the old objects as
    ///   one chunk, and the state takes the memory of each session.
    /// - A riff whose old objects have no ID gets one.
    /// - The server writes a checkpoint at once: it keeps the read
    ///   cursors.
    async fn import(&self, old: &Old) -> Result<(), StoreError> {
        let failed = |what: &str, why: String| {
            StoreError::Failed(format!(
                "the import of the old objects failed: {what}: {why}"
            ))
        };
        let at_ms = now_ms();
        let changes = old.changes(at_ms);
        // The log has no record, so the last record of the import has
        // this position.
        let position = changes.len() as u64;
        let mut sign_ins = 0;
        if let Some(bytes) = old.tokens() {
            let tokens = Tokens::import(bytes, position, Instant::now(), SystemTime::now())
                .map_err(|e| failed("the old object tokens", e.to_string()))?;
            sign_ins = tokens.sign_ins();
            let mark = self.tokens_changes.load(Ordering::SeqCst);
            *self.tokens_change() = tokens;
            self.save_tokens_since(mark)
                .await
                .map_err(|e| failed("the save of the sign-ins", e.to_string()))?;
        }
        let records = self
            .engine
            .import(changes, old.memory(at_ms))
            .await
            .map_err(|e| failed("the command import", e.text()))?;
        let has_id = self.engine.read(|state| state.riff_id().is_some());
        if !has_id {
            self.engine.make_riff(token::random_token());
        }
        let blocked = self.checkpoints().blocked.is_some();
        if !blocked {
            let snapshot = self
                .engine
                .read(|state| state.snapshot(Instant::now(), now_ms()));
            let settings = self.config.checkpoint.clone();
            self.write_checkpoint(&settings, snapshot, true).await;
        }
        tracing::info!(
            records,
            position,
            sign_ins,
            "imported the objects of a riff-server from before the log"
        );
        Ok(())
    }

    fn checkpoints(&self) -> MutexGuard<'_, Checkpoints> {
        self.checkpoints
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// One look at the owner role at `now` (see [`owner`]): it sends
    /// the command `grant_owner` for a request whose time ended, then
    /// checks the owner when a check is due. It warns the owner one
    /// check before the owner is gone, and then sends the command
    /// `end_owner`. Returns the change that it made. The note of a
    /// change is in the chunk of its command.
    async fn owner_tick(&self, checks: &mut Checks, now: Instant) -> Option<OwnerChange> {
        let timing = &self.config.owner_role;
        if self.engine.read(|state| state.owner_due(now)) {
            return self.changed(self.engine.grant_owner().await);
        }
        if !checks.due(now) {
            return None;
        }
        // Only a riff with an owner and another admin checks the owner.
        let owner = self.engine.read(|state| {
            let people = state.people();
            people
                .has_other_admin()
                .then(|| people.owner_user().map(str::to_owned))
        });
        let Some(user) = owner else {
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
                self.warn_owner(user.as_deref()).await;
                None
            }
            Check::Gone => self.changed(self.engine.end_owner().await),
        }
    }

    /// The change of a command of the owner timer. A command that did
    /// not go is only logged: the next look sends it again.
    fn changed(&self, sent: Result<Option<OwnerChange>, Failed>) -> Option<OwnerChange> {
        match sent {
            Ok(change) => change,
            Err(failed) => {
                tracing::warn!("a change of the owner role did not go: {}", failed.text());
                None
            }
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
    /// the thread of each repository, and one line in the chat. The
    /// log line names no email (01M3XA87CJHCGZX283ZQAFKARZ).
    async fn warn_owner(&self, user: Option<&str>) {
        let Some(email) = self.engine.read(|state| state.people().members().owner) else {
            return;
        };
        let news = owner::warn_news(&email, &self.config.owner_role);
        tracing::info!("the server warns the owner: the next check can end the owner role");
        let mut posts = Vec::new();
        if let Some(user) = user {
            for thread in self.engine.read(State::repositories) {
                let to = vec![Selector {
                    user: Some(user.to_owned()),
                    ..Selector::default()
                }];
                posts.push(Server::news(Some(thread), to, &news, Kind::Note));
            }
        }
        let chat = ThreadName::chat();
        posts.push(Server::news(Some(chat), Vec::new(), &news, Kind::Message));
        self.announce_each(posts).await;
    }

    /// Revokes each forge token whose role or repository is not the
    /// role and the repository of the facts now ([`forge::Forge::settle`],
    /// [`State::forge_fact`]), or whose GitHub account is not allowed
    /// any more. The session `ended` lost its token: it
    /// sent its `end` call.
    async fn settle_forge(&self, ended: Option<&Who>) {
        let now = Instant::now();
        let facts: Vec<(Who, Option<(TokenRole, String)>)> = self.engine.read(|state| {
            self.forge
                .holders()
                .into_iter()
                .map(|who| {
                    if ended == Some(&who) {
                        return (who, None);
                    }
                    let fact = state
                        .me(&who, now, now_ms())
                        .and_then(|info| state.forge_fact(&info.uri, now))
                        .map(|(role, thread)| (role, thread.to_string()))
                        .filter(|(_, repo)| {
                            state.forge_allows(repo.split('/').next().unwrap_or_default())
                        });
                    (who, fact)
                })
                .collect()
        });
        self.forge
            .settle(|who| {
                facts
                    .iter()
                    .find(|(w, _)| w == who)
                    .and_then(|(_, f)| f.clone())
            })
            .await;
    }

    /// Asks each idle worker past the limit to stop, and posts a note to
    /// the lead of its user for the first ask of each. Posts one more
    /// note for each worker that still runs after the ask (see [`idle`]).
    async fn stop_idle_workers(&self) {
        let settings = self.engine.read(State::idle);
        let mut notes = Vec::new();
        for stopping in self.engine.stop_idle_workers() {
            if stopping.first {
                notes.push((stopping.worker.clone(), idle::news(&stopping, &settings)));
            }
        }
        for stuck in self.engine.stuck_workers() {
            notes.push((stuck.worker.clone(), idle::stuck(&stuck)));
        }
        let mut posts = Vec::new();
        for (worker, news) in notes {
            tracing::info!("{news}");
            let Some(thread) = worker.default_thread() else {
                continue;
            };
            let lead = Selector::lead(worker.who().user(), &thread.to_string());
            posts.push(Server::news(Some(thread), vec![lead], &news, Kind::Note));
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
            false,
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
    /// - A store with the objects of a riff-server from before the log,
    ///   and no log, gets the import of go-live before the load ends
    ///   (01M3Z8MRDZEKTXSKZTDTDSCZ3W, see [`import`]). So the port opens
    ///   only after the import. The old instance wrote its objects until
    ///   it read the new lease, so the load reads them again after the
    ///   wait, and imports from that read (01M3ZCDNQY2G9ET537B6SBYCBB). When the log
    ///   is there after the wait, another instance made the import: this
    ///   one does not import, and does not change the sign-ins.
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
        let old = read_old_objects(&*store).await?;
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
        let taken = Instant::now();
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
        // An old instance from before the log wrote its objects until it
        // read the new lease. So the import reads them again
        // (01M3ZCDNQY2G9ET537B6SBYCBB). When another instance made the
        // import in that time, the log is there, and this read gives
        // none: this instance does not import.
        let old = match old {
            Some(_) => read_old_objects(&*store).await?,
            None => None,
        };
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
        let lease = Arc::new(lease);
        let saved = Saved {
            store: store.clone(),
            lease: lease.clone(),
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
            old.is_some(),
        );
        // Save the token store once: the load can drop sign-ins
        // (01M3XA87A9GGFA89RQXWSKY0V6), and a store that does not save
        // shows at the start. While that save failed, a refresh gets 503
        // (R150).
        drop(service.0.tokens_change());
        if let Err(error) = service.0.save_tokens_since(0).await {
            service
                .0
                .error(format!("the token store was not saved: {error}"));
        }
        service.keep_lease(lease, taken);
        service.save_each_second();
        if let Some(old) = old {
            service.0.import(&old).await?;
        }
        Ok(service)
    }

    /// `log` is the store of the log. `until` is the end of the first
    /// serve time of a server with a lease. `start` is the second when it
    /// starts to serve. With `import`, the import of go-live follows: it
    /// gives the riff its ID and its owner, so the build sends no
    /// `make_riff` and no `name_owner`.
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
        import: bool,
    ) -> Self {
        let settings = Settings::new(&config.admins, &config.public_url, config.owner_role);
        let state = state.with_settings(settings);
        // A sign-in from before the last removal or revoke of its person
        // is ended: the server stopped between the write of the record
        // and the end of the sign-ins (01M3XA87A9GGFA89RQXWSKY0V6).
        let mut tokens = tokens;
        let mut dropped = tokens.drop_ended(&state.signins_ended());
        // A sign-in that the log does not hold goes too
        // (01M3XGNZYD1E35DXYTHHJT1CR7): its position is after the end of
        // the log, or the people do not know its user. A log that went
        // back, for example after `log cut`, gives such sign-ins. A riff
        // with no sign-in has no people, so there each user counts as
        // known.
        let trusted = config.trusted();
        let end = state.written_position();
        dropped += tokens.drop_outside(end, |user| trusted || state.knows_person(user));
        if dropped > 0 {
            tracing::info!(
                sign_ins = dropped,
                "dropped the sign-ins that the log ended, or that the log does not hold"
            );
        }
        let mut replay = Replay::default();
        replay.refuse_before(start);
        // A riff with no ID gets one: the first start of a riff
        // (01M3WRD99M99PNGP8ME50KC6WS), or a log from before the ID.
        let first = state.riff_id().is_none();
        // The setting names the owner of a riff that has none and had
        // none (01M3JN3ASSV9SA0QZKXXJ0RTEV). A riff with no sign-in has
        // no people.
        let named = config
            .owner
            .clone()
            .filter(|_| !config.trusted() && !state.owned());
        let tokens = Arc::new(Mutex::new(tokens));
        let tokens_changes = Arc::new(AtomicU64::new(u64::from(dropped > 0)));
        let sign_ins = SignInStore {
            tokens: tokens.clone(),
            changes: tokens_changes.clone(),
            needs_sign_in: config.require_sign_in,
            trusted: config.trusted(),
        };
        let engine = Engine::new(state, sign_ins);
        if first && !import {
            engine.make_riff(token::random_token());
        }
        if let Some(owner) = named.filter(|_| !import) {
            engine.name_owner(&owner);
        }
        let save_every = config.save_every;
        let forge = forge::Forge::with_store(
            config.forge.clone(),
            config.forge_store.clone(),
            &config.github_api,
        );
        let service = Service(Arc::new(Server {
            forge,
            config,
            engine,
            tokens,
            tokens_changes,
            tokens_saved: AtomicU64::new(0),
            tokens_writes: Mutex::new(TokenWrites::new(save_every)),
            replay: Mutex::new(replay),
            http,
            saved,
            gate: Gate {
                until: Mutex::new(until),
                stopped: tokio::sync::watch::Sender::new(false),
                lease_ended: AtomicBool::new(false),
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
        service.watch_forge_tokens();
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
                let began = tokio::time::Instant::now();
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
                        let first = records.first().map_or(0, |record| record.envelope.position);
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
        // A riff with no sign-in has no people, so it has no owner.
        if self.0.config.trusted() {
            return;
        }
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
                    // The words name no person: an email is in no log
                    // line (01M3XA87CJHCGZX283ZQAFKARZ).
                    match change {
                        OwnerChange::Granted { .. } => {
                            tracing::info!(
                                "the owner did not answer: the admin that asked is the owner"
                            );
                        }
                        OwnerChange::Gone { owner: Some(_), .. } => {
                            tracing::info!("the owner is gone: the admin that asked is the owner");
                        }
                        OwnerChange::Gone { owner: None, .. } => {
                            tracing::info!("the owner is gone: the riff has no owner");
                        }
                    }
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

    /// Starts the task that compares the forge tokens with the facts
    /// each [`forge::SETTLE_EVERY`]: a session that died with no `end`
    /// call loses its claims after the claim grace, and then its token
    /// ([`Server::settle_forge`]). A server with no async runtime starts
    /// no task. The task ends when the service ends.
    fn watch_forge_tokens(&self) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let server = Arc::downgrade(&self.0);
        runtime.spawn(async move {
            let mut tick = tokio::time::interval(forge::SETTLE_EVERY);
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                let Some(server) = server.upgrade() else {
                    break;
                };
                if server.is_stopped() {
                    break;
                }
                if server.serving() && !server.forge.holders().is_empty() {
                    server.settle_forge(None).await;
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

    /// Stops taking calls, then saves each unsaved change (R129), and
    /// writes a last checkpoint. The gate replies 503 from now on.
    /// `main` calls it on SIGTERM. A server that holds the lease then
    /// ends it, so that a tool of the log can run at once
    /// (01M3X342ARX5Y7R9ZJDT12R9A1).
    pub async fn shutdown(&self) -> Result<(), StoreError> {
        self.0.gate.closing.store(true, Ordering::SeqCst);
        let saved = self.save().await;
        if saved.is_ok() && self.0.saved.is_some() && self.0.leased() {
            self.0.last_checkpoint().await;
        }
        if let Some(store) = &self.0.saved
            && self.0.leased()
            && let Err(error) = store.lease.end().await
        {
            tracing::warn!("the lease was not ended: {error}");
        }
        // The window of the `denied` lines ends here, so its count is
        // not lost (01M3Z67DZX9BC3TYF3PWGFGZJ7).
        self.0.engine.limit().close();
        saved
    }

    /// Ends when the server stops for good (R140, R141).
    pub async fn stopped(&self) {
        self.0.stopping().await;
    }

    /// True when the server stopped because its last write of the time
    /// to the lease is [`lease::Timing::ends_after`] old
    /// (01M3X5TPBMF81TDVZ7Q4NVXBQX). `main` then exits with an error, so
    /// that a new instance loads the state from the store.
    pub fn lease_ended(&self) -> bool {
        self.0.gate.lease_ended.load(Ordering::SeqCst)
    }

    /// Starts the task that reads the lease each
    /// [`lease::Timing::read_every`] (R139), and writes the time to it
    /// each [`lease::Timing::renew_every`]. `taken` is a time before the
    /// take of the lease. The task ends when the server stops or ends.
    ///
    /// When the last write of the time is [`lease::Timing::ends_after`]
    /// old, the server stops for good, and [`Service::lease_ended`] is
    /// true (01M3X5TPBMF81TDVZ7Q4NVXBQX). A fault of the store that is
    /// shorter gives 503, and the server goes on.
    fn keep_lease(&self, lease: Arc<Lease>, taken: Instant) {
        let server = Arc::downgrade(&self.0);
        let timing = self.0.config.lease;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(timing.read_every);
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            // The start of the last write of the time. The take wrote
            // the first time.
            let mut renewed = taken;
            // The same time on the wall clock: the monotonic clock can
            // stop while the machine sleeps.
            let since = taken.elapsed().as_millis() as u64;
            let mut renewed_ms = lease::now_ms().saturating_sub(since);
            loop {
                tick.tick().await;
                let Some(server) = server.upgrade() else {
                    break;
                };
                if server.is_stopped() {
                    break;
                }
                let asked = Instant::now();
                let asked_ms = lease::now_ms();
                // The lease ended by its age: a tool can have changed
                // the log. The state in memory never serves again, and
                // the instance does not write the lease again
                // (01M3X5TPBMF81TDVZ7Q4NVXBQX).
                let age = asked
                    .duration_since(renewed)
                    .max(Duration::from_millis(asked_ms.saturating_sub(renewed_ms)));
                if age >= timing.ends_after {
                    server.gate.lease_ended.store(true, Ordering::SeqCst);
                    server.stop(&format!(
                        "its last write of the time to the lease is {} seconds old, so the \
                         lease ended",
                        age.as_secs()
                    ));
                    break;
                }
                // A renewal that is due takes the place of the read. So
                // while it fails, the serve time ends
                // (01M3X34282SG0DJ6X34F90HS26).
                let renew = asked.duration_since(renewed) >= timing.renew_every;
                let held = if renew {
                    lease.renew().await
                } else {
                    lease.held().await
                };
                if renew && held.is_ok() {
                    renewed = asked;
                    renewed_ms = asked_ms;
                }
                match held {
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
            .route(End::PATH, post(settling::<End>))
            .route(Join::PATH, post(command::<Join>))
            .route(Leave::PATH, post(command::<Leave>))
            .route(Post::PATH, post(command::<Post>))
            .route(Claim::PATH, post(settling::<Claim>))
            .route(Release::PATH, post(settling::<Release>))
            .route(ReleaseFor::PATH, post(settling::<ReleaseFor>))
            .route(Lead::PATH, post(command::<Lead>))
            .route(Pause::PATH, post(command::<Pause>))
            .route(Resume::PATH, post(command::<Resume>))
            .route(Hold::PATH, post(command::<Hold>))
            .route(Free::PATH, post(command::<Free>))
            .route(SetPlan::PATH, post(command::<SetPlan>))
            .route(PlanOff::PATH, post(command::<PlanOff>));
        // `set_idle` and `forge_allow` have a router of their own: in a
        // riff with sign-in, their routes always have the token check.
        let set_idle = Router::new()
            .route(SetIdle::PATH, post(command::<SetIdle>))
            .route(ForgeAllow::PATH, post(settling::<ForgeAllow>));
        // ANCHOR_END: routes
        // The signals and the queries.
        let mut routes = commands
            .route(SetStatus::PATH, post(status))
            .route(SetBlocked::PATH, post(blocked))
            .route(SetStep::PATH, post(step))
            .route(BlockedLook::PATH, post(look_blocks))
            .route(ItemFacts::PATH, post(item_facts))
            .route(Alive::PATH, post(alive))
            .route(WhoRequest::PATH, post(who))
            .route(Threads::PATH, post(threads))
            .route(Read::PATH, post(read))
            .route(RiffQuery::PATH, post(riff))
            .route(PlanSeen::PATH, post(plan_seen))
            .route(PlanShow::PATH, post(plan_show))
            .route(ForgeToken::PATH, post(forge_token))
            .route(ForgeCheck::PATH, post(forge_check))
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
        // The commands of the people. Each one names no `me`: the caller
        // is the caller of the token, so each route has the token check.
        let admin_routes = admin_routes
            .route(Revoke::PATH, post(command::<Revoke>))
            .route(Invite::PATH, post(command::<Invite>))
            .route(Remove::PATH, post(command::<Remove>))
            .route(SetAdmin::PATH, post(command::<SetAdmin>))
            .route(PassOwner::PATH, post(command::<PassOwner>))
            .route(TakeOwner::PATH, post(command::<TakeOwner>))
            .route(DenyOwner::PATH, post(command::<DenyOwner>))
            .route(Members::PATH, post(members))
            .route(LogQuery::PATH, post(log_records))
            .route(ForgeCreate::PATH, post(forge_create))
            .route(ForgeCreated::PATH, post(forge_created))
            .route(ForgeInstall::PATH, post(forge_install))
            .route_layer(guard());
        let mut facts = Router::new().route("/v1/server", get(server_facts));
        if self.0.config.require_sign_in {
            facts = facts.route_layer(guard());
        }
        routes
            .merge(admin_routes)
            .route_layer(middleware::from_fn_with_state(self.0.clone(), check_build))
            // `riff login` and a refresh work with each version
            // (01M3MX4V43SF2XFCZWANHD19WV).
            .route(auth::TOKEN_PATH, post(token))
            .route("/v1/sign-in", get(sign_in_config))
            .route(auth::RESOURCE_METADATA_PATH, get(resource_metadata))
            .route(auth::SERVER_METADATA_PATH, get(server_metadata))
            // The pages of `riff forge create` in the browser: the
            // `state` is their check (#627).
            .route(forge::manifest::NEW_PATH, get(forge_new_page))
            .route(forge::manifest::CREATED_PATH, get(forge_created_page))
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

    /// Signs in a person with the verified email `email` on the device
    /// key `jkt`, as the sign-in of the provider does: the command
    /// `admit` decides by the rules of [`state::people`], and then the
    /// token store starts the chain (01M3XA877YZQ649SWB5TN60V5P).
    /// `allowed_domain` is true when the account is in an allowed
    /// domain (R15). The reply comes after the write of the records
    /// and the save of the token store (R128). The error is the text
    /// that the person reads. A riff with no sign-in has no people, and
    /// refuses.
    ///
    /// ```
    /// # #[tokio::main] async fn main() {
    /// use riff_server::Service;
    /// use riff_server::auth::Config;
    ///
    /// let service = Service::new(Config {
    ///     require_sign_in: true,
    ///     ..Config::default()
    /// });
    /// assert!(!service.owned());
    /// // The first person is the owner, from any domain.
    /// let ada = service.admit("Ada@gmail.com", false, "k1").await.unwrap();
    /// assert_eq!(ada.user, "ada");
    /// assert_eq!(service.members().owner.as_deref(), Some("ada@gmail.com"));
    /// assert!(service.owned());
    ///
    /// // A person with no invite and no allowed domain is refused.
    /// let refused = service.admit("bob@gmail.com", false, "k2").await.unwrap_err();
    /// assert!(refused.ends_with("run: riff invite bob@gmail.com"), "{refused}");
    /// // A person of an allowed domain signs in.
    /// assert!(service.admit("bob@gmail.com", true, "k2").await.is_ok());
    ///
    /// // Another email that gives the USER `ada` is refused (R209).
    /// let taken = service.admit("ada@x.io", true, "k3").await.unwrap_err();
    /// assert_eq!(taken, "the user ada belongs to another account; ask an admin");
    /// # }
    /// ```
    pub async fn admit(
        &self,
        email: &str,
        allowed_domain: bool,
        jkt: &str,
    ) -> Result<TokenReply, String> {
        let identity = Identity {
            email: email.to_owned(),
            user: oidc::user_of(&state::people::email(email)).unwrap_or_default(),
            allowed_domain,
        };
        let mark = self.0.tokens_changes.load(Ordering::SeqCst);
        let reply = self.0.sign_in(&identity, jkt).await;
        self.0
            .save_tokens_since(mark)
            .await
            .map_err(|error| error.to_string())?;
        reply.map_err(|failed| failed.text())
    }

    /// Who may join the riff: what `POST /v1/members` replies. Each
    /// person shows once, with the highest role. The admins of the
    /// settings are in it (R210).
    ///
    /// ```
    /// use riff_server::Service;
    /// use riff_server::auth::Config;
    ///
    /// let service = Service::new(Config {
    ///     admins: vec![" Dan@X.io".into()],
    ///     ..Config::default()
    /// });
    /// let list = service.members();
    /// assert_eq!(list.owner, None);
    /// assert_eq!(list.admins, ["dan@x.io"]);
    /// assert!(list.members.is_empty());
    /// assert_eq!(service.asks(), None);
    /// ```
    pub fn members(&self) -> MembersReply {
        self.0.members()
    }

    /// The email of the admin whose request for the owner role waits
    /// (01M3N7K3ZAZFGABN7032AYJWEM).
    pub fn asks(&self) -> Option<String> {
        self.0.engine.read(State::asks)
    }

    /// The ID of the riff, when it has one (01M3JNVBPMZ1K9WX7Q7DP6Y0DH).
    /// `GET /v1/sign-in` gives the same ID.
    pub fn riff_id(&self) -> Option<String> {
        self.0.engine.read(State::riff_id)
    }

    /// True when the riff has an owner, or had one: a riff whose owner
    /// was gone counts (01M3JN3AQMHZHT6JP3P6GM9PWZ).
    pub fn owned(&self) -> bool {
        self.0.engine.read(State::owned)
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
/// (01M3X4Z64ZNRD0G0F4JV1M64FN).
fn admit(s: &Server, proof: &Proof, me: &SessionUri) -> Result<Admitted, Failed> {
    s.engine
        .admit(proof.signed_in.as_ref(), me)
        .inspect_err(|failed| failed.trace_denied(s.engine.limit(), &proof.path, Some(me)))
}

/// A keep-alive: a sign of life that is not a call (R204). It is a
/// signal.
async fn alive(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<Alive>,
) -> Reply<AliveReply> {
    let caller = admit(&s, &proof, &r.me)?;
    let alive = Signal::Alive {
        activity: r.activity,
        prompt_secs: r.prompt_secs,
    };
    Ok(Json(s.engine.signal(&caller, alive).await?))
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

/// Sets the block of a session (01M41FZPGEK4TNPSM2051W4VMS). It is a
/// signal, like a status.
async fn blocked(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<SetBlocked>,
) -> Reply<()> {
    let caller = admit(&s, &proof, &r.me)?;
    r.check().map_err(bad_request)?;
    let blocked = Signal::Blocked {
        reason: r.reason,
        at_ms: now_ms(),
    };
    s.engine.signal(&caller, blocked).await?;
    Ok(Json(()))
}

/// Changes the long step of a session (01M48VDGTD40P8RBZMS0XB5M9N). It
/// is a signal, like a status, and a sign of life.
async fn step(AxumState(s): AxumState<Shared>, proof: Proof, Json(r): Json<SetStep>) -> Reply<()> {
    let caller = admit(&s, &proof, &r.me)?;
    r.check().map_err(bad_request)?;
    let step = Signal::Step {
        change: r.change,
        at_ms: now_ms(),
    };
    s.engine.signal(&caller, step).await?;
    Ok(Json(()))
}

/// The look of the lead at the blocks of the sessions of its user
/// (01M41FZQ545HQ9Q75CSKX8HF8H). Each block with no answer for
/// `after_secs` gets a second wake of the lead: a message of the server.
async fn look_blocks(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<BlockedLook>,
) -> Reply<BlockedLookReply> {
    let caller = admit(&s, &proof, &r.me)?;
    let after = Duration::from_secs(r.after_secs);
    let (again, unanswered) = s.engine.look_blocks(&caller, after).await?;
    let mut posts = Vec::new();
    for (session, reason) in &again {
        let news = still_blocked(session, reason, r.after_secs);
        tracing::info!("{news}");
        let Some(thread) = session.default_thread() else {
            continue;
        };
        let lead = Selector::lead(session.who().user(), &thread.to_string());
        posts.push(Server::news(Some(thread), vec![lead], &news, Kind::Message));
    }
    s.announce_each(posts).await;
    let unanswered = unanswered
        .into_iter()
        .map(|(session, reason)| Unanswered { session, reason })
        .collect();
    Ok(Json(BlockedLookReply { unanswered }))
}

/// The second wake of the lead for a block with no answer
/// (01M41FZQ545HQ9Q75CSKX8HF8H): the session, its claims and the
/// reason.
///
/// ```
/// let w1 = "riff://mike@pangolin/o/r?session=1a2b3c4d5e&claim=issue-12".parse()?;
/// assert_eq!(
///     riff_server::still_blocked(&w1, "which design?", 600),
///     "blocked: 1a2b3c4d (issue-12) has no answer after 10 minutes: which design?"
/// );
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn still_blocked(session: &SessionUri, reason: &str, after_secs: u64) -> String {
    let id = session.who().session().unwrap_or_default();
    let short: String = id.chars().take(8).collect();
    let claims = match session.claims() {
        [] => String::new(),
        claims => format!(" ({})", claims.join(", ")),
    };
    let minutes = match after_secs.div_ceil(60) {
        1 => "1 minute".to_owned(),
        n => format!("{n} minutes"),
    };
    format!("blocked: {short}{claims} has no answer after {minutes}: {reason}")
}

/// Keeps the facts of the items of the repository of the caller
/// (01M41FZP2C4Z4J6WKRXZ5B31EH). It is a signal.
async fn item_facts(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<ItemFacts>,
) -> Reply<()> {
    let caller = admit(&s, &proof, &r.me)?;
    let thread =
        r.me.default_thread()
            .ok_or_else(|| bad_request("the session is in no repository".into()))?;
    let facts = Signal::Facts {
        thread,
        items: r.items,
        all: r.all,
    };
    s.engine.signal(&caller, facts).await?;
    Ok(Json(()))
}

async fn who(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<WhoRequest>,
) -> Reply<WhoReply> {
    let trusted = s.config.trusted();
    let caller = admit(&s, &proof, &r.me)?;
    let reply = s
        .engine
        .query(&caller, |state| {
            let now = Instant::now();
            let now_ms = now_ms();
            // The people of the written copy. A riff with no sign-in
            // shows none.
            let (owner, members) = if trusted {
                (RiffOwner::NoSignIn, Vec::new())
            } else {
                let people = state.people();
                (people.riff_owner(), people.persons())
            };
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

/// A command that can change the role of the forge token of a session:
/// a claim, a release, the end of a session, the end of the allow of a
/// GitHub account. After the command, the
/// server revokes each token whose role changed ([`forge::Forge::settle`]).
/// The revoke runs after the reply, so a slow GitHub does not hold up
/// the command.
async fn settling<C: Routed>(
    AxumState(s): AxumState<Shared>,
    call: Authenticated<C>,
) -> Result<Response, Failed>
where
    <C as Call>::Reply: serde::Serialize,
{
    // The session that ends loses its token.
    let ended = (C::PATH == End::PATH).then(|| call.me().who().clone());
    let reply = command::<C>(AxumState(s.engine.clone()), call).await;
    if reply.is_ok() && !s.forge.holders().is_empty() {
        let s = s.clone();
        tokio::spawn(async move { s.settle_forge(ended.as_ref()).await });
    }
    reply
}

/// The role and the repository of the forge token of `me`, from the
/// facts of the server (#628): [`State::forge_fact`]. A session that the
/// server does not know gets no token. A `me` with no session gets the
/// lead token only when its person is the lead of its repository. Only
/// a repository of an allowed GitHub account gets a token
/// (`riff forge allow`).
fn forge_facts(state: &State, me: &SessionUri) -> Result<(TokenRole, String), forge::Refusal> {
    match state.forge_fact(me, Instant::now()) {
        Some((role, thread)) => {
            let repo = thread.to_string();
            let owner = repo.split('/').next().unwrap_or_default();
            if !state.forge_allows(owner) {
                return Err(forge::Refusal::NotAllowed {
                    owner: owner.to_owned(),
                });
            }
            Ok((role, repo))
        }
        None if me.who().session().is_some() => Err(forge::Refusal::NoSession),
        None => Err(match me.default_thread() {
            Some(thread) => forge::Refusal::NotLead {
                repo: thread.to_string(),
            },
            None => forge::Refusal::NoRepository,
        }),
    }
}

/// `POST /v1/forge/token`: the forge token of the caller (#628). Only a
/// call with a sign-in gets one.
async fn forge_token(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<ForgeToken>,
) -> Reply<ForgeTokenReply> {
    if proof.signed_in.is_none() {
        return Err(forge::Refusal::NoSignIn.into());
    }
    if s.forge.app().is_none() {
        return Err(forge::Refusal::NoApp.into());
    }
    let caller = admit(&s, &proof, &r.me)?;
    let (role, repo) = s
        .engine
        .peek(&caller, |state| forge_facts(state, &r.me))??;
    Ok(Json(s.forge.give(r.me.who(), &repo, role).await?))
}

/// `POST /v1/forge/check`: `riff forge check` (#628). It holds no token.
async fn forge_check(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<ForgeCheck>,
) -> Reply<ForgeCheckReply> {
    if proof.signed_in.is_none() {
        return Err(forge::Refusal::NoSignIn.into());
    }
    if s.forge.app().is_none() {
        return Err(forge::Refusal::NoApp.into());
    }
    let caller = admit(&s, &proof, &r.me)?;
    let (_, repo) = s
        .engine
        .peek(&caller, |state| forge_facts(state, &r.me))??;
    Ok(Json(s.forge.check(&repo).await?))
}

/// Refuses `user` when it is not the owner or an admin.
fn need_admin(s: &Server, user: &str, what: &str) -> Result<(), (StatusCode, String)> {
    if s.engine.read(|state| state.role(user)) < Role::Admin {
        let reason = format!("only the owner and the admins can {what}");
        return Err(Failed::Refused(Refused::new(Code::NotAllowed, &reason)).into());
    }
    Ok(())
}

/// `POST /v1/forge/create`: `riff forge create` (#627). Only the owner
/// or an admin. The reply is the start page of a new `state`.
async fn forge_create(
    AxumState(s): AxumState<Shared>,
    Extension(signed_in): Extension<SignedIn>,
    Json(r): Json<ForgeCreate>,
) -> Reply<ForgeCreateReply> {
    let user = signed_in.who.user();
    need_admin(&s, user, "make the GitHub App of riff")?;
    if !forge::manifest::account_name(&r.org) {
        let text = format!(
            "{:?} is no GitHub organization: give its name, for example acme",
            r.org
        );
        return Err((StatusCode::BAD_REQUEST, text));
    }
    if !s.forge.has_store() {
        let text = "this riff-server has no store for the App: give it RIFF_FORGE_SECRET";
        return Err((StatusCode::CONFLICT, text.to_owned()));
    }
    let state = s.forge.starts().begin(&r.org, user, Instant::now());
    let url = format!(
        "{}{}?state={state}",
        s.config.public_url,
        forge::manifest::NEW_PATH
    );
    Ok(Json(ForgeCreateReply { url, state }))
}

/// `POST /v1/forge/created`: how far a start of `riff forge create` is.
/// Only the admin that made the `state` sees it.
async fn forge_created(
    AxumState(s): AxumState<Shared>,
    Extension(signed_in): Extension<SignedIn>,
    Json(r): Json<ForgeCreated>,
) -> Reply<ForgeCreatedReply> {
    let user = signed_in.who.user();
    need_admin(&s, user, "make the GitHub App of riff")?;
    s.forge
        .progress(&r.state, user)
        .await
        .map(Json)
        .map_err(|text| (StatusCode::BAD_REQUEST, text))
}

/// `POST /v1/forge/install`: `riff forge install OWNER` (#627).
async fn forge_install(
    AxumState(s): AxumState<Shared>,
    Json(r): Json<ForgeInstall>,
) -> Reply<ForgeInstallReply> {
    if !forge::manifest::account_name(&r.owner) {
        let text = format!(
            "{:?} is no GitHub account: give the name of an organization or a person, for \
             example acme",
            r.owner
        );
        return Err((StatusCode::BAD_REQUEST, text));
    }
    Ok(Json(s.forge.install(&r.owner).await?))
}

/// The query of the pages of `riff forge create`.
#[derive(Deserialize)]
struct ForgePage {
    #[serde(default)]
    state: String,
    #[serde(default)]
    code: String,
}

/// A page for the browser.
fn page(status: StatusCode, body: String) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response()
}

/// `GET /forge/new?state=`: the start page of `riff forge create`. It
/// posts the manifest to GitHub.
async fn forge_new_page(AxumState(s): AxumState<Shared>, Query(q): Query<ForgePage>) -> Response {
    let Some(org) = s.forge.starts().org(&q.state, Instant::now()) else {
        let text = forge::manifest::text_page(forge::manifest::BAD_STATE);
        return page(StatusCode::BAD_REQUEST, text);
    };
    let manifest = forge::manifest::manifest(&s.config.public_url, &org);
    page(
        StatusCode::OK,
        forge::manifest::start_page(&org, &q.state, &manifest),
    )
}

/// `GET /forge/created?code=&state=`: GitHub sends the browser here
/// after the admin made the App. The server swaps the code for the App,
/// and sends the browser to the install page of the App.
async fn forge_created_page(
    AxumState(s): AxumState<Shared>,
    Query(q): Query<ForgePage>,
) -> Response {
    match s.forge.created(&q.code, &q.state).await {
        Ok(install) => (StatusCode::SEE_OTHER, [(header::LOCATION, install)]).into_response(),
        Err(why) => page(StatusCode::BAD_REQUEST, forge::manifest::text_page(&why)),
    }
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

/// Reads the pauses of the riff, as the caller sees them: a query
/// (01M3WRD9BSBKS9TN66H29TGTBV, 01M3XAHZJAF6YVDJ7WX74X8RBX). The
/// commands `pause` and `resume` change them.
async fn riff(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<RiffQuery>,
) -> Reply<RiffReply> {
    let caller = admit(&s, &proof, &r.me)?;
    let me = r.me.clone();
    let reply = s
        .engine
        .query(&caller, move |state| state.pauses_at(&me))
        .await?;
    Ok(Json(reply))
}

/// A look saw the plan of the server at `position`: a signal
/// (01M4A4YTYVHFGK0CJVACJQ8DQ3). It counts only from a session in the
/// thread that is not a worker, and only for the position of the plan
/// of the server. The reply is the plan of the server.
async fn plan_seen(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<PlanSeen>,
) -> Reply<PlanReply> {
    let caller = admit(&s, &proof, &r.me)?;
    let PlanSeen {
        me: _,
        thread,
        position,
    } = r;
    let who = caller.caller().who().clone();
    let seen = s
        .engine
        .query(&caller, |state| state.sees_plan(&who, &thread, position))
        .await?;
    if seen {
        let signal = Signal::PlanSeen {
            thread: thread.clone(),
        };
        s.engine.signal(&caller, signal).await?;
    }
    let reply = s
        .engine
        .read(|state| state.plan(&thread, Instant::now(), now_ms()));
    Ok(Json(reply))
}

/// Reads the plan of a repository thread and its holds: a query
/// (01M4A4Z1NKPDBXV2PRZCG86G6A).
async fn plan_show(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<PlanShow>,
) -> Reply<PlanReply> {
    let caller = admit(&s, &proof, &r.me)?;
    let reply = s
        .engine
        .query(&caller, |state| {
            state.plan(&r.thread, Instant::now(), now_ms())
        })
        .await?;
    Ok(Json(reply))
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

/// Shows who may join the riff: each person once, with the highest
/// role.
async fn members(
    AxumState(s): AxumState<Shared>,
    Extension(_): Extension<SignedIn>,
    Json(Members {}): Json<Members>,
) -> Json<MembersReply> {
    Json(s.members())
}

/// Gives the records of one repository for an audit
/// (01M3ZWRC11R5M9V1KTF05P240W): only to the owner and the admins. It
/// reads the log from the store, so it has each record, also the
/// records before the last checkpoint. A post has only its mark
/// (01M3ZWRC3XBFN8FJDGE8XWZ5EA).
async fn log_records(
    AxumState(s): AxumState<Shared>,
    Extension(signed_in): Extension<SignedIn>,
    Json(r): Json<LogQuery>,
) -> Reply<LogReply> {
    let user = signed_in.who.user();
    if s.engine.read(|state| state.role(user)) < Role::Admin {
        let reason = "only the owner and the admins can read the log";
        return Err(Failed::Refused(Refused::new(Code::NotAllowed, reason)).into());
    }
    let replayed = log::replay(&*s.log).await.map_err(|error| {
        tracing::error!("the log for an audit did not read: {error}");
        let text = format!("the log did not read: {error}");
        (StatusCode::SERVICE_UNAVAILABLE, text)
    })?;
    let records = replayed
        .records
        .into_iter()
        .filter(|record| record.of_repository(&r.repo))
        .map(Record::for_audit)
        .collect();
    Ok(Json(LogReply { records }))
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
    // the caller in the line (01M3X4Z64ZNRD0G0F4JV1M64FN).
    let whole = s.engine.limit().refuse(request, code).await;
    let reply = (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, challenge)],
    );
    closed(reply.into_response(), whole)
}

/// The reply to a call that the token layer refused. When the server
/// did not read the whole body of the call (`whole` is false), the
/// reply closes the call: the server reads no more of it
/// (01M3Z67B9RMVKY7TCXCG8HEZT4).
fn closed(mut reply: Response, whole: bool) -> Response {
    if !whole {
        let close = header::HeaderValue::from_static("close");
        reply.headers_mut().insert(header::CONNECTION, close);
    }
    reply
}

/// Refuses a call from a `riff` of a version that this server cannot
/// talk to, or that names no build (01M3MX1E65XGWDZ062PQ9YXQ5T), with
/// the line `denied` and the code `old_build` (01M3X4Z64ZNRD0G0F4JV1M64FN). A `riff`
/// of the line of this server, or of the line before, goes on
/// (01M3MX1DYY6AVDW946NR0B9T2C, 01M3MX1E1EY1M7JGNCN6FCEVQK). The OAuth
/// metadata, `/v1/token` and `/v1/sign-in` stay open to each client
/// (01M3MX4V43SF2XFCZWANHD19WV).
async fn check_build(AxumState(s): AxumState<Shared>, request: Request, next: Next) -> Response {
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
    let whole = s.engine.limit().refuse(request, DeniedCode::OldBuild).await;
    let reply = (StatusCode::CONFLICT, mismatch.to_string());
    closed(reply.into_response(), whole)
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
        // The line holds no part of an email
        // (01M3XA87CJHCGZX283ZQAFKARZ).
        tracing::info!("sign-in refused: {}", e.for_log());
        no("invalid_grant")
    })?;
    s.first_use(proof).map_err(|_| no("invalid_dpop_proof"))?;
    let pair = s
        .sign_in(&identity, &proof.jkt)
        .await
        .map_err(|failed| match failed {
            Failed::Stopped => no(UNAVAILABLE),
            // The person is not a member, another email holds the USER
            // (R209), or the email gives no USER (R208). Each refuses the
            // person, who must read why. The trace of the refused
            // command `admit` is its log line: it names no email
            // (01M3XA87CJHCGZX283ZQAFKARZ).
            refused => TokenError {
                error: "access_denied".into(),
                error_description: Some(refused.text()),
            },
        })?;
    tracing::info!("{} signed in", pair.user);
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

/// Names the sign-in provider and the riff ID, for `riff login`. The
/// riff ID comes from the written copy (01M3XA87HE06Z6M32ZJPSYSYRZ). A
/// new riff has its ID after the first write: the call waits for it.
async fn sign_in_config(AxumState(s): AxumState<Shared>) -> Reply<SignInConfig> {
    let Some(provider) = &s.config.provider else {
        return Err(not_found("this riff-server has no sign-in provider".into()));
    };
    let mut riff_id = s.engine.read(State::riff_id);
    if riff_id.is_none() {
        s.engine.settle().await?;
        riff_id = s.engine.read(State::riff_id);
    }
    match riff_id {
        Some(riff_id) => Ok(Json(provider.config(&riff_id))),
        None => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "the riff has no ID yet. Try again.".into(),
        )),
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
    let live = until_lagged(rx).filter_map(move |(to, wake)| {
        let _alive = &guard;
        let mine = to == guard.who && may_read(&to, &wake.thread);
        std::future::ready(mine.then_some(wake))
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
    let stream = until_lagged(s.engine.tail()).filter_map(move |tailed| {
        let event = (tailed.thread == q.thread)
            .then(|| Event::default().json_data(tailed).ok())
            .flatten();
        std::future::ready(event.map(Ok))
    });
    let stream = stream.take_until(s.stopping());
    Ok(Sse::new(opened(stream)).keep_alive(KeepAlive::default()))
}

/// The events of `rx` until it lags behind its buffer
/// (01M48RW9MA30A12E7XWX047CJ0). A lagged receiver lost events, so the
/// stream ends there, and does not drop them with no sign. The client
/// connects again. `watch` and `tail` use it.
///
/// ```
/// use futures::StreamExt;
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// let (tx, rx) = tokio::sync::broadcast::channel(2);
/// for n in 1..=3 {
///     tx.send(n).unwrap();
/// }
/// // The receiver lost event 1: the stream ends with no event.
/// let events: Vec<u32> = riff_server::until_lagged(rx).collect().await;
/// assert!(events.is_empty());
/// # }
/// ```
pub fn until_lagged<T>(rx: tokio::sync::broadcast::Receiver<T>) -> impl Stream<Item = T>
where
    T: Clone + Send + 'static,
{
    BroadcastStream::new(rx)
        .take_while(|event| {
            if let Err(lag) = event {
                tracing::info!(
                    "an event stream lags ({lag}): it ends, and the client connects again"
                );
            }
            std::future::ready(event.is_ok())
        })
        .filter_map(|event| std::future::ready(event.ok()))
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

    #[tokio::test]
    async fn a_lagged_stream_ends_on_the_server() {
        let (tx, rx) = tokio::sync::broadcast::channel(4);
        let mut events = Box::pin(until_lagged(rx));
        tx.send(1).unwrap();
        assert_eq!(events.next().await, Some(1));
        // Six more events in a buffer of four: the receiver lags.
        for n in 2..=7 {
            tx.send(n).unwrap();
        }
        let next = isolated::in_time(Duration::from_secs(5), events.next());
        assert_eq!(next.await.unwrap(), None, "the stream ends at the lag");
    }

    #[tokio::test]
    async fn a_stream_that_keeps_up_gives_each_event() {
        let (tx, rx) = tokio::sync::broadcast::channel(4);
        let events = until_lagged(rx);
        for n in 1..=3 {
            tx.send(n).unwrap();
        }
        drop(tx);
        assert_eq!(events.collect::<Vec<u32>>().await, [1, 2, 3]);
    }
    use crate::logline::testing::Capture;
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
        /// What happens one time after the next chunk is in the store,
        /// before the writer gets the proof of the write.
        after_chunk: Mutex<Option<Box<dyn FnOnce() + Send>>>,
        /// The view of an older build: each load of a chunk gives the
        /// second text in the place of the first one. So a value that
        /// this build knows reads as a value of a later build.
        later: Mutex<Option<(&'static str, &'static str)>>,
        /// The start of each write of the token store.
        token_saves: Mutex<Vec<tokio::time::Instant>>,
        /// Each write of the token store fails with this error.
        token_fail: Mutex<Option<StoreError>>,
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
            let later = *self.later.lock().unwrap();
            let Some((known, later)) = later.filter(|_| name.starts_with(log::LOG)) else {
                return self.store.load(name);
            };
            Box::pin(async move {
                let loaded = self.store.load(name).await?;
                Ok(loaded.map(|loaded| store::Loaded {
                    bytes: String::from_utf8(loaded.bytes)
                        .expect("a chunk is text")
                        .replace(known, later)
                        .into_bytes(),
                    ..loaded
                }))
            })
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
                if name == SIGN_INS {
                    self.token_saves
                        .lock()
                        .unwrap()
                        .push(tokio::time::Instant::now());
                    let fail = self.token_fail.lock().unwrap().clone();
                    if let Some(error) = fail {
                        return Err(error);
                    }
                }
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
                let saved = self.store.save(name, bytes, known).await;
                if saved.is_ok() && name.starts_with(log::LOG) {
                    let after = self.after_chunk.lock().unwrap().take();
                    if let Some(after) = after {
                        after();
                    }
                }
                saved
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
        send(&service, Resume::whole(mike())).await.unwrap();
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

    /// A rollback before the new build wrote a checkpoint: the new
    /// build wrote a pause with a scope that the old build does not
    /// know. The old build reads the scope as `other`, and writes no
    /// checkpoint past the record. So the new build has the pause again
    /// (01M3XM2C18TT8VSKGD77YPZG53). The scope of the later build is an
    /// object in the first case: the pause of a repository. It is a text
    /// in the second one: the old build does not know the resume of the
    /// riff, so it has the pause of a new riff.
    #[tokio::test(start_paused = true)]
    async fn new_build_old_build_new_build_keeps_a_pause_of_a_scope_that_the_old_build_does_not_know()
     {
        let repository = mike().default_thread().unwrap();
        let paused = |service: &Service| {
            service.0.engine.read(|state| {
                let pauses = state.pauses();
                (
                    pauses.riff().is_some(),
                    pauses.repository(&repository).is_some(),
                )
            })
        };
        let object = (r#""scope":{"repository":"#, r#""scope":{"wave":"#);
        let text = (r#""scope":"riff""#, r#""scope":"host""#);
        // The pauses of the riff and of the repository, in the new
        // build and in the old build.
        let cases = [
            (
                object,
                Some(Pause::here(mike())),
                (false, true),
                (false, false),
                1,
            ),
            (text, None, (false, false), (true, false), 2),
        ];
        for (view, pause, in_new, in_old, records) in cases {
            let store = Arc::new(Gated::default());
            let new = running_with(with_checkpoints(1000, "0.9.0"), store.clone()).await;
            if let Some(pause) = pause {
                send(&new, pause).await.unwrap();
            }
            assert_eq!(paused(&new), in_new);
            let last = position(&new);
            assert!(checkpoints(&store).await.is_empty());
            drop(new);

            // The old build does not know the scope: it has no pause,
            // and writes no checkpoint past the record.
            *store.later.lock().unwrap() = Some(view);
            let old = Service::load(with_checkpoints(1, "0.8.0"), store.clone())
                .await
                .unwrap();
            assert_eq!(paused(&old), in_old);
            post_n(&old, 3, "old").await;
            tokio::time::sleep(Duration::from_secs(1)).await;
            assert!(checkpoints(&store).await.is_empty());
            let facts = old.0.server_facts();
            assert_eq!(facts.skipped_records, records);
            let why = facts.no_checkpoint.expect("the build writes no checkpoint");
            assert!(why.contains("this build skipped the record at"), "{why}");
            assert_eq!(position(&old), last + 3);
            drop(old);

            // The new build again: it has the pause, and each record of
            // the old build. It writes checkpoints.
            *store.later.lock().unwrap() = None;
            let again = Service::load(with_checkpoints(1, "0.9.0"), store.clone())
                .await
                .unwrap();
            assert_eq!(paused(&again), in_new);
            assert_eq!(position(&again), last + 3);
            let facts = again.0.server_facts();
            assert_eq!((facts.skipped_records, facts.no_checkpoint), (0, None));
            post_n(&again, 2, "again").await;
            tokio::time::sleep(Duration::from_secs(1)).await;
            assert!(!checkpoints(&store).await.is_empty());
        }
    }

    /// A rollback before the new build wrote a checkpoint: the new
    /// build wrote a message with a kind, with a selector (a field, or
    /// another form of JSON), or with a session URI that the old build
    /// does not know. The old build reads each one as `other`, keeps
    /// the message with its text, and writes no checkpoint past the
    /// record. A session with such a URI is the same session: it keeps
    /// its claim. So the new build has the message again as it wrote
    /// it (01M3XSF90E9JYYTC13D9THY4WE).
    #[tokio::test(start_paused = true)]
    async fn new_build_old_build_new_build_keeps_a_message_with_a_value_that_the_old_build_does_not_know()
     {
        let thread = mike().default_thread().unwrap();
        // The last message of the thread: its kind, its address and its
        // text.
        let last_message = |service: &Service| {
            let request = Read {
                after: None,
                me: brett(),
                thread: thread.clone(),
                all: true,
            };
            let state = AxumState(service.0.clone());
            async move {
                let reply = read(state, Proof::none(), Json(request)).await.unwrap();
                reply.messages.last().unwrap().clone()
            }
        };
        let claims_of_mike = |service: &Service| {
            let read = |state: &State| state.uri(mike().who(), Instant::now()).claims().to_vec();
            service.0.engine.read(read)
        };
        let to_brett = vec![Selector {
            user: Some("brett".into()),
            ..Selector::default()
        }];
        let kind = (r#""kind":"note""#, r#""kind":"poll""#);
        let field = (
            r#""to":[{"user":"brett"}]"#,
            r#""to":[{"user":"brett","wave":"17"}]"#,
        );
        let form = (r#""to":[{"user":"brett"}]"#, r#""to":["all"]"#);
        // The URI of mike in each record gets a query part.
        let uri = (
            r#""riff://mike@pangolin/como-technologies/riff?session=a"#,
            r#""riff://mike@pangolin/como-technologies/riff?session=a&wave=17"#,
        );
        let cases = [
            (kind, Kind::Note),
            (field, Kind::Message),
            (form, Kind::Message),
            (uri, Kind::Message),
        ];
        for (view, sent) in cases {
            let store = Arc::new(Gated::default());
            let new = running_with(with_checkpoints(1000, "0.9.0"), store.clone()).await;
            let post = Post {
                kind: sent,
                ..Post::new(&mike(), Some(thread.clone()), to_brett.clone(), "for brett")
            };
            send(&new, claim_of(&mike(), "issue-5")).await.unwrap();
            send(&new, post).await.unwrap();
            let in_new = last_message(&new).await;
            assert_eq!(
                (in_new.kind, &in_new.to, in_new.body.as_str()),
                (sent, &to_brett, "for brett")
            );
            assert!(!in_new.from.is_other());
            let last = position(&new);
            assert!(checkpoints(&store).await.is_empty());
            // The records that the view of the old build changes.
            let records = store
                .chunks()
                .await
                .concat()
                .iter()
                .filter(|change| match change {
                    _ if view == uri => {
                        let session = change.session();
                        session.is_some_and(|session| session.who() == mike().who())
                    }
                    Change::Posted(posted) if view == kind => posted.message.kind == Kind::Note,
                    Change::Posted(posted) => posted.message.to == to_brett,
                    _ => false,
                })
                .count() as u64;
            assert!(records >= 1);
            drop(new);

            // The old build does not know the value: it keeps the
            // message with its text, and writes no checkpoint past the
            // record.
            *store.later.lock().unwrap() = Some(view);
            let old = Service::load(with_checkpoints(1, "0.8.0"), store.clone())
                .await
                .unwrap();
            let in_old = last_message(&old).await;
            assert_eq!(in_old.body, "for brett");
            assert_eq!(in_old.from.who(), mike().who());
            if view == kind {
                assert_eq!((in_old.kind, &in_old.to), (Kind::Other, &to_brett));
            } else if view == uri {
                assert_eq!((in_old.kind, &in_old.to), (sent, &to_brett));
                assert_eq!(in_old.from.other(), ["wave=17"]);
                assert_eq!(in_old.from.lead(), in_new.from.lead());
            } else {
                assert_eq!(in_old.kind, Kind::Message);
                assert!(in_old.to[0].is_other() && !in_old.to[0].matches(&brett()));
            }
            // The session of mike is the same session: it holds its
            // claim.
            assert_eq!(claims_of_mike(&old), ["issue-5"]);
            post_n(&old, 3, "old").await;
            tokio::time::sleep(Duration::from_secs(1)).await;
            assert!(checkpoints(&store).await.is_empty());
            let facts = old.0.server_facts();
            assert_eq!(facts.skipped_records, records);
            let why = facts.no_checkpoint.expect("the build writes no checkpoint");
            assert!(why.contains("this build skipped the record at"), "{why}");
            assert_eq!(position(&old), last + 3);
            drop(old);

            // The new build again: it has the message as it wrote it,
            // and each record of the old build. It writes checkpoints.
            *store.later.lock().unwrap() = None;
            let again = Service::load(with_checkpoints(1, "0.9.0"), store.clone())
                .await
                .unwrap();
            let request = Read {
                after: None,
                me: brett(),
                thread: thread.clone(),
                all: true,
            };
            let reply = read(AxumState(again.0.clone()), Proof::none(), Json(request))
                .await
                .unwrap();
            assert_eq!(reply.messages[reply.messages.len() - 4], in_new);
            assert_eq!(claims_of_mike(&again), ["issue-5"]);
            assert_eq!(position(&again), last + 3);
            let facts = again.0.server_facts();
            assert_eq!((facts.skipped_records, facts.no_checkpoint), (0, None));
            post_n(&again, 2, "again").await;
            tokio::time::sleep(Duration::from_secs(1)).await;
            assert!(!checkpoints(&store).await.is_empty());
        }
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
            let reply = me(AxumState(service.0.clone()), Proof::none(), query)
                .await
                .unwrap();
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
        // The ID and the pause of `make_riff`, the start of the session,
        // the join of the thread, the message.
        assert_eq!(position(&service), 5);
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
            Json(Alive {
                me: new.clone(),
                activity: None,
                prompt_secs: None,
            }),
        )
        .await
        .unwrap();
        assert!(!reply.stop);
        // The join of the repository thread and the start of the
        // session are written. That register makes no lead.
        let chunks = store.chunks().await;
        assert_eq!(chunks.len(), before + 1);
        assert!(matches!(
            chunks[before][..],
            [Change::JoinedThread(_), Change::SessionStarted(_)]
        ));
        let known = service
            .0
            .engine
            .read(|state| state.me(new.who(), Instant::now(), now_ms()));
        assert_eq!(known.unwrap().uri.place(), new.place());
    }

    /// The signal of a `register` sets the place of the session, also
    /// when the command makes no record (01M3WRD97EZJK3AABXECXEY133). A
    /// new worker mark goes to the log (01M3X9X9M079WGFPJZHNXH9VEP).
    #[tokio::test(start_paused = true)]
    async fn a_register_sets_the_place_and_writes_a_new_worker_mark() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let last = position(&service);
        let moved: SessionUri = "riff://brett@heron/como-technologies/riff?session=b#api"
            .parse()
            .unwrap();
        let register = |worker| Register {
            me: moved.clone(),
            worker,
        };
        send(&service, register(false)).await.unwrap();
        assert_eq!(position(&service), last, "no record");
        let shown = |service: &Service| {
            let read = |state: &State| state.me(brett().who(), Instant::now(), now_ms());
            service.0.engine.read(read).unwrap()
        };
        assert!(!shown(&service).worker);
        assert_eq!(shown(&service).uri.place(), moved.place());

        send(&service, register(true)).await.unwrap();
        assert_eq!(position(&service), last + 1, "the worker mark is a record");
        let chunks = store.chunks().await;
        assert!(matches!(
            &chunks[chunks.len() - 1][..],
            [Change::SessionStarted(started)] if started.worker
        ));
        assert!(shown(&service).worker);
        send(&service, register(true)).await.unwrap();
        assert_eq!(position(&service), last + 1, "the same mark: no record");
    }

    /// The server sends no wake to a worker that must clear its context.
    /// The request of a lead waits through the clear: the start with a
    /// fresh context gives the worker the wake that it missed
    /// (01M3X9XBMB3R718Z81BYXTHMZ0).
    #[tokio::test(start_paused = true)]
    async fn the_request_of_a_lead_waits_through_the_clear_of_a_worker() {
        use riff_core::wire::StartReason;

        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let worker: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w1"
            .parse()
            .unwrap();
        let start = |reason| Start {
            me: worker.clone(),
            reason,
            worker: true,
        };
        send(&service, start(StartReason::Process)).await.unwrap();
        send(&service, claim_of(&worker, "issue-7")).await.unwrap();
        let release = Release {
            me: worker.clone(),
            thread: worker.default_thread().unwrap(),
            item: "issue-7".into(),
        };
        // The reply to the release carries the ask to clear.
        assert!(send(&service, release).await.unwrap().must_clear);
        let alive = |service: &Service| {
            alive(
                AxumState(service.0.clone()),
                Proof::none(),
                Json(Alive {
                    me: worker.clone(),
                    activity: None,
                    prompt_secs: None,
                }),
            )
        };
        // The reply to a keep-alive carries it too.
        assert!(alive(&service).await.unwrap().clear);

        // The lead sends a request. The worker gets no wake.
        let mut wakes = service.0.engine.wakes();
        let to = vec![Selector::session("w1")];
        let request = Post::new(&mike(), None, to, "request: claim issue-12");
        let posted = send(&service, request).await.unwrap();
        assert_eq!(posted.woken[0].who(), worker.who(), "the record names it");
        assert!(wakes.try_recv().is_err(), "no wake in MustClear");
        let missed = |service: &Service| service.0.engine.read(|state| state.missed(worker.who()));
        assert!(missed(&service).is_none(), "a watch that starts gets none");
        let refused = send(&service, claim_of(&worker, "issue-12")).await;
        let Err(Failed::Refused(refused)) = refused else {
            panic!("a refusal");
        };
        assert_eq!(refused.code, state::Code::MustClear);
        assert_eq!(refused.reason, state::MUST_CLEAR);

        // A resume is no fresh context: the wake still waits.
        send(&service, start(StartReason::Resume)).await.unwrap();
        assert!(wakes.try_recv().is_err(), "no wake after a resume");

        // The clear: the worker gets the wake that it missed.
        send(&service, start(StartReason::Clear)).await.unwrap();
        let (to, wake) = wakes.recv().await.unwrap();
        assert_eq!(&to, worker.who());
        assert_eq!(wake.from.who(), mike().who());
        // A watch that starts after the clear gets it too.
        assert_eq!(missed(&service).unwrap().seq, wake.seq);
        assert!(!alive(&service).await.unwrap().clear);
        send(&service, claim_of(&worker, "issue-12")).await.unwrap();
    }

    /// One message gives a session one wake (01M3XV0588C2XZKZ3NM67JXCKJ).
    /// The request of the lead and the clear of the worker are in one
    /// chunk: the worker gets the missed wake, and not the wake of the
    /// `posted` record too. A message after the clear, in the same
    /// chunk, gives its own wake.
    #[tokio::test(start_paused = true)]
    async fn a_message_and_the_clear_in_one_chunk_give_one_wake() {
        use riff_core::wire::StartReason;

        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let worker: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w1"
            .parse()
            .unwrap();
        let start = |reason| Start {
            me: worker.clone(),
            reason,
            worker: true,
        };
        send(&service, start(StartReason::Process)).await.unwrap();
        send(&service, claim_of(&worker, "issue-7")).await.unwrap();
        let release = Release {
            me: worker.clone(),
            thread: worker.default_thread().unwrap(),
            item: "issue-7".into(),
        };
        assert!(send(&service, release).await.unwrap().must_clear);

        let mut wakes = service.0.engine.wakes();
        let request = |body: &str| Post::new(&mike(), None, vec![Selector::session("w1")], body);
        let before = store.chunks().await.len();
        let tries = store.tries.load(Ordering::SeqCst);
        store.hold.store(true, Ordering::SeqCst);
        let first = tokio::spawn(send(&service, post_body(&mike(), "one")));
        store.tried(tries + 1).await;
        let waits = tokio::spawn(send(&service, request("request: claim issue-12")));
        sleep(Duration::from_millis(10)).await;
        let clear = tokio::spawn(send(&service, start(StartReason::Clear)));
        sleep(Duration::from_millis(10)).await;
        let later = tokio::spawn(send(&service, request("request: claim issue-13")));
        sleep(Duration::from_millis(50)).await;
        store.release();
        first.await.unwrap().unwrap();
        let waits = waits.await.unwrap().unwrap();
        clear.await.unwrap().unwrap();
        let later = later.await.unwrap().unwrap();

        // The two requests and the clear are in one chunk, in that order.
        let chunks = store.chunks().await;
        assert_eq!(chunks.len(), before + 2);
        let kinds: Vec<&str> = chunks[before + 1]
            .iter()
            .filter_map(|change| match change {
                Change::Posted(_) => Some("posted"),
                Change::SessionStarted(_) => Some("session_started"),
                _ => None,
            })
            .collect();
        assert_eq!(kinds, ["posted", "session_started", "posted"]);

        // One wake for each request, and no more.
        let mut seqs = Vec::new();
        while let Ok((to, wake)) = wakes.try_recv() {
            assert_eq!(&to, worker.who());
            seqs.push(wake.seq);
        }
        assert_eq!(seqs, [waits.seq, later.seq]);
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
        let paused = Change::PauseSet(riff_core::record::PauseSet {
            scope: riff_core::record::Scope::Riff,
            state: riff_core::wire::RiffState::Paused,
        });
        let [first] = &store.chunks().await[..] else {
            panic!("one chunk");
        };
        // The riff ID is the first record of a new log
        // (01M3XA87HE06Z6M32ZJPSYSYRZ).
        let [Change::RiffMade(made), pause] = &first[..] else {
            panic!("{first:?}");
        };
        assert_eq!(*pause, paused);
        assert_eq!(service.riff_id().as_deref(), Some(made.riff_id.as_str()));
        drop(service);

        let next = Service::load(config(), store.clone()).await.unwrap();
        next.save().await.unwrap();
        assert_eq!(store.chunks().await.len(), 1);
        assert_eq!(position(&next), 2);
        assert_eq!(
            next.riff_id(),
            Some(made.riff_id.clone()),
            "the riff keeps its ID"
        );
    }

    /// An old object that does not read stops the start, and the error
    /// names the object (01M3Z8MRDZEKTXSKZTDTDSCZ3W). The server takes
    /// no lease, and writes no log.
    #[tokio::test(start_paused = true)]
    async fn an_old_object_that_does_not_read_stops_the_start() {
        for name in ["sessions", "tokens", "threads/acme%2Fapp"] {
            let store = Memory::default();
            store.save(name, b"[".to_vec(), None).await.unwrap();
            let loaded = Service::load(config(), Arc::new(store.clone())).await;
            let error = loaded.err().expect("the load fails").to_string();
            assert!(error.contains(name), "{error}");
            assert!(store.load(store::LEASE).await.unwrap().is_none());
            assert!(store.list(log::LOG).await.unwrap().is_empty());
        }
    }

    /// A start on a store with the old objects and no log is the import
    /// (01M3Z8MRDZEKTXSKZTDTDSCZ3W): the first chunk has its records,
    /// each with the cause `import` of the server, and a checkpoint
    /// follows at once. A second start does not import again
    /// (01M3Z8MRKTAN8CBAQB721JNZAK).
    #[tokio::test(start_paused = true)]
    async fn a_start_on_the_old_objects_and_no_log_is_the_import() {
        let store = Arc::new(Gated::default());
        let sessions = br#"{"saved_ms":1,"sessions":[],"cursors":[],"claims":[],"riff":"running"}"#;
        let tokens = br#"{"next_sign_in":0,"users":{"ann":"ann@acme.io"},"owner":"ann@acme.io","riff_id":"old-id","sign_ins":[],"access":[],"refresh":[]}"#;
        for (name, bytes) in [("sessions", &sessions[..]), ("tokens", &tokens[..])] {
            store.store.save(name, bytes.to_vec(), None).await.unwrap();
        }
        let service = Service::load(config(), store.clone()).await.unwrap();
        assert_eq!(service.riff_id().as_deref(), Some("old-id"));
        let chunks = records(&store).await;
        assert_eq!(chunks.len(), 1);
        let kinds: Vec<&str> = chunks[0].iter().map(|r| r.change.kind()).collect();
        assert_eq!(
            kinds,
            [
                "riff_made",
                "person_joined",
                "owner_set",
                "pause_set",
                "setting_changed"
            ]
        );
        for record in &chunks[0] {
            assert_eq!(record.envelope.by, Some(riff_core::record::By::Server));
            assert_eq!(record.envelope.command.as_deref(), Some("import"));
        }
        assert_eq!(checkpoints(&store).await.len(), 1);
        // The old objects stay.
        assert!(store.store.load("sessions").await.unwrap().is_some());
        assert!(store.store.load("tokens").await.unwrap().is_some());

        // A second start replays the log, also with the old objects
        // next to it.
        drop(service);
        let again = Service::load(config(), store.clone()).await.unwrap();
        again.save().await.unwrap();
        assert_eq!(records(&store).await.len(), 1);
        assert_eq!(position(&again), 5);
        assert_eq!(again.riff_id().as_deref(), Some("old-id"));
    }

    /// The records of each chunk of the log, in order.
    async fn records(store: &Gated) -> Vec<Vec<riff_core::record::Record>> {
        let mut chunks = Vec::new();
        for name in store.store.list(log::LOG).await.unwrap() {
            let bytes = store.store.load(&name).await.unwrap().unwrap().bytes;
            let (_, lines) = log::decode(&bytes).unwrap();
            let records = lines
                .into_iter()
                .map(|line| match line {
                    Line::Record(record) => *record,
                    Line::Unknown { .. } => panic!("a known kind"),
                })
                .collect();
            chunks.push(records);
        }
        chunks
    }

    /// A second session of mike. Mike's first session is the lead.
    fn mike2() -> SessionUri {
        "riff://mike@pangolin/como-technologies/riff?session=a2"
            .parse()
            .unwrap()
    }

    /// Each record names its cause, and the records of one command are
    /// in one chunk, one after another (01M3X4Z60G1FXQTDC5XDJ05BAX). The note of the
    /// server that a command causes is a `posted` record of that
    /// command (01M3WRD9MGSC3FTBAANT4ZSMKY).
    #[tokio::test(start_paused = true)]
    async fn the_records_of_one_command_are_in_one_chunk_and_name_its_cause() {
        use riff_core::record::By;

        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        send(&service, claim_of(&mike2(), "issue-9")).await.unwrap();
        // Two commands wait for one write: they go in one chunk.
        store.hold.store(true, Ordering::SeqCst);
        let tries = store.tries.load(Ordering::SeqCst);
        let first = tokio::spawn(send(&service, post_body(&brett(), "one")));
        store.tried(tries + 1).await;
        let release = ReleaseFor {
            me: mike(),
            thread: mike().default_thread().unwrap(),
            item: "issue-9".into(),
            session: "a2".into(),
        };
        let freed = tokio::spawn(send(&service, release));
        let second = tokio::spawn(send(&service, post_body(&brett(), "two")));
        sleep(Duration::from_millis(50)).await;
        store.release();
        first.await.unwrap().unwrap();
        freed.await.unwrap().unwrap();
        second.await.unwrap().unwrap();

        let chunks = records(&store).await;
        let mut last = 0;
        for record in chunks.iter().flatten() {
            assert_eq!(record.envelope.position, last + 1);
            assert!(
                record.envelope.by.is_some() && record.envelope.command.is_some(),
                "{record:?}"
            );
            last = record.envelope.position;
        }
        // The first record of the log is of the server.
        let made = &chunks[0][0];
        assert_eq!(made.envelope.by, Some(By::Server));
        assert_eq!(made.envelope.command.as_deref(), Some("make_riff"));
        // The first call of a session registers it: a command of its own.
        let all: Vec<_> = chunks.iter().flatten().collect();
        let mut registered: Vec<_> = all
            .iter()
            .filter(|record| record.envelope.by == Some(By::Session(mike2().who().clone())))
            .map(|record| record.envelope.command.as_deref().unwrap())
            .collect();
        registered.dedup();
        assert_eq!(registered, ["register", "claim"]);

        // The release and its note: one chunk, one after another, with
        // the lead as the cause of the two. The sender of the note is
        // the server.
        let chunk = chunks.last().unwrap();
        let of_release: Vec<_> = chunk
            .iter()
            .filter(|record| record.envelope.command.as_deref() == Some("release_for"))
            .collect();
        assert_eq!(of_release.len(), 2, "{chunk:?}");
        let (released, note) = (of_release[0], of_release[1]);
        assert_eq!(note.envelope.position, released.envelope.position + 1);
        assert!(matches!(released.change, Change::Released(_)));
        let Change::Posted(posted) = &note.change else {
            panic!("a note: {note:?}");
        };
        assert_eq!(posted.message.from, owner::server_uri());
        for record in [released, note] {
            assert_eq!(record.envelope.by, Some(By::Session(mike().who().clone())));
        }
        // The post that came after it is in the same chunk, after it.
        let after: Vec<_> = chunk
            .iter()
            .filter(|record| record.envelope.position > note.envelope.position)
            .map(|record| record.envelope.command.as_deref().unwrap())
            .collect();
        assert_eq!(after, ["post"]);
    }

    /// A command that makes records gives no line. A refused command
    /// and a command with no change give one line each. A signal and a
    /// query give no line (01M3X4Z62RJREQ5H8F18Y85T6V).
    #[tokio::test(start_paused = true)]
    async fn only_a_command_with_no_record_gives_a_log_line() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let capture = Capture::start();
        let traced = |capture: &Capture| -> Vec<serde_json::Value> {
            let mut lines = capture.lines();
            lines.retain(|line| line.get("result").is_some());
            lines
        };

        // Records: no line.
        send(&service, claim_of(&mike(), "issue-7")).await.unwrap();
        send(&service, post_body(&mike(), "hello")).await.unwrap();
        assert!(traced(&capture).is_empty());

        // A signal and a query: no line.
        let Json(reply) = alive(
            AxumState(service.0.clone()),
            Proof::none(),
            Json(Alive {
                me: mike(),
                activity: None,
                prompt_secs: None,
            }),
        )
        .await
        .unwrap();
        assert!(!reply.stop);
        assert_eq!(claims(&service, &brett()).await, ["issue-7"]);
        assert!(traced(&capture).is_empty());

        // A refusal: one line, after the reply has its code.
        let refused = send(&service, claim_of(&brett(), "issue-7"))
            .await
            .unwrap_err();
        let Failed::Refused(refused) = refused else {
            panic!("a refusal");
        };
        assert_eq!(refused.code, state::Code::Held);
        let lines = traced(&capture);
        assert_eq!(lines.len(), 1, "{lines:?}");
        let line = lines[0].as_object().unwrap();
        assert_eq!(line["severity"], "INFO");
        assert_eq!(line["target"], "engine");
        assert_eq!(line["caller"], serde_json::json!({"session": "brett/b"}));
        assert_eq!(line["command"], "claim");
        assert_eq!(line["result"], "refused");
        assert_eq!(line["code"], "held");
        assert_eq!(line["reason"], refused.reason);
        // The call had no token, so the line has no key.
        assert!(!line.contains_key("key"));

        // No change: one line, with no code.
        send(&service, claim_of(&mike(), "issue-7")).await.unwrap();
        let lines = traced(&capture);
        assert_eq!(lines.len(), 2, "{lines:?}");
        let line = lines[1].as_object().unwrap();
        assert_eq!(line["caller"], serde_json::json!({"session": "mike/a"}));
        assert_eq!(line["command"], "claim");
        assert_eq!(line["result"], "no_change");
        assert!(!line.contains_key("code") && !line.contains_key("reason"));

        // A command of the server with no change names the server.
        assert_eq!(service.0.engine.forget().await, Ok(0));
        let lines = traced(&capture);
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert_eq!(lines[2]["caller"], "server");
        assert_eq!(lines[2]["command"], "forget");
        assert_eq!(lines[2]["result"], "no_change");
    }

    /// A chunk that is not written gives one line `failed` with the
    /// severity `ERROR` for each of its commands (01M3X4Z62RJREQ5H8F18Y85T6V).
    #[tokio::test(start_paused = true)]
    async fn a_chunk_that_is_not_written_gives_a_failed_line_for_each_command() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let capture = Capture::start();
        *store.fail.lock().unwrap() = Some(StoreError::Failed("503 from the bucket".into()));
        let lost = send(&service, post_body(&mike(), "lost")).await;
        assert_eq!(lost.unwrap_err(), Failed::Stopped);
        let lines = capture.results("failed");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["severity"], "ERROR");
        assert_eq!(lines[0]["caller"], serde_json::json!({"session": "mike/a"}));
        assert_eq!(lines[0]["command"], "post");
        assert!(capture.results("refused").is_empty());
        assert!(capture.results("no_change").is_empty());
    }

    /// A command that waits in the queue when the server stops for a
    /// lost lease gives the line `failed` with the severity `WARNING`
    /// and the reason of the stop: no alert goes out
    /// (01M3X4Z62RJREQ5H8F18Y85T6V).
    #[tokio::test(start_paused = true)]
    async fn a_command_that_waits_at_a_stop_gives_a_failed_line_that_is_no_error() {
        let store = Arc::new(Gated::default());
        let service = running(store.clone()).await;
        let capture = Capture::start();
        // The writer waits in a write, and a second command waits in
        // the queue.
        store.hold.store(true, Ordering::SeqCst);
        let tries = store.tries.load(Ordering::SeqCst);
        let first = tokio::spawn(send(&service, post_body(&mike(), "one")));
        store.tried(tries + 1).await;
        let second = tokio::spawn(send(&service, claim_of(&brett(), "issue-8")));
        sleep(Duration::from_millis(50)).await;
        service.0.stop("another instance holds the lease");
        assert_eq!(second.await.unwrap().unwrap_err(), Failed::Stopped);
        let lines = capture.results("failed");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["severity"], "WARNING");
        assert_eq!(
            lines[0]["caller"],
            serde_json::json!({"session": "brett/b"})
        );
        assert_eq!(lines[0]["command"], "claim");
        assert_eq!(lines[0]["reason"], "another instance holds the lease");
        store.release();
        let _ = first.await;
        let errors: Vec<_> = capture
            .lines()
            .into_iter()
            .filter(|line| line["severity"] == "ERROR")
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
    }

    /// The role of a caller with no token comes from the trust of the
    /// riff (01M3X4Z6G0TG0B4FT2N1FSPDHS).
    #[tokio::test(start_paused = true)]
    async fn the_role_of_a_caller_with_no_token_comes_from_the_trust_of_the_riff() {
        // Only a person changes the settings.
        let set = || SetIdle {
            me: "riff://mike@pangolin".parse().unwrap(),
            per_host: Some(2),
            after_secs: None,
        };
        // A riff with no sign-in trusts its network.
        let trusted = Service::new(config());
        assert!(trusted.config().trusted());
        assert!(send(&trusted, set()).await.is_ok());

        // A riff with a provider that takes a call with no token.
        let mut config = config();
        config.provider = Some(oidc::Provider {
            issuer: "https://accounts.google.com".into(),
            client_id: "riff".into(),
            client_secret: None,
            allowed_domains: vec!["comotechnologies.io".into()],
        });
        assert!(!config.require_sign_in && !config.trusted());
        let service = Service::new(config);
        let refused = send(&service, set()).await.unwrap_err();
        let Failed::Refused(refused) = refused else {
            panic!("a refusal: {refused:?}");
        };
        assert_eq!(refused.code, state::Code::NotAllowed);
        // A command that needs a member goes on. The session is the
        // lead after its register.
        let register = Register {
            me: mike(),
            worker: false,
        };
        send(&service, register).await.unwrap();
        assert!(send(&service, Resume::here(mike())).await.is_ok());
    }

    /// A marker that no log line may hold.
    const MARK: &str = "MARK-7f3a";

    /// Sends one call to the router of `service`, as `riff` does. With
    /// `token`, the call has that access token and a proof for it.
    async fn call(
        service: &Service,
        method: &str,
        path: &str,
        token: Option<(&str, String)>,
        body: String,
    ) -> StatusCode {
        use tower::ServiceExt;

        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(build::HEADER, Build::this().to_string())
            .header(header::CONTENT_TYPE, "application/json");
        if let Some((token, proof)) = token {
            request = request
                .header(header::AUTHORIZATION, format!("DPoP {token}"))
                .header("dpop", proof);
        }
        let request = request.body(axum::body::Body::from(body)).unwrap();
        let response = service.router().oneshot(request).await.unwrap();
        response.status()
    }

    /// One call of each routed command of mike, with a marked body in
    /// the post, and one more post that the state refuses.
    fn each_command() -> Vec<(&'static str, String)> {
        fn of<C: Routed + serde::Serialize>(command: C) -> (&'static str, String) {
            (C::PATH, serde_json::to_string(&command).unwrap())
        }
        let me = mike;
        let thread = || mike().default_thread().unwrap();
        let design = || "design".parse::<ThreadName>().unwrap();
        let item = || "issue-7".to_owned();
        let body = format!("{MARK} is the body");
        let gone = vec![Selector::session("no-such-session")];
        vec![
            of(Register {
                me: me(),
                worker: false,
            }),
            of(Join {
                me: me(),
                thread: design(),
            }),
            of(Post::new(&me(), Some(thread()), vec![], &body)),
            // A direct message to a session that is not there: refused.
            of(Post::new(&me(), None, gone, &body)),
            of(Leave {
                me: me(),
                thread: design(),
            }),
            of(Resume::whole(me())),
            of(Claim {
                me: me(),
                thread: thread(),
                item: item(),
            }),
            of(Release {
                me: me(),
                thread: thread(),
                item: item(),
            }),
            // Nobody holds the item now: refused.
            of(ReleaseFor {
                me: me(),
                thread: thread(),
                item: item(),
                session: "a2".into(),
            }),
            of(Lead { me: me() }),
            of(Pause::here(me())),
            of(SetIdle {
                me: me(),
                per_host: Some(1),
                after_secs: None,
            }),
            of(Start {
                me: me(),
                reason: riff_core::wire::StartReason::Process,
                worker: false,
            }),
            of(End { me: me() }),
        ]
    }

    /// No line holds the body of a post or a token (01M3X4Z675D0ZQX93E93F3M8FA). The
    /// test runs each command with a marked body and a marked token: in
    /// a riff with no sign-in, which takes them, and in a riff with
    /// sign-in, which refuses each one.
    #[tokio::test(start_paused = true)]
    async fn no_log_line_holds_the_body_of_a_post_or_a_token() {
        let key = riff_core::dpop::Key::generate();
        let token = format!("{MARK}-token");
        let now_secs = now_ms() / 1000;
        let proof = |config: &Config, path: &str| {
            let url = config.url(path);
            let proof = key.proof("POST", &url, Some(&token), now_secs);
            (token.as_str(), proof)
        };
        let commands = each_command();
        // Each of the 13 routed commands is there.
        let mut paths: Vec<&str> = commands.iter().map(|(path, _)| *path).collect();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), 13);

        let capture = Capture::start();
        let open = Service::new(config());
        let mut refused = 0;
        for (path, body) in &commands {
            let token = Some(proof(open.config(), path));
            let status = call(&open, "POST", path, token, body.clone()).await;
            assert!(
                status.is_success() || [400, 403, 409].contains(&status.as_u16()),
                "{path}: {status}"
            );
            refused += usize::from(!status.is_success());
        }
        open.save().await.unwrap();
        assert!(refused >= 2, "some commands are refused");
        assert_eq!(capture.results("refused").len(), refused);
        assert!(!capture.results("no_change").is_empty());
        // A note of the server with a marked body.
        let body = format!("{MARK} is the body of a note");
        let thread = mike().default_thread();
        let note = Server::news(thread, Vec::new(), &body, Kind::Note);
        open.0.engine.announce(note).await.unwrap();

        let mut config = config();
        config.require_sign_in = true;
        let closed = Service::new(config);
        for (path, body) in &commands {
            // A token that the server does not know, with a good proof.
            let good = Some(proof(closed.config(), path));
            let status = call(&closed, "POST", path, good, body.clone()).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
            // The same token with a proof that does not read.
            let bad = Some((token.as_str(), format!("{MARK}-proof")));
            let status = call(&closed, "POST", path, bad, body.clone()).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
            // No token.
            let status = call(&closed, "POST", path, None, body.clone()).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
        }
        let denied = capture.results("denied");
        assert_eq!(denied.len(), 3 * commands.len());
        let mut codes: Vec<&str> = denied
            .iter()
            .map(|line| line["code"].as_str().unwrap())
            .collect();
        codes.dedup();
        assert_eq!(&codes[..3], ["bad_token", "bad_proof", "no_token"]);
        for line in &denied {
            let line = line.as_object().unwrap();
            assert_eq!(line["severity"], "INFO");
            assert_eq!(line["named"], serde_json::json!({"session": "mike/a"}));
            assert_eq!(line["proved"], false);
            assert!(line["path"].as_str().unwrap().starts_with("/v1/"));
            for absent in ["caller", "command", "key", "reason"] {
                assert!(!line.contains_key(absent), "{absent}");
            }
        }

        let text = capture.text();
        assert!(!text.contains(MARK), "{text}");
    }

    /// The line `denied` of a call that names its caller in the query,
    /// of a call from an old build, and of a call whose `me` is long or
    /// has a line break (01M3X4Z64ZNRD0G0F4JV1M64FN). The line stays one line, and a
    /// long name is cut.
    #[tokio::test(start_paused = true)]
    async fn a_denied_line_names_the_caller_of_the_call_with_a_limit() {
        let mut config = config();
        config.require_sign_in = true;
        let service = Service::new(config);
        let capture = Capture::start();
        let body = |me: &str| serde_json::json!({ "me": me }).to_string();

        // The stream of a session names it in the query.
        let path =
            "/v1/watch?uri=riff%3A%2F%2Fmike%40pangolin%2Fcomo-technologies%2Friff%3Fsession%3Da";
        let status = call(&service, "GET", path, None, String::new()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        // A `me` of 32 KiB is cut.
        let status = call(
            &service,
            "POST",
            "/v1/claim",
            None,
            body(&"x".repeat(32 * 1024)),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        // A `me` of 64 KiB makes a body past the limit: no name.
        let status = call(
            &service,
            "POST",
            "/v1/claim",
            None,
            body(&"x".repeat(64 * 1024)),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        // A `me` with a line break.
        let broken = "riff://mike@pangolin\n{\"severity\":\"ERROR\"}";
        let status = call(&service, "POST", "/v1/claim", None, body(broken)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        // A call with no body.
        let status = call(&service, "POST", "/v1/claim", None, String::new()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // `lines` reads each line of the output as one JSON object.
        let lines = capture.results("denied");
        assert_eq!(lines.len(), 5, "{lines:?}");
        for line in &lines {
            assert_eq!(line["severity"], "INFO");
            assert_eq!(line["code"], "no_token");
            assert!(line.to_string().len() < 600, "{line}");
        }
        let has = |line: &serde_json::Value, field: &str| line.get(field).is_some();
        assert_eq!(lines[0]["path"], "/v1/watch");
        assert_eq!(lines[0]["named"], serde_json::json!({"session": "mike/a"}));
        assert_eq!(lines[0]["proved"], false);
        let cut = lines[1]["named"]["text"].as_str().unwrap();
        assert_eq!(cut.len(), trace::NAMED_MAX);
        assert_eq!(lines[1]["named_cut"], true);
        assert!(!has(&lines[2], "named") && !has(&lines[2], "proved"));
        assert_eq!(lines[3]["named"]["text"], broken);
        assert!(!has(&lines[3], "named_cut"));
        assert!(!has(&lines[4], "named"));
    }

    /// A refused call whose body does not end gets its reply at the
    /// time limit (01M3Z67B9RMVKY7TCXCG8HEZT4): the line has no `named`,
    /// and the reply closes the call.
    #[tokio::test(start_paused = true)]
    async fn a_refused_call_with_a_slow_body_ends_at_the_time_limit() {
        use tower::ServiceExt;

        let mut config = config();
        config.require_sign_in = true;
        let service = Service::new(config);
        let capture = Capture::start();
        let send = |body: axum::body::Body| {
            let request = Request::builder()
                .method("POST")
                .uri("/v1/claim")
                .header(build::HEADER, Build::this().to_string())
                .header(header::CONTENT_TYPE, "application/json")
                .body(body)
                .unwrap();
            service.router().oneshot(request)
        };

        // The body gives its start, and then nothing.
        let start = r#"{"me":"riff://mike@pangolin/como-technologies/riff?session=a"#;
        let first = tokio_stream::once(Ok::<_, Infallible>(start));
        let slow = tokio_stream::StreamExt::chain(first, tokio_stream::pending());
        let slow = axum::body::Body::from_stream(slow);
        let began = tokio::time::Instant::now();
        let response = send(slow).await.unwrap();
        assert_eq!(began.elapsed(), trace::BODY_TIME);
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.headers().contains_key(header::WWW_AUTHENTICATE));
        assert_eq!(response.headers()[header::CONNECTION], "close");

        // A call with a whole body keeps its connection.
        let whole = format!("{start}\"}}");
        let response = send(whole.into()).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(!response.headers().contains_key(header::CONNECTION));

        let lines = capture.results("denied");
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0]["code"], "no_token");
        assert!(lines[0].get("named").is_none(), "{}", lines[0]);
        assert_eq!(lines[1]["named"], serde_json::json!({"session": "mike/a"}));
    }

    /// 1000 refused calls in one second give no more `denied` lines
    /// than the limit, and one line with the count
    /// (01M3Z67DZX9BC3TYF3PWGFGZJ7). Each call gets the same refusal, with
    /// a line and with no line. The limit also holds for the calls of
    /// an old build and for a token that acts as another session.
    #[tokio::test(start_paused = true)]
    async fn a_thousand_refused_calls_give_the_lines_of_the_limit_and_one_count() {
        use tower::ServiceExt;

        let mut config = config();
        config.require_sign_in = true;
        let service = Service::new(config);
        let capture = Capture::start();
        let body = serde_json::to_string(&Lead { me: mike() }).unwrap();
        let proof = || Proof {
            signed_in: Some(SignedIn {
                who: brett().who().clone(),
                jkt: "key-of-brett".into(),
                started: 0,
            }),
            path: Alive::PATH.to_owned(),
        };
        for call_number in 0..1000 {
            match call_number % 3 {
                // No token.
                0 => {
                    let status = call(&service, "POST", Lead::PATH, None, body.clone()).await;
                    assert_eq!(status, StatusCode::UNAUTHORIZED);
                }
                // No build.
                1 => {
                    let request = Request::builder()
                        .method("POST")
                        .uri(Lead::PATH)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(axum::body::Body::from(body.clone()))
                        .unwrap();
                    let response = service.router().oneshot(request).await.unwrap();
                    assert_eq!(response.status(), StatusCode::CONFLICT);
                    assert!(!response.headers().contains_key(header::CONNECTION));
                }
                // The token of brett names mike.
                _ => {
                    let refused = alive(
                        AxumState(service.0.clone()),
                        proof(),
                        Json(Alive {
                            me: mike(),
                            activity: None,
                            prompt_secs: None,
                        }),
                    )
                    .await
                    .unwrap_err();
                    assert_eq!(refused.0, StatusCode::FORBIDDEN);
                }
            }
            sleep(Duration::from_millis(1)).await;
        }
        let max = usize::try_from(trace::DENIED_MAX).unwrap();
        assert_eq!(capture.results("denied").len(), max);
        assert!(capture.results("dropped").is_empty());

        sleep(trace::DENIED_INTERVAL).await;
        let dropped = capture.results("dropped");
        assert_eq!(dropped.len(), 1, "{dropped:?}");
        assert_eq!(dropped[0]["severity"], "WARNING");
        assert_eq!(dropped[0]["count"], 1000 - max);
        assert_eq!(capture.results("denied").len(), max);
    }

    /// Done when of #447: 1000 calls with no token and 5 calls with a
    /// valid token of the wrong session in one window give a `denied`
    /// line for each of the 5 calls (01M419Z1V3YT48PR2NTFYWAXG9), and one
    /// `dropped` line with a count for each code (01M419Z1RM0TDJ50F6SEJC40GB).
    #[tokio::test(start_paused = true)]
    async fn a_flood_with_no_token_does_not_hide_a_valid_token_of_the_wrong_session() {
        let mut config = config();
        config.require_sign_in = true;
        let service = Service::new(config);
        let capture = Capture::start();
        let body = serde_json::to_string(&Lead { me: mike() }).unwrap();
        for _ in 0..1000 {
            let status = call(&service, "POST", Lead::PATH, None, body.clone()).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
            sleep(Duration::from_millis(1)).await;
        }
        for _ in 0..5 {
            let proof = Proof {
                signed_in: Some(SignedIn {
                    who: brett().who().clone(),
                    jkt: "key-of-brett".into(),
                    started: 0,
                }),
                path: Alive::PATH.to_owned(),
            };
            let refused = alive(
                AxumState(service.0.clone()),
                proof,
                Json(Alive {
                    me: mike(),
                    activity: None,
                    prompt_secs: None,
                }),
            )
            .await
            .unwrap_err();
            assert_eq!(refused.0, StatusCode::FORBIDDEN);
        }
        let denied = capture.results("denied");
        let of = |code: &str| denied.iter().filter(|line| line["code"] == code).count();
        assert_eq!(of("not_you"), 5, "{denied:?}");
        let open = trace::DENIED_MAX - trace::DENIED_KEPT;
        assert_eq!(of("no_token") as u64, open);

        sleep(trace::DENIED_INTERVAL).await;
        let dropped = capture.results("dropped");
        assert_eq!(dropped.len(), 1, "{dropped:?}");
        assert_eq!(dropped[0]["count"], 1000 - open);
        assert_eq!(
            dropped[0]["counts"],
            serde_json::json!({"no_token": 1000 - open})
        );
    }

    /// A stop of the server ends the window of the limit of rate: the
    /// line with the count comes at once (01M3Z67DZX9BC3TYF3PWGFGZJ7).
    #[tokio::test(start_paused = true)]
    async fn a_stop_writes_the_count_of_the_denied_lines_that_were_not_written() {
        let max = usize::try_from(trace::DENIED_MAX - trace::DENIED_KEPT).unwrap();
        for stop_for_good in [false, true] {
            let mut config = config();
            config.require_sign_in = true;
            let service = Service::new(config);
            let capture = Capture::start();
            for _ in 0..max + 5 {
                let status = call(&service, "POST", Lead::PATH, None, String::new()).await;
                assert_eq!(status, StatusCode::UNAUTHORIZED);
            }
            assert!(capture.results("dropped").is_empty());
            if stop_for_good {
                service.0.stop("another instance holds the lease");
            } else {
                service.shutdown().await.unwrap();
            }
            let dropped = capture.results("dropped");
            assert_eq!(dropped.len(), 1, "{dropped:?}");
            assert_eq!(dropped[0]["count"], 5);
            assert_eq!(dropped[0]["counts"], serde_json::json!({"no_token": 5}));
            // The timer of the window writes no second line.
            sleep(trace::DENIED_INTERVAL * 2).await;
            assert_eq!(capture.results("dropped").len(), 1);
        }
    }

    /// A call from a build that the server cannot talk to gives the
    /// line `denied` with the code `old_build`, and a token that acts
    /// as another session the code `not_you` (01M3X4Z64ZNRD0G0F4JV1M64FN).
    #[tokio::test(start_paused = true)]
    async fn a_denied_line_has_the_code_of_the_refusal() {
        use tower::ServiceExt;

        let service = Service::new(config());
        let capture = Capture::start();
        let body = serde_json::to_string(&Lead { me: mike() }).unwrap();
        let request = Request::builder()
            .method("POST")
            .uri(Lead::PATH)
            .header(header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(body))
            .unwrap();
        let response = service.router().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let lines = capture.results("denied");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["code"], "old_build");
        assert_eq!(lines[0]["path"], Lead::PATH);
        assert_eq!(lines[0]["named"], serde_json::json!({"session": "mike/a"}));

        // The token of brett names mike in the body.
        let proof = Proof {
            signed_in: Some(SignedIn {
                who: brett().who().clone(),
                jkt: "key-of-brett".into(),
                started: 0,
            }),
            path: Alive::PATH.to_owned(),
        };
        let refused = alive(
            AxumState(service.0.clone()),
            proof,
            Json(Alive {
                me: mike(),
                activity: None,
                prompt_secs: None,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(refused.0, StatusCode::FORBIDDEN);
        let lines = capture.results("denied");
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[1]["code"], "not_you");
        assert_eq!(lines[1]["path"], Alive::PATH);
        assert_eq!(lines[1]["named"], serde_json::json!({"session": "mike/a"}));
        assert_eq!(lines[1]["proved"], false);
    }

    /// The line of a command with a token has the thumbprint of the
    /// device key of the token, and not the token.
    #[tokio::test(start_paused = true)]
    async fn the_line_of_a_command_with_a_token_has_its_key() {
        let service = Service::new(config());
        let capture = Capture::start();
        let proof = SignedIn {
            who: mike().who().clone(),
            jkt: "thumbprint-of-mike".into(),
            started: 0,
        };
        let engine = &service.0.engine;
        // A new riff is paused: the claim is refused.
        let call = engine
            .authenticate(Some(&proof), claim_of(&mike(), "issue-7"))
            .unwrap();
        let refused = engine.dispatch(call).await.unwrap_err();
        assert!(matches!(&refused, Failed::Refused(r) if r.code == state::Code::Paused));
        let lines = capture.results("refused");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["key"], "thumbprint-of-mike");
        assert_eq!(lines[0]["code"], "paused");
    }

    const ADA: &str = "ada@gmail.com";
    const BOB: &str = "bob@gmail.com";

    /// The settings of a riff with sign-in that takes a call with no
    /// token. No test calls the provider.
    fn signed_config() -> Config {
        let mut config = config();
        config.provider = Some(oidc::Provider {
            issuer: "https://accounts.example.com".into(),
            client_id: "riff".into(),
            client_secret: None,
            allowed_domains: Vec::new(),
        });
        config
    }

    /// Sends a command of the people as the person `user`, with the
    /// proof of a token: through the one path.
    fn as_person<C: Routed>(
        service: &Service,
        user: &str,
        command: C,
    ) -> impl Future<Output = Result<<C as Call>::Reply, Failed>> + use<C> {
        // A sign-in that is newer than each end of sign-ins.
        let proof = SignedIn {
            who: Who::new(user, None).unwrap(),
            jkt: format!("key-of-{user}"),
            started: u64::MAX,
        };
        with_proof(service, proof, command)
    }

    /// Sends a command with the proof of a token: through the one path.
    fn with_proof<C: Routed>(
        service: &Service,
        proof: SignedIn,
        command: C,
    ) -> impl Future<Output = Result<<C as Call>::Reply, Failed>> + use<C> {
        let engine = service.0.engine.clone();
        async move {
            let call = engine.authenticate(Some(&proof), command)?;
            engine.dispatch(call).await
        }
    }

    /// Between the entry of a removal in the queue and the end of the
    /// sign-ins after its write, the removed person sends no command:
    /// not as a person, and not through a session
    /// (01M3XGP03RDF6S15JYS718WWFC). The log has no record of that
    /// person after `member_removed`. A revoke has the same rule, with
    /// another code: the person is still a member.
    #[tokio::test(start_paused = true)]
    async fn a_removed_person_sends_no_command_while_the_removal_waits() {
        let store = Arc::new(Gated::default());
        let service = riff_of_ada(store.clone()).await;
        let now = Instant::now();
        let session: SessionUri = "riff://bob@kite/como-technologies/riff?session=b1"
            .parse()
            .unwrap();
        // The proof of a token of bob: of the person, and of a session.
        let proofs = |service: &Service, pair: &TokenReply| {
            let tokens = service.tokens();
            let (who, started) = tokens
                .signed_in(&pair.access_token, "key-of-bob", now)
                .unwrap();
            let proof = |who: Who| SignedIn {
                who,
                jkt: "key-of-bob".into(),
                started,
            };
            (proof(who), proof(session.who().clone()))
        };
        let join = |thread: &str| Join {
            me: session.clone(),
            thread: thread.parse().unwrap(),
        };
        let last = |chunks: Vec<Vec<Change>>| chunks.into_iter().flatten().last().unwrap();

        as_person(&service, "ada", invite(BOB)).await.unwrap();
        let bob = service.admit(BOB, false, "key-of-bob").await.unwrap();
        let (person, in_session) = proofs(&service, &bob);
        // Before the removal, the session of bob sends a command.
        with_proof(&service, in_session.clone(), join("design"))
            .await
            .unwrap();

        // The removal is in the queue, and its write waits.
        let tries = store.tries.load(Ordering::SeqCst);
        store.hold.store(true, Ordering::SeqCst);
        let removal = tokio::spawn(as_person(&service, "ada", remove(BOB)));
        store.tried(tries + 1).await;
        let by_session = tokio::spawn(with_proof(&service, in_session, join("plans")));
        let revoke = Revoke { user: None };
        let by_person = tokio::spawn(with_proof(&service, person, revoke));
        sleep(Duration::from_millis(50)).await;
        store.release();
        assert_eq!(removal.await.unwrap().unwrap().sign_ins, 1);
        assert_eq!(code(by_session.await.unwrap()), state::Code::NotMember);
        assert_eq!(code(by_person.await.unwrap()), state::Code::NotMember);
        service.save().await.unwrap();
        // The removal is the last record: bob changed nothing after it.
        assert!(matches!(
            last(store.chunks().await),
            Change::MemberRemoved(removed) if removed.email == BOB
        ));

        // A revoke: bob is a member, and a sign-in from before the
        // record sends no command.
        as_person(&service, "ada", invite(BOB)).await.unwrap();
        let bob = service.admit(BOB, false, "key-of-bob").await.unwrap();
        let (_, in_session) = proofs(&service, &bob);
        let tries = store.tries.load(Ordering::SeqCst);
        store.hold.store(true, Ordering::SeqCst);
        let revoke = Revoke {
            user: Some("bob".into()),
        };
        let revoked = tokio::spawn(as_person(&service, "ada", revoke));
        store.tried(tries + 1).await;
        let by_session = tokio::spawn(with_proof(&service, in_session, join("plans")));
        sleep(Duration::from_millis(50)).await;
        store.release();
        assert_eq!(revoked.await.unwrap().unwrap().sign_ins, 1);
        let Err(Failed::Refused(refused)) = by_session.await.unwrap() else {
            panic!("a refusal");
        };
        assert_eq!(refused.code, state::Code::NotAllowed);
        assert!(refused.reason.contains("Sign in again"), "{refused}");
        service.save().await.unwrap();
        assert!(matches!(
            last(store.chunks().await),
            Change::SigninsEnded(ended) if ended.user == "bob"
        ));
        // A new sign-in of bob sends a command again.
        let bob = service.admit(BOB, false, "key-of-bob").await.unwrap();
        let (_, in_session) = proofs(&service, &bob);
        with_proof(&service, in_session, join("plans"))
            .await
            .unwrap();
    }

    /// The proof of the provider for `email`, with no allowed domain.
    fn identity(email: &str) -> Identity {
        Identity {
            email: email.to_owned(),
            user: oidc::user_of(email).unwrap(),
            allowed_domain: false,
        }
    }

    fn invite(email: &str) -> Invite {
        Invite {
            email: email.into(),
        }
    }

    fn remove(email: &str) -> Remove {
        Remove {
            email: email.into(),
        }
    }

    /// A riff with sign-in on `store`. Ada is the owner.
    async fn riff_of_ada(store: Arc<Gated>) -> Service {
        let service = Service::load(signed_config(), store).await.unwrap();
        service.admit(ADA, false, "key-of-ada").await.unwrap();
        service
    }

    /// The code of a refused command.
    fn code<T: std::fmt::Debug>(reply: Result<T, Failed>) -> state::Code {
        match reply {
            Err(Failed::Refused(refused)) => refused.code,
            other => panic!("not a refusal: {other:?}"),
        }
    }

    /// A riff with no sign-in has no people: it refuses each command of
    /// the people with the code `no_sign_in`, before `permits`
    /// (01M3WRD9G5GAF65EX8P6D5DMQM). The line of the refusal has the
    /// code and no reason.
    #[tokio::test(start_paused = true)]
    async fn a_riff_with_no_sign_in_refuses_each_command_of_the_people() {
        let capture = Capture::start();
        let service = Service::new(config());
        assert!(service.config().trusted());
        let no_sign_in = state::Code::NoSignIn;
        assert_eq!(
            code(as_person(&service, "ada", invite(BOB)).await),
            no_sign_in
        );
        assert_eq!(
            code(as_person(&service, "ada", remove(BOB)).await),
            no_sign_in
        );
        let set = SetAdmin {
            email: BOB.into(),
            admin: true,
        };
        assert_eq!(code(as_person(&service, "ada", set).await), no_sign_in);
        let pass = PassOwner { email: BOB.into() };
        assert_eq!(code(as_person(&service, "ada", pass).await), no_sign_in);
        assert_eq!(
            code(as_person(&service, "ada", TakeOwner {}).await),
            no_sign_in
        );
        assert_eq!(
            code(as_person(&service, "ada", DenyOwner {}).await),
            no_sign_in
        );
        let revoke = Revoke { user: None };
        assert_eq!(code(as_person(&service, "ada", revoke).await), no_sign_in);
        // The sign-in, and the commands of the server.
        assert_eq!(
            code(service.0.engine.sign_in(&identity(ADA)).await),
            no_sign_in
        );
        assert_eq!(code(service.0.engine.grant_owner().await), no_sign_in);
        assert_eq!(code(service.0.engine.end_owner().await), no_sign_in);
        let refused = service.admit(ADA, false, "k").await.unwrap_err();
        assert!(refused.contains("this riff has no sign-in"), "{refused}");
        assert_eq!(service.members().owner, None);

        let lines = capture.results("refused");
        assert_eq!(lines.len(), 11, "{lines:?}");
        for line in &lines {
            let line = line.as_object().unwrap();
            assert_eq!(line["code"], "no_sign_in");
            assert!(!line.contains_key("reason"), "{line:?}");
        }
        assert_eq!(lines[0]["command"], "invite");
        assert_eq!(lines[0]["caller"], serde_json::json!({"person": "ada"}));
        // The line of a sign-in names its USER, not its email.
        assert_eq!(lines[7]["command"], "admit");
        assert_eq!(lines[7]["caller"], serde_json::json!({"sign_in": "ada"}));
    }

    /// The first sign-in of a person runs `admit` through the dispatch,
    /// and makes its records one time (01M3XA877YZQ649SWB5TN60V5P): a
    /// second try makes no second record, also after a new start.
    #[tokio::test(start_paused = true)]
    async fn a_second_try_of_a_first_sign_in_makes_no_second_record() {
        let store = Arc::new(Gated::default());
        let service = riff_of_ada(store.clone()).await;
        as_person(&service, "ada", invite(BOB)).await.unwrap();
        let joined = |chunks: Vec<Vec<Change>>| {
            chunks
                .into_iter()
                .flatten()
                .filter(|change| matches!(change, Change::PersonJoined(p) if p.user == "bob"))
                .count()
        };
        // The command is written, and the server stops before the chain.
        let first = service.0.engine.sign_in(&identity(BOB)).await.unwrap();
        assert_eq!(first.user, "bob");
        assert_eq!(joined(store.chunks().await), 1);
        assert_eq!(service.tokens().chains(), 1, "only the chain of ada");
        // The same server: the second try makes no record, and the chain.
        let again = service.0.engine.sign_in(&identity(BOB)).await.unwrap();
        assert!(again.position >= first.position);
        assert_eq!(joined(store.chunks().await), 1);
        service.save().await.unwrap();
        drop(service);

        // A new start: the person signs in, with no second record.
        let next = Service::load(signed_config(), store.clone()).await.unwrap();
        let bob = next.admit(BOB, false, "key-of-bob").await.unwrap();
        assert_eq!(bob.user, "bob");
        next.save().await.unwrap();
        assert_eq!(joined(store.chunks().await), 1);
        // The log shows who caused the record: the sign-in.
        let people = next.0.engine.read(|state| state.people().persons());
        assert_eq!(people.len(), 2);
    }

    /// A sign-in that is in flight when a `remove` comes does not stay,
    /// at each point of the removal (01M3XA87A9GGFA89RQXWSKY0V6): the
    /// people checked the person before the removal, and the chain
    /// starts later.
    #[tokio::test(start_paused = true)]
    async fn a_sign_in_in_flight_at_a_removal_does_not_stay() {
        let store = Arc::new(Gated::default());
        let service = riff_of_ada(store.clone()).await;
        let now = Instant::now();
        let refused = |token: &str| {
            let check = service.tokens().check(token, "key-of-bob", now);
            assert_eq!(check, Err(token::Refused::Unknown));
        };

        // 1. The chain starts before the record is in the queue.
        as_person(&service, "ada", invite(BOB)).await.unwrap();
        let bob = service.admit(BOB, false, "key-of-bob").await.unwrap();
        let removed = as_person(&service, "ada", remove(BOB)).await.unwrap();
        assert_eq!(removed.sign_ins, 1);
        refused(&bob.access_token);

        // 2. The chain starts between the queue and the write: the
        // record of the removal waits for its chunk.
        as_person(&service, "ada", invite(BOB)).await.unwrap();
        let checked = service.0.engine.sign_in(&identity(BOB)).await.unwrap();
        let tries = store.tries.load(Ordering::SeqCst);
        store.hold.store(true, Ordering::SeqCst);
        let removal = tokio::spawn(as_person(&service, "ada", remove(BOB)));
        store.tried(tries + 1).await;
        let bob = service
            .tokens()
            .start("bob", "key-of-bob", checked.position, now)
            .unwrap();
        assert!(
            service
                .tokens()
                .check(&bob.access_token, "key-of-bob", now)
                .is_ok()
        );
        store.release();
        assert_eq!(removal.await.unwrap().unwrap().sign_ins, 1);
        refused(&bob.access_token);

        // 3. The chain starts between the write and the effect: the
        // chunk is in the log, and the writer did not end the sign-ins
        // yet.
        as_person(&service, "ada", invite(BOB)).await.unwrap();
        let checked = service.0.engine.sign_in(&identity(BOB)).await.unwrap();
        let (tokens, started) = (service.0.tokens.clone(), Arc::new(Mutex::new(None)));
        let keep = started.clone();
        *store.after_chunk.lock().unwrap() = Some(Box::new(move || {
            let pair = tokens
                .lock()
                .unwrap()
                .start("bob", "key-of-bob", checked.position, now);
            *keep.lock().unwrap() = Some(pair);
        }));
        let removed = as_person(&service, "ada", remove(BOB)).await.unwrap();
        assert_eq!(removed.sign_ins, 1);
        let bob = started.lock().unwrap().take().unwrap().unwrap();
        refused(&bob.access_token);

        // 4. The chain starts after the effect: the token store refuses
        // a sign-in from before the removal.
        as_person(&service, "ada", invite(BOB)).await.unwrap();
        let checked = service.0.engine.sign_in(&identity(BOB)).await.unwrap();
        let removed = as_person(&service, "ada", remove(BOB)).await.unwrap();
        assert_eq!(removed.sign_ins, 0);
        let late = service
            .tokens()
            .start("bob", "key-of-bob", checked.position, now);
        assert_eq!(late, Err(token::NoSignIn::Ended));
        assert!(service.tokens().keys("bob", now).is_empty());

        // The person is no member now, and does not sign in.
        let refused = service.admit(BOB, false, "key-of-bob").await.unwrap_err();
        assert!(refused.contains("riff invite bob@gmail.com"), "{refused}");
        // After a new invite, a new sign-in stays.
        as_person(&service, "ada", invite(BOB)).await.unwrap();
        let bob = service.admit(BOB, false, "key-of-bob").await.unwrap();
        assert!(
            service
                .tokens()
                .check(&bob.access_token, "key-of-bob", now)
                .is_ok()
        );
    }

    /// The server stops between the write of a removal and the end of
    /// the sign-ins. After the start, the removed person does not get
    /// in (01M3XA87A9GGFA89RQXWSKY0V6): the load drops each sign-in from
    /// before the removal. A revoke has the same rule.
    #[tokio::test(start_paused = true)]
    async fn a_stop_between_a_removal_and_the_end_of_the_sign_ins_lets_nobody_in() {
        let store = Arc::new(Gated::default());
        let service = riff_of_ada(store.clone()).await;
        as_person(&service, "ada", invite(BOB)).await.unwrap();
        let bob = service.admit(BOB, false, "key-of-bob").await.unwrap();
        let ada = service.admit(ADA, false, "key-of-ada").await.unwrap();
        service.save().await.unwrap();
        // The sign-ins as the store has them before the removal.
        let before = store.store.load(SIGN_INS).await.unwrap().unwrap().bytes;
        as_person(&service, "ada", remove(BOB)).await.unwrap();
        let revoke = Revoke { user: None };
        as_person(&service, "ada", revoke).await.unwrap();
        service.save().await.unwrap();
        drop(service);

        // The store of a server that wrote the two records, and stopped
        // before it ended a sign-in: the log, and the old sign-ins.
        let stopped = Arc::new(Gated::default());
        for name in store.store.list(log::LOG).await.unwrap() {
            let chunk = store.store.load(&name).await.unwrap().unwrap().bytes;
            stopped.store.save(&name, chunk, None).await.unwrap();
        }
        stopped.store.save(SIGN_INS, before, None).await.unwrap();
        let next = Service::load(signed_config(), stopped).await.unwrap();
        let now = Instant::now();
        for (pair, key) in [(&bob, "key-of-bob"), (&ada, "key-of-ada")] {
            let refresh = next.tokens().refresh(&pair.refresh_token, key, now);
            assert_eq!(refresh, Err(token::Refused::Unknown));
        }
        assert_eq!(next.tokens().chains(), 0);
        // The removed person does not sign in. The owner signs in again.
        assert!(next.admit(BOB, false, "key-of-bob").await.is_err());
        assert!(next.admit(ADA, false, "key-of-ada").await.is_ok());
    }

    /// A mark in the domain of each email. The people keep an email in
    /// lower case.
    const EMAIL_MARK: &str = "mark-9c2e.example";

    /// No log line holds an email (01M3XA87CJHCGZX283ZQAFKARZ). The test
    /// runs each command of the people with a marked email: accepted,
    /// with no change, and refused. An email is in a record, and in a
    /// reply to a member.
    #[tokio::test(start_paused = true)]
    async fn no_log_line_holds_an_email() {
        let email = |user: &str| format!("{user}@{EMAIL_MARK}");
        let capture = Capture::start();
        let store = Arc::new(Gated::default());
        let mut config = signed_config();
        config.admins = vec![email("carol")];
        config.owner = Some(email("ada"));
        let service = Service::load(config, store.clone()).await.unwrap();
        // The sign-in: accepted, a second time, and refused.
        service.admit(&email("ada"), false, "k").await.unwrap();
        service.admit(&email("ada"), false, "k").await.unwrap();
        service.admit(&email("eve"), false, "k").await.unwrap_err();
        // An email that gives no valid USER.
        let no_user = format!("@{EMAIL_MARK}");
        service.admit(&no_user, true, "k").await.unwrap_err();
        service
            .admit(&format!("ada@other.{EMAIL_MARK}"), true, "k")
            .await
            .unwrap_err();
        // Each command of a person.
        let invite = |user: &str| invite(&email(user));
        as_person(&service, "ada", invite("bob")).await.unwrap();
        as_person(&service, "ada", invite("bob")).await.unwrap();
        service.admit(&email("bob"), false, "k").await.unwrap();
        service.admit(&email("carol"), false, "k").await.unwrap();
        as_person(&service, "bob", invite("dan")).await.unwrap_err();
        as_person(&service, "bob", remove(&email("ada")))
            .await
            .unwrap_err();
        as_person(&service, "ada", remove(&email("ada")))
            .await
            .unwrap_err();
        let set = |user: &str, admin| SetAdmin {
            email: email(user),
            admin,
        };
        as_person(&service, "ada", set("dan", false))
            .await
            .unwrap_err();
        as_person(&service, "bob", set("dan", true))
            .await
            .unwrap_err();
        as_person(&service, "ada", set("bob", true)).await.unwrap();
        let pass = |user: &str| PassOwner { email: email(user) };
        as_person(&service, "ada", pass("eve")).await.unwrap_err();
        as_person(&service, "bob", pass("bob")).await.unwrap_err();
        as_person(&service, "ada", DenyOwner {}).await.unwrap_err();
        as_person(&service, "ada", TakeOwner {}).await.unwrap();
        as_person(&service, "bob", TakeOwner {}).await.unwrap();
        as_person(&service, "carol", TakeOwner {})
            .await
            .unwrap_err();
        as_person(&service, "ada", DenyOwner {}).await.unwrap();
        let revoke = |user: Option<&str>| Revoke {
            user: user.map(str::to_owned),
        };
        as_person(&service, "eve", revoke(Some("ada")))
            .await
            .unwrap_err();
        as_person(&service, "ada", revoke(Some("bob")))
            .await
            .unwrap();
        as_person(&service, "ada", remove(&email("bob")))
            .await
            .unwrap_err();
        as_person(&service, "ada", set("bob", false)).await.unwrap();
        as_person(&service, "ada", remove(&email("bob")))
            .await
            .unwrap();
        // The commands of the timer: a warning, a request whose time did
        // not end, and an owner who is gone, with a request and with
        // none.
        as_person(&service, "carol", TakeOwner {}).await.unwrap();
        service.0.warn_owner(Some("ada")).await;
        assert_eq!(service.0.engine.grant_owner().await.unwrap(), None);
        let gone = service.0.engine.end_owner().await.unwrap();
        assert!(matches!(
            gone,
            Some(OwnerChange::Gone { owner: Some(_), .. })
        ));
        assert_eq!(service.members().owner, Some(email("carol")));
        let gone = service.0.engine.end_owner().await.unwrap();
        assert!(matches!(gone, Some(OwnerChange::Gone { owner: None, .. })));
        assert_eq!(service.0.engine.end_owner().await.unwrap(), None);
        service.save().await.unwrap();
        drop(service);
        // A new start drops the sign-ins of the removed person.
        let next = Service::load(signed_config(), store.clone()).await.unwrap();
        next.save().await.unwrap();

        // The records hold the emails.
        let records = format!("{:?}", store.chunks().await);
        assert!(records.contains(EMAIL_MARK));
        // Each kind of line is there, and no line holds an email.
        assert!(capture.results("refused").len() >= 10);
        assert!(capture.results("no_change").len() >= 3);
        for line in capture.results("refused") {
            let line = line.as_object().unwrap();
            assert!(!line.contains_key("reason"), "{line:?}");
        }
        let text = capture.text();
        assert!(!text.contains(EMAIL_MARK), "{text}");
        assert!(!text.contains('@'), "{text}");
    }

    /// Each writer of the token store together with the others writes
    /// it at most one time each window: each sign-in, each refresh and
    /// the save of each second (R127). While the store is busy, the
    /// window doubles (01M3ZZQ9TRG9385GRQGM79RCXX).
    #[tokio::test(start_paused = true)]
    async fn all_writers_of_the_token_store_keep_one_window_between_two_writes() {
        let store = Arc::new(Gated::default());
        let service = Service::load(config(), store.clone()).await.unwrap();
        let every = service.0.config.save_every;
        let mut tasks = Vec::new();
        for n in 0..8 {
            let server = service.0.clone();
            tasks.push(tokio::spawn(async move {
                for _ in 0..40 {
                    if n % 2 == 0 {
                        // A sign-in waits for the write of its change.
                        let mark = server.tokens_changes.load(Ordering::SeqCst);
                        drop(server.tokens_change());
                        let _ = server.save_tokens_since(mark).await;
                    } else if server.tokens_written().await.is_ok() {
                        // A refresh.
                        drop(server.tokens_change());
                    }
                    sleep(Duration::from_millis(100)).await;
                }
            }));
        }
        sleep(2 * every).await;
        let busy = tokio::time::Instant::now();
        *store.token_fail.lock().unwrap() = Some(StoreError::Busy("429".into()));
        sleep(6 * every).await;
        *store.token_fail.lock().unwrap() = None;
        let back = tokio::time::Instant::now();
        for task in tasks {
            task.await.unwrap();
        }
        service.save().await.unwrap();

        let saves = store.token_saves.lock().unwrap().clone();
        assert!(saves.len() >= 6, "{} writes", saves.len());
        for two in saves.windows(2) {
            assert!(two[1] - two[0] >= every, "{:?}", two[1] - two[0]);
        }
        // Without the longer window, the busy time has 6 writes. With
        // it, the writes start one, two and four windows apart.
        let in_busy = saves.iter().filter(|at| busy <= **at && **at < back);
        assert!(in_busy.count() <= 3, "{saves:?}");
    }

    /// A busy store gives no 503 to a refresh while the last good write
    /// is less than one window old, and the refresh writes nothing. After
    /// that, a refresh gets 503 until a write works
    /// (01M3ZZQCEKEYK9CE8MGPM80Z2P).
    #[tokio::test(start_paused = true)]
    async fn a_busy_store_gives_no_503_to_a_refresh_in_the_window() {
        let store = Arc::new(Gated::default());
        let service = Service::load(config(), store.clone()).await.unwrap();
        let server = &service.0;
        let every = server.config.save_every;
        let writes = || store.token_saves.lock().unwrap().len();
        drop(server.tokens_change());
        server.save_tokens_since(0).await.unwrap();

        *store.token_fail.lock().unwrap() = Some(StoreError::Busy("429".into()));
        drop(server.tokens_change());
        assert!(server.save_tokens_since(0).await.is_err());
        let before = writes();
        assert!(server.tokens_written().await.is_ok());
        assert_eq!(writes(), before, "a refresh does not write to a busy store");

        // The window grows to at most `BUSY_WINDOW_MOST` times `every`,
        // so the last good write is now older than the window.
        sleep(every * (2 * BUSY_WINDOW_MOST + 6)).await;
        assert_eq!(server.token_writes().window, every * BUSY_WINDOW_MOST);
        let refused = server.tokens_written().await.unwrap_err();
        assert_eq!(refused.error, UNAVAILABLE);

        *store.token_fail.lock().unwrap() = None;
        server.save_tokens_since(0).await.unwrap();
        assert!(server.tokens_written().await.is_ok());
        assert_eq!(server.token_writes().window, every);
    }
}
