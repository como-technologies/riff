//! A fresh context for a worker after each item.
//!
//! # Design
//!
//! A worker takes many items in a row. Its context keeps the history of
//! each item, which costs tokens and mixes old facts with the new item.
//! The agent does not clear its own context, and it does not ask for
//! the clear: riff does it, from the terminal of the worker, when the
//! worker released its last claim (01M3XV0562D3H3P22CJDBPAZBH).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant W as worker (claude in its tmux pane)
//!     participant S as riff-server
//!     participant H as riff hook stop
//!     participant C as riff hook clear
//!     participant T as tmux
//!     W->>S: release (the last claim)
//!     S-->>W: released: the worker is in MustClear
//!     Note over W: the steps that are left, for example the worktree
//!     W->>H: the turn ends: Stop hook
//!     H->>C: start, detached
//!     H-->>W: return at once
//!     C->>S: keep-alive
//!     S-->>C: clear
//!     C->>C: fast-forward the main clone
//!     C->>T: later: /clear, then "Join the riff."
//!     T->>W: /clear: the start hook gives the start routine
//!     W->>S: start (clear): MustClear ends
//!     T->>W: "Join the riff.": the worker claims its next item
//! ```
//!
//! - The server knows that a worker must clear its context: the
//!   `released` record of its last claim says so
//!   (01M3X9XAK1KPZZVM1AJR2H8DSS). The reply to each keep-alive carries
//!   the ask (01M3X9XB37TQCXWPNFZRMRGJB4). So the check needs no file
//!   and no call of the worker, and it does not matter how the worker
//!   released: with the tool or with `riff release`.
//! - The Stop hook runs when the turn ends, so the worker is idle. In a
//!   worker in tmux it starts [`check`] as a detached process
//!   (`riff hook clear`), and returns at once
//!   (01M3JQCCZ5M9VY3RGXWJYJN9Q9).
//! - [`check`] sends one keep-alive. A failed call is sent again, at
//!   most [`ASKS`] times. With no ask to clear, it does nothing. With
//!   the ask, it fast-forwards the main clone
//!   ([`crate::hygiene::fast_forward`]), tells the lead when it cannot,
//!   and types the keys into the pane after [`CLEAR_WAIT`].
//! - The clear comes at the end of the turn, not at the release. So a
//!   worker does the steps after its release in the same turn.
//! - A worker has its riff session ID in `RIFF_SESSION`
//!   (01M3JPQT9BA7JVMZPV68FY4MQ6). So after `/clear` it keeps its ID,
//!   its lead and its watch (01M3JQCD16CNWN5FCQBRKHXYMP).
//! - The keys that clear the context and start the next item are
//!   specific to an agent tool. They are in an [`Agent`] adapter, one
//!   for each tool (01M3JQCD373XZWNSSQYBE561TM).
//! - riff never clears the lead: a worker is never the lead
//!   (01M3X9XA3H6YF0QCYSNB2P0CT2), and only a worker comes into
//!   MustClear.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};
use riff_core::name::SessionUri;
use serde::Deserialize;

use crate::api::{Api, LEAD};
use crate::hygiene;
use crate::terminal::quote;

/// How many times [`check`] sends its keep-alive, when the call fails.
pub const ASKS: u32 = 5;
/// How long [`check`] waits after a failed keep-alive.
pub const ASK_WAIT: Duration = Duration::from_secs(2);

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
    /// The input that compacts the context with `instructions`
    /// (01M3Q88G98MKH364WEQGT4ZE7A).
    fn compact(&self, instructions: &str) -> String;
    /// True when the input line on `screen` is empty, so that riff can
    /// type (01M3Q88GEB9NK5P6DNFJG4618Q). False when riff cannot find
    /// the input line.
    fn input_empty(&self, screen: &str) -> bool;
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

    /// ```
    /// use riff::next::{Agent, ClaudeCode};
    /// assert_eq!(ClaudeCode.compact("Keep the plan."), "/compact Keep the plan.");
    /// ```
    fn compact(&self, instructions: &str) -> String {
        format!("/compact {instructions}")
    }

    /// The input box of Claude Code is the prompt mark `❯` (or `>`)
    /// after a rule line, up to the next rule line.
    ///
    /// ```
    /// use riff::next::{Agent, ClaudeCode};
    /// let screen = |input: &str| format!("Done.\n\n────────\n{input}\n────────\n  riff l1\n");
    /// assert!(ClaudeCode.input_empty(&screen("❯\u{a0}")));
    /// assert!(ClaudeCode.input_empty(&screen("> ")));
    /// assert!(!ClaudeCode.input_empty(&screen("❯ fix the te")));
    /// assert!(!ClaudeCode.input_empty(&screen("❯ \n  second line")));
    /// assert!(ClaudeCode.input_empty("╭────╮\n│ >  │\n╰────╯"));
    /// assert!(!ClaudeCode.input_empty("no input box here"));
    /// ```
    fn input_empty(&self, screen: &str) -> bool {
        let rule = |l: &str| l.trim_start().starts_with(['─', '╭', '╰']);
        let lines: Vec<&str> = screen.lines().collect();
        let Some(at) = (1..lines.len()).rev().find(|&i| {
            rule(lines[i - 1])
                && lines[i]
                    .trim_start_matches(|c: char| c == '│' || c.is_whitespace())
                    .starts_with(['❯', '>'])
        }) else {
            return false;
        };
        let text = |l: &str| {
            l.trim_matches(|c: char| c == '│' || c.is_whitespace())
                .to_owned()
        };
        let first = text(lines[at]);
        let first = first.trim_start_matches(['❯', '>']).trim();
        first.is_empty()
            && lines[at + 1..]
                .iter()
                .take_while(|l| !rule(l))
                .all(|l| text(l).is_empty())
    }
}

/// The part of the Stop hook input that riff uses.
#[derive(Debug, Default, Deserialize)]
pub struct StopInput {
    /// The session ID of the agent tool.
    pub session_id: Option<String>,
    /// The transcript of the session.
    pub transcript_path: Option<std::path::PathBuf>,
}

/// The check of the Stop hook of the worker `me`, whose agent runs in
/// the tmux pane `pane` and works in `dir` (01M3XV0562D3H3P22CJDBPAZBH).
/// It asks the server with a keep-alive. When the worker must clear its
/// context, it fast-forwards the main clone of `dir`
/// (01M3MNP34M5PAZW9VWAYVGNSV2), tells the lead when the main clone
/// stays as it is (01M3MNP36TZYN3PE00AZJTJSER), and starts the keys of
/// the clear. It gives true when it started the keys.
///
/// A keep-alive that fails is sent again after [`ASK_WAIT`], at most
/// [`ASKS`] times. A session that left the riff makes no call
/// ([`crate::leave`]), so riff does not clear it.
pub async fn check(api: &Api, me: &SessionUri, pane: &str, dir: &Path) -> Result<bool> {
    let mut reply = api.alive(me).await;
    for _ in 1..ASKS {
        if reply.is_ok() {
            break;
        }
        tokio::time::sleep(ASK_WAIT).await;
        reply = api.alive(me).await;
    }
    if !reply?.clear {
        return Ok(false);
    }
    let fresh = hygiene::fast_forward(dir);
    if let Some(line) = fresh.line()
        && fresh.tells_the_lead()
        && let Err(e) = api.tell(me, LEAD, &line).await
    {
        eprintln!("riff: cannot tell the lead: {e:#}");
    }
    spawn(&ClaudeCode, pane)?;
    Ok(true)
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
/// that it outlives the check.
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
    fn the_stop_input_reads_the_session() {
        let input: StopInput =
            serde_json::from_str(r#"{"session_id":"a6cf","hook_event_name":"Stop"}"#).unwrap();
        assert_eq!(input.session_id.as_deref(), Some("a6cf"));
    }
}
