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
use riff_core::dpop::Key;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{
    Claim, ClaimReply, Membership, Message, Post, Posted, Read, ReadReply, Register, Revoke,
    Revoked, SessionInfo, SignInConfig, Tailed, ThreadInfo, Threads, ThreadsReply, TokenError,
    TokenReply, TokenRequest, Wake, WhoReply, WhoRequest,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::Mutex;

use crate::{device, login, secrets};

/// The server that `riff` uses when nothing else is set: the shared
/// server on Cloud Run (R5, R133). Local development sets `RIFF_SERVER`.
///
/// ```
/// assert!(riff::api::DEFAULT_SERVER.starts_with("https://"));
/// ```
pub const DEFAULT_SERVER: &str = "https://riff-server-816917641970.us-central1.run.app";

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
        let error = response
            .json::<TokenError>()
            .await
            .map_or_else(|_| "no reason".to_owned(), |e| e.error);
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

    /// A live access token for the caller.
    async fn access_token(&self, auth: &Auth) -> Result<String> {
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
        let request = self.http.request(method.clone(), &url);
        let Some(auth) = &self.auth else {
            return Ok(request);
        };
        let token = self.access_token(auth).await?;
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
    /// no thread, it sends a direct message to one session.
    pub async fn post(
        &self,
        me: &SessionUri,
        thread: Option<&ThreadName>,
        to: &[Selector],
        body: &str,
    ) -> Result<Posted> {
        let request = Post {
            me: me.clone(),
            thread: thread.cloned(),
            to: to.to_vec(),
            body: body.to_owned(),
        };
        self.call("post", &request).await
    }

    /// Sends a direct message (R62). `session` is a session ID or a full
    /// session URI.
    pub async fn tell(&self, me: &SessionUri, session: &str, body: &str) -> Result<Posted> {
        let id = match session.parse::<SessionUri>() {
            Ok(uri) => match uri.who().session() {
                Some(id) => id.to_owned(),
                None => bail!("that URI has no session ID"),
            },
            Err(_) => session.to_owned(),
        };
        self.post(me, None, &[Selector::session(&id)], body).await
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

    pub async fn read(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
        all: bool,
    ) -> Result<Vec<Message>> {
        let request = Read {
            me: me.clone(),
            thread: thread.clone(),
            all,
        };
        let reply: ReadReply = self.call("read", &request).await?;
        Ok(reply.messages)
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

    /// The wakes for one session, on one connection. The session is
    /// live while the stream is open. [`follow`] connects again.
    pub async fn watch(&self, me: &SessionUri) -> Result<impl Stream<Item = Result<Wake>>> {
        self.events("watch", &[("uri", me.to_string())]).await
    }

    /// Each new message in one thread, on one connection. [`follow`]
    /// connects again.
    pub async fn tail(&self, thread: &ThreadName) -> Result<impl Stream<Item = Result<Tailed>>> {
        self.events("tail", &[("thread", thread.to_string())]).await
    }

    /// Ends each sign-in of `user`, or of the caller when `user` is
    /// `None` (R20). It needs [`Api::signed_in`].
    pub async fn revoke(&self, user: Option<&str>) -> Result<Revoked> {
        if self.auth.is_none() {
            bail!("no sign-in for {}: run riff login", self.base);
        }
        let request = Revoke {
            user: user.map(str::to_owned),
        };
        self.call("revoke", &request).await
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
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The messages to show from one thread.
pub struct Inbox {
    pub thread: ThreadName,
    /// Empty when the caller named the thread.
    pub members: Vec<SessionUri>,
    pub messages: Vec<Message>,
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
