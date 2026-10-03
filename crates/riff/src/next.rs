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
//!     H->>H: count the prompts of the transcript
//!     H->>C: start, detached, with the count
//!     H-->>W: return at once
//!     C->>S: keep-alive
//!     S-->>C: clear
//!     C->>C: fast-forward the main clone
//!     C->>C: count the prompts again: no new turn
//!     C->>C: stop the processes of the old context
//!     C->>C: count the prompts again: no new turn
//!     C->>T: /clear, then "Join the riff."
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
//!   and types the keys into the pane with [`keys`].
//! - Just before the keys, after a count that shows no new turn,
//!   [`keys`] stops each process of the old context of the worker, for
//!   example a `just ci` in the background ([`stop_old_context`],
//!   01M3ZV0TJDQ6JCM7XG0036MSV1). It keeps `claude`, its MCP servers
//!   and the watch. Then it counts again. When it stopped one, [`check`]
//!   posts a note to the lead with the pane and each process.
//! - The clear comes at the end of the turn, not at the release. So a
//!   worker does the steps after its release in the same turn.
//! - The keys never come in a turn that the agent started after the
//!   Stop hook (01M3XZCWQED9M9ZB29F730EA58, 01M3ZS67FTAC1784GEVEDXJ837).
//!   The Stop hook counts the prompts in the transcript of the agent
//!   ([`crate::compact::prompts`]) before it returns, and gives the
//!   number to the check. A wake that waits at the end of the turn
//!   starts the next turn when the hook returns, so the check cannot
//!   count first. [`keys`] counts again as the last step before
//!   `/clear`. A higher number shows a new turn: the check types
//!   nothing, and the Stop hook of that turn starts a new check.
//! - Two times stay. The first is the time of one `tmux` call after the
//!   last count: a few milliseconds. The second is the time between
//!   `/clear` and the start prompt. It does no harm: the context is
//!   fresh, and the start prompt asks only for the start routine, so no
//!   answer of the old context is lost.
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
use std::time::Duration;

use anyhow::Result;
use riff_core::name::SessionUri;
use riff_core::selector::Selector;
use riff_core::wire::Kind;
use serde::Deserialize;

use crate::api::{Api, LEAD};
use crate::hygiene;
use crate::terminal::{Terminal, Tmux};

/// How many times [`check`] sends its keep-alive, when the call fails.
pub const ASKS: u32 = 5;
/// How long [`check`] waits after a failed keep-alive.
pub const ASK_WAIT: Duration = Duration::from_secs(2);

/// How long [`keys`] waits before it types `/clear`, so that the agent
/// tool is ready for input after its turn.
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
    /// The variable that the agent tool gives to each process of a
    /// context, and not to itself or to its MCP servers
    /// ([`crate::workload`], 01M3ZV0QSFVCHRSEKYK57B88VA).
    fn context_var(&self) -> &str;
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

    /// ```
    /// use riff::next::{Agent, ClaudeCode};
    /// assert_eq!(ClaudeCode.context_var(), "CLAUDE_PID");
    /// ```
    fn context_var(&self) -> &str {
        "CLAUDE_PID"
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
/// stays as it is (01M3MNP36TZYN3PE00AZJTJSER), and types the keys of
/// the clear with [`keys`]. It gives true when it typed them.
///
/// A keep-alive that fails is sent again after [`ASK_WAIT`], at most
/// [`ASKS`] times. A session that left the riff makes no call
/// ([`crate::leave`]), so riff does not clear it.
///
/// `turns` is the number of prompts that the Stop hook counted in the
/// `transcript` of the agent ([`prompts`]). With no number, the check
/// counts when it starts. When a new turn started after that count, the
/// check types nothing: the Stop hook of the new turn starts a new check
/// (01M3XZCWQED9M9ZB29F730EA58).
pub async fn check(
    api: &Api,
    me: &SessionUri,
    pane: &str,
    dir: &Path,
    transcript: Option<&Path>,
    turns: Option<usize>,
) -> Result<bool> {
    let turns = turns.unwrap_or_else(|| prompts(transcript));
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
    let tmux = Tmux::machine();
    let mut stopped = Vec::new();
    let typed = keys(
        &ClaudeCode,
        turns,
        || prompts(transcript),
        std::thread::sleep,
        || stopped = stop_old_context(&ClaudeCode, me),
        |text| tmux.type_line(pane, text),
    )?;
    if !stopped.is_empty() {
        let to = Selector::lead(me.who().user(), &me.place().repo_text());
        let note = crate::text::old_context_stopped(pane, &stopped);
        if let Err(e) = api.post(me, None, &[to], &note, Kind::Note).await {
            eprintln!("riff: cannot post the note to the lead: {e:#}");
        }
    }
    Ok(typed)
}

/// Stops each process of the old context of the worker `me`, just
/// before the clear (01M3ZV0TJDQ6JCM7XG0036MSV1): its background
/// commands, but not the watch ([`crate::workload::old_context`]).
/// Returns each process that it stopped.
pub fn stop_old_context(agent: &dyn Agent, me: &SessionUri) -> Vec<crate::workload::Proc> {
    let Some(session) = me.who().session() else {
        return Vec::new();
    };
    let var = agent.context_var();
    let all = crate::workload::all(var);
    let old = crate::workload::old_context(&all, session, std::process::id(), None);
    crate::workload::stop(&old, var)
}

/// The number of prompts in the `transcript` of the agent
/// ([`crate::compact::prompts`]): the turns that started. It is 0 with
/// no transcript.
///
/// ```
/// assert_eq!(riff::next::prompts(None), 0);
/// ```
pub fn prompts(transcript: Option<&Path>) -> usize {
    let text = transcript.and_then(|path| std::fs::read_to_string(path).ok());
    text.map_or(0, |text| crate::compact::prompts(&text))
}

/// Types the keys of `agent` with `type_line`: it waits [`CLEAR_WAIT`],
/// types `/clear`, waits [`PROMPT_WAIT`], and types the start prompt.
/// It counts the prompts with `now` after the wait, and again as the
/// last step before `/clear`. When a count is not `turns`, a new turn
/// started: it types nothing and gives false
/// (01M3ZS67FTAC1784GEVEDXJ837). Between the two counts it calls
/// `stop`, which stops the old context (01M3ZV0TJDQ6JCM7XG0036MSV1).
/// So riff stops no process of a turn that started before the wait
/// ended.
///
/// ```
/// use riff::next::{ClaudeCode, keys};
/// let (mut typed, mut stops) = (Vec::new(), 0);
/// let done = keys(&ClaudeCode, 1, || 1, |_| {}, || stops += 1, |t| Ok(typed.push(t.to_owned()))).unwrap();
/// assert!(done);
/// assert_eq!((typed, stops), (vec!["/clear".to_owned(), "Join the riff.".to_owned()], 1));
///
/// // A turn started in the wait: no stop and no keys.
/// let (mut typed, mut stops) = (Vec::new(), 0);
/// let done = keys(&ClaudeCode, 1, || 2, |_| {}, || stops += 1, |t| Ok(typed.push(t.to_owned()))).unwrap();
/// assert!(!done && typed.is_empty() && stops == 0);
/// ```
pub fn keys(
    agent: &dyn Agent,
    turns: usize,
    now: impl Fn() -> usize,
    mut wait: impl FnMut(Duration),
    stop: impl FnOnce(),
    mut type_line: impl FnMut(&str) -> Result<()>,
) -> Result<bool> {
    wait(CLEAR_WAIT);
    if now() != turns {
        return Ok(false);
    }
    stop();
    if now() != turns {
        return Ok(false);
    }
    type_line(agent.clear())?;
    wait(PROMPT_WAIT);
    type_line(agent.start_prompt())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A turn that starts in the wait before `/clear` gets no keys.
    #[test]
    fn a_turn_that_starts_in_the_wait_gets_no_keys() {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("transcript.jsonl");
        let prompt = r#"{"type":"user","message":{"content":"Join the riff."}}"#;
        std::fs::write(&transcript, format!("{prompt}\n")).unwrap();
        let turns = prompts(Some(&transcript));
        let mut typed = Vec::new();
        let done = keys(
            &ClaudeCode,
            turns,
            || prompts(Some(&transcript)),
            |wait| {
                if wait == CLEAR_WAIT {
                    std::fs::write(&transcript, format!("{prompt}\n{prompt}\n")).unwrap();
                }
            },
            || panic!("no stop after a new turn"),
            |text| {
                typed.push(text.to_owned());
                Ok(())
            },
        )
        .unwrap();
        assert!(!done);
        assert!(typed.is_empty(), "{typed:?}");
    }

    /// A failed key stops the keys.
    #[test]
    fn a_failed_key_stops_the_keys() {
        let mut typed = 0;
        let done = keys(
            &ClaudeCode,
            0,
            || 0,
            |_| {},
            || {},
            |_| {
                typed += 1;
                anyhow::bail!("no tmux")
            },
        );
        assert!(done.is_err());
        assert_eq!(typed, 1);
    }

    /// A turn that starts while riff stops the old context gets no keys
    /// (01M3ZV0TJDQ6JCM7XG0036MSV1).
    #[test]
    fn a_turn_that_starts_in_the_stop_gets_no_keys() {
        let count = std::cell::Cell::new(1);
        let mut typed = Vec::new();
        let done = keys(
            &ClaudeCode,
            1,
            || count.get(),
            |_| {},
            || count.set(2),
            |text| {
                typed.push(text.to_owned());
                Ok(())
            },
        )
        .unwrap();
        assert!(!done);
        assert!(typed.is_empty(), "{typed:?}");
    }

    #[test]
    fn the_stop_input_reads_the_session() {
        let input: StopInput =
            serde_json::from_str(r#"{"session_id":"a6cf","hook_event_name":"Stop"}"#).unwrap();
        assert_eq!(input.session_id.as_deref(), Some("a6cf"));
    }
}
