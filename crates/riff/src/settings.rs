//! The settings of riff on this machine.
//!
//! They are in `$XDG_CONFIG_HOME/riff/config.toml`, or
//! `~/.config/riff/config.toml` without `XDG_CONFIG_HOME`
//! (01M3JPQT13ANVA7DNJDVNJ0S8P). A change keeps each other key and each
//! comment of the file.
//!
//! ```toml
//! [workers]
//! limit = 2
//! ```
//!
//! | Key | Default | Meaning |
//! |---|---|---|
//! | `workers.limit` | 0 | The most workers that `riff workers start` runs on this machine (01M3JPQT35BMR7XMAMMFSCDC2B). |

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

/// The settings file of this process.
pub fn path() -> Result<PathBuf> {
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
    let mut doc = read(path)?;
    if !doc.contains_key("workers") {
        doc["workers"] = toml_edit::table();
    }
    let Some(workers) = doc["workers"].as_table_mut() else {
        bail!("workers in {} is not a table", path.display());
    };
    workers["limit"] = value(i64::from(limit));
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
}
