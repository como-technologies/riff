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
//! titles and no wave line.
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

use crate::style::{DIM, ERROR, GOOD, MUTED, WARNING, session as session_style, styled};
use crate::text::{self, ago, safe};

/// The time between two draws of `riff top` with no message.
pub const REFRESH: Duration = Duration::from_secs(3);

/// How long `riff top` keeps the issues of `gh`.
pub const ISSUES_TTL: Duration = Duration::from_secs(60);

/// The longest issue title in a row, in characters.
const TITLE_CHARS: usize = 40;

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
}

/// One cell: a plain lead-in, then its parts, each with its style.
struct Cell {
    pre: String,
    parts: Vec<(String, anstyle::Style)>,
}

impl Cell {
    fn new(text: impl Into<String>, style: anstyle::Style) -> Self {
        Cell {
            pre: String::new(),
            parts: vec![(text.into(), style)],
        }
    }

    fn empty() -> Self {
        Cell::new("", anstyle::Style::new())
    }

    fn width(&self) -> usize {
        self.pre.chars().count()
            + self
                .parts
                .iter()
                .map(|(text, _)| text.chars().count())
                .sum::<usize>()
    }

    /// The parts with their styles. An empty part gets no style.
    fn styled(&self) -> String {
        self.parts
            .iter()
            .filter(|(text, _)| !text.is_empty())
            .map(|(text, style)| styled(*style, text))
            .collect()
    }
}

/// One person of the tree: the role, and live or the time since the
/// last call.
struct PersonRow {
    role: PersonRole,
    live: bool,
    seen_secs: Option<u64>,
}

impl Top<'_> {
    /// The header, then a tree for each person (01M3NT4M5D36KTZ5XZMDP6QFQT):
    /// the person, each host of the person, and each session on the host.
    ///
    /// - A person row: the USER in bold color, the role tag `owner` or
    ///   `admin`, and `live` or `last seen` with the time. Each member
    ///   of `who` gets a row, also when away.
    /// - A session row: the short session ID, the role tag `lead` or
    ///   `worker`, `live` or the idle time, each claim with the title of
    ///   its issue, and the status.
    ///
    /// The status starts with the facts that the riff derives
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
    ///     info("riff://mike@thelio/o/r?session=cccc3333", "merge", Some("waits"), false),
    ///     info("riff://mike@pangolin/o/r?session=dddd4444", "docs", None, true),
    /// ];
    /// let people = [
    ///     Person { user: "ann".into(), role: PersonRole::Admin, live: false, seen_secs: Some(3600) },
    ///     Person { user: "mike".into(), role: PersonRole::Owner, live: true, seen_secs: Some(0) },
    /// ];
    /// let issues = Issues::parse(
    ///     r#"[{"number": 12, "title": "Show the wave", "milestone": {"title": "Wave 3"}}]"#,
    /// );
    /// let owner = RiffOwner::Owner { user: "mike".into(), email: "m@x.io".into() };
    /// let top = Top {
    ///     state: RiffState::Running,
    ///     owner: &owner,
    ///     server: None,
    ///     sessions: &sessions,
    ///     people: &people,
    ///     issues: issues.as_ref(),
    /// };
    /// let text = anstream::adapter::strip_str(&top.view()).to_string();
    /// let rows: Vec<&str> = text.lines().skip_while(|l| !l.starts_with("WHO")).skip(1).collect();
    /// assert!(text.contains("Wave 3: #12 bbbb2222\n"), "{text}");
    /// let words = |row: &str| row.split_whitespace().collect::<Vec<_>>().join(" ");
    /// assert_eq!(words(rows[0]), "ann admin last seen 1h", "{text}");
    /// assert_eq!(words(rows[1]), "mike owner live", "{text}");
    /// assert_eq!(rows[2], "├─ pangolin", "{text}");
    /// assert!(rows[3].starts_with("│  └─ dddd4444  worker  live"), "{text}");
    /// assert!(rows[3].ends_with("idle 10m  2m docs"), "{text}");
    /// assert_eq!(rows[4], "└─ thelio", "{text}");
    /// assert!(rows[5].starts_with("   ├─ cccc3333"), "{text}");
    /// assert!(rows[5].contains("blocked 2m: waits (step: merge)"), "{text}");
    /// assert!(rows[6].starts_with("   ├─ aaaa1111  lead "), "{text}");
    /// assert!(rows[6].ends_with("Wave 3: #12  2m lead"), "{text}");
    /// assert!(rows[7].starts_with("   └─ bbbb2222  worker"), "{text}");
    /// assert!(rows[7].contains("issue-12 Show the wave  2m tests"), "{text}");
    /// ```
    pub fn view(&self) -> String {
        let mut out = text::riff_state(self.state);
        if let Some(line) = text::owner_line(self.owner) {
            let _ = write!(out, "\n{line}");
        }
        let _ = writeln!(out, "\n{}", styled(DIM, &text::build_line(self.server)));
        if let Some((wave, items)) = self.issues.and_then(|i| i.wave.as_ref()) {
            let _ = writeln!(out, "{}: {}", safe(wave), self.holders(items));
        }
        let head = ["WHO", "TAGS", "IDLE", "ITEM", "STATUS"]
            .map(|h| Cell::new(h, anstyle::Style::new().bold()));
        let mut rows = vec![head];
        for (user, person) in self.people() {
            rows.push(person_row(&user, &person));
            let hosts = self.hosts(&user);
            for (h, (host, sessions)) in hosts.iter().enumerate() {
                let last_host = h + 1 == hosts.len();
                let mut row = [(); 5].map(|()| Cell::empty());
                row[0] = Cell {
                    pre: branch(last_host).into(),
                    ..Cell::new(safe(host), anstyle::Style::new())
                };
                rows.push(row);
                for (i, s) in sessions.iter().enumerate() {
                    let pre = format!(
                        "{}{}",
                        if last_host { "   " } else { "│  " },
                        branch(i + 1 == sessions.len())
                    );
                    rows.push(self.row(pre, s));
                }
            }
        }
        let columns = rows[0].len();
        let widths: Vec<usize> = (0..columns)
            .map(|c| rows.iter().map(|r| r[c].width()).max().unwrap_or(0))
            .collect();
        for row in &rows {
            let mut line = String::new();
            for (c, cell) in row.iter().enumerate() {
                let pad = if c + 1 == columns {
                    0
                } else {
                    widths[c] - cell.width() + 2
                };
                let _ = write!(line, "{}{}{}", cell.pre, cell.styled(), " ".repeat(pad));
            }
            let _ = writeln!(out, "{}", line.trim_end());
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

    /// Each item of the wave with the short IDs of the sessions that
    /// claim it, or `free`.
    fn holders(&self, items: &[u64]) -> String {
        items
            .iter()
            .map(|n| {
                let holders: Vec<String> = self
                    .sessions
                    .iter()
                    .flat_map(|s| s.uri.claims().iter().map(move |c| (s, c)))
                    .filter(|(_, c)| issue_of(c) == Some(*n))
                    .map(|(s, c)| {
                        let id = short(s);
                        if c.starts_with("verify-") {
                            format!("verify {id}")
                        } else {
                            id
                        }
                    })
                    .collect();
                if holders.is_empty() {
                    format!("#{n} free")
                } else {
                    format!("#{n} {}", holders.join(" "))
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn row(&self, pre: String, s: &SessionInfo) -> [Cell; 5] {
        let mut tags = Vec::new();
        if s.uri.lead() {
            tags.push("lead");
        }
        if s.worker {
            tags.push("worker");
        }
        let idle = if s.live {
            "live".to_owned()
        } else {
            format!("idle {}", ago(s.idle_secs))
        };
        let items: Vec<String> = s
            .uri
            .claims()
            .iter()
            .map(|c| {
                let title = issue_of(c)
                    .and_then(|n| self.issues?.titles.get(&n))
                    .map(|t| format!(" {}", cut(&safe(t), TITLE_CHARS)));
                format!("{}{}", safe(c), title.unwrap_or_default())
            })
            .collect();
        let mut facts: Vec<(String, anstyle::Style)> = Vec::new();
        if self.state == RiffState::Paused {
            facts.push(("paused".into(), WARNING));
        }
        facts.extend(text::idle_worker(s).map(|idle| (idle, anstyle::Style::new())));
        if s.uri.lead()
            && let Some((wave, items)) = self.issues.and_then(|i| i.wave.as_ref())
        {
            let items: Vec<String> = items.iter().map(|n| format!("#{n}")).collect();
            facts.push((
                format!("{}: {}", safe(wave), items.join(" ")),
                anstyle::Style::new(),
            ));
        }
        let step = s.status.as_ref().map(|info| {
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
        });
        let mut parts = Vec::new();
        for (i, part) in facts.into_iter().chain(step).enumerate() {
            if i > 0 {
                parts.push(("  ".to_owned(), anstyle::Style::new()));
            }
            parts.push(part);
        }
        let status = if parts.is_empty() {
            Cell::new("-", DIM)
        } else {
            Cell {
                pre: String::new(),
                parts,
            }
        };
        [
            Cell {
                pre,
                ..Cell::new(short(s), session_style(&s.uri))
            },
            Cell::new(tags.join(" "), MUTED),
            Cell::new(idle, DIM),
            Cell::new(
                if items.is_empty() {
                    "-".into()
                } else {
                    items.join("; ")
                },
                anstyle::Style::new(),
            ),
            status,
        ]
    }
}

/// The row of a person: the USER, the role tag, and `live` or the time
/// since the last call.
fn person_row(user: &str, person: &PersonRow) -> [Cell; 5] {
    let seen = match (person.live, person.seen_secs) {
        (true, _) => Cell::new("live", GOOD),
        (false, Some(secs)) => Cell::new(format!("last seen {}", ago(secs)), DIM),
        (false, None) => Cell::new("away", DIM),
    };
    [
        Cell::new(safe(user), crate::style::person(user)),
        Cell::new(person.role.tag().unwrap_or_default(), MUTED),
        seen,
        Cell::empty(),
        Cell::empty(),
    ]
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

/// `text` cut to `max` characters, with `…` at the end when it is cut.
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_title_is_cut() {
        assert_eq!(cut("abcdef", 4), "abc…");
        assert_eq!(cut("abc", 4), "abc");
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
