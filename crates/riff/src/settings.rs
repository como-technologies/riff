//! The settings of riff on this machine.
//!
//! They are in `$XDG_CONFIG_HOME/riff/config.toml`, or
//! `~/.config/riff/config.toml` without `XDG_CONFIG_HOME`
//! (01M3JPQT13ANVA7DNJDVNJ0S8P). With `RIFF_HOME`, they are in
//! `$RIFF_HOME/config.toml` (see [`home`](crate::home)). A change keeps each other key and each
//! comment of the file.
//!
//! ```toml
//! [workers]
//! limit = 2
//! interval = 10
//! mcp = ["riff", "github"]
//! jobs = 4
//! nice = 10
//! memory = 22
//! floor = 4
//!
//! [update]
//! auto = true
//!
//! [connect]
//! scope = "repo"
//! ```
//!
//! | Key | Default | Meaning |
//! |---|---|---|
//! | `workers.limit` | 0 | The most workers that `riff workers start` runs on this machine (01M3JPQT35BMR7XMAMMFSCDC2B). |
//! | `workers.interval` | 10 | The seconds between two workers that the rollout of the lead starts. 0 turns the rollout off (see [`rollout`](crate::rollout), 01M3Q5QE9H42FQKEDC5G9GKCWD). |
//! | `workers.mcp` | `["riff"]` | The MCP servers that a worker loads (see [`worker_mcp`](crate::worker_mcp)). |
//! | `workers.jobs` | 0 | The compile jobs and the test threads of one worker. 0: riff makes the number from the machine (see [`limits`](crate::limits), 01M3WFYZRK5CT22GJW6ZHYT9CC). |
//! | `workers.nice` | 10 | The nice value of each worker (01M3WFYZTX05CGDP2NQF9B356K). |
//! | `workers.memory` | 0 | The most memory of all workers of the machine, in GB. 0: riff makes the number from the machine (01M3WFYZX6GVFYW6NTTTKF144R). |
//! | `workers.floor` | 4 | The available memory in GB under which riff starts no new worker (01M3WFZ01PTAYYKG3T5CFA2W4D). |
//! | `lead.compact` | true | riff compacts the lead at the end of a wave (see [`compact`](crate::compact)). |
//! | `lead.quiet` | 60 | The seconds with no input in the pane of the lead before riff compacts it. |
//! | `connect.scope` | none | The answer to the scope question of `riff connect claude`: `repo`, `global` or `none` (see [`enable`](crate::enable), 01M3XY2SNXQJRSH5QX82AFVM2S). With no key, nobody answered yet. |
//! | `update.auto` | false | riff installs each new release of the riff by itself (see [`auto_update`](crate::auto_update)). `riff login` and `riff connect claude` ask a person once when the key is missing (see [`ask_update_auto`]). |

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use toml_edit::{DocumentMut, value};

/// The settings file: `$XDG_CONFIG_HOME/riff/config.toml`, or
/// `$HOME/.config/riff/config.toml`. `None` with neither variable.
///
/// ```
/// use riff::settings::path_from;
/// assert_eq!(path_from(Some("/x".into()), Some("/h".into())), Some("/x/riff/config.toml".into()));
/// assert_eq!(path_from(None, Some("/h".into())), Some("/h/.config/riff/config.toml".into()));
/// assert_eq!(path_from(Some("".into()), None), None);
/// ```
pub fn path_from(
    config: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let dir = match config.filter(|c| !c.is_empty()) {
        Some(config) => PathBuf::from(config),
        None => PathBuf::from(home.filter(|h| !h.is_empty())?).join(".config"),
    };
    Some(dir.join("riff").join("config.toml"))
}

/// The settings file of this process: `$RIFF_HOME/config.toml`, else
/// [`path_from`].
pub fn path() -> Result<PathBuf> {
    if let Some(home) = crate::home::dir() {
        return Ok(home.join("config.toml"));
    }
    path_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
    .context("cannot find the settings file: set XDG_CONFIG_HOME or HOME")
}

fn read(path: &Path) -> Result<DocumentMut> {
    match std::fs::read_to_string(path) {
        Ok(text) => text
            .parse()
            .with_context(|| format!("{} is not valid TOML", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// The most workers on this machine. 0 when the file or the key is
/// missing.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert_eq!(riff::settings::workers_limit(&path)?, 0);
/// riff::settings::set_workers_limit(&path, 2)?;
/// assert_eq!(riff::settings::workers_limit(&path)?, 2);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn workers_limit(path: &Path) -> Result<u16> {
    let doc = read(path)?;
    let Some(limit) = doc.get("workers").and_then(|w| w.get("limit")) else {
        return Ok(0);
    };
    let Some(limit) = limit.as_integer() else {
        bail!("workers.limit in {} is not a number", path.display());
    };
    u16::try_from(limit)
        .with_context(|| format!("workers.limit in {} is out of range", path.display()))
}

/// Sets the most workers on this machine. It keeps each other key.
pub fn set_workers_limit(path: &Path, limit: u16) -> Result<()> {
    set(path, "workers", "limit", value(i64::from(limit)))
}

/// The default of `workers.interval`, in seconds.
pub const WORKERS_INTERVAL: u16 = 10;

/// The seconds between two workers that the rollout starts:
/// `workers.interval`, or [`WORKERS_INTERVAL`] when the file or the key
/// is missing. 0 turns the rollout off (01M3Q5QE9H42FQKEDC5G9GKCWD).
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert_eq!(riff::settings::workers_interval(&path)?, 10);
/// riff::settings::set_workers_interval(&path, 30)?;
/// assert_eq!(riff::settings::workers_interval(&path)?, 30);
/// riff::settings::set_workers_interval(&path, 0)?;
/// assert_eq!(riff::settings::workers_interval(&path)?, 0);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn workers_interval(path: &Path) -> Result<u16> {
    let doc = read(path)?;
    let Some(interval) = doc.get("workers").and_then(|w| w.get("interval")) else {
        return Ok(WORKERS_INTERVAL);
    };
    let Some(interval) = interval.as_integer() else {
        bail!("workers.interval in {} is not a number", path.display());
    };
    u16::try_from(interval)
        .with_context(|| format!("workers.interval in {} is out of range", path.display()))
}

/// Sets `workers.interval`. It keeps each other key.
pub fn set_workers_interval(path: &Path, seconds: u16) -> Result<()> {
    set(path, "workers", "interval", value(i64::from(seconds)))
}

/// The default of `workers.nice`.
pub const WORKERS_NICE: u8 = 10;

/// The most that `workers.nice` can be: the lowest priority of Unix.
pub const NICE_MAX: u8 = 19;

/// The default of `workers.floor`, in GB.
pub const WORKERS_FLOOR: u32 = 4;

/// The compile jobs and the test threads of one worker: `workers.jobs`
/// (01M3WFYZRK5CT22GJW6ZHYT9CC). 0 when the file or the key is missing:
/// riff makes the number from the machine
/// ([`limits::jobs`](crate::limits::jobs)).
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert_eq!(riff::settings::workers_jobs(&path)?, 0);
/// riff::settings::set_workers_jobs(&path, 6)?;
/// assert_eq!(riff::settings::workers_jobs(&path)?, 6);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn workers_jobs(path: &Path) -> Result<u16> {
    number(path, "jobs", 0)
}

/// Sets `workers.jobs`. It keeps each other key.
pub fn set_workers_jobs(path: &Path, jobs: u16) -> Result<()> {
    set(path, "workers", "jobs", value(i64::from(jobs)))
}

/// The nice value of each worker: `workers.nice`
/// (01M3WFYZTX05CGDP2NQF9B356K). [`WORKERS_NICE`] when the file or the
/// key is missing. A value of more than [`NICE_MAX`] is an error.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert_eq!(riff::settings::workers_nice(&path)?, 10);
/// riff::settings::set_workers_nice(&path, 0)?;
/// assert_eq!(riff::settings::workers_nice(&path)?, 0);
/// assert!(riff::settings::set_workers_nice(&path, 20).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn workers_nice(path: &Path) -> Result<u8> {
    let nice: u8 = number(path, "nice", WORKERS_NICE)?;
    if nice > NICE_MAX {
        bail!("workers.nice in {} is more than {NICE_MAX}", path.display());
    }
    Ok(nice)
}

/// Sets `workers.nice`. It keeps each other key.
pub fn set_workers_nice(path: &Path, nice: u8) -> Result<()> {
    if nice > NICE_MAX {
        bail!("the nice value is 0 to {NICE_MAX}");
    }
    set(path, "workers", "nice", value(i64::from(nice)))
}

/// The most memory of all workers of the machine, in GB:
/// `workers.memory` (01M3WFYZX6GVFYW6NTTTKF144R). 0 when the file or the
/// key is missing: riff makes the number from the machine
/// ([`limits::memory`](crate::limits::memory)).
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert_eq!(riff::settings::workers_memory(&path)?, 0);
/// riff::settings::set_workers_memory(&path, 20)?;
/// assert_eq!(riff::settings::workers_memory(&path)?, 20);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn workers_memory(path: &Path) -> Result<u32> {
    number(path, "memory", 0)
}

/// Sets `workers.memory`. It keeps each other key.
pub fn set_workers_memory(path: &Path, gb: u32) -> Result<()> {
    set(path, "workers", "memory", value(i64::from(gb)))
}

/// The available memory in GB under which riff starts no new worker:
/// `workers.floor` (01M3WFZ01PTAYYKG3T5CFA2W4D). [`WORKERS_FLOOR`] when
/// the file or the key is missing. 0 turns the floor off.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert_eq!(riff::settings::workers_floor(&path)?, 4);
/// riff::settings::set_workers_floor(&path, 8)?;
/// assert_eq!(riff::settings::workers_floor(&path)?, 8);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn workers_floor(path: &Path) -> Result<u32> {
    number(path, "floor", WORKERS_FLOOR)
}

/// Sets `workers.floor`. It keeps each other key.
pub fn set_workers_floor(path: &Path, gb: u32) -> Result<()> {
    set(path, "workers", "floor", value(i64::from(gb)))
}

/// The number `workers.KEY`, or `default` when the file or the key is
/// missing.
fn number<T: TryFrom<i64>>(path: &Path, key: &str, default: T) -> Result<T> {
    let doc = read(path)?;
    let Some(item) = doc.get("workers").and_then(|w| w.get(key)) else {
        return Ok(default);
    };
    let Some(n) = item.as_integer() else {
        bail!("workers.{key} in {} is not a number", path.display());
    };
    T::try_from(n)
        .ok()
        .with_context(|| format!("workers.{key} in {} is out of range", path.display()))
}

/// The MCP servers that a worker loads: `workers.mcp`
/// (01M3NB5R6X5AV79DQNKKJBH5J8). `riff` is always the first, also when
/// the file leaves it out. `["riff"]` when the file or the key is
/// missing.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert_eq!(riff::settings::workers_mcp(&path)?, ["riff"]);
/// riff::settings::set_workers_mcp(&path, &["riff".into(), "github".into()])?;
/// assert_eq!(riff::settings::workers_mcp(&path)?, ["riff", "github"]);
/// std::fs::write(&path, "[workers]\nmcp = [\"github\", \"riff\"]\n")?;
/// assert_eq!(riff::settings::workers_mcp(&path)?, ["riff", "github"]);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn workers_mcp(path: &Path) -> Result<Vec<String>> {
    let doc = read(path)?;
    let mut names = vec![RIFF_MCP.to_owned()];
    let Some(mcp) = doc.get("workers").and_then(|w| w.get("mcp")) else {
        return Ok(names);
    };
    let Some(list) = mcp.as_array() else {
        bail!("workers.mcp in {} is not a list", path.display());
    };
    for name in list {
        let Some(name) = name.as_str() else {
            bail!(
                "workers.mcp in {} holds a value that is not a name",
                path.display()
            );
        };
        if !names.iter().any(|n| n == name) {
            names.push(name.to_owned());
        }
    }
    Ok(names)
}

/// Sets `workers.mcp`. It keeps each other key.
pub fn set_workers_mcp(path: &Path, names: &[String]) -> Result<()> {
    let list: toml_edit::Array = names.iter().map(String::as_str).collect();
    set(path, "workers", "mcp", value(list))
}

/// The MCP server of riff. Each worker loads it.
pub const RIFF_MCP: &str = "riff";

/// True when riff updates itself on this machine: `update.auto`. False
/// when the file or the key is missing.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert!(!riff::settings::update_auto(&path)?);
/// riff::settings::set_update_auto(&path, true)?;
/// assert!(riff::settings::update_auto(&path)?);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn update_auto(path: &Path) -> Result<bool> {
    let doc = read(path)?;
    let Some(auto) = doc.get("update").and_then(|u| u.get("auto")) else {
        return Ok(false);
    };
    auto.as_bool()
        .with_context(|| format!("update.auto in {} is not true or false", path.display()))
}

/// Sets `update.auto`. It keeps each other key.
pub fn set_update_auto(path: &Path, auto: bool) -> Result<()> {
    set(path, "update", "auto", value(auto))
}

/// True when riff compacts the lead at the end of a wave: `lead.compact`
/// (01M3Q88GBSRJRP4VGVDV3EJZ4R). True when the file or the key is
/// missing.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert!(riff::settings::lead_compact(&path)?);
/// riff::settings::set_lead_compact(&path, false)?;
/// assert!(!riff::settings::lead_compact(&path)?);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn lead_compact(path: &Path) -> Result<bool> {
    let doc = read(path)?;
    let Some(compact) = doc.get("lead").and_then(|l| l.get("compact")) else {
        return Ok(true);
    };
    compact
        .as_bool()
        .with_context(|| format!("lead.compact in {} is not true or false", path.display()))
}

/// Sets `lead.compact`. It keeps each other key.
pub fn set_lead_compact(path: &Path, compact: bool) -> Result<()> {
    set(path, "lead", "compact", value(compact))
}

/// The quiet time before riff compacts the lead: `lead.quiet`, in
/// seconds (01M3Q88GBSRJRP4VGVDV3EJZ4R). [`LEAD_QUIET`] when the file or
/// the key is missing.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert_eq!(riff::settings::lead_quiet(&path)?, 60);
/// riff::settings::set_lead_quiet(&path, 90)?;
/// assert_eq!(riff::settings::lead_quiet(&path)?, 90);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn lead_quiet(path: &Path) -> Result<u64> {
    let doc = read(path)?;
    let Some(quiet) = doc.get("lead").and_then(|l| l.get("quiet")) else {
        return Ok(LEAD_QUIET);
    };
    let Some(quiet) = quiet.as_integer() else {
        bail!("lead.quiet in {} is not a number", path.display());
    };
    u64::try_from(quiet)
        .with_context(|| format!("lead.quiet in {} is out of range", path.display()))
}

/// Sets `lead.quiet`. It keeps each other key.
pub fn set_lead_quiet(path: &Path, secs: u64) -> Result<()> {
    let secs = i64::try_from(secs).context("the quiet time is too long")?;
    set(path, "lead", "quiet", value(secs))
}

/// The default quiet time, in seconds.
pub const LEAD_QUIET: u64 = 60;

/// The answer of the person to the scope question of `riff connect
/// claude`: `connect.scope` (01M3XY2SNXQJRSH5QX82AFVM2S). `None` when
/// the file or the key is missing: nobody answered yet.
///
/// ```
/// use riff::enable::Scope;
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert_eq!(riff::settings::connect_scope(&path)?, None);
/// riff::settings::set_connect_scope(&path, Scope::Global)?;
/// assert_eq!(riff::settings::connect_scope(&path)?, Some(Scope::Global));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn connect_scope(path: &Path) -> Result<Option<crate::enable::Scope>> {
    let doc = read(path)?;
    let Some(scope) = doc.get("connect").and_then(|c| c.get("scope")) else {
        return Ok(None);
    };
    scope
        .as_str()
        .and_then(crate::enable::Scope::parse)
        .map(Some)
        .with_context(|| {
            format!(
                "connect.scope in {} is not repo, global or none",
                path.display()
            )
        })
}

/// Sets `connect.scope`. It keeps each other key.
pub fn set_connect_scope(path: &Path, scope: crate::enable::Scope) -> Result<()> {
    set(path, "connect", "scope", value(scope.as_str()))
}

/// The question about `update.auto` on a new machine.
pub const ASK_UPDATE_AUTO: &str = "Update riff by itself when the riff gets a new release? [Y/n] ";

/// Asks the person once about `update.auto`, when the machine has no
/// such key and `terminal` is true (01M3NT6WV8Q8EFZBK8DHYKW5CC). It
/// writes [`ASK_UPDATE_AUTO`] to `out`, reads one line of `input`, and
/// sets the key: `n` or `no` is off, each other answer is on. It returns
/// the new value, or `None` when it did not ask, or the input ended.
///
/// ```
/// use riff::settings::{ask_update_auto, update_auto};
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// let mut out = Vec::new();
/// // No terminal: no question.
/// assert_eq!(ask_update_auto(&path, false, &mut &b"n\n"[..], &mut out)?, None);
/// assert!(out.is_empty());
/// // The first time in a terminal, it asks. Enter says yes.
/// assert_eq!(ask_update_auto(&path, true, &mut &b"\n"[..], &mut out)?, Some(true));
/// assert!(update_auto(&path)?);
/// // The key is there: no second question.
/// assert_eq!(ask_update_auto(&path, true, &mut &b"n\n"[..], &mut out)?, None);
/// assert_eq!(String::from_utf8(out)?, riff::settings::ASK_UPDATE_AUTO);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn ask_update_auto(
    path: &Path,
    terminal: bool,
    input: &mut impl std::io::BufRead,
    out: &mut impl std::io::Write,
) -> Result<Option<bool>> {
    let doc = read(path)?;
    if !terminal || doc.get("update").and_then(|u| u.get("auto")).is_some() {
        return Ok(None);
    }
    write!(out, "{ASK_UPDATE_AUTO}")?;
    out.flush()?;
    let mut answer = String::new();
    if input.read_line(&mut answer)? == 0 {
        return Ok(None);
    }
    let auto = !matches!(answer.trim().to_lowercase().as_str(), "n" | "no");
    set_update_auto(path, auto)?;
    Ok(Some(auto))
}

/// Sets `key` in the table `table`, and writes the file. It keeps each
/// other key and each comment.
fn set(path: &Path, table: &str, key: &str, item: toml_edit::Item) -> Result<()> {
    let mut doc = read(path)?;
    if !doc.contains_key(table) {
        doc[table] = toml_edit::table();
    }
    let Some(t) = doc[table].as_table_mut() else {
        bail!("{table} in {} is not a table", path.display());
    };
    t[key] = item;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    }
    std::fs::write(path, doc.to_string())
        .with_context(|| format!("cannot write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_keeps_the_other_keys_and_comments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("riff/config.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# mine\nserver = \"x\"\n\n[workers]\nlimit = 1\n").unwrap();
        set_workers_limit(&path, 3).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# mine"), "{text}");
        assert!(text.contains("server = \"x\""), "{text}");
        assert_eq!(workers_limit(&path).unwrap(), 3);
    }

    #[test]
    fn a_new_file_gets_its_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/config.toml");
        set_workers_limit(&path, 2).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[workers]\nlimit = 2\n"
        );
    }

    #[test]
    fn a_bad_limit_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[workers]\nlimit = \"two\"\n").unwrap();
        assert!(workers_limit(&path).is_err());
        std::fs::write(&path, "[workers]\nlimit = -1\n").unwrap();
        assert!(workers_limit(&path).is_err());
        std::fs::write(&path, "workers = 3\n").unwrap();
        assert!(set_workers_limit(&path, 1).is_err());
    }

    #[test]
    fn update_auto_keeps_the_workers_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        set_workers_limit(&path, 2).unwrap();
        set_update_auto(&path, true).unwrap();
        set_update_auto(&path, false).unwrap();
        assert_eq!(workers_limit(&path).unwrap(), 2);
        assert!(!update_auto(&path).unwrap());
    }

    #[test]
    fn workers_mcp_keeps_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        set_workers_limit(&path, 2).unwrap();
        set_workers_mcp(&path, &["riff".into(), "unifi".into()]).unwrap();
        assert_eq!(workers_limit(&path).unwrap(), 2);
        assert_eq!(workers_mcp(&path).unwrap(), ["riff", "unifi"]);
    }

    #[test]
    fn a_bad_workers_mcp_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[workers]\nmcp = \"riff\"\n").unwrap();
        assert!(workers_mcp(&path).is_err());
        std::fs::write(&path, "[workers]\nmcp = [1]\n").unwrap();
        assert!(workers_mcp(&path).is_err());
    }

    #[test]
    fn a_bad_number_of_the_limits_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[workers]\njobs = \"four\"\nnice = 20\nmemory = -1\nfloor = 1.5\n",
        )
        .unwrap();
        assert!(workers_jobs(&path).is_err());
        assert!(workers_nice(&path).is_err());
        assert!(workers_memory(&path).is_err());
        assert!(workers_floor(&path).is_err());
    }

    #[test]
    fn the_limits_keep_the_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        set_workers_limit(&path, 3).unwrap();
        set_workers_jobs(&path, 5).unwrap();
        set_workers_nice(&path, 5).unwrap();
        set_workers_memory(&path, 20).unwrap();
        set_workers_floor(&path, 6).unwrap();
        assert_eq!(workers_limit(&path).unwrap(), 3);
        assert_eq!(workers_jobs(&path).unwrap(), 5);
        assert_eq!(workers_nice(&path).unwrap(), 5);
        assert_eq!(workers_memory(&path).unwrap(), 20);
        assert_eq!(workers_floor(&path).unwrap(), 6);
    }

    #[test]
    fn a_bad_update_auto_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[update]\nauto = \"yes\"\n").unwrap();
        assert!(update_auto(&path).is_err());
        std::fs::write(&path, "update = 1\n").unwrap();
        assert!(set_update_auto(&path, true).is_err());
    }
}
