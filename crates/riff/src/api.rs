//! The HTTP client for `riff-server`. The protocol is in
//! [`riff_core::wire`].
//!
//! # Tokens
//!
//! [`Api::signed_in`] gives a client that sends a token on each
//! request, when this device has a sign-in at the server. The token
//! acts as the caller (R104):
//!
//! | Caller | Token | Kept in |
//! |---|---|---|
//! | A person | The person access token, see [`login::access_token`] | The OS keyring |
//! | A session | A session token, see [`login::session_token`] | The memory of the process |
//!
//! A session token comes from a token exchange the first time that the
//! client needs it. After that, the client refreshes it with its own
//! refresh token. When the refresh fails, it does a new exchange.
//!
//! Before its first token, a signed-in client checks the riff ID of its
//! sign-in against the riff ID of the server, once
//! ([`Api::check_riff`], 01M3JNVBRS35B3CD67367JF7SJ). Another ID, or
//! none, means that the riff of the sign-in is gone. The client then
//! removes the sign-in, and the call fails with
//! [`text::new_riff`]. The next command runs with no sign-in.
//!
//! When the client gets no token, it asks the server if it has sign-in
//! ([`Api::has_sign_in`]). A riff with no sign-in cannot give a token,
//! so a sign-in of this machine for it is old. The error then names
//! `riff logout`, never `riff login` (R226, R227). The client keeps the
//! old sign-in: the user of a sign-in is the user of each session, so
//! only the person removes it.
//!
//! # Signatures
//!
//! A signed-in client signs each post with its device key (R195, see
//! [`riff_core::signed`]). The reader checks each message before it
//! shows it: [`Api::read`] and [`checked`] give a [`Checked`] message
//! (R199). Without sign-in, the server keeps no signature, so no
//! message is verified (R201), except on a riff with no sign-in: it
//! trusts its network, and the reader counts each of its messages as
//! verified (R212).
//!
//! # Tries
//!
//! Cloud Run can stop a call or a stream at any time: at a deploy, and
//! after 60 minutes for each stream. So:
//!
//! - While the server replies 503, the client sends the request again,
//!   for up to [`BUSY_LIMIT`] (R132). See [`busy_waits`].
//! - [`follow`] opens a stream again each time it ends (R131). `riff
//!   watch` and `riff tail` use it.

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use futures::{Stream, StreamExt};
use riff_core::build::{self, Build, Mismatch};
use riff_core::dpop::Key;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{
    Alive, Claim, ClaimReply, End, Freed, Invite, Invited, Keys, Kind, Lead, LeadReply, Members,
    MembersReply, Membership, Message, Post, Posted, Read, ReadReply, Register, Remove, Removed,
    Revoke, Revoked, Riff, RiffReply, RiffState, SessionInfo, SetStatus, SignInConfig, Start,
    Started, Status, Tailed, ThreadInfo, Threads, ThreadsReply, TokenError, TokenReply,
    TokenRequest, Wake, WhoReply, WhoRequest,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::Mutex;

use crate::{device, login, secrets, text};

/// The server that `riff` uses when nothing else is set: the server on
/// this machine (R133). `RIFF_SERVER` names another server.
///
/// ```
/// assert_eq!(riff::api::DEFAULT_SERVER, "http://127.0.0.1:7878");
/// ```
pub const DEFAULT_SERVER: &str = "http://127.0.0.1:7878";

/// The word that [`Api::tell`] takes in place of a session: the lead of
/// your user in your repository (R179).
pub const LEAD: &str = "lead";

/// How long the client tries a request again while the server replies
/// 503 (R132).
pub const BUSY_LIMIT: Duration = Duration::from_secs(60);

/// The waits between two tries of a request that got 503 (R132). The
/// first wait is 250 ms. Each next wait is double, up to 5 seconds.
/// Together they last [`BUSY_LIMIT`].
///
/// ```
/// use std::time::Duration;
/// use riff::api::{BUSY_LIMIT, busy_waits};
///
/// let waits: Vec<Duration> = busy_waits().collect();
/// assert_eq!(waits[..3], [250, 500, 1000].map(Duration::from_millis));
/// assert!(waits.iter().all(|w| *w <= Duration::from_secs(5)));
/// assert_eq!(waits.iter().sum::<Duration>(), BUSY_LIMIT);
/// ```
pub fn busy_waits() -> impl Iterator<Item = Duration> {
    let most = Duration::from_secs(5);
    let mut left = BUSY_LIMIT;
    let mut next = Duration::from_millis(250);
    std::iter::from_fn(move || {
        if left.is_zero() {
            return None;
        }
        let wait = next.min(left);
        left -= wait;
        next = (next * 2).min(most);
        Some(wait)
    })
}

/// Follows a stream across connections (R131, R148). `connect` opens the
/// stream. When the stream ends or fails, `follow` connects again at
/// once. When a connect fails, `follow` gives the error as one item and
/// waits `retry` before the next connect. The stream of `follow` never
/// ends.
///
/// ```
/// use std::time::Duration;
/// use futures::StreamExt;
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// // Each connection gives one item and then ends.
/// let mut n = 0;
/// let connect = move || {
///     n += 1;
///     let item: anyhow::Result<u32> = Ok(n);
///     async move { anyhow::Ok(futures::stream::iter([item])) }
/// };
/// let items = riff::api::follow(connect, Duration::from_secs(5)).take(3);
/// let items: Vec<u32> = items.map(Result::unwrap).collect().await;
/// assert_eq!(items, [1, 2, 3]);
/// # }
/// ```
pub fn follow<T, S, F, Fut>(connect: F, retry: Duration) -> impl Stream<Item = Result<T>>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<S>>,
    S: Stream<Item = Result<T>>,
{
    enum Link<S> {
        Down { wait: bool },
        Up(std::pin::Pin<Box<S>>),
    }
    let start = (connect, Link::<S>::Down { wait: false });
    futures::stream::unfold(start, move |(mut connect, mut link)| async move {
        loop {
            link = match link {
                Link::Up(mut stream) => match stream.next().await {
                    Some(Ok(item)) => return Some((Ok(item), (connect, Link::Up(stream)))),
                    Some(Err(_)) | None => Link::Down { wait: false },
                },
                Link::Down { wait } => {
                    if wait {
                        tokio::time::sleep(retry).await;
                    }
                    match connect().await {
                        Ok(stream) => Link::Up(Box::pin(stream)),
                        Err(e) => return Some((Err(e), (connect, Link::Down { wait: true }))),
                    }
                }
            }
        }
    })
}

/// A connection to one `riff-server`. Cheap to clone.
///
/// With [`Api::signed_in`], each request carries an access token with
/// the `DPoP` scheme, and a new proof from the device key (R18).
#[derive(Clone)]
pub struct Api {
    http: reqwest::Client,
    base: String,
    auth: Option<Arc<Auth>>,
}

/// Where the tokens of a signed-in [`Api`] come from.
struct Auth {
    key: Key,
    /// The session of a session client. `None` for a person.
    session: Option<String>,
    /// The session pair, once the client has one.
    pair: Mutex<Option<Pair>>,
    /// Set once [`Api::check_riff`] passed.
    riff_checked: tokio::sync::OnceCell<()>,
}

struct Pair {
    access_token: String,
    refresh_token: String,
    /// Seconds since the Unix epoch.
    expires_at: u64,
}

impl Api {
    pub fn new(base: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: base.trim_end_matches('/').to_owned(),
            auth: None,
        }
    }

    /// The URL of the server.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The sign-in provider of the server.
    pub async fn sign_in_config(&self) -> Result<SignInConfig> {
        let response = self
            .anonymous()
            .send(reqwest::Method::GET, "/v1/sign-in", |r| r)
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            bail!("riff-server at {} has no sign-in provider", self.base);
        }
        Ok(response.error_for_status()?.json().await?)
    }

    /// True when the server has a sign-in provider. A riff with no
    /// sign-in replies 404 to `GET /v1/sign-in` (R226).
    pub async fn has_sign_in(&self) -> Result<bool> {
        let response = self
            .anonymous()
            .send(reqwest::Method::GET, "/v1/sign-in", |r| r)
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        response.error_for_status()?;
        Ok(true)
    }

    /// `error` when the server has sign-in, or when riff cannot ask it.
    /// At a riff with no sign-in, the error names the step that helps
    /// (R226).
    async fn no_token(&self, error: anyhow::Error) -> anyhow::Error {
        if !matches!(self.has_sign_in().await, Ok(false)) {
            return error;
        }
        let kept = login::stored(&self.base).is_ok_and(|s| s.is_some());
        anyhow::anyhow!(text::no_sign_in(&self.base, kept))
    }

    /// Calls the token endpoint with a proof from the device key `key`
    /// (R18). See [`TokenRequest`] for the grants.
    pub async fn token(&self, request: &TokenRequest, key: &Key) -> Result<TokenReply> {
        let url = format!("{}/v1/token", self.base);
        let response = self
            .anonymous()
            .send(reqwest::Method::POST, "/v1/token", |r| {
                r.header("dpop", key.proof("POST", &url, None, now()))
                    .form(request)
            })
            .await?;
        if response.status().is_success() {
            return Ok(response.json().await?);
        }
        let error = response.json::<TokenError>().await.map_or_else(
            |_| "no reason".to_owned(),
            |e| match e.error_description {
                Some(why) => format!("{}: {why}", e.error),
                None => e.error,
            },
        );
        bail!("riff-server refused the token request: {error}")
    }

    /// A client that sends a token on each request, when this device
    /// has a sign-in at the server. `session` is the session ID of the
    /// caller, or `None` for a person. The token acts only as that
    /// caller (R19). Without a sign-in, the client sends no token. A
    /// keyring error is an error (R157). When riff cannot open the
    /// keyring, the client sends no token (R158).
    pub fn signed_in(mut self, session: Option<&str>) -> Result<Self> {
        if !secrets::has_keyring() || login::stored(&self.base)?.is_none() {
            return Ok(self);
        }
        self.auth = Some(Arc::new(Auth {
            key: device::key(&self.base)?,
            session: session.map(str::to_owned),
            pair: Mutex::new(None),
            riff_checked: tokio::sync::OnceCell::new(),
        }));
        Ok(self)
    }

    /// The same server with no token.
    fn anonymous(&self) -> Api {
        Api {
            auth: None,
            ..self.clone()
        }
    }

    /// Removes the sign-in of this device when the server is another
    /// riff than the riff of the sign-in (01M3JNVBRS35B3CD67367JF7SJ).
    /// The error then says to run `riff login`. When riff cannot ask the
    /// server, or the server has no sign-in, it goes on.
    pub async fn check_riff(&self) -> Result<()> {
        let Ok(config) = self.sign_in_config().await else {
            return Ok(());
        };
        let old = login::stored(&self.base)?
            .is_some_and(|s| s.riff_id.as_deref() != Some(config.riff_id.as_str()));
        if old {
            login::logout(&self.base)?;
            bail!(text::new_riff(&self.base));
        }
        Ok(())
    }

    /// A live access token for the caller.
    async fn access_token(&self, auth: &Auth) -> Result<String> {
        auth.riff_checked
            .get_or_try_init(|| self.check_riff())
            .await?;
        let Some(session) = &auth.session else {
            return login::access_token(&self.anonymous()).await;
        };
        let mut pair = auth.pair.lock().await;
        if let Some(live) = pair
            .as_ref()
            .filter(|p| now() + login::REFRESH_MARGIN.as_secs() < p.expires_at)
        {
            return Ok(live.access_token.clone());
        }
        let refreshed = match pair.as_ref() {
            Some(old) => {
                let request = TokenRequest {
                    grant_type: "refresh_token".into(),
                    refresh_token: Some(old.refresh_token.clone()),
                    ..TokenRequest::default()
                };
                self.token(&request, &auth.key).await.ok()
            }
            None => None,
        };
        let reply = match refreshed {
            Some(reply) => reply,
            None => login::session_token(&self.anonymous(), session).await?,
        };
        let access_token = reply.access_token.clone();
        *pair = Some(Pair {
            expires_at: now() + reply.expires_in,
            access_token: reply.access_token,
            refresh_token: reply.refresh_token,
        });
        Ok(access_token)
    }

    /// A request to one path, with a token and a proof when the client
    /// is signed in. The proof names the URL without the query.
    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let url = format!("{}{path}", self.base);
        let request = self
            .http
            .request(method.clone(), &url)
            .header(build::HEADER, build::VERSION);
        let Some(auth) = &self.auth else {
            return Ok(request);
        };
        let token = match self.access_token(auth).await {
            Ok(token) => token,
            Err(error) => return Err(self.no_token(error).await),
        };
        let proof = auth.key.proof(method.as_str(), &url, Some(&token), now());
        Ok(request
            .header("authorization", format!("DPoP {token}"))
            .header("dpop", proof))
    }

    /// Sends a request to one path. `body` adds the rest to the request.
    /// While the server replies 503, it waits and sends a new request,
    /// with a new proof (R132). See [`busy_waits`].
    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        body: impl Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response> {
        let mut waits = busy_waits();
        loop {
            // A box: a request may need a token, and a token is a request.
            let request = Box::pin(self.request(method.clone(), path)).await?;
            let response = body(request)
                .send()
                .await
                .with_context(|| format!("cannot reach riff-server at {}", self.base))?;
            check_build(&response)?;
            match waits.next() {
                Some(wait) if response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE => {
                    tokio::time::sleep(wait).await;
                }
                _ => return Ok(response),
            }
        }
    }

    /// Says where the session works now. Call it at the start and after
    /// each move.
    pub async fn register(&self, me: &SessionUri) -> Result<()> {
        self.call("register", &Register { me: me.clone() }).await
    }

    /// A keep-alive: the session still runs (R204).
    pub async fn alive(&self, me: &SessionUri) -> Result<()> {
        self.call("alive", &Alive { me: me.clone() }).await
    }

    /// The session ended (R205).
    pub async fn end(&self, me: &SessionUri) -> Result<()> {
        self.call("end", &End { me: me.clone() }).await
    }

    /// A new start of the session: a new agent process, a resume or a
    /// `/clear`. Its claims are free at once (01M3JEE1QQCFS5TMZW5N2DAD2D).
    pub async fn start(&self, me: &SessionUri) -> Result<Vec<Freed>> {
        let reply: Started = self.call("start", &Start { me: me.clone() }).await?;
        Ok(reply.freed)
    }

    /// Lists the sessions. `all` lists gone sessions too.
    pub async fn who(&self, me: &SessionUri, all: bool) -> Result<Vec<SessionInfo>> {
        let request = WhoRequest {
            me: me.clone(),
            all,
        };
        let reply: WhoReply = self.call("who", &request).await?;
        Ok(reply.sessions)
    }

    pub async fn threads(&self, me: &SessionUri) -> Result<Vec<ThreadInfo>> {
        let reply: ThreadsReply = self.call("threads", &Threads { me: me.clone() }).await?;
        Ok(reply.threads)
    }

    pub async fn join(&self, me: &SessionUri, thread: &ThreadName) -> Result<()> {
        self.call("join", &membership(me, thread)).await
    }

    pub async fn leave(&self, me: &SessionUri, thread: &ThreadName) -> Result<()> {
        self.call("leave", &membership(me, thread)).await
    }

    /// Posts to a thread and wakes each session that `to` selects. With
    /// no thread, it sends a direct message to one session. A post of
    /// kind [`Kind::Status`] asks each woken session for its status. A
    /// signed-in client signs the post (R195). It first asks the server
    /// whether `me` is the lead, because the signature covers the lead
    /// mark.
    pub async fn post(
        &self,
        me: &SessionUri,
        thread: Option<&ThreadName>,
        to: &[Selector],
        body: &str,
        kind: Kind,
    ) -> Result<Posted> {
        let mut request = Post {
            kind,
            ..Post::new(me, thread.cloned(), to.to_vec(), body)
        };
        if let Some(auth) = &self.auth {
            // The signature covers the lead mark (R196), so ask for it.
            let lead = self
                .who(me, false)
                .await?
                .iter()
                .any(|s| s.uri.who() == me.who() && s.uri.lead());
            request.me = request.me.with_lead(lead);
            request.sign(&auth.key, now_ms());
        }
        self.call("post", &request).await
    }

    /// Sets the status of `me`. It replaces the old status (R182).
    pub async fn status(&self, me: &SessionUri, status: &Status) -> Result<()> {
        status.check().map_err(anyhow::Error::msg)?;
        let request = SetStatus {
            me: me.clone(),
            status: status.clone(),
        };
        self.call("status", &request).await
    }

    /// Sends a direct message (R62). `session` is a session ID, a full
    /// session URI, or [`LEAD`] for the lead of the user of `me` in its
    /// repository (R179).
    pub async fn tell(&self, me: &SessionUri, session: &str, body: &str) -> Result<Posted> {
        let to = if session == LEAD {
            Selector::lead(me.who().user(), &me.place().repo_text())
        } else {
            match session.parse::<SessionUri>() {
                Ok(uri) => match uri.who().session() {
                    Some(id) => Selector::session(id),
                    None => bail!("that URI has no session ID"),
                },
                Err(_) => Selector::session(&self.session_id(me, session).await?),
            }
        };
        self.post(me, None, &[to], body, Kind::Message).await
    }

    /// The full ID of the session whose ID is `id`, or starts with it,
    /// as `read` shows it (01M3JPK885GPD16FPK7D05R2RC). An ID that no
    /// session in `who` has goes as it is: the server then says that
    /// the session is gone. A start of more than one ID is an error.
    async fn session_id(&self, me: &SessionUri, id: &str) -> Result<String> {
        let ids: Vec<String> = self
            .who(me, false)
            .await?
            .into_iter()
            .filter_map(|s| s.uri.who().session().map(str::to_owned))
            .filter(|s| s.starts_with(id))
            .collect();
        if ids.iter().any(|s| s == id) {
            return Ok(id.to_owned());
        }
        match ids.as_slice() {
            [] => Ok(id.to_owned()),
            [one] => Ok(one.clone()),
            _ => bail!(
                "{id} is the start of more than one session ID: {}. Give more of it.",
                ids.join(", ")
            ),
        }
    }

    /// The unread messages (or all of them) of one thread. With no
    /// thread, those of each thread that `me` joined. Leaves out each
    /// thread with no messages to show.
    pub async fn inbox(
        &self,
        me: &SessionUri,
        thread: Option<&ThreadName>,
        all: bool,
    ) -> Result<Vec<Inbox>> {
        let targets = match thread {
            Some(t) => vec![(t.clone(), Vec::new())],
            None => self
                .threads(me)
                .await?
                .into_iter()
                .filter(|t| all || t.unread > 0)
                .map(|t| (t.thread, t.members))
                .collect(),
        };
        let mut out = Vec::new();
        for (thread, members) in targets {
            let messages = self.read(me, &thread, all).await?;
            if !messages.is_empty() {
                out.push(Inbox {
                    thread,
                    members,
                    messages,
                });
            }
        }
        Ok(out)
    }

    /// The unread messages (or all of them) of one thread, each checked
    /// with the keys that the server gives (R199), or with its trusted
    /// mark (R212).
    pub async fn read(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
        all: bool,
    ) -> Result<Vec<Checked>> {
        let request = Read {
            me: me.clone(),
            thread: thread.clone(),
            all,
        };
        let reply: ReadReply = self.call("read", &request).await?;
        Ok(reply
            .messages
            .into_iter()
            .map(|message| checked(thread, message, &reply.keys, reply.trusted))
            .collect())
    }

    pub async fn claim(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
        item: &str,
    ) -> Result<ClaimReply> {
        self.call("claim", &claim(me, thread, item)).await
    }

    pub async fn release(&self, me: &SessionUri, thread: &ThreadName, item: &str) -> Result<()> {
        self.call("release", &claim(me, thread, item)).await
    }

    /// Makes `me` the lead of its user in its repository. It replaces
    /// the old lead (R177).
    pub async fn lead(&self, me: &SessionUri) -> Result<LeadReply> {
        self.call("lead", &Lead { me: me.clone() }).await
    }

    /// The state of the riff (01M3JCFTWCR72HQB8CBTQKXJNF).
    pub async fn riff(&self, me: &SessionUri) -> Result<RiffState> {
        let request = Riff {
            me: me.clone(),
            state: None,
        };
        let reply: RiffReply = self.call("riff", &request).await?;
        Ok(reply.state)
    }

    /// Pauses or resumes the riff. Only a person or a lead can
    /// (01M3JCG3T8AJZN31SZQQTP3FAF). When the state changes, it wakes
    /// each session that is not gone: it posts [`text::riff_news`] to
    /// the thread of each repository of such a session, to that
    /// repository
    /// (01M3JCG3YD7C2Y3V0QJPF082YH).
    pub async fn set_riff(
        &self,
        me: &SessionUri,
        state: RiffState,
    ) -> Result<(RiffReply, Vec<Posted>)> {
        let request = Riff {
            me: me.clone(),
            state: Some(state),
        };
        let reply: RiffReply = self.call("riff", &request).await?;
        let mut posted = Vec::new();
        if !reply.changed {
            return Ok((reply, posted));
        }
        let mut repos: Vec<ThreadName> = self
            .who(me, false)
            .await?
            .into_iter()
            .filter_map(|s| s.uri.default_thread())
            .collect();
        repos.sort();
        repos.dedup();
        let body = text::riff_news(state);
        for repo in repos {
            let to = Selector {
                repo: Some(repo.to_string()),
                ..Selector::default()
            };
            posted.push(
                self.post(me, Some(&repo), &[to], &body, Kind::Message)
                    .await?,
            );
        }
        Ok((reply, posted))
    }

    /// The wakes for one session, on one connection. The session is
    /// live while the stream is open. [`follow`] connects again.
    pub async fn watch(&self, me: &SessionUri) -> Result<impl Stream<Item = Result<Wake>>> {
        self.events("watch", &[("uri", me.to_string())]).await
    }

    /// Each new message in one thread, on one connection, checked like
    /// [`Api::read`]. [`follow`] connects again.
    pub async fn tail(&self, thread: &ThreadName) -> Result<impl Stream<Item = Result<Checked>>> {
        let events = self
            .events::<Tailed>("tail", &[("thread", thread.to_string())])
            .await?;
        Ok(events
            .map(move |tailed| tailed.map(|t| checked(&t.thread, t.message, &t.keys, t.trusted))))
    }

    /// Ends each sign-in of `user`, or of the caller when `user` is
    /// `None` (R20). It needs [`Api::signed_in`].
    pub async fn revoke(&self, user: Option<&str>) -> Result<Revoked> {
        self.need_sign_in().await?;
        let request = Revoke {
            user: user.map(str::to_owned),
        };
        self.call("revoke", &request).await
    }

    /// Adds a member of the riff, by verified email. Only an admin can.
    pub async fn invite(&self, email: &str) -> Result<Invited> {
        self.need_sign_in().await?;
        let request = Invite {
            email: email.to_owned(),
        };
        self.call("invite", &request).await
    }

    /// Removes a member of the riff and ends each sign-in of that person.
    /// Only an admin can.
    pub async fn remove(&self, email: &str) -> Result<Removed> {
        self.need_sign_in().await?;
        let request = Remove {
            email: email.to_owned(),
        };
        self.call("remove", &request).await
    }

    /// Who may join the riff.
    pub async fn members(&self) -> Result<MembersReply> {
        self.need_sign_in().await?;
        self.call("members", &Members {}).await
    }

    /// Fails with what to do when this device has no sign-in.
    async fn need_sign_in(&self) -> Result<()> {
        if self.auth.is_some() {
            return Ok(());
        }
        if matches!(self.has_sign_in().await, Ok(false)) {
            bail!(text::nobody_signs_in(&self.base));
        }
        bail!("no sign-in for {}: run riff login", self.base);
    }

    async fn call<Req: Serialize, Rep: DeserializeOwned>(
        &self,
        op: &str,
        request: &Req,
    ) -> Result<Rep> {
        let response = self
            .send(reqwest::Method::POST, &format!("/v1/{op}"), |r| {
                r.json(request)
            })
            .await?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            bail!("{op} failed ({status}): {text}");
        }
        Ok(response.json().await?)
    }

    /// Reads a server-sent event stream and parses each `data:` line.
    async fn events<T: DeserializeOwned>(
        &self,
        op: &str,
        query: &[(&str, String)],
    ) -> Result<impl Stream<Item = Result<T>> + use<T>> {
        let response = self
            .send(reqwest::Method::GET, &format!("/v1/{op}"), |r| {
                r.query(query)
            })
            .await?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            bail!("{op} failed ({status}): {text}");
        }
        let mut buffer = String::new();
        let lines = response.bytes_stream().flat_map(move |chunk| {
            let lines: Vec<Result<String>> = match chunk {
                Ok(bytes) => {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));
                    let mut lines = Vec::new();
                    while let Some(end) = buffer.find('\n') {
                        let line: String = buffer.drain(..=end).collect();
                        lines.push(Ok(line.trim_end().to_owned()));
                    }
                    lines
                }
                Err(e) => vec![Err(e.into())],
            };
            futures::stream::iter(lines)
        });
        Ok(lines.filter_map(|line| async move {
            match line {
                Ok(line) => line
                    .strip_prefix("data:")
                    .map(|data| serde_json::from_str(data.trim()).map_err(Into::into)),
                Err(e) => Some(Err(e)),
            }
        }))
    }
}

/// Seconds since the Unix epoch, for proofs.
/// Refuses a reply of a `riff-server` whose build does not match this
/// `riff`, or that names no build: an older server
/// (01M3JEE7RDTDD3KQMKH41E8D57). The error is a [`Mismatch`].
fn check_build(response: &reqwest::Response) -> Result<()> {
    let this = Build::this();
    let server = Build::from_header(response.headers().get(build::HEADER).map(|v| v.as_bytes()));
    if server.as_ref().is_some_and(|s| s.matches(&this)) {
        return Ok(());
    }
    Err(Mismatch {
        riff: Some(this),
        server,
    }
    .into())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Milliseconds since the Unix epoch, for signatures.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// The messages to show from one thread.
pub struct Inbox {
    pub thread: ThreadName,
    /// Empty when the caller named the thread.
    pub members: Vec<SessionUri>,
    pub messages: Vec<Checked>,
}

/// A message, and whether the reader proved its sender (R199).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checked {
    pub message: Message,
    pub verified: bool,
}

/// Checks one message of `thread` with the keys that the server gave
/// (see [`Message::verified`]). A message from a riff with no sign-in
/// (`trusted`) is verified (R212).
///
/// ```
/// use riff::api::checked;
/// use riff_core::wire::{Keys, Message};
///
/// let message = Message {
///     seq: 1,
///     from: "riff://mike@pangolin".parse()?,
///     to: vec![],
///     body: "hello".into(),
///     at_ms: 0,
///     kind: Default::default(),
///     sig: None,
/// };
/// let thread = "como-technologies/riff".parse()?;
/// assert!(!checked(&thread, message.clone(), &Keys::new(), false).verified);
/// assert!(checked(&thread, message, &Keys::new(), true).verified);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn checked(thread: &ThreadName, message: Message, keys: &Keys, trusted: bool) -> Checked {
    Checked {
        verified: trusted || message.verified(thread, keys),
        message,
    }
}

fn membership(me: &SessionUri, thread: &ThreadName) -> Membership {
    Membership {
        me: me.clone(),
        thread: thread.clone(),
    }
}

fn claim(me: &SessionUri, thread: &ThreadName, item: &str) -> Claim {
    Claim {
        me: me.clone(),
        thread: thread.clone(),
        item: item.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client of a fake server that replies `status` to
    /// `GET /v1/sign-in`.
    async fn sign_in_replies(status: u16) -> Api {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let code = axum::http::StatusCode::from_u16(status).unwrap();
        let router = axum::Router::new()
            .route(
                "/v1/sign-in",
                axum::routing::get(move || async move { code }),
            )
            .layer(axum::middleware::map_response(
                |mut r: axum::response::Response| async move {
                    let build = axum::http::HeaderValue::from_static(build::VERSION);
                    r.headers_mut().insert(build::HEADER, build);
                    r
                },
            ));
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Api::new(&url)
    }

    #[tokio::test]
    async fn has_sign_in_is_false_only_for_a_404() {
        assert!(!sign_in_replies(404).await.has_sign_in().await.unwrap());
        assert!(sign_in_replies(200).await.has_sign_in().await.unwrap());
        assert!(sign_in_replies(500).await.has_sign_in().await.is_err());
        let gone = Api::new("http://127.0.0.1:1");
        assert!(gone.has_sign_in().await.is_err());
    }

    #[tokio::test]
    async fn no_token_keeps_the_error_unless_the_riff_has_no_sign_in() {
        let first = || anyhow::anyhow!("the sign-in ended: run riff login");
        let signed = sign_in_replies(200).await;
        assert_eq!(
            signed.no_token(first()).await.to_string(),
            first().to_string()
        );
        let down = sign_in_replies(500).await;
        assert_eq!(
            down.no_token(first()).await.to_string(),
            first().to_string()
        );
        let open = sign_in_replies(404).await;
        let error = open.no_token(first()).await.to_string();
        assert!(error.contains("has no sign-in"), "{error}");
        assert!(!error.contains("riff login"), "{error}");
    }

    #[tokio::test]
    async fn follow_gives_a_failed_connect_as_one_error_and_tries_again() {
        let mut n = 0;
        let connect = move || {
            n += 1;
            let reply = match n {
                2 | 3 => Err(anyhow::anyhow!("try {n} failed")),
                _ => Ok(futures::stream::iter([Ok(n)])),
            };
            std::future::ready(reply)
        };
        let items: Vec<String> = follow(connect, Duration::from_millis(1))
            .take(4)
            .map(|item| item.map_or_else(|e| e.to_string(), |n: u32| n.to_string()))
            .collect()
            .await;
        assert_eq!(items, ["1", "try 2 failed", "try 3 failed", "4"]);
    }

    #[tokio::test]
    async fn follow_connects_again_after_an_error_in_the_stream() {
        let mut n = 0;
        let connect = move || {
            n += 1;
            let items: Vec<Result<u32>> = vec![Ok(n), Err(anyhow::anyhow!("reset")), Ok(99)];
            std::future::ready(anyhow::Ok(futures::stream::iter(items)))
        };
        let items: Vec<u32> = follow(connect, Duration::from_secs(60))
            .take(2)
            .map(Result::unwrap)
            .collect()
            .await;
        assert_eq!(items, [1, 2]);
    }

    #[test]
    fn busy_waits_grow_and_stop_at_the_limit() {
        let waits: Vec<Duration> = busy_waits().collect();
        let (last, rest) = waits.split_last().unwrap();
        assert!(rest.windows(2).all(|w| w[0] <= w[1]));
        assert!(*last <= Duration::from_secs(5));
        assert_eq!(waits.iter().sum::<Duration>(), BUSY_LIMIT);
    }
}
