//! The `riff` binary on disk, and how a long run follows an update.
//!
//! # Design
//!
//! `riff update` puts a new binary on disk. A process that runs keeps
//! the old one. `riff watch` and `riff tail` run for hours, so each one
//! looks at its binary on disk every [`POLL`]. When the file changed and
//! then stayed the same for one more poll, the process runs the new
//! binary with the same arguments, in place, with `exec`
//! (01M3MNVTC248YYJJQKFD9H1WY9). The process ID stays, so the task of
//! Claude Code that runs the watch goes on. The lock of the watch is on
//! a file with close-on-exec, so the new process takes it again.
//!
//! The new binary reads its working directory at its start. A long run
//! can outlive that directory, for example a watch that started in a
//! worktree which the session removed later. So before the `exec`, a
//! process whose working directory is gone moves to the nearest parent
//! of it that exists (01M3NJGD45GF7Y4CZWQ7GRDHZN). In a worktree of
//! `.claude/worktrees`, that is a directory of the main worktree: the
//! same repository.
//!
//! `riff mcp` cannot exec: Claude Code talks to it over stdio, and the
//! new process has none of the state of the session. So at its next
//! tool call it replies with [`MCP_NEW`] and exits
//! (01M3MNVTE6GAK4WRSCFGYVS0BE).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant U as riff update
//!     participant D as the binary on disk
//!     participant W as riff watch
//!     U->>D: a new binary
//!     loop every POLL
//!         W->>D: the same file?
//!     end
//!     W->>W: exec the new binary, same arguments
//! ```

use std::ffi::OsString;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How often a long run looks at its binary on disk.
pub const POLL: Duration = Duration::from_secs(1);

/// The reply of `riff mcp` after an update (01M3MNVTE6GAK4WRSCFGYVS0BE).
pub const MCP_NEW: &str = "riff: riff was updated on this machine. This riff MCP server stops \
now, so that Claude Code can start the new one. Tell your user to run /mcp and reconnect the \
riff server. Then call the tool again.";

/// What tells two files apart: the inode, the size and the time of the
/// last change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    inode: u64,
    len: u64,
    changed: i64,
    changed_ns: i64,
}

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Stamp {
        inode: meta.ino(),
        len: meta.len(),
        changed: meta.mtime(),
        changed_ns: meta.mtime_nsec(),
    })
}

/// The binary of this process, as it was on disk at the start.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("riff");
/// std::fs::write(&path, "old")?;
/// let binary = riff::binary::Binary::at(&path);
/// assert!(!binary.changed());
/// // A new file in its place, as `cargo install` does.
/// std::fs::write(dir.path().join("new"), "new binary")?;
/// std::fs::rename(dir.path().join("new"), &path)?;
/// assert!(binary.changed());
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Clone, Debug)]
pub struct Binary {
    path: PathBuf,
    start: Option<Stamp>,
}

impl Binary {
    /// The binary of this process. `None` when the OS does not name it.
    pub fn this() -> Option<Binary> {
        std::env::current_exe().ok().map(|path| Binary::at(&path))
    }

    /// The binary at `path`, as it is now.
    pub fn at(path: &Path) -> Binary {
        Binary {
            path: path.to_owned(),
            start: stamp(path),
        }
    }

    /// True when another file is on disk now: a new binary. A missing
    /// file is no new binary.
    pub fn changed(&self) -> bool {
        stamp(&self.path).is_some_and(|now| Some(now) != self.start)
    }

    /// Waits until a new binary is on disk, and has stayed the same for
    /// one [`POLL`], so that its write is done.
    pub async fn new_one(&self) {
        let mut seen = None;
        loop {
            tokio::time::sleep(POLL).await;
            let now = stamp(&self.path);
            if now.is_some() && now != self.start && now == seen {
                return;
            }
            seen = now;
        }
    }

    /// Runs the binary on disk in place of this process, with `args`.
    /// It returns only on an error.
    pub fn exec(&self, args: impl IntoIterator<Item = OsString>) -> std::io::Error {
        let mut command = std::process::Command::new(&self.path);
        if let Ok(dir) = std::env::current_dir() {
            command.env("PWD", dir);
        }
        command.args(args).exec()
    }
}

/// The nearest directory that exists: `dir` or one of its parents.
///
/// ```
/// use riff::binary::nearest_dir;
///
/// let root = tempfile::tempdir()?;
/// let gone = root.path().join("worktrees/issue-12/src");
/// assert_eq!(nearest_dir(&gone), Some(root.path()));
/// std::fs::create_dir_all(&gone)?;
/// assert_eq!(nearest_dir(&gone), Some(gone.as_path()));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn nearest_dir(dir: &Path) -> Option<&Path> {
    dir.ancestors().find(|d| d.is_dir())
}

/// When the working directory is gone, moves this process to the
/// nearest directory of `start` that exists, and says so on stderr
/// (01M3NJGD45GF7Y4CZWQ7GRDHZN). `start` is the working directory at the
/// start of the long run.
fn leave_a_gone_dir(start: Option<&Path>) {
    if std::env::current_dir().is_ok() {
        return;
    }
    let Some(start) = start else {
        return eprintln!("riff: the working directory is gone.");
    };
    let Some(dir) = nearest_dir(start) else {
        return eprintln!("riff: the working directory {} is gone.", start.display());
    };
    match std::env::set_current_dir(dir) {
        Ok(()) => eprintln!(
            "riff: the working directory {} is gone. The new riff runs in {}.",
            start.display(),
            dir.display()
        ),
        Err(e) => eprintln!("riff: cannot change to {}: {e}", dir.display()),
    }
}

/// Waits for a new binary, then runs it in place of this process with
/// the same arguments (01M3MNVTC248YYJJQKFD9H1WY9), in a directory that
/// exists (01M3NJGD45GF7Y4CZWQ7GRDHZN). It never returns when
/// the OS does not name the binary of this process.
pub async fn follow_update() {
    let Some(binary) = Binary::this() else {
        return std::future::pending().await;
    };
    let start = std::env::current_dir().ok();
    binary.new_one().await;
    eprintln!("riff: a new riff is on disk. riff runs it now.");
    leave_a_gone_dir(start.as_deref());
    let error = binary.exec(std::env::args_os().skip(1));
    eprintln!("riff: cannot run the new riff: {error}");
    std::future::pending().await
}
