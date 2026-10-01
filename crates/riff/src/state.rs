//! The state of a session and its detail, as people see them
//! (01M3QB6CJ1XCQG5B1BVR8AF3B4).
//!
//! The server derives the state ([`SessionState`]) and sends it in the
//! reply of `who`. riff only shows it: one word, then the detail of the
//! state from the other facts of the session.
//!
//! | State | Color | Detail |
//! |---|---|---|
//! | `offline` | grey | `seen 2h ago` |
//! | `paused` | yellow | the claims, and the step it stopped at |
//! | `blocked` | red | the reason, then the claims |
//! | `busy` | green | `working on #N`, or `reviewing #N` for a verify claim, then the step |
//! | `idle` | dim | `ready for work` with the time, then a current step. The lead: `monitoring work` |
//!
//! `riff top`, `riff who`, the MCP `who` tool and `riff workers` show
//! the same words.
//!
//! ```mermaid
//! flowchart TD
//!     S[session in who] --> L{open watch?}
//!     L -- no --> Off[offline]
//!     L -- yes --> P{riff paused?}
//!     P -- yes --> Pa[paused]
//!     P -- no --> B{current status blocked?}
//!     B -- yes --> Bl[blocked]
//!     B -- no --> C{holds a claim?}
//!     C -- yes --> Bu[busy]
//!     C -- no --> I[idle]
//! ```

use riff_core::wire::{RiffState, SessionInfo, SessionState, StatusInfo};

use crate::style::{DIM, ERROR, GOOD, MUTED, WARNING};
use crate::text::{ago, safe};

/// The color of a state: `blocked` red, `busy` green, `idle` dim,
/// `offline` grey, `paused` yellow.
pub fn style(state: SessionState) -> anstyle::Style {
    match state {
        SessionState::Offline => MUTED,
        SessionState::Paused => WARNING,
        SessionState::Blocked => ERROR,
        SessionState::Busy => GOOD,
        SessionState::Idle => DIM,
    }
}

/// Fills the state of each session that has none, from its other facts
/// and the state `riff` of the riff: an older server sends no state
/// ([`SessionInfo::fill_state`]).
pub fn fill(sessions: &mut [SessionInfo], riff: RiffState) {
    for s in sessions {
        s.fill_state(riff);
    }
}

/// The state of `s`. riff fills the state before it shows a session
/// ([`SessionInfo::fill_state`]); a session with none is `offline`.
pub fn of(s: &SessionInfo) -> SessionState {
    s.state.unwrap_or_default()
}

/// The status of `s` when it is current: not stale.
fn current(s: &SessionInfo) -> Option<&StatusInfo> {
    s.status.as_ref().filter(|i| !i.stale)
}

/// The detail of the state of `s`: one line for each fact, each with
/// its style. `title` gives the title of an issue, when riff knows it.
/// An idle lead takes no claims, so it shows `monitoring work`, not
/// `ready for work`.
///
/// ```
/// use riff::state::detail;
/// use riff_core::wire::{SessionInfo, SessionState, Status, StatusInfo};
///
/// let plain = |s: &SessionInfo| -> Vec<String> {
///     detail(s, &|n| (n == 12).then(|| "Show the wave".to_owned()))
///         .into_iter()
///         .map(|(line, _)| line)
///         .collect()
/// };
/// let step = |step: &str, blocked: Option<&str>, stale| StatusInfo {
///     status: Status { step: step.into(), blocked: blocked.map(Into::into) },
///     age_secs: 120,
///     stale,
/// };
/// let mut s = SessionInfo {
///     uri: "riff://mike@thelio/o/r?session=w1&claim=issue-12&claim=verify-issue-9".parse()?,
///     live: true,
///     idle_secs: 7200,
///     status: Some(step("tests", None, false)),
///     worker: true,
///     stopping: false,
///     claims_secs: 300,
///     state: Some(SessionState::Busy),
/// };
/// assert_eq!(plain(&s), ["working on #12 Show the wave", "reviewing #9", "2m ago: tests"]);
/// s.state = Some(SessionState::Paused);
/// assert_eq!(plain(&s), ["working on #12 Show the wave", "reviewing #9", "stopped at: tests"]);
/// s.status = Some(step("merge", Some("I need a review"), false));
/// s.state = Some(SessionState::Blocked);
/// assert_eq!(
///     plain(&s),
///     ["I need a review (step: merge, 2m ago)", "working on #12 Show the wave", "reviewing #9"]
/// );
/// s.uri = "riff://mike@thelio/o/r?session=w1".parse()?;
/// s.status = Some(step("tests", None, true));
/// s.state = Some(SessionState::Idle);
/// assert_eq!(plain(&s), ["ready for work for 5m"]);
/// s.status = Some(step("plan the wave", None, false));
/// assert_eq!(plain(&s), ["ready for work for 5m", "2m ago: plan the wave"]);
/// s.uri = "riff://mike@thelio/o/r?session=w1&lead=true".parse()?;
/// assert_eq!(plain(&s), ["monitoring work for 5m", "2m ago: plan the wave"]);
/// s.state = Some(SessionState::Offline);
/// assert_eq!(plain(&s), ["seen 2h ago"]);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn detail(
    s: &SessionInfo,
    title: &dyn Fn(u64) -> Option<String>,
) -> Vec<(String, anstyle::Style)> {
    let plain = anstyle::Style::new();
    let claims = || {
        s.uri
            .claims()
            .iter()
            .map(|c| (claim(c, title), plain))
            .collect::<Vec<_>>()
    };
    let step = |info: &StatusInfo| {
        let (age, step) = (ago(info.age_secs), safe(&info.status.step));
        if info.stale {
            (format!("stale {age}: {step}"), DIM)
        } else {
            (format!("{age} ago: {step}"), plain)
        }
    };
    let mut lines = Vec::new();
    match of(s) {
        SessionState::Offline => lines.push((format!("seen {} ago", ago(s.idle_secs)), MUTED)),
        SessionState::Paused => {
            lines.extend(claims());
            if let Some(info) = &s.status {
                lines.push((format!("stopped at: {}", safe(&info.status.step)), plain));
            }
        }
        SessionState::Blocked => {
            if let Some(info) = current(s) {
                let reason = info.status.blocked.as_deref().unwrap_or_default();
                lines.push((
                    format!(
                        "{} (step: {}, {} ago)",
                        safe(reason),
                        safe(&info.status.step),
                        ago(info.age_secs)
                    ),
                    ERROR,
                ));
            }
            lines.extend(claims());
        }
        SessionState::Busy => {
            lines.extend(claims());
            lines.extend(s.status.as_ref().map(step));
        }
        SessionState::Idle => {
            let what = if s.uri.lead() {
                "monitoring work"
            } else {
                "ready for work"
            };
            lines.push((format!("{what} for {}", ago(s.claims_secs)), plain));
            lines.extend(current(s).map(step));
        }
    }
    lines
}

/// A claim as what the session does: `working on #N TITLE`, or
/// `reviewing #N TITLE` for a verify claim, or `working on CLAIM` for a
/// claim that is not an issue.
fn claim(c: &str, title: &dyn Fn(u64) -> Option<String>) -> String {
    let verb = if c.starts_with("verify-") {
        "reviewing"
    } else {
        "working on"
    };
    match crate::top::issue_of(c) {
        Some(n) => match title(n) {
            Some(t) => format!("{verb} #{n} {}", safe(&t)),
            None => format!("{verb} #{n}"),
        },
        None => format!("{verb} {}", safe(c)),
    }
}

/// The state of a person: `online` in green when a session of the
/// person is live, else a grey `offline` with the time since the last
/// call, when riff knows it.
///
/// ```
/// use riff::state::person;
///
/// assert_eq!(person(true, Some(40)), vec![("online".to_owned(), riff::style::GOOD)]);
/// let off: Vec<String> = person(false, Some(3600)).into_iter().map(|(t, _)| t).collect();
/// assert_eq!(off, ["offline", "seen 1h ago"]);
/// let away: Vec<String> = person(false, None).into_iter().map(|(t, _)| t).collect();
/// assert_eq!(away, ["offline"]);
/// ```
pub fn person(live: bool, seen_secs: Option<u64>) -> Vec<(String, anstyle::Style)> {
    if live {
        return vec![("online".into(), GOOD)];
    }
    let mut parts = vec![("offline".to_owned(), MUTED)];
    parts.extend(seen_secs.map(|secs| (format!("seen {} ago", ago(secs)), MUTED)));
    parts
}
