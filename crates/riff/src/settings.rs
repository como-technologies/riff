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
//! mcp = ["riff", "github"]
//!
//! [update]
//! auto = true
//! ```
//!
//! | Key | Default | Meaning |
//! |---|---|---|
//! | `workers.limit` | 0 | The most workers that `riff workers start` runs on this machine (01M3JPQT35BMR7XMAMMFSCDC2B). |
//! | `workers.mcp` | `["riff"]` | The MCP servers that a worker loads (see [`worker_mcp`](crate::worker_mcp)). |
//! | `update.auto` | false | riff installs each new release of the riff by itself (see [`auto_update`](crate::auto_update)). |

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
    fn a_bad_update_auto_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[update]\nauto = \"yes\"\n").unwrap();
        assert!(update_auto(&path).is_err());
        std::fs::write(&path, "update = 1\n").unwrap();
        assert!(set_update_auto(&path, true).is_err());
    }
}
