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
//! A session gets the tag `worker` when a worker pane of this machine
//! holds it, or when the status of a workers host names it
//! ([`crate::host::HostStatus`]).
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

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;
use std::time::Duration;

use riff_core::build::Build;
use riff_core::wire::{RiffOwner, RiffState, SessionInfo};
use serde::Deserialize;

use crate::style::{DIM, ERROR, MUTED, session as session_style, styled};
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

/// The session IDs of the workers: the worker panes of this machine,
/// and each worker that the status of a workers host names by its
/// short ID.
pub fn workers(panes: &[crate::terminal::WorkerPane], sessions: &[SessionInfo]) -> HashSet<String> {
    let mut ids: HashSet<String> = panes.iter().map(|p| p.session.clone()).collect();
    let users: HashSet<&str> = sessions.iter().map(|s| s.uri.who().user()).collect();
    for user in users {
        for (_, status) in crate::host::hosts(sessions, user) {
            ids.extend(
                crate::host::panes(&status, sessions)
                    .into_iter()
                    .map(|p| p.session),
            );
        }
    }
    ids
}

/// What `riff top` shows.
pub struct Top<'a> {
    pub state: RiffState,
    pub owner: &'a RiffOwner,
    pub server: Option<&'a Build>,
    pub sessions: &'a [SessionInfo],
    pub issues: Option<&'a Issues>,
    pub workers: &'a HashSet<String>,
}

/// One cell: its plain text, and its style.
struct Cell(String, anstyle::Style);

impl Top<'_> {
    /// The header and one row for each session: who, the tags, the
    /// idle time, each claim with the title of its issue, and the
    /// status with its age. Blocked sessions come first, then by user,
    /// host and session ID. It has ANSI styles: print it through
    /// `anstream`.
    ///
    /// ```
    /// use std::collections::HashSet;
    /// use riff::top::{Issues, Top};
    /// use riff_core::wire::{RiffOwner, RiffState, SessionInfo, Status, StatusInfo};
    ///
    /// let info = |uri: &str, step: &str, blocked: Option<&str>| SessionInfo {
    ///     uri: uri.parse().unwrap(),
    ///     live: true,
    ///     idle_secs: 0,
    ///     status: Some(StatusInfo {
    ///         status: Status { step: step.into(), blocked: blocked.map(Into::into) },
    ///         age_secs: 120,
    ///     }),
    /// };
    /// let sessions = [
    ///     info("riff://mike@thelio/o/r?session=aaaa1111&lead=true", "lead", None),
    ///     info("riff://mike@thelio/o/r?session=bbbb2222&claim=issue-12", "tests", None),
    ///     info("riff://ann@heron/o/r?session=cccc3333", "merge", Some("waits")),
    /// ];
    /// let issues = Issues::parse(
    ///     r#"[{"number": 12, "title": "Show the wave", "milestone": {"title": "Wave 3"}}]"#,
    /// );
    /// let workers = HashSet::from(["bbbb2222".to_string()]);
    /// let owner = RiffOwner::Owner { user: "mike".into(), email: "m@x.io".into() };
    /// let top = Top {
    ///     state: RiffState::Running,
    ///     owner: &owner,
    ///     server: None,
    ///     sessions: &sessions,
    ///     issues: issues.as_ref(),
    ///     workers: &workers,
    /// };
    /// let text = anstream::adapter::strip_str(&top.view()).to_string();
    /// let rows: Vec<&str> = text.lines().skip_while(|l| !l.starts_with("SESSION")).collect();
    /// assert!(text.contains("Wave 3: #12 bbbb2222\n"), "{text}");
    /// assert!(rows[1].starts_with("ann@heron (cccc3333)"), "{text}");
    /// assert!(rows[1].contains("blocked 2m: waits (step: merge)"), "{text}");
    /// assert!(rows[2].contains("owner lead"), "{text}");
    /// assert!(rows[3].contains("owner worker"), "{text}");
    /// assert!(rows[3].contains("issue-12 Show the wave"), "{text}");
    /// assert!(rows[3].contains("2m tests"), "{text}");
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
        let mut sessions: Vec<&SessionInfo> = self.sessions.iter().collect();
        sessions.sort_by_key(|s| {
            (
                !blocked(s),
                s.uri.who().user().to_owned(),
                s.uri.place().host().to_owned(),
                s.uri.who().session().unwrap_or_default().to_owned(),
            )
        });
        let head = ["SESSION", "TAGS", "IDLE", "ITEM", "STATUS"]
            .map(|h| Cell(h.into(), anstyle::Style::new().bold()));
        let mut rows = vec![head];
        rows.extend(sessions.into_iter().map(|s| self.row(s)));
        let columns = rows[0].len();
        let widths: Vec<usize> = (0..columns)
            .map(|c| {
                rows.iter()
                    .map(|r| r[c].0.chars().count())
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        for row in &rows {
            let mut line = String::new();
            for (c, Cell(text, style)) in row.iter().enumerate() {
                let pad = if c + 1 == columns {
                    0
                } else {
                    widths[c] - text.chars().count() + 2
                };
                let _ = write!(line, "{}{}", styled(*style, text), " ".repeat(pad));
            }
            let _ = writeln!(out, "{}", line.trim_end());
        }
        out
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

    fn row(&self, s: &SessionInfo) -> [Cell; 5] {
        let mut who = format!("{}@{}", s.uri.who().user(), s.uri.place().host());
        if s.uri.who().session().is_some() {
            let _ = write!(who, " ({})", short(s));
        }
        let mut tags = Vec::new();
        if self.owner.is(s.uri.who().user()) {
            tags.push("owner");
        }
        if s.uri.lead() {
            tags.push("lead");
        }
        if s.uri
            .who()
            .session()
            .is_some_and(|id| self.workers.contains(id))
        {
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
        let status = match &s.status {
            None => Cell("-".into(), DIM),
            Some(info) => {
                let (age, step) = (ago(info.age_secs), safe(&info.status.step));
                match &info.status.blocked {
                    None => Cell(format!("{age} {step}"), anstyle::Style::new()),
                    Some(why) => Cell(
                        format!("blocked {age}: {} (step: {step})", safe(why)),
                        ERROR,
                    ),
                }
            }
        };
        [
            Cell(safe(&who), session_style(&s.uri)),
            Cell(tags.join(" "), MUTED),
            Cell(idle, DIM),
            Cell(
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

fn blocked(s: &SessionInfo) -> bool {
    s.status
        .as_ref()
        .is_some_and(|i| i.status.blocked.is_some())
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
