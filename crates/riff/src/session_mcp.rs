//! The MCP servers of the container sessions of a host.
//!
//! # Design
//!
//! A container session gets the riff tools from the relay
//! ([`crate::session_relay::with_tools`]). A person can give each new
//! session of the host more MCP servers, as `riff workers mcp` does for
//! the workers in tmux (01M57840K7SNPS243CYMYNYVFX):
//!
//! - `riff daemon mcp add NAME` copies the server NAME from the MCP
//!   config of the person ([`crate::worker_mcp::person_servers`]) into
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

use std::path::Path;

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
    crate::worker_mcp::write_as(dir, FILE, &json!({ "mcpServers": servers })).map(drop)
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
