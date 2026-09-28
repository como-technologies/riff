//! A fresh context for a worker after each item.
//!
//! # Design
//!
//! A worker takes many items in a row. Its context keeps the history of
//! each item, which costs tokens and mixes old facts with the new item.
//! The agent does not clear its own context: riff does it, from the
//! terminal of the worker (01M3JQCCZ5M9VY3RGXWJYJN9Q9).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant W as worker (claude in its tmux pane)
//!     participant N as riff workers next
//!     participant F as next-ID file
//!     participant H as riff hook stop
//!     participant T as tmux
//!     W->>N: item merged, released, worktree removed
//!     N->>F: write the pane
//!     N-->>W: end your turn now
//!     W->>H: the turn ends: Stop hook
//!     H->>F: take the pane
//!     H->>T: later: /clear, then "Join the riff."
//!     T->>W: /clear: the start hook gives the start routine
//!     T->>W: "Join the riff.": the worker claims its next item
//! ```
//!
//! - `riff workers next` runs only in a worker, never in the lead, and
//!   only when the worker holds no claims (01M3JQCCX22R4R4MN7XZPTS391).
//!   It writes the file `next-ID` in the local directory
//!   ([`crate::local`]), with the tmux pane of the worker.
//! - The Stop hook runs when the turn ends, so the worker is idle. It
//!   takes the file and starts a detached process that types the keys
//!   into the pane after [`CLEAR_WAIT`]. The hook itself returns at
//!   once.
//! - A worker has its riff session ID in `RIFF_SESSION`
//!   (01M3JPQT9BA7JVMZPV68FY4MQ6). So after `/clear` it keeps its ID,
//!   its lead and its watch (01M3JQCD16CNWN5FCQBRKHXYMP).
//! - The keys that clear the context and start the next item are
//!   specific to an agent tool. They are in an [`Agent`] adapter, one
//!   for each tool (01M3JQCD373XZWNSSQYBE561TM).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::terminal::quote;

/// How long the detached process waits before it types `/clear`, so
/// that the agent tool is ready for input after its turn.
pub const CLEAR_WAIT: Duration = Duration::from_secs(1);
/// How long it waits after `/clear` before it types the start prompt.
pub const PROMPT_WAIT: Duration = Duration::from_secs(3);

/// The keys of one agent tool (01M3JQCD373XZWNSSQYBE561TM).
pub trait Agent {
    /// The input that clears the context and keeps the session.
    fn clear(&self) -> &str;
    /// The prompt that starts the next item.
    fn start_prompt(&self) -> &str;
}

/// Claude Code: `/clear` keeps the riff session (R168), and the start
/// hook gives the start routine.
pub struct ClaudeCode;

impl Agent for ClaudeCode {
    fn clear(&self) -> &str {
        "/clear"
    }

    fn start_prompt(&self) -> &str {
        crate::terminal::JOIN
    }
}

/// The part of the Stop hook input that riff uses.
#[derive(Debug, Default, Deserialize)]
pub struct StopInput {
    /// The session ID of the agent tool.
    pub session_id: Option<String>,
}

/// The file that asks for a fresh context for `session`.
///
/// ```
/// let path = riff::next::mark_path("/run/riff".as_ref(), "w1");
/// assert_eq!(path, std::path::Path::new("/run/riff/next-w1"));
/// ```
pub fn mark_path(dir: &Path, session: &str) -> PathBuf {
    dir.join(format!("next-{session}"))
}

/// Asks for a fresh context for `session`, in `pane`, after the turn.
pub fn mark(dir: &Path, session: &str, pane: &str) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let path = mark_path(dir, session);
    std::fs::write(&path, pane).with_context(|| format!("cannot write {}", path.display()))
}

/// Takes the request of `session`: its pane, and the file is gone.
/// `None` when there is no request.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// assert_eq!(riff::next::take(dir.path(), "w1"), None);
/// riff::next::mark(dir.path(), "w1", "%3")?;
/// assert_eq!(riff::next::take(dir.path(), "w1").as_deref(), Some("%3"));
/// assert_eq!(riff::next::take(dir.path(), "w1"), None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn take(dir: &Path, session: &str) -> Option<String> {
    let path = mark_path(dir, session);
    let pane = std::fs::read_to_string(&path).ok()?;
    std::fs::remove_file(&path).ok()?;
    let pane = pane.trim().to_owned();
    (!pane.is_empty()).then_some(pane)
}

/// The shell script that types the keys of `agent` into `pane` with
/// `tmux`: it waits, clears the context, waits, and types the start
/// prompt.
///
/// ```
/// use riff::next::{ClaudeCode, script};
/// assert_eq!(
///     script(&ClaudeCode, "%3"),
///     "sleep 1; tmux send-keys -t '%3' -l '/clear'; tmux send-keys -t '%3' Enter; \
///      sleep 3; tmux send-keys -t '%3' -l 'Join the riff.'; tmux send-keys -t '%3' Enter"
/// );
/// ```
pub fn script(agent: &dyn Agent, pane: &str) -> String {
    let pane = quote(pane);
    let keys = |text: &str| {
        format!(
            "tmux send-keys -t {pane} -l {}; tmux send-keys -t {pane} Enter",
            quote(text)
        )
    };
    format!(
        "sleep {}; {}; sleep {}; {}",
        CLEAR_WAIT.as_secs(),
        keys(agent.clear()),
        PROMPT_WAIT.as_secs(),
        keys(agent.start_prompt())
    )
}

/// Starts [`script`] as a detached process in its own process group, so
/// that it outlives the hook.
pub fn spawn(agent: &dyn Agent, pane: &str) -> Result<()> {
    use std::os::unix::process::CommandExt;
    Command::new("sh")
        .arg("-c")
        .arg(script(agent, pane))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .context("cannot start sh")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pane_with_a_quote_stays_one_word() {
        let s = script(&ClaudeCode, "%3'x");
        assert!(s.contains(r"-t '%3'\''x'"), "{s}");
    }

    #[test]
    fn an_empty_mark_is_no_request() {
        let dir = tempfile::tempdir().unwrap();
        mark(dir.path(), "w1", "  ").unwrap();
        assert_eq!(take(dir.path(), "w1"), None);
        assert!(!mark_path(dir.path(), "w1").exists());
    }

    #[test]
    fn the_stop_input_reads_the_session() {
        let input: StopInput =
            serde_json::from_str(r#"{"session_id":"a6cf","hook_event_name":"Stop"}"#).unwrap();
        assert_eq!(input.session_id.as_deref(), Some("a6cf"));
    }
}
