//! Writes of riff outside each sandbox into a folder that a session
//! writes.
//!
//! # Design
//!
//! A session writes some folders that riff outside each sandbox also
//! writes: the temp folder of the session and its own state folder
//! ([`local::own`](crate::local::own)). A session can put a link there,
//! for example `token -> ~/.bashrc`. A plain `std::fs::write` follows
//! the link and changes a file of the person.
//!
//! So riff outside opens such a folder once, with no follow of a link
//! ([`Dir::open`]). Then it opens each part in it relative to that open
//! folder, also with no follow (`openat` with `O_NOFOLLOW`). A link at
//! a part makes the step fail, and the target of the link does not
//! change (01M4DWJ0CZDM0AX98TY8CCTC9F).
//!
//! ```mermaid
//! flowchart LR
//!     O["riff outside"] -->|"open DIR, O_NOFOLLOW"| D["fd of DIR"]
//!     D -->|"openat NAME, O_NOFOLLOW"| F["the file"]
//!     D -->|"a link at NAME"| X["ELOOP: riff refuses"]
//! ```
//!
//! - A name is one plain part: no `/`, no `.` and no `..`.
//! - [`Dir::put`] writes a new file and renames it into place. The
//!   rename replaces a link at the name, and never writes its target.
//! - [`Dir::open`] follows no link at the last part of its path only.
//!   Each folder above it must be a folder that no session writes.
//!
//! ```
//! use riff::nofollow::Dir;
//!
//! let tmp = tempfile::tempdir()?;
//! let person = tempfile::tempdir()?;
//! let dir = Dir::open(tmp.path())?;
//! dir.put("token", b"t1", 0o600)?;
//! assert_eq!(dir.read("token")?, "t1");
//!
//! // A session plants a link: riff refuses, and the target stays.
//! std::fs::write(person.path().join("bashrc"), "mine")?;
//! std::os::unix::fs::symlink(person.path(), tmp.path().join("forge"))?;
//! assert!(dir.sub("forge", Some(0o700)).is_err());
//! std::os::unix::fs::symlink(person.path().join("bashrc"), tmp.path().join("lock"))?;
//! assert!(dir.create("lock", 0o600).is_err());
//! assert!(dir.read("lock").is_err());
//! assert_eq!(std::fs::read_to_string(person.path().join("bashrc"))?, "mine");
//! # Ok::<(), std::io::Error>(())
//! ```

use nix::fcntl::{OFlag, openat, renameat};
use nix::sys::stat::{Mode, mkdirat};
use nix::unistd::{UnlinkatFlags, unlinkat};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// An open folder. Each step in it follows no link.
#[derive(Debug)]
pub struct Dir {
    fd: OwnedFd,
    path: PathBuf,
}

/// The flags of each open of a folder.
fn dir_flags() -> OFlag {
    OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_RDONLY
}

/// An error for a name that is not one plain part.
fn plain(name: &str) -> io::Result<&str> {
    match name.is_empty() || name == "." || name == ".." || name.contains('/') {
        true => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name:?} is not one plain part of a path"),
        )),
        false => Ok(name),
    }
}

impl Dir {
    /// Opens the folder `path`. A link at its last part makes it fail.
    pub fn open(path: &Path) -> io::Result<Self> {
        let fd = nix::fcntl::open(path, dir_flags(), Mode::empty())?;
        Ok(Self {
            fd,
            path: path.to_owned(),
        })
    }

    /// The path of the folder, for a message.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The open folder.
    pub fn fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    /// Opens the folder `name` in this folder. With `mode`, it makes the
    /// folder first when it is not there.
    pub fn sub(&self, name: &str, mode: Option<u32>) -> io::Result<Dir> {
        let name = plain(name)?;
        if let Some(mode) = mode {
            match mkdirat(self.fd(), name, Mode::from_bits_truncate(mode)) {
                Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
                Err(e) => return Err(e.into()),
            }
        }
        let fd = openat(self.fd(), name, dir_flags(), Mode::empty())?;
        Ok(Dir {
            fd,
            path: self.path.join(name),
        })
    }

    /// Opens the file `name` to read and write, and makes it with `mode`
    /// when it is not there. It never truncates the file. For a lock.
    pub fn create(&self, name: &str, mode: u32) -> io::Result<File> {
        let flags = OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
        let fd = openat(
            self.fd(),
            plain(name)?,
            flags,
            Mode::from_bits_truncate(mode),
        )?;
        Ok(File::from(fd))
    }

    /// The text of the file `name`.
    pub fn read(&self, name: &str) -> io::Result<String> {
        let flags = OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
        let fd = openat(self.fd(), plain(name)?, flags, Mode::empty())?;
        let mut text = String::new();
        File::from(fd).read_to_string(&mut text)?;
        Ok(text)
    }

    /// Writes `bytes` to the file `name` with `mode`: a new file, then a
    /// rename into place. A reader sees the old text or the new text.
    pub fn put(&self, name: &str, bytes: &[u8], mode: u32) -> io::Result<()> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name = plain(name)?;
        let new = format!(
            ".{name}.{}.{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let flags = OFlag::O_WRONLY
            | OFlag::O_CREAT
            | OFlag::O_EXCL
            | OFlag::O_NOFOLLOW
            | OFlag::O_CLOEXEC;
        let fd = openat(self.fd(), new.as_str(), flags, Mode::from_bits_truncate(mode))?;
        let written = File::from(fd).write_all(bytes);
        let renamed = written.and_then(|()| {
            renameat(self.fd(), new.as_str(), self.fd(), name).map_err(io::Error::from)
        });
        if renamed.is_err() {
            let _ = unlinkat(self.fd(), new.as_str(), UnlinkatFlags::NoRemoveDir);
        }
        renamed
    }

    /// Removes the file `name`. A file that is not there is no fault.
    pub fn remove(&self, name: &str) -> io::Result<()> {
        match unlinkat(self.fd(), plain(name)?, UnlinkatFlags::NoRemoveDir) {
            Ok(()) | Err(nix::errno::Errno::ENOENT) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, symlink};

    #[test]
    fn a_name_with_more_than_one_part_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = Dir::open(tmp.path()).unwrap();
        for name in ["", ".", "..", "a/b", "../x"] {
            assert!(dir.put(name, b"x", 0o600).is_err(), "{name}");
            assert!(dir.sub(name, Some(0o700)).is_err(), "{name}");
        }
    }

    #[test]
    fn a_link_at_the_folder_itself_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let link = tmp.path().join("link");
        symlink(other.path(), &link).unwrap();
        assert!(Dir::open(&link).is_err());
    }

    #[test]
    fn put_replaces_a_link_and_leaves_its_target() {
        let tmp = tempfile::tempdir().unwrap();
        let person = tempfile::tempdir().unwrap();
        let target = person.path().join("bashrc");
        std::fs::write(&target, "mine").unwrap();
        symlink(&target, tmp.path().join("token")).unwrap();
        let dir = Dir::open(tmp.path()).unwrap();
        dir.put("token", b"t2", 0o600).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "mine");
        let meta = std::fs::symlink_metadata(tmp.path().join("token")).unwrap();
        assert!(meta.is_file());
        assert_eq!(meta.mode() & 0o777, 0o600);
        assert_eq!(dir.read("token").unwrap(), "t2");
        // No new file is left behind.
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
    }

    #[test]
    fn remove_takes_away_a_link_and_not_its_target() {
        let tmp = tempfile::tempdir().unwrap();
        let person = tempfile::tempdir().unwrap();
        let target = person.path().join("bashrc");
        std::fs::write(&target, "mine").unwrap();
        symlink(&target, tmp.path().join("token")).unwrap();
        let dir = Dir::open(tmp.path()).unwrap();
        dir.remove("token").unwrap();
        dir.remove("token").unwrap();
        assert!(target.exists());
        assert!(!tmp.path().join("token").exists());
    }

    #[test]
    fn sub_makes_a_folder_and_opens_an_old_one() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = Dir::open(tmp.path()).unwrap();
        let sub = dir.sub("forge", Some(0o700)).unwrap();
        sub.put("t", b"x", 0o600).unwrap();
        let again = dir.sub("forge", Some(0o700)).unwrap();
        assert_eq!(again.read("t").unwrap(), "x");
        assert_eq!(again.path(), tmp.path().join("forge"));
        assert!(dir.sub("none", None).is_err());
    }
}
