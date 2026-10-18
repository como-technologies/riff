//! The compile cache of the container sessions of a host: the volume
//! [`SCCACHE_VOLUME`] (#1069).
//!
//! # Design
//!
//! Each session runs its own `sccache` server in its container, and each
//! server reads and writes the one volume [`SCCACHE_VOLUME`] of the host
//! (01M4Z60B7RPSQ1ABGVMJSZWS5P). `riff daemon cache` is the one place to
//! look at it:
//!
//! | Command | Does |
//! |---|---|
//! | `riff daemon cache` | shows the size of the volume now, and the most size |
//! | `riff daemon cache SIZE` | sets the most size in [`CACHE_FILE`] (01M57818168N3AQGE3G5GDYDDD) |
//! | `riff daemon cache --clear` | deletes each file in the volume ([`clear`], 01M5781A4PRTTYRHH5GJ1GHN7G) |
//!
//! Each of them also takes `--host HOST`: the daemon of that host does
//! it ([`change`], [`crate::daemon::remote`]).
//!
//! `riff daemon status` shows the same line ([`crate::text::daemon_cache`],
//! 01M5781930TF6XBX7ME7RAMX2X).
//!
//! The daemon reads [`CACHE_FILE`] at each start of a session, and puts
//! the size in `SCCACHE_CACHE_SIZE` of the spec ([`Setting::apply`]). So
//! a session that runs keeps the size of its start.
//!
//! The clear keeps the volume and its top folder: each session that runs
//! has it as a bind. A session has `SCCACHE_IGNORE_SERVER_IO_ERROR=1`.
//! For `sccache`, a file that the clear deleted is a miss: the build
//! compiles, and writes the file again.
//!
//! In rootless podman, a folder of the volume can belong to a user of
//! the container, and the person cannot read it. Then [`measure`] and
//! [`clear`] use `podman unshare`, as the root of the user namespace
//! (01M588JW2KS6EVX1SE49686FQ7).
//!
//! ```mermaid
//! flowchart LR
//!     S["riff daemon cache SIZE"] --> F["cache.json in the folder of the daemon"]
//!     F --> D["the daemon starts a session:<br/>SCCACHE_CACHE_SIZE=SIZE"]
//!     C["riff daemon cache --clear"] --> P["podman volume inspect riff-sccache"]
//!     P --> R["delete each file in its folder,<br/>keep the folder"]
//!     R -- "permission denied" --> U["podman unshare find ... rm -rf"]
//!     R --> B["a build that runs: a miss, it compiles"]
//! ```

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub use crate::run::SCCACHE_VOLUME;
use crate::runtime::{Env, SessionSpec};

/// The file of the setting, in the folder of the daemon.
pub const CACHE_FILE: &str = "cache.json";

/// The variable of the most size of the cache of `sccache`.
pub const SIZE_VAR: &str = "SCCACHE_CACHE_SIZE";

/// The setting of the cache of the host: its most size, for example
/// `40G`. With no file, the size is [`crate::sccache::SIZE`].
///
/// ```
/// use riff::compile_cache::Setting;
///
/// assert_eq!(Setting::default().size, "40G");
/// let dir = tempfile::tempdir()?;
/// assert_eq!(Setting::read(dir.path())?, Setting::default());
/// Setting { size: "20G".into() }.write(dir.path())?;
/// assert_eq!(Setting::read(dir.path())?.size, "20G");
/// std::fs::write(dir.path().join("cache.json"), r#"{"size":"20GB"}"#)?;
/// assert!(Setting::read(dir.path()).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Setting {
    /// The most size: a number with K, M, G or T.
    pub size: String,
}

impl Default for Setting {
    fn default() -> Self {
        Setting {
            size: crate::sccache::SIZE.to_owned(),
        }
    }
}

impl Setting {
    /// The setting in the folder `dir` of the daemon: its [`CACHE_FILE`],
    /// or [`Setting::default`] when the file is not there. A file that
    /// riff cannot read, or a size that is not a size, is an error.
    pub fn read(dir: &Path) -> Result<Setting> {
        let path = dir.join(CACHE_FILE);
        let setting: Setting = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("cannot read the cache setting in {}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Setting::default()),
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        };
        if !crate::sccache::valid_size(&setting.size) {
            bail!("the size in {} is not a size such as 40G", path.display());
        }
        Ok(setting)
    }

    /// Writes the setting to [`CACHE_FILE`] in the folder `dir`, in
    /// place of the old one.
    pub fn write(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
        let path = dir.join(CACHE_FILE);
        let tmp = path.with_extension("json.new");
        std::fs::write(&tmp, serde_json::to_vec(self)?)
            .with_context(|| format!("cannot write {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))
    }

    /// Puts the size in `SCCACHE_CACHE_SIZE` of `spec`, in place of an
    /// old value (01M57818168N3AQGE3G5GDYDDD).
    ///
    /// ```
    /// use riff::compile_cache::Setting;
    /// use riff::runtime::{Env, SessionSpec};
    ///
    /// let mut spec = SessionSpec {
    ///     name: "s".into(),
    ///     image: "i".into(),
    ///     home: "/h".into(),
    ///     workdir: "/w".into(),
    ///     binds: vec![],
    ///     env: vec![Env::Value("SCCACHE_CACHE_SIZE".into(), "40G".into())],
    ///     memory: "8g".into(),
    ///     cpus: 8,
    ///     command: vec![],
    /// };
    /// Setting { size: "20G".into() }.apply(&mut spec);
    /// assert_eq!(spec.env, [Env::Value("SCCACHE_CACHE_SIZE".into(), "20G".into())]);
    /// ```
    pub fn apply(&self, spec: &mut SessionSpec) {
        spec.env
            .retain(|e| !matches!(e, Env::Value(name, _) if name == SIZE_VAR));
        spec.env
            .push(Env::Value(SIZE_VAR.into(), self.size.clone()));
    }
}

/// `riff daemon cache SIZE`: sets the most size in the folder `dir` of
/// the daemon, and gives the setting. With no size, it changes nothing.
///
/// ```
/// use riff::compile_cache::{Setting, set};
///
/// let dir = tempfile::tempdir()?;
/// assert_eq!(set(dir.path(), None)?, Setting::default());
/// assert!(!dir.path().join("cache.json").exists());
/// assert_eq!(set(dir.path(), Some("12G"))?.size, "12G");
/// assert!(set(dir.path(), Some("12GB")).is_err());
/// assert_eq!(Setting::read(dir.path())?.size, "12G");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn set(dir: &Path, size: Option<&str>) -> Result<Setting> {
    let Some(size) = size else {
        return Setting::read(dir);
    };
    if !crate::sccache::valid_size(size) {
        bail!("the size of the cache is a number with K, M, G or T, for example 40G");
    }
    let setting = Setting {
        size: size.to_owned(),
    };
    setting.write(dir)?;
    Ok(setting)
}

/// `riff daemon cache` with its argument, on this host or for the
/// daemon of a host with `--host`: empties the volume with `clear`, or
/// sets the most `size` in the folder `dir` of the daemon. It gives the
/// text to show: the line of [`shown`] last.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let shown = riff::compile_cache::change(dir.path(), Some("12G"), false)?;
/// assert!(shown.contains("at most 12G"));
/// assert!(riff::compile_cache::change(dir.path(), Some("12GB"), false).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn change(dir: &Path, size: Option<&str>, clear: bool) -> Result<String> {
    let mut lines = Vec::new();
    if clear {
        if let Some(folder) = folder() {
            self::clear(&folder)?;
            lines.push(crate::text::DAEMON_CACHE_CLEARED.to_owned());
        }
    } else {
        set(dir, size)?;
    }
    lines.push(shown(dir));
    Ok(lines.join("\n"))
}

/// The folder of the volume [`SCCACHE_VOLUME`] on the host, or `None`
/// when podman does not have the volume: no session made it yet.
pub fn folder() -> Option<PathBuf> {
    let out = Command::new("podman")
        .args([
            "volume",
            "inspect",
            "--format",
            "{{.Mountpoint}}",
            SCCACHE_VOLUME,
        ])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    folder_of(&String::from_utf8_lossy(&out.stdout))
}

/// The folder in the output of `podman volume inspect --format
/// {{.Mountpoint}}`.
///
/// ```
/// use riff::compile_cache::folder_of;
/// use std::path::PathBuf;
///
/// assert_eq!(
///     folder_of("/h/.local/share/containers/storage/volumes/riff-sccache/_data\n"),
///     Some(PathBuf::from("/h/.local/share/containers/storage/volumes/riff-sccache/_data"))
/// );
/// assert_eq!(folder_of("\n"), None);
/// assert_eq!(folder_of("not/absolute"), None);
/// ```
pub fn folder_of(out: &str) -> Option<PathBuf> {
    let path = PathBuf::from(out.lines().next()?.trim());
    path.is_absolute().then_some(path)
}

/// The size of the files in the folder `dir` now, in bytes: the size
/// that `podman system df -v` shows for the volume
/// (01M588JW2KS6EVX1SE49686FQ7). It reads the folder as the person
/// first. In rootless podman, a folder of the volume can belong to a
/// user of the container: then the person cannot read it, and riff
/// reads it with `podman unshare du`, as the root of the user
/// namespace. It is an error when riff cannot read it either way: riff
/// never shows 0 for a folder that it cannot read.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// std::fs::create_dir(dir.path().join("a"))?;
/// std::fs::write(dir.path().join("a/f"), vec![1u8; 10_000])?;
/// assert!(riff::compile_cache::measure(dir.path())? >= 10_000);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn measure(dir: &Path) -> Result<u64> {
    match used(dir) {
        Ok(bytes) => Ok(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let du = [OsStr::new("du"), OsStr::new("-s"), OsStr::new("-B1")];
            let out = unshare(&[&du[..], &[dir.as_os_str()]].concat())
                .with_context(|| format!("cannot read {}: {e}", dir.display()))?;
            du_bytes(&out)
                .with_context(|| format!("cannot read the output of du for {}", dir.display()))
        }
        Err(e) => Err(e).with_context(|| format!("cannot read {}", dir.display())),
    }
}

/// The size in bytes in the output of `du -s -B1 DIR`.
///
/// ```
/// use riff::compile_cache::du_bytes;
///
/// assert_eq!(du_bytes("7187000000\t/v/_data\n"), Some(7_187_000_000));
/// assert_eq!(du_bytes("du: cannot read\n"), None);
/// ```
pub fn du_bytes(out: &str) -> Option<u64> {
    out.split_whitespace().next()?.parse().ok()
}

/// The size of the files in the folder `dir` now, in bytes, as the
/// person reads it. An error when the person cannot read a folder in
/// it. A file that a build deletes meanwhile is no matter.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// std::fs::create_dir(dir.path().join("a"))?;
/// std::fs::write(dir.path().join("a/f"), vec![1u8; 10_000])?;
/// assert!(riff::compile_cache::used(dir.path())? >= 10_000);
/// assert!(riff::compile_cache::used(&dir.path().join("none")).is_err());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn used(dir: &Path) -> std::io::Result<u64> {
    std::fs::read_dir(dir)?.try_fold(0, |sum, entry| match entry {
        Ok(entry) => Ok(sum + size(&entry.path())?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(sum),
        Err(e) => Err(e),
    })
}

/// The size of `path` on the disk, with each file in it.
fn size(path: &Path) -> std::io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    let meta = match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        meta => meta?,
    };
    let own = meta.blocks() * 512;
    if !meta.is_dir() {
        return Ok(own);
    }
    match used(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(own),
        inner => Ok(own + inner?),
    }
}

/// Runs `podman unshare` with `args`: the command runs as the root of
/// the user namespace of rootless podman. It gives the output, or an
/// error with the words of podman.
fn unshare(args: &[&OsStr]) -> Result<String> {
    let out = Command::new("podman")
        .arg("unshare")
        .args(args)
        .output()
        .context("cannot run podman unshare")?;
    if !out.status.success() {
        bail!(
            "podman unshare {} failed: {}",
            args[0].to_string_lossy(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Deletes each file and folder in the folder `dir`, and keeps `dir`
/// (01M5781A4PRTTYRHH5GJ1GHN7G). It deletes as the person first. When
/// the person may not delete a file (a file of a user of the container
/// in rootless podman), it deletes with `podman unshare`, as the root
/// of the user namespace (01M588JW2KS6EVX1SE49686FQ7). When that fails
/// too, the error says both causes.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// std::fs::create_dir_all(dir.path().join("a/b"))?;
/// std::fs::write(dir.path().join("a/b/f"), b"o")?;
/// std::fs::write(dir.path().join("g"), b"o")?;
/// riff::compile_cache::clear(dir.path())?;
/// assert!(dir.path().is_dir());
/// assert_eq!(std::fs::read_dir(dir.path())?.count(), 0);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn clear(dir: &Path) -> Result<()> {
    let Err(e) = clear_here(dir) else {
        return Ok(());
    };
    let denied = e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied)
    });
    if !denied {
        return Err(e);
    }
    // A build can write a new file meanwhile: a second try.
    let find = [
        OsStr::new("find"),
        dir.as_os_str(),
        OsStr::new("-mindepth"),
        OsStr::new("1"),
        OsStr::new("-maxdepth"),
        OsStr::new("1"),
        OsStr::new("-exec"),
        OsStr::new("rm"),
        OsStr::new("-rf"),
        OsStr::new("--"),
        OsStr::new("{}"),
        OsStr::new("+"),
    ];
    unshare(&find)
        .or_else(|_| unshare(&find))
        .map(drop)
        .map_err(|second| anyhow::anyhow!("{e:#}; {second:#}"))
        .with_context(|| format!("cannot empty {}", dir.display()))
}

/// Deletes each file and folder in the folder `dir` as the person. A
/// build that runs can write a new file meanwhile: riff tries each
/// entry a second time. A file that is gone is no matter.
fn clear_here(dir: &Path) -> Result<()> {
    let entries =
        std::fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let remove = || {
            let gone = if entry.file_type().is_ok_and(|t| t.is_dir()) {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            match gone {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            }
        };
        remove()
            .or_else(|_| remove())
            .with_context(|| format!("cannot delete {}", path.display()))?;
    }
    Ok(())
}

/// The cache of the host now, as `riff daemon cache` and `riff daemon
/// status` show it ([`crate::text::daemon_cache`]).
pub fn shown(dir: &Path) -> String {
    let setting = Setting::read(dir).unwrap_or_default();
    let now = folder().map(|f| measure(&f).map_err(|e| format!("{e:#}")));
    crate::text::daemon_cache(&setting.size, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder that the person cannot read is never 0: riff reads its
    /// true size with podman unshare, or says that it cannot (#1115).
    #[test]
    fn a_folder_that_the_person_cannot_read_is_not_0() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let shut = dir.path().join("shut");
        std::fs::create_dir(&shut).unwrap();
        std::fs::write(shut.join("f"), vec![1u8; 100_000]).unwrap();
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
        let readable = std::fs::read_dir(&shut).is_ok();
        let used = used(dir.path());
        let measured = measure(dir.path());
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o700)).unwrap();
        if readable {
            eprintln!("skip: root reads each folder");
            return;
        }
        assert_eq!(
            used.unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        match measured {
            Ok(bytes) => assert!(bytes >= 100_000, "{bytes}"),
            Err(e) => assert!(format!("{e:#}").contains("cannot read"), "{e:#}"),
        }
    }

    #[test]
    fn a_clear_of_a_folder_that_is_not_there_fails() {
        let dir = tempfile::tempdir().unwrap();
        assert!(clear(&dir.path().join("none")).is_err());
    }

    #[test]
    fn a_new_size_takes_the_place_of_the_old_one_in_the_spec() {
        let mut spec = crate::run::spec(&crate::run::Facts {
            session: "1a2b3c4d-0000".into(),
            item: "issue-12".into(),
            image: "i".into(),
            home: "/h".into(),
            user: "m".into(),
            clone: "/c".into(),
            objects: "/o".into(),
            claude: "/cl".into(),
            credentials: "/cr".into(),
            gitconfig: None,
            gh_config: None,
        });
        Setting { size: "7G".into() }.apply(&mut spec);
        Setting { size: "9G".into() }.apply(&mut spec);
        let sizes: Vec<_> = spec
            .env
            .iter()
            .filter_map(|e| match e {
                Env::Value(name, value) if name == SIZE_VAR => Some(value.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(sizes, ["9G"]);
    }
}
