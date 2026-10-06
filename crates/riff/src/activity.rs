//! The facts of the hooks: what a session does now
//! (01M41FZNTPXQNCZ1S99HE42PYQ).
//!
//! # Design
//!
//! riff makes the state of a session from facts. A session does not
//! report it. The hooks of Claude Code see each tool call and the end of
//! each turn. A hook writes the newest fact to a file on the machine,
//! and makes no call. `riff mcp` reads the file and puts the fact into
//! the keep-alive that it sends anyway: each minute, in a worker each 10
//! seconds. So the server gets no more calls, and the fact is at most
//! one keep-alive old.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant C as Claude Code
//!     participant H as riff hook tool / stop
//!     participant F as activity-ID
//!     participant M as riff mcp
//!     participant S as riff-server
//!     C->>H: PreToolUse, PostToolUse, Stop (stdin)
//!     H->>F: the newest fact, with its time
//!     loop each keep-alive
//!         M->>F: read
//!         M->>S: alive, with the fact and its age
//!     end
//! ```
//!
//! | Hook | Fact |
//! |---|---|
//! | `PreToolUse` | a tool runs: `Bash: run just ci` |
//! | `PostToolUse` | a turn runs, between two tools |
//! | `Stop` | the turn ended |
//! | `UserPromptSubmit` | the person gave a prompt: its time, in a file of its own |
//!
//! The prompt hook (`riff hook prompt`) keeps the time of the last
//! prompt in its own file, so the next tool fact does not hide it. The
//! keep-alive carries its age. A prompt after a block ends the block:
//! the person answered (01M48VDWPDYRPEAXHR1MYDN1M7).
//!
//! A riff tool and `riff watch` give no fact: they are no work. So a
//! session that a message wakes, and that only reads, does not show
//! work. The text of a Bash call is its description, never its command:
//! a command can hold a secret. A fact is one line of at most
//! [`ACTIVITY_CHARS`] characters.
//!
//! ```
//! use riff::activity::{Fact, Hook, of_hook};
//!
//! let input = r#"{"tool_name":"Bash","tool_input":{"command":"just ci","description":"Run just ci"}}"#;
//! let fact = of_hook(Hook::Pre, input).unwrap();
//! assert_eq!(fact, Fact { tool: Some("Bash: Run just ci".into()), turn: true });
//! let riff = r#"{"tool_name":"mcp__riff__read","tool_input":{}}"#;
//! assert_eq!(of_hook(Hook::Pre, riff), None);
//! let watch = r#"{"tool_name":"Bash","tool_input":{"command":"riff watch --once"}}"#;
//! assert_eq!(of_hook(Hook::Pre, watch), None);
//! assert_eq!(of_hook(Hook::Stop, "{}"), Some(Fact { tool: None, turn: false }));
//! ```

use std::path::{Path, PathBuf};

use riff_core::wire::{ACTIVITY_CHARS, Activity};
use serde::{Deserialize, Serialize};

/// A hook of Claude Code that gives a fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hook {
    /// `PreToolUse`: a tool starts.
    Pre,
    /// `PostToolUse`: a tool ended, and the turn goes on.
    Post,
    /// `Stop`: the turn ended.
    Stop,
}

/// What a hook saw: the tool that runs, and whether a turn runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default)]
    pub turn: bool,
}

/// The part of the hook input that riff reads.
#[derive(Debug, Default, Deserialize)]
struct Input {
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Option<serde_json::Value>,
}

/// The fact of the hook `hook` with the input `input` (the JSON on
/// stdin). `None` for a riff tool and for `riff watch`: they are no
/// work.
pub fn of_hook(hook: Hook, input: &str) -> Option<Fact> {
    let input: Input = serde_json::from_str(input).unwrap_or_default();
    if hook == Hook::Stop {
        return Some(Fact {
            tool: None,
            turn: false,
        });
    }
    let name = input.tool_name.unwrap_or_default();
    let field = |key: &str| {
        input
            .tool_input
            .as_ref()
            .and_then(|i| i.get(key))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or_default()
            .to_owned()
    };
    if name.starts_with("mcp__riff__") || name.starts_with("mcp__plugin_riff_") {
        return None;
    }
    if name == "Bash" && field("command").starts_with("riff watch") {
        return None;
    }
    let tool = (hook == Hook::Pre).then(|| {
        let description = field("description");
        let text = if name == "Bash" && !description.is_empty() {
            format!("Bash: {description}")
        } else {
            name
        };
        one_line(&text)
    });
    Some(Fact { tool, turn: true })
}

/// `text` as one line of at most [`ACTIVITY_CHARS`] characters, with no
/// control character.
fn one_line(text: &str) -> String {
    let line: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    line.trim().chars().take(ACTIVITY_CHARS).collect()
}

/// A fact with the time of the hook, as the file keeps it.
#[derive(Debug, Serialize, Deserialize)]
struct Saved {
    #[serde(flatten)]
    fact: Fact,
    at_ms: u64,
}

/// The file of the facts of the session `session` in `dir`.
pub fn path(dir: &Path, session: &str) -> PathBuf {
    dir.join(format!("activity-{session}"))
}

/// Writes `fact` as the newest fact of `session`, at `at_ms`.
///
/// ```
/// use riff::activity::{Fact, read, write};
///
/// let dir = tempfile::tempdir()?;
/// let fact = Fact { tool: Some("Edit".into()), turn: true };
/// write(dir.path(), "a1", &fact, 1_000)?;
/// let activity = read(dir.path(), "a1", 31_000).unwrap();
/// assert_eq!((activity.tool.as_deref(), activity.turn, activity.secs), (Some("Edit"), true, 30));
/// assert!(read(dir.path(), "b2", 31_000).is_none());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn write(dir: &Path, session: &str, fact: &Fact, at_ms: u64) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let saved = Saved {
        fact: fact.clone(),
        at_ms,
    };
    let path = path(dir, session);
    let new = path.with_extension("new");
    std::fs::write(&new, serde_json::to_vec(&saved)?)?;
    std::fs::rename(new, path)
}

/// The newest fact of `session`, with its age at `now_ms`. `None` when
/// no hook wrote one, or the file does not read.
pub fn read(dir: &Path, session: &str, now_ms: u64) -> Option<Activity> {
    let text = std::fs::read(path(dir, session)).ok()?;
    let saved: Saved = serde_json::from_slice(&text).ok()?;
    Some(Activity {
        tool: saved.fact.tool,
        turn: saved.fact.turn,
        secs: now_ms.saturating_sub(saved.at_ms) / 1000,
    })
}

/// The file of the time of the last prompt of `session` in `dir`.
fn prompt_path(dir: &Path, session: &str) -> PathBuf {
    dir.join(format!("prompt-{session}"))
}

/// Writes `at_ms` as the time of the last prompt of the person in
/// `session` (01M48VDWPDYRPEAXHR1MYDN1M7).
///
/// ```
/// use riff::activity::{prompt_secs, prompted};
///
/// let dir = tempfile::tempdir()?;
/// assert_eq!(prompt_secs(dir.path(), "a1", 31_000), None);
/// prompted(dir.path(), "a1", 1_000)?;
/// assert_eq!(prompt_secs(dir.path(), "a1", 31_000), Some(30));
/// assert_eq!(prompt_secs(dir.path(), "b2", 31_000), None);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn prompted(dir: &Path, session: &str, at_ms: u64) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = prompt_path(dir, session);
    let new = path.with_extension("new");
    std::fs::write(&new, at_ms.to_string())?;
    std::fs::rename(new, path)
}

/// The seconds since the last prompt of the person in `session`, at
/// `now_ms`. `None` when the prompt hook wrote none.
pub fn prompt_secs(dir: &Path, session: &str, now_ms: u64) -> Option<u64> {
    let text = std::fs::read_to_string(prompt_path(dir, session)).ok()?;
    let at_ms: u64 = text.trim().parse().ok()?;
    Some(now_ms.saturating_sub(at_ms) / 1000)
}

/// The prompt hook: reads the input on stdin, and writes the time of
/// the prompt for the session of this agent process. It never fails
/// the hook.
pub fn run_prompt(input: &str) {
    let Some(dir) = crate::local::dir() else {
        return;
    };
    let given = serde_json::from_str::<serde_json::Value>(input)
        .ok()
        .and_then(|v| v.get("session_id")?.as_str().map(str::to_owned));
    let Some(id) = crate::identity::agent_session(given) else {
        return;
    };
    let _ = prompted(&dir, &id, now_ms());
}

/// The hook `hook`: reads the input on stdin, and writes the fact of the
/// session of this agent process. It never fails the hook.
pub fn run(hook: Hook, input: &str, session: Option<String>) {
    let (Some(fact), Some(dir)) = (of_hook(hook, input), crate::local::dir()) else {
        return;
    };
    let given = serde_json::from_str::<serde_json::Value>(input)
        .ok()
        .and_then(|v| v.get("session_id")?.as_str().map(str::to_owned));
    let Some(id) = session.or_else(|| crate::identity::agent_session(given)) else {
        return;
    };
    let _ = write(&dir, &id, &fact, now_ms());
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_post_tool_fact_is_a_turn_with_no_tool() {
        let input = r#"{"tool_name":"Edit","tool_input":{"file_path":"/x"}}"#;
        let fact = of_hook(Hook::Post, input).unwrap();
        assert_eq!(
            fact,
            Fact {
                tool: None,
                turn: true
            }
        );
    }

    #[test]
    fn a_bash_call_shows_its_description_and_never_its_command() {
        let input = r#"{"tool_name":"Bash","tool_input":{"command":"TOKEN=s3cr3t deploy"}}"#;
        let fact = of_hook(Hook::Pre, input).unwrap();
        assert_eq!(fact.tool.as_deref(), Some("Bash"));
        let input = r#"{"tool_name":"Bash","tool_input":{"command":"TOKEN=s3cr3t deploy","description":"Deploy\nthe stage"}}"#;
        let fact = of_hook(Hook::Pre, input).unwrap();
        assert_eq!(fact.tool.as_deref(), Some("Bash: Deploy the stage"));
    }

    #[test]
    fn a_long_text_is_cut() {
        let long = "x".repeat(300);
        let input = format!(r#"{{"tool_name":"Bash","tool_input":{{"description":"{long}"}}}}"#);
        let tool = of_hook(Hook::Pre, &input).unwrap().tool.unwrap();
        assert_eq!(tool.chars().count(), ACTIVITY_CHARS);
    }

    #[test]
    fn a_plugin_riff_tool_gives_no_fact() {
        let input = r#"{"tool_name":"mcp__plugin_riff_riff__status","tool_input":{}}"#;
        assert_eq!(of_hook(Hook::Post, input), None);
    }
}
