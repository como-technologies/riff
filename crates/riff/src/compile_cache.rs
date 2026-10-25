//! The compile cache of the container sessions of a host: the volume
//! [`SCCACHE_VOLUME`] (#1069).
//!
//! # Design
//!
//! riff sets no cache in a session (01M5SMW28KT5D5HPRH7KAZ8TVC). The
//! `riff.toml` of riff names the volume [`SCCACHE_VOLUME`], and each
//! `sccache` server of a session reads and writes it
//! (01M5SMW2DQ8G919EJ4P0GXF9Y3). `riff daemon cache` is the one place to
//! look at that volume, and to empty it. It knows the name of the volume
//! of riff: a repository that names another volume is outside it.
//!
//! | Command | Does |
//! |---|---|
//! | `riff daemon cache` | shows the size of the volume now |
//! | `riff daemon cache --clear` | deletes each file in the volume ([`clear`], 01M5781A4PRTTYRHH5GJ1GHN7G) |
//!
//! Each of them also takes `--host HOST`: the daemon of that host does
//! it ([`change`], [`crate::daemon::remote`]).
//!
//! `riff daemon status` shows the same line ([`crate::text::daemon_cache`],
//! 01M5781930TF6XBX7ME7RAMX2X).
//!
//! The most size of the cache is the variable `SCCACHE_CACHE_SIZE` in the
//! `riff.toml` of the repository: riff sets no size.
//!
//! The clear keeps the volume and its top folder: each session that runs
//! has it as a bind. The `riff.toml` of riff sets
//! `SCCACHE_IGNORE_SERVER_IO_ERROR=1`. For `sccache`, a file that the
//! clear deleted is a miss: the build compiles, and writes the file
//! again.
//!
//! In rootless podman, a folder of the volume can belong to a user of
//! the container, and the person cannot read it. Then [`measure`] and
//! [`clear`] use `podman unshare`, as the root of the user namespace
//! (01M588JW2KS6EVX1SE49686FQ7).
//!
//! ```mermaid
//! flowchart LR
//!     C["riff daemon cache --clear"] --> P["podman volume inspect riff-sccache"]
//!     P --> R["delete each file in its folder,<br/>keep the folder"]
//!     R -- "permission denied" --> U["podman unshare find ... rm -rf"]
//!     R --> B["a build that runs: a miss, it compiles"]
//! ```

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The named volume of the compile cache of riff: the one that the
/// `riff.toml` of riff names (01M5SMW2DQ8G919EJ4P0GXF9Y3).
pub const SCCACHE_VOLUME: &str = "riff-sccache";

/// `riff daemon cache`, on this host or for the daemon of a host with
/// `--host`: empties the volume when `clear`. It gives the text to show:
/// the line of [`shown`] last.
///
/// ```
/// let shown = riff::compile_cache::change(false)?;
/// assert!(shown.contains("riff-sccache"));
/// assert!(!shown.contains("at most"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn change(clear: bool) -> Result<String> {
    let mut lines = Vec::new();
    if clear && let Some(folder) = folder() {
        self::clear(&folder)?;
        lines.push(crate::text::DAEMON_CACHE_CLEARED.to_owned());
    }
    lines.push(shown());
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

/// The size of the files in the folder `dir` now, in bytes: the size on
/// the disk of each file (01M588JW2KS6EVX1SE49686FQ7). riff shows it
/// with 1GB = 1024³ bytes ([`crate::text::size_words`]), the unit that
/// `sccache` reads in `40G`: 40 × 1024³ bytes. `podman system
/// df -v` counts 1GB as 1000³ bytes, so its number for the volume is
/// about 7% larger, for example 8.3GB for 7.7GB.
///
/// It reads the folder as the person first. In rootless podman, a
/// folder of the volume can belong to a user of the container: then
/// the person cannot read it, and riff reads it with `podman unshare
/// du`, as the root of the user namespace. It is an error when riff
/// cannot read it either way: riff never shows 0 for a folder that it
/// cannot read.
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
pub fn shown() -> String {
    let now = folder().map(|f| measure(&f).map_err(|e| format!("{e:#}")));
    crate::text::daemon_cache(now)
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
}
