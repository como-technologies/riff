//! The state of a session and its detail, as people see them
//! (01M3QB6CJ1XCQG5B1BVR8AF3B4).
//!
//! The server derives the state ([`SessionState`]) from facts and sends
//! it in the reply of `who`. riff only shows it: one word, then the
//! detail of the state from the other facts of the session. The words
//! of the session (its step) come last, with their age: they help a
//! person, and make no state.
//!
//! | State | Color | Detail |
//! |---|---|---|
//! | `offline` | grey | `seen 2h ago` |
//! | `paused` | yellow | the claims, and the step it stopped at |
//! | `blocked` | red | the reason with its age, `the lead gave no answer` when it gave none (01M41FZQCHWY1YVGAZ60ZHJK21), then the claims |
//! | `must clear` | yellow | `must clear its context before its next claim` (01M3X9XC99KY4RQY36A7CYWY11) |
//! | `waiting` | cyan | the claims, then what they wait for: `waits for a verify of PR #418` (01M41FZP9A50CH4A2VX344DW49) |
//! | `busy` | green | `working on #N`, or `reviewing #N` for a verify claim, then the work, then the step |
//! | `idle` | dim | `ready for work` with the time, then a current step. The lead: `monitoring work` |
//!
//! The work is the newest fact of the hooks (01M41FZNTPXQNCZ1S99HE42PYQ):
//! `runs Bash: run just ci for 12m`, `works for 2m`, or
//! `turn ended 5m ago`.
//!
//! A worker that is not offline also shows the time since its last
//! fresh start, when the log has one: `fresh start 12m ago`
//! (01M3X9XC99KY4RQY36A7CYWY11). A fresh start is a new agent process or a clear of the
//! context.
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
//!     P -- no --> B{a block that holds?}
//!     B -- yes --> Bl[blocked]
//!     B -- no --> M{worker that must clear?}
//!     M -- yes --> Mc[must clear]
//!     M -- no --> W{each claim waits?}
//!     W -- yes --> Wa[waiting]
//!     W -- no --> C{holds a claim?}
//!     C -- yes --> Bu[busy]
//!     C -- no --> I[idle]
//! ```

use riff_core::wire::{Activity, RiffState, SessionInfo, SessionState, StatusInfo};

use crate::style::{DIM, ERROR, GOOD, MUTED, WAITING, WARNING};
use crate::text::{ago, safe};

/// The color of a state: `blocked` red, `waiting` cyan, `busy` green,
/// `idle` dim, `offline` grey, `paused` and `must clear` yellow.
pub fn style(state: SessionState) -> anstyle::Style {
    match state {
        SessionState::Offline => MUTED,
        SessionState::Paused => WARNING,
        SessionState::Blocked => ERROR,
        SessionState::MustClear => WARNING,
        SessionState::Waiting => WAITING,
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

/// The work of a session: the newest fact of its hooks
/// (01M41FZNTPXQNCZ1S99HE42PYQ).
///
/// ```
/// use riff::state::work;
/// use riff_core::wire::Activity;
///
/// let tool = Activity { tool: Some("Bash: run just ci".into()), turn: true, secs: 720 };
/// assert_eq!(work(&tool), "runs Bash: run just ci for 12m");
/// let between = Activity { tool: None, turn: true, secs: 5 };
/// assert_eq!(work(&between), "works, 5s ago");
/// let ended = Activity { tool: None, turn: false, secs: 300 };
/// assert_eq!(work(&ended), "turn ended 5m ago");
/// ```
pub fn work(activity: &Activity) -> String {
    let age = ago(activity.secs);
    match (&activity.tool, activity.turn) {
        (Some(tool), _) => format!("runs {} for {age}", safe(tool)),
        (None, true) => format!("works, {age} ago"),
        (None, false) => format!("turn ended {age} ago"),
    }
}

/// The detail of the state of `s`: one line for each fact, each with
/// its style. `title` gives the title of an issue, when riff knows it.
/// An idle lead takes no claims, so it shows `monitoring work`, not
/// `ready for work`.
///
/// ```
/// use riff::state::detail;
/// use riff_core::wire::{Activity, BlockedInfo, SessionInfo, SessionState, Status, StatusInfo, Waits};
///
/// let plain = |s: &SessionInfo| -> Vec<String> {
///     detail(s, &|n| (n == 12).then(|| "Show the wave".to_owned()))
///         .into_iter()
///         .map(|(line, _)| line)
///         .collect()
/// };
/// let step = |step: &str, stale| StatusInfo {
///     status: Status { step: step.into() },
///     age_secs: 120,
///     stale,
/// };
/// let mut s = SessionInfo {
///     uri: "riff://mike@thelio/o/r?session=w1&claim=issue-12&claim=verify-issue-9".parse()?,
///     live: true,
///     idle_secs: 7200,
///     status: Some(step("tests", false)),
///     worker: true,
///     stopping: false,
///     claims_secs: 300,
///     must_clear: false,
///     fresh_secs: None,
///     state: Some(SessionState::Busy),
///     work: Some(Activity { tool: Some("Bash: run just ci".into()), turn: true, secs: 720 }),
///     waits: None,
///     blocked: None,
/// };
/// assert_eq!(
///     plain(&s),
///     ["working on #12 Show the wave", "reviewing #9", "runs Bash: run just ci for 12m", "2m ago: tests"]
/// );
/// s.state = Some(SessionState::Paused);
/// assert_eq!(plain(&s), ["working on #12 Show the wave", "reviewing #9", "stopped at: tests"]);
///
/// // A wait from facts: no status call.
/// s.waits = Some(Waits::Verify { pull: 418 });
/// s.state = Some(SessionState::Waiting);
/// assert_eq!(
///     plain(&s),
///     ["working on #12 Show the wave", "reviewing #9", "waits for a verify of PR #418", "2m ago: tests"]
/// );
///
/// // A block, and a block with no answer of the lead.
/// let mut block = BlockedInfo {
///     reason: "which design?".into(),
///     secs: 1500,
///     answered: false,
///     woken_again: true,
///     unanswered: false,
/// };
/// s.blocked = Some(block.clone());
/// s.state = Some(SessionState::Blocked);
/// assert_eq!(
///     plain(&s),
///     ["which design? (25m ago)", "working on #12 Show the wave", "reviewing #9"]
/// );
/// block.unanswered = true;
/// s.blocked = Some(block);
/// assert_eq!(
///     plain(&s),
///     ["which design? (25m ago)", "the lead gave no answer", "working on #12 Show the wave", "reviewing #9"]
/// );
///
/// s.uri = "riff://mike@thelio/o/r?session=w1".parse()?;
/// (s.blocked, s.waits, s.work) = (None, None, None);
/// s.status = Some(step("tests", true));
/// s.state = Some(SessionState::Idle);
/// assert_eq!(plain(&s), ["ready for work for 5m"]);
/// s.status = Some(step("plan the wave", false));
/// assert_eq!(plain(&s), ["ready for work for 5m", "2m ago: plan the wave"]);
/// s.uri = "riff://mike@thelio/o/r?session=w1&lead=true".parse()?;
/// assert_eq!(plain(&s), ["monitoring work for 5m", "2m ago: plan the wave"]);
/// s.state = Some(SessionState::Offline);
/// assert_eq!(plain(&s), ["seen 2h ago"]);
///
/// // A worker that released its last claim, 12 minutes after its last
/// // fresh start.
/// s.uri = "riff://mike@thelio/o/r?session=w1".parse()?;
/// (s.must_clear, s.fresh_secs) = (true, Some(720));
/// s.state = Some(SessionState::MustClear);
/// assert_eq!(
///     plain(&s),
///     ["must clear its context before its next claim", "fresh start 12m ago"]
/// );
/// s.state = Some(SessionState::Idle);
/// assert_eq!(
///     plain(&s),
///     ["ready for work for 5m", "2m ago: plan the wave", "fresh start 12m ago"]
/// );
/// s.state = Some(SessionState::Offline);
/// assert_eq!(plain(&s), ["seen 2h ago"]);
/// s.state = Some(SessionState::Idle);
/// s.worker = false;
/// assert_eq!(plain(&s), ["ready for work for 5m", "2m ago: plan the wave"]);
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
            if let Some(block) = &s.blocked {
                let reason = format!("{} ({} ago)", safe(&block.reason), ago(block.secs));
                lines.push((reason, ERROR));
                if block.unanswered {
                    lines.push(("the lead gave no answer".to_owned(), ERROR));
                }
            }
            lines.extend(claims());
        }
        SessionState::MustClear => {
            let what = "must clear its context before its next claim";
            lines.push((what.to_owned(), WARNING));
        }
        SessionState::Waiting => {
            lines.extend(claims());
            lines.extend(s.waits.as_ref().map(|w| (w.to_string(), WAITING)));
            lines.extend(s.status.as_ref().map(step));
        }
        SessionState::Busy => {
            lines.extend(claims());
            lines.extend(s.work.as_ref().map(|w| (work(w), plain)));
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
    if let (true, Some(secs)) = (s.worker && of(s) != SessionState::Offline, s.fresh_secs) {
        lines.push((format!("fresh start {} ago", ago(secs)), DIM));
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
