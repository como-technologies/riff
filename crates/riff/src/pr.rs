//! The steps that each item repeats on GitHub: open a pull request, wait
//! for its merge, and report a verify.
//!
//! # Design
//!
//! Each step is one plain `riff` command, a thin wrapper around the `gh`
//! of the machine. It needs no new service. A session runs one command
//! for one step, with no shell loop, so the worktree guard of Claude
//! Code accepts it, and each session does the step in the same way.
//!
//! | Command | `gh` calls |
//! |---|---|
//! | `riff pr open` | `issue view`, `pr create`, `pr merge --auto --squash` |
//! | `riff pr wait N` | `pr view`, `pr checks --required`, again at the [`pace`] of `--every`. After the merge: the total of the tokens of the issue ([`crate::usage`]) |
//! | `riff verify pass\|fail N` | `pr view`, `api …/rules/branches/BRANCH`, `api …/commits/COMMIT/check-runs` and `api …/pulls/N/files` (a pass), `pr comment`, `api …/statuses/COMMIT` |
//!
//! `riff pr open` takes the issue from the claim of the session, and
//! writes the body in the form of the hygiene check: the link line and
//! the trailers `Issue:` and `Milestone:` ([`body`]). The trailer
//! `Milestone:` names a wave by its number, `Wave N` with no name
//! ([`hygiene::milestone_value`], 01M518A07QF1YACBYT91M0DD1R). It adds
//! each of them only when the summary does not have it, and refuses a
//! summary with a line for another issue or another wave
//! (01M3W2627GYXR8CFW76KB6CB9W). It checks the pull request with
//! [`hygiene::check_pr_for`] before it opens it, and turns on auto-merge
//! with a squash at once (01M3NB6FTGPD0S5JTXXXNGNNDT).
//!
//! The rules are the ones that the riff config of the repository asks
//! for ([`crate::hygiene_rules`]). Only the repository of riff gets
//! each rule. Another repository gets the rules for what riff reads, the
//! link line and the trailer `Issue:`, so its pull request needs no
//! milestone and no trailer `Milestone:`
//! (01M5SMSHNQ9E4P9W6RCB8E1WTN, 01M5SMSMWE89SC3Q7XFBQCFY32).
//!
//! `riff pr wait N` looks at the pull request until it is merged, and
//! prints the merge commit. It stops with an error when the pull request
//! closes unmerged or a required check fails ([`Wait`],
//! 01M3NB6FWMGBQ9VTY6RCBPKBHK).
//!
//! The command runs in its own process, so the count of the daemon
//! ([`crate::ghcalls`]) cannot see its calls, and it uses the token of
//! the person like all other sessions. A look is 3 GraphQL points, so
//! the pace is slow: each `--every` seconds (30) for the first 2
//! minutes, then each 5 minutes ([`pace`]). At that default pace, one
//! pull request that waits uses at most [`WAIT_POINTS_AN_HOUR`] points
//! an hour. A shorter `--every` costs more: `--every 10` uses 72
//! points in the first hour (01M6JE9ME6YSHV538GNPH7ZRGQ).
//!
//! ```mermaid
//! flowchart LR
//!     A["0 to 2 minutes:<br/>a look each 30 s"] --> B["after 2 minutes:<br/>a look each 5 min"]
//! ```
//!
//! `riff verify pass|fail N` puts the result on the pull request as a
//! comment that names the head commit, sets the status `riff/verify` of
//! that commit with the URL of the comment, and posts the result to the
//! holder of the issue of the `Issue:` trailer
//! (01M3NB6FYXXKX80VHEVA5CV6RY). A verify counts only for its commit: it
//! reports nothing when the head of the pull request is not the commit
//! that the verifier tested, `HEAD` of its worktree or `--commit`
//! ([`same_commit`]).
//!
//! GitHub sometimes fails a call with a 502, 503 or 504. A [`Gh`] made
//! with [`Gh::retrying`] runs such a call again, up to [`RETRIES`] times,
//! after [`RETRY_WAIT`] and double that for each next try
//! ([`gateway_fault`], 01M6H12ET0FQ553QHGVF5XJMQ9). The release commands
//! use it. Another failure gets no new try. After the last try, the error
//! of the call ends with the number of tries.
//!
//! A limit of GitHub is a failure that a new try cannot fix. A [`Gh`] made
//! with [`Gh::stopping_on_limit`] fails such a call with
//! [`ghcalls::LimitOut`]: the reset of the counter that failed, and what
//! to run then (01M6QB459TJVK6CS9FHWH6EVK2). The release commands use it.
//!
//! The full check of a commit is its one full test run
//! (01M49HAZ5BZYGAR9PGC089RM3F). In the repository of riff it is the job
//! `Gate`. In another repository it is each check that the ruleset of the
//! default branch requires, but `riff/verify`; a repository with no CI has
//! none, and `riff/verify` is its only check ([`repo_setup::full_check`],
//! 01M5QYDE41FVFX26EC8K5QGYH4, 01M5SMNKDWBD1J6Y18ENQG87CK). The verifier
//! does not run the tests again: it reviews. So `riff verify pass` reports
//! nothing while the full check of the head commit has no success
//! ([`gate_of`], 01M49HAZ7P3JMWNCG1SWCMAXQP). A fail needs no Gate.
//!
//! A pull request that changes the session image in `riff.toml`
//! ([`pin_changed`]) needs one more thing for a pass: the registry of the
//! new pin must give its manifest with no login
//! ([`crate::registry::anonymous_here`], 01M5SMRRAG1WPBJMQKKTPSRHRP). A
//! pull request that merges an image that a host cannot pull stops each
//! new session of the repository on that host. The verify of the wave
//! uses the same rule, because it calls [`report`]. A fail needs no
//! such check.
//!
//! The author of a pull request releases its item at the verify
//! request. So the issue can have no holder. Then the result wakes the
//! lead of the user of the verifier in the repository ([`result_to`],
//! 01M3Z9N70J4H79VJN4ZKKH3G6S): a result is never silent.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as author
//!     participant V as verifier
//!     participant G as gh
//!     participant E as riff-server
//!     A->>G: riff pr open: pr create, pr merge --auto --squash
//!     V->>G: riff verify pass 40: pr comment, status riff/verify
//!     V->>E: post to [claim=issue-12], and to the lead when no session holds issue-12
//!     A->>G: riff pr wait 40: pr view until MERGED
//! ```

use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::SessionInfo;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::ghcalls::{self, Calls};
use crate::registry;
use crate::repo_setup;
use crate::session_image;
use crate::text;

/// The status code in the first line of a reply that `gh api --include`
/// printed, for example `502` of `HTTP/2.0 502 Bad Gateway`.
pub fn http_status(reply: &str) -> Option<&str> {
    let line = reply.lines().next()?;
    let code = line.strip_prefix("HTTP/")?.split_whitespace().nth(1)?;
    (code.len() == 3 && code.bytes().all(|b| b.is_ascii_digit())).then_some(code)
}

/// The answer to a conditional read ([`Gh::ask`], #1566).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// GitHub answered `304`: the reply is still the one of the `ETag` that
    /// the ask sent.
    Same,
    /// GitHub gave a reply, so the reply is new. `etag` is its `ETag`, when
    /// the headers have one.
    Changed { etag: Option<String> },
}

/// The `ETag` in the headers of a reply that `gh api --include` printed.
///
/// ```
/// use riff::pr::etag_of;
///
/// let reply = "HTTP/2.0 200 OK\r\nContent-Type: x\r\nEtag: W/\"a1\"\r\n\r\n[]";
/// assert_eq!(etag_of(reply), Some("W/\"a1\"".to_owned()));
/// // A line of the body is no header.
/// assert_eq!(etag_of("HTTP/2.0 200 OK\r\n\r\netag: body"), None);
/// assert_eq!(etag_of(""), None);
/// ```
pub fn etag_of(reply: &str) -> Option<String> {
    reply
        .lines()
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            let value = value.trim();
            (name.eq_ignore_ascii_case("etag") && !value.is_empty()).then(|| value.to_owned())
        })
}

/// True when a failed call of `gh` ended in a 502, 503 or 504 of GitHub:
/// `HTTP 504: 504 Gateway Timeout (…/graphql)` of a GraphQL call,
/// `Bad Gateway (HTTP 502)` of a REST call, or the status line of
/// `gh api --include` in `stdout`.
///
/// ```
/// use riff::pr::gateway_fault;
///
/// assert!(gateway_fault("", "HTTP 504: 504 Gateway Timeout (https://api.github.com/graphql)"));
/// assert!(gateway_fault("", "gh: Bad Gateway (HTTP 502)\n"));
/// assert!(gateway_fault("HTTP/2.0 503 Service Unavailable\r\n", ""));
/// assert!(!gateway_fault("", "HTTP 404: Not Found (https://api.github.com/graphql)"));
/// assert!(!gateway_fault("", "HTTP 5040: no such thing"));
/// assert!(!gateway_fault("", "GraphQL: API rate limit already exceeded"));
/// ```
pub fn gateway_fault(stdout: &str, stderr: &str) -> bool {
    const CODES: [&str; 3] = ["502", "503", "504"];
    if http_status(stdout).is_some_and(|code| CODES.contains(&code)) {
        return true;
    }
    stderr.match_indices("HTTP ").any(|(at, word)| {
        let after = &stderr[at + word.len()..];
        CODES.iter().any(|code| {
            after
                .strip_prefix(code)
                .is_some_and(|rest| !rest.starts_with(|c: char| c.is_ascii_digit()))
        })
    })
}

/// How many times a `gh` that retries runs a call again after a 502, 503
/// or 504 of GitHub ([`Gh::retrying`], 01M6H12ET0FQ553QHGVF5XJMQ9).
pub const RETRIES: u32 = 3;

/// The wait before the first new try of a call. The wait doubles for
/// each new try: 2 s, 4 s, 8 s.
pub const RETRY_WAIT: Duration = Duration::from_secs(2);

/// The variable that gives the first wait before a new try in
/// milliseconds. Only a test sets it.
///
/// ```
/// assert_eq!(riff::pr::RETRY_VAR, "RIFF_GH_RETRY_MS");
/// ```
pub const RETRY_VAR: &str = "RIFF_GH_RETRY_MS";

/// The context of the verify status of a commit.
pub const VERIFY_CONTEXT: &str = "riff/verify";

/// The `gh` of the machine.
#[derive(Debug, Clone)]
pub struct Gh {
    program: PathBuf,
    /// The repository of each call, as `OWNER/REPO`: `GH_REPO`. With
    /// none, `gh` takes the repository of the current folder.
    repo: Option<String>,
    /// The time limit of each call. With none, a call can take each
    /// time that `gh` takes.
    limit: Option<Duration>,
    /// The token of each call, from one read for each time `keep`
    /// ([`Gh::with_token_kept`]). With none, each call of `gh` reads its
    /// token itself.
    token: Option<Shared>,
    /// Who calls: the name in the count of the calls ([`Gh::by`]).
    caller: &'static str,
    /// Where the count of the calls goes ([`Gh::with_calls`]). With
    /// none, the calls are not counted.
    calls: Option<Arc<Calls>>,
    /// Where a riff process other than the daemon adds its calls
    /// ([`Gh::here`], #1582). With none, the calls are not added.
    tally: Option<ghcalls::Tally>,
    /// True when the reads of this `gh` share their replies with the other
    /// `gh` of the process ([`Gh::read`]).
    shares: bool,
    /// How old a shared reply can be for [`Gh::read`]. With zero, each
    /// read calls `gh`.
    accepts: Duration,
    /// How many times a call that GitHub fails with a 502, 503 or 504
    /// runs again ([`Gh::retrying`]). With zero, it does not.
    retries: u32,
    /// The wait before the first new try: it doubles for each next one.
    retry_wait: Duration,
    /// True when a call that fails on a limit of GitHub fails with
    /// [`ghcalls::LimitOut`] ([`Gh::stopping_on_limit`]).
    stops_on_limit: bool,
}

impl Default for Gh {
    fn default() -> Self {
        Self {
            program: "gh".into(),
            repo: None,
            limit: None,
            token: None,
            caller: ghcalls::OTHER,
            calls: None,
            tally: None,
            shares: false,
            accepts: Duration::ZERO,
            retries: 0,
            retry_wait: RETRY_WAIT,
            stops_on_limit: false,
        }
    }
}

/// The token that the clones of a `gh` share, and how long one read
/// stays.
#[derive(Debug, Clone)]
struct Shared {
    keep: Duration,
    read: Arc<Mutex<Option<GhToken>>>,
}

/// One read of the token of `gh`: its time, and the token, or `None`
/// when the read gave none. Its debug text has no value, and the memory
/// of the token is cleared when it drops.
struct GhToken {
    at: Instant,
    token: Option<Zeroizing<String>>,
}

impl fmt::Debug for GhToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GhToken")
    }
}

/// How long the calls of a `gh` with one token use one read of it
/// ([`Gh::with_one_token`]). A process that runs for a long time reads
/// it again after this time, so a new sign-in of `gh` reaches it.
pub const TOKEN_KEEP: Duration = Duration::from_secs(600);

/// The variables that give `gh` its token. When one is set, a call of
/// `gh` reads no keyring.
const TOKEN_VARS: [&str; 2] = ["GH_TOKEN", "GITHUB_TOKEN"];

/// The time between two looks at a `gh` with a time limit.
const LIMIT_POLL: Duration = Duration::from_millis(20);

impl Gh {
    /// How long a shared reply of a fact that never changes stays: the
    /// message of a commit ([`Gh::read_within`]).
    pub const LONG: Duration = Duration::from_secs(3600);

    /// How long a shared reply of a fact that changes rarely stays: the
    /// default branch and the rules of a repository.
    pub const SLOW: Duration = Duration::from_secs(600);

    /// The `gh` at `program`, for tests.
    pub fn at(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            ..Self::default()
        }
    }

    /// The `gh` of the machine for the repository `repo` (`OWNER/REPO`)
    /// from each folder: it sets `GH_REPO`. The riff daemon uses it: it
    /// runs in no clone (#719). Its calls go to the count of the daemon
    /// ([`Calls::global`]), and its reads share their replies
    /// ([`Gh::read`]).
    pub fn for_repo(repo: &str) -> Self {
        Self {
            shares: true,
            ..Self::in_daemon(ghcalls::OTHER).with_repo(repo)
        }
    }

    /// The `gh` of the riff daemon with no repository, for the caller
    /// `caller`: a step of the daemon that runs in no clone, such as the
    /// clone of a main clone. Its calls go to the count of the daemon
    /// ([`Calls::global`]).
    ///
    /// ```
    /// let gh = riff::pr::Gh::in_daemon("clone");
    /// assert_eq!(gh.caller(), "clone");
    /// ```
    pub fn in_daemon(caller: &'static str) -> Self {
        Self::default()
            .by(caller)
            .with_calls(Calls::global().clone())
    }

    /// The same `gh` for the repository `repo` (`OWNER/REPO`) from each
    /// folder: it sets `GH_REPO`.
    ///
    /// ```
    /// let gh = riff::pr::Gh::here("init").with_repo("o/r");
    /// assert_eq!(gh.caller(), "init");
    /// ```
    pub fn with_repo(self, repo: &str) -> Self {
        Self {
            repo: Some(repo.to_owned()),
            ..self
        }
    }

    /// The same `gh` for the caller `caller` in the count of the calls: a
    /// name of [`ghcalls::DAEMON`] or [`ghcalls::CALLERS`]. A test fails on
    /// a `gh` with no name, as `Gh::default()` in code outside tests.
    pub fn by(self, caller: &'static str) -> Self {
        Self { caller, ..self }
    }

    /// The `gh` of a command of `riff` (`riff top`, `riff pr wait`, ...)
    /// that runs in a process of its own. Its calls go to the tally of the
    /// daemon, under the name `caller`, so that `riff daemon status` shows
    /// them ([`ghcalls::Tally`], #1582). A process on the host writes the
    /// folder of the daemon. A process in a container sends its calls to
    /// the daemon on the socket of its session.
    ///
    /// ```
    /// let gh = riff::pr::Gh::here("top");
    /// assert_eq!(gh.caller(), "top");
    /// ```
    pub fn here(caller: &'static str) -> Self {
        Self::default()
            .by(caller)
            .with_tally(ghcalls::Tally::here())
    }

    /// The same `gh` whose calls go to `tally` ([`Gh::here`]). A `gh` that
    /// counts its calls tells the real `gh` so
    /// ([`crate::gh_wrapper::COUNTED_VAR`]): the wrapper of a container
    /// counts the call no more.
    ///
    /// ```
    /// use riff::ghcalls::Tally;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let gh = isolated::write_program(dir.path().join("gh"), "#!/bin/sh\necho '[]'\n");
    /// let tally = Tally::in_folder(dir.path());
    /// let gh = riff::pr::Gh::at(gh).by("pr wait").with_tally(Some(tally.clone()));
    /// gh.run(&["pr", "view", "40", "--json", "state"], None)?;
    /// gh.run(&["auth", "status"], None)?;
    /// let calls = tally.read(riff::daemon::now_ms());
    /// // `gh auth` uses no limit: one call.
    /// assert_eq!(calls.len(), 1);
    /// assert_eq!((calls[0].caller.as_str(), calls[0].kind.as_str(), calls[0].points), ("pr wait", "pr view", 1));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn with_tally(self, tally: Option<ghcalls::Tally>) -> Self {
        Self { tally, ..self }
    }

    /// The name of the caller in the count of the calls ([`Gh::by`]).
    pub fn caller(&self) -> &'static str {
        self.caller
    }

    /// The same `gh` with the count `calls` for its calls.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use riff::ghcalls::Calls;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let gh = isolated::write_program(dir.path().join("gh"), "#!/bin/sh\necho '[]'\n");
    /// let calls = Arc::new(Calls::default());
    /// let gh = riff::pr::Gh::at(gh).by("look").with_calls(calls.clone());
    /// gh.run(&["pr", "list", "--limit", "100", "--json", "number,statusCheckRollup"], None)?;
    /// gh.run(&["api", "repos/o/r"], None)?;
    /// let shown = calls.shown(riff::daemon::now_ms());
    /// assert_eq!(shown.total(), (2, 2));
    /// assert_eq!((shown.rows[0].caller.as_str(), shown.rows[0].kind.as_str()), ("look", "pr list"));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn with_calls(self, calls: Arc<Calls>) -> Self {
        Self {
            calls: Some(calls),
            ..self
        }
    }

    /// The same `gh` whose reads share their replies ([`Gh::read`]), for
    /// a test: [`Gh::for_repo`] does it already.
    pub fn with_shared_reads(self) -> Self {
        Self {
            shares: true,
            ..self
        }
    }

    /// The same `gh` whose [`Gh::read`] takes a shared reply that is not
    /// older than `age`. A look that must see the forge as it is now
    /// takes none (the default). A look that repeats the read of another
    /// loop takes the reply of that loop.
    pub fn accepting(self, age: Duration) -> Self {
        Self {
            accepts: age,
            ..self
        }
    }

    /// The same `gh` with the time limit `limit` for each call
    /// (01M53S7WGM9DQV2DXFHBRSMTP8). At the end of the limit, riff stops
    /// `gh` and each process that it started, and the call fails.
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    ///
    /// let dir = tempfile::tempdir()?;
    /// let hangs = isolated::write_program(dir.path().join("gh"), "#!/bin/sh\nsleep 600\n");
    /// let gh = riff::pr::Gh::at(&hangs).with_limit(Duration::from_millis(500));
    /// assert_eq!(gh.limit(), Some(Duration::from_millis(500)));
    /// let start = Instant::now();
    /// let error = gh.run(&["pr", "list"], None).unwrap_err().to_string();
    /// assert_eq!(error, "gh pr list: no answer in 0.5 s. riff stopped it.");
    /// assert!(start.elapsed() < Duration::from_secs(10));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn with_limit(self, limit: Duration) -> Self {
        Self {
            limit: Some(limit),
            ..self
        }
    }

    /// The same `gh`, whose calls run again after a 502, 503 or 504 of
    /// GitHub ([`gateway_fault`], 01M6H12ET0FQ553QHGVF5XJMQ9): up to
    /// [`RETRIES`] new tries, after [`RETRY_WAIT`] and double that for
    /// each next one. A call that still fails gives the error of `gh`
    /// and the number of tries. The release commands use it. A call that
    /// fails in another way is not tried again.
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let count = dir.path().join("count");
    /// let script = format!(
    ///     "#!/bin/sh\necho x >> {c}\n[ \"$(wc -l < {c})\" -lt 2 ] && {{ echo 'HTTP 504: 504 Gateway Timeout' >&2; exit 1; }}\necho '[]'\n",
    ///     c = count.display(),
    /// );
    /// let gh = isolated::write_program(dir.path().join("gh"), &script);
    /// let again = riff::pr::Gh::at(&gh).retrying_after(Duration::from_millis(1));
    /// assert_eq!(again.run(&["issue", "list"], None)?, "[]\n");
    /// assert_eq!(std::fs::read_to_string(&count)?.lines().count(), 2);
    /// // Without `retrying`, the first fault is the answer.
    /// std::fs::remove_file(&count)?;
    /// assert!(riff::pr::Gh::at(&gh).run(&["issue", "list"], None).is_err());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn retrying(self) -> Self {
        let wait = std::env::var(RETRY_VAR)
            .ok()
            .and_then(|ms| ms.parse().ok())
            .map_or(RETRY_WAIT, Duration::from_millis);
        self.retrying_after(wait)
    }

    /// [`Gh::retrying`] with the wait `wait` before the first new try.
    /// [`RETRY_VAR`] gives it to the `riff` of a test.
    pub fn retrying_after(self, wait: Duration) -> Self {
        Self {
            retries: RETRIES,
            retry_wait: wait,
            ..self
        }
    }

    /// The same `gh`, whose call fails with [`ghcalls::LimitOut`] when a
    /// limit of GitHub is out (01M6QB459TJVK6CS9FHWH6EVK2). It makes no
    /// new try. The reset comes from the call that failed, as the daemon
    /// reads it (#1581), so the error says when the counter starts again.
    /// The release commands use it.
    ///
    /// ```
    /// use riff::ghcalls::{LimitOut, Resource};
    ///
    /// let dir = tempfile::tempdir()?;
    /// let log = dir.path().join("log");
    /// let script = format!(
    ///     "#!/bin/sh\necho \"$*\" >> {}\n\
    ///      case \"$1 $2\" in\n\
    ///      \"api rate_limit\") echo '{{\"resources\":{{\"graphql\":{{\"remaining\":0,\"reset\":4102444800}}}}}}' ;;\n\
    ///      *) echo 'GraphQL: API rate limit already exceeded for user ID 1.' >&2; exit 1 ;;\n\
    ///      esac\n",
    ///     log.display(),
    /// );
    /// let gh = isolated::write_program(dir.path().join("gh"), &script);
    /// // Without it, the error is the raw error of `gh`.
    /// let raw = riff::pr::Gh::at(&gh).run(&["pr", "list"], None).unwrap_err();
    /// assert!(raw.downcast_ref::<LimitOut>().is_none(), "{raw:#}");
    /// // With it, the error has the reset of the counter that is out.
    /// let error = riff::pr::Gh::at(&gh).stopping_on_limit().run(&["pr", "list"], None).unwrap_err();
    /// let out = error.downcast_ref::<LimitOut>().expect("a limit");
    /// assert_eq!(out.0.resource, Resource::Graphql);
    /// assert_eq!(out.0.reset_ms, 4_102_444_800_000);
    /// assert!(riff::ghcalls::is_limit_error(&error));
    /// assert!(error.to_string().ends_with("Run the same command again then."), "{error:#}");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn stopping_on_limit(self) -> Self {
        Self {
            stops_on_limit: true,
            ..self
        }
    }

    /// The time limit of each call, when it has one.
    pub fn limit(&self) -> Option<Duration> {
        self.limit
    }

    /// The same `gh`, whose calls share the token of one read in each
    /// [`TOKEN_KEEP`] (01M54510SFK1Z9VPFYBYYSV10F). See
    /// [`Gh::with_token_kept`].
    pub fn with_one_token(self) -> Self {
        self.with_token_kept(TOKEN_KEEP)
    }

    /// The same `gh`, whose calls share the token of one read in each
    /// time `keep`. Each call of `gh` reads its token from the OS
    /// keyring, and each read opens a session of the Secret Service
    /// (#1003). So a call runs `gh auth token` when the last read is
    /// `keep` old or there is none, and each call gets the token in
    /// `GH_TOKEN`: in its environment, never in a command line or a
    /// file. The clones of this `gh` share the token. A `gh` that makes
    /// no call reads nothing. When the environment has `GH_TOKEN` or
    /// `GITHUB_TOKEN`, it reads nothing, and `gh` takes that token. When
    /// `gh auth token` fails, each call reads its token itself.
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let log = dir.path().join("gh.log");
    /// let script = format!(
    ///     "#!/bin/sh\necho \"${{GH_TOKEN:-none}} $*\" >> {}\n\
    ///      [ \"$1 $2\" = \"auth token\" ] && echo gho_one_read\nexit 0\n",
    ///     log.display(),
    /// );
    /// let gh = isolated::write_program(dir.path().join("gh"), &script);
    /// let shared = riff::pr::Gh::at(&gh).with_one_token();
    /// shared.run(&["pr", "view", "a"], None)?;
    /// shared.clone().run(&["pr", "view", "b"], None)?;
    /// // A token that stays no time: a read for each call.
    /// let fresh = riff::pr::Gh::at(&gh).with_token_kept(Duration::ZERO);
    /// fresh.run(&["pr", "view", "c"], None)?;
    /// # if std::env::var_os("GH_TOKEN").is_none() && std::env::var_os("GITHUB_TOKEN").is_none() {
    /// assert_eq!(
    ///     std::fs::read_to_string(&log)?,
    ///     "none auth token\ngho_one_read pr view a\ngho_one_read pr view b\n\
    ///      none auth token\ngho_one_read pr view c\n",
    /// );
    /// # }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn with_token_kept(self, keep: Duration) -> Self {
        Self {
            token: Some(Shared {
                keep,
                read: Arc::default(),
            }),
            ..self
        }
    }

    /// The token for the calls of this `gh`: `None` when it reads no
    /// token, or when the read fails.
    fn token(&self) -> Option<Zeroizing<String>> {
        let shared = self.token.as_ref()?;
        let mut read = shared.read.lock().unwrap_or_else(PoisonError::into_inner);
        if read.as_ref().is_none_or(|r| r.at.elapsed() >= shared.keep) {
            *read = Some(GhToken {
                at: Instant::now(),
                token: self.read_token(),
            });
        }
        read.as_ref()?.token.clone()
    }

    /// One read of the token with `gh auth token`, or `None` when the
    /// environment gives `gh` a token, or when the read fails.
    fn read_token(&self) -> Option<Zeroizing<String>> {
        if TOKEN_VARS
            .iter()
            .any(|var| std::env::var_os(var).is_some_and(|v| !v.is_empty()))
        {
            return None;
        }
        let plain = Gh {
            token: None,
            ..self.clone()
        };
        let (ok, stdout, _) = plain.output(&["auth", "token"], None).ok()?;
        let stdout = Zeroizing::new(stdout);
        let token = stdout.trim();
        (ok && !token.is_empty()).then(|| Zeroizing::new(token.to_owned()))
    }

    /// Runs `gh ARGS` with `input` on stdin, and returns its stdout. It
    /// fails with the stderr of `gh` when `gh` fails.
    pub fn run(&self, args: &[&str], input: Option<&str>) -> Result<String> {
        let (ok, stdout, stderr) = self.output(args, input)?;
        if !ok {
            bail!("gh {}: {}", args.join(" "), text::forge(stderr.trim()));
        }
        Ok(stdout)
    }

    /// [`Gh::run`] with no input, for a call that names itself: when `gh`
    /// fails, the error is `WHAT failed: STDERR`, with the stderr of
    /// `gh` and not its arguments ([`crate::text::gh_failed`]).
    pub fn run_as(&self, what: &str, args: &[&str]) -> Result<String> {
        let (ok, stdout, stderr) = self.output(args, None)?;
        if !ok {
            bail!(text::gh_failed(what, stderr.trim()));
        }
        Ok(stdout)
    }

    /// Sends `body` with `gh api -X METHOD PATH`, and returns the reply
    /// with its headers. The body goes in a file, not on stdin: a body
    /// on stdin can get lost (#566, 01M49HF057R08DGW6X5A8EHR42). It
    /// fails with the stderr of `gh`, and with the HTTP status of the
    /// reply when `gh` got one.
    ///
    /// ```
    /// assert_eq!(riff::pr::http_status("HTTP/2.0 502 Bad Gateway\r\nServer: x\r\n"), Some("502"));
    /// assert_eq!(riff::pr::http_status(""), None);
    /// ```
    pub fn send(&self, method: &str, path: &str, body: &serde_json::Value) -> Result<String> {
        let mut file = tempfile::NamedTempFile::new().context("cannot make a file for the body")?;
        serde_json::to_writer(&mut file, body)?;
        let file_path = file.path().to_string_lossy().into_owned();
        let args = [
            "api",
            "-X",
            method,
            path,
            "--include",
            "--input",
            &file_path,
        ];
        let (ok, stdout, stderr) = self.output(&args, None)?;
        if !ok {
            let status = http_status(&stdout)
                .map(|s| format!("HTTP {s}: "))
                .unwrap_or_default();
            bail!(
                "gh api -X {method} {path}: {status}{}",
                text::forge(stderr.trim())
            );
        }
        Ok(stdout)
    }

    /// Runs `gh ARGS` and reads its JSON output.
    pub fn json<T: DeserializeOwned>(&self, args: &[&str]) -> Result<T> {
        let out = self.run(args, None)?;
        serde_json::from_str(&out).with_context(|| format!("gh {}", args.join(" ")))
    }

    /// Reads with `gh ARGS` a fact that more than one loop of the daemon
    /// needs (#1549). It takes the reply of an earlier read of the same
    /// call in this process when that reply is not older than the age
    /// that [`Gh::accepting`] gave. Else it runs `gh`, and keeps the
    /// reply for the next read. A `gh` that shares no reads (the default,
    /// see [`Gh::with_shared_reads`]) just runs `gh`. A read that fails
    /// keeps nothing.
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let log = dir.path().join("log");
    /// let script = format!("#!/bin/sh\necho x >> {}\necho '[]'\n", log.display());
    /// let gh = riff::pr::Gh::at(isolated::write_program(dir.path().join("gh"), &script))
    ///     .with_shared_reads();
    /// let args = ["pr", "list", "--repo", "o/r"];
    /// // A read that accepts nothing runs `gh`, and keeps the reply.
    /// gh.read(&args)?;
    /// // A read that accepts a minute takes the reply that is there.
    /// let later = gh.clone().accepting(Duration::from_secs(60));
    /// assert_eq!(later.read(&args)?, "[]\n");
    /// assert_eq!(std::fs::read_to_string(&log)?.lines().count(), 1);
    /// // A read that accepts nothing runs `gh` each time.
    /// gh.read(&args)?;
    /// assert_eq!(std::fs::read_to_string(&log)?.lines().count(), 2);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn read(&self, args: &[&str]) -> Result<String> {
        self.read_within(args, self.accepts)
    }

    /// [`Gh::read`] that takes a reply of any `gh` of the process not older
    /// than `age`, whatever [`Gh::accepting`] says. A fact that never
    /// changes, or that changes rarely, asks for a long age.
    pub fn read_within(&self, args: &[&str], age: Duration) -> Result<String> {
        if !self.shares {
            return self.run(args, None);
        }
        let key = self.key(args);
        if let Some(reply) = replies::get(&key, age) {
            return Ok(reply);
        }
        let reply = self.run(args, None)?;
        replies::put(key, &reply);
        Ok(reply)
    }

    /// [`Gh::read`] and the JSON of its reply.
    pub fn read_json<T: DeserializeOwned>(&self, args: &[&str]) -> Result<T> {
        let out = self.read(args)?;
        serde_json::from_str(&out).with_context(|| format!("gh {}", args.join(" ")))
    }

    /// The reply that an earlier read of `gh ARGS` kept, when it is not
    /// older than `age`. It calls no `gh`.
    pub fn peek(&self, args: &[&str], age: Duration) -> Option<String> {
        self.shares.then(|| replies::get(&self.key(args), age))?
    }

    /// The key of the shared reply of `gh ARGS`: the program, the
    /// repository and the arguments.
    fn key(&self, args: &[&str]) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}",
            self.program.display(),
            self.repo.as_deref().unwrap_or_default(),
            args.join("\u{1f}")
        )
    }

    /// The key of this `gh` and its repository, for what a loop of the
    /// daemon keeps between two looks ([`crate::changes`]).
    pub(crate) fn scope(&self) -> String {
        self.key(&[])
    }

    /// Makes the reply that an earlier read of `gh ARGS` kept as new as
    /// now. The caller knows that the reply still holds: a conditional
    /// read answered `304` ([`Gh::ask`], #1566). It calls no `gh`.
    pub fn renew(&self, args: &[&str]) {
        if self.shares {
            replies::renew(&self.key(args));
        }
    }

    /// Asks GitHub with `gh api -i PATH` whether the reply of `PATH`
    /// changed since the answer with the `ETag` `etag` (#1566). The `ETag`
    /// goes in `If-None-Match`. A `304` costs no point. `gh` exits with 1
    /// for it and prints `gh: HTTP 304`: riff reads it as [`Answer::Same`],
    /// not as a fault. With no `etag` the answer is always
    /// [`Answer::Changed`], and it has the `ETag` to ask with the next
    /// time.
    ///
    /// ```
    /// use riff::pr::{Answer, Gh};
    ///
    /// let dir = tempfile::tempdir()?;
    /// let script = r#"#!/bin/sh
    /// case "$*" in
    ///   *'If-None-Match: W/"v1"'*)
    ///     printf 'HTTP/2.0 304 Not Modified\r\nEtag: W/"v1"\r\n\r\n'
    ///     echo 'gh: HTTP 304' >&2
    ///     exit 1 ;;
    ///   *'If-None-Match'*) printf 'HTTP/2.0 200 OK\r\nEtag: W/"v2"\r\n\r\n[]\n' ;;
    ///   *) printf 'HTTP/2.0 200 OK\r\nEtag: W/"v1"\r\n\r\n[]\n' ;;
    /// esac
    /// "#;
    /// let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script));
    /// let path = "repos/o/r/issues?state=all&sort=updated&direction=desc&per_page=1";
    /// // The first ask has no `ETag`: the answer is new, with its `ETag`.
    /// assert_eq!(gh.ask(path, None)?, Answer::Changed { etag: Some(r#"W/"v1""#.into()) });
    /// // Nothing changed: a `304` is no fault.
    /// assert_eq!(gh.ask(path, Some(r#"W/"v1""#))?, Answer::Same);
    /// // Something changed: the answer has the new `ETag`.
    /// let changed = gh.ask(path, Some(r#"W/"v0""#))?;
    /// assert_eq!(changed, Answer::Changed { etag: Some(r#"W/"v2""#.into()) });
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn ask(&self, path: &str, etag: Option<&str>) -> Result<Answer> {
        let header = etag.map(|etag| format!("If-None-Match: {etag}"));
        let mut args = vec!["api", "-i"];
        if let Some(header) = &header {
            args.extend(["-H", header]);
        }
        args.push(path);
        let (ok, stdout, stderr) = self.output(&args, None)?;
        if ok {
            return Ok(Answer::Changed {
                etag: etag_of(&stdout),
            });
        }
        if http_status(&stdout) == Some("304") || stderr.contains("HTTP 304") {
            return Ok(Answer::Same);
        }
        let status = http_status(&stdout)
            .map(|s| format!("HTTP {s}: "))
            .unwrap_or_default();
        bail!("gh api {path}: {status}{}", text::forge(stderr.trim()))
    }

    /// The wait of the limit of GitHub now, when this `gh` counts its calls
    /// ([`Gh::with_calls`]) and a resource waits for its reset (#1557).
    /// With two waits, the one that ends last.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use riff::ghcalls::{Calls, Resource};
    ///
    /// let dir = tempfile::tempdir()?;
    /// let log = dir.path().join("log");
    /// let script = format!(
    ///     "#!/bin/sh\necho \"$*\" >> {}\n\
    ///      case \"$1 $2\" in\n\
    ///      \"api rate_limit\") echo '{{\"resources\":{{\"graphql\":{{\"remaining\":0,\"reset\":4102444800}}}}}}' ;;\n\
    ///      *) echo 'GraphQL: API rate limit already exceeded for user ID 1.' >&2; exit 1 ;;\n\
    ///      esac\n",
    ///     log.display(),
    /// );
    /// let calls = Arc::new(Calls::default());
    /// let gh = riff::pr::Gh::at(isolated::write_program(dir.path().join("gh"), &script))
    ///     .with_calls(calls.clone());
    /// assert!(gh.limit_wait().is_none());
    /// // The call that finds the limit out fails, and starts the wait.
    /// assert!(gh.run(&["pr", "view", "40"], None).is_err());
    /// assert_eq!(gh.limit_wait().unwrap().resource, Resource::Graphql);
    /// // Until the reset, a call of the resource fails with no call of `gh`.
    /// let error = gh.run(&["pr", "view", "41"], None).unwrap_err();
    /// assert!(error.downcast_ref::<riff::ghcalls::Limited>().is_some(), "{error:#}");
    /// assert_eq!(std::fs::read_to_string(&log)?.lines().count(), 2, "one call and `rate_limit`");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn limit_wait(&self) -> Option<ghcalls::Wait> {
        self.calls.as_ref()?.limit_wait(crate::daemon::now_ms())
    }

    /// The error for a call of `gh ARGS` that riff does not make: its
    /// resource waits for the reset of the limit. A `gh` that counts no
    /// calls waits for nothing.
    fn refused(&self, args: &[&str]) -> Option<ghcalls::Limited> {
        let calls = self.calls.as_ref()?;
        let resource = ghcalls::Resource::of(args)?;
        calls
            .waiting(resource, crate::daemon::now_ms())
            .map(ghcalls::Limited)
    }

    /// Starts the wait of the resource of the call `gh ARGS` that failed
    /// with `stdout` and `stderr`, when `stderr` says that its limit is out
    /// (#1557). The reset comes from [`Gh::wait_for`].
    fn note_limit(&self, args: &[&str], stdout: &str, stderr: &str) {
        let (Some(calls), Some(resource)) = (&self.calls, ghcalls::Resource::of(args)) else {
            return;
        };
        let now = crate::daemon::now_ms();
        if !ghcalls::is_limit(stderr) || calls.waiting(resource, now).is_some() {
            return;
        }
        calls.start_wait(self.wait_for(resource, stdout, stderr, now), now);
    }

    /// The wait for the limit of `resource` that the call with `stdout` and
    /// `stderr` found out at `now`. GitHub keeps more than one counter for a
    /// token, and a read can answer from a counter that has points while
    /// the call failed on the counter that is out (#1581). So the reset
    /// comes from the call that failed, in this order:
    ///
    /// 1. The header `x-ratelimit-reset` in its answer.
    /// 2. A secondary limit waits [`ghcalls::SECONDARY_WAIT_MS`].
    /// 3. A counter that shows no points left: `gh api rate_limit`, and
    ///    for GraphQL some reads of `rateLimit` ([`ghcalls::PROBES`]).
    /// 4. Else the latest reset that it saw, the end of the hour of that
    ///    counter, as a guess ([`ghcalls::Wait::guessed`]). With no reading
    ///    at all the guess is [`ghcalls::UNREAD_WAIT_MS`].
    ///
    /// ```
    /// use std::sync::Arc;
    /// use riff::ghcalls::{Calls, Resource};
    ///
    /// let dir = tempfile::tempdir()?;
    /// // The counter of `rate_limit` has points left. The first read of
    /// // GraphQL lands on it too, the second on the counter that is out.
    /// let script = "#!/bin/sh\n\
    ///      case \"$1 $2\" in\n\
    ///      \"api rate_limit\") echo '{\"resources\":{\"graphql\":{\"remaining\":3204,\"reset\":4102444849}}}' ;;\n\
    ///      \"api graphql\")\n\
    ///        if [ -e \"$0.seen\" ]; then echo '{\"data\":{\"rateLimit\":{\"remaining\":0,\"resetAt\":\"2100-01-01T00:00:15Z\"}}}'\n\
    ///        else touch \"$0.seen\"; echo '{\"data\":{\"rateLimit\":{\"remaining\":3204,\"resetAt\":\"2100-01-01T00:00:49Z\"}}}'; fi ;;\n\
    ///      *) echo 'GraphQL: API rate limit already exceeded for user ID 1.' >&2; exit 1 ;;\n\
    ///      esac\n";
    /// let gh = riff::pr::Gh::at(isolated::write_program(dir.path().join("gh"), script))
    ///     .with_calls(Arc::new(Calls::default()));
    /// assert!(gh.run(&["pr", "view", "40"], None).is_err());
    /// let wait = gh.limit_wait().unwrap();
    /// assert_eq!(wait.resource, Resource::Graphql);
    /// // The reset of the counter that is out, not the one of `rate_limit`.
    /// assert_eq!(wait.reset_ms, 4_102_444_815_000);
    /// assert!(!wait.guessed);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    fn wait_for(
        &self,
        resource: ghcalls::Resource,
        stdout: &str,
        stderr: &str,
        now: i64,
    ) -> ghcalls::Wait {
        let known = |reset_ms: i64| ghcalls::Wait::until(resource, reset_ms.max(now));
        if let Some(reset_ms) =
            ghcalls::header_reset(stdout).or_else(|| ghcalls::header_reset(stderr))
        {
            return known(reset_ms);
        }
        if ghcalls::is_secondary(stderr) {
            return known(now + ghcalls::SECONDARY_WAIT_MS);
        }
        // The latest reset of a counter that has points left.
        let mut seen: Option<i64> = None;
        let mut out = |counter: Option<ghcalls::Counter>| {
            let counter = counter?;
            seen = seen.max(Some(counter.reset_ms));
            (counter.remaining == 0).then_some(counter.reset_ms)
        };
        if let Some(reset_ms) = out(self.read_counter(resource)) {
            return known(reset_ms);
        }
        if resource == ghcalls::Resource::Graphql {
            for _ in 0..ghcalls::PROBES {
                if let Some(reset_ms) = out(self.probe()) {
                    return known(reset_ms);
                }
            }
        }
        ghcalls::Wait {
            resource,
            reset_ms: seen.map_or(now + ghcalls::UNREAD_WAIT_MS, |reset_ms| reset_ms.max(now)),
            guessed: true,
        }
    }

    /// The counter of `resource` in `gh api rate_limit`, which no limit
    /// counts.
    fn read_counter(&self, resource: ghcalls::Resource) -> Option<ghcalls::Counter> {
        let (_, out, _) = self.spawn(&["api", "rate_limit"], None).ok()?;
        ghcalls::counter_in(&out, resource)
    }

    /// The counter that one read of `rateLimit` of GraphQL lands on. It counts
    /// as a call, and costs no point: GitHub charges nothing for a query that
    /// asks for `rateLimit` alone (#1607).
    fn probe(&self) -> Option<ghcalls::Counter> {
        let query = format!("query={}", ghcalls::PROBE_QUERY);
        let args = ["api", "graphql", "-f", query.as_str()];
        let (_, out, _) = self.spawn(&args, None).ok()?;
        if let Some(calls) = &self.calls {
            let points = ghcalls::points_of(&args, &out);
            calls.record(crate::daemon::now_ms(), self.caller, "api graphql", points);
        }
        ghcalls::probe_in(&out)
    }

    /// Runs `gh ARGS`, and runs it again after a 502, 503 or 504 of
    /// GitHub when this `gh` retries ([`Gh::retrying`]). The stderr of a
    /// call that still fails ends with the number of tries. A call that
    /// fails on a limit of GitHub fails with [`ghcalls::LimitOut`] when
    /// this `gh` stops on a limit ([`Gh::stopping_on_limit`]).
    fn output(&self, args: &[&str], input: Option<&str>) -> Result<(bool, String, String)> {
        let result = self.tried(args, input)?;
        if self.stops_on_limit
            && let (false, stdout, stderr) = &result
            && ghcalls::is_limit(stderr)
            && let Some(resource) = ghcalls::Resource::of(args)
        {
            let wait = self.wait_for(resource, stdout, stderr, crate::daemon::now_ms());
            return Err(ghcalls::LimitOut(wait).into());
        }
        Ok(result)
    }

    /// [`Gh::output`] with the new tries after a 502, 503 or 504.
    fn tried(&self, args: &[&str], input: Option<&str>) -> Result<(bool, String, String)> {
        let mut wait = self.retry_wait;
        let mut tries = 1;
        loop {
            match self.counted(args, input) {
                Ok((false, stdout, stderr)) if gateway_fault(&stdout, &stderr) => {
                    if tries > self.retries {
                        let stderr = match tries {
                            1 => stderr,
                            _ => format!("{}\n{}", stderr.trim_end(), text::gh_tried(tries)),
                        };
                        return Ok((false, stdout, stderr));
                    }
                    std::thread::sleep(wait);
                    wait = wait.saturating_mul(2);
                    tries += 1;
                }
                result => return result,
            }
        }
    }

    /// Runs `gh ARGS` one time, and counts the call ([`Gh::with_calls`]).
    /// While the limit of its resource is out, it makes no call and fails
    /// with [`ghcalls::Limited`].
    fn counted(&self, args: &[&str], input: Option<&str>) -> Result<(bool, String, String)> {
        if let Some(limited) = self.refused(args) {
            return Err(limited.into());
        }
        let result = self.spawn(args, input);
        if let Some(calls) = &self.calls {
            let now = crate::daemon::now_ms();
            let kind = ghcalls::kind_of(args);
            let stdout = result.as_ref().map_or("", |(_, out, _)| out.as_str());
            calls.record(now, self.caller, &kind, ghcalls::points_of(args, stdout));
            if kind == "api graphql"
                && stdout.contains("\"rateLimit\"")
                && let Some(left) = ghcalls::left_in(stdout, now)
            {
                calls.keep_left(left);
            }
        }
        if let Some(tally) = &self.tally
            && ghcalls::Resource::of(args).is_some()
        {
            let stdout = result.as_ref().map_or("", |(_, out, _)| out.as_str());
            tally.record(
                self.caller,
                &ghcalls::kind_of(args),
                ghcalls::points_of(args, stdout),
            );
        }
        if let Ok((false, stdout, stderr)) = &result {
            self.note_limit(args, stdout, stderr);
        }
        result
    }

    /// Runs `gh ARGS`: whether it exits with status 0, its stdout and its
    /// stderr. With a time limit, it fails at the end of the limit
    /// ([`Gh::with_limit`]).
    fn spawn(&self, args: &[&str], input: Option<&str>) -> Result<(bool, String, String)> {
        use std::os::unix::process::CommandExt;
        let mut command = Command::new(&self.program);
        if let Some(repo) = &self.repo {
            command.env("GH_REPO", repo);
        }
        if let Some(token) = self.token() {
            command.env("GH_TOKEN", token.as_str());
        }
        if self.tally.is_some() {
            command.env(crate::gh_wrapper::COUNTED_VAR, "1");
        }
        command
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if self.limit.is_some() {
            // A group of its own: the stop at the end of the limit also
            // stops each process that `gh` started.
            command.process_group(0);
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("cannot run {}", self.program.display()))?;
        let Some(limit) = self.limit else {
            if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
                stdin.write_all(input.as_bytes())?;
            }
            let out = child.wait_with_output()?;
            return Ok((
                out.status.success(),
                String::from_utf8_lossy(&out.stdout).into_owned(),
                String::from_utf8_lossy(&out.stderr).into_owned(),
            ));
        };
        let end = Instant::now() + limit;
        // The pipes go on threads: a `gh` that does not read or that
        // writes much blocks no step of the wait.
        let writer = child
            .stdin
            .take()
            .zip(input.map(str::to_owned))
            .map(|(mut stdin, input)| {
                std::thread::spawn(move || {
                    let _ = stdin.write_all(input.as_bytes());
                })
            });
        let stdout = read_all(child.stdout.take());
        let stderr = read_all(child.stderr.take());
        let status: ExitStatus = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= end {
                if let Ok(group) = i32::try_from(child.id()) {
                    let _ = nix::sys::signal::killpg(
                        nix::unistd::Pid::from_raw(group),
                        nix::sys::signal::Signal::SIGKILL,
                    );
                }
                let _ = child.wait();
                bail!(text::gh_no_answer(&args.join(" "), limit));
            }
            std::thread::sleep(LIMIT_POLL);
        };
        if let Some(writer) = writer {
            let _ = writer.join();
        }
        let text = |pipe: JoinHandle<Vec<u8>>| {
            String::from_utf8_lossy(&pipe.join().unwrap_or_default()).into_owned()
        };
        Ok((status.success(), text(stdout), text(stderr)))
    }
}

/// The replies of the reads that the loops of the daemon share
/// ([`Gh::read`], #1549). The wave, its look and its watches read the same
/// pull requests and issues: one reply serves the loops that ask within the
/// age they accept.
mod replies {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex, PoisonError};
    use std::time::{Duration, Instant};

    /// How long a reply stays, whatever age a read accepts.
    const KEEP: Duration = Duration::from_secs(3600);

    static REPLIES: LazyLock<Mutex<HashMap<String, (Instant, String)>>> =
        LazyLock::new(Mutex::default);

    /// The reply of `key`, when it is not older than `age`. With an age of
    /// zero there is none.
    pub(super) fn get(key: &str, age: Duration) -> Option<String> {
        if age.is_zero() {
            return None;
        }
        let replies = REPLIES.lock().unwrap_or_else(PoisonError::into_inner);
        let (at, reply) = replies.get(key)?;
        (at.elapsed() <= age).then(|| reply.clone())
    }

    /// Makes the reply of `key`, when there is one, as new as now.
    pub(super) fn renew(key: &str) {
        let mut replies = REPLIES.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((at, _)) = replies.get_mut(key) {
            *at = Instant::now();
        }
    }

    /// Keeps `reply` for `key`, and forgets the replies older than [`KEEP`].
    pub(super) fn put(key: String, reply: &str) {
        let mut replies = REPLIES.lock().unwrap_or_else(PoisonError::into_inner);
        replies.retain(|_, (at, _)| at.elapsed() < KEEP);
        replies.insert(key, (Instant::now(), reply.to_owned()));
    }
}

/// Reads each byte of `pipe` on a thread of its own.
fn read_all(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

/// The body of a pull request for the issue of `link` in the form of the
/// hygiene check: the link line, the summary, and the trailers.
///
/// It adds the link line and each trailer only when `summary` does not
/// have it. It refuses a link line, or a trailer `Issue:` or
/// `Milestone:`, of `summary` that is not the one of the pull request,
/// and names the line (01M3W2627GYXR8CFW76KB6CB9W). The trailer
/// `Milestone:` names a wave by its number (01M518A07QF1YACBYT91M0DD1R),
/// and a trailer of the summary with the same number is the one of the
/// pull request.
///
/// ```
/// use hygiene::{Issue, Link, PullRequest};
///
/// let body = riff::pr::body(Link::Closes(12), "Show the wave.\n", "Wave 3").unwrap();
/// assert_eq!(body, "Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n");
/// let pr = PullRequest::new("Show the wave", &body, Some("Wave 3"));
/// assert!(hygiene::check_pr(&pr, Some(&Issue::new(12, "OPEN", Some("Wave 3")))).is_empty());
///
/// // A wave with a name: the trailer has only its number.
/// let body = riff::pr::body(Link::Closes(12), "Show the wave.\n", "Wave 3: Cloud").unwrap();
/// assert_eq!(body, "Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n");
/// let named = "Text.\n\nMilestone: Wave 3: Old name\n";
/// assert!(riff::pr::body(Link::Closes(12), named, "Wave 3: Cloud").is_ok());
///
/// let body = riff::pr::body(Link::Refs(12), "", "Wave 3").unwrap();
/// assert_eq!(body, "Refs #12\n\nIssue: #12\nMilestone: Wave 3\n");
///
/// // A summary that is a full body stays as it is.
/// let full = "Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n";
/// assert_eq!(riff::pr::body(Link::Closes(12), full, "Wave 3").unwrap(), full);
///
/// // A line for another issue is refused.
/// let other = riff::pr::body(Link::Closes(12), "Text.\n\nIssue: #9\n", "Wave 3");
/// assert_eq!(
///     other.unwrap_err().to_string(),
///     "the body has the line `Issue: #9`, but this pull request needs `Issue: #12`. \
///      Change the line, or remove it."
/// );
/// ```
pub fn body(link: hygiene::Link, summary: &str, milestone: &str) -> Result<String> {
    body_for(link, summary, Some(milestone))
}

/// The body of a pull request like [`body`]. With `None` for the
/// milestone, it has no trailer `Milestone:`: the body of a repository
/// that asks only for the rules that riff reads
/// (01M5SMSHNQ9E4P9W6RCB8E1WTN). A trailer `Milestone:` of `summary`
/// stays as it is then.
///
/// ```
/// use hygiene::Link;
///
/// let body = riff::pr::body_for(Link::Closes(12), "Show the wave.\n", None).unwrap();
/// assert_eq!(body, "Closes #12\n\nShow the wave.\n\nIssue: #12\n");
///
/// // A line for another issue is refused, as in `body`.
/// assert!(riff::pr::body_for(Link::Closes(12), "Issue: #9\n", None).is_err());
/// ```
pub fn body_for(link: hygiene::Link, summary: &str, milestone: Option<&str>) -> Result<String> {
    let n = link.issue();
    let line = match link {
        hygiene::Link::Closes(_) => format!("Closes #{n}"),
        hygiene::Link::Refs(_) => format!("Refs #{n}"),
    };
    let summary = summary.replace("\r\n", "\n");
    let summary = summary.trim();

    let mut linked = false;
    for found in summary
        .lines()
        .filter(|l| hygiene::Link::parse(l).is_some())
    {
        if hygiene::Link::parse(found) != Some(link) {
            let found = found.trim();
            let refs = match link {
                hygiene::Link::Closes(_) => "Give --refs for a line `Refs #N`. ",
                hygiene::Link::Refs(_) => "Give no --refs for a line `Closes #N`. ",
            };
            bail!(
                "the body has the line `{found}`, but this pull request needs `{line}`. \
                 {refs}Change the line, or remove it."
            );
        }
        linked = true;
    }

    let trailers = hygiene::trailers(summary);
    let mut missing = String::new();
    let issue = format!("#{n}");
    let mut wanted = vec![("Issue", issue.clone())];
    if let Some(milestone) = milestone {
        wanted.push(("Milestone", hygiene::milestone_value(milestone)));
    }
    for (key, want) in wanted {
        let fits = |value: &str| match (key, milestone) {
            ("Milestone", Some(milestone)) => hygiene::names_milestone(value, milestone),
            _ => value == issue,
        };
        let mut values = trailers.iter().filter(|(k, _)| k == key).map(|(_, v)| v);
        if let Some(value) = values.find(|value| !fits(value)) {
            bail!(
                "the body has the line `{key}: {value}`, but this pull request needs \
                 `{key}: {want}`. Change the line, or remove it."
            );
        }
        if !trailers.iter().any(|(k, _)| k == key) {
            missing.push_str(&format!("{key}: {want}\n"));
        }
    }

    let mut body = String::new();
    if !linked {
        body.push_str(&format!("{line}\n\n"));
    }
    if !summary.is_empty() {
        body.push_str(summary);
        body.push('\n');
        // A new trailer joins the trailers of the summary, in its last
        // paragraph. With none, the trailers are a new paragraph.
        if trailers.is_empty() && !missing.is_empty() {
            body.push('\n');
        }
    }
    body.push_str(&missing);
    Ok(body)
}

/// The issue of the claims of a session: the one claim `issue-N`.
///
/// ```
/// let claims = ["verify-issue-3".to_owned(), "issue-12".to_owned()];
/// assert_eq!(riff::pr::claimed_issue(&claims).unwrap(), 12);
/// assert!(riff::pr::claimed_issue(&[]).is_err());
/// let two = ["issue-1".to_owned(), "issue-2".to_owned()];
/// assert!(riff::pr::claimed_issue(&two).is_err());
/// ```
pub fn claimed_issue(claims: &[String]) -> Result<u64> {
    let issues: Vec<u64> = claims
        .iter()
        .filter_map(|c| c.strip_prefix("issue-")?.parse().ok())
        .collect();
    match issues[..] {
        [n] => Ok(n),
        [] => bail!("this session holds no claim issue-N. Claim the issue, or give --issue N."),
        _ => bail!("this session holds more than one issue. Give --issue N."),
    }
}

/// Refuses a pull request that breaks a rule of `rules`, with the error
/// of each broken rule. `riff pr open` runs it before `gh pr create`, so a
/// body that `hygiene pr` would fail opens nothing
/// (01M6H3EK1J3DEEV09FAKKKFPEB).
///
/// A line `Tests: ...` at the end of the summary is a trailer line. When
/// it has more than 72 characters, the merge wraps it, and the rule
/// `trailer-wrap` refuses it. The same text as a plain sentence passes.
/// Only `rules = "riff"` has this rule.
///
/// ```
/// use hygiene::{Issue, Link, Rules};
///
/// let issue = Issue::new(12, "OPEN", Some("Wave 3"));
/// let tests = format!("{}done", "unit tests pass, ".repeat(17));
/// let line = format!("Tests: {tests}");
/// assert_eq!(line.chars().count(), 300);
///
/// let body = riff::pr::body(Link::Closes(12), &format!("Show it.\n\n{line}\n"), "Wave 3")?;
/// let error = riff::pr::check(Rules::All, "Show it", &body, &issue).unwrap_err();
/// assert!(error.to_string().contains("trailer-wrap: the trailer line `Tests: "), "{error}");
/// assert!(error.to_string().contains("has 300 characters"), "{error}");
///
/// // The same text as a plain sentence.
/// let body = riff::pr::body(Link::Closes(12), &format!("Show it.\n\nI ran the tests: {tests}\n"), "Wave 3")?;
/// assert!(riff::pr::check(Rules::All, "Show it", &body, &issue).is_ok());
///
/// // A repository that is not riff has no such rule.
/// let body = riff::pr::body_for(Link::Closes(12), &format!("Show it.\n\n{line}\n"), None)?;
/// assert!(riff::pr::check(Rules::Read, "Show it", &body, &issue).is_ok());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn check(rules: hygiene::Rules, title: &str, body: &str, issue: &hygiene::Issue) -> Result<()> {
    let milestone = issue.milestone.as_ref().map(|m| m.title.as_str());
    let errors = hygiene::check_pr_for(
        rules,
        &hygiene::PullRequest::new(title, body, milestone),
        Some(issue),
    );
    if errors.is_empty() {
        return Ok(());
    }
    let lines: Vec<String> = errors.iter().map(ToString::to_string).collect();
    bail!(
        "the pull request breaks issue hygiene:\n{}",
        lines.join("\n")
    )
}

/// Opens the pull request of the current branch for issue `issue`, and
/// turns on auto-merge with a squash (01M3NB6FTGPD0S5JTXXXNGNNDT). The
/// pull request is against the default branch. Returns its number and
/// its URL.
///
/// `rules` are the rules of the hygiene check that it applies
/// ([`crate::hygiene_rules`]). With [`hygiene::Rules::All`] the issue
/// needs a milestone, the body has the trailer `Milestone:`, and the
/// pull request gets that milestone. With [`hygiene::Rules::Read`] the
/// issue needs none: the body has the link line and the trailer
/// `Issue:`, and the pull request gets the milestone of the issue only
/// when the issue has one (01M5SMSHNQ9E4P9W6RCB8E1WTN).
pub fn open(
    gh: &Gh,
    title: &str,
    summary: &str,
    issue: u64,
    refs: bool,
    rules: hygiene::Rules,
) -> Result<(u64, String)> {
    let n = issue.to_string();
    let found: hygiene::Issue =
        gh.json(&["issue", "view", &n, "--json", "number,state,milestone"])?;
    let milestone = found.milestone.as_ref().map(|m| m.title.clone());
    if milestone.is_none() && rules == hygiene::Rules::All {
        bail!("issue #{issue} has no milestone. Ask the lead to put it in a wave.");
    }
    // The milestone goes into the body and into `gh pr create`: a text
    // of the forge that `text::forge` changes is refused whole.
    if let Some(milestone) = &milestone
        && text::forge(milestone) != *milestone
    {
        bail!(
            "the milestone of issue #{issue} ({}) has a control character or is too long. Ask \
             the lead to rename it.",
            text::forge(milestone)
        );
    }
    let link = if refs {
        hygiene::Link::Refs(issue)
    } else {
        hygiene::Link::Closes(issue)
    };
    let wave = milestone
        .as_deref()
        .filter(|_| rules == hygiene::Rules::All);
    let body = body_for(link, summary, wave)?;
    check(rules, title, &body, &found)?;
    let mut args = vec!["pr", "create", "--title", title];
    if let Some(milestone) = &milestone {
        args.extend(["--milestone", milestone]);
    }
    args.extend(["--body-file", "-"]);
    let url = text::forge(gh.run(&args, Some(&body))?.trim());
    let number = url
        .rsplit('/')
        .next()
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or_else(|| anyhow!("gh pr create printed no pull request URL: {url}"))?;
    let pr = number.to_string();
    gh.run(&["pr", "merge", &pr, "--auto", "--squash"], None)
        .with_context(|| {
            format!("pull request #{number} is open, but auto-merge is off. Run: gh pr merge --auto --squash {number}")
        })?;
    Ok((number, url))
}

/// Where a pull request is on its way to the merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wait {
    /// Merged, with the merge commit.
    Merged(String),
    /// Closed with no merge.
    Closed,
    /// Each required check that failed.
    Failed(Vec<String>),
    /// Open, and no required check failed.
    Open,
}

/// A pull request, as `gh pr view --json state,mergeCommit` gives it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrState {
    /// `OPEN`, `CLOSED` or `MERGED`.
    pub state: String,
    /// The merge commit, when it is merged.
    pub merge_commit: Option<Commit>,
}

/// A commit, as `gh --json` gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct Commit {
    /// The full hash.
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub oid: String,
}

/// A check, as `gh pr checks --json name,bucket` gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct Check {
    /// The name, for example `Gate`.
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub name: String,
    /// `pass`, `fail`, `pending`, `skipping` or `cancel`.
    pub bucket: String,
}

/// Where the pull request is, from its state and its required checks.
///
/// ```
/// use riff::pr::{Check, Commit, PrState, Wait, wait_state};
///
/// let open = PrState { state: "OPEN".into(), merge_commit: None };
/// let check = |name: &str, bucket: &str| Check { name: name.into(), bucket: bucket.into() };
/// assert_eq!(wait_state(&open, &[check("Gate", "pending")]), Wait::Open);
/// assert_eq!(
///     wait_state(&open, &[check("Gate", "pass"), check("riff/verify", "fail")]),
///     Wait::Failed(vec!["riff/verify".into()])
/// );
/// let merged = PrState { state: "MERGED".into(), merge_commit: Some(Commit { oid: "1a2b".into() }) };
/// assert_eq!(wait_state(&merged, &[]), Wait::Merged("1a2b".into()));
/// let closed = PrState { state: "CLOSED".into(), merge_commit: None };
/// assert_eq!(wait_state(&closed, &[]), Wait::Closed);
/// ```
pub fn wait_state(pr: &PrState, checks: &[Check]) -> Wait {
    match (pr.state.as_str(), &pr.merge_commit) {
        ("MERGED", Some(commit)) => return Wait::Merged(commit.oid.clone()),
        ("CLOSED", _) => return Wait::Closed,
        _ => {}
    }
    let failed: Vec<String> = checks
        .iter()
        .filter(|c| matches!(c.bucket.as_str(), "fail" | "cancel"))
        .map(|c| c.name.clone())
        .collect();
    if failed.is_empty() {
        Wait::Open
    } else {
        Wait::Failed(failed)
    }
}

/// How long `riff pr wait` looks each `every`, from its start. After
/// that it looks each [`SLOW_EVERY`] (01M6JE9ME6YSHV538GNPH7ZRGQ).
pub const FAST_WAIT: Duration = Duration::from_secs(120);

/// The time between two looks of `riff pr wait` after [`FAST_WAIT`].
/// `--every` can make it longer, never shorter.
pub const SLOW_EVERY: Duration = Duration::from_secs(300);

/// The GraphQL points of one look: `pr view` costs 1 and `pr checks`
/// costs 2 ([`crate::ghcalls`]).
pub const LOOK_POINTS: u32 = 3;

/// The most GraphQL points that `riff pr wait` uses in one hour for one
/// pull request at the default `--every` of 30 seconds
/// (01M6JE9ME6YSHV538GNPH7ZRGQ). The limit of a token is 5,000 an hour
/// for all its sessions.
pub const WAIT_POINTS_AN_HOUR: u32 = 60;

/// How long `riff pr wait` waits after a look, when it waits `elapsed`
/// already: `every` in the first [`FAST_WAIT`], then [`SLOW_EVERY`], or
/// `every` when that is longer (01M6JE9ME6YSHV538GNPH7ZRGQ).
///
/// ```
/// use std::time::Duration;
/// use riff::pr::{FAST_WAIT, LOOK_POINTS, WAIT_POINTS_AN_HOUR, pace};
///
/// let secs = Duration::from_secs;
/// let every = secs(30);
/// assert_eq!(pace(every, secs(0)), secs(30));
/// assert_eq!(pace(every, FAST_WAIT - secs(1)), secs(30));
/// assert_eq!(pace(every, FAST_WAIT), secs(300));
/// assert_eq!(pace(secs(600), FAST_WAIT), secs(600));
///
/// // The looks of the first hour.
/// let looks_in_an_hour = |every| {
///     let (mut at, mut looks) = (secs(0), 0);
///     while at < secs(3600) {
///         looks += 1;
///         at += pace(every, at);
///     }
///     looks
/// };
///
/// // The default pace has the most looks in the first hour: 5 fast
/// // ones, then 11 slow ones. It stays under the bound.
/// assert_eq!(looks_in_an_hour(secs(30)), 16);
/// assert!(16 * LOOK_POINTS <= WAIT_POINTS_AN_HOUR);
///
/// // A shorter `--every` breaks the bound: 24 looks, 72 points.
/// assert_eq!(looks_in_an_hour(secs(10)), 24);
/// assert_eq!(24 * LOOK_POINTS, 72);
/// assert!(24 * LOOK_POINTS > WAIT_POINTS_AN_HOUR);
/// ```
pub fn pace(every: Duration, elapsed: Duration) -> Duration {
    if elapsed < FAST_WAIT {
        every
    } else {
        every.max(SLOW_EVERY)
    }
}

/// Looks at pull request `number` until it is merged
/// (01M3NB6FWMGBQ9VTY6RCBPKBHK), at the [`pace`] of `every`. Returns the
/// merge commit. Fails when it closes unmerged or a required check
/// fails.
///
/// A look of `gh` can fail for a short time, for example while the
/// network is away. After one good look, a failed look does not end the
/// wait (01M3Z8GG5EGEYAVEXG0HS46ACT): it prints
/// [`text::pr_look_failed`] on stderr, one time until the next good
/// look, and looks again after the [`pace`]. When the first look fails,
/// the wait ends with its error: `gh` cannot see the pull request.
pub fn wait(gh: &Gh, number: u64, every: Duration) -> Result<String> {
    wait_with(gh, number, every, std::thread::sleep)
}

/// [`wait`] with `sleep` for each wait between two looks. A test gives a
/// `sleep` that moves a clock and does not wait.
pub fn wait_with(
    gh: &Gh,
    number: u64,
    every: Duration,
    mut sleep: impl FnMut(Duration),
) -> Result<String> {
    let n = number.to_string();
    let look = || {
        let pr: PrState = gh.json(&["pr", "view", &n, "--json", "state,mergeCommit"])?;
        // With no required check, `gh pr checks` prints no JSON.
        let (_, out, _) = gh.output(
            &["pr", "checks", &n, "--required", "--json", "name,bucket"],
            None,
        )?;
        let checks: Vec<Check> = serde_json::from_str(&out).unwrap_or_default();
        anyhow::Ok(wait_state(&pr, &checks))
    };
    let mut looked = false;
    let mut told = false;
    let mut elapsed = Duration::ZERO;
    loop {
        let next = pace(every, elapsed);
        match look() {
            Ok(Wait::Merged(commit)) => return Ok(commit),
            Ok(Wait::Closed) => bail!("pull request #{number} is closed, and not merged"),
            Ok(Wait::Failed(names)) => bail!(
                "pull request #{number} waits no more: a required check failed: {}",
                names.join(", ")
            ),
            Ok(Wait::Open) => {
                looked = true;
                told = false;
            }
            Err(e) if looked => {
                if !told {
                    let error = format!("{e:#}");
                    eprintln!("{}", text::pr_look_failed(number, &error, next.as_secs()));
                    told = true;
                }
            }
            Err(e) => return Err(e),
        }
        sleep(next);
        elapsed += next;
    }
}

/// The result of a verify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// Each criterion passed.
    Pass,
    /// One or more criteria failed.
    Fail,
}

impl Verdict {
    /// `PASS` or `FAIL`.
    pub fn word(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
        }
    }

    /// The state of the commit status: `success` or `failure`.
    pub fn state(self) -> &'static str {
        match self {
            Self::Pass => "success",
            Self::Fail => "failure",
        }
    }
}

/// A pull request, as `gh pr view --json headRefOid,body` gives it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Head {
    /// The head commit.
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub head_ref_oid: String,
    /// The body, with the `Issue:` trailer.
    pub body: String,
}

/// The issue of a body: its `Issue: #N` trailer.
///
/// ```
/// assert_eq!(riff::pr::trailer_issue("Closes #12\n\nIssue: #12\nMilestone: Wave 3\n"), Some(12));
/// assert_eq!(riff::pr::trailer_issue("Closes #12\n"), None);
/// ```
pub fn trailer_issue(body: &str) -> Option<u64> {
    hygiene::trailers(&body.replace("\r\n", "\n"))
        .iter()
        .find(|(key, _)| key == "Issue")
        .and_then(|(_, value)| hygiene::issue_ref(value))
}

/// The issue of the first line `Closes #N` of a body. A pull request
/// that closes an issue is the work of that item, on any branch.
///
/// ```
/// assert_eq!(riff::pr::closes_issue("Closes #12\n\nSummary\n"), Some(12));
/// assert_eq!(riff::pr::closes_issue("Refs #12\n"), None);
/// ```
pub fn closes_issue(body: &str) -> Option<u64> {
    body.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("Closes #")?;
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
}

/// The issue of pull request `number`: the `Issue:` trailer of its body.
pub fn issue_of(gh: &Gh, number: u64) -> Result<u64> {
    #[derive(Deserialize)]
    struct Body {
        body: String,
    }
    let pr: Body = gh.json(&["pr", "view", &number.to_string(), "--json", "body"])?;
    trailer_issue(&pr.body)
        .with_context(|| format!("pull request #{number} has no trailer `Issue: #N`"))
}

/// The comment of a verify on the pull request: it names the commit.
///
/// ```
/// use riff::pr::{Verdict, comment};
/// assert_eq!(
///     comment(Verdict::Pass, 12, "1a2b3c4d", "All good.\n"),
///     "verify result: PASS for issue-12, commit 1a2b3c4d.\n\nAll good.\n"
/// );
/// ```
pub fn comment(verdict: Verdict, issue: u64, commit: &str, result: &str) -> String {
    format!(
        "verify result: {} for issue-{issue}, commit {commit}.\n\n{}\n",
        verdict.word(),
        result.trim_end()
    )
}

/// A verify that is on GitHub: the issue, the head commit and the URL of
/// the comment.
#[derive(Debug, Clone)]
pub struct Reported {
    /// The issue of the `Issue:` trailer.
    pub issue: u64,
    /// The head commit that the status is on.
    pub commit: String,
    /// The URL of the comment.
    pub url: String,
}

/// The sessions that the result of a verify of `issue` wakes: the
/// holder of the issue. When no session of `sessions` holds the issue
/// in the repository of the pull request, also the lead of the user of
/// `me` there (01M3Z9N70J4H79VJN4ZKKH3G6S). `thread` is the thread of
/// the post: the repository of the pull request. The repository of `me`
/// does not count: a person in a terminal has none
/// (01M6FYW8NG0E9HYPWTG2RV5EAM). `sessions` is the list of `who`: it
/// has no session that is gone.
///
/// ```
/// use riff::pr::result_to;
/// use riff_core::name::ThreadName;
/// use riff_core::wire::SessionInfo;
///
/// let info = |uri: &str| SessionInfo {
///     uri: uri.parse().unwrap(),
///     live: true,
///     idle_secs: 0,
///     status: None,
///     worker: true,
///     container: false,
///     binary: None,
///     claims_secs: 0,
///     must_clear: false,
///     fresh_secs: None,
///     state: None,
///     work: None,
///     waits: None,
///     blocked: None,
///     step: None,
///     machine: None,
/// };
/// let thread: ThreadName = "o/r".parse().unwrap();
/// let to = |me: &str, sessions: &[SessionInfo]| -> Vec<String> {
///     let me = me.parse().unwrap();
///     result_to(12, &me, &thread, sessions).iter().map(ToString::to_string).collect()
/// };
/// let session = "riff://mike@pangolin/o/r?session=v1&claim=verify-issue-12";
/// let holder = info("riff://mike@thelio/o/r?session=a1&claim=issue-12");
/// assert_eq!(to(session, &[holder]), ["claim=issue-12"]);
/// // The author released the item, or the claim is in another
/// // repository: the lead wakes.
/// let lead = ["claim=issue-12", "user=mike,repo=o/r,lead=true"];
/// assert_eq!(to(session, &[]), lead);
/// assert_eq!(to(session, &[info("riff://mike@thelio/o/r?session=a1")]), lead);
/// assert_eq!(to(session, &[info("riff://mike@thelio/o/s?session=a1&claim=issue-12")]), lead);
/// // A person in a terminal has no repository: the thread gives it.
/// let person = "riff://mike@pangolin";
/// assert_eq!(to(person, &[]), lead);
/// let holder = info("riff://mike@thelio/o/r?session=a1&claim=issue-12");
/// assert_eq!(to(person, &[holder]), ["claim=issue-12"]);
/// ```
pub fn result_to(
    issue: u64,
    me: &SessionUri,
    thread: &ThreadName,
    sessions: &[SessionInfo],
) -> Vec<Selector> {
    let item = format!("issue-{issue}");
    let repo = thread.to_string();
    let held = sessions
        .iter()
        .any(|s| s.uri.place().repo_text() == repo && s.uri.claims().contains(&item));
    let mut to = vec![Selector {
        claim: Some(item),
        ..Selector::default()
    }];
    if !held {
        to.push(Selector::lead(me.who().user(), &repo));
    }
    to
}

/// The commit `HEAD` of the git worktree `dir`: the commit that the
/// verifier tested.
pub fn head_here(dir: &Path) -> Result<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(dir)
        .output()
        .context("cannot run git")?;
    if !out.status.success() {
        bail!(
            "cannot read HEAD of {}: give the commit that you tested with --commit SHA",
            dir.display()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Checks that the verifier tested the head commit of the pull request:
/// a verify counts only for its commit. `tested` is a full hash, or its
/// first 7 or more characters.
///
/// ```
/// use riff::pr::same_commit;
/// assert!(same_commit("1a2b3c4d5e6f", "1a2b3c4d5e6f").is_ok());
/// assert!(same_commit("1a2b3c4d5e6f", "1a2b3c4").is_ok());
/// assert!(same_commit("1a2b3c4d5e6f", "1a2b").is_err(), "too short");
/// let moved = same_commit("9f8e7d6c5b4a", "1a2b3c4d5e6f").unwrap_err().to_string();
/// assert!(moved.starts_with("you tested 1a2b3c4d5e6f, but the head of the pull request is 9f8e7d6c5b4a"));
/// ```
pub fn same_commit(head: &str, tested: &str) -> Result<()> {
    if tested.len() >= 7 && head.starts_with(tested) {
        return Ok(());
    }
    bail!(
        "you tested {tested}, but the head of the pull request is {head}. A verify counts only \
         for its commit: nothing is reported. Test the head commit, or tell the author."
    )
}

/// The default branch of `repo` (`OWNER/REPO`) on the forge, for example
/// `main`, `master` or `trunk` (01M5QYBWPAJ219BE8QKVKG1TWW). The wave
/// reads it at each look, and keeps it in no file
/// (01M5R56GN3DC09GAPAF69GENEZ).
///
/// ```
/// use riff::pr::{Gh, default_branch};
///
/// let dir = tempfile::tempdir()?;
/// let gh = Gh::at(isolated::write_program(
///     dir.path().join("gh"),
///     "#!/bin/sh\n\
///      case \"$*\" in\n\
///        'api repos/o/r --jq .default_branch') echo master ;;\n\
///        'api repos/o/empty --jq .default_branch') echo null ;;\n\
///        *) exit 1 ;;\n\
///      esac\n",
/// ));
/// assert_eq!(default_branch(&gh, "o/r")?, "master");
/// assert!(default_branch(&gh, "o/empty").is_err());
/// assert!(default_branch(&gh, "o/gone").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn default_branch(gh: &Gh, repo: &str) -> Result<String> {
    let path = format!("repos/{repo}");
    // The default branch changes rarely: the daemon reads it one time in
    // [`Gh::SLOW`] (#1549).
    let branch = gh.read_within(&["api", &path, "--jq", ".default_branch"], Gh::SLOW)?;
    match text::forge(branch.trim()) {
        branch if branch.is_empty() || branch == "null" => {
            bail!("gh api {path}: the repository has no default branch")
        }
        branch => Ok(branch),
    }
}

/// The name of the check run of the full test run of a commit in the
/// repository of riff: the job `Gate` of `.github/workflows/ci.yml`
/// (01M49HAZ5BZYGAR9PGC089RM3F). Another repository has its own checks, and
/// a repository with no CI has no full check: riff invents no check for it
/// ([`repo_setup::full_check_of`]).
pub const GATE: &str = "Gate";

/// A check run, as `gh api repos/R/commits/C/check-runs` gives it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CheckRun {
    /// `queued`, `in_progress` or `completed`.
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub status: String,
    /// `success`, `failure`, `cancelled` and so on, when it is completed.
    #[serde(default, deserialize_with = "crate::text::forge_de_opt")]
    pub conclusion: Option<String>,
    /// The commit of the run.
    #[serde(default, deserialize_with = "crate::text::forge_de")]
    pub head_sha: String,
    /// The page of the job, for example
    /// `https://github.com/o/r/actions/runs/38861280871/job/1`.
    #[serde(default, deserialize_with = "crate::text::forge_de_opt")]
    pub html_url: Option<String>,
    /// When the run started, for example `2026-10-24T17:04:30Z`.
    #[serde(default, deserialize_with = "crate::text::forge_de_opt")]
    pub started_at: Option<String>,
    /// When the run ended, for example `2026-10-24T17:26:50Z`, when it is
    /// completed.
    #[serde(default, deserialize_with = "crate::text::forge_de_opt")]
    pub completed_at: Option<String>,
}

/// The check runs of one commit.
#[derive(Debug, Clone, Deserialize)]
pub struct CheckRuns {
    /// Each run.
    pub check_runs: Vec<CheckRun>,
}

/// The state of the full check of a commit, for example of the [`GATE`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Gate {
    /// Each newest run of the check ([`newest_runs`]) is completed with a
    /// success ([`is_success`]).
    Passed,
    /// The commit has no run of the check.
    #[default]
    Missing,
    /// A newest run of the check is not completed.
    Running,
    /// A newest run of the check is completed with this conclusion, which
    /// is no success.
    Ended(String),
}

/// True when a completed check run with this `conclusion` counts as a
/// success. GitHub counts `skipped` and `neutral` as a pass for a required
/// check, so riff does too (01M5R13DE5XRWB13E0BQ17C80F).
///
/// ```
/// use riff::pr::is_success;
///
/// assert!(["success", "skipped", "neutral"].into_iter().all(is_success));
/// assert!(!["failure", "cancelled", "timed_out", "stale", "action_required"].into_iter().any(is_success));
/// ```
pub fn is_success(conclusion: &str) -> bool {
    matches!(conclusion, "success" | "skipped" | "neutral")
}

/// The newest runs of one check (01M5RGM8HBVQMY25H5PYESK17J): the runs
/// with the latest `started_at`, then the latest `completed_at`, as `gh pr
/// checks` and the merge rules of GitHub read them. A check can run again
/// on one commit, and the old run stays in the list. A run with no time is
/// older than a run with a time. Runs that tie all count, so a failure is
/// never lost for a tie. GitHub gives each time in UTC and in one form, so
/// the text order is the time order.
///
/// ```
/// use riff::pr::{CheckRun, newest_runs};
///
/// let run = |started: &str, conclusion: &str| CheckRun {
///     status: "completed".into(),
///     conclusion: Some(conclusion.into()),
///     started_at: Some(started.into()),
///     ..CheckRun::default()
/// };
/// let runs = [
///     run("2026-10-24T17:00:00Z", "failure"),
///     run("2026-10-24T17:20:00Z", "success"),
/// ];
/// let newest = newest_runs(&runs);
/// assert_eq!(newest.len(), 1);
/// assert_eq!(newest[0].conclusion.as_deref(), Some("success"));
/// assert!(newest_runs(&[]).is_empty());
/// ```
pub fn newest_runs(runs: &[CheckRun]) -> Vec<&CheckRun> {
    newest_by(runs.iter(), |run| {
        (run.started_at.as_deref(), run.completed_at.as_deref())
    })
}

/// The rule of [`newest_runs`] for any list: the items of `items` with the
/// latest time, where `time` gives the pair `(started_at, completed_at)` of
/// an item. The checks of the rollup of a pull request use it too
/// ([`crate::rollout::Pull::failed_checks`]).
///
/// ```
/// use riff::pr::newest_by;
///
/// let runs = [
///     ("old", Some("2026-10-24T17:00:00Z")),
///     ("new", Some("2026-10-24T17:20:00Z")),
///     ("none", None),
/// ];
/// let newest = newest_by(runs.iter(), |(_, started)| (*started, None));
/// assert_eq!(newest, [&("new", Some("2026-10-24T17:20:00Z"))]);
/// ```
pub fn newest_by<'a, T>(
    items: impl Iterator<Item = &'a T> + Clone,
    time: impl Fn(&T) -> (Option<&str>, Option<&str>),
) -> Vec<&'a T> {
    let Some(newest) = items.clone().map(&time).max() else {
        return Vec::new();
    };
    items.filter(|item| time(item) == newest).collect()
}

/// The state of one check from its check runs. A pass needs one run or
/// more, and each newest run ([`newest_runs`]) completed with a success
/// ([`is_success`]). An old run does not count when a newer run of the
/// check is there (01M5RGM8HBVQMY25H5PYESK17J).
///
/// ```
/// use riff::pr::{CheckRun, Gate, gate_state};
///
/// let run = |status: &str, conclusion: Option<&str>| CheckRun {
///     status: status.into(),
///     conclusion: conclusion.map(Into::into),
///     ..CheckRun::default()
/// };
/// assert_eq!(gate_state(&[run("completed", Some("success"))]), Gate::Passed);
/// assert_eq!(gate_state(&[]), Gate::Missing);
/// assert_eq!(gate_state(&[run("in_progress", None)]), Gate::Running);
/// // Two runs with no time tie: both count.
/// assert_eq!(
///     gate_state(&[run("completed", Some("success")), run("completed", Some("failure"))]),
///     Gate::Ended("failure".into())
/// );
/// // A required check that GitHub skipped counts as a pass.
/// assert_eq!(gate_state(&[run("completed", Some("skipped"))]), Gate::Passed);
/// assert_eq!(gate_state(&[run("completed", Some("neutral"))]), Gate::Passed);
/// assert_eq!(
///     gate_state(&[run("completed", Some("cancelled"))]),
///     Gate::Ended("cancelled".into())
/// );
///
/// // The newest run decides, in any order of the list.
/// let at = |started: &str, conclusion: &str| CheckRun {
///     started_at: Some(started.into()),
///     ..run("completed", Some(conclusion))
/// };
/// let failed = at("2026-10-24T17:04:00Z", "failure");
/// let passed = at("2026-10-24T17:26:00Z", "success");
/// assert_eq!(gate_state(&[failed.clone(), passed.clone()]), Gate::Passed);
/// assert_eq!(gate_state(&[passed.clone(), failed.clone()]), Gate::Passed);
/// let failed = at("2026-10-24T17:26:00Z", "failure");
/// let passed = at("2026-10-24T17:04:00Z", "success");
/// assert_eq!(gate_state(&[passed.clone(), failed.clone()]), Gate::Ended("failure".into()));
/// assert_eq!(gate_state(&[failed, passed]), Gate::Ended("failure".into()));
/// ```
pub fn gate_state(runs: &[CheckRun]) -> Gate {
    let newest = newest_runs(runs);
    if newest.is_empty() {
        return Gate::Missing;
    }
    for run in newest {
        match (run.status.as_str(), run.conclusion.as_deref()) {
            ("completed", Some(conclusion)) if is_success(conclusion) => {}
            ("completed", conclusion) => {
                return Gate::Ended(conclusion.unwrap_or("none").to_owned());
            }
            _ => return Gate::Running,
        }
    }
    Gate::Passed
}

/// The state of the full check of a commit from the state of each of its
/// checks. A check that runs makes it run. Else a check that ended with no
/// success gives its conclusion. Else a check with no run makes it missing.
///
/// ```
/// use riff::pr::{Gate, full_state};
///
/// assert_eq!(full_state([Gate::Passed, Gate::Passed]), Gate::Passed);
/// assert_eq!(full_state([Gate::Passed, Gate::Missing]), Gate::Missing);
/// assert_eq!(full_state([Gate::Missing, Gate::Running]), Gate::Running);
/// let failed = Gate::Ended("failure".into());
/// assert_eq!(full_state([failed.clone(), Gate::Missing]), failed);
/// assert_eq!(full_state([failed.clone(), Gate::Running]), Gate::Running);
/// assert_eq!(full_state([]), Gate::Passed);
/// ```
pub fn full_state(states: impl IntoIterator<Item = Gate>) -> Gate {
    let mut ended = None;
    let mut missing = false;
    for state in states {
        match state {
            Gate::Passed => {}
            Gate::Running => return Gate::Running,
            Gate::Ended(conclusion) => {
                ended.get_or_insert(conclusion);
            }
            Gate::Missing => missing = true,
        }
    }
    match (ended, missing) {
        (Some(conclusion), _) => Gate::Ended(conclusion),
        (None, true) => Gate::Missing,
        (None, false) => Gate::Passed,
    }
}

/// A name in a GitHub path, with each byte but a letter, a digit and
/// `-_.` as `%XX`: the check `checks / lint and test` has spaces and a
/// slash.
///
/// ```
/// assert_eq!(riff::pr::percent("Gate"), "Gate");
/// assert_eq!(
///     riff::pr::percent("checks / lint and test"),
///     "checks%20%2F%20lint%20and%20test"
/// );
/// ```
pub fn percent(name: &str) -> String {
    let mut out = String::new();
    for byte in name.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// The check runs of the check `check` on `commit` in `repo` with `gh`.
/// `commit` is a commit, or a branch such as `main`.
pub fn check_runs(gh: &Gh, repo: &str, commit: &str, check: &str) -> Result<Vec<CheckRun>> {
    let name = percent(check);
    let path = format!("repos/{repo}/commits/{commit}/check-runs?check_name={name}");
    let runs: CheckRuns = gh.json(&["api", &path])?;
    Ok(runs.check_runs)
}

/// True when the full check is done, from the state of each of its
/// checks ([`gate_state`]): each check has a run, and each newest run is
/// completed, with a success or not (01M5QYDHN27RC5SRHAB56PFKK0). A check
/// with no run has not ended, so a failed check next to it is not done.
/// [`full_state`] gives a failure first: it tells that a check failed,
/// and `full_done` tells that each check ended.
///
/// ```
/// use riff::pr::{Gate, full_done};
///
/// let failed = Gate::Ended("failure".into());
/// assert!(full_done(&[Gate::Passed, Gate::Passed]));
/// assert!(full_done(&[Gate::Passed, failed.clone()]));
/// assert!(full_done(&[]));
/// // A failed check next to a check with no run, or a run that goes.
/// assert!(!full_done(&[failed.clone(), Gate::Missing]));
/// assert!(!full_done(&[Gate::Passed, Gate::Missing]));
/// assert!(!full_done(&[failed, Gate::Running]));
/// ```
pub fn full_done(states: &[Gate]) -> bool {
    states
        .iter()
        .all(|state| matches!(state, Gate::Passed | Gate::Ended(_)))
}

/// The state of each check of `checks` on `commit` in `repo`, in order
/// ([`gate_state`] of its check runs).
pub fn gate_states(gh: &Gh, repo: &str, checks: &[String], commit: &str) -> Result<Vec<Gate>> {
    checks
        .iter()
        .map(|check| Ok(gate_state(&check_runs(gh, repo, commit, check)?)))
        .collect()
}

/// The state of the full check `checks` of `commit` in `repo`
/// ([`repo_setup::full_check`]): the state of the check runs of each name
/// ([`gate_states`]), and the state of all of them ([`full_state`]).
pub fn gate_of(gh: &Gh, repo: &str, checks: &[String], commit: &str) -> Result<Gate> {
    Ok(full_state(gate_states(gh, repo, checks, commit)?))
}

/// The text of the file `path` in `repo` at `git_ref` (a commit or a
/// branch), with `gh`. `None` when the file is not there.
fn file_at(gh: &Gh, repo: &str, git_ref: &str, path: &str) -> Result<Option<String>> {
    let api = format!("repos/{repo}/contents/{path}?ref={git_ref}");
    let args = ["api", "-H", "Accept: application/vnd.github.raw+json", &api];
    let (ok, stdout, stderr) = gh.output(&args, None)?;
    match (ok, stderr.contains("HTTP 404")) {
        (true, _) => Ok(Some(stdout)),
        (false, true) => Ok(None),
        (false, false) => bail!("gh {}: {}", args.join(" "), text::forge(stderr.trim())),
    }
}

/// The session image that pull request `number` of `repo` pins in
/// [`session_image::FILE`] at its head `commit`, when it changes the pin
/// of its base branch (01M5SMRRAG1WPBJMQKKTPSRHRP). `None` when the pull
/// request leaves the file as it is, or deletes it. An error when the
/// file of the head pins no good image.
///
/// ```
/// use riff::pr::{Gh, pin_changed};
///
/// let dir = tempfile::tempdir()?;
/// let digest = |n: &str| format!("sha256:{}", n.repeat(64));
/// let file = |n: &str| format!("[session]\nimage = \"ghcr.io/o/i@{}\"\n", digest(n));
/// std::fs::write(dir.path().join("head"), file("1"))?;
/// std::fs::write(dir.path().join("base"), file("0"))?;
/// let gh = Gh::at(isolated::write_program(
///     dir.path().join("gh"),
///     format!(
///         "#!/bin/sh\n\
///          case \"$*\" in\n\
///            *'/pulls/40/files'*) printf 'src/main.rs\\nriff.toml\\n' ;;\n\
///            'pr view 40 --json baseRefName --jq .baseRefName') echo main ;;\n\
///            *'contents/riff.toml?ref=abc') cat {0}/head ;;\n\
///            *'contents/riff.toml?ref=main') cat {0}/base ;;\n\
///            *) exit 1 ;;\n\
///          esac\n",
///         dir.path().display()
///     ),
/// ));
/// let image = pin_changed(&gh, "o/r", 40, "abc")?.unwrap();
/// assert_eq!(image.digest(), digest("1"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn pin_changed(
    gh: &Gh,
    repo: &str,
    number: u64,
    commit: &str,
) -> Result<Option<session_image::Image>> {
    let files = gh.run(
        &[
            "api",
            &format!("repos/{repo}/pulls/{number}/files"),
            "--paginate",
            "--jq",
            ".[].filename",
        ],
        None,
    )?;
    if !files.lines().any(|name| name == session_image::FILE) {
        return Ok(None);
    }
    let Some(head) = file_at(gh, repo, commit, session_image::FILE)? else {
        return Ok(None);
    };
    let place = format!("{} of pull request #{number}", session_image::FILE);
    let image = session_image::Image::of_config(&head, &place)
        .map_err(|e| anyhow!("{}", text::verify_pin_bad(&format!("{e:#}"))))?;
    let base = gh.run(
        &[
            "pr",
            "view",
            &number.to_string(),
            "--json",
            "baseRefName",
            "--jq",
            ".baseRefName",
        ],
        None,
    )?;
    let old = file_at(gh, repo, base.trim(), session_image::FILE)?
        .and_then(|text| session_image::Image::of_config(&text, "the base").ok());
    Ok((old.as_ref() != Some(&image)).then_some(image))
}

/// Checks that a host can pull the session image that pull request
/// `number` pins, when it changes the pin ([`pin_changed`]): the registry
/// must give the manifest with no login
/// (01M5SMRRAG1WPBJMQKKTPSRHRP).
fn image_pullable(gh: &Gh, repo: &str, number: u64, commit: &str) -> Result<()> {
    let Some(image) = pin_changed(gh, repo, number, commit)? else {
        return Ok(());
    };
    match registry::anonymous_here(&image) {
        Ok(registry::Pull::Open) => Ok(()),
        Ok(registry::Pull::Closed(why)) => {
            bail!("{}", text::verify_image_closed(number, &image, &why))
        }
        Err(e) => bail!(
            "{}",
            text::verify_image_unasked(number, &image, &format!("{e:#}"))
        ),
    }
}

/// Puts the result on pull request `number` as a comment, and sets the
/// status `riff/verify` of its head commit in `repo`
/// (01M3NB6FYXXKX80VHEVA5CV6RY). It reports nothing when the head is
/// not the commit `tested` ([`same_commit`]), and no pass while the full
/// check of the head has no success (01M49HAZ7P3JMWNCG1SWCMAXQP,
/// 01M5QYDE41FVFX26EC8K5QGYH4), or while the session image that the pull
/// request pins is one that a host cannot pull with no login
/// (01M5SMRRAG1WPBJMQKKTPSRHRP). The caller posts it to the holder of the
/// issue.
pub fn report(
    gh: &Gh,
    repo: &str,
    number: u64,
    tested: &str,
    verdict: Verdict,
    result: &str,
) -> Result<Reported> {
    let n = number.to_string();
    let head: Head = gh.json(&["pr", "view", &n, "--json", "headRefOid,body"])?;
    same_commit(&head.head_ref_oid, tested)?;
    let Some(issue) = trailer_issue(&head.body) else {
        bail!("pull request #{number} has no trailer `Issue: #N`");
    };
    let commit = head.head_ref_oid;
    if verdict == Verdict::Pass {
        let checks = repo_setup::full_check(gh, repo)?;
        let gate = gate_of(gh, repo, &checks, &commit)?;
        if gate != Gate::Passed {
            bail!("{}", text::gate_not_passed(number, &commit, &checks, &gate));
        }
        image_pullable(gh, repo, number, &commit)?;
    }
    let text = comment(verdict, issue, &commit, result);
    let url = text::forge(
        gh.run(&["pr", "comment", &n, "--body-file", "-"], Some(&text))?
            .trim(),
    );
    let path = format!("repos/{repo}/statuses/{commit}");
    let state = format!("state={}", verdict.state());
    let description = format!("description={}: verify-issue-{issue}", verdict.word());
    let target = format!("target_url={url}");
    gh.run(
        &[
            "api",
            &path,
            "-f",
            &state,
            "-f",
            &format!("context={VERIFY_CONTEXT}"),
            "-f",
            &description,
            "-f",
            &target,
        ],
        None,
    )?;
    Ok(Reported { issue, commit, url })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use hygiene::{Issue, Link, Rules};

    use super::{CheckRun, Gate, Gh, body, body_for, check, full_done, full_state, gate_state};
    use crate::text;

    const FULL: &str = "Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n";

    /// #1566: a `304` renews the kept reply, so a read that accepts a short
    /// age takes it. A reply that nobody kept, and a `gh` that shares no
    /// read, have nothing to renew.
    #[test]
    fn a_renewed_reply_is_as_new_as_now() {
        let dir = tempfile::tempdir().unwrap();
        let gh = Gh::at(isolated::write_program(
            dir.path().join("gh"),
            "#!/bin/sh\necho '[]'\n",
        ))
        .with_shared_reads();
        let args = ["issue", "list", "--repo", "o/renew"];
        let age = Duration::from_millis(250);
        gh.renew(&args);
        assert!(gh.peek(&args, age).is_none(), "nothing was kept");
        gh.read(&args).unwrap();
        assert!(gh.peek(&args, age).is_some());
        std::thread::sleep(2 * age);
        assert!(gh.peek(&args, age).is_none(), "the reply is old");
        gh.renew(&args);
        assert!(gh.peek(&args, age).is_some(), "the reply is as new as now");
        // A `gh` that shares no read keeps and renews nothing.
        let alone = Gh::at(dir.path().join("gh"));
        alone.renew(&args);
        assert!(alone.peek(&args, age).is_none());
    }

    /// #1566: a conditional read that fails with something other than a
    /// `304` is a fault, with the status when `gh` printed one.
    #[test]
    fn a_conditional_read_that_fails_is_a_fault_and_not_a_304() {
        let dir = tempfile::tempdir().unwrap();
        let script = "#!/bin/sh\n\
            printf 'HTTP/2.0 404 Not Found\\r\\n\\r\\n'\n\
            echo 'gh: Not Found (HTTP 404)' >&2\n\
            exit 1\n";
        let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script));
        let error = gh.ask("repos/o/gone/issues", None).unwrap_err().to_string();
        assert!(error.contains("HTTP 404"), "{error}");
        assert!(!error.contains("304"), "{error}");
    }

    fn run(started: Option<&str>, completed: Option<&str>, conclusion: Option<&str>) -> CheckRun {
        CheckRun {
            status: if conclusion.is_some() {
                "completed"
            } else {
                "in_progress"
            }
            .into(),
            conclusion: conclusion.map(Into::into),
            started_at: started.map(Into::into),
            completed_at: completed.map(Into::into),
            ..CheckRun::default()
        }
    }

    /// 01M5RGM8HBVQMY25H5PYESK17J: the newest run of a check decides, and
    /// the old run does not count, in any order of the list.
    #[test]
    fn the_newest_run_of_a_check_decides_its_state() {
        let old = |c| {
            run(
                Some("2026-10-24T17:01:00Z"),
                Some("2026-10-24T17:04:30Z"),
                c,
            )
        };
        let new = |c| {
            run(
                Some("2026-10-24T17:22:00Z"),
                Some("2026-10-24T17:26:50Z"),
                c,
            )
        };
        assert_eq!(
            gate_state(&[old(Some("failure")), new(Some("success"))]),
            Gate::Passed
        );
        assert_eq!(
            gate_state(&[new(Some("success")), old(Some("failure"))]),
            Gate::Passed
        );
        let failed = Gate::Ended("failure".into());
        assert_eq!(
            gate_state(&[old(Some("success")), new(Some("failure"))]),
            failed
        );
        assert_eq!(
            gate_state(&[new(Some("failure")), old(Some("success"))]),
            failed
        );
        // A new run that runs makes the check run, also when an old run passed.
        assert_eq!(
            gate_state(&[old(Some("success")), new(None)]),
            Gate::Running
        );
        // An old run that runs does not count when a new run passed.
        assert_eq!(gate_state(&[old(None), new(Some("success"))]), Gate::Passed);
    }

    #[test]
    fn the_end_time_breaks_a_tie_of_the_start_time() {
        let start = Some("2026-10-24T17:01:00Z");
        let first = run(start, Some("2026-10-24T17:04:30Z"), Some("failure"));
        let second = run(start, Some("2026-10-24T17:09:00Z"), Some("success"));
        assert_eq!(gate_state(&[first.clone(), second.clone()]), Gate::Passed);
        assert_eq!(gate_state(&[second, first]), Gate::Passed);
    }

    #[test]
    fn a_run_with_no_time_is_older_and_a_tie_counts_each_run() {
        let timed = run(Some("2026-10-24T17:01:00Z"), None, Some("success"));
        let bare = run(None, None, Some("failure"));
        assert_eq!(gate_state(&[bare.clone(), timed.clone()]), Gate::Passed);
        assert_eq!(gate_state(&[timed, bare.clone()]), Gate::Passed);
        let other = run(None, None, Some("success"));
        assert_eq!(
            gate_state(&[other, bare]),
            Gate::Ended("failure".into()),
            "a tie must not lose a failure"
        );
    }

    #[test]
    fn the_full_state_uses_the_newest_run_of_each_check() {
        let at = |s, c| run(Some(s), Some(s), Some(c));
        let lint = [
            at("2026-10-24T17:01:00Z", "failure"),
            at("2026-10-24T17:22:00Z", "success"),
        ];
        let docs = [at("2026-10-24T17:05:00Z", "success")];
        let state = full_state([gate_state(&lint), gate_state(&docs)]);
        assert_eq!(state, Gate::Passed);
        let docs = [
            at("2026-10-24T17:05:00Z", "success"),
            at("2026-10-24T17:30:00Z", "cancelled"),
        ];
        let state = full_state([gate_state(&lint), gate_state(&docs)]);
        assert_eq!(state, Gate::Ended("cancelled".into()));
    }

    /// #1575: `Hygiene` failed and the job `Gate` has no run yet, as on
    /// the commit cfed717 of #1573. The state is the failure, but the full
    /// check is not done.
    #[test]
    fn a_failed_check_next_to_a_check_with_no_run_is_not_done() {
        let hygiene = [run(
            Some("2026-11-03T16:06:14Z"),
            Some("2026-11-03T16:06:36Z"),
            Some("failure"),
        )];
        let tests = [run(
            Some("2026-11-03T16:06:20Z"),
            Some("2026-11-03T16:10:53Z"),
            Some("success"),
        )];
        let states = [gate_state(&hygiene), gate_state(&tests), gate_state(&[])];
        assert_eq!(
            full_state(states.clone()),
            Gate::Ended("failure".into()),
            "the state still gives the failure"
        );
        assert!(!full_done(&states));

        // The job `Gate` runs, and then it ends.
        let states = [
            gate_state(&hygiene),
            gate_state(&tests),
            gate_state(&[run(None, None, None)]),
        ];
        assert!(!full_done(&states));
        let states = [
            gate_state(&hygiene),
            gate_state(&tests),
            gate_state(&[run(None, None, Some("failure"))]),
        ];
        assert!(full_done(&states));
    }

    #[test]
    fn a_full_check_with_each_check_completed_is_done() {
        let at = |c| [run(Some("2026-10-24T17:05:00Z"), None, Some(c))];
        let states = [gate_state(&at("success")), gate_state(&at("skipped"))];
        assert!(full_done(&states));
        let states = [gate_state(&at("success")), gate_state(&at("cancelled"))];
        assert!(full_done(&states));
    }

    fn refused(link: Link, summary: &str) -> String {
        body(link, summary, "Wave 3").unwrap_err().to_string()
    }

    #[test]
    fn a_body_with_the_link_line_and_the_trailers_has_each_one_time() {
        for summary in [
            FULL.to_owned(),
            FULL.replace('\n', "\r\n"),
            format!("\n{FULL}\n\n"),
        ] {
            assert_eq!(body(Link::Closes(12), &summary, "Wave 3").unwrap(), FULL);
        }
    }

    #[test]
    fn a_body_gets_only_the_lines_that_it_does_not_have() {
        for part in [
            "Show the wave.\n",
            "Closes #12\n\nShow the wave.\n",
            "Show the wave.\n\nIssue: #12\nMilestone: Wave 3\n",
            "Show the wave.\n\nIssue: #12\n",
            "Closes #12\n\nShow the wave.\n\nIssue: #12\n",
        ] {
            assert_eq!(
                body(Link::Closes(12), part, "Wave 3").unwrap(),
                FULL,
                "{part}"
            );
        }
        assert_eq!(
            body(
                Link::Closes(12),
                "Show the wave.\n\nMilestone: Wave 3\n",
                "Wave 3"
            )
            .unwrap(),
            "Closes #12\n\nShow the wave.\n\nMilestone: Wave 3\nIssue: #12\n"
        );
        assert_eq!(
            body(Link::Refs(12), "Issue: #12\nMilestone: Wave 3\n", "Wave 3").unwrap(),
            "Refs #12\n\nIssue: #12\nMilestone: Wave 3\n"
        );
    }

    #[test]
    fn a_line_that_is_not_of_the_pull_request_is_refused_by_name() {
        assert_eq!(
            refused(Link::Closes(12), &FULL.replace("Issue: #12", "Issue: #9")),
            "the body has the line `Issue: #9`, but this pull request needs `Issue: #12`. \
             Change the line, or remove it."
        );
        assert_eq!(
            refused(Link::Closes(12), &FULL.replace("Wave 3", "Wave 4")),
            "the body has the line `Milestone: Wave 4`, but this pull request needs \
             `Milestone: Wave 3`. Change the line, or remove it."
        );
        assert_eq!(
            refused(Link::Closes(12), &FULL.replace("Closes #12", "Closes #9")),
            "the body has the line `Closes #9`, but this pull request needs `Closes #12`. \
             Give --refs for a line `Refs #N`. Change the line, or remove it."
        );
        assert_eq!(
            refused(Link::Closes(12), &FULL.replace("Closes #12", "Refs #12")),
            "the body has the line `Refs #12`, but this pull request needs `Closes #12`. \
             Give --refs for a line `Refs #N`. Change the line, or remove it."
        );
        assert_eq!(
            refused(Link::Refs(12), FULL),
            "the body has the line `Closes #12`, but this pull request needs `Refs #12`. \
             Give no --refs for a line `Closes #N`. Change the line, or remove it."
        );
        assert!(refused(Link::Closes(12), "Text.\n\nIssue: 12\n").contains("`Issue: 12`"));
    }

    /// The same line two times stays: the hygiene check then refuses
    /// the pull request.
    #[test]
    fn a_line_that_the_body_has_two_times_stays() {
        let twice = "Closes #12\nCloses #12\n\nIssue: #12\nIssue: #12\nMilestone: Wave 3\n";
        assert_eq!(body(Link::Closes(12), twice, "Wave 3").unwrap(), twice);
    }

    /// 01M6H3EK1J3DEEV09FAKKKFPEB: a line `Tests: ...` at the end of the
    /// summary is a trailer line. At 73 characters the check refuses it
    /// with the rule `trailer-wrap`; at 72 it passes. Text in a paragraph
    /// that is no block of trailers passes at any length. A repository
    /// that is not riff has no such rule.
    #[test]
    fn a_tests_line_over_72_characters_is_refused_before_the_pull_request_opens() {
        let issue = Issue::new(12, "OPEN", Some("Wave 3"));
        let tests_line = |length: usize| format!("Tests: {}", "x".repeat(length - "Tests: ".len()));
        let checked = |rules, summary: &str| {
            let body = body_for(Link::Closes(12), summary, Some("Wave 3")).unwrap();
            check(rules, "Show the wave", &body, &issue)
        };

        let summary = |line: &str| format!("Show the wave.\n\n{line}\n");
        assert!(checked(Rules::All, &summary(&tests_line(72))).is_ok());
        let error = checked(Rules::All, &summary(&tests_line(73))).unwrap_err();
        let error = error.to_string();
        assert!(error.starts_with("the pull request breaks issue hygiene:\ntrailer-wrap: "));
        assert!(error.contains(&format!("`{}` has 73 characters", tests_line(73))));
        assert_eq!(error.matches("trailer-wrap").count(), 1, "{error}");

        // A sentence in a paragraph of its own is no trailer line.
        let sentence = format!("I ran the tests, and {}.", "all pass, ".repeat(30));
        assert!(checked(Rules::All, &summary(&sentence)).is_ok());

        // Another repository keeps only the rules that riff reads.
        assert!(checked(Rules::Read, &summary(&tests_line(300))).is_ok());
    }

    /// At the end of its time limit, a `gh` stops with each process that
    /// it started, also a process that keeps its pipes open
    /// (01M53S7WGM9DQV2DXFHBRSMTP8).
    #[test]
    fn a_gh_with_a_time_limit_stops_with_each_process_that_it_started() {
        let dir = tempfile::tempdir().unwrap();
        let pid = dir.path().join("pid");
        let script = format!(
            "#!/bin/sh\nsleep 600 &\necho $! > {}\nwait\n",
            pid.display()
        );
        let limit = Duration::from_secs(2);
        let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script)).with_limit(limit);
        let start = Instant::now();
        let error = gh
            .json::<serde_json::Value>(&["pr", "view", "12"])
            .unwrap_err();
        assert_eq!(error.to_string(), text::gh_no_answer("pr view 12", limit));
        assert!(start.elapsed() < isolated::HANG);
        let sleep = std::fs::read_to_string(&pid).unwrap();
        let stat = PathBuf::from(format!("/proc/{}/stat", sleep.trim()));
        // A stopped process stays a zombie until its parent reaps it.
        let gone = || std::fs::read_to_string(&stat).map_or(true, |s| s.contains(") Z "));
        while !gone() {
            assert!(start.elapsed() < isolated::HANG, "sleep {sleep} runs on");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// A `gh` with a time limit that answers in time gives its stdout,
    /// its stderr and its status, and reads its input.
    #[test]
    fn a_gh_with_a_time_limit_that_answers_gives_its_output() {
        let dir = tempfile::tempdir().unwrap();
        let script = "#!/bin/sh\ncat\necho \"$*\" >&2\nexit 3\n";
        let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script))
            .with_limit(isolated::HANG);
        let big = "a body\n".repeat(20_000);
        let (ok, stdout, stderr) = gh.output(&["issue", "comment"], Some(&big)).unwrap();
        assert!(!ok);
        assert_eq!(stdout, big);
        assert_eq!(stderr, "issue comment\n");
    }

    /// A `gh` that counts its calls, with a fake whose body is `body`. Each
    /// call goes in `gh.log`. A call that no case of `body` takes fails on
    /// the primary limit.
    fn limited_gh(dir: &std::path::Path, body: &str) -> Gh {
        let script = format!(
            "#!/bin/sh\necho \"$*\" >> \"$0.log\"\n{body}\n\
             echo 'GraphQL: API rate limit already exceeded for user ID 1.' >&2\nexit 1\n"
        );
        Gh::at(isolated::write_program(dir.join("gh"), script))
            .with_calls(std::sync::Arc::new(crate::ghcalls::Calls::default()))
    }

    /// The calls that `limited_gh` got.
    fn logged(dir: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("gh.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// 01M6H1N2XYKK6QRCJS02RZJHKZ (#1581): the reset of a wait comes from
    /// the answer of the call that failed. A header `x-ratelimit-reset` in
    /// it wins over any read: `gh api rate_limit` answers from a counter
    /// that has points.
    #[test]
    fn the_reset_of_a_wait_is_the_header_of_the_call_that_failed() {
        let dir = tempfile::tempdir().unwrap();
        let gh = limited_gh(
            dir.path(),
            "case \"$1 $2\" in \"api rate_limit\") \
             echo '{\"resources\":{\"graphql\":{\"remaining\":3204,\"reset\":4102444849}}}'; exit 0 ;; esac\n\
             printf '< X-Ratelimit-Remaining: 0\\n< X-Ratelimit-Reset: 4102444815\\n' >&2",
        );
        assert!(gh.run(&["pr", "view", "40"], None).is_err());
        let wait = gh.limit_wait().unwrap();
        assert_eq!((wait.reset_ms, wait.guessed), (4_102_444_815_000, false));
        assert_eq!(logged(dir.path()), ["pr view 40"], "no read");
    }

    /// 01M6H1N2XYKK6QRCJS02RZJHKZ (#1581): GitHub keeps more than one
    /// counter for a token. `gh api rate_limit` and the first reads of
    /// `rateLimit` land on the counter that has points; a later read lands
    /// on the counter that is out, and its reset is the reset of the wait.
    #[test]
    fn a_read_of_the_counter_that_is_out_gives_the_reset_of_the_wait() {
        let dir = tempfile::tempdir().unwrap();
        let good = r#"{"data":{"rateLimit":{"remaining":3204,"resetAt":"2100-01-01T00:00:49Z"}}}"#;
        let out = r#"{"data":{"rateLimit":{"remaining":0,"resetAt":"2100-01-01T00:00:15Z"}}}"#;
        let gh = limited_gh(
            dir.path(),
            &format!(
                "case \"$1 $2\" in\n\
                 \"api rate_limit\") echo '{{\"resources\":{{\"graphql\":{{\"remaining\":3204,\"reset\":4102444849}}}}}}'; exit 0 ;;\n\
                 \"api graphql\") if [ \"$(grep -c 'api graphql' \"$0.log\")\" -lt 3 ]; then echo '{good}'; else echo '{out}'; fi; exit 0 ;;\n\
                 esac"
            ),
        );
        assert!(gh.run(&["pr", "view", "40"], None).is_err());
        let wait = gh.limit_wait().unwrap();
        assert_eq!((wait.reset_ms, wait.guessed), (4_102_444_815_000, false));
        let calls = logged(dir.path());
        assert_eq!(calls.len(), 5, "{calls:?}");
        assert!(calls[1].starts_with("api rate_limit"), "{calls:?}");
        assert!(calls[2..].iter().all(|c| c.starts_with("api graphql")));
        // Each read of `rateLimit` counts as a call of GraphQL, and costs no
        // point: GitHub charges nothing for a query that asks for it alone.
        let shown = gh.calls.as_ref().unwrap().shown(crate::daemon::now_ms());
        let reads = shown.rows.iter().find(|row| row.kind == "api graphql");
        assert_eq!(
            reads.map(|row| (row.calls, row.points)),
            Some((3, 0)),
            "{shown:?}"
        );
    }

    /// 01M6H1N2XYKK6QRCJS02RZJHKZ (#1581): when no read shows a counter
    /// that is out, the wait is the latest reset that riff saw, and the wait
    /// says that it is a guess.
    #[test]
    fn no_counter_that_is_out_gives_the_end_of_the_hour_as_a_guess() {
        let dir = tempfile::tempdir().unwrap();
        let gh = limited_gh(
            dir.path(),
            "case \"$1 $2\" in\n\
             \"api rate_limit\") echo '{\"resources\":{\"graphql\":{\"remaining\":3204,\"reset\":4102444849}}}'; exit 0 ;;\n\
             \"api graphql\") echo '{\"data\":{\"rateLimit\":{\"remaining\":3000,\"resetAt\":\"2100-01-01T00:00:55Z\"}}}'; exit 0 ;;\n\
             esac",
        );
        assert!(gh.run(&["pr", "view", "40"], None).is_err());
        let wait = gh.limit_wait().unwrap();
        assert_eq!((wait.reset_ms, wait.guessed), (4_102_444_855_000, true));
        let reads = logged(dir.path()).len();
        assert_eq!(
            reads,
            2 + crate::ghcalls::PROBES,
            "the call, the read, the probes"
        );
    }

    /// 01M6H1N2XYKK6QRCJS02RZJHKZ (#1581): REST has no read of `rateLimit`.
    /// A counter with points left gives a guess, and a read that fails
    /// gives the wait for a read that cannot be done.
    #[test]
    fn a_limit_of_rest_and_an_unreadable_counter_give_a_guess() {
        let dir = tempfile::tempdir().unwrap();
        let gh = limited_gh(
            dir.path(),
            "case \"$1 $2\" in \"api rate_limit\") \
             echo '{\"resources\":{\"core\":{\"remaining\":100,\"reset\":4102444849}}}'; exit 0 ;; esac",
        );
        assert!(gh.run(&["api", "repos/o/r"], None).is_err());
        let wait = gh.limit_wait().unwrap();
        assert_eq!(wait.resource, crate::ghcalls::Resource::Rest);
        assert_eq!((wait.reset_ms, wait.guessed), (4_102_444_849_000, true));
        assert_eq!(logged(dir.path()).len(), 2, "no read of GraphQL");

        let dir = tempfile::tempdir().unwrap();
        let gh = limited_gh(dir.path(), "");
        let before = crate::daemon::now_ms();
        assert!(gh.run(&["pr", "view", "40"], None).is_err());
        let wait = gh.limit_wait().unwrap();
        assert!(wait.guessed);
        let unread = wait.reset_ms - before;
        assert!(
            (crate::ghcalls::UNREAD_WAIT_MS..crate::ghcalls::UNREAD_WAIT_MS + 60_000)
                .contains(&unread),
            "{unread}"
        );
    }

    /// 01M6H1N2XYKK6QRCJS02RZJHKZ (#1581): a secondary limit has no reset.
    /// The wait is one minute, and riff reads no counter.
    #[test]
    fn a_secondary_limit_waits_one_minute_and_reads_no_counter() {
        let dir = tempfile::tempdir().unwrap();
        let script = "#!/bin/sh\necho \"$*\" >> \"$0.log\"\n\
                      echo 'You have exceeded a secondary rate limit.' >&2\nexit 1\n";
        let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script))
            .with_calls(std::sync::Arc::new(crate::ghcalls::Calls::default()));
        let before = crate::daemon::now_ms();
        assert!(gh.run(&["pr", "view", "40"], None).is_err());
        let wait = gh.limit_wait().unwrap();
        assert!(!wait.guessed);
        let span = wait.reset_ms - before;
        assert!(
            (crate::ghcalls::SECONDARY_WAIT_MS..crate::ghcalls::SECONDARY_WAIT_MS + 60_000)
                .contains(&span),
            "{span}"
        );
        assert_eq!(logged(dir.path()), ["pr view 40"]);
    }

    /// 01M6JGF7265VX2SDXZ4EJAGXVB (#1582): a `gh` that counts its calls
    /// tells the real `gh` so ([`crate::gh_wrapper::COUNTED_VAR`]): the
    /// wrapper of a container counts the call no more. A `gh` that counts
    /// none does not.
    #[test]
    fn a_gh_that_counts_its_calls_tells_the_real_gh() {
        let dir = tempfile::tempdir().unwrap();
        let script = "#!/bin/sh\necho \"${RIFF_GH_COUNTED:-unset}\"\n";
        let program = isolated::write_program(dir.path().join("gh"), script);
        assert_eq!(
            Gh::at(&program).run(&["pr", "view", "1"], None).unwrap(),
            "unset\n"
        );
        let tally = crate::ghcalls::Tally::in_folder(dir.path());
        let counting = Gh::at(&program).by("top").with_tally(Some(tally.clone()));
        assert_eq!(counting.run(&["pr", "view", "1"], None).unwrap(), "1\n");
        let calls = tally.read(crate::daemon::now_ms());
        assert_eq!(calls.len(), 1, "{calls:?}");
        assert_eq!(
            (calls[0].caller.as_str(), calls[0].kind.as_str()),
            ("top", "pr view")
        );
    }
}
