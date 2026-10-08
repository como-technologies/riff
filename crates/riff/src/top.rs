//! `riff top`: a live table of each session, its item and its status
//! (01M3NB54P1RBHTA5TKXP8BMY3K).
//!
//! # Design
//!
//! `riff top` is for people. It reads with the calls of `riff who`
//! (`riff` and `who`) and follows the repository thread with the stream
//! of `riff tail`. It posts nothing and wakes no session
//! (01M3NB589WMPRSAR43BSG9SP41). It draws the table again every
//! [`REFRESH`] and after each message of the thread.
//!
//! The titles of the issues and the current wave come from one
//! `gh issue list` of the open issues ([`Issues`]) of each repository
//! with a live session, and of the working directory ([`board_repos`],
//! 01M42KHN80V49HDDZF953HXDT0).
//! riff keeps them for [`ISSUES_TTL`], then reads them again. A
//! repository with no `gh` read has no titles and no board, and no error
//! line. When a later read of `gh` fails, riff keeps the last issues of
//! that repository ([`Issues::newest_each`], 01M3ZC09FA9DZPTHK31XECZ566):
//! a network fault stops `gh` too.
//!
//! The author of a pull request releases its item at the verify
//! request. So an item with no claim can wait for a verify or for the
//! merge. One `gh pr list` of the open pull requests gives these items
//! ([`Issues::verify`], [`crate::rollout::in_verify`]). The board shows
//! them in `verify`, not in `free` (01M3Z9N6X92KT051P10CKKV7EK). When
//! that call fails, the board counts only the claims.
//!
//! The rows are a tree with four levels: person, host, repository,
//! session (01M3NT4M5D36KTZ5XZMDP6QFQT, 01M42KHN33M4K13GKTX2WM6CMM).
//! `riff top --by repo` puts the repository first. A row with one row
//! under it stays on one line with it, so a small riff stays short. Each
//! row above the sessions has the counts of its sessions. `--user`,
//! `--host` and `--repo` show a part ([`Show`],
//! 01M42KHNCBMBCT3TFBYWE339H5). A session gets only the tag of its
//! role, `lead` or `worker`, from the server, so each machine shows the
//! same (01M3NT4M159EHN5W8JRTQ417N4). The owner is a person: the tag
//! `owner` is on the row of the person.
//!
//! ```mermaid
//! flowchart TD
//!     P["mike: 2 sessions"] --> H1["pangolin › riff: 1 session"]
//!     P --> H2["thelio: 1 session"]
//!     H1 --> S1["session 5b1e2a90 in the worktree issue-7"]
//!     H2 --> R["strata: 1 session"]
//!     R --> S2["session 6d2b7c1a"]
//! ```
//!
//! ```mermaid
//! sequenceDiagram
//!     participant T as riff top
//!     participant G as gh
//!     participant S as riff-server
//!     T->>G: issue list, pr list of each repository (each ISSUES_TTL)
//!     loop each REFRESH, and each message of the thread
//!         T->>S: riff, who
//!         T->>T: draw the table in place
//!     end
//! ```
//!
//! # A look that fails
//!
//! A look is the two calls `riff` and `who`. A laptop sleeps, and the
//! Wi-Fi drops or gets a new address. So after one good look, a look
//! that fails does not end `riff top`
//! (01M3Z8FXE2DY34ZP75WJE1S8HR), when a new try can repair the fault
//! ([`crate::api::passes`]). Each call of a look has the budget of a
//! short command, [`crate::link::SHORT_BUDGET`]
//! (01M4A803Z4Q0KX6NT1KC6QR43H), and the link makes new connections
//! after a fault ([`crate::link::Link::swap`]). When a look fails,
//! `riff top` keeps the last table, shows [`Top::fault`] as its first
//! line, and looks again at its interval. The line goes at the next
//! good look.
//!
//! `riff top --once`, the first look, and a fault that a new try cannot
//! repair end `riff top` with the error.
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> Good: the first look is good
//!     [*] --> [*]: the first look fails, the error
//!     Good --> Good: a good look, a new table
//!     Good --> Fault: a look fails, a new try can repair it
//!     Fault --> Fault: the same, the last table and the line
//!     Fault --> Good: a good look, a new table and no line
//!     Good --> [*]: another fault, the error
//!     Fault --> [*]: another fault, the error
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::time::Duration;

use riff_core::build::Build;
use riff_core::name::Repo;
use riff_core::wire::{Person, PersonRole, RiffOwner, RiffReply, SessionInfo, SessionState};
use serde::Deserialize;

use crate::host::HostStatus;
use crate::state;
use crate::style::{BOLD, ERROR, MUTED, WARNING, session as session_style, styled};
use crate::text::{self, safe};
use crate::view;

/// The time between two draws of `riff top` with no message.
pub const REFRESH: Duration = Duration::from_secs(3);

/// How long `riff top` keeps the issues of `gh`.
pub const ISSUES_TTL: Duration = Duration::from_secs(60);

/// The open issues of the repository: the title of each, and the
/// current wave.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Issues {
    /// The title of each open issue, by number.
    pub titles: BTreeMap<u64, String>,
    /// The current wave: the open `Wave N` milestone with the lowest N,
    /// and its open issues.
    pub wave: Option<(String, Vec<u64>)>,
    /// The issues whose pull request waits for a verify or for the
    /// merge (01M3Z9N6X92KT051P10CKKV7EK).
    pub verify: BTreeSet<u64>,
}

#[derive(Deserialize)]
struct GhIssue {
    number: u64,
    #[serde(deserialize_with = "crate::text::forge_de")]
    title: String,
    milestone: Option<GhMilestone>,
}

#[derive(Deserialize)]
struct GhMilestone {
    #[serde(deserialize_with = "crate::text::forge_de")]
    title: String,
}

impl Issues {
    /// The arguments of `gh` for the open issues of `repo`.
    pub fn gh_args(repo: &str) -> Vec<String> {
        [
            "issue",
            "list",
            "--repo",
            repo,
            "--state",
            "open",
            "--limit",
            "1000",
            "--json",
            "number,title,milestone",
        ]
        .map(String::from)
        .to_vec()
    }

    /// The issues in the JSON of `gh issue list --json
    /// number,title,milestone`. `None` for other text.
    ///
    /// ```
    /// use riff::top::Issues;
    ///
    /// let json = r#"[
    ///   {"number": 12, "title": "Show the wave", "milestone": {"title": "Wave 10"}},
    ///   {"number": 13, "title": "Later", "milestone": {"title": "Wave 11"}},
    ///   {"number": 9, "title": "Old", "milestone": {"title": "Wave 9: Cloud"}},
    ///   {"number": 4, "title": "Kept", "milestone": {"title": "Backlog"}},
    ///   {"number": 5, "title": "New", "milestone": null}
    /// ]"#;
    /// let issues = Issues::parse(json).unwrap();
    /// assert_eq!(issues.titles[&12], "Show the wave");
    /// assert_eq!(issues.wave, Some(("Wave 9: Cloud".into(), vec![9])));
    /// assert_eq!(Issues::parse("not json"), None);
    /// ```
    pub fn parse(json: &str) -> Option<Issues> {
        let list: Vec<GhIssue> = serde_json::from_str(json).ok()?;
        let mut waves: BTreeMap<u64, (String, Vec<u64>)> = BTreeMap::new();
        for issue in &list {
            let Some(milestone) = &issue.milestone else {
                continue;
            };
            if let Some(n) = wave_number(&milestone.title) {
                let wave = waves
                    .entry(n)
                    .or_insert_with(|| (milestone.title.clone(), Vec::new()));
                wave.1.push(issue.number);
            }
        }
        let wave = waves.into_values().next().map(|(title, mut items)| {
            items.sort_unstable();
            (title, items)
        });
        Some(Issues {
            titles: list.into_iter().map(|i| (i.number, i.title)).collect(),
            wave,
            verify: BTreeSet::new(),
        })
    }

    /// These issues with the open pull requests in the JSON of
    /// `gh pr list --json` with [`crate::rollout::PULL_FIELDS`]. Other
    /// text gives no pull request.
    ///
    /// ```
    /// use riff::top::Issues;
    ///
    /// let issues = Issues::parse(r#"[{"number": 12, "title": "a", "milestone": null}]"#).unwrap();
    /// let pulls = r#"[
    ///   {"number": 40, "headRefName": "worktree-issue-12", "isDraft": false, "statusCheckRollup": []},
    ///   {"number": 41, "headRefName": "worktree-issue-13", "isDraft": false,
    ///    "statusCheckRollup": [{"context": "riff/verify", "state": "FAILURE"}]}
    /// ]"#;
    /// assert_eq!(issues.clone().with_pulls(pulls).verify, [12].into());
    /// assert!(issues.with_pulls("not json").verify.is_empty());
    /// ```
    #[must_use]
    pub fn with_pulls(mut self, json: &str) -> Issues {
        let pulls: Vec<crate::rollout::Pull> = serde_json::from_str(json).unwrap_or_default();
        self.verify = crate::rollout::in_verify(&pulls).into_iter().collect();
        self
    }

    /// The arguments of `gh` for the open pull requests of `repo`.
    pub fn gh_pull_args(repo: &str) -> Vec<String> {
        [
            "pr",
            "list",
            "--repo",
            repo,
            "--state",
            "open",
            "--limit",
            "100",
            "--json",
            crate::rollout::PULL_FIELDS,
        ]
        .map(String::from)
        .to_vec()
    }

    /// The open issues of `repo` from `gh`. `None` when `gh` is not
    /// there or fails.
    pub fn from_gh(repo: &str) -> Option<Issues> {
        let gh = |args: Vec<String>| {
            let out = std::process::Command::new("gh")
                .args(args)
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
        };
        let issues = Self::parse(&gh(Self::gh_args(repo))?)?;
        Some(match gh(Self::gh_pull_args(repo)) {
            Some(pulls) => issues.with_pulls(&pulls),
            None => issues,
        })
    }

    /// The issues after a new read of `gh`: `new`, or `last` when `gh`
    /// failed (01M3ZC09FA9DZPTHK31XECZ566). So the titles and the board
    /// stay while the network is away.
    ///
    /// ```
    /// use riff::top::Issues;
    ///
    /// let json = r#"[{"number": 7, "title": "Fix", "milestone": {"title": "Wave 3"}}]"#;
    /// let last = Issues::parse(json);
    /// // `gh` failed: the last issues stay.
    /// assert_eq!(Issues::newest(last.clone(), None), last);
    /// // `gh` gave no open issue: the board is empty.
    /// let new = Issues::parse("[]");
    /// assert_eq!(Issues::newest(last, new.clone()), new);
    /// assert_eq!(Issues::newest(None, None), None);
    /// ```
    pub fn newest(last: Option<Issues>, new: Option<Issues>) -> Option<Issues> {
        new.or(last)
    }
}

/// The N of a milestone `Wave N`, or `Wave N: NAME`.
///
/// ```
/// use riff::top::wave_number;
///
/// assert_eq!(wave_number("Wave 10"), Some(10));
/// assert_eq!(wave_number("Wave 5: Cloud"), Some(5));
/// assert_eq!(wave_number("Backlog"), None);
/// assert_eq!(wave_number("Wavelength"), None);
/// ```
pub fn wave_number(title: &str) -> Option<u64> {
    let rest = title.strip_prefix("Wave ")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let after = &rest[digits.len()..];
    if !(after.is_empty() || after.starts_with(':')) {
        return None;
    }
    digits.parse().ok()
}

/// The issue number of a claim `issue-N` or `verify-issue-N`.
///
/// ```
/// use riff::top::issue_of;
///
/// assert_eq!(issue_of("issue-202"), Some(202));
/// assert_eq!(issue_of("verify-issue-196"), Some(196));
/// assert_eq!(issue_of("docs"), None);
/// ```
pub fn issue_of(claim: &str) -> Option<u64> {
    claim
        .strip_prefix("verify-")
        .unwrap_or(claim)
        .strip_prefix("issue-")?
        .parse()
        .ok()
}

impl Issues {
    /// The issues of each repository after a new read of `gh` for each
    /// repository in `new` (01M3ZC09FA9DZPTHK31XECZ566): a read that
    /// failed keeps the last issues of its repository. A repository
    /// that `new` does not name has no live session any more, and goes.
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use riff::top::Issues;
    ///
    /// let wave = |n: u64| Issues::parse(&format!(
    ///     r#"[{{"number": {n}, "title": "a", "milestone": {{"title": "Wave {n}"}}}}]"#
    /// ));
    /// let last = BTreeMap::from([("o/r".to_owned(), wave(3).unwrap()), ("o/old".to_owned(), wave(4).unwrap())]);
    /// let each = Issues::newest_each(last, vec![("o/r".into(), None), ("o/s".into(), wave(5))]);
    /// assert_eq!(each.keys().collect::<Vec<_>>(), ["o/r", "o/s"]);
    /// assert_eq!(each["o/r"], wave(3).unwrap());
    /// ```
    pub fn newest_each(
        mut last: BTreeMap<String, Issues>,
        new: Vec<(String, Option<Issues>)>,
    ) -> BTreeMap<String, Issues> {
        new.into_iter()
            .filter_map(|(repo, read)| {
                let issues = Issues::newest(last.remove(&repo), read)?;
                Some((repo, issues))
            })
            .collect()
    }
}

/// The top level of the tree of `riff top` (01M42KHNCBMBCT3TFBYWE339H5).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum By {
    /// Person, host, repository, session.
    #[default]
    Person,
    /// Repository, person, host, session: a look across people.
    Repo,
}

/// The part of the riff that `riff top` shows: `--user`, `--host`,
/// `--repo` and `--by` (01M42KHNCBMBCT3TFBYWE339H5). Each filter that
/// is set must match.
#[derive(Debug, Default, Clone)]
pub struct Show {
    pub user: Option<String>,
    pub host: Option<String>,
    /// `OWNER/REPO`.
    pub repo: Option<String>,
    pub by: By,
}

impl Show {
    /// True when `s` matches each filter that is set.
    pub fn shows(&self, s: &SessionInfo) -> bool {
        let place = s.uri.place();
        self.user.as_ref().is_none_or(|u| s.uri.who().user() == u)
            && self.host.as_ref().is_none_or(|h| place.host() == h)
            && self.repo.as_ref().is_none_or(|r| &place.repo_text() == r)
    }

    /// True when a person with no session can show: no filter of a
    /// host or a repository is set, and the user matches.
    fn shows_person(&self, user: &str) -> bool {
        self.host.is_none() && self.repo.is_none() && self.user.as_ref().is_none_or(|u| u == user)
    }
}

/// The repositories that get a board: each repository, `OWNER/REPO`,
/// of a live session that `show` shows, and `here`, the repository of
/// the working directory, when `show` has no `--user` and no `--host`
/// and its `--repo` matches (01M42KHN80V49HDDZF953HXDT0). `riff top`
/// reads the issues of each one with `gh`.
pub fn board_repos(sessions: &[SessionInfo], show: &Show, here: Option<&str>) -> BTreeSet<String> {
    let mut repos: BTreeSet<String> = sessions
        .iter()
        .filter(|s| s.live && show.shows(s))
        .filter(|s| matches!(s.uri.place().repo(), Repo::Git { .. }))
        .map(|s| s.uri.place().repo_text())
        .collect();
    if let Some(here) = here
        && show.user.is_none()
        && show.host.is_none()
        && show.repo.as_ref().is_none_or(|r| r == here)
    {
        repos.insert(here.to_owned());
    }
    repos
}

/// What `riff top` shows.
pub struct Top<'a> {
    /// The pauses of the riff, as the caller sees them.
    pub pauses: &'a RiffReply,
    pub owner: &'a RiffOwner,
    pub server: Option<&'a Build>,
    pub sessions: &'a [SessionInfo],
    /// The members of the riff from `who`, also when away.
    pub people: &'a [Person],
    /// The issues of each repository, by `OWNER/REPO`
    /// ([`Issues::newest_each`]). A repository with no `gh` read has
    /// none.
    pub issues: &'a BTreeMap<String, Issues>,
    /// The repository of the working directory, `OWNER/REPO`, for its
    /// pause. `None` outside git.
    pub repo: Option<&'a str>,
    /// The part of the riff to show.
    pub show: &'a Show,
    /// The width of the terminal in columns: no line is wider.
    pub width: usize,
    /// The line of a look that failed ([`crate::text::top_fault`]),
    /// while the table is the one of the last good look
    /// (01M3Z8FXE2DY34ZP75WJE1S8HR).
    pub fault: Option<&'a str>,
    /// The machines that run workers ([`machines`]): their numbers go
    /// under the row of their host (01M421QPZ9E01PQ62PDBH378SJ).
    pub machines: &'a [Machine],
}

/// The numbers of one machine that runs workers, for `riff top`.
#[derive(Debug, Clone, PartialEq)]
pub struct Machine {
    pub user: String,
    pub host: String,
    pub status: HostStatus,
}

/// The machines that run workers: each live workers host in
/// `sessions`, with the numbers of its status, and `here`, the machine
/// of `riff top`, when no host tells its numbers
/// (01M421QPZ9E01PQ62PDBH378SJ).
///
/// ```
/// use riff::host::HostStatus;
/// use riff::top::{Machine, machines};
/// use riff_core::wire::{SessionInfo, Status, StatusInfo};
///
/// let status = HostStatus { limit: 2, floor: 4, deaths: 0, machine: None, disk: None, monitor: None, workers: vec![] };
/// let host = SessionInfo {
///     uri: "riff://mike@thelio/o/r?session=h1".parse().unwrap(),
///     live: true,
///     idle_secs: 0,
///     status: Some(StatusInfo {
///         status: Status { step: status.line() },
///         age_secs: 1,
///         stale: false,
///     }),
///     worker: false,
///     stopping: false,
///     claims_secs: 0,
///     must_clear: false,
///     fresh_secs: None,
///     state: None,
///     work: None,
///     waits: None,
///     blocked: None,
///     step: None,
/// };
/// let here = Machine { user: "mike".into(), host: "pangolin".into(), status: status.clone() };
/// let found = machines(std::slice::from_ref(&host), Some(here.clone()));
/// assert_eq!(found.len(), 2);
/// assert_eq!((found[0].user.as_str(), found[0].host.as_str()), ("mike", "thelio"));
/// assert_eq!(found[1], here);
/// // A workers host on the machine of riff top wins over the local numbers.
/// let there = Machine { host: "thelio".into(), ..here };
/// assert_eq!(machines(&[host], Some(there)).len(), 1);
/// ```
pub fn machines(sessions: &[SessionInfo], here: Option<Machine>) -> Vec<Machine> {
    let mut found: Vec<Machine> = sessions
        .iter()
        .filter(|s| s.live)
        .filter_map(|s| {
            let status = HostStatus::parse(&s.status.as_ref()?.status.step)?;
            Some(Machine {
                user: s.uri.who().user().to_owned(),
                host: s.uri.place().host().to_owned(),
                status,
            })
        })
        .collect();
    if let Some(here) = here
        && !found
            .iter()
            .any(|m| m.user == here.user && m.host == here.host)
    {
        found.push(here);
    }
    found
}

/// The parts of the line of the numbers of a machine in `riff top`
/// (01M421QPZ9E01PQ62PDBH378SJ): the load average of 1 and 5 minutes
/// against the physical cores, the cap of the clock and the clock now,
/// the available memory against the floor, the workers against the
/// limit, and the jobs of each worker. A number over its limit has the
/// warning style. A host that tells fewer numbers gets fewer parts.
///
/// ```
/// use riff::host::HostStatus;
/// use riff::machine::Machine;
/// use riff::monitor::Numbers;
/// use riff::top::numbers;
///
/// let plain = |parts: Vec<(String, anstyle::Style)>| {
///     parts.into_iter().map(|(t, _)| t).collect::<Vec<_>>().join("  ")
/// };
/// let status = HostStatus {
///     limit: 4,
///     floor: 4,
///     deaths: 0,
///     machine: Some(Machine { cores: 16, mhz: 3000, now_mhz: 2990, mem_gb: 31, avail_gb: 20, load: 13.2 }),
///     disk: None,
///     monitor: Some(Numbers { on: true, load5: 9.8, limit: 12.0, physical: 8, jobs: 2, kill: None }),
///     workers: vec![("%3".into(), "1a2b3c4d".into()); 3],
/// };
/// assert_eq!(
///     plain(numbers(&status)),
///     "load 13.2 9.8/8  3000MHz now 2990  20GB avail/4  workers 3/4  jobs 2"
/// );
/// // Over the limits: the warning style.
/// let over = HostStatus {
///     machine: status.machine.map(|m| Machine { avail_gb: 3, ..m }),
///     monitor: status.monitor.clone().map(|n| Numbers { load5: 12.5, ..n }),
///     ..status.clone()
/// };
/// let warned: Vec<String> = numbers(&over)
///     .into_iter()
///     .filter(|(_, style)| *style == riff::style::WARNING)
///     .map(|(t, _)| t)
///     .collect();
/// assert_eq!(warned, ["load 13.2 12.5/8", "3GB avail/4"]);
/// // The line of a big machine fits in 80 columns under its host.
/// let big = HostStatus {
///     limit: 10,
///     machine: Some(Machine { cores: 64, mhz: 5883, now_mhz: 5800, mem_gb: 256, avail_gb: 200, load: 33.2 }),
///     monitor: Some(Numbers { on: true, load5: 28.4, limit: 48.0, physical: 32, jobs: 12, kill: None }),
///     workers: vec![("%3".into(), "1a2b3c4d".into()); 10],
///     ..status.clone()
/// };
/// assert!(6 + plain(numbers(&big)).len() <= 80, "{}", plain(numbers(&big)));
/// // A host of the release before tells no numbers of the monitor.
/// let old = HostStatus { monitor: None, ..status };
/// assert_eq!(plain(numbers(&old)), "load 13.2/16  3000MHz now 2990  20GB avail/4  workers 3/4");
/// ```
pub fn numbers(status: &HostStatus) -> Vec<(String, anstyle::Style)> {
    let style = |over: bool| {
        if over { WARNING } else { anstyle::Style::new() }
    };
    let mut parts = Vec::new();
    let monitor = status.monitor.as_ref();
    if let Some(m) = &status.machine {
        let (load, over) = match monitor {
            Some(n) => (
                format!("load {:.1} {:.1}/{}", m.load, n.load5, n.physical),
                n.load5 > n.limit,
            ),
            None => (format!("load {:.1}/{}", m.load, m.cores), m.busy()),
        };
        parts.push((load, style(over)));
        parts.push((
            format!("{}MHz now {}", m.mhz, m.now_mhz),
            anstyle::Style::new(),
        ));
        parts.push((
            format!("{}GB avail/{}", m.avail_gb, status.floor),
            style(m.low(status.floor)),
        ));
    }
    let runs = status.workers.len();
    parts.push((
        format!("workers {runs}/{}", status.limit),
        style(runs > usize::from(status.limit)),
    ));
    if let Some(n) = monitor {
        parts.push((format!("jobs {}", n.jobs), anstyle::Style::new()));
    }
    parts
}

/// The line of the last kill on a machine in `riff top`
/// (01M421QPZ9E01PQ62PDBH378SJ), or `None` with no kill.
///
/// ```
/// use riff::host::HostStatus;
/// use riff::monitor::{Kill, Numbers};
///
/// let kill = Kill { at: 1727980000, by: "systemd-oomd".into(), what: String::new() };
/// let monitor = Numbers { on: true, load5: 1.0, limit: 12.0, physical: 8, jobs: 2, kill: Some(kill) };
/// let status = HostStatus { limit: 2, floor: 4, deaths: 0, machine: None, disk: None, monitor: Some(monitor), workers: vec![] };
/// assert_eq!(
///     riff::top::kill_line(&status).unwrap().0,
///     format!("last kill {} by systemd-oomd", riff::text::clock(1727980000)),
/// );
/// assert_eq!(riff::top::kill_line(&HostStatus { monitor: None, ..status }), None);
/// ```
pub fn kill_line(status: &HostStatus) -> Option<(String, anstyle::Style)> {
    let kill = status.monitor.as_ref()?.kill.as_ref()?;
    Some((
        format!(
            "last kill {} by {}",
            crate::text::clock(kill.at),
            safe(&kill.by)
        ),
        WARNING,
    ))
}

/// One line of the tree: a plain lead-in, then its parts, each with its
/// style.
struct Line {
    pre: String,
    parts: Vec<(String, anstyle::Style)>,
}

impl Line {
    /// A line of `pre`, then each part that is not empty, with two
    /// spaces between them.
    fn new(pre: impl Into<String>, parts: Vec<(String, anstyle::Style)>) -> Self {
        let mut joined = Vec::new();
        for part in parts.into_iter().filter(|(text, _)| !text.is_empty()) {
            if !joined.is_empty() {
                joined.push(("  ".to_owned(), anstyle::Style::new()));
            }
            joined.push(part);
        }
        Line {
            pre: pre.into(),
            parts: joined,
        }
    }

    /// The line with its styles, cut to `width` columns with `…` at the
    /// end when it is wider.
    fn render(&self, width: usize) -> String {
        let pieces = || {
            std::iter::once((self.pre.as_str(), anstyle::Style::new())).chain(
                self.parts
                    .iter()
                    .map(|(text, style)| (text.as_str(), *style)),
            )
        };
        let full: usize = pieces().map(|(text, _)| display_width(text)).sum();
        let mut kept: Vec<(String, anstyle::Style)> = Vec::new();
        if full <= width {
            kept.extend(pieces().map(|(text, style)| (text.to_owned(), style)));
        } else {
            // Keep one column for the `…`.
            let mut left = width.saturating_sub(1);
            for (text, style) in pieces() {
                let (start, cut) = take_width(text, left);
                left -= display_width(&start);
                kept.push((start, style));
                if cut {
                    break;
                }
            }
            // No spaces before the `…`.
            while let Some((text, _)) = kept.last_mut() {
                text.truncate(text.trim_end().len());
                if !text.is_empty() {
                    break;
                }
                kept.pop();
            }
            match kept.last_mut() {
                Some((text, _)) => text.push('…'),
                None => kept.push(("…".into(), anstyle::Style::new())),
            }
        }
        kept.iter()
            .map(|(text, style)| paint(*style, text))
            .collect()
    }
}

/// `line`, a line with styles, as it is when it fits in `width`
/// columns. Else the line with no styles, cut with `…`.
fn fit(line: &str, width: usize) -> String {
    let plain = anstream::adapter::strip_str(line).to_string();
    if display_width(&plain) <= width {
        line.to_owned()
    } else {
        Line::new("", vec![(plain, anstyle::Style::new())]).render(width)
    }
}

/// `text` in `style`, or nothing when `text` is empty.
fn paint(style: anstyle::Style, text: &str) -> String {
    if text.is_empty() {
        String::new()
    } else {
        styled(style, text)
    }
}

/// The columns of `text` on a terminal.
fn display_width(text: &str) -> usize {
    textwrap::core::display_width(text)
}

/// The start of `text` that fits in `cols` columns, and true when riff
/// cut the text.
fn take_width(text: &str, cols: usize) -> (String, bool) {
    let mut kept = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = display_width(c.encode_utf8(&mut [0; 4]));
        if used + w > cols {
            return (kept, true);
        }
        used += w;
        kept.push(c);
    }
    (kept, false)
}

/// One person of the tree: the role, and live or the time since the
/// last call.
struct PersonRow {
    role: PersonRole,
    live: bool,
    seen_secs: Option<u64>,
}

/// A level of the tree above the sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Person,
    Host,
    Repo,
}

impl Level {
    /// The key of `s` at this level: the user, the host, or
    /// `OWNER/REPO`.
    fn key(self, s: &SessionInfo) -> String {
        match self {
            Level::Person => s.uri.who().user().to_owned(),
            Level::Host => s.uri.place().host().to_owned(),
            Level::Repo => s.uri.place().repo_text(),
        }
    }
}

/// One row of the tree above the sessions: a person, a host or a
/// repository, each session under it, and the rows of the next level.
/// The rows of the last level have no rows under them, only sessions.
struct Group<'a> {
    level: Level,
    key: String,
    sessions: Vec<&'a SessionInfo>,
    kids: Vec<Group<'a>>,
}

/// The rows of `sessions` at the first of `levels`, by key, each with
/// the rows of the next levels. In each row, blocked sessions come
/// first, then each session by its ID.
fn groups<'a>(sessions: &[&'a SessionInfo], levels: &[Level]) -> Vec<Group<'a>> {
    let Some((&level, rest)) = levels.split_first() else {
        return Vec::new();
    };
    let mut by: BTreeMap<String, Vec<&SessionInfo>> = BTreeMap::new();
    for s in sessions {
        by.entry(level.key(s)).or_default().push(s);
    }
    by.into_iter()
        .map(|(key, mut sessions)| {
            sessions.sort_by_key(|s| (!blocked(s), s.uri.who().session().map(str::to_owned)));
            Group {
                level,
                key,
                kids: groups(&sessions, rest),
                sessions,
            }
        })
        .collect()
}

/// The counts of a row of the tree (01M42KHN33M4K13GKTX2WM6CMM): the
/// sessions, then the busy, idle and blocked ones and the claims, each
/// that is more than 0.
///
/// ```text
/// 3 sessions: 1 busy, 1 idle, 1 blocked, 2 claims
/// ```
fn counts(sessions: &[&SessionInfo]) -> String {
    let many = |n: usize, word: &str| {
        if n == 1 {
            format!("1 {word}")
        } else {
            format!("{n} {word}s")
        }
    };
    let count = |state: SessionState| sessions.iter().filter(|s| state::of(s) == state).count();
    let claims: usize = sessions.iter().map(|s| s.uri.claims().len()).sum();
    let mut parts: Vec<String> = [
        (SessionState::Busy, "busy"),
        (SessionState::Idle, "idle"),
        (SessionState::Blocked, "blocked"),
    ]
    .into_iter()
    .map(|(state, word)| (count(state), word))
    .filter(|(n, _)| *n > 0)
    .map(|(n, word)| format!("{n} {word}"))
    .collect();
    if claims > 0 {
        parts.push(many(claims, "claim"));
    }
    let head = many(sessions.len(), "session");
    if parts.is_empty() {
        head
    } else {
        format!("{head}: {}", parts.join(", "))
    }
}

impl Top<'_> {
    /// The header, the blocked sessions, a board for each repository,
    /// then the tree (01M3NT4M5D36KTZ5XZMDP6QFQT,
    /// 01M42KHN33M4K13GKTX2WM6CMM). The tree grows down, not across: no
    /// line is wider than [`Top::width`]. riff cuts a wider line with `…`
    /// (01M3QA8EZHX5B8C9CKF8Q3154X). No line has a column heading.
    ///
    /// - The header: the facts `riff`, `owner` and `build`, as in
    ///   `riff who`.
    /// - A board for each repository of a live session, and of
    ///   [`Top::repo`], that has issues ([`board_repos`],
    ///   01M42KHN80V49HDDZF953HXDT0): the current wave
    ///   with its repository, then one line for each group of its open
    ///   items: `free`, `claimed`, and `verify` for an item with a
    ///   verify claim, and for an item with no claim whose pull request
    ///   waits for a verify or for the merge. Only a claim in that
    ///   repository counts. A repository with no issues has no board,
    ///   and no error line.
    /// - The tree has four levels: person, host, repository, session.
    ///   With [`By::Repo`] the repository comes first: repository,
    ///   person, host, session. Each person, host and repository line
    ///   has the counts of its sessions: the sessions, then the busy,
    ///   idle and blocked ones and the claims. A row with one row
    ///   under it stays on one line with it, after a `›`: so a small
    ///   riff stays short.
    /// - A person line: the USER in bold color, the role tag `owner` or
    ///   `admin`, and [`state::person`]. Each member of `who` gets a
    ///   line, also when away, when [`Top::show`] has no filter of a
    ///   host or a repository.
    /// - A repository line: [`text::Label::repo`], the name of the
    ///   repository in the label of each of its sessions
    ///   (01M3WNHCD659FH3Z5VYYH69WWR, 01M4CPVJ9ANPEBTWY9GETE2DGW).
    /// - A session: the first line has the short session ID, its
    ///   worktree `#WORKTREE`, the role tag `lead` or `worker`, and the
    ///   state word that the server derives
    ///   (01M3QB6CJ1XCQG5B1BVR8AF3B4). Under it comes one line for each
    ///   fact of the [`state::detail`]. A session with no detail takes
    ///   one line. An issue has the title of its own repository.
    ///
    /// Each row comes by its key: the user, the host, `OWNER/REPO`. In a
    /// repository, blocked sessions come first. Each blocked session
    /// also has a red line before the boards: the session, its claims,
    /// the reason, the time that it waits, and `the lead gave no answer`
    /// when the lead gave none (01M41FZR4XRP55M409YBCPTHPH,
    /// 01M41FZQCHWY1YVGAZ60ZHJK21). [`Top::show`] picks the sessions of
    /// each part (01M42KHNCBMBCT3TFBYWE339H5). The text has ANSI styles:
    /// print it through `anstream`.
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use riff::top::{By, Issues, Show, Top};
    /// use riff_core::wire::{
    ///     BlockedInfo, Person, PersonRole, RiffOwner, RiffState, SessionInfo, SessionState, Status,
    ///     StatusInfo,
    /// };
    ///
    /// let info = |uri: &str, step: &str, blocked: Option<&str>, worker, state| SessionInfo {
    ///     uri: uri.parse().unwrap(),
    ///     live: state != SessionState::Offline,
    ///     idle_secs: 300,
    ///     status: Some(StatusInfo {
    ///         status: Status { step: step.into() },
    ///         age_secs: 120,
    ///         stale: false,
    ///     }),
    ///     worker,
    ///     stopping: false,
    ///     claims_secs: 600,
    ///     must_clear: false,
    ///     fresh_secs: None,
    ///     state: Some(state),
    ///     work: None,
    ///     waits: None,
    ///     blocked: blocked.map(|reason| BlockedInfo {
    ///         reason: reason.into(),
    ///         secs: 1500,
    ///         answered: false,
    ///         woken_again: true,
    ///         unanswered: true,
    ///     }),
    ///     step: None,
    /// };
    /// let sessions = [
    ///     info("riff://mike@thelio/o/r?session=aaaa1111&lead=true", "plan", None, false, SessionState::Idle),
    ///     info("riff://mike@thelio/o/r?session=bbbb2222&claim=issue-12", "tests", None, true, SessionState::Busy),
    ///     info("riff://mike@thelio/o/r?session=cccc3333", "merge", None, false, SessionState::Offline),
    ///     info(
    ///         "riff://mike@pangolin/o/r?session=dddd4444&claim=verify-issue-13#verify-13",
    ///         "docs",
    ///         Some("waits for the lead"),
    ///         true,
    ///         SessionState::Blocked,
    ///     ),
    ///     // A session of another repository: its claim is not on the
    ///     // board of o/r, and the read of its issues failed.
    ///     info(
    ///         "riff://mike@pangolin/o/s?session=eeee5555&lead=true&claim=issue-14",
    ///         "plan",
    ///         None,
    ///         false,
    ///         SessionState::Busy,
    ///     ),
    /// ];
    /// let people = [
    ///     Person { user: "ann".into(), role: PersonRole::Admin, live: false, seen_secs: Some(3600) },
    ///     Person { user: "mike".into(), role: PersonRole::Owner, live: true, seen_secs: Some(0) },
    /// ];
    /// let issues = Issues::parse(
    ///     r#"[{"number": 12, "title": "Show the wave in the board of riff top", "milestone": {"title": "Wave 3"}},
    ///         {"number": 13, "title": "Later", "milestone": {"title": "Wave 3"}},
    ///         {"number": 14, "title": "Free", "milestone": {"title": "Wave 3"}},
    ///         {"number": 15, "title": "Asked", "milestone": {"title": "Wave 3"}}]"#,
    /// )
    /// .unwrap()
    /// // The author of #15 asked for a verify and released the item.
    /// .with_pulls(
    ///     r#"[{"number": 40, "headRefName": "worktree-issue-15", "isDraft": false, "statusCheckRollup": []}]"#,
    /// );
    /// let issues = BTreeMap::from([("o/r".to_owned(), issues)]);
    /// let owner = RiffOwner::Owner { user: "mike".into(), email: "m@x.io".into() };
    /// let running = RiffState::Running.into();
    /// let all = Show::default();
    /// let mut top = Top {
    ///     pauses: &running,
    ///     owner: &owner,
    ///     server: None,
    ///     sessions: &sessions,
    ///     people: &people,
    ///     issues: &issues,
    ///     repo: Some("o/r"),
    ///     show: &all,
    ///     width: 80,
    ///     fault: None,
    ///     machines: &[],
    /// };
    /// let text = anstream::adapter::strip_str(&top.view()).to_string();
    /// assert!(text.starts_with("riff   running\nowner  mike (m@x.io)\nbuild  "), "{text}");
    /// assert!(
    ///     text.contains("\n\nblocked  dddd4444 verify-issue-13, 25m, the lead gave no answer: waits for the…\n"),
    ///     "{text}"
    /// );
    /// // o/s has no issues: no board, and no error line.
    /// assert!(text.contains("\n\nWave 3 (o/r)\n  free: #14\n  claimed: #12\n  verify: #13 #15\n\n"), "{text}");
    /// assert!(!text.contains("o/s"), "{text}");
    /// let tree: Vec<&str> = text.rsplit("\n\n").next().unwrap().lines().collect();
    /// assert_eq!(tree, [
    ///     "ann  admin  offline  seen 1h ago",
    ///     "mike  owner  online  5 sessions: 2 busy, 1 idle, 1 blocked, 3 claims",
    ///     "├─ pangolin  2 sessions: 1 busy, 1 blocked, 2 claims",
    ///     "│  ├─ r  1 session: 1 blocked, 1 claim",
    ///     "│  │  └─ dddd4444  #verify-13  worker  blocked",
    ///     "│  │       waits for the lead (25m ago)",
    ///     "│  │       the lead gave no answer",
    ///     "│  │       reviewing #13 Later",
    ///     "│  └─ s  1 session: 1 busy, 1 claim",
    ///     "│     └─ eeee5555  lead  busy",
    ///     "│          working on #14",
    ///     "│          2m ago: plan",
    ///     "└─ thelio  › r  3 sessions: 1 busy, 1 idle, 1 claim",
    ///     "   ├─ aaaa1111  lead  idle",
    ///     "   │    monitoring work for 10m",
    ///     "   │    2m ago: plan",
    ///     "   ├─ bbbb2222  worker  busy",
    ///     "   │    working on #12 Show the wave in the board of riff top",
    ///     "   │    2m ago: tests",
    ///     "   └─ cccc3333  offline",
    ///     "        seen 5m ago",
    /// ], "{text}");
    ///
    /// // `--by repo`: the repository comes first.
    /// let by_repo = Show { by: By::Repo, ..Show::default() };
    /// top.show = &by_repo;
    /// let text = anstream::adapter::strip_str(&top.view()).to_string();
    /// let rows: Vec<&str> = text
    ///     .rsplit("\n\n")
    ///     .next()
    ///     .unwrap()
    ///     .lines()
    ///     .filter(|l| l.contains(" session"))
    ///     .collect();
    /// assert_eq!(rows, [
    ///     "r  › mike  owner  online  4 sessions: 1 busy, 1 idle, 1 blocked, 2 claims",
    ///     "├─ pangolin  1 session: 1 blocked, 1 claim",
    ///     "└─ thelio  3 sessions: 1 busy, 1 idle, 1 claim",
    ///     "s  › mike  owner  online  › pangolin  1 session: 1 busy, 1 claim",
    /// ], "{text}");
    /// top.show = &all;
    ///
    /// // A narrow terminal cuts the wide line with `…`.
    /// top.width = 40;
    /// let text = anstream::adapter::strip_str(&top.view()).to_string();
    /// assert!(text.contains("\n   │    working on #12 Show the wave in…\n"), "{text}");
    /// assert!(text.lines().all(|l| l.chars().count() <= 40), "{text}");
    ///
    /// // While the looks fail, the line of the fault is first, and the
    /// // table of the last good look stays.
    /// top.width = 80;
    /// let table = anstream::adapter::strip_str(&top.view()).to_string();
    /// top.fault = Some("riff: no good look since 21:35:07: cannot reach riff-server");
    /// let text = anstream::adapter::strip_str(&top.view()).to_string();
    /// assert_eq!(text, format!("{}\n{table}", top.fault.unwrap()));
    /// ```
    pub fn view(&self) -> String {
        let here = self.repo.and_then(|repo| repo.parse().ok());
        let (mut facts, paused) = view::state_facts(self.pauses, here.as_ref());
        match self.owner {
            RiffOwner::NoSignIn => {}
            RiffOwner::Nobody => facts.push(("owner", "none".into())),
            RiffOwner::Owner { user, email } => {
                facts.push(("owner", format!("{} ({})", safe(user), safe(email))));
            }
        }
        facts.extend(view::build_facts(self.server));
        let mut out = String::new();
        if let Some(fault) = self.fault {
            let _ = writeln!(out, "{}", fit(&styled(ERROR, &safe(fault)), self.width));
        }
        for line in view::facts(&facts).lines().chain(paused.as_deref()) {
            let _ = writeln!(out, "{}", fit(line, self.width));
        }
        let shown: Vec<&SessionInfo> = self
            .sessions
            .iter()
            .filter(|s| self.show.shows(s))
            .collect();
        let mut lines = Vec::new();
        let blocks: Vec<&SessionInfo> = shown.iter().copied().filter(|s| blocked(s)).collect();
        if !blocks.is_empty() {
            lines.push(Line::new("", vec![]));
        }
        for s in blocks {
            lines.push(Line::new("", vec![(block_line(s), ERROR)]));
        }
        for repo in board_repos(self.sessions, self.show, self.repo) {
            let Some((wave, items)) = self.issues.get(&repo).and_then(|i| i.wave.as_ref()) else {
                continue;
            };
            lines.push(Line::new("", vec![]));
            let wave = format!("{} ({})", safe(wave), safe(&repo));
            lines.push(Line::new("", vec![(wave, BOLD)]));
            lines.extend(self.board(&repo, items));
        }
        lines.push(Line::new("", vec![]));
        let people = self.people();
        // A person on the command line has no session, and no row.
        let agents: Vec<&SessionInfo> = shown
            .into_iter()
            .filter(|s| s.uri.who().session().is_some())
            .collect();
        let tops = match self.show.by {
            By::Person => {
                let mut tops = groups(&agents, &[Level::Person, Level::Host, Level::Repo]);
                for user in people.keys().filter(|u| self.show.shows_person(u)) {
                    if !tops.iter().any(|g| &g.key == user) {
                        tops.push(Group {
                            level: Level::Person,
                            key: user.clone(),
                            sessions: Vec::new(),
                            kids: Vec::new(),
                        });
                    }
                }
                tops.sort_by(|a, b| a.key.cmp(&b.key));
                tops
            }
            By::Repo => groups(&agents, &[Level::Repo, Level::Person, Level::Host]),
        };
        for group in &tops {
            self.draw(group, "", "", &people, &mut lines);
        }
        for line in &lines {
            let _ = writeln!(out, "{}", line.render(self.width).trim_end());
        }
        out
    }

    /// The lines of the row `group` and of each row and session under
    /// it: the first line after `pre`, each line under it after
    /// `under`. A row with one row under it stays on one line with it.
    fn draw(
        &self,
        group: &Group,
        pre: &str,
        under: &str,
        people: &BTreeMap<String, PersonRow>,
        lines: &mut Vec<Line>,
    ) {
        let mut chain = vec![group];
        while let [only] = chain[chain.len() - 1].kids.as_slice() {
            chain.push(only);
        }
        let end = chain[chain.len() - 1];
        let mut parts = Vec::new();
        for (i, row) in chain.iter().enumerate() {
            let mut label = self.label(row, people);
            if i > 0
                && let Some((text, _)) = label.first_mut()
            {
                *text = format!("› {text}");
            }
            parts.extend(label);
        }
        if !group.sessions.is_empty() {
            parts.push((counts(&group.sessions), MUTED));
        }
        lines.push(Line::new(pre, parts));
        // The numbers of the machine of each host on the line
        // (01M421QPZ9E01PQ62PDBH378SJ).
        let bar = if end.sessions.is_empty() {
            "   "
        } else {
            "│  "
        };
        let more = format!("{under}{bar}");
        for row in chain.iter().filter(|r| r.level == Level::Host) {
            let user = row.sessions.first().map(|s| s.uri.who().user());
            let machine = self
                .machines
                .iter()
                .find(|m| Some(m.user.as_str()) == user && m.host == row.key);
            if let Some(m) = machine {
                lines.push(Line::new(more.clone(), numbers(&m.status)));
                if let Some(kill) = kill_line(&m.status) {
                    lines.push(Line::new(more.clone(), vec![kill]));
                }
            }
        }
        if end.kids.is_empty() {
            for (i, s) in end.sessions.iter().enumerate() {
                let last = i + 1 == end.sessions.len();
                let pre = format!("{under}{}", branch(last));
                let more = format!("{under}{}  ", if last { "   " } else { "│  " });
                lines.extend(self.session(pre, &more, s));
            }
        }
        for (i, kid) in end.kids.iter().enumerate() {
            let last = i + 1 == end.kids.len();
            let pre = format!("{under}{}", branch(last));
            let next = format!("{under}{}", if last { "   " } else { "│  " });
            self.draw(kid, &pre, &next, people, lines);
        }
    }

    /// The parts of the line of a row: a person ([`person_parts`]), a
    /// host, or a repository ([`Top::repo_name`]).
    fn label(
        &self,
        row: &Group,
        people: &BTreeMap<String, PersonRow>,
    ) -> Vec<(String, anstyle::Style)> {
        match row.level {
            Level::Person => {
                let away = PersonRow {
                    role: PersonRole::Member,
                    live: false,
                    seen_secs: None,
                };
                person_parts(&row.key, people.get(&row.key).unwrap_or(&away))
            }
            Level::Host => vec![(safe(&row.key), anstyle::Style::new())],
            Level::Repo => {
                let name = row.sessions.first().map(|s| Self::repo_name(s));
                vec![(
                    name.unwrap_or_else(|| safe(&row.key)),
                    anstyle::Style::new(),
                )]
            }
        }
    }

    /// Each person by USER: each member of `who`, and each user with a
    /// session. A user that `who` does not list, as in a riff with no
    /// sign-in, gets the time from its sessions.
    fn people(&self) -> BTreeMap<String, PersonRow> {
        let mut people: BTreeMap<String, PersonRow> = self
            .people
            .iter()
            .map(|p| {
                let row = PersonRow {
                    role: p.role,
                    live: p.live,
                    seen_secs: p.seen_secs,
                };
                (p.user.clone(), row)
            })
            .collect();
        for s in self.sessions {
            let user = s.uri.who().user();
            if self.people.iter().any(|p| p.user == user) {
                continue;
            }
            let row = people.entry(user.to_owned()).or_insert(PersonRow {
                role: if self.owner.is(user) {
                    PersonRole::Owner
                } else {
                    PersonRole::Member
                },
                live: false,
                seen_secs: None,
            });
            row.live |= s.live;
            row.seen_secs = Some(row.seen_secs.map_or(s.idle_secs, |t| t.min(s.idle_secs)));
        }
        people
    }

    /// The board of the wave of `repo`: one line for each group of its
    /// open items that is not empty. An item with a verify claim is in
    /// `verify`, an item with another claim in `claimed`. An item with
    /// no claim whose pull request waits for a verify or for the merge
    /// is in `verify` too: it is no work for a build
    /// (01M3Z9N6X92KT051P10CKKV7EK). Each other item is in `free`. Only
    /// a claim in `repo` counts: an issue of another repository can
    /// have the same number.
    fn board(&self, repo: &str, items: &[u64]) -> Vec<Line> {
        let waits = |n: &u64| self.issues.get(repo).is_some_and(|i| i.verify.contains(n));
        let mut groups: [(&str, Vec<String>); 3] =
            [("free", vec![]), ("claimed", vec![]), ("verify", vec![])];
        for n in items {
            let claims: Vec<&String> = self
                .sessions
                .iter()
                .filter(|s| s.uri.place().repo_text() == repo)
                .flat_map(|s| s.uri.claims().iter())
                .filter(|c| issue_of(c) == Some(*n))
                .collect();
            let group = if claims.iter().any(|c| c.starts_with("verify-"))
                || (claims.is_empty() && waits(n))
            {
                2
            } else if claims.is_empty() {
                0
            } else {
                1
            };
            groups[group].1.push(format!("#{n}"));
        }
        groups
            .into_iter()
            .filter(|(_, items)| !items.is_empty())
            .map(|(name, items)| {
                let line = format!("{name}: {}", items.join(" "));
                Line::new("  ", vec![(line, anstyle::Style::new())])
            })
            .collect()
    }

    /// The lines of the session `s`: the first line after `pre`, each
    /// line of its detail after `more`.
    fn session(&self, pre: String, more: &str, s: &SessionInfo) -> Vec<Line> {
        let mut tags = Vec::new();
        if s.uri.lead() {
            tags.push("lead");
        }
        if s.worker {
            tags.push("worker");
        }
        // The parts of the label of the session that its rows above do
        // not show (01M4CPVJ9ANPEBTWY9GETE2DGW).
        let label = text::Label::of(&s.uri);
        let worktree = label
            .worktree
            .map(|w| format!("#{}", safe(&w)))
            .unwrap_or_default();
        let head = Line::new(
            pre,
            vec![
                (short(s), session_style(&s.uri)),
                (worktree, anstyle::Style::new()),
                (tags.join(" "), MUTED),
                (state::of(s).word().to_owned(), state::style(state::of(s))),
            ],
        );
        let issues = self.issues.get(&s.uri.place().repo_text());
        let title = |n| issues?.titles.get(&n).cloned();
        std::iter::once(head)
            .chain(
                state::detail(s, &title)
                    .into_iter()
                    .map(|part| Line::new(more, vec![part])),
            )
            .collect()
    }

    /// The name of the repository of `s`: [`text::Label::repo`], so the
    /// repository row and the status line of `s` show one name
    /// (01M3WNHCD659FH3Z5VYYH69WWR, 01M4CPVJ9ANPEBTWY9GETE2DGW).
    fn repo_name(s: &SessionInfo) -> String {
        let label = text::Label::of(&s.uri);
        safe(&label.repo.unwrap_or_else(|| s.uri.place().repo_text()))
    }
}

/// The parts of the line of a person: the USER, the role tag, and
/// [`state::person`].
fn person_parts(user: &str, person: &PersonRow) -> Vec<(String, anstyle::Style)> {
    let role = person.role.tag().unwrap_or_default().to_owned();
    let mut parts = vec![(safe(user), crate::style::person(user)), (role, MUTED)];
    parts.extend(state::person(person.live, person.seen_secs));
    parts
}

/// The branch of a tree row: the last one closes the tree.
fn branch(last: bool) -> &'static str {
    if last { "└─ " } else { "├─ " }
}

/// The line of a blocked session before the board
/// (01M41FZR4XRP55M409YBCPTHPH): `blocked`, the short session ID, its
/// claims, the time that it waits, `the lead gave no answer` when the
/// lead gave none (01M41FZQCHWY1YVGAZ60ZHJK21), then the reason. The
/// facts come first: a narrow terminal cuts the end.
fn block_line(s: &SessionInfo) -> String {
    let mut line = format!("blocked  {}", short(s));
    if !s.uri.claims().is_empty() {
        let _ = write!(line, " {}", safe(&s.uri.claims().join(" ")));
    }
    if let Some(block) = &s.blocked {
        let _ = write!(line, ", {}", crate::text::ago(block.secs));
        if block.unanswered {
            line.push_str(", the lead gave no answer");
        }
        let _ = write!(line, ": {}", safe(&block.reason));
    }
    line
}

/// True when the state of `s` is `blocked`.
fn blocked(s: &SessionInfo) -> bool {
    state::of(s) == SessionState::Blocked
}

/// The short session ID of the [`text::Label`] of `s`.
fn short(s: &SessionInfo) -> String {
    text::Label::of(&s.uri).id.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{DIM, GOOD};

    fn plain(line: &Line, width: usize) -> String {
        anstream::adapter::strip_str(&line.render(width)).to_string()
    }

    /// A live session at `uri` in `state`, with no status.
    fn info(uri: &str, state: SessionState) -> SessionInfo {
        SessionInfo {
            uri: uri.parse().unwrap(),
            live: state != SessionState::Offline,
            idle_secs: 0,
            status: None,
            worker: false,
            stopping: false,
            claims_secs: 0,
            must_clear: false,
            fresh_secs: None,
            state: Some(state),
            work: None,
            waits: None,
            blocked: None,
            step: None,
        }
    }

    /// The text of `riff top` for `sessions` with `issues` and `show`,
    /// with no styles.
    fn shown(sessions: &[SessionInfo], issues: &BTreeMap<String, Issues>, show: &Show) -> String {
        let owner = RiffOwner::NoSignIn;
        let running = riff_core::wire::RiffState::Running.into();
        let top = Top {
            pauses: &running,
            owner: &owner,
            server: None,
            sessions,
            people: &[],
            issues,
            repo: None,
            show,
            width: 100,
            fault: None,
            machines: &[],
        };
        anstream::adapter::strip_str(&top.view()).to_string()
    }

    /// The lines of the tree: the last part of the text.
    fn tree(text: &str) -> Vec<String> {
        let part = text.rsplit("\n\n").next().unwrap();
        part.lines().map(str::to_owned).collect()
    }

    /// 2 people, 3 hosts and 2 repositories.
    fn riff_of_two() -> Vec<SessionInfo> {
        use SessionState::{Blocked, Busy, Idle};
        vec![
            info(
                "riff://mike@pangolin/o/riff?session=m1&claim=issue-7#issue-7",
                Busy,
            ),
            info("riff://mike@pangolin/o/riff?session=m2", Idle),
            info("riff://mike@pangolin/o/strata?session=m3&lead=true", Idle),
            info(
                "riff://mike@thelio/o/riff?session=m4&claim=issue-8",
                Blocked,
            ),
            info(
                "riff://brett@kadomony/o/strata?session=b1&claim=issue-88#issue-88",
                Busy,
            ),
            info("riff://brett@kadomony/o/strata?session=b2&lead=true", Idle),
        ]
    }

    /// `riff top` and `riff statusline` give one label for one session:
    /// the user, the host and the repository of the rows of the tree,
    /// then the short ID and the worktree of the row of the session
    /// make the start of the status line (01M4CPVJ9ANPEBTWY9GETE2DGW).
    #[test]
    fn top_and_the_status_line_give_one_label() {
        use SessionState::{Busy, Idle};
        let id = "8c7f26da-5cd6-4ce9";
        let mut worker = info(
            &format!("riff://mike@pangolin/o/riff?session={id}&claim=issue-604#issue-604"),
            Busy,
        );
        worker.worker = true;
        let lead = info(
            "riff://mike@pangolin/o/riff?session=74758398-31cb&lead=true",
            Idle,
        );
        for s in [worker, lead] {
            let text = shown(std::slice::from_ref(&s), &BTreeMap::new(), &Show::default());
            let rows = tree(&text);
            let words = |row: &str| -> Vec<String> {
                row.split_whitespace()
                    .filter(|w| !matches!(*w, "›" | "├─" | "└─"))
                    .map(str::to_owned)
                    .collect()
            };
            let top = words(&rows[0]);
            let session = words(&rows[1]);
            let (user, host, repo, short) = (&top[0], &top[2], &top[3], &session[0]);
            let worktree = session
                .get(1)
                .filter(|w| w.starts_with('#'))
                .map_or("", String::as_str);
            let label = format!("{user}@{host}:{repo}{worktree} ({short})");
            assert_eq!(label, text::name(&s.uri), "{text}");
            let id = s.uri.who().session().unwrap();
            let line = text::statusline(id, Some(&s));
            assert!(line.starts_with(&format!("{label} ")), "{line} {text}");
        }
    }

    /// With repositories of two owners, the repository row of
    /// `riff top` and the status line of each session still give one
    /// name: the name of [`text::Label::repo`]
    /// (01M3WNHCD659FH3Z5VYYH69WWR, 01M4CPVJ9ANPEBTWY9GETE2DGW).
    #[test]
    fn two_owners_give_one_label_in_top_and_the_status_line() {
        use SessionState::{Busy, Idle};
        let worker = info(
            "riff://mike@pangolin/o/riff?session=8c7f26da-5cd6&claim=issue-604#issue-604",
            Busy,
        );
        let other = info(
            "riff://mike@pangolin/n/dotfiles?session=dbb36565-00e9",
            Idle,
        );
        let sessions = [worker, other];
        let text = shown(&sessions, &BTreeMap::new(), &Show::default());
        let rows = tree(&text);
        for s in &sessions {
            let label = text::Label::of(&s.uri);
            let short = label.id.clone().unwrap();
            let at = rows
                .iter()
                .position(|r| r.split_whitespace().any(|w| w == short))
                .unwrap_or_else(|| panic!("{short}: {text}"));
            let repo = rows[at - 1]
                .split_whitespace()
                .find(|w| !matches!(*w, "│" | "├─" | "└─"))
                .unwrap();
            assert_eq!(Some(repo), label.repo.as_deref(), "{text}");
            let id = s.uri.who().session().unwrap();
            let line = text::statusline(id, Some(s));
            assert!(line.starts_with(&format!("{label} ")), "{line} {text}");
        }
    }

    /// The tree has four levels with the counts on each person, host
    /// and repository line. A person with one host and one repository
    /// gets the short form (01M42KHN33M4K13GKTX2WM6CMM).
    #[test]
    fn two_people_three_hosts_and_two_repositories_give_the_four_level_tree() {
        let text = shown(&riff_of_two(), &BTreeMap::new(), &Show::default());
        assert_eq!(
            tree(&text),
            [
                "brett  online  › kadomony  › strata  2 sessions: 1 busy, 1 idle, 1 claim",
                "├─ b1  #issue-88  busy",
                "│    working on #88",
                "└─ b2  lead  idle",
                "     monitoring work for 0s",
                "mike  online  4 sessions: 1 busy, 2 idle, 1 blocked, 2 claims",
                "├─ pangolin  3 sessions: 1 busy, 2 idle, 1 claim",
                "│  ├─ riff  2 sessions: 1 busy, 1 idle, 1 claim",
                "│  │  ├─ m1  #issue-7  busy",
                "│  │  │    working on #7",
                "│  │  └─ m2  idle",
                "│  │       ready for work for 0s",
                "│  └─ strata  1 session: 1 idle",
                "│     └─ m3  lead  idle",
                "│          monitoring work for 0s",
                "└─ thelio  › riff  1 session: 1 blocked, 1 claim",
                "   └─ m4  blocked",
                "        working on #8",
            ],
            "{text}"
        );
    }

    /// Each repository with a live session has its own board. A
    /// repository whose read of `gh` failed shows its sessions, no
    /// board and no error line (01M42KHN80V49HDDZF953HXDT0).
    #[test]
    fn each_repository_has_its_own_board_and_a_failed_read_shows_none() {
        let wave = |n: u64| {
            let json =
                format!(r#"[{{"number": {n}, "title": "t", "milestone": {{"title": "Wave 3"}}}}]"#);
            Issues::parse(&json).unwrap()
        };
        let both = BTreeMap::from([
            ("o/riff".to_owned(), wave(7)),
            ("o/strata".to_owned(), wave(88)),
        ]);
        let text = shown(&riff_of_two(), &both, &Show::default());
        assert!(
            text.contains(
                "\n\nWave 3 (o/riff)\n  claimed: #7\n\nWave 3 (o/strata)\n  claimed: #88\n\n"
            ),
            "{text}"
        );
        // The read of strata failed.
        let riff = BTreeMap::from([("o/riff".to_owned(), wave(7))]);
        let text = shown(&riff_of_two(), &riff, &Show::default());
        assert!(
            text.contains("\n\nWave 3 (o/riff)\n  claimed: #7\n\n"),
            "{text}"
        );
        assert!(!text.contains("(o/strata)"), "{text}");
        assert!(!text.to_lowercase().contains("error"), "{text}");
        assert!(text.contains("├─ b1  #issue-88  busy"), "{text}");
    }

    /// `--user`, `--host` and `--repo` show only their sessions, also
    /// together, and only the boards of their repositories
    /// (01M42KHNCBMBCT3TFBYWE339H5).
    #[test]
    fn the_filters_show_a_part() {
        let sessions = riff_of_two();
        let show = |user: Option<&str>, host: Option<&str>, repo: Option<&str>| Show {
            user: user.map(str::to_owned),
            host: host.map(str::to_owned),
            repo: repo.map(str::to_owned),
            by: By::Person,
        };
        let ids = |show: &Show| -> Vec<String> {
            sessions
                .iter()
                .filter(|s| show.shows(s))
                .map(short)
                .collect()
        };
        assert_eq!(ids(&show(Some("brett"), None, None)), ["b1", "b2"]);
        assert_eq!(ids(&show(None, Some("thelio"), None)), ["m4"]);
        assert_eq!(ids(&show(None, None, Some("o/strata"))), ["m3", "b1", "b2"]);
        assert_eq!(
            ids(&show(Some("mike"), Some("pangolin"), Some("o/strata"))),
            ["m3"]
        );
        let repos: Vec<String> =
            board_repos(&sessions, &show(None, Some("thelio"), None), Some("o/here"))
                .into_iter()
                .collect();
        assert_eq!(repos, ["o/riff"]);
        // The repository of the working directory gets a board too,
        // with no `--user` and no `--host`, when `--repo` matches it.
        let here = |show: &Show| board_repos(&sessions, show, Some("o/here")).contains("o/here");
        assert!(here(&Show::default()));
        assert!(!here(&show(None, None, Some("o/riff"))));
        assert!(!here(&show(Some("mike"), None, None)));
        // A person with no session shows only with no filter of a host
        // or a repository.
        let ann = show(None, None, None);
        assert!(ann.shows_person("ann"));
        assert!(!show(None, Some("pangolin"), None).shows_person("ann"));
        assert!(!show(Some("mike"), None, None).shows_person("ann"));
    }

    #[test]
    fn a_wide_line_is_cut_with_an_ellipsis() {
        let line = Line::new("├─ ", vec![("abcdef".into(), ERROR), ("gh".into(), DIM)]);
        assert_eq!(plain(&line, 80), "├─ abcdef  gh");
        assert_eq!(plain(&line, 13), "├─ abcdef  gh");
        assert_eq!(plain(&line, 12), "├─ abcdef…");
        assert_eq!(plain(&line, 7), "├─ abc…");
        assert_eq!(plain(&line, 2), "├…");
    }

    #[test]
    fn a_wide_terminal_cuts_nothing() {
        let status = "blocked 2m: waits for a verify of the pull request ".repeat(3);
        let line = Line::new("   │    ", vec![(status.clone(), ERROR)]);
        assert_eq!(plain(&line, 200), format!("   │    {status}"));
        assert!(plain(&line, 80).ends_with('…'));
    }

    #[test]
    fn a_cut_counts_wide_characters_as_two_columns() {
        let line = Line::new("", vec![("日本語です".into(), anstyle::Style::new())]);
        assert_eq!(plain(&line, 10), "日本語です");
        assert_eq!(plain(&line, 6), "日本…");
    }

    #[test]
    fn a_cut_keeps_the_style_of_the_cut_part() {
        let line = Line::new("", vec![("abcdef".into(), ERROR)]);
        assert_eq!(line.render(4), styled(ERROR, "abc…"));
    }

    #[test]
    fn a_wide_fact_line_loses_its_styles_and_is_cut() {
        let line = format!("riff   {}", styled(GOOD, "running"));
        assert_eq!(fit(&line, 80), line);
        assert_eq!(fit(&line, 10), "riff   ru…");
    }

    #[test]
    fn the_lowest_wave_is_the_current_wave() {
        let json = r#"[
            {"number": 1, "title": "a", "milestone": {"title": "Wave 10"}},
            {"number": 3, "title": "b", "milestone": {"title": "Wave 9"}},
            {"number": 2, "title": "c", "milestone": {"title": "Wave 9"}}
        ]"#;
        let issues = Issues::parse(json).unwrap();
        assert_eq!(issues.wave, Some(("Wave 9".into(), vec![2, 3])));
    }

    #[test]
    fn no_wave_milestone_gives_no_wave() {
        let issues = Issues::parse(r#"[{"number": 1, "title": "a", "milestone": null}]"#);
        assert_eq!(issues.unwrap().wave, None);
    }

    /// A `gh` that fails keeps the last issues, and a `gh` that works
    /// replaces them (01M3ZC09FA9DZPTHK31XECZ566).
    #[test]
    fn a_read_of_gh_that_fails_keeps_the_last_issues() {
        let wave = |n: u64| {
            let json = format!(
                r#"[{{"number": {n}, "title": "a", "milestone": {{"title": "Wave {n}"}}}}]"#
            );
            Issues::parse(&json)
        };
        let mut issues = wave(9);
        for read in [None, None, wave(10), None] {
            issues = Issues::newest(issues, read);
        }
        assert_eq!(issues, wave(10));
        // With no `gh` from the start, there are no issues.
        assert_eq!(Issues::newest(None, None), None);
    }
}
