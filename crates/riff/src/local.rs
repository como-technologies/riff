//! Files on this machine that keep one session ID for all the riff
//! processes of an agent session.
//!
//! # Why
//!
//! `/clear` gives a Claude Code session a new session ID. Each process
//! that Claude Code starts after `/clear` gets the new ID. But
//! `riff mcp` keeps its process and its old ID, and the tools act as
//! the old ID. So `riff mcp` records its ID here. `riff watch`, the
//! hooks and the `riff` commands of the session use the recorded ID
//! (R58, R167).
//!
//! ```mermaid
//! flowchart LR
//!     C[Claude Code, PID 42] --> M[riff mcp]
//!     C --> B[shell] --> W[riff watch]
//!     C --> H[riff hook session-start]
//!     M -- "writes the session ID, holds the lock" --> F[(mcp-42)]
//!     W -- "finds PID 42 above it, reads" --> F
//!     H -- "finds PID 42 above it, reads" --> F
//! ```
//!
//! # Files
//!
//! | File | Written by | Holds |
//! |---|---|---|
//! | `mcp-PID` | `riff mcp`. PID is its parent: the agent tool. | The session ID. |
//! | `watch-ID` | `riff watch` for the session ID. | Nothing. Only the lock counts. |
//! | `next-ID` | `riff workers next` of a worker. The Stop hook takes it (see [`crate::next`]). | The tmux pane of the worker. |
//! | `left-ID` | The `leave` tool. The `join` tool removes it. | Nothing. The file counts. |
//! | `update.lock` | The update of riff by itself (see [`crate::auto_update`]). | Nothing. Only the lock counts. |
//! | `update-tried` | The same update. | The release tag that it tried last. |
//! | `update.log` | The same update. | Its output. |
//! | `host-USER-REPO` | `riff workers host` of USER in REPO (see [`crate::host`]). | Its PID and its session ID. |
//!
//! The writers of `mcp-PID`, `watch-ID`, `update.lock` and
//! `host-USER-REPO` hold a lock on
//! the file while they run. The system ends
//! the lock when the process ends, also after a crash. A file with no
//! lock is stale, and riff ignores it. So a PID that the system gives
//! to a new process again does not find a stale session.
//!
//! The files are in [`dir`]. A reader looks for `mcp-PID` of each
//! process above it, nearest first, up to [`MAX_DEPTH`] processes.
//!
//! ```
//! let run = tempfile::tempdir()?;
//! let agent = std::process::id();
//! let mcp = riff::local::record(run.path(), agent, "a6cf")?.expect("free");
//! assert_eq!(riff::local::recorded(run.path(), agent).as_deref(), Some("a6cf"));
//! drop(mcp);
//! assert_eq!(riff::local::recorded(run.path(), agent), None);
//! # Ok::<(), std::io::Error>(())
//! ```

use std::ffi::OsString;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The most processes above this one that a reader looks at.
pub const MAX_DEPTH: usize = 8;

/// A lock on a file. It ends when this value drops or the process ends.
///
/// The lock belongs to the open file, not to this value. A child process
/// that another thread starts gets a copy of the open file, and holds the
/// lock until it runs its program. So the lock can outlive the drop for a
/// short time.
#[derive(Debug)]
pub struct Held(File);

/// The directory of the files: `$RIFF_HOME/state` (see
/// [`home`](crate::home)), else `$XDG_RUNTIME_DIR/riff`, else
/// `$XDG_STATE_HOME/riff`, else `$HOME/.local/state/riff`. `None`
/// without `HOME`.
pub fn dir() -> Option<PathBuf> {
    if let Some(home) = crate::home::dir() {
        return Some(home.join("state"));
    }
    dir_from(
        std::env::var_os("XDG_RUNTIME_DIR"),
        std::env::var_os("XDG_STATE_HOME"),
        std::env::var_os("HOME"),
    )
}

/// [`dir`] from the values of `XDG_RUNTIME_DIR`, `XDG_STATE_HOME` and
/// `HOME`. An empty value counts as unset.
///
/// ```
/// use riff::local::dir_from;
/// use std::path::PathBuf;
///
/// let some = |s: &str| Some(s.into());
/// assert_eq!(
///     dir_from(some("/run/user/1000"), some("/s"), some("/h")),
///     Some(PathBuf::from("/run/user/1000/riff"))
/// );
/// assert_eq!(dir_from(None, some("/s"), some("/h")), Some(PathBuf::from("/s/riff")));
/// assert_eq!(dir_from(some(""), None, some("/h")), Some(PathBuf::from("/h/.local/state/riff")));
/// assert_eq!(dir_from(None, None, None), None);
/// ```
pub fn dir_from(
    runtime: Option<OsString>,
    state: Option<OsString>,
    home: Option<OsString>,
) -> Option<PathBuf> {
    let set = |v: Option<OsString>| v.filter(|v| !v.is_empty()).map(PathBuf::from);
    set(runtime)
        .or_else(|| set(state))
        .or_else(|| set(home).map(|h| h.join(".local").join("state")))
        .map(|d| d.join("riff"))
}

/// Records `session` as the session of the agent process `agent`, and
/// holds the record while the result lives. `None` when a live process
/// holds the record of `agent` already.
pub fn record(dir: &Path, agent: u32, session: &str) -> io::Result<Option<Held>> {
    let Some(Held(mut file)) = lock(&dir.join(format!("mcp-{agent}")))? else {
        return Ok(None);
    };
    file.set_len(0)?;
    file.write_all(session.as_bytes())?;
    Ok(Some(Held(file)))
}

/// The session that a live process recorded for the agent process
/// `agent`.
pub fn recorded(dir: &Path, agent: u32) -> Option<String> {
    let mut file = File::open(dir.join(format!("mcp-{agent}"))).ok()?;
    if !locked(&file) {
        return None;
    }
    let mut session = String::new();
    file.read_to_string(&mut session).ok()?;
    Some(session.trim().to_owned()).filter(|s| !s.is_empty())
}

/// The session recorded for the nearest process above this one.
pub fn recorded_above(dir: &Path) -> Option<String> {
    ancestors().find_map(|pid| recorded(dir, pid))
}

/// Takes the watch lock of `session`. `None` when another watch holds it
/// (R169).
pub fn watch(dir: &Path, session: &str) -> io::Result<Option<Held>> {
    lock(&watch_file(dir, session))
}

/// True while a watch holds the lock of `session`.
///
/// ```
/// let run = tempfile::tempdir()?;
/// assert!(!riff::local::watching(run.path(), "a6cf"));
/// let watch = riff::local::watch(run.path(), "a6cf")?.expect("free");
/// assert!(riff::local::watching(run.path(), "a6cf"));
/// assert!(riff::local::watch(run.path(), "a6cf")?.is_none());
/// drop(watch);
/// assert!(!riff::local::watching(run.path(), "a6cf"));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn watching(dir: &Path, session: &str) -> bool {
    File::open(watch_file(dir, session)).is_ok_and(|file| locked(&file))
}

fn watch_file(dir: &Path, session: &str) -> PathBuf {
    dir.join(format!("watch-{}", riff_core::name::sanitize(session)))
}

/// Takes the update lock of this machine
/// (01M3N7JJH0SXXQYYBAHWPCNQGX). `None` when another update holds it.
///
/// ```
/// let run = tempfile::tempdir()?;
/// assert!(!riff::local::updating(run.path()));
/// let update = riff::local::update(run.path())?.expect("free");
/// assert!(riff::local::updating(run.path()));
/// assert!(riff::local::update(run.path())?.is_none());
/// drop(update);
/// assert!(!riff::local::updating(run.path()));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn update(dir: &Path) -> io::Result<Option<Held>> {
    lock(&dir.join("update.lock"))
}

/// True while an update of riff holds the update lock.
pub fn updating(dir: &Path) -> bool {
    File::open(dir.join("update.lock")).is_ok_and(|file| locked(&file))
}

/// Takes the lock of the workers host of `user` in `repo` on this
/// machine, and writes `pid` and `session` in it
/// (01M3NBV44GKAX6WS391PN6R72W). `Err` when another host holds it,
/// with the PID and the session of that host.
///
/// ```
/// let run = tempfile::tempdir()?;
/// let first = riff::local::host(run.path(), "mike", "como/riff", 42, "h1")?.expect("free");
/// assert_eq!(
///     riff::local::host(run.path(), "mike", "como/riff", 43, "h2")?.unwrap_err(),
///     "42 h1"
/// );
/// assert!(riff::local::host(run.path(), "brett", "como/riff", 43, "h2")?.is_ok());
/// drop(first);
/// assert!(riff::local::host(run.path(), "mike", "como/riff", 43, "h2")?.is_ok());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn host(
    dir: &Path,
    user: &str,
    repo: &str,
    pid: u32,
    session: &str,
) -> io::Result<Result<Held, String>> {
    let path = dir.join(format!(
        "host-{}-{}",
        riff_core::name::sanitize(user),
        riff_core::name::sanitize(repo)
    ));
    let Some(Held(mut file)) = lock(&path)? else {
        return Ok(Err(std::fs::read_to_string(&path)?.trim().to_owned()));
    };
    file.set_len(0)?;
    write!(file, "{pid} {session}")?;
    Ok(Ok(Held(file)))
}

/// The release tag that the last update of riff by itself tried.
///
/// ```
/// let run = tempfile::tempdir()?;
/// assert_eq!(riff::local::tried(run.path()), None);
/// riff::local::set_tried(run.path(), "v0.4.0")?;
/// assert_eq!(riff::local::tried(run.path()).as_deref(), Some("v0.4.0"));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn tried(dir: &Path) -> Option<String> {
    let tag = std::fs::read_to_string(dir.join("update-tried")).ok()?;
    Some(tag.trim().to_owned()).filter(|t| !t.is_empty())
}

/// Records `tag` as the release that the update of riff by itself tries.
pub fn set_tried(dir: &Path, tag: &str) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("update-tried"), tag)
}

/// The log of the update of riff by itself.
pub fn update_log(dir: &Path) -> PathBuf {
    dir.join("update.log")
}

/// Records that the session `session` left the riff
/// (01M3MEEFC9ZQVW2KC9FNJ75MTY). The record has no lock: it outlives
/// `riff mcp`, so it holds over a resume and `/clear`.
///
/// ```
/// let run = tempfile::tempdir()?;
/// assert!(!riff::local::left(run.path(), "a6cf"));
/// riff::local::leave(run.path(), "a6cf")?;
/// assert!(riff::local::left(run.path(), "a6cf"));
/// assert!(!riff::local::left(run.path(), "b2"));
/// riff::local::join(run.path(), "a6cf")?;
/// assert!(!riff::local::left(run.path(), "a6cf"));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn leave(dir: &Path, session: &str) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(left_file(dir, session), "")
}

/// Removes the record of [`leave`]. A session with no record is in the
/// riff already.
pub fn join(dir: &Path, session: &str) -> io::Result<()> {
    match std::fs::remove_file(left_file(dir, session)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// True when the session `session` left the riff.
pub fn left(dir: &Path, session: &str) -> bool {
    left_file(dir, session).exists()
}

/// True when the session `session` left the riff, with the files in
/// [`dir`]. False with no directory.
pub fn left_here(session: &str) -> bool {
    dir().is_some_and(|dir| left(&dir, session))
}

fn left_file(dir: &Path, session: &str) -> PathBuf {
    dir.join(format!("left-{}", riff_core::name::sanitize(session)))
}

/// Opens `path` and takes its lock. `None` when another open file holds
/// the lock.
fn lock(path: &Path) -> io::Result<Option<Held>> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(Held(file))),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

/// True when another open file holds the lock of `file`.
fn locked(file: &File) -> bool {
    match file.try_lock_shared() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(TryLockError::WouldBlock) => true,
        Err(TryLockError::Error(_)) => false,
    }
}

/// The processes above this one, nearest first. It stops before the
/// first process of the system.
fn ancestors() -> impl Iterator<Item = u32> {
    std::iter::successors(Some(std::os::unix::process::parent_id()), |&pid| {
        parent_of(pid)
    })
    .take_while(|&pid| pid > 1)
    .take(MAX_DEPTH)
}

/// The parent of `pid`: from `/proc` on Linux, from `ps` elsewhere.
fn parent_of(pid: u32) -> Option<u32> {
    if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        return parent_in_stat(&stat);
    }
    let out = Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// The parent PID in the text of `/proc/PID/stat`. The command name can
/// hold spaces and parentheses, so the fields start after the last `)`.
///
/// ```
/// use riff::local::parent_in_stat;
///
/// assert_eq!(parent_in_stat("1234 (riff) S 42 1234 1234 0 -1"), Some(42));
/// assert_eq!(parent_in_stat("1234 (a) b (c) S 7 1 1"), Some(7));
/// assert_eq!(parent_in_stat("garbage"), None);
/// ```
pub fn parent_in_stat(stat: &str) -> Option<u32> {
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    /// [`record`] when the lock is free. Other tests start child
    /// processes, so a dropped lock can live on for a short time (see
    /// [`Held`]).
    fn record_when_free(dir: &Path, agent: u32, session: &str) -> Held {
        let begin = Instant::now();
        loop {
            if let Some(held) = record(dir, agent, session).unwrap() {
                return held;
            }
            assert!(begin.elapsed() < Duration::from_secs(10), "no free lock");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn a_second_record_for_one_agent_waits_for_the_first() {
        let run = tempfile::tempdir().unwrap();
        let first = record_when_free(run.path(), 42, "a");
        assert!(record(run.path(), 42, "b").unwrap().is_none());
        assert_eq!(recorded(run.path(), 42).as_deref(), Some("a"));
        drop(first);
        let second = record_when_free(run.path(), 42, "b");
        assert_eq!(recorded(run.path(), 42).as_deref(), Some("b"));
        drop(second);
    }

    #[test]
    fn a_record_is_free_again_while_other_threads_start_processes() {
        let stop = Arc::new(AtomicBool::new(false));
        let spawner = std::thread::spawn({
            let stop = stop.clone();
            move || {
                while !stop.load(Ordering::Relaxed) {
                    let _ = Command::new("true").status();
                }
            }
        });
        let run = tempfile::tempdir().unwrap();
        for _ in 0..500 {
            drop(record_when_free(run.path(), 42, "a"));
            drop(record_when_free(run.path(), 42, "b"));
        }
        stop.store(true, Ordering::Relaxed);
        spawner.join().unwrap();
    }

    #[test]
    fn a_shorter_session_replaces_a_longer_one() {
        let run = tempfile::tempdir().unwrap();
        drop(record_when_free(run.path(), 42, "a-long-session"));
        let _held = record_when_free(run.path(), 42, "b");
        assert_eq!(recorded(run.path(), 42).as_deref(), Some("b"));
    }

    #[test]
    fn a_stale_record_is_ignored() {
        let run = tempfile::tempdir().unwrap();
        std::fs::write(run.path().join("mcp-42"), "old").unwrap();
        assert_eq!(recorded(run.path(), 42), None);
    }

    #[test]
    fn records_are_found_above_this_process() {
        let run = tempfile::tempdir().unwrap();
        assert_eq!(recorded_above(run.path()), None);
        let parent = std::os::unix::process::parent_id();
        let _held = record(run.path(), parent, "a6cf").unwrap().unwrap();
        assert_eq!(recorded_above(run.path()).as_deref(), Some("a6cf"));
    }

    #[test]
    fn the_parent_of_this_process_is_known() {
        assert_eq!(
            parent_of(std::process::id()),
            Some(std::os::unix::process::parent_id())
        );
    }

    #[test]
    fn a_session_id_cannot_leave_the_directory() {
        let run = tempfile::tempdir().unwrap();
        let _held = watch(run.path(), "../x").unwrap().unwrap();
        assert!(run.path().join("watch-..-x").exists());
    }
}
