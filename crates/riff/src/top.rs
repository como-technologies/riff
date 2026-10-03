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
//! `gh issue list` of the open issues ([`Issues`]). riff keeps them for
//! [`ISSUES_TTL`], then reads them again. With no `gh`, the table has
//! no titles and no board. When a later read of `gh` fails, riff keeps
//! the last issues ([`Issues::newest`], 01M3ZC09FA9DZPTHK31XECZ566): a
//! network fault stops `gh` too.
//!
//! The author of a pull request releases its item at the verify
//! request. So an item with no claim can wait for a verify or for the
//! merge. One `gh pr list` of the open pull requests gives these items
//! ([`Issues::verify`], [`crate::rollout::in_verify`]). The board shows
//! them in `verify`, not in `free` (01M3Z9N6X92KT051P10CKKV7EK). When
//! that call fails, the board counts only the claims.
//!
//! The rows are a tree for each person: the person, each host, and each
//! session on the host (01M3NT4M5D36KTZ5XZMDP6QFQT). A session gets only
//! the tag of its role, `lead` or `worker`, from the server, so each
//! machine shows the same (01M3NT4M159EHN5W8JRTQ417N4). The owner is a
//! person: the tag `owner` is on the row of the person.
//!
//! One riff holds the sessions of more than one repository. So each
//! session line names its place, the repository and the worktree, as
//! `riff who` does (01M3WNHCD659FH3Z5VYYH69WWR). The board and the
//! titles are those of one repository, [`Top::repo`]: the wave line
//! names it, and a claim in another repository is not on the board.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant T as riff top
//!     participant G as gh
//!     participant S as riff-server
//!     T->>G: issue list, pr list (each ISSUES_TTL)
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
//! ([`crate::api::passes`]). A look also fails when it gets no reply
//! in [`LOOK_WAIT`]: a dead connection can give no error. `riff top`
//! keeps the last table, shows [`Top::fault`] as its first line, and
//! looks again at its interval, with new connections
//! ([`crate::api::Api::reconnected`]). The line goes at the next good
//! look.
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

use crate::state;
use crate::style::{BOLD, ERROR, MUTED, session as session_style, styled};
use crate::text::safe;
use crate::view;

/// The time between two draws of `riff top` with no message.
pub const REFRESH: Duration = Duration::from_secs(3);

/// The longest time that `riff top` waits for one look, when it runs
/// until stopped (01M3Z8FXE2DY34ZP75WJE1S8HR).
pub const LOOK_WAIT: Duration = Duration::from_secs(10);

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

/// What `riff top` shows.
pub struct Top<'a> {
    /// The pauses of the riff, as the caller sees them.
    pub pauses: &'a RiffReply,
    pub owner: &'a RiffOwner,
    pub server: Option<&'a Build>,
    pub sessions: &'a [SessionInfo],
    /// The members of the riff from `who`, also when away.
    pub people: &'a [Person],
    pub issues: Option<&'a Issues>,
    /// The repository of the issues, `OWNER/REPO`: the repository of
    /// the working directory. `None` outside git.
    pub repo: Option<&'a str>,
    /// The width of the terminal in columns: no line is wider.
    pub width: usize,
    /// The line of a look that failed ([`crate::text::top_fault`]),
    /// while the table is the one of the last good look
    /// (01M3Z8FXE2DY34ZP75WJE1S8HR).
    pub fault: Option<&'a str>,
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

impl Top<'_> {
    /// The header, the board of the current wave, then a tree for each
    /// person (01M3NT4M5D36KTZ5XZMDP6QFQT): the person, each host of the
    /// person, and each session on the host. The tree grows down, not
    /// across: no line is wider than [`Top::width`]. riff cuts a wider
    /// line with `…` (01M3QA8EZHX5B8C9CKF8Q3154X). No line has a column
    /// heading.
    ///
    /// - The header: the facts `riff`, `owner` and `build`, as in
    ///   `riff who`.
    /// - The board: the current wave with its repository
    ///   ([`Top::repo`]), then one line for each group of its open
    ///   items: `free`, `claimed`, and `verify` for an item with a
    ///   verify claim, and for an item with no claim whose pull request
    ///   waits for a verify or for the merge. Only a claim in that
    ///   repository counts.
    /// - A person line: the USER in bold color, the role tag `owner` or
    ///   `admin`, and [`state::person`]. Each member of `who` gets a
    ///   line, also when away.
    /// - A session: the first line has the short session ID, its place
    ///   `REPO#WORKTREE` (01M3WNHCD659FH3Z5VYYH69WWR), the role tag
    ///   `lead` or `worker`, and the state word that the server derives
    ///   (01M3QB6CJ1XCQG5B1BVR8AF3B4). Under it comes one line for each
    ///   fact of the [`state::detail`]. A session with no detail takes
    ///   one line. An issue has its title only in the repository of
    ///   the board.
    ///
    /// People come by USER, and hosts by name. In each person, blocked
    /// sessions come first. Each blocked session also has a red line
    /// before the board: the session, its claims, the reason, the time
    /// that it waits, and `the lead gave no answer` when the lead gave
    /// none (01M41FZR4XRP55M409YBCPTHPH, 01M41FZQCHWY1YVGAZ60ZHJK21). It
    /// has ANSI styles: print it through `anstream`.
    ///
    /// ```
    /// use riff::top::{Issues, Top};
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
    ///     // A session of another repository: its claim is not on the board.
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
    /// );
    /// // The author of #15 asked for a verify and released the item.
    /// let issues = issues.map(|i| i.with_pulls(
    ///     r#"[{"number": 40, "headRefName": "worktree-issue-15", "isDraft": false, "statusCheckRollup": []}]"#,
    /// ));
    /// let owner = RiffOwner::Owner { user: "mike".into(), email: "m@x.io".into() };
    /// let running = RiffState::Running.into();
    /// let mut top = Top {
    ///     pauses: &running,
    ///     owner: &owner,
    ///     server: None,
    ///     sessions: &sessions,
    ///     people: &people,
    ///     issues: issues.as_ref(),
    ///     repo: Some("o/r"),
    ///     width: 80,
    ///     fault: None,
    /// };
    /// let text = anstream::adapter::strip_str(&top.view()).to_string();
    /// assert!(text.starts_with("riff   running\nowner  mike (m@x.io)\nbuild  "), "{text}");
    /// assert!(
    ///     text.contains("\n\nblocked  dddd4444 verify-issue-13, 25m, the lead gave no answer: waits for the…\n"),
    ///     "{text}"
    /// );
    /// assert!(text.contains("\n\nWave 3 (o/r)\n  free: #14\n  claimed: #12\n  verify: #13 #15\n\n"), "{text}");
    /// let tree: Vec<&str> = text.rsplit("\n\n").next().unwrap().lines().collect();
    /// assert_eq!(tree, [
    ///     "ann  admin  offline  seen 1h ago",
    ///     "mike  owner  online",
    ///     "├─ pangolin",
    ///     "│  ├─ dddd4444  r#verify-13  worker  blocked",
    ///     "│  │    waits for the lead (25m ago)",
    ///     "│  │    the lead gave no answer",
    ///     "│  │    reviewing #13 Later",
    ///     "│  └─ eeee5555  s  lead  busy",
    ///     "│       working on #14",
    ///     "│       2m ago: plan",
    ///     "└─ thelio",
    ///     "   ├─ aaaa1111  r  lead  idle",
    ///     "   │    monitoring work for 10m",
    ///     "   │    2m ago: plan",
    ///     "   ├─ bbbb2222  r  worker  busy",
    ///     "   │    working on #12 Show the wave in the board of riff top",
    ///     "   │    2m ago: tests",
    ///     "   └─ cccc3333  r  offline",
    ///     "        seen 5m ago",
    /// ], "{text}");
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
        let mut lines = Vec::new();
        let blocks: Vec<&SessionInfo> = self.sessions.iter().filter(|s| blocked(s)).collect();
        if !blocks.is_empty() {
            lines.push(Line::new("", vec![]));
        }
        for s in blocks {
            lines.push(Line::new("", vec![(block_line(s), ERROR)]));
        }
        if let Some((wave, items)) = self.issues.and_then(|i| i.wave.as_ref()) {
            lines.push(Line::new("", vec![]));
            let wave = match self.repo {
                Some(repo) => format!("{} ({})", safe(wave), safe(repo)),
                None => safe(wave),
            };
            lines.push(Line::new("", vec![(wave, BOLD)]));
            lines.extend(self.board(items));
        }
        lines.push(Line::new("", vec![]));
        for (user, person) in self.people() {
            lines.push(person_line(&user, &person));
            let hosts = self.hosts(&user);
            for (h, (host, sessions)) in hosts.iter().enumerate() {
                let last_host = h + 1 == hosts.len();
                lines.push(Line::new(
                    branch(last_host),
                    vec![(safe(host), anstyle::Style::new())],
                ));
                let under = if last_host { "   " } else { "│  " };
                for (i, s) in sessions.iter().enumerate() {
                    let last = i + 1 == sessions.len();
                    let pre = format!("{under}{}", branch(last));
                    let more = format!("{under}{}  ", if last { "   " } else { "│  " });
                    lines.extend(self.session(pre, &more, s));
                }
            }
        }
        for line in &lines {
            let _ = writeln!(out, "{}", line.render(self.width).trim_end());
        }
        out
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

    /// The hosts of `user` by name, each with its agent sessions:
    /// blocked first, then by session ID. A person on the command line
    /// has no session, and no row.
    fn hosts(&self, user: &str) -> Vec<(String, Vec<&SessionInfo>)> {
        let mut hosts: BTreeMap<String, Vec<&SessionInfo>> = BTreeMap::new();
        for s in self.sessions {
            if s.uri.who().user() == user && s.uri.who().session().is_some() {
                hosts
                    .entry(s.uri.place().host().to_owned())
                    .or_default()
                    .push(s);
            }
        }
        hosts
            .into_iter()
            .map(|(host, mut sessions)| {
                sessions.sort_by_key(|s| (!blocked(s), s.uri.who().session().map(str::to_owned)));
                (host, sessions)
            })
            .collect()
    }

    /// The board of the wave: one line for each group of its open
    /// items that is not empty. An item with a verify claim is in
    /// `verify`, an item with another claim in `claimed`. An item with
    /// no claim whose pull request waits for a verify or for the merge
    /// is in `verify` too: it is no work for a build
    /// (01M3Z9N6X92KT051P10CKKV7EK). Each other item is in `free`. Only
    /// a claim in the repository of the board counts: an issue of
    /// another repository can have the same number.
    fn board(&self, items: &[u64]) -> Vec<Line> {
        let waits = |n: &u64| self.issues.is_some_and(|i| i.verify.contains(n));
        let mut groups: [(&str, Vec<String>); 3] =
            [("free", vec![]), ("claimed", vec![]), ("verify", vec![])];
        for n in items {
            let claims: Vec<&String> = self
                .sessions
                .iter()
                .filter(|s| self.of_board(s))
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
        let head = Line::new(
            pre,
            vec![
                (short(s), session_style(&s.uri)),
                (self.place(s), anstyle::Style::new()),
                (tags.join(" "), MUTED),
                (state::of(s).word().to_owned(), state::style(state::of(s))),
            ],
        );
        let known = self.of_board(s);
        let title = |n| self.issues.filter(|_| known)?.titles.get(&n).cloned();
        std::iter::once(head)
            .chain(
                state::detail(s, &title)
                    .into_iter()
                    .map(|part| Line::new(more, vec![part])),
            )
            .collect()
    }

    /// True when `s` is in the repository of the board, or when riff
    /// does not know that repository.
    fn of_board(&self, s: &SessionInfo) -> bool {
        self.repo
            .is_none_or(|repo| s.uri.place().repo_text() == repo)
    }

    /// The place of `s` in the form of `riff who`: `REPO#WORKTREE`, and
    /// `-` for the repository outside git. The repository is its short
    /// name when the repositories of the sessions have one owner, else
    /// `OWNER/REPO` (01M3WNHCD659FH3Z5VYYH69WWR).
    fn place(&self, s: &SessionInfo) -> String {
        let place = s.uri.place();
        let mut out = match place.repo() {
            Repo::Git { name, .. } if self.one_owner() => safe(name),
            _ => safe(&place.repo_text()),
        };
        if let Some(worktree) = place.worktree() {
            out.push('#');
            out.push_str(&safe(worktree));
        }
        out
    }

    /// True when the repositories of the sessions have one owner.
    fn one_owner(&self) -> bool {
        let mut owners = self
            .sessions
            .iter()
            .filter_map(|s| match s.uri.place().repo() {
                Repo::Git { owner, .. } => Some(owner),
                Repo::None => None,
            });
        let first = owners.next();
        owners.all(|owner| Some(owner) == first)
    }
}

/// The line of a person: the USER, the role tag, and
/// [`state::person`].
fn person_line(user: &str, person: &PersonRow) -> Line {
    let role = person.role.tag().unwrap_or_default().to_owned();
    let mut parts = vec![(safe(user), crate::style::person(user)), (role, MUTED)];
    parts.extend(state::person(person.live, person.seen_secs));
    Line::new("", parts)
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

/// The short session ID of `riff who`.
fn short(s: &SessionInfo) -> String {
    let id = s.uri.who().session().unwrap_or_default();
    id.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{DIM, GOOD};

    fn plain(line: &Line, width: usize) -> String {
        anstream::adapter::strip_str(&line.render(width)).to_string()
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
