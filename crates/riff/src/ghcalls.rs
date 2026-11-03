//! The count of the calls of `gh` that the riff daemon makes (#1549).
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
//! # The points
//!
//! GitHub counts the points of a GraphQL query, not the calls: one
//! point for each 100 requests that the query needs, at the most it
//! could ask. A `gh` call has no header that says its cost, so riff
//! takes the cost from a table of the queries that `gh` sends, from
//! `rateLimit(dryRun: true) { cost }` (#1549):
//!
//! | Call | Points |
//! |---|---|
//! | `gh pr list --json statusCheckRollup` | 2 for each 100 pull requests |
//! | `gh pr list --head BRANCH` | 1 |
//! | `gh issue list --json comments` | 2 for each 100 issues |
//! | `gh issue list` with no `comments` | 1 for each 100 issues |
//! | `gh issue view`, `gh pr view` | 1 |
//! | `gh pr checks` | 2 |
//! | a write: `pr create`, `pr merge`, a comment, `issue close` | 2 |
//! | `gh api graphql` | the `cost` in the reply, else 1 |
//! | `gh api PATH` of REST, `gh run`, `gh auth` | 0, the REST limit counts them |
//!
//! The reply of a query that asks for `rateLimit` also says how many
//! points are left and when they come back. riff keeps the last one
//! ([`Left`]). GitHub keeps more than one counter for one token: two
//! reads a second apart can give different numbers and different times
//! of reset. So the number is a guide, not a promise.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use serde::{Deserialize, Serialize};

/// How long a record stays: the count of `riff daemon status`.
pub const WINDOW_MS: i64 = 3_600_000;

/// The target for the looks and the watches of two waves that have no
/// work: at most this many GraphQL points an hour (#1549).
pub const IDLE_POINTS_AN_HOUR: u32 = 300;

/// The target for the looks, the watches and the sweeps of one wave with
/// 8 sessions: at most this many GraphQL points an hour (#1549).
pub const BUSY_POINTS_AN_HOUR: u32 = 1_500;

/// The caller of a `gh` that has no name.
pub const OTHER: &str = "other";

/// One call of `gh`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Call {
    at_ms: i64,
    caller: &'static str,
    kind: String,
    points: u32,
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
}

/// The calls of one caller and one kind in the last hour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    /// Who called: `wave`, `look`, `watches`, `sweep`, `usage` or
    /// [`OTHER`].
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
}

impl Shown {
    /// The calls and the points of all rows.
    ///
    /// ```
    /// use riff::ghcalls::{Row, Shown};
    ///
    /// let row = |calls, points| Row { caller: "look".into(), kind: "pr list".into(), calls, points };
    /// let shown = Shown { rows: vec![row(3, 6), row(1, 2)], left: None };
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
    pub fn record(&self, now_ms: i64, caller: &'static str, kind: &str, points: u32) {
        let mut inner = self.lock();
        forget(&mut inner.calls, now_ms);
        inner.calls.push_back(Call {
            at_ms: now_ms,
            caller,
            kind: kind.to_owned(),
            points,
        });
    }

    /// Keeps the reading of the limit that a reply gave.
    pub fn keep_left(&self, left: Left) {
        self.lock().left = Some(left);
    }

    /// The calls of the last hour at `now_ms`, one row for each caller and
    /// kind, the most points first, then the most calls.
    pub fn shown(&self, now_ms: i64) -> Shown {
        let mut inner = self.lock();
        forget(&mut inner.calls, now_ms);
        let mut sums: BTreeMap<(&str, &str), (u32, u32)> = BTreeMap::new();
        for call in &inner.calls {
            let sum = sums.entry((call.caller, &call.kind)).or_default();
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
        Shown {
            rows,
            left: inner.left.clone(),
        }
    }
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
    u32::try_from(items.div_ceil(100))
        .unwrap_or(u32::MAX)
        .max(1)
}

/// True when the `--json` of `args` names `field`.
fn has_field(args: &[&str], field: &str) -> bool {
    args.iter()
        .position(|a| *a == "--json")
        .and_then(|at| args.get(at + 1))
        .is_some_and(|fields| fields.split(',').any(|f| f == field))
}

/// The `cost` that a reply of GraphQL gives in `rateLimit`.
fn cost_in(stdout: &str) -> Option<u32> {
    let reply: serde_json::Value = serde_json::from_str(stdout).ok()?;
    u32::try_from(reply["data"]["rateLimit"]["cost"].as_u64()?).ok()
}

/// The GraphQL points that the call `gh ARGS` with the output `stdout`
/// cost, from the table in the design. A call of REST costs none.
///
/// ```
/// use riff::ghcalls::points_of;
///
/// // 100 pull requests with their checks: 2 points. A second page: 4.
/// let pulls = ["pr", "list", "--json", "number,statusCheckRollup"];
/// assert_eq!(points_of(&pulls, "[]"), 2);
/// let page = format!("[{}]", vec!["{}"; 101].join(","));
/// assert_eq!(points_of(&pulls, &page), 4);
/// assert_eq!(points_of(&["pr", "list", "--head", "b", "--json", "number,state"], "[]"), 1);
/// assert_eq!(points_of(&["issue", "list", "--json", "number,comments"], "[]"), 2);
/// assert_eq!(points_of(&["issue", "list", "--json", "title,labels"], "[]"), 1);
/// assert_eq!(points_of(&["issue", "view", "12"], ""), 1);
/// assert_eq!(points_of(&["pr", "checks", "40"], ""), 2);
/// assert_eq!(points_of(&["issue", "close", "7"], ""), 2);
/// // A query that asks for `rateLimit` gives its own cost.
/// let reply = r#"{"data":{"rateLimit":{"cost":3,"remaining":10,"resetAt":"x"}}}"#;
/// assert_eq!(points_of(&["api", "graphql", "-f", "query=x"], reply), 3);
/// assert_eq!(points_of(&["api", "graphql", "-f", "query=x"], "{}"), 1);
/// assert_eq!(points_of(&["api", "repos/o/r"], "{}"), 0);
/// assert_eq!(points_of(&["run", "view", "1"], ""), 0);
/// ```
pub fn points_of(args: &[&str], stdout: &str) -> u32 {
    let kind = kind_of(args);
    match kind.as_str() {
        "api graphql" => cost_in(stdout).unwrap_or(1),
        "pr list" => {
            let each = if has_field(args, "statusCheckRollup") {
                2
            } else {
                1
            };
            each * pages(stdout)
        }
        "issue list" => {
            let each = if has_field(args, "comments") { 2 } else { 1 };
            each * pages(stdout)
        }
        "pr checks" => 2,
        "pr create" | "pr merge" | "pr comment" | "pr edit" | "issue create" | "issue edit"
        | "issue comment" | "issue close" | "issue reopen" | "pr close" | "pr ready" => 2,
        other if other.starts_with("pr ") || other.starts_with("issue ") => 1,
        "release list" | "release view" | "repo view" | "repo clone" => 1,
        _ => 0,
    }
}

/// The reading of the limit in a reply of GraphQL that asked for
/// `rateLimit { remaining resetAt }`. `read_ms` is the time of the reading.
///
/// ```
/// use riff::ghcalls::left_in;
///
/// let reply = r#"{"data":{"rateLimit":{"cost":1,"remaining":3716,"resetAt":"2026-11-03T15:00:35Z"}}}"#;
/// let left = left_in(reply, 7).unwrap();
/// assert_eq!((left.remaining, left.reset_at.as_str(), left.read_ms), (3716, "2026-11-03T15:00:35Z", 7));
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

    #[test]
    fn the_last_reading_of_the_limit_stays() {
        let calls = Calls::default();
        assert_eq!(calls.shown(0).left, None);
        let left = Left {
            remaining: 10,
            reset_at: "2026-11-03T15:00:35Z".into(),
            read_ms: 5,
        };
        calls.keep_left(left.clone());
        assert_eq!(calls.shown(WINDOW_MS * 3).left, Some(left));
    }
}
