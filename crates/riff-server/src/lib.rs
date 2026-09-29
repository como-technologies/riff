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
//! - [`Service::load`] loads the state from a [`store::Store`] (R30). A
//!   task then saves the changed objects each [`SAVE_EVERY`] (R127).
//!   [`Service::save`] saves them at once; `main` calls it on SIGTERM
//!   (R129). A server from [`Service::new`] has no store and saves
//!   nothing (R34).
//! - The server knows the [`store::Version`] of each object. Each save
//!   names it, so a save over the changes of another instance fails
//!   (R141). A failed save marks its object as changed again.
//! - The token store is the object [`store::TOKENS`]. A call that
//!   changes it gets its reply only after the save (R128). When that
//!   save fails, the reply is 503, and the task saves the tokens again.
//! - A server with a store takes the [`lease`] before it loads, and
//!   keeps reading it. A gate replies 503 to each call while the server
//!   does not serve (R139). The server saves only while it holds the
//!   lease (R155).
//! - A server that reads another ID in the lease, or whose save finds
//!   another version, stops for good (R140, R141): each stream closes,
//!   each call gets 503, and it saves nothing more.
//!   [`Service::stopped`] tells `main`, which exits after
//!   [`lease::Timing::exit_after`].
//! - A server refuses each proof issued before it started to serve
//!   (R142).
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
pub mod gcs;
pub mod idle;
pub mod lease;
pub mod listen;
pub mod oidc;
pub mod owner;
pub mod state;
pub mod store;
pub mod token;

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{Form, Query, Request, State as AxumState};
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
    ACCESS_TOKEN_TYPE, AdminSet, Alive, AliveReply, Claim, ClaimReply, DenyOwner, End,
    ID_TOKEN_TYPE, Idle, Invite, Invited, Keys, Kind, Lead, LeadReply, Members, MembersReply,
    Membership, OwnerAsked, OwnerDenied, OwnerPassed, PassOwner, Person, Post, Posted, Read,
    ReadReply, Register, Remove, Removed, ResourceMetadata, Revoke, Revoked, Riff, RiffOwner,
    RiffReply, ServerMetadata, SetAdmin, SetIdle, SetStatus, SignInConfig, Start, Started,
    TOKEN_EXCHANGE, Tailed, TakeOwner, Threads, ThreadsReply, TokenError, TokenReply, TokenRequest,
    Wake, WhoReply, WhoRequest,
};
use serde::Deserialize;
use tokio::sync::broadcast;
use tokio::time::MissedTickBehavior;
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::{Config, Refusal, Replay, SignedIn};
use crate::lease::Lease;
use crate::owner::{Check, Checks};
use crate::state::{Delivery, State};
use crate::store::{SESSIONS, Store, StoreError, THREADS, TOKENS, Version};
use crate::token::{NO_OWNER, OwnerChange, Tokens, Took};

/// Events that a slow stream may miss before it drops them.
const EVENT_BUFFER: usize = 1024;

/// The server saves each changed object at most this often (R127).
pub const SAVE_EVERY: Duration = Duration::from_secs(1);

type Shared = Arc<Server>;
type Reply<T> = Result<Json<T>, (StatusCode, String)>;

struct Server {
    config: Config,
    state: Mutex<State>,
    tokens: Mutex<Tokens>,
    /// The number of changes to the token store.
    tokens_changes: AtomicU64,
    /// The number of changes to the token store that are saved.
    tokens_saved: AtomicU64,
    replay: Mutex<Replay>,
    wakes: broadcast::Sender<(Who, Wake)>,
    tail: broadcast::Sender<Tailed>,
    http: reqwest::Client,
    saved: Option<Saved>,
    gate: Gate,
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

/// Where a server saves its state.
struct Saved {
    store: Arc<dyn Store>,
    /// The version of each object that the server knows. The lock lets
    /// only one save run at a time.
    versions: tokio::sync::Mutex<HashMap<String, Version>>,
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

    /// Saves the token store now, when a change after the first `mark`
    /// changes is not saved (R128). A server with no store does nothing.
    async fn save_tokens_since(&self, mark: u64) -> Result<(), StoreError> {
        let Some(saved) = &self.saved else {
            return Ok(());
        };
        if !self.tokens_unsaved(mark) {
            return Ok(());
        }
        let mut versions = saved.versions.lock().await;
        // A save that ran while this call waited for the lock can hold
        // the change already.
        if !self.tokens_unsaved(mark) {
            return Ok(());
        }
        self.save_tokens(saved, &mut versions).await
    }

    async fn save_tokens(
        &self,
        saved: &Saved,
        versions: &mut HashMap<String, Version>,
    ) -> Result<(), StoreError> {
        let (bytes, changes) = {
            let tokens = self.tokens();
            let changes = self.tokens_changes.load(Ordering::SeqCst);
            (tokens.to_bytes(Instant::now(), SystemTime::now()), changes)
        };
        if !self.leased() {
            return Err(StoreError::Failed("the server does not serve now".into()));
        }
        let known = versions.get(TOKENS).copied();
        let version = match saved.store.save(TOKENS, bytes, known).await {
            Ok(version) => version,
            Err(error) => {
                if let StoreError::Conflict(_) = error {
                    self.stop(&error.to_string());
                }
                return Err(error);
            }
        };
        versions.insert(TOKENS.into(), version);
        self.tokens_saved.fetch_max(changes, Ordering::SeqCst);
        Ok(())
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

    /// Stops for good (R140).
    fn stop(&self, why: &str) {
        if !self.gate.stopped.send_replace(true) {
            tracing::warn!("stopped for good: {why}");
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

    fn deliver(&self, delivery: Delivery) {
        // A send fails only when nobody listens. That is not an error.
        for wake in delivery.wakes {
            let _ = self.wakes.send(wake);
        }
        let _ = self.tail.send(delivery.tailed);
    }

    /// One look at the owner role at `now` (see [`owner`]): it grants a
    /// request whose time ended, then checks the owner when a check is
    /// due. It warns the owner one check before the owner is gone.
    /// Returns the change that it made.
    fn owner_tick(&self, checks: &mut Checks, now: Instant) -> Option<OwnerChange> {
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
            .is_some_and(|user| self.state().present(user, since, now));
        match checks.record(now, seen, timing) {
            Check::Seen | Check::Missed => None,
            Check::Warn => {
                self.warn_owner(&owner::warn_news(&email, timing), user.as_deref());
                None
            }
            Check::Gone => self.tokens_change().owner_gone(),
        }
    }

    /// Warns the owner that the next check can drop the owner
    /// (01M3Q546335NBTKG5BHQ27QC93): a note to each session of `user` in
    /// the thread of each repository, and one line in the chat.
    fn warn_owner(&self, news: &str, user: Option<&str>) {
        tracing::info!("{news}");
        let (now, at_ms) = (Instant::now(), now_ms());
        let me = owner::server_uri();
        let mut deliveries = Vec::new();
        {
            let mut state = self.state();
            if let Some(user) = user {
                for thread in state.repositories() {
                    let to = vec![Selector {
                        user: Some(user.to_owned()),
                        ..Selector::default()
                    }];
                    deliveries.push(state.announce(
                        &me,
                        Some(thread),
                        to,
                        news,
                        Kind::Note,
                        now,
                        at_ms,
                    ));
                }
            }
            let chat = ThreadName::chat();
            deliveries.push(state.announce(
                &me,
                Some(chat),
                Vec::new(),
                news,
                Kind::Message,
                now,
                at_ms,
            ));
        }
        self.deliver_all(deliveries);
    }

    /// Saves a change of the owner role that no person made, and posts
    /// its note (01M3N7K4DVHSF7AQ402F14J26Z). When the riff has no owner
    /// now, it also asks each admin for a volunteer.
    async fn owner_changed(&self, change: &OwnerChange) {
        if let Err(error) = self.save_tokens_since(0).await {
            tracing::error!("the token store was not saved: {error}");
        }
        let news = owner::change_news(change, &self.config.owner_role);
        tracing::info!("{news}");
        let admins = match change {
            OwnerChange::Gone { owner: None, .. } => self.tokens().admin_users(&self.config.admins),
            _ => Vec::new(),
        };
        self.announce(&news, &admins);
    }

    /// Asks each idle worker past the limit to stop, and posts a note to
    /// the lead of its user for each (see [`idle`]).
    fn stop_idle_workers(&self) {
        let (now, at_ms) = (Instant::now(), now_ms());
        let me = owner::server_uri();
        let mut deliveries = Vec::new();
        {
            let mut state = self.state();
            let settings = state.idle();
            for stopping in state.stop_idle_workers(now) {
                let news = idle::news(&stopping, &settings);
                tracing::info!("{news}");
                let Some(thread) = stopping.worker.default_thread() else {
                    continue;
                };
                let lead = Selector::lead(stopping.worker.who().user(), &thread.to_string());
                let note =
                    state.announce(&me, Some(thread), vec![lead], &news, Kind::Note, now, at_ms);
                deliveries.push(note);
            }
        }
        for delivery in deliveries {
            match delivery {
                Ok(mut delivery) => {
                    delivery.tailed.trusted = self.config.trusted();
                    self.deliver(delivery);
                }
                Err(error) => tracing::warn!("a note of the server did not go: {error}"),
            }
        }
    }

    /// Posts a note of the server to the thread of each repository of
    /// the riff. It sends the same text to each live lead of `users` as a
    /// direct message (see [`owner`]).
    fn announce(&self, news: &str, users: &[String]) {
        let (now, at_ms) = (Instant::now(), now_ms());
        let me = owner::server_uri();
        let mut deliveries = Vec::new();
        {
            let mut state = self.state();
            for thread in state.repositories() {
                let to = vec![Selector {
                    repo: Some(thread.to_string()),
                    ..Selector::default()
                }];
                let note = state.announce(&me, Some(thread), to, news, Kind::Note, now, at_ms);
                deliveries.push(note);
            }
            let direct = owner::to_lead(news);
            for user in users {
                for lead in state.live_leads(user, now) {
                    let Some(id) = lead.session() else {
                        continue;
                    };
                    let to = vec![Selector::session(id)];
                    let message = state.announce(&me, None, to, &direct, Kind::Message, now, at_ms);
                    deliveries.push(message);
                }
            }
        }
        self.deliver_all(deliveries);
    }

    /// Delivers each post of the server. A post that did not go is only
    /// logged.
    fn deliver_all(&self, deliveries: Vec<Result<Delivery, String>>) {
        for delivery in deliveries {
            match delivery {
                Ok(mut delivery) => {
                    delivery.tailed.trusted = self.config.trusted();
                    self.deliver(delivery);
                }
                Err(error) => tracing::warn!("a note of the server did not go: {error}"),
            }
        }
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
    /// A new server with these settings. It saves nothing.
    pub fn new(config: Config) -> Self {
        Service::build(
            config,
            State::default(),
            Tokens::default(),
            oidc::client(oidc::FETCH_TIMEOUT),
            None,
            None,
            now_ms() / 1000,
        )
    }

    /// A server with the state that `store` holds. It takes the lease,
    /// waits, loads the state, and serves from the next whole second
    /// (R138, R142). It saves each change to `store` within
    /// [`SAVE_EVERY`], while the service lives (R30). It fails when
    /// another instance took the lease during the wait.
    ///
    /// ```
    /// # #[tokio::main] async fn main() -> Result<(), riff_server::store::StoreError> {
    /// use std::sync::Arc;
    /// use std::time::Duration;
    /// use riff_server::Service;
    /// use riff_server::auth::Config;
    /// use riff_server::store::Memory;
    ///
    /// let mut config = Config::default();
    /// config.lease.wait = Duration::from_millis(10);
    /// let store = Memory::default();
    /// let old = Service::load(config.clone(), Arc::new(store.clone())).await?;
    /// // A deploy: a new server on the same store.
    /// let new = Service::load(config, Arc::new(store)).await?;
    /// old.stopped().await;
    /// # Ok(()) }
    /// ```
    pub async fn load(config: Config, store: Arc<dyn Store>) -> Result<Self, StoreError> {
        // The build of the HTTP client blocks. Build it now, so that it
        // does not use up the serve time after the lease read (R139).
        let http = oidc::client(oidc::FETCH_TIMEOUT);
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
        let mut versions = HashMap::new();
        let mut threads = Vec::new();
        for name in store.list(THREADS).await? {
            if let Some(loaded) = store.load(&name).await? {
                versions.insert(name.clone(), loaded.version);
                threads.push((name, loaded.bytes));
            }
        }
        let sessions = store.load(SESSIONS).await?;
        if let Some(loaded) = &sessions {
            versions.insert(SESSIONS.into(), loaded.version);
        }
        let tokens = match store.load(TOKENS).await? {
            Some(loaded) => {
                versions.insert(TOKENS.into(), loaded.version);
                Tokens::from_bytes(&loaded.bytes, Instant::now(), SystemTime::now())
                    .map_err(|e| StoreError::not_valid(&*store, TOKENS, e))?
            }
            None => Tokens::default(),
        };
        let state = State::load(
            sessions.as_ref().map(|loaded| loaded.bytes.as_slice()),
            threads
                .iter()
                .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
            Instant::now(),
            now_ms(),
        )
        .map_err(|e| StoreError::not_valid(&*store, &e.name, e.why))?;
        tracing::info!(threads = threads.len(), "loaded the state");
        let asked = Instant::now();
        if !lease.held().await? {
            return Err(StoreError::Conflict(store::LEASE.into()));
        }
        let saved = Saved {
            store,
            versions: tokio::sync::Mutex::new(versions),
        };
        let until = asked + config.lease.valid_for;
        let service = Service::build(config, state, tokens, http, Some(saved), Some(until), start);
        // Save the tokens once, so that a new riff ID stays
        // (01M3JNVBPMZ1K9WX7Q7DP6Y0DH).
        drop(service.0.tokens_change());
        service.keep_lease(lease);
        service.save_each_second();
        Ok(service)
    }

    /// `until` is the end of the first serve time of a server with a
    /// lease. `start` is the second when it starts to serve.
    fn build(
        config: Config,
        state: State,
        tokens: Tokens,
        http: reqwest::Client,
        saved: Option<Saved>,
        until: Option<Instant>,
        start: u64,
    ) -> Self {
        let mut tokens = tokens;
        if let Some(owner) = &config.owner {
            tokens.name_owner(owner);
        }
        let (wakes, _) = broadcast::channel(EVENT_BUFFER);
        let (tail, _) = broadcast::channel(EVENT_BUFFER);
        let mut replay = Replay::default();
        replay.refuse_before(start);
        let service = Service(Arc::new(Server {
            config,
            state: Mutex::new(state),
            tokens: Mutex::new(tokens),
            tokens_changes: AtomicU64::new(0),
            tokens_saved: AtomicU64::new(0),
            replay: Mutex::new(replay),
            wakes,
            tail,
            http,
            saved,
            gate: Gate {
                until: Mutex::new(until),
                stopped: tokio::sync::watch::Sender::new(false),
                closing: AtomicBool::new(false),
            },
        }));
        service.watch_owner();
        service.watch_idle_workers();
        service
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
                if let Some(change) = server.owner_tick(&mut checks, Instant::now()) {
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
                    server.stop_idle_workers();
                }
            }
        });
    }

    /// Saves each changed object now. A server with no store does
    /// nothing. A server that does not hold the lease now saves nothing
    /// (R140, R155). When a save fails, the other objects are still
    /// saved, and the first error is returned. A save that finds another
    /// version stops the server for good (R141).
    pub async fn save(&self) -> Result<(), StoreError> {
        let Some(saved) = &self.0.saved else {
            return Ok(());
        };
        let mut versions = saved.versions.lock().await;
        if !self.0.leased() {
            return Ok(());
        }
        let mut result = if self.0.tokens_unsaved(0) {
            self.0.save_tokens(saved, &mut versions).await
        } else {
            Ok(())
        };
        if self.0.is_stopped() {
            return result;
        }
        let changes = self.0.state().changes(Instant::now(), now_ms());
        for (object, bytes) in changes {
            let name = object.name();
            let known = versions.get(&name).copied();
            match saved.store.save(&name, bytes, known).await {
                Ok(version) => {
                    versions.insert(name, version);
                }
                Err(error @ StoreError::Conflict(_)) => {
                    self.0.stop(&error.to_string());
                    return Err(error);
                }
                Err(error) => {
                    self.0.state().mark_changed(object);
                    result = result.and(Err(error));
                }
            }
        }
        result
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

    /// Starts the task that saves the changes each [`SAVE_EVERY`]. The
    /// task ends when the service ends.
    fn save_each_second(&self) {
        let server = Arc::downgrade(&self.0);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(SAVE_EVERY);
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                let Some(server) = server.upgrade() else {
                    break;
                };
                if server.is_stopped() {
                    break;
                }
                if let Err(error) = Service(server).save().await {
                    tracing::error!("save failed: {error}");
                }
            }
        });
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
            .route("/v1/lead", post(lead))
            .route("/v1/riff", post(riff))
            .route("/v1/status", post(status))
            .route("/v1/alive", post(alive))
            .route("/v1/end", post(end))
            .route("/v1/start", post(start))
            .route("/v1/watch", get(watch))
            .route("/v1/tail", get(tail_thread));
        let guard = || middleware::from_fn_with_state(self.0.clone(), require_token);
        // A riff with sign-in knows who changes the idle workers
        // (01M3Q5A0TF9K49V8Z1ZY9NDF74).
        let trusted = self.0.config.trusted();
        if trusted {
            routes = routes.route("/v1/idle", post(idle_workers));
        }
        if self.0.config.require_sign_in {
            routes = routes.route_layer(guard());
        }
        let mut admin_routes = Router::new();
        if !trusted {
            admin_routes = admin_routes.route("/v1/idle", post(idle_workers));
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

async fn register(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Register>,
) -> Reply<()> {
    let mut state = acts_as(&s, caller, &r.me)?;
    state.register(&r.me, Instant::now());
    state.worker(r.me.who(), r.worker);
    Ok(Json(()))
}

/// A keep-alive: a sign of life that is not a call (R204).
async fn alive(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Alive>,
) -> Reply<AliveReply> {
    let reply = acts_as(&s, caller, &r.me)?.alive(&r.me, Instant::now());
    Ok(Json(reply))
}

/// The session ended: it is gone, and its claims are free (R205).
async fn end(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<End>,
) -> Reply<()> {
    acts_as(&s, caller, &r.me)?.end(&r.me, Instant::now());
    Ok(Json(()))
}

/// A new start of the session: its claims are free at once, and its
/// lead stays (01M3JEE1QQCFS5TMZW5N2DAD2D).
async fn start(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Start>,
) -> Reply<Started> {
    let freed = acts_as(&s, caller, &r.me)?.start(&r.me, Instant::now());
    Ok(Json(Started { freed }))
}

async fn who(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<WhoRequest>,
) -> Reply<WhoReply> {
    let now = Instant::now();
    let (owner, members) = if s.config.trusted() {
        (RiffOwner::NoSignIn, Vec::new())
    } else {
        let tokens = s.tokens();
        (tokens.riff_owner(), tokens.people(&s.config.admins))
    };
    let mut state = acts_as(&s, caller, &r.me)?;
    state.called(&r.me, now);
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
    Ok(Json(WhoReply {
        sessions: state.who(now, now_ms, r.all),
        owner,
        people,
    }))
}

async fn threads(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Threads>,
) -> Reply<ThreadsReply> {
    Ok(Json(ThreadsReply {
        threads: acts_as(&s, caller, &r.me)?.threads(&r.me, Instant::now()),
    }))
}

async fn join(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Membership>,
) -> Reply<()> {
    acts_as(&s, caller, &r.me)?.join(&r.me, &r.thread, Instant::now());
    Ok(Json(()))
}

async fn leave(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Membership>,
) -> Reply<()> {
    acts_as(&s, caller, &r.me)?.leave(&r.me, &r.thread, Instant::now());
    Ok(Json(()))
}

/// Adds a message. With sign-in, the post needs a valid signature from
/// the key of its token (R197), and the message keeps it. Without
/// sign-in, the message keeps no signature (R201).
async fn post_message(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(mut r): Json<Post>,
) -> Reply<Posted> {
    let now_ms = now_ms();
    let at_ms = match &caller {
        Some(Extension(caller)) => caller
            .check_post(&r, now_ms)
            .map_err(|message| (StatusCode::FORBIDDEN, message))?,
        None => {
            r.sig = None;
            now_ms
        }
    };
    let signed = caller.is_some();
    let me = r.me.clone();
    let mut delivery = acts_as(&s, caller, &me)?
        .post(r, Instant::now(), at_ms)
        .map_err(bad_request)?;
    if signed {
        delivery.tailed.keys = s.keys([me.who().user()]);
    }
    delivery.tailed.trusted = s.config.trusted();
    Ok(Json(posted(&s, delivery)))
}

async fn status(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<SetStatus>,
) -> Reply<()> {
    acts_as(&s, caller, &r.me)?
        .set_status(&r.me, r.status, Instant::now(), now_ms())
        .map_err(bad_request)?;
    Ok(Json(()))
}

/// Gives the messages. With sign-in, the reply holds the keys of each
/// sender, so the reader can verify each message (R199). A riff with
/// no sign-in marks the reply as trusted (R211).
async fn read(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Read>,
) -> Reply<ReadReply> {
    let signed = caller.is_some();
    let messages = acts_as(&s, caller, &r.me)?
        .read(&r.me, &r.thread, r.all, Instant::now())
        .map_err(not_found)?;
    let keys = if signed {
        s.keys(messages.iter().map(|m| m.from.who().user()))
    } else {
        Keys::new()
    };
    Ok(Json(ReadReply {
        messages,
        keys,
        trusted: s.config.trusted(),
    }))
}

async fn claim(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Claim>,
) -> Reply<ClaimReply> {
    let reply = acts_as(&s, caller, &r.me)?
        .claim(&r.me, &r.thread, &r.item, Instant::now())
        .map_err(bad_request)?;
    Ok(Json(reply))
}

async fn release(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Claim>,
) -> Reply<()> {
    acts_as(&s, caller, &r.me)?
        .release(&r.me, &r.thread, &r.item, Instant::now())
        .map_err(bad_request)?;
    Ok(Json(()))
}

async fn lead(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Lead>,
) -> Reply<LeadReply> {
    let reply = acts_as(&s, caller, &r.me)?
        .lead(&r.me, Instant::now())
        .map_err(bad_request)?;
    Ok(Json(reply))
}

/// Reads the state of the riff, or sets it for a person or a lead
/// (01M3JCG3T8AJZN31SZQQTP3FAF). A refused set gets 403.
async fn riff(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<Riff>,
) -> Reply<RiffReply> {
    let reply = acts_as(&s, caller, &r.me)?
        .riff(&r.me, r.state, Instant::now())
        .map_err(|message| (StatusCode::FORBIDDEN, message))?;
    Ok(Json(reply))
}

/// Reads the settings of idle workers, and sets each given value
/// (01M3Q5A0TF9K49V8Z1ZY9NDF74). With sign-in, only the owner or an
/// admin can set them.
async fn idle_workers(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Json(r): Json<SetIdle>,
) -> Reply<Idle> {
    let sets = r.per_host.is_some() || r.after_secs.is_some();
    if sets && let Some(Extension(SignedIn { who, .. })) = &caller {
        admin_only(&s, who, "change the settings of idle workers")?;
    }
    if r.after_secs == Some(0) {
        return Err(bad_request("the idle time is at least 1 second".into()));
    }
    let mut state = acts_as(&s, caller, &r.me)?;
    state.called(&r.me, Instant::now());
    Ok(Json(state.set_idle(r.per_host, r.after_secs)))
}

/// The OAuth 2.1 token endpoint: swaps a refresh token for a new pair.
async fn token(
    AxumState(s): AxumState<Shared>,
    headers: HeaderMap,
    Form(r): Form<TokenRequest>,
) -> impl IntoResponse {
    let no_store = || [(header::CACHE_CONTROL, "no-store")];
    let refuse =
        |error: TokenError| (StatusCode::BAD_REQUEST, no_store(), Json(error)).into_response();
    if r.resource
        .as_ref()
        .is_some_and(|resource| !s.config.is_resource(resource))
    {
        return refuse(no("invalid_target"));
    }
    let Ok(proof) = s.proof(&headers, "POST", auth::TOKEN_PATH, None) else {
        return refuse(no("invalid_dpop_proof"));
    };
    let mark = s.tokens_changes.load(Ordering::SeqCst);
    let reply = match r.grant_type.as_str() {
        "refresh_token" => refresh(&s, &r, &proof),
        TOKEN_EXCHANGE => match r.subject_token_type.as_deref() {
            Some(ID_TOKEN_TYPE) => exchange(&s, &r, &proof).await,
            Some(ACCESS_TOKEN_TYPE) => for_session(&s, &r, &proof),
            _ => Err(no("invalid_request")),
        },
        _ => Err(no("unsupported_grant_type")),
    };
    if let Err(error) = s.save_tokens_since(mark).await {
        tracing::error!("the token store was not saved: {error}");
        let error = no("temporarily_unavailable");
        return (StatusCode::SERVICE_UNAVAILABLE, no_store(), Json(error)).into_response();
    }
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
/// server posts the note, and tells each live lead of the owner.
async fn take_owner(
    AxumState(s): AxumState<Shared>,
    Extension(SignedIn { who: caller, .. }): Extension<SignedIn>,
    Json(TakeOwner {}): Json<TakeOwner>,
) -> Reply<OwnerAsked> {
    admin_only(&s, &caller, "take the owner role")?;
    let answer = s.config.owner_role.answer;
    let mark = s.tokens_changes.load(Ordering::SeqCst);
    let took = s
        .tokens_change()
        .take_owner(caller.user(), &s.config.admins, answer, Instant::now())
        .map_err(|why| (StatusCode::CONFLICT, why))?;
    saved(&s, mark).await?;
    let user = caller.user();
    let (reply, news, tell) = match took {
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
    s.announce(&news, tell.as_slice());
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
    s.announce(&news, tell.as_slice());
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

/// Refuses a call from a `riff` of a version that this server cannot
/// talk to, or that names no build (01M3MX1E65XGWDZ062PQ9YXQ5T). A `riff`
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
    };
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
fn refresh(s: &Server, r: &TokenRequest, proof: &dpop::Proof) -> Result<TokenReply, TokenError> {
    let token = r.refresh_token.as_deref().unwrap_or_default();
    if !s.tokens().knows_refresh(token, &proof.jkt) {
        return Err(no("invalid_grant"));
    }
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

/// Swaps a person access token for a session pair (R19).
fn for_session(
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
    s.first_use(proof).map_err(|_| no("invalid_dpop_proof"))?;
    s.tokens_change()
        .for_session(token, &proof.jkt, session, now)
        .map_err(|_| no("invalid_grant"))
}

/// Refuses a request that acts as another user or session than its
/// token (R104), with 403. Without a token check, that part passes.
/// Refuses a known session ID under a new user (R159), with 409. Gives
/// the state, locked, so no other call comes between the check and the
/// change.
fn acts_as<'a>(
    s: &'a Server,
    caller: Option<Extension<SignedIn>>,
    me: &SessionUri,
) -> Result<MutexGuard<'a, State>, (StatusCode, String)> {
    if let Some(Extension(caller)) = caller {
        caller
            .may_act_as(me.who())
            .map_err(|message| (StatusCode::FORBIDDEN, message))?;
    }
    let state = s.state();
    state
        .check_user(me)
        .map_err(|message| (StatusCode::CONFLICT, message))?;
    Ok(state)
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

/// Streams the wakes for one session. The session is live while the
/// stream is open.
async fn watch(
    AxumState(s): AxumState<Shared>,
    caller: Option<Extension<SignedIn>>,
    Query(q): Query<WatchQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let rx = s.wakes.subscribe();
    let missed = {
        let mut state = acts_as(&s, caller, &q.uri)?;
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
        .filter_map(|wake| std::future::ready(Event::default().json_data(wake).ok().map(Ok)))
        .take_until(s.stopping());
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
    let stream = stream.take_until(s.stopping());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Memory;
    use futures::future::BoxFuture;
    use std::time::Duration;
    use tokio::time::sleep;

    /// A memory store that counts the saves of each object.
    #[derive(Default)]
    struct Counting {
        store: Memory,
        saves: Mutex<Vec<String>>,
    }

    impl Counting {
        fn saves(&self, name: &str) -> usize {
            self.saves
                .lock()
                .unwrap()
                .iter()
                .filter(|n| *n == name)
                .count()
        }
    }

    impl Store for Counting {
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
            self.saves.lock().unwrap().push(name.to_owned());
            self.store.save(name, bytes, known)
        }
    }

    #[tokio::test(start_paused = true)]
    async fn each_changed_object_is_saved_at_most_once_each_second() {
        let store = Arc::new(Counting::default());
        let service = Service::load(Config::default(), store.clone())
            .await
            .unwrap();
        let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a"
            .parse()
            .unwrap();
        let thread = me.default_thread().unwrap();
        let name = store::thread_object(&thread);
        let post = |n: usize| {
            for _ in 0..n {
                let now = Instant::now();
                service
                    .0
                    .state()
                    .post(Post::new(&me, Some(thread.clone()), vec![], "x"), now, 0)
                    .unwrap();
            }
        };
        // The first tick comes at once, and nothing changed.
        sleep(Duration::from_millis(10)).await;
        post(5);
        sleep(Duration::from_millis(500)).await;
        assert_eq!(store.saves(&name), 0);
        sleep(Duration::from_millis(600)).await;
        assert_eq!(store.saves(&name), 1);
        assert_eq!(store.saves(SESSIONS), 1);
        post(5);
        sleep(Duration::from_millis(100)).await;
        assert_eq!(store.saves(&name), 1);
        sleep(Duration::from_secs(1)).await;
        assert_eq!(store.saves(&name), 2);
        // Nothing changed, so nothing is saved.
        sleep(Duration::from_secs(3)).await;
        assert_eq!(store.saves(&name), 2);

        let loaded = Service::load(Config::default(), Arc::new(store.store.clone()))
            .await
            .unwrap();
        let now = Instant::now();
        let messages = loaded.0.state().read(&me, &thread, true, now).unwrap();
        assert_eq!(messages.len(), 10);
    }

    #[tokio::test(start_paused = true)]
    async fn the_save_task_ends_with_the_service() {
        let store = Arc::new(Counting::default());
        let service = Service::load(Config::default(), store.clone())
            .await
            .unwrap();
        let server = Arc::downgrade(&service.0);
        drop(service);
        sleep(Duration::from_secs(2)).await;
        assert!(server.upgrade().is_none());
    }

    #[tokio::test]
    async fn a_server_with_no_store_saves_nothing() {
        let service = Service::default();
        let me: SessionUri = "riff://mike@pangolin/-?session=a#x".parse().unwrap();
        service.0.state().register(&me, Instant::now());
        assert!(service.save().await.is_ok());
    }
}
