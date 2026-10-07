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
//! client needs it. It has no refresh token
//! (01M3WFVAB44T8EP4QZD4KS7DRF). Before it expires, the client does a
//! new exchange with the person token
//! (01M3WFVADCDZM8XX590KAEMEYG). Each process of a session holds a
//! token of its own, and a new token ends no other token. See
//! [`login`] for the person pair.
//!
//! ```mermaid
//! flowchart TD
//!     C[a call of a session] --> L{session token live for 60 s more?}
//!     L -- yes --> U[use it]
//!     L -- no --> P[the person access token, see login::access_token]
//!     P --> X[POST /v1/token: swap for a session token]
//!     X -- token --> K[keep it in memory] --> U
//!     X -- invalid_grant --> R[refresh the person pair once, swap again]
//! ```
//!
//! When the server replies 401 to a call with a token, the client drops
//! that token, gets a new one, and sends the call once more
//! (01M3MX4VCEBTY0DN4JMF624WYE). So a session goes on when the server
//! lost its token, and after `riff login` on the machine
//! (01M3MX4VM8CK1GAGJAM2P29NWH).
//!
//! The token and sign-in calls take a server of each version: `riff
//! login` and a refresh work also when the versions do not match
//! (01M3MX4V43SF2XFCZWANHD19WV).
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
//! # A session that left the riff
//!
//! Each call of a gone session brings it back (R207). So a session that
//! left the riff makes no request (01M3MEEFETT9A0DRWBKQTG77Z2). The
//! leave is a mark on this machine: the file `left-ID` in
//! [`local::marks`]. The client of a session reads the mark before each
//! request, in the one function that sends each request
//! (`Api::send_with`, 01M3XQVJXWBC3DKAVWBPXPSGZS). A request of a
//! session with the mark fails with [`Left`], and nothing goes to the
//! server. So the status line, each hook, the watch, each tool and each
//! task of `riff mcp` have the same check, and a new call site has it
//! too.
//!
//! ```mermaid
//! flowchart TD
//!     T[a tool, a hook, the watch, the status line, a task of riff mcp] --> S[Api::send_with]
//!     S --> M{the mark left-ID of the session?}
//!     M -- yes --> L[the error Left: no request]
//!     M -- no --> R[the request to riff-server]
//!     V[Api::leave_riff] -- "writes the mark, then the end call" --> R
//!     J[Api::join_riff] -- "removes the mark, then the register" --> S
//! ```
//!
//! [`Api::signed_in`] gives the client of a session its mark.
//! [`Api::leave_riff`] writes the mark first, and then sends the end
//! call: the one request that goes out with the mark
//! (01M3XQVK05FAT3PR43W8RNEYHY). [`Api::join_riff`] removes the mark,
//! and registers.
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
//! # The link
//!
//! Each request goes through the link of its server ([`crate::link`]):
//! one shared HTTP client, the budget of each call, the rule for a new
//! try, and the call ID of each call. [`follow`] opens a stream again
//! each time it ends (R131). `riff watch` and `riff tail` use it.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use futures::{Stream, StreamExt};
use riff_core::build::{self, Build, Mismatch};
use riff_core::dpop::Key;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{
    Activity, AdminSet, Alive, AliveReply, BlockedLook, CALL_HEADER, Call, Claim, ClaimReply,
    DenyOwner, End, Free, FreeReply, Freed, Hold, HoldReply, Idle, IdleQuery, Invite, Invited,
    ItemFact, ItemFacts, Join, Keys, Kind, Lead, LeadReply, Leave, LogQuery, LogReply, MeReply,
    Members, MembersReply, Message, OwnerAsked, OwnerDenied, OwnerPassed, PassOwner, Pause, Post,
    Posted, REFUSED_HEADER, Read, Register, Release, ReleaseFor, ReleaseReply, Remove, Removed,
    Resume, Revoke, Revoked, RiffQuery, RiffReply, RiffState, ServerFacts, SessionInfo, SetAdmin,
    SetBlocked, SetIdle, SetStatus, SetStep, SignInConfig, Start, StartReason, Status, StepChange,
    Tailed, TakeOwner, ThreadInfo, Threads, TokenError, TokenReply, TokenRequest, Unanswered, Wake,
    WhoReply, WhoRequest,
};
use serde::de::DeserializeOwned;
use tokio::sync::Mutex;

use crate::link::{self, Budget, Limits, Link, Outcome, Reply, WaitLine};
use crate::{device, local, login, secrets, text};

/// A change of the members that `riff-server` made, and the posts of
/// its note (01M3MN14ZCTRVD3T455P6TFK1B). The change stands also when
/// the post fails (01M3MN1537Z0K3BRK6H2BZKZT0).
#[derive(Debug)]
pub struct Changed<T> {
    /// The reply of `riff-server` to the change.
    pub done: T,
    /// The note in the thread of each repository, or the error of the
    /// post.
    pub news: Result<Vec<Posted>>,
}

/// The server that `riff` uses when nothing else is set: the server on
/// this machine (R133). `RIFF_SERVER` names another server.
///
/// ```
/// assert_eq!(riff::api::DEFAULT_SERVER, "http://127.0.0.1:7878");
/// ```
pub const DEFAULT_SERVER: &str = "http://127.0.0.1:7878";

/// The port of a server that `--server` or `RIFF_SERVER` names with no
/// port (01M3K0Q80BCZQD7DNQQ333ZN09).
pub const DEFAULT_PORT: u16 = 7878;

/// The URL of the server that `--server` or `RIFF_SERVER` names: a URL,
/// `HOST` or `HOST:PORT` (01M3K0Q80BCZQD7DNQQ333ZN09). With no scheme,
/// it adds `http://`, and [`DEFAULT_PORT`] when there is no port. A bare
/// IPv6 address gets brackets. A URL stays as it is, with no `/` at the
/// end.
///
/// ```
/// use riff::api::server_url;
///
/// assert_eq!(server_url("first").unwrap(), "http://first:7878");
/// assert_eq!(server_url("first:9000").unwrap(), "http://first:9000");
/// assert_eq!(server_url("[::1]").unwrap(), "http://[::1]:7878");
/// assert_eq!(server_url("[::1]:9000").unwrap(), "http://[::1]:9000");
/// assert_eq!(server_url("::1").unwrap(), "http://[::1]:7878");
/// assert_eq!(server_url("fe80::2").unwrap(), "http://[fe80::2]:7878");
/// assert_eq!(server_url("https://riff.example.com/").unwrap(), "https://riff.example.com");
/// assert_eq!(server_url(riff::api::DEFAULT_SERVER).unwrap(), riff::api::DEFAULT_SERVER);
/// assert!(server_url("").is_err());
/// assert!(server_url("first:port").is_err());
/// ```
pub fn server_url(value: &str) -> Result<String, String> {
    let value = value.trim().trim_end_matches('/');
    if value.contains("://") {
        return Ok(value.to_owned());
    }
    if value.parse::<std::net::Ipv6Addr>().is_ok() {
        return Ok(format!("http://[{value}]:{DEFAULT_PORT}"));
    }
    let (host, port) = match value.rsplit_once(':') {
        Some((host, port)) if !port.ends_with(']') => (host, Some(port)),
        _ => (value, None),
    };
    if host.is_empty() {
        return Err("name a server: a URL, HOST or HOST:PORT".into());
    }
    match port {
        None => Ok(format!("http://{host}:{DEFAULT_PORT}")),
        Some(port) if port.parse::<u16>().is_ok() => Ok(format!("http://{host}:{port}")),
        Some(port) => Err(format!("{port} is not a port")),
    }
}

/// The word that [`Api::tell`] takes in place of a session: the lead of
/// your user in your repository (R179).
pub const LEAD: &str = "lead";

/// Who got the message of a block or of a failed step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Told {
    /// The lead of the user.
    Lead,
    /// Nobody: the user has no lead.
    Nobody,
    /// The session is the lead: its own person reads its terminal.
    You,
}

/// Follows a stream across connections (R131, R148). `connect` opens the
/// stream. When the stream ends or fails, `follow` connects again at
/// once. When that connect fails, `follow` tries once more at once and
/// gives no item for it: a cut can leave a dead connection in the pool
/// (01M3Q59CAA46C316BD4D1ED7C6). When a connect fails again, `follow`
/// gives the error as one item and waits `retry` before the next
/// connect. The stream of `follow` never ends.
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
        /// Not connected. After a failed connect, `follow` waits. The
        /// first connect after a stream ends is `fresh`: when it fails,
        /// `follow` tries once more at once, with no item.
        Down {
            wait: bool,
            fresh: bool,
        },
        Up(std::pin::Pin<Box<S>>),
    }
    let ended = Link::<S>::Down {
        wait: false,
        fresh: true,
    };
    futures::stream::unfold(
        (connect, ended),
        move |(mut connect, mut link)| async move {
            loop {
                link = match link {
                    Link::Up(mut stream) => match stream.next().await {
                        Some(Ok(item)) => return Some((Ok(item), (connect, Link::Up(stream)))),
                        Some(Err(_)) | None => Link::Down {
                            wait: false,
                            fresh: true,
                        },
                    },
                    Link::Down { wait, fresh } => {
                        if wait {
                            tokio::time::sleep(retry).await;
                        }
                        match connect().await {
                            Ok(stream) => Link::Up(Box::pin(stream)),
                            Err(_) if fresh => Link::Down {
                                wait: false,
                                fresh: false,
                            },
                            Err(e) => {
                                let down = Link::Down {
                                    wait: true,
                                    fresh: false,
                                };
                                return Some((Err(e), (connect, down)));
                            }
                        }
                    }
                }
            }
        },
    )
}

/// The line of [`Reconnect`] while a connect fails.
pub const RECONNECTING: &str = "(reconnecting…)";

/// The line of [`Reconnect`] when a stream is back after a failed connect.
pub const BACK: &str = "(back)";

/// What a person sees of a stream that [`follow`] follows
/// (01M3NK7VHXB0PAR8VH8GQQA06K). A server that ends a long poll is
/// normal, so a reconnect shows nothing. Only a failed connect shows one
/// short dim line, [`RECONNECTING`], and the next item [`BACK`]. A server
/// that riff cannot talk to ([`Mismatch`]) shows its error in red.
///
/// ```
/// use riff::api::{BACK, RECONNECTING, Reconnect};
///
/// let plain = |line: Option<String>| line.map(|l| anstream::adapter::strip_str(&l).to_string());
/// let mut link = Reconnect::default();
/// assert_eq!(link.line(&anyhow::Ok(1)), None);
/// let cut: anyhow::Result<u32> = Err(anyhow::anyhow!("error decoding response body"));
/// assert_eq!(plain(link.line(&cut)).as_deref(), Some(RECONNECTING));
/// assert_eq!(link.line(&cut), None);
/// assert_eq!(plain(link.line(&anyhow::Ok(2))).as_deref(), Some(BACK));
/// assert_eq!(link.line(&anyhow::Ok(3)), None);
/// ```
#[derive(Debug, Default)]
pub struct Reconnect {
    lost: bool,
}

impl Reconnect {
    /// The line to show before `item`, with its style, if any.
    pub fn line<T>(&mut self, item: &Result<T>) -> Option<String> {
        use crate::style::{DIM, ERROR, styled};
        match (item, self.lost) {
            (Ok(_), true) => {
                self.lost = false;
                Some(styled(DIM, BACK))
            }
            (Ok(_), false) | (Err(_), true) => None,
            (Err(e), false) => {
                self.lost = true;
                Some(if e.downcast_ref::<Mismatch>().is_some() {
                    styled(ERROR, &format!("riff: {e:#}"))
                } else {
                    styled(DIM, RECONNECTING)
                })
            }
        }
    }
}

/// What a server that answers tells about itself: see [`Api::probe`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    /// The build of the server. `None` for an old server that names no
    /// build.
    pub build: Option<Build>,
    /// True when the riff has sign-in, false when it trusts its network.
    /// `None` when the server did not say, for example to another build.
    pub sign_in: Option<bool>,
}

/// What a pause or a resume names (01M3XAHZBGSSJB3YX23K88W01K).
///
/// ```
/// use riff::api::PauseScope;
///
/// let me = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
/// let riff = "como-technologies/riff".parse()?;
/// assert_eq!(PauseScope::Here.repository(&me), Some(riff));
/// assert_eq!(PauseScope::Riff.repository(&me), None);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PauseScope {
    /// The repository of the caller.
    Here,
    /// A repository by its name. Only the owner or an admin can.
    Repository(ThreadName),
    /// The whole riff. Only the owner or an admin can.
    Riff,
}

impl PauseScope {
    /// The scope of `riff pause` and `riff resume` with the flags
    /// `--riff` and `--repo`.
    pub fn of(riff: bool, repo: Option<ThreadName>) -> PauseScope {
        match (riff, repo) {
            (true, _) => PauseScope::Riff,
            (false, Some(repo)) => PauseScope::Repository(repo),
            (false, None) => PauseScope::Here,
        }
    }

    /// The repository of the scope, for the caller `me`. `None` for the
    /// whole riff, and for a caller outside a repository.
    pub fn repository(&self, me: &SessionUri) -> Option<ThreadName> {
        match self {
            PauseScope::Here => me.default_thread(),
            PauseScope::Repository(thread) => Some(thread.clone()),
            PauseScope::Riff => None,
        }
    }
}

/// A connection to one `riff-server`. Cheap to clone.
///
/// With [`Api::signed_in`], each request carries an access token with
/// the `DPoP` scheme, and a new proof from the device key (R18).
#[derive(Clone)]
pub struct Api {
    /// The link of the server: its clients and what the process knows
    /// of it ([`crate::link`]).
    link: Arc<Link>,
    /// The budget of each call of this client
    /// (01M4A803Z4Q0KX6NT1KC6QR43H).
    budget: Budget,
    auth: Option<Arc<Auth>>,
    /// Where the client shows [`crate::link::WAITING`]. `None` is stderr.
    waits: Option<WaitLine>,
    /// The mark of the leave of the session of this client. `None` for
    /// a person, and for a client that no session uses.
    mark: Option<Arc<Mark>>,
}

/// Where the mark of the leave of one session is.
struct Mark {
    /// A directory as [`local::marks`].
    dir: PathBuf,
    session: String,
}

/// Where the tokens of a signed-in [`Api`] come from.
struct Auth {
    key: Key,
    /// The session of a session client. `None` for a person.
    session: Option<String>,
    /// The session token, once the client has one.
    held: Mutex<Option<Held>>,
    /// Set once [`Api::check_riff`] passed.
    riff_checked: tokio::sync::OnceCell<()>,
}

/// A session access token. It has no refresh token
/// (01M3WFVAB44T8EP4QZD4KS7DRF).
struct Held {
    access_token: String,
    /// Seconds since the Unix epoch.
    expires_at: u64,
}

/// Where a request goes: a call, which gets the whole reply in the
/// limit of each try, or the open of a stream, which has no limit after
/// its head.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Via {
    Call,
    Stream,
}

/// What a request got.
enum Got {
    /// The whole reply of a call.
    Whole(Reply),
    /// The head of a stream. Its body is the stream.
    Open(reqwest::Response),
}

/// The end of one try (01M4A8041F8EK1VYDE4C9QG8N8).
enum Step {
    /// A reply or a refusal of `riff-server`.
    Done(Got),
    /// A new token: a new try at once.
    Again,
    /// A fault: a new try after a wait, while the budget lasts. It has
    /// what the call gives when the budget ends.
    Fault(Result<Got>),
    /// An error that a new try cannot repair.
    End(anyhow::Error),
}

impl Api {
    /// A client of the server at `base`, on the one link of that server
    /// in the process ([`crate::link::of`]).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use riff::api::Api;
    ///
    /// let one = Api::new("http://127.0.0.1:7878");
    /// let two = Api::new("http://127.0.0.1:7878");
    /// assert!(Arc::ptr_eq(one.link(), two.link()));
    /// ```
    pub fn new(base: &str) -> Self {
        Self {
            link: link::of(base),
            budget: Budget::Short,
            auth: None,
            waits: None,
            mark: None,
        }
    }

    /// The link of the server of this client.
    pub fn link(&self) -> &Arc<Link> {
        &self.link
    }

    /// The same caller with the time limits `limits`, on the link of
    /// those limits. A test gives limits in milliseconds.
    ///
    /// ```
    /// use std::time::Duration;
    /// use riff::api::Api;
    /// use riff::link::Limits;
    ///
    /// let fast = Limits { try_wait: Duration::from_millis(200), ..Limits::default() };
    /// let api = Api::new("http://127.0.0.1:7878").with_limits(fast);
    /// assert_eq!(api.limits(), fast);
    /// ```
    pub fn with_limits(&self, limits: Limits) -> Api {
        Api {
            link: link::with(self.base(), limits),
            ..self.clone()
        }
    }

    /// The time limits of this client.
    pub fn limits(&self) -> Limits {
        self.link.limits()
    }

    /// The same caller, with the budget `time` for each call
    /// (01M4A803Z4Q0KX6NT1KC6QR43H).
    ///
    /// ```
    /// use std::time::Duration;
    /// use riff::{api::Api, link::Budget};
    ///
    /// let api = Api::new("http://127.0.0.1:7878");
    /// assert_eq!(api.budget(), Budget::Short);
    /// let three = Duration::from_secs(3);
    /// assert_eq!(api.with_budget(three).budget(), Budget::Within(three));
    /// assert_eq!(api.one_try(three).budget(), Budget::OneTry(three));
    /// ```
    pub fn with_budget(&self, time: Duration) -> Api {
        Api {
            budget: Budget::Within(time),
            ..self.clone()
        }
    }

    /// The same caller, with one try in `time` for each call: the status
    /// line.
    pub fn one_try(&self, time: Duration) -> Api {
        Api {
            budget: Budget::OneTry(time),
            ..self.clone()
        }
    }

    /// The budget of each call of this client.
    pub fn budget(&self) -> Budget {
        self.budget
    }

    /// The same client for the session `session`, with the mark of its
    /// leave in `dir`. See "A session that left the riff" in the module
    /// doc. [`Api::signed_in`] calls it with [`local::marks`].
    ///
    /// ```
    /// # #[tokio::main(flavor = "current_thread")] async fn main() -> anyhow::Result<()> {
    /// use riff::api::{Api, Left};
    ///
    /// let marks = tempfile::tempdir()?;
    /// let api = Api::new("http://127.0.0.1:1").for_session(marks.path(), "a6cf");
    /// assert!(!api.left());
    /// riff::local::leave(marks.path(), "a6cf")?;
    /// assert!(api.left());
    ///
    /// // No request goes out: the error is the leave, not the connect.
    /// let me = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let error = api.alive(&me).await.unwrap_err();
    /// assert!(error.is::<Left>(), "{error:#}");
    /// assert!(error.to_string().contains("/riff:join"));
    /// # Ok(()) }
    /// ```
    pub fn for_session(mut self, dir: &Path, session: &str) -> Self {
        self.mark = Some(Arc::new(Mark {
            dir: dir.to_owned(),
            session: session.to_owned(),
        }));
        self
    }

    /// True when the session of this client left the riff: its mark is
    /// there. False for a client with no session.
    pub fn left(&self) -> bool {
        self.mark
            .as_ref()
            .is_some_and(|mark| local::left(&mark.dir, &mark.session))
    }

    /// The same client with no mark: only for the end call of a leave.
    fn unmarked(&self) -> Api {
        Api {
            mark: None,
            ..self.clone()
        }
    }

    /// The same client, which gives the line [`WAITING`] to `show`, not
    /// to stderr. `riff chat` uses it to keep its screen
    /// (01M3THEE5V3RFHF9QTA8MA8QDF).
    ///
    /// ```
    /// let api = riff::api::Api::new("http://127.0.0.1:7878").waits_to(|line| println!("{line}"));
    /// assert_eq!(api.base(), "http://127.0.0.1:7878");
    /// ```
    pub fn waits_to(mut self, show: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.waits = Some(Arc::new(show));
        self
    }

    /// The URL of the server.
    pub fn base(&self) -> &str {
        self.link.base()
    }

    /// The sign-in provider of the server.
    pub async fn sign_in_config(&self) -> Result<SignInConfig> {
        let response = self
            .anonymous()
            .send_with(reqwest::Method::GET, "/v1/sign-in", |r| r, Check::None)
            .await?;
        if response.status == reqwest::StatusCode::NOT_FOUND {
            bail!("riff-server at {} has no sign-in provider", self.base());
        }
        response.error_for_status()?.json()
    }

    /// True when the server has a sign-in provider. A riff with no
    /// sign-in replies 404 to `GET /v1/sign-in` (R226).
    pub async fn has_sign_in(&self) -> Result<bool> {
        let response = self
            .anonymous()
            .send_with(reqwest::Method::GET, "/v1/sign-in", |r| r, Check::None)
            .await?;
        if response.status == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        response.error_for_status()?;
        Ok(true)
    }

    /// What the server tells about itself, with no token and no check
    /// of its build, within `wait`: for `riff server` and `riff update`
    /// (01M3Q5VE74608N5H2M73RB6Y2Z). An error when it does not answer.
    pub async fn probe(&self, wait: Duration) -> Result<Probe> {
        let base = self.base();
        let response = self
            .link
            .client()
            .get(format!("{base}/v1/sign-in"))
            .header(build::HEADER, build::VERSION)
            .timeout(wait)
            .send()
            .await
            .with_context(|| format!("cannot reach riff-server at {base}"))?;
        let build = Build::from_header(response.headers().get(build::HEADER).map(|v| v.as_bytes()));
        let sign_in = match response.status() {
            reqwest::StatusCode::NOT_FOUND => Some(false),
            s if s.is_success() => Some(true),
            _ => None,
        };
        Ok(Probe { build, sign_in })
    }

    /// `error` when the server has sign-in, or when riff cannot ask it.
    /// At a riff with no sign-in, the error names the step that helps
    /// (R226).
    async fn no_token(&self, error: anyhow::Error) -> anyhow::Error {
        if !matches!(self.has_sign_in().await, Ok(false)) {
            return error;
        }
        let kept = login::stored(self.base()).is_ok_and(|s| s.is_some());
        anyhow::anyhow!(text::no_sign_in(self.base(), kept))
    }

    /// Calls the token endpoint with a proof from the device key `key`
    /// (R18). See [`TokenRequest`] for the grants. A refusal of the
    /// server is a [`TokenRefused`].
    pub async fn token(&self, request: &TokenRequest, key: &Key) -> Result<TokenReply> {
        let url = format!("{}/v1/token", self.base());
        let response = self
            .anonymous()
            .send_with(
                reqwest::Method::POST,
                "/v1/token",
                |r| {
                    r.header("dpop", key.proof("POST", &url, None, now()))
                        .form(request)
                },
                Check::None,
            )
            .await?;
        if response.status.is_success() {
            return response.json();
        }
        let refused = response.json::<TokenError>().map_or_else(
            |_| TokenRefused {
                error: "no reason".into(),
                description: None,
            },
            |e| TokenRefused {
                error: e.error,
                description: e.error_description,
            },
        );
        Err(refused.into())
    }

    /// A client that sends a token on each request, when this device
    /// has a sign-in at the server. `session` is the session ID of the
    /// caller, or `None` for a person. The token acts only as that
    /// caller (R19). Without a sign-in, the client sends no token. A
    /// keyring error is an error (R157). When riff cannot open the
    /// keyring, the client sends no token (R158).
    ///
    /// The client of a session also gets the mark of its leave, with or
    /// without a sign-in ([`Api::for_session`],
    /// 01M3XQVJXWBC3DKAVWBPXPSGZS).
    pub fn signed_in(mut self, session: Option<&str>) -> Result<Self> {
        if let (Some(dir), Some(session)) = (local::marks(), session) {
            self = self.for_session(&dir, session);
        }
        if !secrets::has_keyring() || login::stored(self.base())?.is_none() {
            return Ok(self);
        }
        self.auth = Some(Arc::new(Auth {
            key: device::key(self.base())?,
            session: session.map(str::to_owned),
            held: Mutex::new(None),
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
        let old = login::stored(self.base())?
            .is_some_and(|s| s.riff_id.as_deref() != Some(config.riff_id.as_str()));
        if old {
            login::logout(self.base())?;
            bail!(text::new_riff(self.base()));
        }
        Ok(())
    }

    /// A live access token for the caller, from a task of its own
    /// (01M3ND6R8YXN1KTRTRAV5A7F14). [`Api::access_token`] holds the
    /// lock of the session token, and the first check of the riff,
    /// across its requests. A caller can stop polling its future, for example a
    /// `select!` that runs another branch, and the other branch can then
    /// wait for the same lock. The runtime polls the task, so the lock
    /// is always given back, and one swap still runs at a time.
    ///
    /// The return type says `Send`: `access_token` comes back here
    /// through `session_token`, so the compiler cannot infer it.
    fn token_in_task(
        &self,
        auth: &Arc<Auth>,
    ) -> impl Future<Output = Result<String>> + Send + 'static {
        let (api, auth) = (self.clone(), auth.clone());
        let task = tokio::spawn(async move { api.access_token(&auth).await });
        async move { task.await.context("the task of the access token failed")? }
    }

    /// A live access token for the caller. A session swaps the person
    /// token for a new session token before the old one expires
    /// (01M3WFVADCDZM8XX590KAEMEYG).
    async fn access_token(&self, auth: &Auth) -> Result<String> {
        auth.riff_checked
            .get_or_try_init(|| self.check_riff())
            .await?;
        let Some(session) = &auth.session else {
            return login::access_token(&self.anonymous()).await;
        };
        let mut held = auth.held.lock().await;
        if let Some(live) = held
            .as_ref()
            .filter(|h| now() + login::REFRESH_MARGIN.as_secs() < h.expires_at)
        {
            return Ok(live.access_token.clone());
        }
        let reply = login::session_token(&self.anonymous(), session).await?;
        let access_token = reply.access_token.clone();
        *held = Some(Held {
            expires_at: now() + reply.expires_in,
            access_token: reply.access_token,
        });
        Ok(access_token)
    }

    /// Drops `token` when it is the token that the caller holds, so that
    /// the next request gets a new one (01M3MX4VCEBTY0DN4JMF624WYE).
    async fn forget(&self, auth: &Auth, token: &str) -> Result<()> {
        if auth.session.is_none() {
            return login::forget(self.base(), token).await;
        }
        if let Some(held) = auth.held.lock().await.as_mut()
            && held.access_token == token
        {
            held.expires_at = 0;
        }
        Ok(())
    }

    /// A request to one path, with a token and a proof when the client
    /// is signed in, and the token. The proof names the URL without the
    /// query. A call goes on the shared client of the link, a stream on a
    /// connection of its own (01M3WN72ECF0WKR4M7M6ZYAF9J).
    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        via: Via,
    ) -> Result<(reqwest::RequestBuilder, Option<String>)> {
        let url = format!("{}{path}", self.base());
        let client = match via {
            Via::Call => self.link.client(),
            Via::Stream => self.link.streams(),
        };
        let request = client
            .request(method.clone(), &url)
            .header(build::HEADER, build::VERSION);
        let Some(auth) = &self.auth else {
            return Ok((request, None));
        };
        let token = match self.token_in_task(auth).await {
            Ok(token) => token,
            Err(error) => return Err(self.no_token(error).await),
        };
        let proof = auth.key.proof(method.as_str(), &url, Some(&token), now());
        let request = request
            .header("authorization", format!("DPoP {token}"))
            .header("dpop", proof);
        Ok((request, Some(token)))
    }

    /// Sends a call to one path, checks the build of the reply, and gives
    /// the whole reply. See [`Api::send_with`].
    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        body: impl Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
    ) -> Result<Reply> {
        self.send_with(method, path, body, Check::Build).await
    }

    /// Sends a call to one path, with the rule for a new try of the link
    /// (01M4A8041F8EK1VYDE4C9QG8N8), and gives the whole reply, also a
    /// refusal. `body` adds the rest to each try of the request.
    async fn send_with(
        &self,
        method: reqwest::Method,
        path: &str,
        body: impl Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
        check: Check,
    ) -> Result<Reply> {
        match self.tries(method, path, body, check, Via::Call).await? {
            Got::Whole(reply) => Ok(reply),
            Got::Open(_) => unreachable!("a call gets the whole reply"),
        }
    }

    /// Opens a stream at one path, with the rule for a new try of the
    /// link, and checks the build of its head.
    async fn open(
        &self,
        path: &str,
        body: impl Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response> {
        let got = self.tries(reqwest::Method::GET, path, body, Check::Build, Via::Stream);
        match got.await? {
            Got::Open(response) => Ok(response),
            Got::Whole(_) => unreachable!("a stream gets its head"),
        }
    }

    /// Sends a request until it gets a reply or a refusal, or its budget
    /// ends (01M4A803Z4Q0KX6NT1KC6QR43H, 01M4A8041F8EK1VYDE4C9QG8N8). Each
    /// try has the limit `min(TRY_WAIT, the budget that is left)`. A
    /// fault gets a new try after a wait of [`link::waits`]. After a 401
    /// to a token, it sends the request once more with a new token
    /// (01M3MX4VCEBTY0DN4JMF624WYE). The waits of a gap show
    /// [`link::WAITING`] one time (01M3THEE5V3RFHF9QTA8MA8QDF).
    ///
    /// Each request of the client goes through this function. So it
    /// has the one check of the leave: before each request, also before
    /// a new try, it fails with [`Left`] when the session of the client
    /// left the riff (01M3XQVJXWBC3DKAVWBPXPSGZS).
    async fn tries(
        &self,
        method: reqwest::Method,
        path: &str,
        body: impl Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
        check: Check,
        via: Via,
    ) -> Result<Got> {
        let limits = self.link.limits();
        let deadline = tokio::time::Instant::now() + self.budget.time(&limits);
        let mut waits = link::waits(&limits);
        let mut waited = Duration::ZERO;
        let mut again = true;
        loop {
            if self.left() {
                return Err(Left.into());
            }
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            // A token is a request too: it gets the budget that is left.
            // A box: a request may need a token, and a token is a request.
            let within = self.with_budget(left);
            let (request, token) = Box::pin(within.request(method.clone(), path, via)).await?;
            let limit = limits.try_wait.min(left);
            let renew = again && token.is_some();
            let tried = self.try_once(body(request), limit, check, via, renew).await;
            let last = match tried {
                Step::Done(got) => {
                    self.link.ended_wait();
                    return Ok(got);
                }
                Step::End(error) => return Err(error),
                Step::Again => {
                    again = false;
                    if let (Some(auth), Some(token)) = (&self.auth, &token) {
                        self.forget(auth, token).await?;
                    }
                    continue;
                }
                Step::Fault(last) => last,
            };
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            let wait = waits.next().unwrap_or(limits.most_wait);
            if !self.budget.again() || wait >= left {
                return last;
            }
            self.link.show_wait(waited, self.waits.as_ref());
            waited += wait;
            tokio::time::sleep(wait).await;
        }
    }

    /// One try of a request in `limit` (01M4A8041F8EK1VYDE4C9QG8N8).
    /// With `renew`, a 401 asks for a new token.
    async fn try_once(
        &self,
        request: reqwest::RequestBuilder,
        limit: Duration,
        check: Check,
        via: Via,
        renew: bool,
    ) -> Step {
        let link = &self.link;
        let no_reply = || -> anyhow::Error {
            NoReply {
                base: self.base().to_owned(),
                wait: limit,
            }
            .into()
        };
        // A call has the limit to the end of its reply. A stream has it
        // to the end of its head: its body has no limit.
        let sent = match via {
            Via::Call => request.timeout(limit).send().await,
            Via::Stream => match tokio::time::timeout(limit, request.send()).await {
                Ok(sent) => sent,
                Err(_) => return Step::Fault(Err(no_reply())),
            },
        };
        let response = match sent {
            Ok(response) => response,
            Err(error) => {
                let away = format!("cannot reach riff-server at {}", self.base());
                // No server on this machine (01M4A804683G1EXM53893VHW7S).
                if link::refused(&error) && link::loopback(self.base()) && !link.replied() {
                    return Step::End(anyhow::Error::new(error).context(away));
                }
                if error.is_builder() {
                    return Step::End(error.into());
                }
                // A fault with no HTTP reply: a dead connection can stay
                // in the pool (01M4A8043S2ZCKRH19Z3Q8AJ1F).
                if via == Via::Call {
                    link.swap();
                }
                if error.is_timeout() && !error.is_connect() {
                    return Step::Fault(Err(no_reply()));
                }
                return Step::Fault(Err(anyhow::Error::new(error).context(away)));
            }
        };
        link.note_reply();
        let status = response.status();
        let has_build = response.headers().contains_key(build::HEADER);
        let outcome = link::outcome(status, has_build);
        // A reply of the front end, before the check of the build
        // (01M3QCMJ9F1GRTRRSB4AW9TC3D).
        if outcome == Outcome::Fault && !has_build {
            let base = self.base().to_owned();
            return Step::Fault(Err(Outage { base, status }.into()));
        }
        if check == Check::Build
            && let Err(error) = link.check_build(status, response.headers(), response.url())
        {
            return Step::End(error);
        }
        if status == reqwest::StatusCode::UNAUTHORIZED && renew {
            return Step::Again;
        }
        let got = match via {
            Via::Stream => Got::Open(response),
            Via::Call => {
                let headers = response.headers().clone();
                match response.bytes().await {
                    Ok(body) => Got::Whole(Reply {
                        status,
                        headers,
                        body: body.to_vec(),
                    }),
                    // A cut in the body.
                    Err(error) => {
                        link.swap();
                        if error.is_timeout() {
                            return Step::Fault(Err(no_reply()));
                        }
                        let base = self.base().to_owned();
                        let cut = anyhow::Error::new(error)
                            .context(format!("cannot reach riff-server at {base}"));
                        return Step::Fault(Err(cut));
                    }
                }
            }
        };
        match outcome {
            // A 503 of riff-server: when the budget ends, the call gives it.
            Outcome::Fault => Step::Fault(Ok(got)),
            Outcome::Reply | Outcome::Refusal => Step::Done(got),
        }
    }

    /// Says where the session works now. Call it at the start and after
    /// each move. It is not a worker.
    pub async fn register(&self, me: &SessionUri) -> Result<()> {
        self.register_as(me, false).await
    }

    /// [`Api::register`], a worker or not. A worker says that it is a
    /// worker (01M3NT4M159EHN5W8JRTQ417N4).
    pub async fn register_as(&self, me: &SessionUri, worker: bool) -> Result<()> {
        let register = Register {
            me: me.clone(),
            worker,
        };
        self.call(&register).await
    }

    /// A keep-alive: the session still runs (R204).
    pub async fn alive(&self, me: &SessionUri) -> Result<AliveReply> {
        self.alive_with(me, None, None).await
    }

    /// A keep-alive with the newest fact of the hooks of the session
    /// (01M41FZNTPXQNCZ1S99HE42PYQ), and the seconds since the last
    /// prompt of its person (01M48VDWPDYRPEAXHR1MYDN1M7).
    pub async fn alive_with(
        &self,
        me: &SessionUri,
        activity: Option<Activity>,
        prompt_secs: Option<u64>,
    ) -> Result<AliveReply> {
        let alive = Alive {
            me: me.clone(),
            activity,
            prompt_secs,
        };
        self.call(&alive).await
    }

    /// Says that `me` cannot go on with no decision, in one command
    /// (01M41FZPGEK4TNPSM2051W4VMS): it tells the lead of its user
    /// `blocked: REASON`, and sets the block. With no lead, the block
    /// holds, and the session asks its own user. The lead itself tells
    /// nobody: it waits for its own person (01M48VDSB4CHQS9P6XVDJ6FMKS).
    pub async fn blocked(&self, me: &SessionUri, reason: &str) -> Result<Told> {
        let set = SetBlocked {
            me: me.clone(),
            reason: reason.to_owned(),
        };
        set.check().map_err(anyhow::Error::msg)?;
        let lead = self
            .me(me)
            .await
            .is_ok_and(|r| r.session.is_some_and(|s| s.uri.lead()));
        let told = if lead {
            Told::You
        } else if self
            .tell(me, LEAD, &format!("blocked: {reason}"))
            .await
            .is_ok()
        {
            Told::Lead
        } else {
            Told::Nobody
        };
        self.call(&set).await?;
        Ok(told)
    }

    /// Changes the long step of `me` (01M48VDGTD40P8RBZMS0XB5M9N). A
    /// failed step also tells the lead of its user `step failed: NAME:
    /// REASON` (01M48VDS663X064YS5ZGCCZSTB), unless `me` is the lead.
    /// It gives who got the message.
    pub async fn step(&self, me: &SessionUri, change: StepChange) -> Result<Told> {
        let set = SetStep {
            me: me.clone(),
            change,
        };
        set.check().map_err(anyhow::Error::msg)?;
        let StepChange::Fail { reason } = &set.change else {
            self.call(&set).await?;
            return Ok(Told::Nobody);
        };
        let session = self.me(me).await.ok().and_then(|r| r.session);
        let told = if session.as_ref().is_some_and(|s| s.uri.lead()) {
            Told::You
        } else {
            let name = session
                .and_then(|s| s.step)
                .map_or_else(|| "a step".to_owned(), |s| s.name);
            let body = format!("step failed: {name}: {reason}");
            if self.tell(me, LEAD, &body).await.is_ok() {
                Told::Lead
            } else {
                Told::Nobody
            }
        };
        self.call(&set).await?;
        Ok(told)
    }

    /// The look of the lead `me` at the blocks of the sessions of its
    /// user (01M41FZQ545HQ9Q75CSKX8HF8H). It gives each block that the
    /// look made unanswered.
    pub async fn look_blocks(&self, me: &SessionUri, after_secs: u64) -> Result<Vec<Unanswered>> {
        let look = BlockedLook {
            me: me.clone(),
            after_secs,
        };
        Ok(self.call(&look).await?.unanswered)
    }

    /// Gives the server what `me` saw of the items of its repository on
    /// the forge (01M41FZP2C4Z4J6WKRXZ5B31EH). With `all`, the facts
    /// replace each fact of the repository.
    pub async fn item_facts(&self, me: &SessionUri, items: Vec<ItemFact>, all: bool) -> Result<()> {
        let facts = ItemFacts {
            me: me.clone(),
            items,
            all,
        };
        self.call(&facts).await
    }

    /// Reads the settings of idle workers. With a value, it sets each
    /// given value first: the command `set_idle`, on a path of its own
    /// (01M3Q5A0TF9K49V8Z1ZY9NDF74, 01M3WRD9BSBKS9TN66H29TGTBV).
    pub async fn idle(
        &self,
        me: &SessionUri,
        per_host: Option<u16>,
        after_secs: Option<u64>,
    ) -> Result<Idle> {
        let me = me.clone();
        if per_host.is_none() && after_secs.is_none() {
            return self.call(&IdleQuery { me }).await;
        }
        let request = SetIdle {
            me,
            per_host,
            after_secs,
        };
        self.call(&request).await
    }

    /// The session ended (R205).
    pub async fn end(&self, me: &SessionUri) -> Result<()> {
        self.call(&End { me: me.clone() }).await
    }

    /// The session `me` of this client leaves the riff
    /// (01M3XQVK05FAT3PR43W8RNEYHY). It writes the mark first, so each
    /// other request of the session stops from now. Then it sends the
    /// end call, the one request that goes out with the mark. When the
    /// end call fails, it removes the mark: the session stays in the
    /// riff. A client with no mark cannot keep a leave, and refuses.
    pub async fn leave_riff(&self, me: &SessionUri) -> Result<()> {
        let Some(mark) = &self.mark else {
            bail!(text::LEAVE_NO_MARK);
        };
        local::leave(&mark.dir, &mark.session)
            .with_context(|| format!("cannot write the mark of the leave in {:?}", mark.dir))?;
        let ended = self.unmarked().end(me).await;
        if ended.is_err() {
            let _ = local::join(&mark.dir, &mark.session);
        }
        ended
    }

    /// The session `me` of this client joins the riff again
    /// (01M3MEEFKX14QCQM0F9ZYW93PP): it removes the mark, and registers,
    /// a worker or not.
    pub async fn join_riff(&self, me: &SessionUri, worker: bool) -> Result<()> {
        if let Some(mark) = &self.mark {
            local::join(&mark.dir, &mark.session).with_context(|| {
                format!("cannot remove the mark of the leave in {:?}", mark.dir)
            })?;
        }
        self.register_as(me, worker).await
    }

    /// A new start of the session: a new agent process, a resume or a
    /// `/clear`. Its claims are free at once (01M3JEE1QQCFS5TMZW5N2DAD2D).
    /// The call says why the session starts, and if it is a worker
    /// (01M3X9X9M079WGFPJZHNXH9VEP).
    pub async fn start(
        &self,
        me: &SessionUri,
        reason: StartReason,
        worker: bool,
    ) -> Result<Vec<Freed>> {
        let start = Start {
            me: me.clone(),
            reason,
            worker,
        };
        Ok(self.call(&start).await?.freed)
    }

    /// Lists the sessions. `all` lists gone sessions too.
    pub async fn who(&self, me: &SessionUri, all: bool) -> Result<Vec<SessionInfo>> {
        Ok(self.roster(me, all).await?.sessions)
    }

    /// Lists the sessions and names the owner of the riff
    /// (01M3Q63NK0AHM25MB258B0K8XP). `all` lists gone sessions too.
    pub async fn roster(&self, me: &SessionUri, all: bool) -> Result<WhoReply> {
        let request = WhoRequest {
            me: me.clone(),
            all,
        };
        self.call(&request).await
    }

    /// Only the session `me` and the build of the server, for the
    /// status line (01M3T5GFVS8NMA992KHZN4VE17). It is not a call of
    /// `me`: the server changes nothing.
    pub async fn me(&self, me: &SessionUri) -> Result<MeReply> {
        self.fetch("me", &[("uri", me.to_string())]).await
    }

    /// The facts of the server, for `riff server`
    /// (01M3TJWJ12WEDCXW3W0529KRP2). The server answers also while it
    /// replies 503 to each other call, and to a `riff` of each version.
    /// An error when the server has no facts, for example an old server,
    /// or when the caller has no sign-in at a riff with sign-in.
    pub async fn facts(&self) -> Result<ServerFacts> {
        let response = self
            .send_with(reqwest::Method::GET, "/v1/server", |r| r, Check::None)
            .await?;
        response.error_for_status()?.json()
    }

    pub async fn threads(&self, me: &SessionUri) -> Result<Vec<ThreadInfo>> {
        Ok(self.call(&Threads { me: me.clone() }).await?.threads)
    }

    pub async fn join(&self, me: &SessionUri, thread: &ThreadName) -> Result<()> {
        let join = Join {
            me: me.clone(),
            thread: thread.clone(),
        };
        self.call(&join).await
    }

    pub async fn leave(&self, me: &SessionUri, thread: &ThreadName) -> Result<()> {
        let leave = Leave {
            me: me.clone(),
            thread: thread.clone(),
        };
        self.call(&leave).await
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
        self.call(&request).await
    }

    /// Sets the status of `me`. It replaces the old status (R182).
    pub async fn status(&self, me: &SessionUri, status: &Status) -> Result<()> {
        status.check().map_err(anyhow::Error::msg)?;
        let request = SetStatus {
            me: me.clone(),
            status: status.clone(),
        };
        self.call(&request).await
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
    /// thread with no messages to show. It reads each page.
    pub async fn inbox(
        &self,
        me: &SessionUri,
        thread: Option<&ThreadName>,
        all: bool,
    ) -> Result<Vec<Inbox>> {
        self.inbox_pages(me, thread, all, None, true).await
    }

    /// As [`Api::inbox`], but it reads one page of each thread
    /// (01M3TBZBX140GJWCV5GZ73Q5Z5). With `all`, the page starts after
    /// the seq `after`. [`Inbox::next`] tells when more messages follow.
    pub async fn inbox_page(
        &self,
        me: &SessionUri,
        thread: Option<&ThreadName>,
        all: bool,
        after: Option<u64>,
    ) -> Result<Vec<Inbox>> {
        self.inbox_pages(me, thread, all, after, false).await
    }

    async fn inbox_pages(
        &self,
        me: &SessionUri,
        thread: Option<&ThreadName>,
        all: bool,
        after: Option<u64>,
        each_page: bool,
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
            let (messages, next) = if each_page {
                (self.read(me, &thread, all).await?, None)
            } else {
                self.read_page(me, &thread, all, after).await?
            };
            if !messages.is_empty() {
                out.push(Inbox {
                    thread,
                    members,
                    messages,
                    next,
                });
            }
        }
        Ok(out)
    }

    /// The unread messages (or all of them) of one thread, each checked
    /// with the keys that the server gives (R199), or with its trusted
    /// mark (R212). It reads each page.
    pub async fn read(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
        all: bool,
    ) -> Result<Vec<Checked>> {
        self.read_pages(me, thread, all, None).await
    }

    /// Each kept message of one thread after the seq `after`, checked
    /// like [`Api::read`]. It reads each page. `riff tail` and
    /// `riff chat` read with it after a break (see [`crate::catch_up`]).
    pub async fn read_after(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
        after: u64,
    ) -> Result<Vec<Checked>> {
        self.read_pages(me, thread, true, Some(after)).await
    }

    async fn read_pages(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
        all: bool,
        mut after: Option<u64>,
    ) -> Result<Vec<Checked>> {
        let mut out = Vec::new();
        loop {
            let (mut messages, next) = self.read_page(me, thread, all, after).await?;
            out.append(&mut messages);
            match next {
                Some(next) => after = Some(next),
                None => return Ok(out),
            }
        }
    }

    /// One page of [`Api::read`], and the seq of its last message when
    /// more messages follow (01M3TBZBX140GJWCV5GZ73Q5Z5). With `all`, the
    /// page starts after the seq `after`.
    pub async fn read_page(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
        all: bool,
        after: Option<u64>,
    ) -> Result<(Vec<Checked>, Option<u64>)> {
        let request = Read {
            me: me.clone(),
            thread: thread.clone(),
            all,
            after: after.filter(|_| all),
        };
        let reply = self.call(&request).await?;
        let messages = reply
            .messages
            .into_iter()
            .map(|message| checked(thread, message, &reply.keys, reply.trusted))
            .collect();
        Ok((messages, reply.next))
    }

    /// Claims a work item. The server refuses a claim of an item that
    /// another session holds with the code `held`, and a text that names
    /// the holder (01M3WRD9JBQMNN96TXJH8EAJ3W). It refuses a claim of a
    /// worker of a held item with the code `on_hold`, and a text that
    /// names the lead, the time and the reason
    /// (01M43GSGPJ69TPWPA4935WR8RW). These refusals are not an error
    /// here: the answer is not granted, and it has the text.
    pub async fn claim(&self, me: &SessionUri, thread: &ThreadName, item: &str) -> Result<Claimed> {
        let claim = Claim {
            me: me.clone(),
            thread: thread.clone(),
            item: item.to_owned(),
        };
        match self.call(&claim).await {
            Ok(ClaimReply { holder, warning }) => Ok(Claimed {
                granted: true,
                holder: Some(holder),
                held: None,
                warning,
            }),
            Err(error) => match error.downcast::<Refusal>() {
                Ok(refusal) if matches!(refusal.code.as_deref(), Some("held" | "on_hold")) => {
                    Ok(Claimed {
                        granted: false,
                        holder: None,
                        held: Some(refusal.text),
                        warning: None,
                    })
                }
                Ok(refusal) => Err(refusal.into()),
                Err(error) => Err(error),
            },
        }
    }

    /// Holds `item` of the repository thread `thread` with `reason`, so
    /// that no worker can claim it (01M43GSGB9ZFHSG0Q83Y50FEGW). Only a
    /// lead of the thread, the owner or an admin can
    /// (01M43GSGGY0QMB5D5EH92M6ZFP).
    pub async fn hold(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
        item: &str,
        reason: &str,
    ) -> Result<HoldReply> {
        let hold = Hold {
            me: me.clone(),
            thread: thread.clone(),
            item: item.to_owned(),
            reason: reason.to_owned(),
        };
        self.call(&hold).await
    }

    /// Ends the hold of `item` of the repository thread `thread`
    /// (01M43GSGB9ZFHSG0Q83Y50FEGW).
    pub async fn free(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
        item: &str,
    ) -> Result<FreeReply> {
        let free = Free {
            me: me.clone(),
            thread: thread.clone(),
            item: item.to_owned(),
        };
        self.call(&free).await
    }

    /// Frees a claim of `me`. The reply says if `me` is a worker that
    /// must clear its context now (01M3X9XB37TQCXWPNFZRMRGJB4).
    pub async fn release(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
        item: &str,
    ) -> Result<ReleaseReply> {
        let release = Release {
            me: me.clone(),
            thread: thread.clone(),
            item: item.to_owned(),
        };
        self.call(&release).await
    }

    /// Frees the claim of the session `holder` (its session ID, or the
    /// start of it) for it. Only the lead of the user of the holder can
    /// (01M3WG243BW7P6E1ME0DFNQF8C).
    pub async fn release_for(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
        item: &str,
        holder: &str,
    ) -> Result<()> {
        let request = ReleaseFor {
            me: me.clone(),
            thread: thread.clone(),
            item: item.to_owned(),
            session: holder.to_owned(),
        };
        self.call(&request).await
    }

    /// Makes `me` the lead of its user in its repository. It replaces
    /// the old lead (R177).
    pub async fn lead(&self, me: &SessionUri) -> Result<LeadReply> {
        self.call(&Lead { me: me.clone() }).await
    }

    /// The state of the riff for the place of `me`: paused when the
    /// whole riff or the repository of `me` is paused
    /// (01M3JCFTWCR72HQB8CBTQKXJNF, 01M3XAHZBGSSJB3YX23K88W01K).
    pub async fn riff(&self, me: &SessionUri) -> Result<RiffState> {
        Ok(self.pauses(me).await?.state)
    }

    /// The pauses of the riff as `me` sees them: the pause of the whole
    /// riff, and each repository that is paused, with who set each
    /// (01M3XAHZJAF6YVDJ7WX74X8RBX).
    pub async fn pauses(&self, me: &SessionUri) -> Result<RiffReply> {
        self.call(&RiffQuery { me: me.clone() }).await
    }

    /// Pauses or resumes the whole riff: [`Api::set_pause`] with
    /// [`PauseScope::Riff`]. Only the owner or an admin can
    /// (01M3XAHZDSQR263QZVB41CK0MX).
    pub async fn set_riff(
        &self,
        me: &SessionUri,
        state: RiffState,
    ) -> Result<(RiffReply, Vec<Posted>)> {
        self.set_pause(me, &PauseScope::Riff, state).await
    }

    /// Pauses or resumes `scope` (01M3XAHZBGSSJB3YX23K88W01K). See
    /// 01M3XAHZDSQR263QZVB41CK0MX for who can. When the state changes,
    /// it wakes each session that the change stops or starts, and that
    /// is not gone (01M3JCG3YD7C2Y3V0QJPF082YH,
    /// 01M3XAHZSJ5914BRQBZ2G4ZBSA): it posts [`text::riff_news`] to the
    /// thread of the repository, to that repository. For the whole
    /// riff, it posts to each repository of a session that is not
    /// paused. A session that another pause still stops does not wake.
    pub async fn set_pause(
        &self,
        me: &SessionUri,
        scope: &PauseScope,
        state: RiffState,
    ) -> Result<(RiffReply, Vec<Posted>)> {
        // A pause and a resume are two commands, each on a path of its
        // own (01M3WRD9BSBKS9TN66H29TGTBV).
        let (riff, repository) = match scope {
            PauseScope::Here => (false, None),
            PauseScope::Repository(thread) => (false, Some(thread.clone())),
            PauseScope::Riff => (true, None),
        };
        let me_now = me.clone();
        let reply: RiffReply = match state {
            RiffState::Paused => {
                let pause = Pause {
                    me: me_now,
                    riff,
                    repository,
                };
                self.call(&pause).await?
            }
            RiffState::Running => {
                let resume = Resume {
                    me: me_now,
                    riff,
                    repository,
                };
                self.call(&resume).await?
            }
        };
        if !reply.changed {
            return Ok((reply, Vec::new()));
        }
        let thread = scope.repository(me);
        let body = text::riff_news(thread.as_ref(), state);
        let repos = match thread {
            // The whole riff is paused: its sessions stay paused.
            Some(_) if reply.riff.is_some() => Vec::new(),
            Some(thread) => vec![thread],
            None => {
                let mut repos = self.repos(me, false).await?;
                repos.retain(|repo| reply.repository(repo).is_none());
                repos
            }
        };
        let posted = self.post_to_repos(me, repos, &body, Kind::Message).await?;
        Ok((reply, posted))
    }

    /// The repository of each session in `who` (`all` counts the gone
    /// sessions too), in the order of the names.
    async fn repos(&self, me: &SessionUri, all: bool) -> Result<Vec<ThreadName>> {
        let mut repos: Vec<ThreadName> = self
            .who(me, all)
            .await?
            .into_iter()
            .filter_map(|s| s.uri.default_thread())
            .collect();
        repos.sort();
        repos.dedup();
        Ok(repos)
    }

    /// Posts `body` to the thread of each repository of a session in
    /// `who` (`all` counts the gone sessions too), to that repository.
    async fn post_to_each_repo(
        &self,
        me: &SessionUri,
        all: bool,
        body: &str,
        kind: Kind,
    ) -> Result<Vec<Posted>> {
        let repos = self.repos(me, all).await?;
        self.post_to_repos(me, repos, body, kind).await
    }

    /// Posts `body` to the thread of each repository in `repos`, to
    /// that repository.
    async fn post_to_repos(
        &self,
        me: &SessionUri,
        repos: Vec<ThreadName>,
        body: &str,
        kind: Kind,
    ) -> Result<Vec<Posted>> {
        let mut posted = Vec::new();
        for repo in repos {
            let to = Selector {
                repo: Some(repo.to_string()),
                ..Selector::default()
            };
            posted.push(self.post(me, Some(&repo), &[to], body, kind).await?);
        }
        Ok(posted)
    }

    /// Posts the note of a change of the members, from `me`, to the
    /// thread of each repository of the riff: the repository of each
    /// session in `riff who --all` (01M3MN14ZCTRVD3T455P6TFK1B). The
    /// note wakes no session.
    async fn members_news<T>(&self, me: &SessionUri, done: T, body: &str) -> Changed<T> {
        let news = self.post_to_each_repo(me, true, body, Kind::Note).await;
        Changed { done, news }
    }

    /// The wakes for one session, on one connection. The open stream is
    /// no sign of life: see [`keep_alive`]. [`follow`] connects again.
    pub async fn watch(&self, me: &SessionUri) -> Result<impl Stream<Item = Result<Wake>>> {
        self.events("watch", &[("uri", me.to_string())]).await
    }

    /// Each new message in one thread, on one connection, checked like
    /// [`Api::read`]. `me` is the caller: the server gives a direct thread
    /// only to its two sessions. [`follow`] connects again.
    pub async fn tail(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
    ) -> Result<impl Stream<Item = Result<Checked>>> {
        let query = [("uri", me.to_string()), ("thread", thread.to_string())];
        let events = self.events::<Tailed>("tail", &query).await?;
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
        self.call(&request).await
    }

    /// Adds a member of the riff, by verified email. Only an admin can.
    /// `me` is the person, and posts the note of the change.
    pub async fn invite(&self, me: &SessionUri, email: &str) -> Result<Changed<Invited>> {
        self.need_sign_in().await?;
        let request = Invite {
            email: email.to_owned(),
        };
        let done = self.call(&request).await?;
        let body = text::invited_news(me.who().user(), &done);
        Ok(self.members_news(me, done, &body).await)
    }

    /// Removes a member of the riff and ends each sign-in of that person.
    /// Only an admin can. `me` is the person, and posts the note of the
    /// change.
    pub async fn remove(&self, me: &SessionUri, email: &str) -> Result<Changed<Removed>> {
        self.need_sign_in().await?;
        let request = Remove {
            email: email.to_owned(),
        };
        let done = self.call(&request).await?;
        let body = text::removed_news(me.who().user(), &done);
        Ok(self.members_news(me, done, &body).await)
    }

    /// Makes a person an admin, or an admin a member again. Only the
    /// owner can. `me` is the person, and posts the note of the change.
    pub async fn set_admin(
        &self,
        me: &SessionUri,
        email: &str,
        admin: bool,
    ) -> Result<Changed<AdminSet>> {
        self.need_sign_in().await?;
        let request = SetAdmin {
            email: email.to_owned(),
            admin,
        };
        let done = self.call(&request).await?;
        let body = text::admin_news(me.who().user(), &done);
        Ok(self.members_news(me, done, &body).await)
    }

    /// Passes the owner role to a member or an admin. Only the owner
    /// can. `me` is the person, and posts the note of the change.
    pub async fn pass_owner(&self, me: &SessionUri, email: &str) -> Result<Changed<OwnerPassed>> {
        self.need_sign_in().await?;
        let request = PassOwner {
            email: email.to_owned(),
        };
        let done = self.call(&request).await?;
        let body = text::owner_news(me.who().user(), &done);
        Ok(self.members_news(me, done, &body).await)
    }

    /// Asks for the owner role (01M3N7K3ZAZFGABN7032AYJWEM). Only an
    /// admin can. The server posts the note of the change.
    pub async fn take_owner(&self) -> Result<OwnerAsked> {
        self.need_sign_in().await?;
        self.call(&TakeOwner {}).await
    }

    /// Keeps the owner role that an admin asks for
    /// (01M3N7K41N03P26BEFFNX5617K). Only the owner can. The server posts
    /// the note of the change.
    pub async fn deny_owner(&self) -> Result<OwnerDenied> {
        self.need_sign_in().await?;
        self.call(&DenyOwner {}).await
    }

    /// Who may join the riff.
    pub async fn members(&self) -> Result<MembersReply> {
        self.need_sign_in().await?;
        self.call(&Members {}).await
    }

    /// The records of the repository thread `repo`, for an audit
    /// (01M3ZWRC11R5M9V1KTF05P240W). Only the owner and the admins can
    /// read them.
    pub async fn log(&self, repo: &ThreadName) -> Result<LogReply> {
        self.need_sign_in().await?;
        self.call(&LogQuery { repo: repo.clone() }).await
    }

    /// Fails with what to do when this device has no sign-in.
    async fn need_sign_in(&self) -> Result<()> {
        if self.auth.is_some() {
            return Ok(());
        }
        if matches!(self.has_sign_in().await, Ok(false)) {
            bail!(text::nobody_signs_in(self.base()));
        }
        bail!("no sign-in for {}: run riff login", self.base());
    }

    /// Sends one call, and gives its reply: the one function for each
    /// call (01M3WRD8TBDPA4JNEZY6J4N2EX). The type of the call gives the
    /// path and the type of the reply ([`Call`]). A call that the server
    /// refuses gives a [`Refusal`]. Each try of the call sends one call
    /// ID ([`link::call_id`], 01M4A8048J60YSVNVYF2432KE8), so the server
    /// runs a command one time only.
    async fn call<C: Call>(&self, call: &C) -> Result<C::Reply> {
        let id = link::call_id();
        let response = self
            .send(reqwest::Method::POST, C::PATH, |r| {
                r.json(call).header(CALL_HEADER, &id)
            })
            .await?;
        let status = response.status;
        if !status.is_success() {
            let code = response.header(REFUSED_HEADER).map(str::to_owned);
            let text = response.text();
            let op = C::PATH.trim_start_matches("/v1/").to_owned();
            return Err(Refusal {
                op,
                status,
                code,
                text,
            }
            .into());
        }
        response.json()
    }

    /// `GET /v1/{op}` with `query`.
    async fn fetch<Rep: DeserializeOwned>(
        &self,
        op: &str,
        query: &[(&str, String)],
    ) -> Result<Rep> {
        let response = self
            .send(reqwest::Method::GET, &format!("/v1/{op}"), |r| {
                r.query(query)
            })
            .await?;
        let status = response.status;
        if !status.is_success() {
            let text = response.text();
            bail!("{op} failed ({status}): {text}");
        }
        response.json()
    }

    /// Reads a server-sent event stream and parses each `data:` line.
    /// The stream has a connection of its own
    /// (01M3WN72ECF0WKR4M7M6ZYAF9J).
    async fn events<T: DeserializeOwned>(
        &self,
        op: &str,
        query: &[(&str, String)],
    ) -> Result<impl Stream<Item = Result<T>> + use<T>> {
        let response = self.open(&format!("/v1/{op}"), |r| r.query(query)).await?;
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

/// Whether [`Api::send_with`] checks the build of the reply.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Check {
    /// [`Link::check_build`].
    Build,
    /// No check: a call that each version takes
    /// (01M3MX4V43SF2XFCZWANHD19WV).
    None,
}

/// The client sent no request: its session left the riff
/// (01M3XQVJXWBC3DKAVWBPXPSGZS). The text names `/riff:join`.
///
/// ```
/// assert_eq!(riff::api::Left.to_string(), riff::text::LEFT_COMMAND);
/// ```
#[derive(Debug)]
pub struct Left;

impl std::fmt::Display for Left {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(text::LEFT_COMMAND)
    }
}

impl std::error::Error for Left {}

/// `riff-server` refused a token request, with an OAuth error.
///
/// ```
/// let refused = riff::api::TokenRefused { error: "invalid_grant".into(), description: None };
/// assert_eq!(refused.to_string(), "riff-server refused the token request: invalid_grant");
/// assert!(refused.ended());
/// ```
#[derive(Debug)]
pub struct TokenRefused {
    /// The OAuth error code.
    pub error: String,
    pub description: Option<String>,
}

impl TokenRefused {
    /// True when the server does not take the grant: the sign-in ended,
    /// or the server does not know the token.
    pub fn ended(&self) -> bool {
        self.error == "invalid_grant"
    }
}

impl std::fmt::Display for TokenRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "riff-server refused the token request: {}", self.error)?;
        match &self.description {
            Some(why) => write!(f, ": {why}"),
            None => Ok(()),
        }
    }
}

impl std::error::Error for TokenRefused {}

/// The front end of `riff-server` replied by itself for
/// the budget of a call: see [`link::outcome`].
///
/// ```
/// let outage = riff::api::Outage {
///     base: "http://127.0.0.1:7878".into(),
///     status: reqwest::StatusCode::BAD_GATEWAY,
/// };
/// assert_eq!(
///     outage.to_string(),
///     "riff-server at http://127.0.0.1:7878 does not answer: its front end replied 502 Bad Gateway"
/// );
/// ```
#[derive(Debug)]
pub struct Outage {
    /// The URL of the server.
    pub base: String,
    /// The status of the last reply of the front end.
    pub status: reqwest::StatusCode,
}

impl std::fmt::Display for Outage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "riff-server at {} does not answer: its front end replied {}",
            self.base, self.status
        )
    }
}

impl std::error::Error for Outage {}

/// The server at `base` gave no reply to a call in `wait`. The text is
/// [`text::no_reply`].
///
/// ```
/// use std::time::Duration;
///
/// let base = "http://127.0.0.1:7878";
/// let wait = Duration::from_secs(20);
/// let error = riff::api::NoReply { base: base.into(), wait };
/// assert_eq!(error.to_string(), riff::text::no_reply(base, wait));
/// ```
#[derive(Debug)]
pub struct NoReply {
    /// The URL of the server.
    pub base: String,
    /// How long the caller waited.
    pub wait: Duration,
}

impl std::fmt::Display for NoReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&text::no_reply(&self.base, self.wait))
    }
}

impl std::error::Error for NoReply {}

/// True when a new try can repair `error`
/// (01M3Z8FXE2DY34ZP75WJE1S8HR): riff did not reach the server, the
/// connection failed in the middle of a call, no reply came in time
/// ([`NoReply`]), or the front end replied by itself ([`Outage`]). A
/// command that runs until stopped goes on after such an error.
///
/// False for each other error, for example no sign-in, a refused token
/// ([`TokenRefused`]), a refused call ([`Refusal`]), a session that left
/// ([`Left`]) or a version that riff cannot talk to ([`Mismatch`]). A
/// new try gives the same error, so the command ends with its text.
///
/// ```
/// use std::time::Duration;
/// use riff::api::{Api, Left, NoReply, Outage, TokenRefused, passes};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// // Nothing listens on port 9.
/// let base = "http://127.0.0.1:9";
/// let away = Api::new(base).probe(Duration::from_secs(5)).await.unwrap_err();
/// assert!(passes(&away), "{away:#}");
/// let slow = NoReply { base: base.into(), wait: Duration::from_secs(10) };
/// assert!(passes(&slow.into()));
/// let outage = Outage { base: base.into(), status: reqwest::StatusCode::BAD_GATEWAY };
/// assert!(passes(&outage.into()));
///
/// let refused = TokenRefused { error: "invalid_grant".into(), description: None };
/// assert!(!passes(&refused.into()));
/// assert!(!passes(&Left.into()));
/// assert!(!passes(&anyhow::anyhow!(riff::text::no_sign_in(base, false))));
/// # }
/// ```
pub fn passes(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.is::<Outage>()
            || cause.is::<NoReply>()
            || cause
                .downcast_ref::<reqwest::Error>()
                .is_some_and(|e| !e.is_status() && !e.is_builder())
    })
}

/// Seconds since the Unix epoch, for proofs.
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
    /// The seq of the last message, when more messages follow.
    pub next: Option<u64>,
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
///     payload: None,
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

/// A call that the server refused: the status and the text of the
/// reply, and the code of the refusal when the server names one.
///
/// ```
/// let refusal = riff::api::Refusal {
///     op: "claim".into(),
///     status: reqwest::StatusCode::CONFLICT,
///     code: Some("paused".into()),
///     text: "the riff is paused".into(),
/// };
/// assert_eq!(refusal.to_string(), "claim failed (409 Conflict): the riff is paused");
/// ```
#[derive(Clone, Debug)]
pub struct Refusal {
    /// The call, for example `claim`.
    pub op: String,
    pub status: reqwest::StatusCode,
    /// The code of the refusal, for example `held`.
    pub code: Option<String>,
    pub text: String,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} failed ({}): {}", self.op, self.status, self.text)
    }
}

impl std::error::Error for Refusal {}

/// The answer to a claim ([`Api::claim`]).
#[derive(Clone, Debug)]
pub struct Claimed {
    /// True when the caller holds the item now.
    pub granted: bool,
    /// The URI of the caller now, when the claim is granted.
    pub holder: Option<SessionUri>,
    /// The text of the server that names the holder, when another
    /// session holds the item, or the hold, when a lead holds it and the
    /// caller is a worker.
    pub held: Option<String>,
    /// The hold of the item, when the claim is granted to a session
    /// that is not a worker (01M43GSGPJ69TPWPA4935WR8RW).
    pub warning: Option<String>,
}

/// Sends a keep-alive for `me` each `every`, and never ends
/// (01M3WG240PNMQYZ7TX6Z7ZF6M9). `riff watch` runs it while it watches:
/// an open watch stream is no sign of life for the server, because a
/// front end can hold the stream of a dead client open. A failed
/// keep-alive is not reported: the next one tries again.
///
/// ```mermaid
/// sequenceDiagram
///     participant W as riff watch
///     participant F as front end
///     participant S as riff-server
///     W->>S: watch (a call: a sign of life)
///     loop each 60 s
///         W->>S: keep-alive
///     end
///     Note over W: the process is killed
///     F-->>S: the stream stays open
///     S->>S: no keep-alive for 3 minutes: gone
///     S->>S: 5 minutes: the claims are free
/// ```
pub async fn keep_alive(api: &Api, me: &SessionUri, every: Duration) {
    let mut tick = tokio::time::interval(every);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    tick.tick().await;
    loop {
        tick.tick().await;
        // A hung request must not stop the next keep-alive.
        let _ = tokio::time::timeout(every, api.alive(me)).await;
    }
}

/// [`keep_alive`] that ends when a reply asks this session to stop: the
/// server stops an idle worker (01M4385Z5BN03E6HTEB5GQVZ8X).
pub async fn keep_alive_until_stop(api: &Api, me: &SessionUri, every: Duration) {
    let mut tick = tokio::time::interval(every);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    tick.tick().await;
    loop {
        tick.tick().await;
        // A hung request must not stop the next keep-alive.
        if let Ok(Ok(reply)) = tokio::time::timeout(every, api.alive(me)).await
            && reply.stop
        {
            return;
        }
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
                2..=4 => Err(anyhow::anyhow!("try {n} failed")),
                _ => Ok(futures::stream::iter([Ok(n)])),
            };
            std::future::ready(reply)
        };
        let items: Vec<String> = follow(connect, Duration::from_millis(1))
            .take(4)
            .map(|item| item.map_or_else(|e| e.to_string(), |n: u32| n.to_string()))
            .collect()
            .await;
        assert_eq!(items, ["1", "try 3 failed", "try 4 failed", "5"]);
    }

    /// A cut can leave a dead connection in the pool, so the first
    /// connect after a stream ends can fail. `follow` tries once more at
    /// once, and gives no error (01M3Q59CAA46C316BD4D1ED7C6).
    #[tokio::test]
    async fn follow_tries_once_more_at_once_after_a_stream_ends() {
        let mut n = 0;
        let connect = move || {
            n += 1;
            let reply = match n {
                1 | 3 => Err(anyhow::anyhow!("dead connection")),
                _ => Ok(futures::stream::iter([Ok(n)])),
            };
            std::future::ready(reply)
        };
        // A wait of a minute would stop the test: no connect waits.
        let items = follow(connect, Duration::from_secs(60)).take(2);
        let items = isolated::in_time(Duration::from_secs(5), items.collect::<Vec<_>>());
        let items: Vec<u32> = items
            .await
            .unwrap()
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert_eq!(items, [2, 4]);
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
}
