//! The MCP servers of a worker.
//!
//! # Design
//!
//! A worker needs only the riff MCP server. Each other MCP server of the
//! person costs context and start time, and lets a worker reach things
//! that it has no need for, for example mail
//! (01M3NB5R6X5AV79DQNKKJBH5J8).
//!
//! `riff workers start` runs `claude` with `--strict-mcp-config` and an
//! `--mcp-config` file that holds only the servers of `workers.mcp`
//! (see [`settings`](crate::settings)) (01M3NB5R92ZC61VW6Y45SJEAY9):
//!
//! - `riff`: this `riff` binary with the argument `mcp`, as the riff
//!   plugin runs it.
//! - Each other name: the server with that name in the MCP config of
//!   the person. riff looks in the user config and the local config of
//!   Claude Code (`~/.claude.json`, or `$CLAUDE_CONFIG_DIR/.claude.json`),
//!   and in `.mcp.json` of the main worktree. The local config wins over
//!   `.mcp.json`, and `.mcp.json` wins over the user config. A name that
//!   none of them has is left out, with a warning.
//!
//! ```mermaid
//! flowchart LR
//!     S["config.toml<br/>workers.mcp = [riff, github]"] --> B[riff workers start]
//!     C["~/.claude.json<br/>.mcp.json"] -- "github" --> B
//!     B -- "writes" --> F[("workers-mcp.json<br/>riff, github")]
//!     B --> P["claude --strict-mcp-config<br/>--mcp-config workers-mcp.json"]
//!     F --> P
//! ```
//!
//! `--strict-mcp-config` drops each other server: the claude.ai
//! connectors, the servers of the person, and the MCP server of the
//! riff plugin too. So the riff tools of a worker come from the server
//! `riff` of the file, and their names are `mcp__riff__*`. The skills
//! and the hooks of the plugin still load. They name no tool by its
//! full name.
//!
//! The file is in the given folder of riff
//! ([`given_dir`](crate::confine::given_dir)): each session reads it,
//! and no session writes it (01M4DDWPGRM1P4A76GFHPQHMQF). Only the
//! person reads it, because a server config can hold a token. The
//! command line of a worker names only the file.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Map, Value, json};

use crate::settings::RIFF_MCP;

/// The name of the file with the MCP servers of the workers.
pub const FILE: &str = "workers-mcp.json";

/// The config file of Claude Code: `$CLAUDE_CONFIG_DIR/.claude.json`,
/// else `$HOME/.claude.json`. An empty value counts as unset.
///
/// ```
/// use riff::worker_mcp::claude_json_from;
/// use std::path::PathBuf;
/// assert_eq!(claude_json_from(Some("/c".into()), Some("/h".into())), Some(PathBuf::from("/c/.claude.json")));
/// assert_eq!(claude_json_from(Some("".into()), Some("/h".into())), Some(PathBuf::from("/h/.claude.json")));
/// assert_eq!(claude_json_from(None, None), None);
/// ```
pub fn claude_json_from(
    config: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let set = |v: Option<std::ffi::OsString>| v.filter(|v| !v.is_empty()).map(PathBuf::from);
    set(config)
        .or_else(|| set(home))
        .map(|d| d.join(".claude.json"))
}

/// The config file of Claude Code of this process. See [`claude_json_from`].
pub fn claude_json() -> Option<PathBuf> {
    claude_json_from(
        std::env::var_os("CLAUDE_CONFIG_DIR"),
        std::env::var_os("HOME"),
    )
}

/// The `mcpServers` of a JSON value, or an empty map.
fn servers_of(value: Option<&Value>) -> Map<String, Value> {
    value
        .and_then(|v| v.get("mcpServers"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// A JSON file, or `None` when it is missing or not JSON.
fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// The MCP servers of the person for the repository in `main`: the user
/// config, then `.mcp.json` of `main`, then the local config. A later
/// one wins.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let main = dir.path().join("repo");
/// std::fs::create_dir(&main)?;
/// let claude = dir.path().join(".claude.json");
/// std::fs::write(&claude, serde_json::json!({
///     "mcpServers": {"github": {"command": "gh-mcp"}, "unifi": {"command": "u"}},
///     "projects": {main.to_str().unwrap(): {"mcpServers": {"unifi": {"command": "u2"}}}},
/// }).to_string())?;
/// std::fs::write(main.join(".mcp.json"), r#"{"mcpServers": {"github": {"command": "gh2"}}}"#)?;
/// let all = riff::worker_mcp::person_servers(Some(&claude), &main);
/// assert_eq!(all["github"]["command"], "gh2");
/// assert_eq!(all["unifi"]["command"], "u2");
/// assert!(riff::worker_mcp::person_servers(None, &main).contains_key("github"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn person_servers(claude_json: Option<&Path>, main: &Path) -> Map<String, Value> {
    let config = claude_json.and_then(read_json);
    let mut all = servers_of(config.as_ref());
    all.extend(servers_of(read_json(&main.join(".mcp.json")).as_ref()));
    let local = config
        .as_ref()
        .and_then(|c| c.get("projects"))
        .and_then(|p| p.get(main.to_string_lossy().as_ref()));
    all.extend(servers_of(local));
    all
}

/// The MCP config of a worker: `riff` as `riff mcp`, and each other of
/// `names` from `person`. Returns the config and each name that
/// `person` does not have.
///
/// ```
/// use serde_json::json;
/// let person = json!({"github": {"command": "gh-mcp"}, "gmail": {"url": "x"}});
/// let (config, missing) = riff::worker_mcp::config(
///     &["riff".into(), "github".into(), "nope".into()],
///     "/bin/riff".as_ref(),
///     person.as_object().unwrap(),
/// );
/// assert_eq!(config, json!({"mcpServers": {
///     "riff": {"command": "/bin/riff", "args": ["mcp"]},
///     "github": {"command": "gh-mcp"},
/// }}));
/// assert_eq!(missing, ["nope"]);
/// ```
pub fn config(names: &[String], riff: &Path, person: &Map<String, Value>) -> (Value, Vec<String>) {
    let mut servers = Map::new();
    let mut missing = Vec::new();
    for name in names {
        if name == RIFF_MCP {
            servers.insert(
                name.clone(),
                json!({"command": riff.to_string_lossy(), "args": ["mcp"]}),
            );
        } else if let Some(server) = person.get(name) {
            servers.insert(name.clone(), server.clone());
        } else {
            missing.push(name.clone());
        }
    }
    (json!({ "mcpServers": servers }), missing)
}

/// Writes `config` to [`FILE`] in `dir`, readable only by the person.
/// Returns its path.
///
/// ```
/// use std::os::unix::fs::PermissionsExt;
/// let dir = tempfile::tempdir()?;
/// let path = riff::worker_mcp::write(dir.path(), &serde_json::json!({"mcpServers": {}}))?;
/// assert_eq!(std::fs::read_to_string(&path)?, r#"{"mcpServers":{}}"#);
/// assert_eq!(std::fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn write(dir: &Path, config: &Value) -> Result<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let path = dir.join(FILE);
    let new = dir.join(format!("{FILE}.{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&new)
        .with_context(|| format!("cannot write {}", new.display()))?;
    file.write_all(config.to_string().as_bytes())
        .with_context(|| format!("cannot write {}", new.display()))?;
    std::fs::rename(&new, &path).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(path)
}

/// Writes the MCP config of the workers of the repository in `main`
/// for this machine, and returns its path. It warns about each name of
/// `workers.mcp` that the MCP config of the person does not have.
pub fn prepare(main: &Path, riff: &Path) -> Result<PathBuf> {
    let names = crate::settings::workers_mcp(&crate::settings::path()?)?;
    let person = person_servers(claude_json().as_deref(), main);
    let (config, missing) = config(&names, riff, &person);
    for name in missing {
        eprintln!("{}", crate::text::worker_mcp_missing(&name));
    }
    // No session writes it (01M4DDWPGRM1P4A76GFHPQHMQF).
    write(&crate::confine::given_here()?, &config)
}
