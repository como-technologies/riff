//! The reads and the writes of the host in a folder that a container
//! writes, and their one guard (#2172, 01M7FE5FYFAR92PM3EJJ211VM2).
//!
//! # Design
//!
//! A container writes each bind with write: its clone, its Claude folder,
//! the folder of its socket and its volumes. It can put a link there, for
//! example `.claude.json -> ~/.bashrc`, and the paths are the same in the
//! container and on the host. A write of the host that follows the link
//! writes over a file of the person. A read that follows it can bring a
//! file of the person, for example the sign-in file of `claude`, into a
//! folder that the container reads. So the host follows no link there.
//!
//! - Each read and write of the host in a folder that a container writes
//!   goes through this module: [`read()`], [`write()`], [`list`],
//!   [`modified`] and [`set_mode`]. Each one takes that folder, `root`,
//!   and a name in it. riff made `root`, and a container cannot change
//!   it: it is the source of a bind. So a link above `root` counts, for
//!   example a data folder of riff on another disk.
//! - Each part of the name below `root` opens from the folder above it
//!   (`openat`), with no follow of a link (`O_NOFOLLOW`). So a link in the
//!   middle of the name counts as a link too. A link gives the error
//!   [`text::host_file_link`], which names it. A read takes only a plain
//!   file: a FIFO or a device gives the error [`text::host_file_not_plain`],
//!   and the read does not wait for a writer.
//! - [`write()`] makes a new file `NAME.riff` beside `NAME`, with
//!   `create_new` (`O_CREAT | O_EXCL`, which follows no link), and renames
//!   it to `NAME`. A rename replaces a link and never writes through it.
//!   All the same, [`write()`] refuses a link at either name, so that the
//!   person sees it.
//! - The steps of the host there: the Claude folder of each start
//!   ([`crate::run::claude_folder`]), the transcripts
//!   ([`crate::usage::transcripts_in`], [`crate::usage::written_in`],
//!   [`crate::usage::Replies::read`]), and the mode of the socket of a
//!   session. The host also writes the list of the branches of `origin`
//!   of the main clone through it, in the folder of a session or a lead,
//!   which no container writes ([`crate::session_clone::write_refs`]).
//!   The host makes no clone of a session, and reads no file in a clone
//!   ([`crate::session_clone`]). The host reads no file in a volume: it
//!   only measures and clears the cache of sccache, with no follow of a
//!   link ([`crate::compile_cache`]).
//! - A test of this module reads the code of riff. It finds each
//!   production line that names the clone, the Claude folder or the
//!   folder of the socket of a session or a lead. It finds each function
//!   that calls this module, and each function that calls one of these.
//!   Each function that it finds follows no link. Each step above goes
//!   through this module.
//!
//! ```mermaid
//! flowchart LR
//!     C["a read or a write of ROOT/NAME"] --> P["open each part of NAME<br/>from the folder above it<br/>with O_NOFOLLOW"]
//!     P --> L{"a link?"}
//!     L -- yes --> E["an error that names the link<br/>the host writes nothing"]
//!     L -- no --> W["write NAME.riff with create_new<br/>rename it to NAME"]
//! ```
//!
//! ```
//! use std::path::Path;
//!
//! let dir = tempfile::tempdir()?;
//! let root = dir.path().join("claude");
//! std::fs::create_dir(&root)?;
//! riff::host_file::write(&root, Path::new("settings.json"), b"{}")?;
//! assert_eq!(riff::host_file::read(&root, Path::new("settings.json"))?, Some(b"{}".to_vec()));
//! assert_eq!(riff::host_file::read(&root, Path::new("none.json"))?, None);
//!
//! // A link that the container made: the host neither reads nor writes
//! // the file that it names.
//! let person = dir.path().join("person.txt");
//! std::fs::write(&person, "the file of the person")?;
//! std::os::unix::fs::symlink(&person, root.join(".claude.json"))?;
//! let error = riff::host_file::write(&root, Path::new(".claude.json"), b"{}").unwrap_err();
//! assert!(error.to_string().contains(&root.join(".claude.json").display().to_string()), "{error}");
//! assert!(riff::host_file::read(&root, Path::new(".claude.json")).is_err());
//! assert_eq!(std::fs::read_to_string(&person)?, "the file of the person");
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};
use nix::errno::Errno;
use nix::fcntl::{AtFlags, OFlag, openat, renameat};
use nix::sys::stat::{FchmodatFlags, FileStat, Mode, SFlag, fchmodat, fstatat};
use nix::unistd::{UnlinkatFlags, unlinkat};

use crate::text;

/// The end of the name of the new file of [`write()`], beside the file.
pub const NEW: &str = "riff";

/// The kind of an entry of a folder, as [`list`] sees it, with no follow
/// of a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A plain file.
    File,
    /// A folder.
    Folder,
    /// A link: the host does not follow it.
    Link,
    /// Something else, for example a FIFO or a socket.
    Other,
}

impl Kind {
    fn of(stat: &FileStat) -> Kind {
        match SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT {
            SFlag::S_IFREG => Kind::File,
            SFlag::S_IFDIR => Kind::Folder,
            SFlag::S_IFLNK => Kind::Link,
            _ => Kind::Other,
        }
    }
}

/// The parts of `name`: an error when it is empty, absolute, or has `.`
/// or `..`, so that each part is a name in the folder above it.
fn parts(root: &Path, name: &Path) -> Result<Vec<OsString>> {
    let parts = name
        .components()
        .map(|part| match part {
            Component::Normal(part) => Ok(part.to_owned()),
            _ => bail!(
                "riff takes no {} in {}: a name must go down from the folder",
                name.display(),
                root.display()
            ),
        })
        .collect::<Result<Vec<_>>>()?;
    if parts.is_empty() {
        bail!("riff takes no empty name in {}", root.display());
    }
    Ok(parts)
}

/// The error of the part `part` in the folder `at` of `root`: the error
/// [`text::host_file_link`] when it is a link.
fn fault(dir: &OwnedFd, at: &Path, part: &OsStr, errno: Errno) -> anyhow::Error {
    let path = at.join(part);
    if kind_at(dir, part) == Some(Kind::Link) {
        anyhow::anyhow!("{}", text::host_file_link(&path))
    } else {
        anyhow::Error::new(std::io::Error::from(errno)).context(format!("open {}", path.display()))
    }
}

/// The folder `root`, opened with each link of its own path followed.
fn open_root(root: &Path) -> Result<OwnedFd> {
    nix::fcntl::open(
        root,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(std::io::Error::from)
    .with_context(|| format!("open {}", root.display()))
}

/// The folder `ROOT/PARTS`: each part opens from the folder above it,
/// with no follow of a link. `None` when a part is not there.
fn open_folder(root: &Path, parts: &[OsString]) -> Result<Option<(OwnedFd, PathBuf)>> {
    let mut dir = open_root(root)?;
    let mut at = root.to_owned();
    for part in parts {
        let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
        match openat(&dir, part.as_os_str(), flags, Mode::empty()) {
            Ok(next) => dir = next,
            Err(Errno::ENOENT) => return Ok(None),
            Err(errno) => return Err(fault(&dir, &at, part, errno)),
        }
        at.push(part);
    }
    Ok(Some((dir, at)))
}

/// The folder of `name` in `root`, the last part of `name`, and the path
/// of that folder. `None` when the folder is not there.
#[allow(clippy::type_complexity)]
fn open_parent(root: &Path, name: &Path) -> Result<Option<(OwnedFd, OsString, PathBuf)>> {
    let mut parts = parts(root, name)?;
    let last = parts.pop().expect("parts gives at least one part");
    Ok(open_folder(root, &parts)?.map(|(dir, at)| (dir, last, at)))
}

/// The bytes of the plain file `ROOT/NAME`, or `None` when it is not
/// there. A link in `NAME` gives the error [`text::host_file_link`]. A
/// file that is no plain file, for example a FIFO, gives the error
/// [`text::host_file_not_plain`].
///
/// ```
/// use std::path::Path;
///
/// let root = tempfile::tempdir()?;
/// std::fs::create_dir(root.path().join("projects"))?;
/// std::fs::write(root.path().join("projects/a.jsonl"), "line")?;
/// assert_eq!(riff::host_file::read(root.path(), Path::new("projects/a.jsonl"))?, Some(b"line".to_vec()));
/// assert_eq!(riff::host_file::read(root.path(), Path::new("none/a.jsonl"))?, None);
///
/// // A link in the middle of the name counts.
/// let other = tempfile::tempdir()?;
/// std::fs::write(other.path().join("a.jsonl"), "secret")?;
/// std::os::unix::fs::symlink(other.path(), root.path().join("linked"))?;
/// let error = riff::host_file::read(root.path(), Path::new("linked/a.jsonl")).unwrap_err();
/// assert_eq!(error.to_string(), riff::text::host_file_link(&root.path().join("linked")));
/// assert!(riff::host_file::read(root.path(), Path::new("../a.jsonl")).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read(root: &Path, name: &Path) -> Result<Option<Vec<u8>>> {
    let Some((dir, last, at)) = open_parent(root, name)? else {
        return Ok(None);
    };
    // O_NONBLOCK: the open of a FIFO does not wait for a writer.
    let flags = OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC;
    let fd = match openat(&dir, last.as_os_str(), flags, Mode::empty()) {
        Ok(fd) => fd,
        Err(Errno::ENOENT) => return Ok(None),
        Err(errno) => return Err(fault(&dir, &at, &last, errno)),
    };
    let path = at.join(&last);
    let mut file = File::from(fd);
    let meta = file
        .metadata()
        .with_context(|| format!("read {}", path.display()))?;
    if !meta.is_file() {
        bail!("{}", text::host_file_not_plain(&path));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("read {}", path.display()))?;
    Ok(Some(bytes))
}

/// The text of [`read()`]: an error when it is no UTF-8.
///
/// ```
/// use std::path::Path;
///
/// let root = tempfile::tempdir()?;
/// std::fs::write(root.path().join("a"), "text")?;
/// assert_eq!(riff::host_file::read_to_string(root.path(), Path::new("a"))?.as_deref(), Some("text"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read_to_string(root: &Path, name: &Path) -> Result<Option<String>> {
    read(root, name)?
        .map(|bytes| {
            String::from_utf8(bytes)
                .with_context(|| format!("{} is no text", root.join(name).display()))
        })
        .transpose()
}

/// The kind of the entry `part` of the folder `dir`, with no follow of a
/// link. `None` when it is not there.
fn kind_at(dir: &OwnedFd, part: &OsStr) -> Option<Kind> {
    fstatat(dir, part, AtFlags::AT_SYMLINK_NOFOLLOW)
        .ok()
        .map(|stat| Kind::of(&stat))
}

/// Writes `bytes` into the file `ROOT/NAME`: a new file `NAME.riff` with
/// `create_new`, then a rename to `NAME`. So a process that reads the
/// file sees the old text or the new text, never a part. The file keeps
/// its mode (01M509TK01PW3RN2YPRJD38RSX). The folder of
/// `NAME` must be there. A link at `NAME`, at `NAME.riff` or in the
/// folders of `NAME` gives the error [`text::host_file_link`], and the
/// host writes nothing. A file `NAME.riff` that an earlier write left
/// goes first.
///
/// ```
/// use std::path::Path;
///
/// let root = tempfile::tempdir()?;
/// let name = Path::new("settings.json");
/// std::fs::write(root.path().join("settings.json.riff"), "left")?;
/// riff::host_file::write(root.path(), name, b"new")?;
/// assert_eq!(std::fs::read_to_string(root.path().join(name))?, "new");
/// assert!(!root.path().join("settings.json.riff").exists());
///
/// // A link at the new file: the host writes nothing through it.
/// let person = root.path().join("person");
/// std::fs::write(&person, "kept")?;
/// std::os::unix::fs::symlink(&person, root.path().join("settings.json.riff"))?;
/// let error = riff::host_file::write(root.path(), name, b"newer").unwrap_err();
/// assert_eq!(error.to_string(), riff::text::host_file_link(&root.path().join("settings.json.riff")));
/// assert_eq!(std::fs::read_to_string(&person)?, "kept");
/// assert_eq!(std::fs::read_to_string(root.path().join(name))?, "new");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn write(root: &Path, name: &Path, bytes: &[u8]) -> Result<()> {
    let Some((dir, last, at)) = open_parent(root, name)? else {
        bail!(
            "riff cannot write {}: its folder is not there",
            root.join(name).display()
        );
    };
    let path = at.join(&last);
    let old = fstatat(&dir, last.as_os_str(), AtFlags::AT_SYMLINK_NOFOLLOW).ok();
    if old.as_ref().map(Kind::of) == Some(Kind::Link) {
        bail!("{}", text::host_file_link(&path));
    }
    let mut new = last.clone();
    new.push(".");
    new.push(NEW);
    let new_path = at.join(&new);
    match kind_at(&dir, &new) {
        Some(Kind::Link) => bail!("{}", text::host_file_link(&new_path)),
        Some(Kind::File) => unlinkat(&dir, new.as_os_str(), UnlinkatFlags::NoRemoveDir)
            .map_err(std::io::Error::from)
            .with_context(|| format!("remove {}", new_path.display()))?,
        _ => {}
    }
    let flags =
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let mode = Mode::from_bits_truncate(0o666);
    let fd = match openat(&dir, new.as_os_str(), flags, mode) {
        Ok(fd) => fd,
        Err(errno) => return Err(fault(&dir, &at, &new, errno)),
    };
    let mut file = File::from(fd);
    // The file keeps its mode (01M509TK01PW3RN2YPRJD38RSX).
    let kept = match old.filter(|old| Kind::of(old) == Kind::File) {
        Some(old) => nix::sys::stat::fchmod(&file, Mode::from_bits_truncate(old.st_mode))
            .map_err(std::io::Error::from),
        None => Ok(()),
    };
    let written = kept
        .and_then(|()| file.write_all(bytes))
        .with_context(|| format!("write {}", new_path.display()))
        .and_then(|()| {
            renameat(&dir, new.as_os_str(), &dir, last.as_os_str())
                .map_err(std::io::Error::from)
                .with_context(|| format!("write {}", path.display()))
        });
    if written.is_err() {
        let _ = unlinkat(&dir, new.as_os_str(), UnlinkatFlags::NoRemoveDir);
    }
    written
}

/// Each entry of the folder `ROOT/NAME` with its kind, with no follow of
/// a link, in the order of the names. `None` when the folder is not
/// there. A link at `NAME` or in its folders gives the error
/// [`text::host_file_link`].
///
/// ```
/// use std::path::Path;
/// use riff::host_file::Kind;
///
/// let root = tempfile::tempdir()?;
/// let projects = root.path().join("projects");
/// std::fs::create_dir_all(projects.join("-data-clone"))?;
/// std::fs::write(projects.join("a.jsonl"), "")?;
/// std::os::unix::fs::symlink("/etc", projects.join("etc"))?;
/// assert_eq!(
///     riff::host_file::list(root.path(), Path::new("projects"))?,
///     Some(vec![
///         ("-data-clone".into(), Kind::Folder),
///         ("a.jsonl".into(), Kind::File),
///         ("etc".into(), Kind::Link),
///     ])
/// );
/// assert_eq!(riff::host_file::list(root.path(), Path::new("none"))?, None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn list(root: &Path, name: &Path) -> Result<Option<Vec<(OsString, Kind)>>> {
    let parts = parts(root, name)?;
    let Some((dir, at)) = open_folder(root, &parts)? else {
        return Ok(None);
    };
    let listed = nix::dir::Dir::from_fd(
        dir.try_clone()
            .with_context(|| format!("list {}", at.display()))?,
    )
    .map_err(std::io::Error::from)
    .with_context(|| format!("list {}", at.display()))?;
    let mut entries = Vec::new();
    for entry in listed {
        let entry = entry
            .map_err(std::io::Error::from)
            .with_context(|| format!("list {}", at.display()))?;
        let name = OsStr::from_bytes(entry.file_name().to_bytes());
        if name == "." || name == ".." {
            continue;
        }
        if let Some(kind) = kind_at(&dir, name) {
            entries.push((name.to_owned(), kind));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(Some(entries))
}

/// Sets the mode `mode` of the entry `ROOT/NAME`, for example a socket,
/// with no follow of a link (`AT_SYMLINK_NOFOLLOW`). A link there gives
/// the error [`text::host_file_link`], and the mode of the file that it
/// names stays.
///
/// ```
/// use std::os::unix::fs::PermissionsExt;
/// use std::path::Path;
///
/// let root = tempfile::tempdir()?;
/// let socket = root.path().join("riff.sock");
/// let _listener = std::os::unix::net::UnixListener::bind(&socket)?;
/// riff::host_file::set_mode(root.path(), Path::new("riff.sock"), 0o600)?;
/// assert_eq!(std::fs::metadata(&socket)?.permissions().mode() & 0o777, 0o600);
///
/// let person = root.path().join("person");
/// std::fs::write(&person, "")?;
/// std::fs::set_permissions(&person, std::fs::Permissions::from_mode(0o644))?;
/// std::os::unix::fs::symlink(&person, root.path().join("link.sock"))?;
/// assert!(riff::host_file::set_mode(root.path(), Path::new("link.sock"), 0o600).is_err());
/// assert_eq!(std::fs::metadata(&person)?.permissions().mode() & 0o777, 0o644);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn set_mode(root: &Path, name: &Path, mode: u32) -> Result<()> {
    let Some((dir, last, at)) = open_parent(root, name)? else {
        bail!(
            "riff cannot set the mode of {}: its folder is not there",
            root.join(name).display()
        );
    };
    let path = at.join(&last);
    if kind_at(&dir, &last) == Some(Kind::Link) {
        bail!("{}", text::host_file_link(&path));
    }
    fchmodat(
        &dir,
        last.as_os_str(),
        Mode::from_bits_truncate(mode as nix::sys::stat::mode_t),
        FchmodatFlags::NoFollowSymlink,
    )
    .map_err(std::io::Error::from)
    .with_context(|| format!("cannot set the mode of {}", path.display()))
}

/// The time of the last change of the plain file `ROOT/NAME`, with no
/// follow of a link. `None` when it is not there, or is no plain file.
///
/// ```
/// use std::path::Path;
///
/// let root = tempfile::tempdir()?;
/// std::fs::write(root.path().join("a.jsonl"), "")?;
/// std::os::unix::fs::symlink(root.path().join("a.jsonl"), root.path().join("b.jsonl"))?;
/// assert!(riff::host_file::modified(root.path(), Path::new("a.jsonl"))?.is_some());
/// assert_eq!(riff::host_file::modified(root.path(), Path::new("b.jsonl"))?, None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn modified(root: &Path, name: &Path) -> Result<Option<SystemTime>> {
    let Some((dir, last, _)) = open_parent(root, name)? else {
        return Ok(None);
    };
    let Ok(stat) = fstatat(&dir, last.as_os_str(), AtFlags::AT_SYMLINK_NOFOLLOW) else {
        return Ok(None);
    };
    if Kind::of(&stat) != Kind::File {
        return Ok(None);
    }
    let since = Duration::new(
        u64::try_from(stat.st_mtime).unwrap_or(0),
        u32::try_from(stat.st_mtime_nsec).unwrap_or(0),
    );
    Ok(Some(SystemTime::UNIX_EPOCH + since))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of a session in a temp folder, and a file of the person
    /// beside it with the text `SECRET`.
    fn folders() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let claude = root.path().join("claude");
        std::fs::create_dir(&claude).unwrap();
        let person = root.path().join("person");
        std::fs::write(&person, "SECRET").unwrap();
        (root, claude, person)
    }

    #[test]
    fn a_write_through_a_linked_folder_writes_nothing() {
        let (root, claude, person) = folders();
        let outside = root.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, claude.join("projects")).unwrap();
        let error = write(&claude, Path::new("projects/a.jsonl"), b"x").unwrap_err();
        assert_eq!(
            error.to_string(),
            text::host_file_link(&claude.join("projects"))
        );
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        assert_eq!(std::fs::read_to_string(person).unwrap(), "SECRET");
    }

    #[test]
    fn a_write_keeps_a_link_at_the_name_and_its_file() {
        let (_root, claude, person) = folders();
        let link = claude.join("settings.json");
        std::os::unix::fs::symlink(&person, &link).unwrap();
        let error = write(&claude, Path::new("settings.json"), b"{}").unwrap_err();
        assert_eq!(error.to_string(), text::host_file_link(&link));
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(person).unwrap(), "SECRET");
        assert!(!claude.join("settings.json.riff").exists());
    }

    /// 01M509TK01PW3RN2YPRJD38RSX: the file keeps its mode.
    #[test]
    fn a_write_keeps_the_mode_of_the_file() {
        use std::os::unix::fs::PermissionsExt;
        let (_root, claude, _person) = folders();
        let path = claude.join("settings.json");
        std::fs::write(&path, "{}").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        write(&claude, Path::new("settings.json"), b"{\"a\":1}").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o640);
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"a\":1}");
    }

    #[test]
    fn a_write_needs_its_folder_and_takes_no_folder_at_the_name() {
        let (_root, claude, _person) = folders();
        assert!(write(&claude, Path::new("none/a.json"), b"{}").is_err());
        std::fs::create_dir(claude.join("a.json")).unwrap();
        assert!(write(&claude, Path::new("a.json"), b"{}").is_err());
        assert!(!claude.join("a.json.riff").exists());
    }

    #[test]
    fn a_dangling_link_counts_as_a_link() {
        let (root, claude, _person) = folders();
        let gone = root.path().join("gone");
        std::os::unix::fs::symlink(&gone, claude.join(".claude.json")).unwrap();
        let error = read(&claude, Path::new(".claude.json")).unwrap_err();
        assert_eq!(
            error.to_string(),
            text::host_file_link(&claude.join(".claude.json"))
        );
        assert!(write(&claude, Path::new(".claude.json"), b"{}").is_err());
        assert!(!gone.exists());
    }

    #[test]
    fn a_list_through_a_linked_folder_is_an_error() {
        let (root, claude, _person) = folders();
        std::os::unix::fs::symlink(root.path(), claude.join("projects")).unwrap();
        let error = list(&claude, Path::new("projects")).unwrap_err();
        assert_eq!(
            error.to_string(),
            text::host_file_link(&claude.join("projects"))
        );
        assert_eq!(modified(&claude, Path::new("projects/person")).ok(), None);
    }

    #[test]
    fn a_link_above_the_root_counts() {
        let (root, claude, _person) = folders();
        let data = root.path().join("data");
        std::os::unix::fs::symlink(&claude, &data).unwrap();
        write(&data, Path::new("a.json"), b"{}").unwrap();
        assert_eq!(std::fs::read(claude.join("a.json")).unwrap(), b"{}");
        assert_eq!(
            read(&data, Path::new("a.json")).unwrap(),
            Some(b"{}".to_vec())
        );
    }

    #[test]
    fn a_name_must_go_down_from_the_root() {
        let (_root, claude, _person) = folders();
        for name in ["", "/etc/passwd", "../person", "a/../b", "./a"] {
            assert!(read(&claude, Path::new(name)).is_err(), "{name}");
            assert!(write(&claude, Path::new(name), b"x").is_err(), "{name}");
        }
    }
}

/// The scan of the code of riff (01M7FE5FYFAR92PM3EJJ211VM2): it finds
/// each production line that names a folder that a container writes,
/// and each function that gets a path there from its caller. It checks
/// that the steps of the host there follow no link.
#[cfg(test)]
mod scan {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use crate::host_git::tests::{production, sources};

    /// The words of a production line that names a folder that a
    /// container writes in a folder of a session or a lead: its clone, its
    /// Claude folder, and the folder of its socket.
    const NAMES: [&str; 8] = [
        "join(\"claude\")",
        "join(\"clone\")",
        "\"claude/",
        "\"clone/",
        "claude_in(",
        "clone_in(",
        "owned.socket",
        "session_dir(",
    ];

    /// Each production line that names such a folder, as the scan finds
    /// it: the file, and the line with no space at its ends. A new line
    /// fails the scan: read and write in the folder only through
    /// [`super`], then add the line here.
    const NAMED: &[(&str, &str)] = &[
        // The folder of the socket: its path, its bind, its record, the
        // remove of the folder, and its socket ([`crate::daemon`],
        // `bind_in`).
        (
            "daemon.rs",
            "pub fn session_dir(dir: &Path, name: &str) -> PathBuf {",
        ),
        ("daemon.rs", "socket: session_dir(&self.dir, &name),"),
        (
            "daemon.rs",
            "let _ = std::fs::remove_dir_all(session_dir(&self.dir, &name));",
        ),
        (
            "daemon.rs",
            "let _ = std::fs::remove_dir_all(&owned.socket);",
        ),
        ("daemon.rs", "let dir = &owned.socket;"),
        ("daemon.rs", "socket: owned.socket.clone(),"),
        ("daemon.rs", "source: Source::Path(owned.socket.clone()),"),
        // The paths of the clone and of the Claude folder in the folder of
        // a session or a lead ([`crate::run::clone_in`],
        // [`crate::run::claude_in`]).
        ("run.rs", "pub fn clone_in(folder: &Path) -> PathBuf {"),
        ("run.rs", "folder.join(\"clone\")"),
        ("run.rs", "pub fn claude_in(folder: &Path) -> PathBuf {"),
        ("run.rs", "folder.join(\"claude\")"),
        // The transcripts of a session and of a lead
        // ([`crate::usage::transcripts_in`], [`crate::usage::written_in`]).
        (
            "daemon.rs",
            "usage::transcripts_in(&crate::run::claude_in(&record.folder), &live.session);",
        ),
        (
            "lead.rs",
            "crate::usage::written_in(&run::claude_in(&record.folder), &record.live.session)",
        ),
        // The clone of an older riff in the folder of a lead: it goes with
        // no git ([`crate::lead::remove_host_clone`]).
        ("lead.rs", "let clone = run::clone_in(folder);"),
        // The start of a session and of a lead: the Claude folder
        // ([`crate::run::claude_folder`]), and the paths of the spec. The
        // clone is a volume of the container: the host has no folder there.
        ("lead.rs", "&run::claude_in(folder),"),
        ("lead.rs", "&run::clone_in(folder),"),
        ("lead.rs", "&run::claude_in(&folder),"),
        ("lead.rs", "&run::clone_in(&folder),"),
        ("lead.rs", "path: run::clone_in(folder),"),
        ("lead.rs", "claude: run::claude_in(folder),"),
        ("run.rs", "let clone = clone_in(&folder);"),
        ("run.rs", "let claude = claude_in(&folder);"),
        ("wave.rs", "path: run::clone_in(folder),"),
        ("wave.rs", "claude: run::claude_in(folder),"),
    ];

    /// The steps of the host that read or write a file in such a folder,
    /// each one through [`super`]: the file and the start of the
    /// function. The list of the refs of `origin` goes through it too, in
    /// a folder that no container writes.
    const STEPS: &[(&str, &str)] = &[
        ("run.rs", "pub fn claude_folder("),
        ("usage.rs", "fn transcripts_with_times("),
        ("usage.rs", "pub fn read(paths: &[PathBuf]) -> Replies {"),
        ("daemon.rs", "fn bind_in("),
        ("session_clone.rs", "pub fn write_refs("),
    ];

    /// The calls that take a path in such a folder: the guard, and each
    /// function of riff that calls it. A function with one of them gets
    /// the path from a line of [`NAMED`] or from its caller, so it follows
    /// no link, as a function with a line of [`NAMED`].
    const GUARDED: [&str; 6] = [
        "host_file::",
        "claude_folder(",
        "transcripts_with_times(",
        "Replies::read(",
        "bind_in(",
        "write_refs(",
    ];

    /// The calls that read or write a file, or look at it, with each link
    /// followed. No function of [`STEPS`], no function with a line of
    /// [`NAMED`], and no function with a call of [`GUARDED`] has one of
    /// them, but on a line of [`OUTSIDE`].
    const FOLLOWS: [&str; 19] = [
        "std::fs::read(",
        "std::fs::read_to_string(",
        "std::fs::write(",
        "std::fs::read_dir(",
        "std::fs::metadata(",
        "std::fs::set_permissions(",
        "std::fs::copy(",
        "std::fs::canonicalize(",
        "File::open(",
        "File::create(",
        "OpenOptions::new(",
        "tokio::fs::",
        ".is_file()",
        ".is_dir()",
        ".exists()",
        ".canonicalize()",
        ".metadata()",
        "permissions::add(",
        "permissions::write(",
    ];

    /// The lines of those functions with a call of [`FOLLOWS`] on a path
    /// out of each folder that a container writes: the file, the line,
    /// and the path.
    const OUTSIDE: &[(&str, &str)] = &[
        // The `~/.gitconfig` of the person.
        (
            "lead.rs",
            "let existing = |p: PathBuf| p.exists().then_some(p);",
        ),
        (
            "run.rs",
            "let existing = |p: PathBuf| p.exists().then_some(p);",
        ),
        (
            "wave.rs",
            "let existing = |p: PathBuf| p.exists().then_some(p);",
        ),
        // The `~/.claude.json` of the person.
        ("run.rs", "let theme = std::fs::read(person)"),
        // The `events.jsonl` of the daemon in the folder of the session.
        (
            "daemon.rs",
            "log: std::fs::read_to_string(events).unwrap_or_default(),",
        ),
        ("daemon.rs", "let log = std::fs::OpenOptions::new()"),
        ("daemon.rs", "(std::fs::File::create(&events), 0, 0)"),
        // The records of the daemon.
        (
            "daemon.rs",
            "let Ok(entries) = std::fs::read_dir(self.dir.join(LIVE)) else {",
        ),
        ("daemon.rs", "let record = std::fs::read_to_string(&path)"),
    ];

    /// The code of riff: each file `.rs` of `src`, with its name in `src`
    /// and its production lines.
    fn code() -> Vec<(String, String)> {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut all: Vec<PathBuf> = Vec::new();
        sources(&src, &mut all);
        all.sort();
        all.iter()
            .map(|path| {
                let name = path
                    .strip_prefix(&src)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                (name, std::fs::read_to_string(path).unwrap())
            })
            .collect()
    }

    /// The indent of `line`.
    fn indent(line: &str) -> usize {
        line.len() - line.trim_start().len()
    }

    /// True when `line` starts a function.
    fn starts_fn(line: &str) -> bool {
        let line = line.trim_start();
        [
            "fn ",
            "pub fn ",
            "pub(crate) fn ",
            "async fn ",
            "pub async fn ",
            "pub(crate) async fn ",
        ]
        .iter()
        .any(|start| line.starts_with(start))
    }

    /// The lines of the function that starts at `lines[start]`: up to the
    /// `}` with its indent.
    fn body<'a>(lines: &[(usize, &'a str)], start: usize) -> Vec<(usize, &'a str)> {
        let end = format!("{}}}", " ".repeat(indent(lines[start].1)));
        let mut out = Vec::new();
        for &(n, line) in &lines[start..] {
            out.push((n, line));
            if line == end {
                break;
            }
        }
        out
    }

    /// The start of the function that holds `lines[at]`.
    fn holder(lines: &[(usize, &str)], at: usize) -> usize {
        (0..=at)
            .rev()
            .find(|&i| {
                starts_fn(lines[i].1) && (i == at || indent(lines[i].1) < indent(lines[at].1))
            })
            .unwrap_or_else(|| panic!("no function holds line {}", lines[at].0))
    }

    /// Each line of `body` with a call of [`FOLLOWS`] that is not on a
    /// line of [`OUTSIDE`] of `file`.
    fn follows(file: &str, body: &[(usize, &str)]) -> Vec<String> {
        body.iter()
            .filter(|(_, line)| FOLLOWS.iter().any(|call| line.contains(call)))
            .filter(|(_, line)| !OUTSIDE.contains(&(file, line.trim())))
            .map(|(n, line)| format!("{file}:{n}: {}", line.trim()))
            .collect()
    }

    /// The name of the function that starts at `line`.
    fn name_of(line: &str) -> &str {
        let after = line.split_once("fn ").map_or(line, |(_, after)| after);
        &after[..after.find(['(', '<']).unwrap_or(after.len())]
    }

    /// The scan of the code `text` of `file`: each line that names a
    /// folder that a container writes, and each line with a call of
    /// [`FOLLOWS`] in a function with such a line or with a call of
    /// [`GUARDED`].
    fn scan(file: &str, text: &str) -> (Vec<String>, Vec<String>) {
        let lines = production(text);
        let (mut found, mut follow) = (Vec::new(), Vec::new());
        for (at, (_, line)) in lines.iter().enumerate() {
            let named = NAMES.iter().any(|name| line.contains(name));
            if named {
                found.push(line.trim().to_owned());
            }
            if named || GUARDED.iter().any(|call| line.contains(call)) {
                follow.extend(follows(file, &body(&lines, holder(&lines, at))));
            }
        }
        (found, follow)
    }

    /// 01M7FE5FYFAR92PM3EJJ211VM2: the scan finds each line of riff that
    /// names a folder that a container writes. Each one is known. Each
    /// function with such a line, or with a call of [`GUARDED`], follows
    /// no link.
    #[test]
    fn each_name_of_a_folder_that_a_container_writes_is_known() {
        let mut found: Vec<(String, String)> = Vec::new();
        let mut follow = Vec::new();
        for (file, text) in code().iter().filter(|(f, _)| f != "host_file.rs") {
            let (named, followed) = scan(file, text);
            found.extend(named.into_iter().map(|line| (file.clone(), line)));
            follow.extend(followed);
        }
        let count = |list: Vec<(String, String)>| {
            let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
            for entry in list {
                *counts.entry(entry).or_default() += 1;
            }
            counts
        };
        let known = NAMED
            .iter()
            .map(|(f, l)| ((*f).to_owned(), (*l).to_owned()))
            .collect();
        let (found, known) = (count(found), count(known));
        let new: Vec<_> = found
            .iter()
            .filter(|(k, n)| known.get(*k) != Some(n))
            .collect();
        let gone: Vec<_> = known
            .iter()
            .filter(|(k, n)| found.get(*k) != Some(n))
            .collect();
        assert!(
            new.is_empty() && gone.is_empty(),
            "the names of a folder that a container writes changed: read and write there only \
             through host_file, then change NAMED.\nnew: {new:#?}\ngone: {gone:#?}"
        );
        follow.sort();
        follow.dedup();
        assert!(
            follow.is_empty(),
            "a link is followed:\n{}",
            follow.join("\n")
        );
    }

    /// 01M7FE5FYFAR92PM3EJJ211VM2: each step of the host that reads or
    /// writes in a folder that a container writes goes through the guard,
    /// and follows no link.
    #[test]
    fn each_step_in_a_folder_that_a_container_writes_goes_through_the_guard() {
        let code = code();
        for (file, start) in STEPS {
            let text = &code.iter().find(|(f, _)| f == file).unwrap().1;
            let lines = production(text);
            let at = lines
                .iter()
                .position(|(_, line)| line.trim_start().starts_with(start))
                .unwrap_or_else(|| panic!("{file} has no {start}"));
            let body = body(&lines, at);
            assert!(
                body.iter().any(|(_, line)| line.contains("host_file::")),
                "{file}: {start} does not go through host_file"
            );
            let follow = follows(file, &body);
            assert!(
                follow.is_empty(),
                "a link is followed:\n{}",
                follow.join("\n")
            );
        }
    }

    /// Each function of riff that calls the guard has its call in
    /// [`GUARDED`]: the scan checks each function that calls it too.
    #[test]
    fn each_caller_of_the_guard_is_guarded() {
        let mut missing = Vec::new();
        for (file, text) in code().iter().filter(|(f, _)| f != "host_file.rs") {
            let lines = production(text);
            for (at, _) in lines.iter().enumerate() {
                if !lines[at].1.contains("host_file::") {
                    continue;
                }
                let name = name_of(lines[holder(&lines, at)].1);
                let call = format!("{name}(");
                if !GUARDED.iter().any(|g| g.ends_with(&call)) {
                    missing.push(format!("{file}: {name}"));
                }
            }
        }
        missing.dedup();
        assert!(
            missing.is_empty(),
            "a caller of host_file is not in GUARDED:\n{}",
            missing.join("\n")
        );
    }

    /// Each line of [`OUTSIDE`] is a production line of its file: a line
    /// that went does not stay in the list.
    #[test]
    fn each_line_outside_is_in_the_code() {
        let code = code();
        for (file, line) in OUTSIDE {
            let text = &code.iter().find(|(f, _)| f == file).unwrap().1;
            assert!(
                production(text).iter().any(|(_, l)| l.trim() == *line),
                "{file} has no line {line}"
            );
        }
    }

    #[test]
    fn the_scan_finds_a_read_that_follows_a_link() {
        let file = "fn a(folder: &Path) {\n    let claude = folder.join(\"claude\");\n    \
                    let _ = std::fs::read(claude.join(\"settings.json\"));\n}\n";
        let lines = production(file);
        let found = follows("x.rs", &body(&lines, holder(&lines, 1)));
        assert_eq!(
            found,
            ["x.rs:3: let _ = std::fs::read(claude.join(\"settings.json\"));"]
        );
    }

    /// A function that gets the path of a Claude folder from its caller
    /// names no folder, but its call of the guard shows it: the scan
    /// checks it.
    #[test]
    fn the_scan_checks_a_function_that_gets_the_path_from_its_caller() {
        let file = "fn seen(claude: &Path) -> bool {\n    \
                    transcripts_with_times(claude).is_empty() && claude.canonicalize().is_ok()\n}\n\
                    fn other(claude: &Path) -> bool {\n    claude.exists()\n}\n";
        let (found, follow) = scan("x.rs", file);
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(
            follow,
            ["x.rs:2: transcripts_with_times(claude).is_empty() && claude.canonicalize().is_ok()"]
        );
    }

    #[test]
    fn the_name_of_a_function_is_up_to_its_arguments() {
        assert_eq!(
            name_of("    pub fn read(paths: &[PathBuf]) -> Replies {"),
            "read"
        );
        assert_eq!(name_of("fn bind_in<P>(dir: P) {"), "bind_in");
        assert_eq!(name_of("pub(crate) async fn a() {"), "a");
    }

    #[test]
    fn the_body_of_a_function_ends_at_its_brace() {
        let file = "fn a() {\n    if x {\n    }\n    b();\n}\nfn c() {}\n";
        let lines = production(file);
        let numbers: Vec<usize> = body(&lines, holder(&lines, 3))
            .iter()
            .map(|(n, _)| *n)
            .collect();
        assert_eq!(numbers, [1, 2, 3, 4, 5]);
    }
}
