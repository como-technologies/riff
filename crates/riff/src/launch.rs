//! What riff gives `claude` at each start.
//!
//! # Design
//!
//! riff is not in the Claude config of the person: no riff MCP server,
//! plugin, hook, permission rule or status line in `~/.claude` or in
//! `.claude/` of a repository. A plain `claude` is plain Claude. riff
//! gives each of these to each `claude` that it starts: the lead
//! ([`crate::start`]) and each worker ([`crate::worker`])
//! (01M4BYH7Y3P1JMQR51TWFGVZ39).
//!
//! | Flag | Gives |
//! |---|---|
//! | `--plugin-dir DIR` | The plugin: the skill, the hooks, the commands ([`crate::plugin::root`]). |
//! | `--strict-mcp-config --mcp-config FILE` | The riff MCP server and the servers of `workers.mcp`, and no other ([`crate::worker_mcp`]). |
//! | `--settings` | The status line ([`statusline`]), the rules of riff work ([`riff_rules`], 01M4BYH874WQ16Q0337WQA8AMV) and the rules of the profile of the role ([`crate::role_rules`]). |
//! | the variable `RIFF_ON=1` ([`ON`]) | The hooks, the status line and `riff mcp` act (01M4BYH80CFW1TBGKVA2VN9ZBQ). |
//!
//! ```mermaid
//! flowchart LR
//!     R["riff (lead)<br/>riff workers start"] -- "writes" --> P[("plugin dir")]
//!     R -- "writes" --> M[("workers-mcp.json")]
//!     R --> C["claude --plugin-dir<br/>--strict-mcp-config --mcp-config<br/>--settings, RIFF_ON=1"]
//!     P --> C
//!     M --> C
//!     U["~/.claude, .claude/"] -. "no riff entry" .-> C
//! ```
//!
//! A plain `claude` with the plugin of an older release has no
//! `RIFF_ON=1`, so the hooks of that plugin do nothing
//! ([`crate::enable`]). `riff` removes the entries of older releases
//! after the person confirms ([`crate::old_config`]).
//!
//! ```
//! use riff::launch::{Given, args};
//!
//! let given = Given { plugin: "/d/riff".into(), mcp: "/s/workers-mcp.json".into() };
//! assert_eq!(
//!     args(&given),
//!     ["--plugin-dir", "/d/riff", "--strict-mcp-config", "--mcp-config", "/s/workers-mcp.json"],
//! );
//! ```

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Map, Value, json};

use crate::permissions::Rules;

/// The variable that riff gives each `claude` that it starts
/// (01M4BYH80CFW1TBGKVA2VN9ZBQ).
pub const ON: (&str, &str) = (crate::enable::VAR, "1");

/// The command of the riff status line.
pub const STATUSLINE: &str = "riff statusline";

/// The files that riff gives `claude` with its flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Given {
    /// The root of the plugin, for `--plugin-dir`.
    pub plugin: PathBuf,
    /// The MCP config, for `--mcp-config`.
    pub mcp: PathBuf,
}

impl Given {
    /// Writes the plugin and the MCP config for the repository in
    /// `main`, with this `riff` binary at `riff` as the riff MCP server.
    pub fn prepare(main: &Path, riff: &Path) -> Result<Given> {
        Ok(Given {
            plugin: crate::plugin::root()?,
            mcp: crate::worker_mcp::prepare(main, riff)?,
        })
    }
}

/// The flags of `claude` for `given`. `--mcp-config` takes more than
/// one value, so a caller puts `--settings` after it.
pub fn args(given: &Given) -> Vec<String> {
    vec![
        "--plugin-dir".to_owned(),
        given.plugin.to_string_lossy().into_owned(),
        "--strict-mcp-config".to_owned(),
        "--mcp-config".to_owned(),
        given.mcp.to_string_lossy().into_owned(),
    ]
}

/// The `statusLine` setting of the riff status line
/// (01M4BYH7Y3P1JMQR51TWFGVZ39).
///
/// ```
/// assert_eq!(riff::launch::statusline()["command"], "riff statusline");
/// ```
pub fn statusline() -> Value {
    json!({"type": "command", "command": STATUSLINE})
}

/// The flag settings with the status line and `rules`.
///
/// ```
/// use riff::permissions::Rules;
///
/// let rules = Rules { allow: vec!["Bash(riff *)".into()], deny: vec!["D".into()] };
/// let settings = riff::launch::settings(&rules);
/// assert_eq!(settings["statusLine"]["command"], "riff statusline");
/// assert_eq!(settings["permissions"]["allow"][0], "Bash(riff *)");
/// assert_eq!(settings["permissions"]["deny"][0], "D");
/// ```
pub fn settings(rules: &Rules) -> Map<String, Value> {
    let mut settings = Map::new();
    settings.insert("statusLine".into(), statusline());
    let args = crate::role_rules::flag(
        &[
            "--settings".to_owned(),
            Value::Object(settings).to_string(),
        ],
        rules,
    );
    match serde_json::from_str(&args[1]) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

/// The rules of riff work in the repository of `dir`: the riff tools,
/// the riff commands and the steps of a pull request, and no push to
/// the default branch (01M4BYH874WQ16Q0337WQA8AMV). See
/// [`crate::permissions::rules`].
pub fn riff_rules(dir: &Path) -> Rules {
    crate::permissions::Project::of(dir).rules()
}

/// `rules` and `more` in one set, each rule once.
///
/// ```
/// use riff::permissions::Rules;
///
/// let a = Rules { allow: vec!["A".into()], deny: vec!["D".into()] };
/// let b = Rules { allow: vec!["A".into(), "B".into()], deny: vec![] };
/// let both = riff::launch::merge(a, &b);
/// assert_eq!(both.allow, ["A", "B"]);
/// assert_eq!(both.deny, ["D"]);
/// ```
pub fn merge(mut rules: Rules, more: &Rules) -> Rules {
    for (have, want) in [
        (&mut rules.allow, &more.allow),
        (&mut rules.deny, &more.deny),
    ] {
        for rule in want {
            if !have.contains(rule) {
                have.push(rule.clone());
            }
        }
    }
    rules
}
