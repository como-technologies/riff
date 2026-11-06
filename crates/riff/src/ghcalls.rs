//! The count of the calls of `gh` that riff makes (#1549, #1582).
//!
//! # Design
//!
//! Each session, each look and each lead of one person uses the token of
//! that person. GitHub gives that token 5,000 GraphQL points an hour. When
//! the points are gone, every `gh` call that needs GraphQL fails, also the
//! report of a verify that ran to its end. So riff counts what it spends.
//!
//! Each [`Gh`](crate::pr::Gh) of the daemon sends a record of each call to
//! [`Calls`]: who called ([`Gh::by`](crate::pr::Gh::by)), what kind of call
//! it was ([`kind_of`]), and the GraphQL points that it cost
//! ([`points_of`]). [`Calls`] keeps the records of the last hour.
//! `riff daemon status` shows them ([`Shown`]), with the points that
//! GitHub says are left.
//!
//! ```mermaid
//! flowchart LR
//!     W["the loops of the daemon:<br/>wave, look, watches, sweep, usage"] --> G["Gh::output"]
//!     G --> C["Calls: the last hour"]
//!     G --> L["a reply with rateLimit:<br/>the points left"]
//!     L --> C
//!     C --> S["riff daemon status"]
//! ```
//!
//! # The other riff processes
//!
//! The daemon is not the only caller. `riff top`, `riff pr wait`, the
//! release commands and each other command of `riff` call `gh` in a
//! process of their own, with the same token, and so does each `gh` in
//! a container session. Each of them adds its calls to the [`Tally`]: a
//! folder of the daemon, one record on a line ([`Tally::record`]). A
//! process on the host writes the folder itself. A process in a container
//! has no access to it: it sends the records on the socket of its
//! session, and the daemon writes them ([`Record`],
//! [`SessionAsk::GhCalls`](crate::daemon::SessionAsk::GhCalls)). The
//! `gh` in a container is the binary of riff, which the daemon binds at
//! the path of `gh` ([`crate::gh_wrapper`]), but not in a session of an
//! image with its own `gh` there ([`Shown::own_gh`]). `riff daemon status`
//! adds the folder to the calls of the daemon. Each call keeps the ID of
//! its process, so `riff ps` shows the calls on the process
//! ([`Call::pid`], [`crate::ps`], #1610).
//!
//! ```mermaid
//! flowchart LR
//!     H["riff top, riff pr wait, riff verify, ...<br/>on the host"] -- "append" --> F["gh-calls/ in the folder of the daemon"]
//!     C["Gh::here and the gh wrapper<br/>in a container"] -- "SessionAsk::GhCalls" --> D["riff daemon"]
//!     D -- "append" --> F
//!     F --> S["riff daemon status"]
//!     M["the Calls of the daemon"] --> S
//! ```
//!
//! # The other callers
//!
//! The counter of GitHub counts every call of the token, also the calls
//! of a tool that is not riff. When the reply of a query shows the points
//! that the counter used ([`Left`]), riff takes from them the points of
//! the calls that it counted since the counter started, up to the time of
//! that reading. What is left is [`Other`]: the calls of a caller that
//! riff does not see. Both numbers are from the time of the reading, and
//! the line of the status says that time, because the reading can be old
//! (01M6QBRR7Q2NXCARHE21572GQ5, #1607).
//!
//! GitHub keeps more than one counter for one token, and each call lands
//! on one of them. A reading shows one counter, but riff counted the
//! calls of all of them. So riff can count more than the counter used. The
//! line then says that the points of riff are estimates, and not that the
//! other callers used 0 ([`Other::counted_more`]).
//!
//! ```mermaid
//! flowchart TD
//!     R["a reading of the counter:<br/>used, at the time T"] --> C["the calls of riff<br/>from the start of the counter to T"]
//!     C --> M{"counted more<br/>than used?"}
//!     M -- no --> O["Other callers: used less counted"]
//!     M -- yes --> E["Other callers: unknown, the points of riff are estimates"]
//! ```
//!
//! # The points
//!
//! GitHub counts the points of a GraphQL query, not the calls: it adds
//! the requests that the query could need (1 for the list, and for each
//! item of the list 1 for each list inside it), divides by 100, rounds to
//! the nearest whole number, and charges 1 at the least. A `gh` call has
//! no header that says its cost, so riff takes the cost from a table of
//! the queries that `gh` sends. The table comes from a measurement
//! (01M6QBRR4GDSNZ9YPTDR3AJ0KA, #1607): the `cost` that GitHub gives for
//! each query in `rateLimit(dryRun: true) { cost }`, recorded in
//! `ghcalls/costs.txt`, with the drop of the real counter as a check. A
//! test compares [`points_of`] with that record. `gh` asks for 100 items
//! in a page, or for the rest of `--limit` (30 when the call has none),
//! so the cost follows `--limit` and the fields:
//!
//! | Call | Points for each page |
//! |---|---|
//! | `gh pr list --json statusCheckRollup` | 2 for a page of 75 or more pull requests, else 1 |
//! | `gh pr list` with no list inside, `gh pr list --head BRANCH` | 1 |
//! | `gh issue list --json comments,labels` | 2 for a page of 75 or more issues, else 1 |
//! | `gh issue list` with no `comments`, `gh issue list` alone | 1 |
//! | `gh issue view`, `gh pr view` | 1 |
//! | `gh pr checks` | 2 |
//! | a write: `pr create`, `pr merge`, a comment, `issue close` | 2 |
//! | `gh api graphql` | the `cost` in the reply, else 1 |
//! | `gh api graphql` that asks for `rateLimit` and nothing else | 0: it says `cost: 1`, and the counter does not move |
//! | `gh api PATH` of REST, `gh run`, `gh auth` | 0, the REST limit counts them |
//! | `gh api -i -H 'If-None-Match: ETAG' PATH`, the conditional read of the look ([`crate::changes`], #1566) | 0, also when `gh` shows its `304` as a failed call |
//!
//! The reply of a query that asks for `rateLimit` also says how many
//! points are left and when they come back. riff keeps the last one
//! ([`Left`]). GitHub keeps more than one counter for one token: two
//! reads a second apart can give different numbers and different times
//! of reset. So the number is a guide, not a promise.
//!
//! # When the limit runs out
//!
//! When a call of `gh` fails because a limit of GitHub is out, riff does
//! not try again each minute: each try costs a call, and the limit stays
//! out until its reset (#1557). [`Calls`] keeps a [`Wait`] for the
//! resource: GraphQL and REST have a limit each ([`Resource`]). Until the
//! reset the `gh` of the daemon makes no call of that resource, and the
//! call fails at once with [`Limited`]. The loops of the daemon look at the
//! wait first: the wave waits for the reset and tells the lead one time,
//! the look and the watches wait, and the end step of a verify keeps its
//! result (see [`crate::wave`]).
//!
//! GitHub keeps more than one counter for one token. A read can answer
//! from a counter that still has points while the call failed on the
//! counter that is out (#1581). So the reset comes from the call that
//! failed, and not from any read ([`Gh`](crate::pr::Gh)):
//!
//! 1. A header `x-ratelimit-reset` in the answer of the failed call
//!    ([`header_reset`]), as `gh api -i` and `GH_DEBUG=api` show it.
//! 2. A secondary limit ([`is_secondary`]) has no reset: riff waits
//!    [`SECONDARY_WAIT_MS`].
//! 3. A counter that shows no points: the reply of `gh api rate_limit`
//!    ([`counter_in`]), or for GraphQL a few reads of
//!    `rateLimit { remaining resetAt }` ([`probe_in`]), as each read can
//!    land on another counter.
//! 4. When no counter shows no points, riff waits for the latest reset
//!    that it saw: the end of the hour of that counter. The [`Wait`] says
//!    that it is a guess ([`Wait::guessed`]), and so does the message to
//!    the lead.
//!
//! The key of the message to the lead names the outage, not the reset
//! time: the resource and the hour of the counter ([`Wait::key`]).
//!
//! ```mermaid
//! flowchart TD
//!     C["a call of gh"] --> W{"a wait for its resource?"}
//!     W -- yes --> R["fails at once: Limited, no call"]
//!     W -- no --> G["gh"]
//!     G -- "fails: rate limit" --> H{"a reset in its answer,<br/>or a secondary limit?"}
//!     H -- yes --> K["Calls keeps the Wait"]
//!     H -- no --> A{"a counter with no points:<br/>gh api rate_limit, rateLimit reads?"}
//!     A -- yes --> K
//!     A -- no --> E["the end of the hour of the counter<br/>that riff saw: a guess"]
//!     E --> K
//!     K --> S["riff daemon status shows it"]
//! ```

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use serde::{Deserialize, Serialize};

use crate::text;

mod tally;

pub use tally::{CALLERS, MAX_RECORDS, Record, SESSION, Tally, clean};

/// How long a record stays: the count of `riff daemon status`.
pub const WINDOW_MS: i64 = 3_600_000;

/// The points of the GraphQL counter of a token, when a reading does not
/// say its limit.
pub const GRAPHQL_LIMIT: u64 = 5_000;

/// The target for the looks and the watches of two waves that have no
/// work: at most this many GraphQL points an hour (#1549).
pub const IDLE_POINTS_AN_HOUR: u32 = 300;

/// The target for the looks, the watches and the sweeps of one wave with
/// 8 sessions: at most this many GraphQL points an hour (#1549).
pub const BUSY_POINTS_AN_HOUR: u32 = 1_500;

/// The caller of a `gh` that has no name.
pub const OTHER: &str = "other";

/// How long riff waits after the reset that GitHub names: the reset is in
/// whole seconds, and a call in the same second can still fail.
pub const RESET_SLACK_MS: i64 = 1_000;

/// How long riff waits for a limit when it cannot read any counter.
pub const UNREAD_WAIT_MS: i64 = 300_000;

/// How long riff waits for a secondary limit of GitHub: GitHub refuses the
/// calls for a short time, and has no reset for it.
pub const SECONDARY_WAIT_MS: i64 = 60_000;

/// How many times riff reads `rateLimit` of GraphQL to find the counter
/// that is out: each read can land on another counter (#1581).
pub const PROBES: usize = 4;

/// The query of a read of the GraphQL counter.
pub const PROBE_QUERY: &str = "{ rateLimit { limit remaining resetAt } }";

/// One call of `gh`: a line of the [`Tally`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    /// When the call ended, in milliseconds since the epoch.
    pub at_ms: i64,
    /// Who called ([`Row::caller`]).
    pub caller: String,
    /// The kind of call ([`kind_of`]).
    pub kind: String,
    /// The GraphQL points that it cost ([`points_of`]).
    pub points: u32,
    /// The process that called: its ID on the host, or in its container
    /// when [`Call::session`] names one (#1610). `riff ps` shows the calls
    /// on that process. A line from an older riff has 0.
    #[serde(default)]
    pub pid: u32,
    /// The container of the process, or empty for a process on the host.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub session: String,
}

/// The records of the calls of the last hour, and the last reading of the
/// GraphQL limit.
#[derive(Debug, Default)]
pub struct Calls {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    calls: VecDeque<Call>,
    left: Option<Left>,
    /// The wait of each resource that waits.
    waits: BTreeMap<Resource, Wait>,
}

/// The commands of `gh` that read nothing of GitHub: they use no limit.
const NO_LIMIT: &[&str] = &[
    "auth",
    "alias",
    "completion",
    "config",
    "extension",
    "help",
    "version",
];

/// A limit of GitHub. GraphQL and REST count apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Resource {
    /// The points of GraphQL: `gh pr`, `gh issue`, `gh api graphql`.
    Graphql,
    /// The requests of REST: `gh api PATH`, `gh run`.
    Rest,
}

impl Resource {
    /// The resource that the call `gh ARGS` uses, and `None` for a call
    /// that uses no limit: `gh auth` reads the token, and `gh --version`,
    /// `gh help` and `gh config` read nothing of GitHub.
    ///
    /// ```
    /// use riff::ghcalls::Resource;
    ///
    /// assert_eq!(Resource::of(&["pr", "view", "40"]), Some(Resource::Graphql));
    /// assert_eq!(Resource::of(&["api", "graphql", "-f", "query=x"]), Some(Resource::Graphql));
    /// assert_eq!(Resource::of(&["api", "repos/o/r"]), Some(Resource::Rest));
    /// assert_eq!(Resource::of(&["run", "view", "1"]), Some(Resource::Rest));
    /// assert_eq!(Resource::of(&["auth", "token"]), None);
    /// assert_eq!(Resource::of(&["--version"]), None);
    /// assert_eq!(Resource::of(&["config", "get", "editor"]), None);
    /// assert_eq!(Resource::of(&[]), None);
    /// ```
    pub fn of(args: &[&str]) -> Option<Self> {
        let first = args.first()?;
        if first.starts_with('-') || NO_LIMIT.contains(first) {
            return None;
        }
        let kind = kind_of(args);
        match kind.as_str() {
            "api graphql" | "release list" | "release view" | "repo view" | "repo clone" => {
                Some(Self::Graphql)
            }
            other if other.starts_with("pr ") || other.starts_with("issue ") => Some(Self::Graphql),
            _ => Some(Self::Rest),
        }
    }

    /// The name of the resource in a text.
    pub fn name(self) -> &'static str {
        match self {
            Self::Graphql => "GraphQL",
            Self::Rest => "REST",
        }
    }

    /// The name of the resource in the reply of `gh api rate_limit`.
    fn key(self) -> &'static str {
        match self {
            Self::Graphql => "graphql",
            Self::Rest => "core",
        }
    }
}

/// A resource that waits for the reset of its limit (#1557).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wait {
    /// The resource.
    pub resource: Resource,
    /// When the limit starts again, in milliseconds since the epoch.
    pub reset_ms: i64,
    /// True when riff found no reset of the counter that is out: the wait
    /// is the end of the hour of a counter that it saw (#1581).
    #[serde(default)]
    pub guessed: bool,
}

impl Wait {
    /// A wait for a reset that riff read.
    pub fn until(resource: Resource, reset_ms: i64) -> Self {
        Self {
            resource,
            reset_ms,
            guessed: false,
        }
    }

    /// When riff may call again: [`RESET_SLACK_MS`] after the reset.
    pub fn until_ms(&self) -> i64 {
        self.reset_ms + RESET_SLACK_MS
    }

    /// The key of the post that tells the lead of this wait. It names the
    /// outage: the resource and the hour of the counter. The counters of
    /// one token and the waits of one outage give the same key, and each
    /// host of one person reads the same hour, so the lead gets one
    /// message (01M5TKG2K1AYK1KE7C2AA2W9DS, #1581).
    ///
    /// ```
    /// use riff::ghcalls::{Resource, Wait};
    ///
    /// let wait = Wait::until(Resource::Graphql, 1_793_718_035_000);
    /// assert_eq!(wait.key(), "github-limit-graphql-2026110315");
    /// // Two counters of one token, 34 seconds apart: the same outage.
    /// let other = Wait::until(Resource::Graphql, 1_793_718_069_000);
    /// assert_eq!(other.key(), wait.key());
    /// // The next hour is a new outage, and REST has its own.
    /// assert_ne!(Wait::until(Resource::Graphql, 1_793_721_635_000).key(), wait.key());
    /// assert_ne!(Wait::until(Resource::Rest, 1_793_718_035_000).key(), wait.key());
    /// ```
    pub fn key(&self) -> String {
        // Half an hour on: the resets of the counters of one token are
        // some minutes apart at the most, and they share the nearest hour.
        let hour = chrono::DateTime::from_timestamp_millis(self.reset_ms + 1_800_000)
            .map_or_else(String::new, |at| at.format("%Y%m%d%H").to_string());
        format!("github-limit-{}-{hour}", self.resource.key())
    }
}

/// The error of a call of `gh` that riff did not make: the limit of its
/// resource is out until the reset ([`Wait`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limited(pub Wait);

impl fmt::Display for Limited {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&text::gh_limited(&self.0))
    }
}

impl std::error::Error for Limited {}

/// The error of a command that stops on a limit of GitHub: a call failed
/// because the limit of its resource is out, and the command makes no new
/// try ([`Gh::stopping_on_limit`](crate::pr::Gh::stopping_on_limit),
/// 01M6QB459TJVK6CS9FHWH6EVK2). The [`Wait`] is the reset of the counter
/// that failed. The release commands use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitOut(pub Wait);

impl fmt::Display for LimitOut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&text::gh_limit_out(&self.0, crate::daemon::now_ms()))
    }
}

impl std::error::Error for LimitOut {}

/// True when `stderr` of a failed `gh` says that a limit of GitHub is out:
/// the limit of the primary counter, a secondary limit, or HTTP 429.
///
/// ```
/// use riff::ghcalls::is_limit;
///
/// assert!(is_limit("GraphQL: API rate limit already exceeded for user ID 1."));
/// assert!(is_limit("gh: API rate limit exceeded for user ID 1. (HTTP 403)"));
/// assert!(is_limit("You have exceeded a secondary rate limit."));
/// assert!(is_limit("gh: Too Many Requests (HTTP 429)"));
/// assert!(!is_limit("gh: Bad Gateway (HTTP 502)"));
/// ```
pub fn is_limit(stderr: &str) -> bool {
    stderr.to_ascii_lowercase().contains("rate limit") || stderr.contains("HTTP 429")
}

/// True when `error` is a call that failed on a limit of GitHub: riff made
/// no call ([`Limited`]), a command stopped on the limit ([`LimitOut`]),
/// or `gh` said that the limit is out ([`is_limit`]).
pub fn is_limit_error(error: &anyhow::Error) -> bool {
    error.downcast_ref::<Limited>().is_some()
        || error.downcast_ref::<LimitOut>().is_some()
        || is_limit(&format!("{error:#}"))
}

/// What one reading shows of a counter of GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counter {
    /// The points or requests left in the counter that answered.
    pub remaining: u64,
    /// When that counter starts again, in milliseconds since the epoch.
    pub reset_ms: i64,
}

/// The counter of `resource` in the reply `stdout` of `gh api rate_limit`.
/// The reply comes from one counter of the token: it can show points left
/// while another counter is out (#1581).
///
/// ```
/// use riff::ghcalls::{Counter, Resource, counter_in};
///
/// let reply = r#"{"resources":{
///     "core":{"limit":5000,"remaining":4000,"reset":1793718035},
///     "graphql":{"limit":5000,"remaining":0,"reset":1793718035}}}"#;
/// let out = Counter { remaining: 0, reset_ms: 1_793_718_035_000 };
/// assert_eq!(counter_in(reply, Resource::Graphql), Some(out));
/// assert_eq!(counter_in(reply, Resource::Rest).unwrap().remaining, 4000);
/// assert_eq!(counter_in("not json", Resource::Rest), None);
/// ```
pub fn counter_in(stdout: &str, resource: Resource) -> Option<Counter> {
    let reply: serde_json::Value = serde_json::from_str(stdout).ok()?;
    let counter = &reply["resources"][resource.key()];
    Some(Counter {
        remaining: counter["remaining"].as_u64()?,
        reset_ms: counter["reset"].as_i64()?.checked_mul(1000)?,
    })
}

/// The counter in a reply of GraphQL that asked for
/// `rateLimit { remaining resetAt }` ([`PROBE_QUERY`]). Each read can
/// answer from another counter of the token (#1581).
///
/// ```
/// use riff::ghcalls::{Counter, probe_in};
///
/// let reply = r#"{"data":{"rateLimit":{"remaining":0,"resetAt":"2026-11-03T15:00:35Z"}}}"#;
/// let out = Counter { remaining: 0, reset_ms: 1_793_718_035_000 };
/// assert_eq!(probe_in(reply), Some(out));
/// assert_eq!(probe_in(r#"{"data":{}}"#), None);
/// ```
pub fn probe_in(stdout: &str) -> Option<Counter> {
    let left = left_in(stdout, 0)?;
    let at = chrono::DateTime::parse_from_rfc3339(&left.reset_at).ok()?;
    Some(Counter {
        remaining: left.remaining,
        reset_ms: at.timestamp_millis(),
    })
}

/// The reset in the header `x-ratelimit-reset` of the answer of a call, in
/// milliseconds since the epoch. `gh api -i` writes the headers to stdout,
/// and `GH_DEBUG=api` writes them to stderr. A header `x-ratelimit-remaining`
/// above zero says that the counter is not out: there is no reset then.
///
/// ```
/// use riff::ghcalls::header_reset;
///
/// let debug = "< HTTP/2.0 403 Forbidden\n< X-Ratelimit-Remaining: 0\n< X-Ratelimit-Reset: 1793718015\n";
/// assert_eq!(header_reset(debug), Some(1_793_718_015_000));
/// assert_eq!(header_reset("x-ratelimit-reset: 1793718015\r\n"), Some(1_793_718_015_000));
/// assert_eq!(header_reset("x-ratelimit-remaining: 3204\nx-ratelimit-reset: 1793718049\n"), None);
/// assert_eq!(header_reset("GraphQL: API rate limit already exceeded"), None);
/// ```
pub fn header_reset(text: &str) -> Option<i64> {
    let header = |name: &str| {
        text.lines().find_map(|line| {
            let line = line.trim_start_matches(['<', ' ']).to_ascii_lowercase();
            let value = line.strip_prefix(name)?.strip_prefix(':')?;
            value.trim().parse::<i64>().ok()
        })
    };
    if header("x-ratelimit-remaining").is_some_and(|left| left > 0) {
        return None;
    }
    header("x-ratelimit-reset")?.checked_mul(1000)
}

/// True when `stderr` of a failed `gh` says that the limit is a secondary
/// limit of GitHub. It has no reset: it ends after a short time.
///
/// ```
/// use riff::ghcalls::is_secondary;
///
/// assert!(is_secondary("You have exceeded a secondary rate limit."));
/// assert!(!is_secondary("GraphQL: API rate limit already exceeded for user ID 1."));
/// ```
pub fn is_secondary(stderr: &str) -> bool {
    stderr.to_ascii_lowercase().contains("secondary rate limit")
}

/// The GraphQL points that GitHub said are left, in the reply of a query
/// that asked for `rateLimit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Left {
    /// The points left in the counter that answered.
    pub remaining: u64,
    /// When that counter starts again, as GitHub writes it:
    /// `2026-11-03T15:00:35Z`.
    pub reset_at: String,
    /// When riff read it, in milliseconds since the epoch.
    pub read_ms: i64,
    /// The points of the counter in one hour ([`GRAPHQL_LIMIT`] when the
    /// reply does not say it).
    #[serde(default = "graphql_limit")]
    pub limit: u64,
}

fn graphql_limit() -> u64 {
    GRAPHQL_LIMIT
}

impl Left {
    /// When the counter that answered started, in milliseconds since the
    /// epoch: one hour before its reset.
    ///
    /// ```
    /// use riff::ghcalls::Left;
    ///
    /// let left = Left { remaining: 10, reset_at: "2026-11-03T15:00:35Z".into(), read_ms: 0, limit: 5000 };
    /// assert_eq!(left.reset_ms(), Some(1_793_718_035_000));
    /// assert_eq!(left.started_ms(), Some(1_793_714_435_000));
    /// let bad = Left { reset_at: "soon".into(), ..left };
    /// assert_eq!(bad.started_ms(), None);
    /// ```
    pub fn started_ms(&self) -> Option<i64> {
        Some(self.reset_ms()? - WINDOW_MS)
    }

    /// When the counter that answered starts again, in milliseconds
    /// since the epoch.
    pub fn reset_ms(&self) -> Option<i64> {
        chrono::DateTime::parse_from_rfc3339(&self.reset_at)
            .ok()
            .map(|at| at.timestamp_millis())
    }
}

/// The GraphQL points of the counter that riff did not count: the calls
/// of a caller that no riff process sees (#1582).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Other {
    /// The points of the counter that the other callers used: what the
    /// counter used, less what riff counted, and 0 when riff counted more
    /// ([`Other::counted_more`]).
    pub points: u64,
    /// The points that the counter had used when riff read it.
    pub used: u64,
    /// The points of the calls that riff counted from the start of the
    /// counter to that reading.
    pub counted: u64,
    /// When the counter started, in milliseconds since the epoch.
    pub since_ms: i64,
    /// When riff read the counter, in milliseconds since the epoch.
    pub read_ms: i64,
}

impl Other {
    /// True when riff counted more points than the counter used, both at
    /// the time of the reading (#1607). The points of the other callers
    /// are then not known: the counter that answered is one of several,
    /// or the points of riff are too high. `points` is 0, and it says
    /// nothing.
    ///
    /// ```
    /// use riff::ghcalls::Other;
    ///
    /// let other = Other { points: 0, used: 332, counted: 1_201, since_ms: 0, read_ms: 8 * 60_000 };
    /// assert!(other.counted_more());
    /// let same = Other { used: 1_201, ..other };
    /// assert!(!same.counted_more());
    /// ```
    pub fn counted_more(&self) -> bool {
        self.counted > self.used
    }

    /// The points that the other callers used in a minute, on average,
    /// from the start of the counter to the reading. At least one minute
    /// counts.
    ///
    /// ```
    /// use riff::ghcalls::Other;
    ///
    /// let other = Other { points: 2_570, used: 2_778, counted: 208, since_ms: 0, read_ms: 28 * 60_000 };
    /// assert_eq!(other.a_minute(), 91);
    /// let new = Other { points: 30, since_ms: 0, read_ms: 5_000, ..other };
    /// assert_eq!(new.a_minute(), 30);
    /// ```
    pub fn a_minute(&self) -> u64 {
        let minutes = u64::try_from((self.read_ms - self.since_ms) / 60_000)
            .unwrap_or(0)
            .max(1);
        self.points / minutes
    }
}

/// The calls of one caller and one kind in the last hour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    /// Who called: `wave`, `look`, `watches`, `sweep`, `usage`, the name of
    /// a command of `riff` (`top`, `pr wait`, ...), `session` for a `gh`
    /// in a container, or [`OTHER`].
    pub caller: String,
    /// The kind of call ([`kind_of`]).
    pub kind: String,
    /// How many calls.
    pub calls: u32,
    /// Their GraphQL points ([`points_of`]).
    pub points: u32,
}

/// What `riff daemon status` shows of the use of GitHub.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shown {
    /// The calls of the last hour, the most points first.
    pub rows: Vec<Row>,
    /// The last reading of the GraphQL limit, when there is one.
    pub left: Option<Left>,
    /// The resources that wait for the reset of their limit (#1557).
    pub waits: Vec<Wait>,
    /// The points of the counter that no riff process used (#1582). It
    /// needs a reading of the counter that has not ended.
    #[serde(default)]
    pub other: Option<Other>,
    /// The session images with their own `gh` at the path of the wrapper:
    /// the daemon binds no wrapper in their sessions, so it counts none of
    /// their `gh` calls (#1595). An old daemon gives none.
    #[serde(default)]
    pub own_gh: Vec<String>,
}

impl Shown {
    /// The calls and the points of all rows.
    ///
    /// ```
    /// use riff::ghcalls::{Row, Shown};
    ///
    /// let row = |calls, points| Row { caller: "look".into(), kind: "pr list".into(), calls, points };
    /// let shown = Shown { rows: vec![row(3, 6), row(1, 2)], ..Shown::default() };
    /// assert_eq!(shown.total(), (4, 8));
    /// ```
    pub fn total(&self) -> (u32, u32) {
        self.rows
            .iter()
            .fold((0, 0), |(c, p), row| (c + row.calls, p + row.points))
    }
}

impl Calls {
    /// The records of this process. A `Gh` of the daemon sends its calls
    /// here ([`Gh::for_repo`](crate::pr::Gh::for_repo)).
    pub fn global() -> &'static Arc<Calls> {
        static GLOBAL: OnceLock<Arc<Calls>> = OnceLock::new();
        GLOBAL.get_or_init(Arc::default)
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records a call at `now_ms`. It forgets the records older than
    /// [`WINDOW_MS`].
    ///
    /// ```
    /// use riff::ghcalls::Calls;
    ///
    /// let calls = Calls::default();
    /// calls.record(0, "look", "pr list", 2);
    /// calls.record(1_000, "look", "pr list", 2);
    /// calls.record(1_000, "sweep", "pr list", 1);
    /// let shown = calls.shown(2_000);
    /// assert_eq!(shown.rows[0].points, 4);
    /// assert_eq!(shown.rows[0].calls, 2);
    /// assert_eq!(shown.total(), (3, 5));
    /// // An hour later the first record is gone.
    /// assert_eq!(calls.shown(3_600_500).total(), (2, 3));
    /// ```
    pub fn record(&self, now_ms: i64, caller: &str, kind: &str, points: u32) {
        let mut inner = self.lock();
        forget(&mut inner.calls, now_ms);
        inner.calls.push_back(Call {
            at_ms: now_ms,
            caller: caller.to_owned(),
            kind: kind.to_owned(),
            points,
            pid: std::process::id(),
            session: String::new(),
        });
    }

    /// The calls of this process in the last hour at `now_ms`, oldest
    /// first (#1610).
    ///
    /// ```
    /// use riff::ghcalls::Calls;
    ///
    /// let calls = Calls::default();
    /// calls.record(0, "look", "pr list", 2);
    /// calls.record(4_000_000, "look", "pr list", 2);
    /// let recent = calls.recent(4_000_000);
    /// assert_eq!(recent.len(), 1);
    /// assert_eq!(recent[0].pid, std::process::id());
    /// ```
    pub fn recent(&self, now_ms: i64) -> Vec<Call> {
        let mut inner = self.lock();
        forget(&mut inner.calls, now_ms);
        inner.calls.iter().cloned().collect()
    }

    /// Keeps the reading of the limit that a reply gave.
    pub fn keep_left(&self, left: Left) {
        self.lock().left = Some(left);
    }

    /// The calls of the last hour at `now_ms`, one row for each caller and
    /// kind, the most points first, then the most calls.
    pub fn shown(&self, now_ms: i64) -> Shown {
        self.shown_with(now_ms, Vec::new())
    }

    /// [`Calls::shown`] with the calls of the other riff processes
    /// ([`Tally::read`]) added to the calls of this process (#1582).
    ///
    /// ```
    /// use riff::ghcalls::{Calls, Left, Tally};
    ///
    /// let dir = tempfile::tempdir()?;
    /// let calls = Calls::default();
    /// calls.record(10_000, "look", "pr list", 2);
    /// // A `riff top` on the host adds its calls to the folder.
    /// let tally = Tally::in_folder(dir.path());
    /// tally.record_at(20_000, "top", "pr list", 2);
    /// tally.record_at(30_000, "pr wait", "pr view", 1);
    /// let shown = calls.shown_with(40_000, tally.read(40_000));
    /// assert_eq!(shown.total(), (3, 5));
    /// assert!(shown.rows.iter().any(|row| row.caller == "pr wait"));
    ///
    /// // The counter used 100 points since it started: riff counted 5.
    /// let left = Left {
    ///     remaining: 4_900,
    ///     reset_at: "1970-01-01T01:00:00Z".into(),
    ///     read_ms: 40_000,
    ///     limit: 5_000,
    /// };
    /// calls.keep_left(left);
    /// let other = calls.shown_with(41_000, tally.read(41_000)).other.unwrap();
    /// assert_eq!((other.used, other.counted, other.points), (100, 5, 95));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn shown_with(&self, now_ms: i64, others: Vec<Call>) -> Shown {
        let mut inner = self.lock();
        forget(&mut inner.calls, now_ms);
        let calls: Vec<&Call> = inner.calls.iter().chain(others.iter()).collect();
        let mut sums: BTreeMap<(&str, &str), (u32, u32)> = BTreeMap::new();
        for call in &calls {
            let sum = sums.entry((&call.caller, &call.kind)).or_default();
            sum.0 += 1;
            sum.1 += call.points;
        }
        let mut rows: Vec<Row> = sums
            .into_iter()
            .map(|((caller, kind), (calls, points))| Row {
                caller: caller.to_owned(),
                kind: kind.to_owned(),
                calls,
                points,
            })
            .collect();
        rows.sort_by_key(|row| std::cmp::Reverse((row.points, row.calls)));
        let other = inner
            .left
            .as_ref()
            .and_then(|left| other_of(left, &calls, now_ms));
        Shown {
            rows,
            left: inner.left.clone(),
            waits: inner.waits(now_ms),
            other,
            own_gh: Vec::new(),
        }
    }

    /// Starts `wait` for the reset of its resource. It gives `true` when
    /// this starts the wait, and `false` when the resource waits already:
    /// a call that was on its way when the limit ran out fails later, and
    /// it starts no second wait.
    ///
    /// ```
    /// use riff::ghcalls::{Calls, Resource, Wait};
    ///
    /// let calls = Calls::default();
    /// assert!(calls.start_wait(Wait::until(Resource::Graphql, 60_000), 0));
    /// assert!(!calls.start_wait(Wait::until(Resource::Graphql, 90_000), 1_000));
    /// // The wait ends one second after the reset. REST has no wait.
    /// assert!(calls.waiting(Resource::Graphql, 60_999).is_some());
    /// assert!(calls.waiting(Resource::Graphql, 61_000).is_none());
    /// assert!(calls.waiting(Resource::Rest, 0).is_none());
    /// // The wait is over: a new limit starts a new wait.
    /// assert!(calls.start_wait(Wait::until(Resource::Graphql, 7_260_000), 3_700_000));
    /// ```
    pub fn start_wait(&self, wait: Wait, now_ms: i64) -> bool {
        let mut inner = self.lock();
        if inner.waiting(wait.resource, now_ms).is_some() {
            return false;
        }
        inner.waits.insert(wait.resource, wait);
        true
    }

    /// The wait of `resource` at `now_ms`, when it has one.
    pub fn waiting(&self, resource: Resource, now_ms: i64) -> Option<Wait> {
        self.lock().waiting(resource, now_ms)
    }

    /// The wait that ends last at `now_ms`, when a resource waits. The look
    /// of a wave needs all the resources, so it waits for the last reset.
    pub fn limit_wait(&self, now_ms: i64) -> Option<Wait> {
        self.lock()
            .waits(now_ms)
            .into_iter()
            .max_by_key(Wait::until_ms)
    }
}

impl Inner {
    fn waiting(&self, resource: Resource, now_ms: i64) -> Option<Wait> {
        let wait = *self.waits.get(&resource)?;
        (wait.until_ms() > now_ms).then_some(wait)
    }

    fn waits(&self, now_ms: i64) -> Vec<Wait> {
        self.waits
            .keys()
            .filter_map(|resource| self.waiting(*resource, now_ms))
            .collect()
    }
}

/// The points of the counter of `left` that none of `calls` used, when the
/// counter has not ended at `now_ms` (#1582). The counter used `limit`
/// less `remaining` points when riff read it. The calls that riff counted
/// from the start of the counter to that reading used some of them.
fn other_of(left: &Left, calls: &[&Call], now_ms: i64) -> Option<Other> {
    let since_ms = left.started_ms()?;
    if now_ms >= left.reset_ms()? {
        return None;
    }
    let used = left.limit.saturating_sub(left.remaining);
    let counted: u64 = calls
        .iter()
        .filter(|call| call.at_ms >= since_ms && call.at_ms <= left.read_ms)
        .map(|call| u64::from(call.points))
        .sum();
    Some(Other {
        points: used.saturating_sub(counted),
        used,
        counted,
        since_ms,
        read_ms: left.read_ms,
    })
}

/// Drops the records that are older than [`WINDOW_MS`] at `now_ms`.
fn forget(calls: &mut VecDeque<Call>, now_ms: i64) {
    while calls
        .front()
        .is_some_and(|call| now_ms - call.at_ms >= WINDOW_MS)
    {
        calls.pop_front();
    }
}

/// The options of `gh api` that take a value.
const API_VALUE_FLAGS: &[&str] = &[
    "-X",
    "--method",
    "-H",
    "--header",
    "-f",
    "--raw-field",
    "-F",
    "--field",
    "-q",
    "--jq",
    "-t",
    "--template",
    "--input",
    "--cache",
    "--hostname",
    "-p",
    "--preview",
];

/// The kind of a call of `gh`: its first two words, and for `gh api` the
/// path of REST with no repository and no number. Two calls of one kind
/// cost the same.
///
/// ```
/// use riff::ghcalls::kind_of;
///
/// assert_eq!(kind_of(&["pr", "list", "--repo", "o/r", "--json", "number"]), "pr list");
/// assert_eq!(kind_of(&["issue", "view", "12", "--json", "state"]), "issue view");
/// assert_eq!(kind_of(&["api", "graphql", "-f", "query=x"]), "api graphql");
/// assert_eq!(kind_of(&["api", "repos/o/r", "--jq", ".default_branch"]), "api repo");
/// assert_eq!(
///     kind_of(&["api", "repos/o/r/commits/1a2b3c4d5e6f/check-runs?check_name=Gate"]),
///     "api commits/_/check-runs",
/// );
/// assert_eq!(kind_of(&["api", "-X", "POST", "repos/o/r/issues/12/comments"]), "api issues/_/comments");
/// assert_eq!(kind_of(&["api", "-i", "-H", "Accept: x", "user"]), "api user");
/// assert_eq!(kind_of(&["auth", "token"]), "auth token");
/// ```
pub fn kind_of(args: &[&str]) -> String {
    match args {
        ["api", rest @ ..] => api_kind(rest),
        [first, second, ..] if !second.starts_with('-') => format!("{first} {second}"),
        [first, ..] => (*first).to_owned(),
        [] => "none".to_owned(),
    }
}

/// The kind of `gh api ARGS`.
fn api_kind(args: &[&str]) -> String {
    let mut path = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if API_VALUE_FLAGS.contains(arg) {
            args.next();
        } else if !arg.starts_with('-') {
            path = Some(*arg);
            break;
        }
    }
    let Some(path) = path else {
        return "api".to_owned();
    };
    let path = path.split('?').next().unwrap_or(path).trim_matches('/');
    if path == "graphql" {
        return "api graphql".to_owned();
    }
    let mut parts: Vec<&str> = path.split('/').collect();
    if parts.first() == Some(&"repos") && parts.len() >= 3 {
        parts.drain(..3);
    }
    let shown: Vec<String> = parts
        .iter()
        .take(3)
        .map(|part| {
            let id = part.len() >= 7 && part.bytes().all(|b| b.is_ascii_hexdigit());
            if id || part.bytes().all(|b| b.is_ascii_digit()) {
                "_".to_owned()
            } else {
                (*part).to_owned()
            }
        })
        .collect();
    match shown.is_empty() {
        true => "api repo".to_owned(),
        false => format!("api {}", shown.join("/")),
    }
}

/// The number of pages of 100 that a list of `stdout` took: `gh` asks for
/// 100 items in each query.
fn pages(stdout: &str) -> u32 {
    let items = serde_json::from_str::<Vec<serde::de::IgnoredAny>>(stdout).map_or(0, |v| v.len());
    u32::try_from(items.div_ceil(PAGE))
        .unwrap_or(u32::MAX)
        .max(1)
}

/// The items that `gh` asks for in one query of a list.
const PAGE: usize = 100;

/// The items that `gh pr list` and `gh issue list` read when the call has
/// no `--limit`.
const DEFAULT_LIMIT: usize = 30;

/// The fields of `--json` that open a list inside each item, with the
/// lists that each of them opens. The cost of a page counts one request
/// for each of them in each item. `statusCheckRollup` is the commits of
/// the pull request and the checks of the last one.
const LISTS: &[(&str, usize)] = &[
    ("statusCheckRollup", 2),
    ("assignees", 1),
    ("comments", 1),
    ("files", 1),
    ("labels", 1),
    ("latestReviews", 1),
    ("projectItems", 1),
    ("reviewRequests", 1),
    ("reviews", 1),
];

/// The value of the option `names` in `args`, as `--limit 5`,
/// `-L 5` or `--limit=5`.
fn option_of<'a>(args: &[&'a str], names: &[&str]) -> Option<&'a str> {
    args.iter().enumerate().find_map(|(at, arg)| {
        if names.contains(arg) {
            return args.get(at + 1).copied();
        }
        let (name, value) = arg.split_once('=')?;
        names.contains(&name).then_some(value)
    })
}

/// The lists inside each item that the call `gh KIND ARGS` of a list
/// asks for. A list of issues without `--json` asks for the labels.
fn lists_in(kind: &str, args: &[&str]) -> usize {
    match option_of(args, &["--json"]) {
        Some(fields) => fields
            .split(',')
            .filter_map(|field| LISTS.iter().find(|(name, _)| *name == field))
            .map(|(_, lists)| lists)
            .sum(),
        None => usize::from(kind == "issue list"),
    }
}

/// The points of the pages of the list that `gh KIND ARGS` read, with the
/// reply `stdout` (#1607). A page of `first` items costs the requests of
/// the query, 1 for the list and the lists inside each item for each
/// item, divided by 100 and rounded to the nearest whole number, and 1 at
/// the least. `gh` asks for 100 items in a page and for the rest of
/// `--limit` in the last one.
fn list_points(kind: &str, args: &[&str], stdout: &str) -> u32 {
    let limit = option_of(args, &["--limit", "-L"])
        .and_then(|limit| limit.parse().ok())
        .unwrap_or(DEFAULT_LIMIT);
    let lists = lists_in(kind, args);
    (0..pages(stdout) as usize)
        .map(|page| {
            let first = limit.saturating_sub(page * PAGE).clamp(1, PAGE);
            let requests = 1 + first * lists;
            u32::try_from((requests + 50) / 100)
                .unwrap_or(u32::MAX)
                .max(1)
        })
        .sum()
}

/// The `cost` that a reply of GraphQL gives in `rateLimit`.
fn cost_in(stdout: &str) -> Option<u32> {
    let reply: serde_json::Value = serde_json::from_str(stdout).ok()?;
    u32::try_from(reply["data"]["rateLimit"]["cost"].as_u64()?).ok()
}

/// True when a reply of GraphQL holds `rateLimit` and nothing else. GitHub
/// charges no point for such a query, though its `cost` says 1: the
/// counter does not move (#1607).
fn only_rate_limit(stdout: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(stdout)
        .ok()
        .and_then(|reply| {
            reply["data"]
                .as_object()
                .map(|data| data.keys().eq(["rateLimit"]))
        })
        .unwrap_or(false)
}

/// The GraphQL points that the call `gh ARGS` with the output `stdout`
/// cost, from the table in the design. A call of REST costs none.
///
/// ```
/// use riff::ghcalls::points_of;
///
/// // 100 pull requests with their checks: 2 points. A second page: 4.
/// let pulls = ["pr", "list", "--limit", "1000", "--json", "number,statusCheckRollup"];
/// assert_eq!(points_of(&pulls, "[]"), 2);
/// let page = format!("[{}]", vec!["{}"; 101].join(","));
/// assert_eq!(points_of(&pulls, &page), 4);
/// // The cost follows the page: 30 pull requests (the default) cost 1.
/// assert_eq!(points_of(&["pr", "list", "--json", "number,statusCheckRollup"], "[]"), 1);
/// assert_eq!(points_of(&["pr", "list", "--head", "b", "--json", "number,state"], "[]"), 1);
/// let issues = ["issue", "list", "--limit", "100", "--json", "number,comments"];
/// assert_eq!(points_of(&issues, "[]"), 1);
/// let labels = ["issue", "list", "--limit", "100", "--json", "comments,labels"];
/// assert_eq!(points_of(&labels, "[]"), 2);
/// assert_eq!(points_of(&["issue", "list", "--json", "title,labels"], "[]"), 1);
/// assert_eq!(points_of(&["issue", "view", "12"], ""), 1);
/// assert_eq!(points_of(&["pr", "checks", "40"], ""), 2);
/// assert_eq!(points_of(&["issue", "close", "7"], ""), 2);
/// // A query that asks for `rateLimit` gives its own cost.
/// let reply = r#"{"data":{"rateLimit":{"cost":3,"remaining":10,"resetAt":"x"},"repository":{}}}"#;
/// assert_eq!(points_of(&["api", "graphql", "-f", "query=x"], reply), 3);
/// assert_eq!(points_of(&["api", "graphql", "-f", "query=x"], "{}"), 1);
/// // A query that asks for nothing else costs nothing, though it says 1.
/// let read = r#"{"data":{"rateLimit":{"cost":1,"remaining":10,"resetAt":"x"}}}"#;
/// assert_eq!(points_of(&["api", "graphql", "-f", "query=x"], read), 0);
/// assert_eq!(points_of(&["api", "repos/o/r"], "{}"), 0);
/// assert_eq!(points_of(&["run", "view", "1"], ""), 0);
/// ```
pub fn points_of(args: &[&str], stdout: &str) -> u32 {
    let kind = kind_of(args);
    match kind.as_str() {
        "api graphql" if only_rate_limit(stdout) => 0,
        "api graphql" => cost_in(stdout).unwrap_or(1),
        "pr list" | "issue list" => list_points(&kind, args, stdout),
        "pr checks" => 2,
        "pr create" | "pr merge" | "pr comment" | "pr edit" | "issue create" | "issue edit"
        | "issue comment" | "issue close" | "issue reopen" | "pr close" | "pr ready" => 2,
        other if other.starts_with("pr ") || other.starts_with("issue ") => 1,
        "release list" | "release view" | "repo view" | "repo clone" => 1,
        _ => 0,
    }
}

/// The reading of the limit in a reply of GraphQL that asked for
/// `rateLimit { limit remaining resetAt }`. `read_ms` is the time of the
/// reading. A reply with no `limit` gets [`GRAPHQL_LIMIT`].
///
/// ```
/// use riff::ghcalls::left_in;
///
/// let reply = r#"{"data":{"rateLimit":{"cost":1,"remaining":3716,"resetAt":"2026-11-03T15:00:35Z"}}}"#;
/// let left = left_in(reply, 7).unwrap();
/// assert_eq!((left.remaining, left.reset_at.as_str(), left.read_ms), (3716, "2026-11-03T15:00:35Z", 7));
/// assert_eq!(left.limit, 5000);
/// let own = r#"{"data":{"rateLimit":{"limit":12500,"remaining":3716,"resetAt":"2026-11-03T15:00:35Z"}}}"#;
/// assert_eq!(left_in(own, 7).unwrap().limit, 12500);
/// assert_eq!(left_in(r#"{"data":{}}"#, 7), None);
/// assert_eq!(left_in("not json", 7), None);
/// ```
pub fn left_in(stdout: &str, read_ms: i64) -> Option<Left> {
    let reply: serde_json::Value = serde_json::from_str(stdout).ok()?;
    let limit = &reply["data"]["rateLimit"];
    Some(Left {
        remaining: limit["remaining"].as_u64()?,
        reset_at: limit["resetAt"].as_str()?.to_owned(),
        read_ms,
        limit: limit["limit"].as_u64().unwrap_or(GRAPHQL_LIMIT),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 01M6GYN0CSFRBSHGE5CEBHDVZH: the count adds the calls and the points
    /// of one caller and one kind in one row, the most points first, and
    /// forgets a call after an hour.
    #[test]
    fn the_count_sums_each_caller_and_kind_and_forgets_after_an_hour() {
        let calls = Calls::default();
        for at in [0, 60_000, 120_000] {
            calls.record(at, "look", "issue list", 4);
        }
        calls.record(60_000, "look", "api repo", 0);
        calls.record(90_000, "wave", "issue list", 4);
        let shown = calls.shown(150_000);
        let rows: Vec<(&str, &str, u32, u32)> = shown
            .rows
            .iter()
            .map(|r| (r.caller.as_str(), r.kind.as_str(), r.calls, r.points))
            .collect();
        assert_eq!(
            rows,
            [
                ("look", "issue list", 3, 12),
                ("wave", "issue list", 1, 4),
                ("look", "api repo", 1, 0),
            ]
        );
        assert_eq!(shown.total(), (5, 16));
        // The call at 0 is an hour old at 3,600,000.
        let later = calls.shown(WINDOW_MS + 60_000);
        assert_eq!(later.total(), (3, 8), "{later:?}");
        assert!(calls.shown(2 * WINDOW_MS).rows.is_empty());
    }

    /// 01M6H1N2XYKK6QRCJS02RZJHKZ (#1557): a limit that is out shows in the
    /// count until its reset, and the wait that ends last is the one that
    /// the loops of the daemon wait for.
    #[test]
    fn a_wait_shows_until_its_reset_and_the_last_one_counts() {
        let calls = Calls::default();
        assert_eq!(calls.limit_wait(0), None);
        calls.start_wait(Wait::until(Resource::Rest, 30_000), 0);
        calls.start_wait(Wait::until(Resource::Graphql, 90_000), 0);
        let last = Wait::until(Resource::Graphql, 90_000);
        assert_eq!(calls.limit_wait(10_000), Some(last));
        assert_eq!(calls.shown(10_000).waits.len(), 2);
        // REST starts again first: the wait that stands is GraphQL.
        let rest = Wait::until(Resource::Rest, 30_000);
        assert_eq!(calls.waiting(Resource::Rest, 31_000), None, "{rest:?}");
        assert_eq!(calls.shown(31_000).waits, [last]);
        assert_eq!(calls.limit_wait(91_000), None);
        assert!(calls.shown(91_000).waits.is_empty());
    }

    #[test]
    fn a_limit_is_found_in_the_error_of_gh_and_in_a_refused_call() {
        let limited = anyhow::Error::new(Limited(Wait::until(Resource::Rest, 0)));
        assert!(is_limit_error(&limited));
        let gh = anyhow::anyhow!(
            "gh pr view 40: GraphQL: API rate limit already exceeded for user ID 1."
        );
        assert!(is_limit_error(&gh));
        assert!(!is_limit_error(&anyhow::anyhow!("gh pr view 40: HTTP 502")));
        // The kinds of call use the resource of their limit.
        assert_eq!(
            Resource::of(&["pr", "checks", "40"]),
            Some(Resource::Graphql)
        );
        assert_eq!(
            Resource::of(&["api", "-X", "POST", "repos/o/r/issues/12/comments"]),
            Some(Resource::Rest)
        );
    }

    #[test]
    fn the_last_reading_of_the_limit_stays() {
        let calls = Calls::default();
        assert_eq!(calls.shown(0).left, None);
        let left = Left {
            remaining: 10,
            reset_at: "2026-11-03T15:00:35Z".into(),
            read_ms: 5,
            limit: GRAPHQL_LIMIT,
        };
        calls.keep_left(left.clone());
        assert_eq!(calls.shown(WINDOW_MS * 3).left, Some(left));
    }

    /// The counter of 22:00:17 to 23:00:17 UTC of the issue: the reading at
    /// 22:28 shows 2,222 points left, so it used 2,778.
    fn counter() -> (Left, i64) {
        let start = 1_793_743_217_000; // 2026-11-03T22:00:17Z
        let left = Left {
            remaining: 2_222,
            reset_at: "2026-11-03T23:00:17Z".into(),
            read_ms: start + 28 * 60_000,
            limit: GRAPHQL_LIMIT,
        };
        (left, start)
    }

    /// 01M6JGF755TSQKWJBBZNEAW1GP (#1582): the points that the counter used
    /// less the points that riff counted from the start of the counter to
    /// the reading are the points of the other callers.
    #[test]
    fn the_other_callers_are_the_counter_less_the_calls_that_riff_counted() {
        let (left, start) = counter();
        assert_eq!(left.started_ms(), Some(start));
        let calls = Calls::default();
        calls.keep_left(left.clone());
        // 208 points of riff in the counter: the daemon and a riff top.
        calls.record(start + 60_000, "wave", "issue list", 100);
        let tally = [
            Call {
                at_ms: start + 120_000,
                caller: "top".into(),
                kind: "pr list".into(),
                points: 100,
                ..Call::default()
            },
            Call {
                at_ms: start + 130_000,
                caller: "pr wait".into(),
                kind: "pr view".into(),
                points: 8,
                ..Call::default()
            },
        ];
        // A call before the counter started, and one after the reading, are
        // not in this counter.
        let outside = [
            Call {
                at_ms: start - 1,
                caller: "top".into(),
                kind: "pr list".into(),
                points: 50,
                ..Call::default()
            },
            Call {
                at_ms: left.read_ms + 1,
                caller: "top".into(),
                kind: "pr list".into(),
                points: 50,
                ..Call::default()
            },
        ];
        let calls_in = [tally.as_slice(), outside.as_slice()].concat();
        let shown = calls.shown_with(left.read_ms + 60_000, calls_in);
        let other = shown.other.unwrap();
        assert_eq!(
            (other.used, other.counted, other.points),
            (2_778, 208, 2_570)
        );
        assert_eq!(other.a_minute(), 91);
        assert_eq!(other.since_ms, start);
        assert_eq!(other.read_ms, left.read_ms);
    }

    /// There is no other line without a reading, and none after the counter
    /// started again: the reading is of the old counter.
    #[test]
    fn the_other_line_needs_a_counter_that_has_not_ended() {
        let (left, start) = counter();
        let calls = Calls::default();
        assert_eq!(calls.shown(start).other, None);
        calls.keep_left(left.clone());
        assert!(calls.shown(left.read_ms).other.is_some());
        let reset = left.reset_ms().unwrap();
        assert!(calls.shown(reset - 1).other.is_some());
        assert_eq!(calls.shown(reset).other, None);
        // A reading with a reset that riff cannot read gives none.
        calls.keep_left(Left {
            reset_at: "soon".into(),
            ..left
        });
        assert_eq!(calls.shown(start).other, None);
    }

    /// More points counted than the counter used (the counter that answered
    /// is not the one that the calls used): the points of the other callers
    /// are 0, and `counted_more` says that 0 means nothing (#1607).
    #[test]
    fn the_other_callers_never_use_less_than_nothing() {
        let (left, start) = counter();
        let calls = Calls::default();
        calls.keep_left(Left {
            remaining: GRAPHQL_LIMIT - 10,
            ..left
        });
        calls.record(start + 1_000, "wave", "issue list", 30);
        let other = calls.shown(start + 2_000).other.unwrap();
        assert_eq!((other.used, other.counted, other.points), (10, 30, 0));
        assert!(other.counted_more());
    }

    /// 01M6QBRR4GDSNZ9YPTDR3AJ0KA (#1607): a page costs the requests of its
    /// query, so the points of a list follow `--limit` in each form, the
    /// size of the last page, and the lists inside each item.
    #[test]
    fn a_list_costs_the_requests_of_each_page() {
        let reply = |items: usize| format!("[{}]", vec!["{}"; items].join(","));
        let checks = "number,statusCheckRollup";
        let pulls = |limit: &[&'static str]| {
            let mut args = vec!["pr", "list"];
            args.extend(limit);
            args.extend(["--json", checks]);
            args
        };
        // 1 + 2 x 74 = 149 requests: 1. 1 + 2 x 75 = 151: 2.
        assert_eq!(points_of(&pulls(&["--limit", "74"]), "[]"), 1);
        assert_eq!(points_of(&pulls(&["--limit", "75"]), "[]"), 2);
        assert_eq!(points_of(&pulls(&["-L", "75"]), "[]"), 2);
        assert_eq!(points_of(&pulls(&["--limit=75"]), "[]"), 2);
        // A limit that is no number is no limit: 30.
        assert_eq!(points_of(&pulls(&["--limit", "many"]), "[]"), 1);
        // The last page asks for the rest of the limit: 100, then 20.
        let limit = ["--limit", "120"];
        assert_eq!(points_of(&pulls(&limit), &reply(100)), 2);
        assert_eq!(points_of(&pulls(&limit), &reply(120)), 2 + 1);
        // Two full pages of a limit of 1,000.
        let limit = ["--limit", "1000"];
        assert_eq!(points_of(&pulls(&limit), &reply(150)), 2 + 2);
        // A list of issues without `--json` asks for the labels; a list of
        // pull requests without `--json` asks for no list.
        assert_eq!(points_of(&["issue", "list", "--limit", "100"], "[]"), 1);
        assert_eq!(lists_in("issue list", &["issue", "list"]), 1);
        assert_eq!(lists_in("pr list", &["pr", "list"]), 0);
        assert_eq!(
            lists_in("issue list", &["--json", "comments,labels,body,milestone"]),
            2
        );
    }

    /// 01M6QBRR4GDSNZ9YPTDR3AJ0KA (#1607): a query that asks for `rateLimit`
    /// and nothing else costs no point. A query with more costs its `cost`.
    #[test]
    fn a_read_of_the_counter_alone_costs_no_point() {
        let args = ["api", "graphql", "-f", "query=x"];
        let alone = r#"{"data":{"rateLimit":{"limit":5000,"remaining":4341,"resetAt":"x"}}}"#;
        assert_eq!(points_of(&args, alone), 0);
        let with_cost = r#"{"data":{"rateLimit":{"cost":1,"remaining":4341,"resetAt":"x"}}}"#;
        assert_eq!(points_of(&args, with_cost), 0);
        let queue =
            r#"{"data":{"rateLimit":{"cost":2,"remaining":4341,"resetAt":"x"},"repository":{}}}"#;
        assert_eq!(points_of(&args, queue), 2);
        assert_eq!(points_of(&args, "not json"), 1);
    }

    /// A reading from an old daemon has no limit: the limit of a token.
    #[test]
    fn a_reading_with_no_limit_gets_the_limit_of_a_token() {
        let left: Left = serde_json::from_str(
            r#"{"remaining":10,"reset_at":"2026-11-03T15:00:35Z","read_ms":5}"#,
        )
        .unwrap();
        assert_eq!(left.limit, GRAPHQL_LIMIT);
        let shown: Shown = serde_json::from_str(r#"{"rows":[],"left":null,"waits":[]}"#).unwrap();
        assert_eq!(shown.other, None);
    }
}
