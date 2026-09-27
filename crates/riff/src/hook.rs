//! The Claude Code hooks.
//!
//! # Design
//!
//! A hook cannot call a tool. So the start hook cannot start the watch
//! itself. It adds context instead, and the session starts
//! `riff watch --once` as a background task of the Bash tool (R66).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant C as Claude Code
//!     participant H as riff hook session-start
//!     participant S as session
//!     participant W as riff watch --once
//!     C->>H: stdin: session_id, source
//!     H-->>C: stdout: additionalContext
//!     C->>S: context
//!     S->>W: Bash, run_in_background
//!     W-->>S: one wake, then exit
//!     S->>S: read
//!     S->>W: Bash, run_in_background (R171)
//! ```
//!
//! # Wake sources
//!
//! Tested in Claude Code on 2026-09-27:
//!
//! | Source | Ends | The session gets |
//! |---|---|---|
//! | Monitor tool | after 30 minutes at most | one notice for each line, and a notice at the end |
//! | Bash with `run_in_background` | when the command exits; a test task ran 20 minutes and more, past the 10-minute limit of a foreground call | one notice when the command exits |
//!
//! A notice wakes an idle session. In a turn, it comes with the result
//! of the next tool call. A Monitor task expires each 30 minutes, and a
//! busy session often starts it again only at the end of its turn. So
//! riff uses a background Bash task, which ends only on a wake. The
//! session reads, then starts the watch again at once. No message is
//! lost while no watch runs: a new watch wakes the session once if an
//! addressed message is unread (R49). So the session reads before it
//! starts the watch again.
//!
//! The context depends on the `source` of the start (R68):
//!
//! | Source | The session |
//! |---|---|
//! | `startup` | Follows the start routine (R54, R166). |
//! | `resume` | Continues. |
//! | `clear` | Keeps its riff session ID and its claims (R168). Follows the start routine. |
//! | `compact` | Continues. |
//!
//! The watch does not depend on the source. A watch that runs holds a
//! lock (R169, see [`crate::local`]). When the lock is held, the
//! context tells the session to keep that watch. Else it tells the
//! session to start one. So after `/clear`, the session keeps the watch
//! from before `/clear`: it runs for the same ID.
//!
//! The hook never stops a session start (R69). When it cannot find the
//! session, the context has no URI, and the hook still exits with
//! status 0.
//!
//! ```
//! use riff::hook::{Source, StartInput};
//!
//! let input: StartInput = serde_json::from_str(r#"{"session_id":"a6cf","source":"clear"}"#)?;
//! assert_eq!(input.source, Source::Clear);
//! let context = riff::hook::start_context(None, input.source, true);
//! assert!(context.contains("claims stay"));
//! assert!(context.contains("Keep it."));
//! assert!(!context.contains("TaskStop"));
//! # Ok::<(), serde_json::Error>(())
//! ```

use std::fmt::Write;

use riff_core::name::SessionUri;
use serde::Deserialize;

use crate::text::DATA_NOTE;

/// Why the session started, as Claude Code gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// A resumed session. It keeps its session ID.
    Resume,
    /// `/clear`. Claude Code gives the session a new session ID, but the
    /// riff session keeps its ID (R168).
    Clear,
    /// The context was compacted. The background tasks still run.
    Compact,
    /// A new session. An unknown source counts as a new session.
    #[default]
    #[serde(other)]
    Startup,
}

/// The part of the SessionStart hook input that riff uses.
#[derive(Debug, Default, Deserialize)]
pub struct StartInput {
    /// The session ID.
    pub session_id: Option<String>,
    /// Why the session started.
    #[serde(default)]
    pub source: Source,
}

/// The context that the start hook adds. `uri` is the session, when the
/// hook found it. `watching` is true when a watch runs for the session.
pub fn start_context(uri: Option<&SessionUri>, source: Source, watching: bool) -> String {
    let mut out = String::from("riff: ");
    match uri {
        Some(uri) => writeln!(out, "this session is {uri}.").unwrap(),
        None => out.push_str("riff could not find this session. Call the riff whoami tool.\n"),
    }
    let start = "run `riff watch --once` with the Bash tool, with run_in_background true and \
                 the description \"riff wakes\".";
    if source == Source::Clear {
        out.push_str(
            "- /clear did not change your riff session. Its session ID and its claims stay. \
             The riff whoami tool shows them.\n",
        );
    }
    if watching {
        out.push_str("- A task runs `riff watch` for this session. Keep it.\n");
    } else {
        writeln!(out, "- Now {start}").unwrap();
    }
    out.push_str(
        "- When the task ends, call the riff read tool with no thread. Then start the watch \
         again at once, also in the middle of a turn. When the watch says \"Do not start the \
         watch again now\", do not start it.\n",
    );
    if matches!(source, Source::Startup | Source::Clear) {
        out.push_str(
            "- To find work, follow the start routine of the riff skill. Pick a free \
             item yourself. Do not wait for a plan or for permission. A scope from \
             your user wins.\n",
        );
    }
    writeln!(out, "- {DATA_NOTE}").unwrap();
    out
}

/// The hook output that gives `context` to Claude Code.
///
/// ```
/// let out: serde_json::Value = serde_json::from_str(&riff::hook::start_output("hi"))?;
/// assert_eq!(out["hookSpecificOutput"]["hookEventName"], "SessionStart");
/// assert_eq!(out["hookSpecificOutput"]["additionalContext"], "hi");
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn start_output(context: &str) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": context,
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri() -> SessionUri {
        "riff://mike@pangolin/como-technologies/riff?session=a6cf"
            .parse()
            .unwrap()
    }

    const SOURCES: [Source; 4] = [
        Source::Startup,
        Source::Resume,
        Source::Clear,
        Source::Compact,
    ];

    #[test]
    fn each_source_starts_the_watch_when_none_runs() {
        for source in SOURCES {
            let context = start_context(Some(&uri()), source, false);
            assert!(
                context.contains("Now run `riff watch --once` with the Bash tool"),
                "{context}"
            );
            assert!(context.contains("run_in_background true"));
            assert!(context.contains("start the watch again at once"));
            assert!(context.contains("middle of a turn"));
            assert!(context.contains(DATA_NOTE));
            assert!(context.contains("session=a6cf"));
        }
    }

    #[test]
    fn each_source_keeps_a_watch_that_runs() {
        for source in SOURCES {
            let context = start_context(Some(&uri()), source, true);
            assert!(context.contains("Keep it."), "{context}");
            assert!(!context.contains("Now run"), "{context}");
            assert!(context.contains("start the watch again at once"));
        }
    }

    #[test]
    fn no_source_stops_a_watch() {
        for source in SOURCES {
            for watching in [false, true] {
                assert!(!start_context(None, source, watching).contains("TaskStop"));
            }
        }
    }

    #[test]
    fn clear_keeps_the_session_and_its_claims() {
        let context = start_context(Some(&uri()), Source::Clear, true);
        assert!(context.contains("did not change your riff session"));
        assert!(context.contains("claims stay"));
        assert!(context.contains("whoami"));
        for source in [Source::Startup, Source::Resume, Source::Compact] {
            assert!(!start_context(None, source, true).contains("claims stay"));
        }
    }

    #[test]
    fn new_sessions_get_the_start_routine() {
        assert!(start_context(None, Source::Startup, false).contains("start routine"));
        assert!(start_context(None, Source::Startup, false).contains("Pick a free item yourself"));
        assert!(start_context(None, Source::Clear, false).contains("start routine"));
        assert!(!start_context(None, Source::Resume, false).contains("start routine"));
        assert!(!start_context(None, Source::Compact, false).contains("start routine"));
    }

    #[test]
    fn no_uri_asks_for_whoami() {
        assert!(start_context(None, Source::Startup, false).contains("whoami"));
    }

    #[test]
    fn an_unknown_source_is_a_startup() {
        let input: StartInput = serde_json::from_str(r#"{"source":"later"}"#).unwrap();
        assert_eq!(input.source, Source::Startup);
        let input: StartInput = serde_json::from_str("{}").unwrap();
        assert_eq!(input.source, Source::Startup);
        assert_eq!(input.session_id, None);
    }
}
