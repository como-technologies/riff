//! The Claude Code hooks.
//!
//! # Design
//!
//! A hook cannot call a tool. So the start hook cannot start the watch
//! itself. It adds context instead, and the session starts the watch
//! with the Monitor tool (R66).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant C as Claude Code
//!     participant H as riff hook session-start
//!     participant S as session
//!     participant W as riff watch
//!     C->>H: stdin: session_id, source
//!     H-->>C: stdout: additionalContext
//!     C->>S: context
//!     S->>W: Monitor: riff watch
//!     W-->>S: one line for each wake
//!     Note over S,W: The Monitor ends after 30 minutes.<br/>The session starts it again (R67).
//! ```
//!
//! The context depends on the `source` of the start (R68):
//!
//! | Source | The session |
//! |---|---|
//! | `startup` | Starts the watch. Follows the start routine (R54). |
//! | `resume` | Starts the watch. The old watch stopped with the old process. |
//! | `clear` | Has a new session ID. Stops the watch of the old ID, starts a new one, and follows the start routine. |
//! | `compact` | Keeps the watch that runs. Starts one only if none runs. |
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
//! let context = riff::hook::start_context(None, input.source);
//! assert!(context.contains("riff watch"));
//! assert!(context.contains("TaskStop"));
//! # Ok::<(), serde_json::Error>(())
//! ```

use std::fmt::Write;

use riff_core::name::SessionUri;
use serde::Deserialize;

use crate::text::DATA_NOTE;

/// The Monitor timeout for the watch: the maximum that Claude Code allows.
pub const MONITOR_TIMEOUT_MS: u32 = 1_800_000;

/// Why the session started, as Claude Code gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// A resumed session. It keeps its session ID.
    Resume,
    /// `/clear`. The session has a new session ID.
    Clear,
    /// The context was compacted. The Monitor tasks still run.
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
/// hook found it.
pub fn start_context(uri: Option<&SessionUri>, source: Source) -> String {
    let mut out = String::from("riff: ");
    match uri {
        Some(uri) => writeln!(out, "this session is {uri}.").unwrap(),
        None => out.push_str("riff could not find this session. Call the riff whoami tool.\n"),
    }
    let start = format!(
        "run `riff watch` with the Monitor tool, with timeout_ms {MONITOR_TIMEOUT_MS} and the \
         description \"riff wakes\"."
    );
    match source {
        Source::Compact => writeln!(
            out,
            "- Keep the Monitor task that runs `riff watch`. If none runs, {start}"
        ),
        Source::Clear => writeln!(
            out,
            "- /clear gave this session a new session ID. The old `riff watch` Monitor task \
             still wakes you for the old ID. Stop it with TaskStop.\n- Now {start}"
        ),
        Source::Startup | Source::Resume => writeln!(out, "- Now {start}"),
    }
    .unwrap();
    out.push_str(
        "- Each line of the watch is a wake. Call the riff read tool with no thread.\n\
         - When the Monitor ends, start it again.\n",
    );
    if matches!(source, Source::Startup | Source::Clear) {
        out.push_str("- To find work, follow the start routine of the riff skill.\n");
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

    #[test]
    fn each_source_starts_or_keeps_the_watch() {
        for source in [
            Source::Startup,
            Source::Resume,
            Source::Clear,
            Source::Compact,
        ] {
            let context = start_context(Some(&uri()), source);
            assert!(
                context.contains("`riff watch` with the Monitor tool"),
                "{context}"
            );
            assert!(context.contains("timeout_ms 1800000"));
            assert!(context.contains("start it again"));
            assert!(context.contains(DATA_NOTE));
            assert!(context.contains("session=a6cf"));
        }
    }

    #[test]
    fn only_clear_stops_the_old_watch() {
        assert!(start_context(None, Source::Clear).contains("TaskStop"));
        assert!(!start_context(None, Source::Startup).contains("TaskStop"));
    }

    #[test]
    fn compact_keeps_the_watch() {
        let context = start_context(None, Source::Compact);
        assert!(context.contains("Keep the Monitor task"));
        assert!(context.contains("If none runs, run `riff watch`"));
    }

    #[test]
    fn new_sessions_get_the_start_routine() {
        assert!(start_context(None, Source::Startup).contains("start routine"));
        assert!(start_context(None, Source::Clear).contains("start routine"));
        assert!(!start_context(None, Source::Resume).contains("start routine"));
        assert!(!start_context(None, Source::Compact).contains("start routine"));
    }

    #[test]
    fn no_uri_asks_for_whoami() {
        assert!(start_context(None, Source::Startup).contains("whoami"));
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
