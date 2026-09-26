//! The HTTP client for `riff-server`. The protocol is in
//! [`riff_core::wire`].

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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

/// The server that `riff` uses when nothing else is set.
pub const DEFAULT_SERVER: &str = "http://127.0.0.1:7878";

/// A connection to one `riff-server`. Cheap to clone.
///
/// With [`Api::with_token`], each request carries the access token with
/// the `DPoP` scheme, and a new proof from the device key (R18).
#[derive(Clone)]
pub struct Api {
    http: reqwest::Client,
    base: String,
    auth: Option<Arc<(String, Key)>>,
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
            .http
            .get(format!("{}/v1/sign-in", self.base))
            .send()
            .await
            .with_context(|| format!("cannot reach riff-server at {}", self.base))?;
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
            .http
            .post(&url)
            .header("dpop", key.proof("POST", &url, None, now()))
            .form(request)
            .send()
            .await
            .with_context(|| format!("cannot reach riff-server at {}", self.base))?;
        if response.status().is_success() {
            return Ok(response.json().await?);
        }
        let error = response
            .json::<TokenError>()
            .await
            .map_or_else(|_| "no reason".to_owned(), |e| e.error);
        bail!("riff-server refused the token request: {error}")
    }

    /// Sends `access_token` on each request, with proofs from `key`.
    pub fn with_token(mut self, access_token: &str, key: Key) -> Self {
        self.auth = Some(Arc::new((access_token.to_owned(), key)));
        self
    }

    /// A request to one path, with the token and a proof when there is
    /// a token. The proof names the URL without the query.
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{path}", self.base);
        let request = self.http.request(method.clone(), &url);
        let Some(auth) = &self.auth else {
            return request;
        };
        let (token, key) = auth.as_ref();
        request
            .header("authorization", format!("DPoP {token}"))
            .header("dpop", key.proof(method.as_str(), &url, Some(token), now()))
    }

    /// Says where the session works now. Call it at the start and after
    /// each move.
    pub async fn register(&self, me: &SessionUri) -> Result<()> {
        self.call("register", &Register { me: me.clone() }).await
    }

    pub async fn who(&self) -> Result<Vec<SessionInfo>> {
        let reply: WhoReply = self.call("who", &WhoRequest {}).await?;
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

    /// The wakes for one session. The session is live while the stream
    /// is open.
    pub async fn watch(&self, me: &SessionUri) -> Result<impl Stream<Item = Result<Wake>>> {
        self.events("watch", &[("uri", me.to_string())]).await
    }

    /// Each new message in one thread.
    pub async fn tail(&self, thread: &ThreadName) -> Result<impl Stream<Item = Result<Tailed>>> {
        self.events("tail", &[("thread", thread.to_string())]).await
    }

    /// Ends each sign-in of `user`, or of the caller when `user` is
    /// `None` (R20). It needs [`Api::with_token`].
    pub async fn revoke(&self, user: Option<&str>) -> Result<Revoked> {
        if self.auth.is_none() {
            bail!("not signed in");
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
            .request(reqwest::Method::POST, &format!("/v1/{op}"))
            .json(request)
            .send()
            .await
            .with_context(|| format!("cannot reach riff-server at {}", self.base))?;
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
            .request(reqwest::Method::GET, &format!("/v1/{op}"))
            .query(query)
            .send()
            .await
            .with_context(|| format!("cannot reach riff-server at {}", self.base))?
            .error_for_status()?;
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
