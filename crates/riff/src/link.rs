//! The link: the one part of `riff` that talks to one `riff-server`
//! (01M4A803WN0KTDGGAX2E771XDF). The design is
//! `docs/src/design-link.md`. [`crate::api::Api`] keeps the typed calls
//! (`who`, `post`, `claim` and the others) on top of the link.
//!
//! # One link for each server
//!
//! [`of`] gives the one link of a server in the process: a map by the
//! URL of the server. Each [`crate::api::Api`] of one server shares its
//! link: one HTTP client for the calls, one client for the streams, and
//! what the process knows of the server (a reply came, a wait line is
//! shown, the note of another build is shown). A test gives
//! [`Limits`] in milliseconds with [`with`]: that link is a link of its
//! own.
//!
//! ```mermaid
//! flowchart LR
//!     A1[Api of top] --> L
//!     A2[Api of a tool call] --> L
//!     A3[Api of a hook] --> L
//!     L["link::of(base):<br/>the client of the calls,<br/>the client of the streams"]
//!     L -->|calls: one shared connection| S[riff-server]
//!     L -->|each stream: a connection of its own| S
//! ```
//!
//! # The budget of a call
//!
//! Each call has a budget: the time from its first try to its end
//! (01M4A803Z4Q0KX6NT1KC6QR43H). The budget is a deadline. The link cuts
//! the open try at the deadline. Each try has the limit
//! `min(TRY_WAIT, the budget that is left)`.
//!
//! | Call | Budget |
//! |---|---|
//! | A short command, a tool call of `riff mcp`, a post of `riff chat`, a look of `riff top`, a call of `riff workers host` | [`SHORT_BUDGET`] |
//! | The start hook | [`crate::hook::STATE_WAIT`] |
//! | The status line | `STATUSLINE_WAIT`, with one try ([`Budget::OneTry`]) |
//! | The end of a session | [`crate::mcp::END_WAIT`] |
//! | The open of a stream | [`SHORT_BUDGET`]; then the stream has no limit, and [`crate::api::follow`] connects again |
//!
//! # One rule for a new try
//!
//! Each try ends in one of three ways: a reply, a fault or a refusal
//! ([`outcome`], 01M4A8041F8EK1VYDE4C9QG8N8).
//!
//! | Outcome | Examples | What the link does |
//! |---|---|---|
//! | reply | Each 2xx of `riff-server` | It gives the reply. |
//! | fault | No connect. A cut before the end of the reply. No reply in the limit of the try. A 5xx or 429 with no build header: a reply of the front end. A 503 of `riff-server`. | A new try after a wait, while the budget lasts. |
//! | refusal | Each other reply of `riff-server`. A build that riff cannot talk to. | No new try. |
//!
//! - A 401 to a token is the one exception: the link gets a new token
//!   and tries one more time.
//! - The wait before a new try grows from [`FIRST_WAIT`] to
//!   [`MOST_WAIT`]. Each wait has a random part of up to half of it
//!   ([`waits`]), so many workers do not try at the same moment after a
//!   deploy.
//! - A fault with no HTTP reply (no connect, a cut, no reply in time)
//!   makes a new client of the calls, which each `Api` of the server
//!   shares ([`Link::swap`], 01M4A8043S2ZCKRH19Z3Q8AJ1F). A 503 or a 429
//!   came on a good connection, so it keeps the client.
//! - A refused connect to a loopback address of a server that never
//!   replied to this process ends the call at once
//!   (01M4A804683G1EXM53893VHW7S). `riff` cannot tell a server that
//!   starts from no server, so `riff who` with no server on this machine
//!   ends at once. Each other connect error is a fault.
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> Try
//!     Try --> Reply: a reply
//!     Try --> Refusal: a refusal
//!     Try --> Wait: a fault, budget left
//!     Try --> Ended: a fault, no budget left
//!     Try --> Ended: a refused connect to a loopback server that never replied
//!     Wait --> Try: the same call ID
//!     Reply --> [*]
//!     Refusal --> [*]
//!     Ended --> [*]: the error of the last fault
//! ```
//!
//! # Each call gets to the server one time
//!
//! Each call of a command has a call ID ([`call_id`]): 16 random bytes in
//! base 64, in the header [`riff_core::wire::CALL_HEADER`]
//! (01M4A8048J60YSVNVYF2432KE8). Each try of the call sends the same ID.
//! So the link sends a command again after each fault, also after a cut
//! and after no reply in time, and `riff-server` runs it one time only
//! (01M48VFX22S4811DYBBD7QDW24). The link sends the ID with each call of
//! [`riff_core::wire::Call`]. The server reads it only for a command: a
//! query and a signal are safe to send two times.
//!
//! # The constants
//!
//! | Constant | Value |
//! |---|---|
//! | [`CONNECT_WAIT`] | 5 s |
//! | [`TRY_WAIT`] | 20 s |
//! | [`SHORT_BUDGET`] | 60 s |
//! | [`FIRST_WAIT`] | 250 ms |
//! | [`MOST_WAIT`] | 5 s |
//! | [`STREAM_IDLE`] | 45 s |
//! | [`STREAM_RETRY`] | 5 s |
//! | [`LINE_AFTER`] | 1 s |

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use riff_core::build::{self, Build, Mismatch};

use crate::auto_update;

/// The limit of each connect, of a call or a stream
/// (01M48RW9E8NS2FPHFHG2S10R7A).
pub const CONNECT_WAIT: Duration = Duration::from_secs(5);

/// The limit of each try of a call, from the send to the end of the
/// reply (01M48RW9E8NS2FPHFHG2S10R7A).
pub const TRY_WAIT: Duration = Duration::from_secs(20);

/// A stream that gives no byte for this time ends, and
/// [`crate::api::follow`] connects again (01M48RW9HNKPNZ75H9R01BG6V5). It
/// is three keep-alive comments of the server.
pub const STREAM_IDLE: Duration = Duration::from_secs(45);

/// The client of the calls sends an HTTP/2 ping at this interval.
pub const PING_EVERY: Duration = Duration::from_secs(10);

/// The client of the calls drops a connection when a ping gets no
/// answer in this time.
pub const PING_WAIT: Duration = Duration::from_secs(5);

/// The budget of a call of a short command, of a tool call of `riff
/// mcp`, of a post of `riff chat`, of a look of `riff top` and of a call
/// of `riff workers host` (01M4A803Z4Q0KX6NT1KC6QR43H).
pub const SHORT_BUDGET: Duration = Duration::from_secs(60);

/// The first wait before a new try of a call.
pub const FIRST_WAIT: Duration = Duration::from_millis(250);

/// The longest wait between two tries of a call.
pub const MOST_WAIT: Duration = Duration::from_secs(5);

/// The wait before a new connect of a stream after a connect failed.
pub const STREAM_RETRY: Duration = Duration::from_secs(5);

/// A call shows [`WAITING`] when its waits for a new try, with the wait
/// that comes, add up to this.
pub const LINE_AFTER: Duration = Duration::from_secs(1);

/// The line that a call shows while it waits for a new try
/// (01M3THEE5V3RFHF9QTA8MA8QDF).
pub const WAITING: &str = "(waits for riff-server…)";

/// The time limits of a link. [`Limits::default`] has the constants. A
/// test gives limits in milliseconds, or the budget of a `riff` process
/// with [`BUDGET_VAR`].
///
/// ```
/// use std::time::Duration;
/// use riff::link::{Limits, CONNECT_WAIT, FIRST_WAIT, MOST_WAIT, SHORT_BUDGET, STREAM_IDLE, TRY_WAIT};
///
/// let limits = Limits::default();
/// assert_eq!(limits.connect, CONNECT_WAIT);
/// assert_eq!(limits.try_wait, TRY_WAIT);
/// assert_eq!(limits.stream_idle, STREAM_IDLE);
/// assert_eq!(limits.budget, SHORT_BUDGET);
/// assert_eq!((limits.first_wait, limits.most_wait), (FIRST_WAIT, MOST_WAIT));
/// assert_eq!(STREAM_IDLE, Duration::from_secs(45));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Limits {
    /// The limit of each connect.
    pub connect: Duration,
    /// The limit of each try of a call.
    pub try_wait: Duration,
    /// The longest time with no byte on a stream.
    pub stream_idle: Duration,
    /// The budget of a call with [`Budget::Short`].
    pub budget: Duration,
    /// The first wait before a new try.
    pub first_wait: Duration,
    /// The longest wait before a new try.
    pub most_wait: Duration,
}

/// The environment variable that sets [`Limits::budget`] of a `riff`
/// process in milliseconds. Only a test sets it.
///
/// ```
/// assert_eq!(riff::link::BUDGET_VAR, "RIFF_LINK_BUDGET_MS");
/// ```
pub const BUDGET_VAR: &str = "RIFF_LINK_BUDGET_MS";

/// [`SHORT_BUDGET`], or the budget that [`BUDGET_VAR`] sets.
fn short_budget() -> Duration {
    std::env::var(BUDGET_VAR)
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map_or(SHORT_BUDGET, Duration::from_millis)
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            connect: CONNECT_WAIT,
            try_wait: TRY_WAIT,
            stream_idle: STREAM_IDLE,
            budget: short_budget(),
            first_wait: FIRST_WAIT,
            most_wait: MOST_WAIT,
        }
    }
}

/// The budget of a call (01M4A803Z4Q0KX6NT1KC6QR43H). See "The budget of
/// a call" in the module doc.
///
/// ```
/// use std::time::Duration;
/// use riff::link::{Budget, Limits, SHORT_BUDGET};
///
/// let limits = Limits::default();
/// assert_eq!(Budget::Short.time(&limits), SHORT_BUDGET);
/// let three = Duration::from_secs(3);
/// assert_eq!(Budget::Within(three).time(&limits), three);
/// assert!(Budget::Within(three).again());
/// assert!(!Budget::OneTry(three).again());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Budget {
    /// [`Limits::budget`]: [`SHORT_BUDGET`] by default.
    #[default]
    Short,
    /// Tries while this time lasts.
    Within(Duration),
    /// One try, with this time as its limit: the status line.
    OneTry(Duration),
}

impl Budget {
    /// The time of the budget.
    pub fn time(self, limits: &Limits) -> Duration {
        match self {
            Budget::Short => limits.budget,
            Budget::Within(time) | Budget::OneTry(time) => time,
        }
    }

    /// True when a fault gets a new try.
    pub fn again(self) -> bool {
        !matches!(self, Budget::OneTry(_))
    }
}

/// The waits before each new try of one call: from
/// [`Limits::first_wait`], double each time, up to [`Limits::most_wait`].
/// Each wait has a random part of up to half of it. It never ends: the
/// budget ends the call.
///
/// ```
/// use std::time::Duration;
/// use riff::link::{Limits, waits};
///
/// let limits = Limits::default();
/// let waits: Vec<Duration> = waits(&limits).take(8).collect();
/// for (wait, full) in waits.iter().zip([250, 500, 1000, 2000, 4000, 5000, 5000, 5000]) {
///     let full = Duration::from_millis(full);
///     assert!(*wait <= full && *wait >= full / 2, "{wait:?} for {full:?}");
/// }
/// ```
pub fn waits(limits: &Limits) -> impl Iterator<Item = Duration> + use<> {
    waits_with(limits, random_below)
}

/// The waits of [`waits`], with `part` as the random part: it gets half
/// of the full wait and gives a time from zero to it.
fn waits_with<F: FnMut(Duration) -> Duration>(
    limits: &Limits,
    mut part: F,
) -> impl Iterator<Item = Duration> + use<F> {
    let (mut next, most) = (limits.first_wait, limits.most_wait);
    std::iter::from_fn(move || {
        let full = next.min(most);
        next = (next * 2).min(most);
        Some(full - part(full / 2))
    })
}

/// A random time from zero to `most`.
fn random_below(most: Duration) -> Duration {
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).expect("the OS gives random bytes");
    let nanos = u64::try_from(most.as_nanos()).unwrap_or(u64::MAX);
    Duration::from_nanos(u64::from_le_bytes(bytes) % nanos.saturating_add(1))
}

/// A new call ID: 16 random bytes in base 64, with no padding
/// (01M4A8048J60YSVNVYF2432KE8).
///
/// ```
/// let id = riff::link::call_id();
/// assert_eq!(id.len(), 22);
/// assert!(id.bytes().all(|b| b.is_ascii_graphic()));
/// assert_ne!(id, riff::link::call_id());
/// ```
pub fn call_id() -> String {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).expect("the OS gives random bytes");
    URL_SAFE_NO_PAD.encode(bytes)
}

/// How one try ended (01M4A8041F8EK1VYDE4C9QG8N8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A reply of `riff-server`: each 2xx.
    Reply,
    /// No reply of `riff-server`: a new try can repair it.
    Fault,
    /// A reply of `riff-server` that says no: a new try gives the same.
    Refusal,
}

/// How a try with an HTTP reply ended, by its status and whether it
/// names a build. A 5xx or a 429 with no build header comes from the
/// front end, not from `riff-server` (01M3QCMJ9F1GRTRRSB4AW9TC3D).
///
/// ```
/// use reqwest::StatusCode;
/// use riff::link::{Outcome, outcome};
///
/// assert_eq!(outcome(StatusCode::OK, true), Outcome::Reply);
/// for front in [502, 503, 504, 429] {
///     let status = StatusCode::from_u16(front).unwrap();
///     assert_eq!(outcome(status, false), Outcome::Fault, "{status}");
/// }
/// assert_eq!(outcome(StatusCode::SERVICE_UNAVAILABLE, true), Outcome::Fault);
/// assert_eq!(outcome(StatusCode::BAD_GATEWAY, true), Outcome::Refusal);
/// assert_eq!(outcome(StatusCode::CONFLICT, true), Outcome::Refusal);
/// assert_eq!(outcome(StatusCode::NOT_FOUND, false), Outcome::Refusal);
/// ```
pub fn outcome(status: reqwest::StatusCode, has_build: bool) -> Outcome {
    let front = (status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS)
        && !has_build;
    if front || status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
        Outcome::Fault
    } else if status.is_success() {
        Outcome::Reply
    } else {
        Outcome::Refusal
    }
}

/// True when `base` names a loopback address: `localhost`, `127.0.0.0/8`
/// or `::1`.
///
/// ```
/// use riff::link::loopback;
///
/// assert!(loopback("http://127.0.0.1:7878"));
/// assert!(loopback("http://localhost:7878"));
/// assert!(loopback("http://[::1]:7878"));
/// assert!(!loopback("https://riff.example.com"));
/// assert!(!loopback("http://10.0.0.2:7878"));
/// ```
pub fn loopback(base: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(base) else {
        return false;
    };
    let host = url.host_str().unwrap_or_default();
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// True when the connect of `error` was refused: nothing listens on the
/// port.
pub fn refused(error: &reqwest::Error) -> bool {
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(now) = cause {
        if now
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::ConnectionRefused)
        {
            return true;
        }
        cause = now.source();
    }
    false
}

/// Where a call shows [`WAITING`]. `None` is stderr.
pub type WaitLine = Arc<dyn Fn(&str) + Send + Sync>;

/// The link of one `riff-server` in this process. See the module doc.
pub struct Link {
    base: String,
    limits: Limits,
    /// The client of the calls. A fault with no HTTP reply swaps it.
    calls: RwLock<reqwest::Client>,
    /// The count of the swaps of [`Link::calls`].
    swaps: AtomicU64,
    /// The client of the streams. It has no pool.
    streams: reqwest::Client,
    /// True once the server gave this process a reply
    /// (01M3TJWJ9914B7Z5EQJF310REK).
    replied: AtomicBool,
    /// True once a call showed [`WAITING`], until a call gets its reply.
    /// So each gap shows one line, also with many calls.
    wait_shown: AtomicBool,
    /// True once the link showed the note of another build.
    noted: AtomicBool,
}

/// The links of this process, by the URL of the server and the limits.
static LINKS: Mutex<BTreeMap<(String, Limits), Arc<Link>>> = Mutex::new(BTreeMap::new());

/// The build of the last `riff-server` that answered this process with
/// a build that it can talk to.
static SERVER_BUILD: Mutex<Option<Build>> = Mutex::new(None);

/// The one link of the server at `base` in this process, with the
/// default [`Limits`].
///
/// ```
/// use std::sync::Arc;
///
/// let one = riff::link::of("http://127.0.0.1:7878");
/// let two = riff::link::of("http://127.0.0.1:7878/");
/// assert!(Arc::ptr_eq(&one, &two));
/// assert!(!Arc::ptr_eq(&one, &riff::link::of("http://127.0.0.1:7879")));
/// ```
pub fn of(base: &str) -> Arc<Link> {
    with(base, Limits::default())
}

/// The one link of the server at `base` with `limits` in this process.
/// A test gives limits in milliseconds.
pub fn with(base: &str, limits: Limits) -> Arc<Link> {
    let base = base.trim_end_matches('/').to_owned();
    let mut links = LINKS.lock().unwrap_or_else(|poison| poison.into_inner());
    links
        .entry((base.clone(), limits))
        .or_insert_with(|| {
            Arc::new(Link {
                calls: RwLock::new(call_client(&limits)),
                swaps: AtomicU64::new(0),
                streams: stream_client(&limits),
                base,
                limits,
                replied: AtomicBool::new(false),
                wait_shown: AtomicBool::new(false),
                noted: AtomicBool::new(false),
            })
        })
        .clone()
}

/// The build of the last `riff-server` that answered this process with
/// a build that it can talk to. `None` before the first answer.
pub fn server_build() -> Option<Build> {
    SERVER_BUILD.lock().ok()?.clone()
}

/// The HTTP client of the calls (01M48RW9E8NS2FPHFHG2S10R7A).
/// `Client::new` stops the process on the same error.
fn call_client(limits: &Limits) -> reqwest::Client {
    let builder = reqwest::Client::builder()
        .connect_timeout(limits.connect)
        .http2_keep_alive_interval(PING_EVERY)
        .http2_keep_alive_timeout(PING_WAIT)
        .http2_keep_alive_while_idle(true);
    #[cfg(target_os = "linux")]
    let builder = builder.tcp_user_timeout(limits.try_wait);
    builder.build().expect("the HTTP client of the calls")
}

/// The HTTP client of the streams. `pool_max_idle_per_host(0)` keeps no
/// idle connection, so the client has no pool, also for HTTP/2
/// (01M3WN72ECF0WKR4M7M6ZYAF9J). The read limit resets at each read, so
/// a stream has no total limit (01M48RW9HNKPNZ75H9R01BG6V5).
fn stream_client(limits: &Limits) -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(0)
        .connect_timeout(limits.connect)
        .read_timeout(limits.stream_idle)
        .build()
        .expect("the HTTP client of the streams")
}

impl Link {
    /// The URL of the server, with no `/` at the end.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The time limits of the link.
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// The client of the calls now.
    pub fn client(&self) -> reqwest::Client {
        self.calls
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    /// The client of the streams.
    pub fn streams(&self) -> reqwest::Client {
        self.streams.clone()
    }

    /// Makes a new client of the calls, after a fault with no HTTP reply
    /// (01M4A8043S2ZCKRH19Z3Q8AJ1F). A dead connection can stay in the
    /// pool of the old client. Each `Api` of the server takes the new
    /// client at its next try.
    pub fn swap(&self) {
        *self
            .calls
            .write()
            .unwrap_or_else(|poison| poison.into_inner()) = call_client(&self.limits);
        self.swaps.fetch_add(1, Ordering::SeqCst);
    }

    /// The count of the new clients of the calls since the start.
    pub fn swaps(&self) -> u64 {
        self.swaps.load(Ordering::SeqCst)
    }

    /// True when the server gave this process a reply before. Then a
    /// refused connect is a restart, and the call tries again
    /// (01M3TJWJ9914B7Z5EQJF310REK).
    pub fn replied(&self) -> bool {
        self.replied.load(Ordering::SeqCst)
    }

    /// Notes an HTTP reply of the server or of its front end.
    pub fn note_reply(&self) {
        self.replied.store(true, Ordering::SeqCst);
    }

    /// Notes the end of a call: the next gap shows its line again.
    pub fn ended_wait(&self) {
        self.wait_shown.store(false, Ordering::SeqCst);
    }

    /// Waits `wait` for a new try of a call. `waited` is the time of the
    /// waits of the call before this one. It counts this wait before the
    /// check of [`Link::show_wait`], so the line shows before the wait
    /// that takes the gap past [`LINE_AFTER`]
    /// (01M3THEE5V3RFHF9QTA8MA8QDF).
    pub async fn wait(&self, waited: &mut Duration, wait: Duration, show: Option<&WaitLine>) {
        *waited += wait;
        self.show_wait(*waited, show);
        tokio::time::sleep(wait).await;
    }

    /// Shows [`WAITING`] with `show`, or on stderr, when the waits of a
    /// call add up to [`LINE_AFTER`], one time for each gap.
    fn show_wait(&self, waited: Duration, show: Option<&WaitLine>) {
        if waited < LINE_AFTER || self.wait_shown.swap(true, Ordering::SeqCst) {
            return;
        }
        let line = crate::style::styled(crate::style::DIM, WAITING);
        match show {
            Some(show) => show(&line),
            None => anstream::eprintln!("{line}"),
        }
    }

    /// Refuses a reply of a `riff-server` of a build that this `riff`
    /// cannot talk to, or that names no build
    /// (01M3MX1E65XGWDZ062PQ9YXQ5T). For a reply with no build, the error
    /// names its status and URL (01M3QCMJ9F1GRTRRSB4AW9TC3D). The error
    /// is a [`Mismatch`]. Another build that it can talk to goes on, with
    /// one note on stderr for each link (01M3MX1E8M9TKBN90P4DYKH3H8). A
    /// newer release at the server can start the update of riff by itself
    /// ([`auto_update::begin`]).
    pub fn check_build(
        &self,
        status: reqwest::StatusCode,
        headers: &reqwest::header::HeaderMap,
        url: &reqwest::Url,
    ) -> anyhow::Result<()> {
        let this = Build::this();
        let server = Build::from_header(headers.get(build::HEADER).map(|v| v.as_bytes()));
        if let Some(server) = &server {
            auto_update::begin(&self.base, server);
        }
        match server {
            Some(server) if build::compatible(&this, &server) => {
                if !server.matches(&this) && !self.noted.swap(true, Ordering::Relaxed) {
                    eprintln!("riff: {}", build::other_build(&this, &server));
                }
                if let Ok(mut seen) = SERVER_BUILD.lock() {
                    *seen = Some(server);
                }
                Ok(())
            }
            server => {
                let seen = server
                    .is_none()
                    .then(|| format!("status {status} from {url}"));
                Err(Mismatch {
                    riff: Some(this),
                    server,
                    seen,
                }
                .into())
            }
        }
    }
}

/// The whole reply to a call: the link read its body in the limit of the
/// try, so a cut in the body is a fault that gets a new try.
#[derive(Debug, Clone)]
pub struct Reply {
    pub status: reqwest::StatusCode,
    pub headers: reqwest::header::HeaderMap,
    pub body: Vec<u8>,
}

impl Reply {
    /// The value of the header `name`, when it is text.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    /// The body as JSON.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> anyhow::Result<T> {
        Ok(serde_json::from_slice(&self.body)?)
    }

    /// The body as text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The reply, or an error with its status and text when it is not a
    /// 2xx.
    pub fn error_for_status(self) -> anyhow::Result<Self> {
        if self.status.is_success() {
            return Ok(self);
        }
        anyhow::bail!("riff-server replied {}: {}", self.status, self.text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// A link of its own for each test, and the lines that it shows.
    fn link_and_lines(name: &str) -> (Arc<Link>, WaitLine, Arc<StdMutex<Vec<String>>>) {
        let link = with(&format!("http://wait.test/{name}"), Limits::default());
        let lines = Arc::new(StdMutex::new(Vec::new()));
        let into = lines.clone();
        let show: WaitLine = Arc::new(move |line: &str| into.lock().unwrap().push(line.to_owned()));
        (link, show, lines)
    }

    /// Waits as `Api::tries` does until the server is away no more:
    /// `gap` after the first fault. Gives the count of the lines.
    async fn wait_through(
        name: &str,
        gap: Duration,
        part: impl FnMut(Duration) -> Duration,
    ) -> usize {
        let (link, show, lines) = link_and_lines(name);
        let start = tokio::time::Instant::now();
        let mut waits = waits_with(&Limits::default(), part);
        let mut waited = Duration::ZERO;
        while start.elapsed() < gap {
            let wait = waits.next().unwrap();
            link.wait(&mut waited, wait, Some(&show)).await;
        }
        link.ended_wait();
        let count = lines.lock().unwrap().len();
        count
    }

    /// The lowest random parts: 125, 250, 500, 1000 ms. The first three
    /// add up to 875 ms. The line shows before the fourth wait, which
    /// takes the gap past 1 s.
    #[tokio::test(start_paused = true)]
    async fn a_gap_of_2_s_shows_one_line_with_the_lowest_waits() {
        assert_eq!(wait_through("lowest", Duration::from_secs(2), |half| half).await, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_gap_of_2_s_shows_one_line_with_the_highest_waits() {
        let none = |_| Duration::ZERO;
        assert_eq!(wait_through("highest", Duration::from_secs(2), none).await, 1);
    }

    /// Two waits of at most 250 and 500 ms show no line.
    #[tokio::test(start_paused = true)]
    async fn a_gap_of_less_than_1_s_shows_no_line() {
        let none = |_| Duration::ZERO;
        let short = Duration::from_millis(750);
        assert_eq!(wait_through("short-highest", short, none).await, 0);
        assert_eq!(wait_through("short-lowest", short, |half| half).await, 0);
    }

    #[test]
    fn the_lowest_waits_are_half_of_the_full_waits() {
        let waits: Vec<u64> = waits_with(&Limits::default(), |half| half)
            .take(5)
            .map(|wait| u64::try_from(wait.as_millis()).unwrap())
            .collect();
        assert_eq!(waits, [125, 250, 500, 1000, 2000]);
    }
}
