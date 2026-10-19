//! The MCP servers of the container sessions of a host.
//!
//! # Design
//!
//! A container session gets the riff tools from the relay
//! ([`crate::session_relay::with_tools`]). A person can give each new
//! session of the host more MCP servers (01M57840K7SNPS243CYMYNYVFX):
//!
//! - `riff daemon mcp add NAME` copies the server NAME from the MCP
//!   config of the person ([`person_servers`]) into
//!   [`FILE`] in the folder of the daemon. Only the person can read the
//!   file, because a server config can hold a token. A later change of
//!   the config of the person counts only after a new `add`.
//! - A stdio server runs in the container, so its program must be in
//!   the session image. `add` looks for it in the image of the
//!   repository of the current folder ([`has_program`]), and refuses
//!   when it is not there (01M57840NN08NA4PH0YR81PDGT). A server with a
//!   URL needs no program.
//! - `riff daemon mcp remove NAME` takes it out. `riff` always stays.
//!
//! At each start of a session, the daemon reads [`FILE`] and gives the
//! servers to the container in [`VAR`], an [`Env::Host`](crate::runtime::Env::Host):
//! its value is in no spec and in no command line. The relay adds them
//! to the `--mcp-config` of `claude`, and starts `claude` with no
//! [`VAR`] (01M57840R2HNXGP9B2ENSKTWD3). A change counts for each new
//! session; a session that runs keeps its servers.
//!
//! ```mermaid
//! flowchart LR
//!     C["~/.claude.json<br/>.mcp.json"] -- "riff daemon mcp add github" --> F[("mcp.json<br/>in the folder of the daemon")]
//!     F -- "each start" --> D[riff daemon]
//!     D -- "RIFF_MCP_SERVERS" --> R["riff relay in the container"]
//!     R --> P["claude --mcp-config<br/>riff, github"]
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

use crate::settings::RIFF_MCP;
use crate::text;

/// The file of the servers, in the folder of the daemon.
pub const FILE: &str = "mcp.json";

/// The program of the containers.
const PODMAN: &str = "podman";

/// The variable that gives the servers to the relay of a session.
pub const VAR: &str = "RIFF_MCP_SERVERS";

/// The servers in the folder `dir` of the daemon: the `mcpServers` of
/// [`FILE`]. None when the file is not there. A file that riff cannot
/// read is an error.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// assert!(riff::session_mcp::read(dir.path())?.is_empty());
/// std::fs::write(dir.path().join("mcp.json"), r#"{"mcpServers": {"docs": {"url": "u"}}}"#)?;
/// assert_eq!(riff::session_mcp::read(dir.path())?["docs"]["url"], "u");
/// std::fs::write(dir.path().join("mcp.json"), "no")?;
/// assert!(riff::session_mcp::read(dir.path()).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read(dir: &Path) -> Result<Map<String, Value>> {
    let path = dir.join(FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
    };
    let value: Value = serde_json::from_str(&text)
        .with_context(|| format!("cannot read the MCP servers in {}", path.display()))?;
    Ok(value
        .get("mcpServers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default())
}

/// Writes `servers` to [`FILE`] in `dir`, readable only by the person.
fn write(dir: &Path, servers: &Map<String, Value>) -> Result<()> {
    write_as(dir, FILE, &json!({ "mcpServers": servers })).map(drop)
}

/// The names of the servers of each session: `riff` first, then each
/// name of `servers` in the order of the alphabet.
///
/// ```
/// let servers = serde_json::json!({"github": {}, "docs": {}});
/// assert_eq!(riff::session_mcp::names(servers.as_object().unwrap()), ["riff", "docs", "github"]);
/// ```
pub fn names(servers: &Map<String, Value>) -> Vec<String> {
    let mut others: Vec<String> = servers.keys().filter(|n| *n != RIFF_MCP).cloned().collect();
    others.sort();
    std::iter::once(RIFF_MCP.to_owned()).chain(others).collect()
}

/// The program that the server `server` runs in the container: the
/// `command` of a stdio server. None for a server with a URL.
///
/// ```
/// use riff::session_mcp::program;
/// use serde_json::json;
/// assert_eq!(program(&json!({"command": "npx", "args": ["x"]})), Some("npx"));
/// assert_eq!(program(&json!({"type": "stdio", "command": "/usr/bin/gh"})), Some("/usr/bin/gh"));
/// assert_eq!(program(&json!({"type": "http", "url": "https://x"})), None);
/// ```
pub fn program(server: &Value) -> Option<&str> {
    match server.get("type").and_then(Value::as_str) {
        None | Some("stdio") => server.get("command").and_then(Value::as_str),
        Some(_) => None,
    }
}

/// `riff daemon mcp add NAME`: copies the server `name` of `person` into
/// [`FILE`] in `dir`, and gives the names (01M57840K7SNPS243CYMYNYVFX).
/// `has` says if the session image has a program; riff asks it only for
/// a stdio server. It refuses a name that `person` does not have, and a
/// program that the image does not have (01M57840NN08NA4PH0YR81PDGT).
///
/// ```
/// use riff::session_mcp::{add, read};
/// use serde_json::json;
/// let dir = tempfile::tempdir()?;
/// let person = json!({"docs": {"type": "http", "url": "u"}, "npm": {"command": "npx"}});
/// let person = person.as_object().unwrap();
/// let never = |_: &str| -> anyhow::Result<bool> { panic!("a URL needs no program") };
/// assert_eq!(add(dir.path(), "docs", person, never)?, ["riff", "docs"]);
/// let refused = add(dir.path(), "npm", person, |p| Ok(p != "npx")).unwrap_err();
/// assert!(refused.to_string().contains("npx"), "{refused}");
/// assert!(add(dir.path(), "nope", person, |_| Ok(true)).is_err());
/// assert_eq!(add(dir.path(), "riff", person, |_| Ok(true))?, ["riff", "docs"]);
/// assert_eq!(read(dir.path())?.len(), 1);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn add(
    dir: &Path,
    name: &str,
    person: &Map<String, Value>,
    has: impl FnOnce(&str) -> Result<bool>,
) -> Result<Vec<String>> {
    let mut servers = read(dir)?;
    if name == RIFF_MCP {
        return Ok(names(&servers));
    }
    let Some(server) = person.get(name) else {
        bail!("{}", text::session_mcp_unknown(name));
    };
    if let Some(program) = program(server)
        && !has(program)?
    {
        bail!("{}", text::session_mcp_no_program(name, program));
    }
    servers.insert(name.to_owned(), server.clone());
    write(dir, &servers)?;
    Ok(names(&servers))
}

/// `riff daemon mcp remove NAME`: takes the server `name` out of
/// [`FILE`] in `dir`, and gives the names. It refuses `riff`.
///
/// ```
/// use riff::session_mcp::{add, remove};
/// let dir = tempfile::tempdir()?;
/// let person = serde_json::json!({"docs": {"url": "u"}});
/// add(dir.path(), "docs", person.as_object().unwrap(), |_| Ok(true))?;
/// assert!(remove(dir.path(), "riff").is_err());
/// assert_eq!(remove(dir.path(), "docs")?, ["riff"]);
/// assert_eq!(remove(dir.path(), "docs")?, ["riff"]);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn remove(dir: &Path, name: &str) -> Result<Vec<String>> {
    if name == RIFF_MCP {
        bail!("{}", text::SESSION_MCP_KEEPS_RIFF);
    }
    let mut servers = read(dir)?;
    if servers.remove(name).is_some() {
        write(dir, &servers)?;
    }
    Ok(names(&servers))
}

/// The secrets of `servers`: each string value of the `env` and the
/// `headers` of each server (01M57840R2HNXGP9B2ENSKTWD3).
///
/// ```
/// let servers = serde_json::json!({
///     "a": {"command": "x", "env": {"TOKEN": "t1", "EMPTY": ""}},
///     "b": {"url": "u", "headers": {"Authorization": "Bearer t2"}},
/// });
/// assert_eq!(riff::session_mcp::secrets(servers.as_object().unwrap()), ["t1", "Bearer t2"]);
/// ```
pub fn secrets(servers: &Map<String, Value>) -> Vec<String> {
    servers
        .values()
        .flat_map(|server| ["env", "headers"].map(|key| server.get(key)))
        .flatten()
        .filter_map(Value::as_object)
        .flat_map(Map::values)
        .filter_map(Value::as_str)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The servers of the value of [`VAR`], for the relay. None when the
/// value is not there or is not a JSON object.
///
/// ```
/// use riff::session_mcp::from_var;
/// assert_eq!(from_var(Some(r#"{"docs": {"url": "u"}}"#))["docs"]["url"], "u");
/// assert!(from_var(Some("no")).is_empty());
/// assert!(from_var(None).is_empty());
/// ```
pub fn from_var(value: Option<&str>) -> Map<String, Value> {
    value
        .and_then(|v| serde_json::from_str::<Value>(v).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// True when the image `image` has the program `program`: `podman run`
/// of the image looks for it with `command -v`, with no network. podman
/// pulls the image when this machine does not have it.
pub fn has_program(image: &str, program: &str) -> Result<bool> {
    let status = std::process::Command::new(PODMAN)
        .args(["run", "--rm", "--network=none", "--entrypoint", "/bin/sh"])
        .arg(image)
        .args(["-c", "command -v \"$1\" >/dev/null", "sh", program])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status()
        .with_context(|| text::no_program(PODMAN))?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1 | 127) => Ok(false),
        _ => bail!("podman run of {image}: {status}"),
    }
}

/// The config file of Claude Code: `$CLAUDE_CONFIG_DIR/.claude.json`,
/// else `$HOME/.claude.json`. An empty value counts as unset.
///
/// ```
/// use riff::session_mcp::claude_json_from;
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
/// let all = riff::session_mcp::person_servers(Some(&claude), &main);
/// assert_eq!(all["github"]["command"], "gh2");
/// assert_eq!(all["unifi"]["command"], "u2");
/// assert!(riff::session_mcp::person_servers(None, &main).contains_key("github"));
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

/// Writes `config` to the file `name` in `dir`, readable only by the person.
pub fn write_as(dir: &Path, name: &str, config: &Value) -> Result<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    crate::local::make_dir(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let path = dir.join(name);
    let new = dir.join(format!("{name}.{}", std::process::id()));
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
