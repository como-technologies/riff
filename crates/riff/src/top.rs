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
//! [`ISSUES_TTL`]. With no `gh`, or when `gh` fails, the table has no
//! titles and no board.
//!
//! The rows are a tree for each person: the person, each host, and each
//! session on the host (01M3NT4M5D36KTZ5XZMDP6QFQT). A session gets only
//! the tag of its role, `lead` or `worker`, from the server, so each
//! machine shows the same (01M3NT4M159EHN5W8JRTQ417N4). The owner is a
//! person: the tag `owner` is on the row of the person.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant T as riff top
//!     participant G as gh
//!     participant S as riff-server
//!     T->>G: issue list (each ISSUES_TTL)
//!     loop each REFRESH, and each message of the thread
//!         T->>S: riff, who
//!         T->>T: draw the table in place
//!     end
//! ```

use std::collections::BTreeMap;
use std::fmt::Write;
use std::time::Duration;

use riff_core::build::Build;
use riff_core::wire::{Person, PersonRole, RiffOwner, RiffState, SessionInfo};
use serde::Deserialize;

use crate::style::{BOLD, DIM, ERROR, GOOD, MUTED, WARNING, session as session_style, styled};
use crate::text::{self, ago, safe};
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
}

#[derive(Deserialize)]
struct GhIssue {
    number: u64,
    title: String,
    milestone: Option<GhMilestone>,
}

#[derive(Deserialize)]
struct GhMilestone {
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
        })
    }

    /// The open issues of `repo` from `gh`. `None` when `gh` is not
    /// there or fails.
    pub fn from_gh(repo: &str) -> Option<Issues> {
        let out = std::process::Command::new("gh")
            .args(Self::gh_args(repo))
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Self::parse(&String::from_utf8_lossy(&out.stdout))
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
    pub state: RiffState,
    pub owner: &'a RiffOwner,
    pub server: Option<&'a Build>,
    pub sessions: &'a [SessionInfo],
    /// The members of the riff from `who`, also when away.
    pub people: &'a [Person],
    pub issues: Option<&'a Issues>,
    /// The width of the terminal in columns: no line is wider.
    pub width: usize,
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
    /// line with `…` (01M3QA8EZHX5B8C9CKF8Q3154X).
    ///
    /// - The header: the facts `riff`, `owner` and `build`, as in
    ///   `riff who`.
    /// - The board: the current wave, then one line for each group of
    ///   its open items: `free`, `claimed`, and `verify` for an item
    ///   with a verify claim.
    /// - A person line: the USER in bold color, the role tag `owner` or
    ///   `admin`, and the presence: `online`, or `offline` with the time
    ///   since the last call. Each member of `who` gets a line, also
    ///   when away.
    /// - A session: the first line has the short session ID, the role
    ///   tag `lead` or `worker`, and the presence. Under it comes what
    ///   the session does: a line for each claim with the title of its
    ///   issue, and a line for the status. A session with no claim and
    ///   no status takes one line.
    ///
    /// No line has a column heading. The status starts with the facts that the riff derives
    /// (01M3Q555KC1RKNEC4ZA9HQYJG2): `paused` while the riff is paused,
    /// the [`text::idle_worker`] time, and for the lead the current wave
    /// with its open items. The step that
    /// the session set comes after them, with its age. A stale step is
    /// dim and says `stale`: it is not the current state.
    ///
    /// People come by USER, and hosts by name. In each person, blocked
    /// sessions come first. It has ANSI styles: print it through
    /// `anstream`.
    ///
    /// ```
    /// use riff::top::{Issues, Top};
    /// use riff_core::wire::{Person, PersonRole, RiffOwner, RiffState, SessionInfo, Status, StatusInfo};
    ///
    /// let info = |uri: &str, step: &str, blocked: Option<&str>, worker| SessionInfo {
    ///     uri: uri.parse().unwrap(),
    ///     live: true,
    ///     idle_secs: 0,
    ///     status: Some(StatusInfo {
    ///         status: Status { step: step.into(), blocked: blocked.map(Into::into) },
    ///         age_secs: 120,
    ///         stale: false,
    ///     }),
    ///     worker,
    ///     stopping: false,
    ///     claims_secs: 600,
    /// };
    /// let sessions = [
    ///     info("riff://mike@thelio/o/r?session=aaaa1111&lead=true", "lead", None, false),
    ///     info("riff://mike@thelio/o/r?session=bbbb2222&claim=issue-12", "tests", None, true),
    ///     SessionInfo {
    ///         live: false,
    ///         idle_secs: 300,
    ///         ..info("riff://mike@thelio/o/r?session=cccc3333", "merge", Some("waits"), false)
    ///     },
    ///     info("riff://mike@pangolin/o/r?session=dddd4444", "docs", None, true),
    /// ];
    /// let people = [
    ///     Person { user: "ann".into(), role: PersonRole::Admin, live: false, seen_secs: Some(3600) },
    ///     Person { user: "mike".into(), role: PersonRole::Owner, live: true, seen_secs: Some(0) },
    /// ];
    /// let issues = Issues::parse(
    ///     r#"[{"number": 12, "title": "Show the wave in the board of riff top", "milestone": {"title": "Wave 3"}},
    ///         {"number": 13, "title": "Later", "milestone": {"title": "Wave 3"}}]"#,
    /// );
    /// let owner = RiffOwner::Owner { user: "mike".into(), email: "m@x.io".into() };
    /// let mut top = Top {
    ///     state: RiffState::Running,
    ///     owner: &owner,
    ///     server: None,
    ///     sessions: &sessions,
    ///     people: &people,
    ///     issues: issues.as_ref(),
    ///     width: 80,
    /// };
    /// let text = anstream::adapter::strip_str(&top.view()).to_string();
    /// assert!(text.starts_with("riff   running\nowner  mike (m@x.io)\nbuild  "), "{text}");
    /// assert!(text.contains("\n\nWave 3\n  free: #13\n  claimed: #12\n\n"), "{text}");
    /// let tree: Vec<&str> = text.rsplit("\n\n").next().unwrap().lines().collect();
    /// assert_eq!(tree, [
    ///     "ann  admin  offline 1h",
    ///     "mike  owner  online",
    ///     "├─ pangolin",
    ///     "│  └─ dddd4444  worker  online",
    ///     "│       idle 10m  2m docs",
    ///     "└─ thelio",
    ///     "   ├─ cccc3333  offline 5m",
    ///     "   │    blocked 2m: waits (step: merge)",
    ///     "   ├─ aaaa1111  lead  online",
    ///     "   │    Wave 3: #12 #13  2m lead",
    ///     "   └─ bbbb2222  worker  online",
    ///     "        issue-12 Show the wave in the board of riff top",
    ///     "        2m tests",
    /// ], "{text}");
    ///
    /// // A narrow terminal cuts the wide line with `…`.
    /// top.width = 40;
    /// let text = anstream::adapter::strip_str(&top.view()).to_string();
    /// assert!(text.contains("\n        issue-12 Show the wave in the b…\n"), "{text}");
    /// assert!(text.lines().all(|l| l.chars().count() <= 40), "{text}");
    /// ```
    pub fn view(&self) -> String {
        let (fact, paused) = view::state_fact(self.state);
        let mut facts = vec![fact];
        match self.owner {
            RiffOwner::NoSignIn => {}
            RiffOwner::Nobody => facts.push(("owner", "none".into())),
            RiffOwner::Owner { user, email } => {
                facts.push(("owner", format!("{} ({})", safe(user), safe(email))));
            }
        }
        facts.extend(view::build_facts(self.server));
        let mut out = String::new();
        for line in view::facts(&facts).lines().chain(paused.as_deref()) {
            let _ = writeln!(out, "{}", fit(line, self.width));
        }
        let mut lines = Vec::new();
        if let Some((wave, items)) = self.issues.and_then(|i| i.wave.as_ref()) {
            lines.push(Line::new("", vec![]));
            lines.push(Line::new("", vec![(safe(wave), BOLD)]));
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
    /// `verify`, an item with another claim in `claimed`, each other
    /// item in `free`.
    fn board(&self, items: &[u64]) -> Vec<Line> {
        let mut groups: [(&str, Vec<String>); 3] =
            [("free", vec![]), ("claimed", vec![]), ("verify", vec![])];
        for n in items {
            let claims: Vec<&String> = self
                .sessions
                .iter()
                .flat_map(|s| s.uri.claims().iter())
                .filter(|c| issue_of(c) == Some(*n))
                .collect();
            let group = if claims.iter().any(|c| c.starts_with("verify-")) {
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
    /// other line after `more`.
    fn session(&self, pre: String, more: &str, s: &SessionInfo) -> Vec<Line> {
        let mut tags = Vec::new();
        if s.uri.lead() {
            tags.push("lead");
        }
        if s.worker {
            tags.push("worker");
        }
        let mut lines = vec![Line::new(
            pre,
            vec![
                (short(s), session_style(&s.uri)),
                (tags.join(" "), MUTED),
                presence(s.live, Some(s.idle_secs)),
            ],
        )];
        for c in s.uri.claims() {
            let title = issue_of(c)
                .and_then(|n| self.issues?.titles.get(&n))
                .map(|t| format!(" {}", safe(t)));
            let item = format!("{}{}", safe(c), title.unwrap_or_default());
            lines.push(Line::new(more, vec![(item, anstyle::Style::new())]));
        }
        let mut status: Vec<(String, anstyle::Style)> = Vec::new();
        if self.state == RiffState::Paused {
            status.push(("paused".into(), WARNING));
        }
        status.extend(text::idle_worker(s).map(|idle| (idle, anstyle::Style::new())));
        if s.uri.lead()
            && let Some((wave, items)) = self.issues.and_then(|i| i.wave.as_ref())
        {
            let items: Vec<String> = items.iter().map(|n| format!("#{n}")).collect();
            status.push((
                format!("{}: {}", safe(wave), items.join(" ")),
                anstyle::Style::new(),
            ));
        }
        status.extend(s.status.as_ref().map(|info| {
            let (age, step) = (ago(info.age_secs), safe(&info.status.step));
            match (&info.status.blocked, info.stale) {
                (None, false) => (format!("{age} {step}"), anstyle::Style::new()),
                (Some(why), false) => (
                    format!("blocked {age}: {} (step: {step})", safe(why)),
                    ERROR,
                ),
                (None, true) => (format!("stale {age}: {step}"), DIM),
                (Some(why), true) => (
                    format!("stale {age}: blocked: {} (step: {step})", safe(why)),
                    DIM,
                ),
            }
        }));
        if !status.is_empty() {
            lines.push(Line::new(more, status));
        }
        lines
    }
}

/// The [`text::presence`] of a person or a session: `online` in green,
/// else a dim `offline`.
fn presence(live: bool, secs: Option<u64>) -> (String, anstyle::Style) {
    let style = if live { GOOD } else { DIM };
    (text::presence(live, secs), style)
}

/// The line of a person: the USER, the role tag, and the presence.
fn person_line(user: &str, person: &PersonRow) -> Line {
    let role = person.role.tag().unwrap_or_default().to_owned();
    Line::new(
        "",
        vec![
            (safe(user), crate::style::person(user)),
            (role, MUTED),
            presence(person.live, person.seen_secs),
        ],
    )
}

/// The branch of a tree row: the last one closes the tree.
fn branch(last: bool) -> &'static str {
    if last { "└─ " } else { "├─ " }
}

/// True when the current status is blocked. A stale block is not.
fn blocked(s: &SessionInfo) -> bool {
    s.status
        .as_ref()
        .is_some_and(|i| i.status.blocked.is_some() && !i.stale)
}

/// The short session ID of `riff who`.
fn short(s: &SessionInfo) -> String {
    let id = s.uri.who().session().unwrap_or_default();
    id.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
