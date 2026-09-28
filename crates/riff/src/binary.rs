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
        std::process::Command::new(&self.path).args(args).exec()
    }
}

/// Waits for a new binary, then runs it in place of this process with
/// the same arguments (01M3MNVTC248YYJJQKFD9H1WY9). It never returns when
/// the OS does not name the binary of this process.
pub async fn follow_update() {
    let Some(binary) = Binary::this() else {
        return std::future::pending().await;
    };
    binary.new_one().await;
    eprintln!("riff: a new riff is on disk. riff runs it now.");
    let error = binary.exec(std::env::args_os().skip(1));
    eprintln!("riff: cannot run the new riff: {error}");
    std::future::pending().await
}
