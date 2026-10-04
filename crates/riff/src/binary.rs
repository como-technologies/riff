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
//! The new binary keeps the place of the old one: the old process gives
//! its place (host, repository and worktree) in the hidden argument
//! [`crate::identity::PLACE_ARG`], and the new one reads it there, not
//! from its directory (01M3NJGD45GF7Y4CZWQ7GRDHZN). An argument, not a
//! variable, so that no process that the new binary starts inherits
//! the place. A long run can
//! outlive its working directory, for example a watch that started in a
//! worktree which the session removed later. So before the `exec`, a
//! process whose working directory is gone also moves to the nearest
//! parent of it that exists, where each process that it starts works.
//!
//! `riff top` follows an update the same way (01M3NT6WXGCNKW3EQ7MBJDQTR4).
//! `riff workers host` does it between two requests of the lead, and
//! gives the new process its session (01M3Q55KJ8BKMPE9RADB63X8SP).
//! `riff chat` and `riff mcp`
//! do too, but only at a moment with no work in flight, and they give
//! the new process their state in a hidden option (see [`with_last`]):
//! the chat its last line, so that it shows each line once, and
//! `riff mcp` its client, so that the connection to Claude Code stays
//! (01M3NT6WZTKAFKGDWGCFKC8TB5). The new process keeps stdin, stdout
//! and stderr. A `riff mcp` that dies takes the tools of its session
//! with it, so `riff mcp` first runs the new binary once as a check
//! ([`Binary::new_one_that`], 01M43F5F9AQ9S39E1JZF8EBJEH). It runs it
//! in its place only when the check passes.
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
use std::future::Future;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use riff_core::name::Place;

use crate::identity;

/// How often a long run looks at its binary on disk.
pub const POLL: Duration = Duration::from_secs(1);

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
        self.next(None).await;
    }

    /// Waits until a new binary is on disk that passes `check`
    /// (01M43F5F9AQ9S39E1JZF8EBJEH). A binary that fails it stays
    /// refused: this waits for the next one, and says the error once on
    /// stderr.
    pub async fn new_one_that<F, Fut>(&self, check: F)
    where
        F: Fn(PathBuf) -> Fut,
        Fut: Future<Output = Result<(), String>>,
    {
        let mut refused = None;
        loop {
            let new = self.next(refused).await;
            match check(self.path.clone()).await {
                Ok(()) => return,
                Err(e) => {
                    eprintln!("{}", crate::text::new_riff_refused(&e));
                    refused = Some(new);
                }
            }
        }
    }

    /// The stamp of the next new binary on disk: not the one of the
    /// start, not `refused`, and the same for one [`POLL`].
    async fn next(&self, refused: Option<Stamp>) -> Stamp {
        let mut seen = None;
        loop {
            tokio::time::sleep(POLL).await;
            let now = stamp(&self.path);
            if let Some(new) = now
                && now != self.start
                && now != refused
                && now == seen
            {
                return new;
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

/// The path of the `riff` binary on disk, for a process that this one
/// starts. After `cargo install` put a new binary in place, Linux names
/// the binary of a process that still runs `PATH (deleted)`. That path
/// does not exist, so this gives `PATH`: the new binary
/// (01M3Q55KMQSSJVQEN86XFB8PSG).
///
/// ```
/// use std::path::{Path, PathBuf};
/// use riff::binary::on_disk;
///
/// let bin = Path::new("/home/mike/.cargo/bin/riff");
/// assert_eq!(on_disk(PathBuf::from("/home/mike/.cargo/bin/riff (deleted)")), bin);
/// assert_eq!(on_disk(bin.to_owned()), bin);
/// ```
pub fn on_disk(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|p| p.strip_suffix(" (deleted)")) {
        Some(path) => PathBuf::from(path),
        None => path,
    }
}

/// [`on_disk`] for the binary of this process.
pub fn this_on_disk() -> std::io::Result<PathBuf> {
    std::env::current_exe().map(on_disk)
}

/// `args` with [`identity::PLACE_ARG`] and `place` first. It drops the
/// place of an earlier update.
///
/// ```
/// use std::ffi::OsString;
/// use riff::binary::with_place;
/// use riff_core::name::{Place, Repo};
///
/// let repo = Repo::Git { owner: "acme".into(), name: "alpha".into() };
/// let place = Place::new("thelio", repo, Some("issue-12"))?;
/// let args = |a: &[&str]| a.iter().map(OsString::from).collect::<Vec<_>>();
/// let want = args(&["--place", "thelio/acme/alpha#issue-12", "watch", "--once"]);
/// assert_eq!(with_place(args(&["watch", "--once"]), &place), want);
/// let again = args(&["--place", "old/acme/beta", "watch", "--once"]);
/// assert_eq!(with_place(again, &place), want);
/// let joined = args(&["--place=old/acme/beta", "watch", "--once"]);
/// assert_eq!(with_place(joined, &place), want);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn with_place(args: impl IntoIterator<Item = OsString>, place: &Place) -> Vec<OsString> {
    let mut out = vec![
        identity::PLACE_ARG.into(),
        identity::place_text(place).into(),
    ];
    out.extend(without(args, identity::PLACE_ARG));
    out
}

/// `args` with the option `name` and `value` last. It drops the value
/// of an earlier update. The old process gives the new one its state in
/// such a hidden option, for example the last line that the chat showed.
///
/// ```
/// use std::ffi::OsString;
/// use riff::binary::with_last;
///
/// let args = |a: &[&str]| a.iter().map(OsString::from).collect::<Vec<_>>();
/// let want = args(&["chat", "--after", "7"]);
/// assert_eq!(with_last(args(&["chat"]), "--after", "7"), want);
/// assert_eq!(with_last(args(&["chat", "--after", "3"]), "--after", "7"), want);
/// assert_eq!(with_last(args(&["chat", "--after=3"]), "--after", "7"), want);
/// ```
pub fn with_last(
    args: impl IntoIterator<Item = OsString>,
    name: &str,
    value: impl Into<OsString>,
) -> Vec<OsString> {
    let mut out = without(args, name);
    out.extend([name.into(), value.into()]);
    out
}

/// `args` with no option `name`, as `name VALUE` or `name=VALUE`.
fn without(args: impl IntoIterator<Item = OsString>, name: &str) -> Vec<OsString> {
    let joined = format!("{name}=");
    let mut out = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == name {
            args.next();
        } else if !arg.to_string_lossy().starts_with(&joined) {
            out.push(arg);
        }
    }
    out
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

/// A long run that follows an update: it waits for a new binary, then
/// runs it in place of this process (01M3MNVTC248YYJJQKFD9H1WY9).
#[derive(Debug)]
pub struct Follow {
    binary: Option<Binary>,
    /// The working directory at the start of the long run.
    start: Option<PathBuf>,
}

impl Follow {
    /// Follows the binary of this process, from the current directory.
    pub fn this() -> Follow {
        Follow {
            binary: Binary::this(),
            start: std::env::current_dir().ok(),
        }
    }

    /// Waits until a new binary is on disk (see [`Binary::new_one`]). It
    /// never returns when the OS does not name the binary of this
    /// process.
    pub async fn new_one(&self) {
        match &self.binary {
            Some(binary) => binary.new_one().await,
            None => std::future::pending().await,
        }
    }

    /// Waits until a new binary is on disk that passes `check` (see
    /// [`Binary::new_one_that`]). It never returns when the OS does not
    /// name the binary of this process.
    pub async fn new_one_that<F, Fut>(&self, check: F)
    where
        F: Fn(PathBuf) -> Fut,
        Fut: Future<Output = Result<(), String>>,
    {
        match &self.binary {
            Some(binary) => binary.new_one_that(check).await,
            None => std::future::pending().await,
        }
    }

    /// Runs the new binary in place of this process with `args`, in a
    /// directory that exists (01M3NJGD45GF7Y4CZWQ7GRDHZN). It returns
    /// only on an error, and says so on stderr.
    pub fn run(&self, args: Vec<OsString>) {
        let Some(binary) = &self.binary else { return };
        eprintln!("riff: a new riff is on disk. riff runs it now.");
        leave_a_gone_dir(self.start.as_deref());
        let error = binary.exec(args);
        eprintln!("riff: cannot run the new riff: {error}");
    }
}

/// Waits for a new binary, then runs it in place of this process with
/// the same arguments (01M3MNVTC248YYJJQKFD9H1WY9), the same `place`,
/// and in a directory that exists (01M3NJGD45GF7Y4CZWQ7GRDHZN). It never
/// returns.
pub async fn follow_update(place: &Place) {
    let follow = Follow::this();
    follow.new_one().await;
    follow.run(with_place(std::env::args_os().skip(1), place));
    std::future::pending().await
}
