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
//! tmp = "/data/riff-tmp"
//! cache = "40G"
//!
//! [update]
//! auto = true
//!
//! [watch]
//! limit = 6000
//!
//! [connect]
//! scope = "repo"
//! ```
//!
//! | Key | Default | Meaning |
//! |---|---|---|
//! | `workers.tmp` | `~/.cache/riff/tmp` | The folder of the temp folders of the workers (see [`temp`](crate::temp), 01M41VAGJC69S9R2TD1B1EQ4W4). |
//! | `workers.cache` | `40G` | The most size of the compile cache of the workers (see [`sccache`](crate::sccache), 01M49237BM12PVBERD6JXDSX5V). |
//! | `lead.compact` | true | riff compacts the lead at the end of a wave (see [`compact`](crate::compact)). |
//! | `lead.quiet` | 60 | The seconds with no input in the pane of the lead before riff compacts it. |
//! | `lead.wake` | 15 | The minutes that a block waits for an answer before riff wakes the lead again, and again before riff tells the person (see [`crate::look`], 01M41FZQ545HQ9Q75CSKX8HF8H). |
//! | `lead.notify` | true | riff shows a block with no answer of the lead in a desktop notification on the machine of the lead (01M41FZQKZKW131Z8822G31T5G). |
//! | `watch.limit` | 6000 | The longest time in seconds that `riff watch --once` waits for a wake. 0: no limit (01M3Z64J08GW6N1H42AR2FZQZ4). |
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

/// The root of the temp folders of the workers: `workers.tmp`
/// (01M41VAGJC69S9R2TD1B1EQ4W4). `None` when the file or the key is
/// missing: then [`crate::temp::root`] takes the default.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert_eq!(riff::settings::workers_tmp(&path)?, None);
/// std::fs::write(&path, "[workers]\ntmp = \"/data/riff-tmp\"\n")?;
/// assert_eq!(riff::settings::workers_tmp(&path)?, Some("/data/riff-tmp".into()));
/// std::fs::write(&path, "[workers]\ntmp = 3\n")?;
/// assert!(riff::settings::workers_tmp(&path).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn workers_tmp(path: &Path) -> Result<Option<PathBuf>> {
    let doc = read(path)?;
    let Some(item) = doc.get("workers").and_then(|w| w.get("tmp")) else {
        return Ok(None);
    };
    match item.as_str() {
        Some(dir) if !dir.is_empty() => Ok(Some(PathBuf::from(dir))),
        _ => bail!("workers.tmp in {} is not a path", path.display()),
    }
}

/// The most size of the compile cache of the workers: `workers.cache`
/// (01M49237BM12PVBERD6JXDSX5V). [`crate::sccache::SIZE`] when the
/// file or the key is missing.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert_eq!(riff::settings::workers_cache(&path)?, "40G");
/// std::fs::write(&path, "[workers]\ncache = \"20G\"\n")?;
/// assert_eq!(riff::settings::workers_cache(&path)?, "20G");
/// std::fs::write(&path, "[workers]\ncache = 3\n")?;
/// assert!(riff::settings::workers_cache(&path).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn workers_cache(path: &Path) -> Result<String> {
    let doc = read(path)?;
    let Some(item) = doc.get("workers").and_then(|w| w.get("cache")) else {
        return Ok(crate::sccache::SIZE.to_owned());
    };
    match item.as_str() {
        Some(size) if crate::sccache::valid_size(size) => Ok(size.to_owned()),
        _ => bail!(
            "workers.cache in {} is not a size such as 40G",
            path.display()
        ),
    }
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

/// The minutes that a block waits for an answer before riff wakes the
/// lead again: `lead.wake` (01M41FZQ545HQ9Q75CSKX8HF8H). [`LEAD_WAKE`]
/// when the file or the key is missing. 0 is 1 minute.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert_eq!(riff::settings::lead_wake(&path)?, 15);
/// riff::settings::set_lead_wake(&path, 30)?;
/// assert_eq!(riff::settings::lead_wake(&path)?, 30);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn lead_wake(path: &Path) -> Result<u64> {
    let doc = read(path)?;
    let Some(wake) = doc.get("lead").and_then(|l| l.get("wake")) else {
        return Ok(LEAD_WAKE);
    };
    let Some(wake) = wake.as_integer() else {
        bail!("lead.wake in {} is not a number", path.display());
    };
    let wake = u64::try_from(wake)
        .with_context(|| format!("lead.wake in {} is out of range", path.display()))?;
    Ok(wake.max(1))
}

/// Sets `lead.wake`. It keeps each other key.
pub fn set_lead_wake(path: &Path, minutes: u64) -> Result<()> {
    let minutes = i64::try_from(minutes).context("the wake time is too long")?;
    set(path, "lead", "wake", value(minutes))
}

/// The default of `lead.wake`, in minutes.
pub const LEAD_WAKE: u64 = 15;

/// True when riff shows a block with no answer of the lead in a desktop
/// notification on this machine: `lead.notify`
/// (01M41FZQKZKW131Z8822G31T5G). True when the file or the key is
/// missing.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert!(riff::settings::lead_notify(&path)?);
/// riff::settings::set_lead_notify(&path, false)?;
/// assert!(!riff::settings::lead_notify(&path)?);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn lead_notify(path: &Path) -> Result<bool> {
    let doc = read(path)?;
    let Some(notify) = doc.get("lead").and_then(|l| l.get("notify")) else {
        return Ok(true);
    };
    notify
        .as_bool()
        .with_context(|| format!("lead.notify in {} is not true or false", path.display()))
}

/// Sets `lead.notify`. It keeps each other key.
pub fn set_lead_notify(path: &Path, notify: bool) -> Result<()> {
    set(path, "lead", "notify", value(notify))
}

/// The default of `watch.limit`, in seconds: 100 minutes
/// (01M3Z64J08GW6N1H42AR2FZQZ4). Claude Code stops a background task
/// after 2 hours at most.
pub const WATCH_LIMIT: u64 = 6000;

/// The longest time that `riff watch --once` waits for a wake:
/// `watch.limit`, in seconds (01M3Z64J08GW6N1H42AR2FZQZ4).
/// [`WATCH_LIMIT`] when the file or the key is missing. With 0, the
/// watch waits with no limit.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.toml");
/// assert_eq!(riff::settings::watch_limit(&path)?, 6000);
/// riff::settings::set_watch_limit(&path, 1500)?;
/// assert_eq!(riff::settings::watch_limit(&path)?, 1500);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn watch_limit(path: &Path) -> Result<u64> {
    let doc = read(path)?;
    let Some(limit) = doc.get("watch").and_then(|w| w.get("limit")) else {
        return Ok(WATCH_LIMIT);
    };
    let Some(limit) = limit.as_integer() else {
        bail!("watch.limit in {} is not a number", path.display());
    };
    u64::try_from(limit)
        .with_context(|| format!("watch.limit in {} is out of range", path.display()))
}

/// Sets `watch.limit`. It keeps each other key.
pub fn set_watch_limit(path: &Path, secs: u64) -> Result<()> {
    let secs = i64::try_from(secs).context("the limit is too long")?;
    set(path, "watch", "limit", value(secs))
}

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
/// other key and each comment. It writes a new file and renames it, so
/// a process that reads the settings at the same time sees the old
/// file or the new file, never a part (01M41FMA3FGF3TKTXPBND9NDW1).
fn set(path: &Path, table: &str, key: &str, item: toml_edit::Item) -> Result<()> {
    let mut doc = read(path)?;
    if !doc.contains_key(table) {
        doc[table] = toml_edit::table();
    }
    let Some(t) = doc[table].as_table_mut() else {
        bail!("{table} in {} is not a table", path.display());
    };
    t[key] = item;
    // A settings file that is a symbolic link stays a link: riff writes
    // the file that it names.
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let fail = || format!("cannot write {}", path.display());
    let mut new = tempfile::NamedTempFile::new_in(dir).with_context(fail)?;
    std::io::Write::write_all(&mut new, doc.to_string().as_bytes()).with_context(fail)?;
    new.persist(&path).map_err(|e| e.error).with_context(fail)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_keeps_the_other_keys_and_comments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("riff/config.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# mine\nserver = \"x\"\n\n[watch]\nlimit = 1\n").unwrap();
        set_watch_limit(&path, 3).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# mine"), "{text}");
        assert!(text.contains("server = \"x\""), "{text}");
        assert_eq!(watch_limit(&path).unwrap(), 3);
    }

    /// A process that reads the settings while riff writes them reads
    /// the old file or the new file, never an empty or a part of a file
    /// (01M41FMA3FGF3TKTXPBND9NDW1).
    #[test]
    fn a_reader_never_sees_a_part_of_a_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        set_watch_limit(&path, 1).unwrap();
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader = {
            let (path, done) = (path.clone(), done.clone());
            std::thread::spawn(move || {
                let mut reads = 0;
                while !done.load(std::sync::atomic::Ordering::SeqCst) {
                    let limit = watch_limit(&path).unwrap();
                    assert!((1..=2).contains(&limit), "read the limit {limit}");
                    reads += 1;
                }
                reads
            })
        };
        for i in 0..500 {
            set_watch_limit(&path, 1 + i % 2).unwrap();
        }
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(reader.join().unwrap() > 0);
    }

    /// A settings file that is a symbolic link stays a link, and riff
    /// writes the file that it names.
    #[test]
    fn a_linked_file_stays_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("dotfiles/riff.toml");
        std::fs::create_dir_all(real.parent().unwrap()).unwrap();
        std::fs::write(&real, "[watch]\nlimit = 1\n").unwrap();
        let link = dir.path().join("config.toml");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        set_watch_limit(&link, 3).unwrap();
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(watch_limit(&real).unwrap(), 3);
    }

    #[test]
    fn a_new_file_gets_its_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/config.toml");
        set_watch_limit(&path, 2).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[watch]\nlimit = 2\n"
        );
    }

    #[test]
    fn a_bad_limit_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[watch]\nlimit = \"two\"\n").unwrap();
        assert!(watch_limit(&path).is_err());
        std::fs::write(&path, "[watch]\nlimit = -1\n").unwrap();
        assert!(watch_limit(&path).is_err());
        std::fs::write(&path, "watch = 3\n").unwrap();
        assert!(set_watch_limit(&path, 1).is_err());
    }

    #[test]
    fn update_auto_keeps_the_watch_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        set_watch_limit(&path, 2).unwrap();
        set_update_auto(&path, true).unwrap();
        set_update_auto(&path, false).unwrap();
        assert_eq!(watch_limit(&path).unwrap(), 2);
        assert!(!update_auto(&path).unwrap());
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
