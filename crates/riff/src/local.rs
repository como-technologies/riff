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
//! | `build-PID` | The same `riff mcp`, next to `mcp-PID`. | The build of that `riff mcp`. |
//! | `start-PID` | The same `riff mcp`, next to `mcp-PID`. | The start time of the agent process PID, from `/proc`. |
//! | `stuck-PID` | The PostToolUse hook of the session of the agent process PID, when it told the session that its `riff mcp` is stuck on an old riff (see [`first_tell`]). | The inode of the new riff. |
//! | `notools-PID` | The `riff mcp` of the agent process PID, when its tools refused the `tools/list` of Claude Code (see [`set_no_tools`]). It removes the file after a list that works. | The start time of the agent process, and the time of the mark. |
//! | `notools-told-PID` | The hooks of the session of the agent process PID, when they told the session that it has no riff tools (see [`first_no_tools_tell`]). | The mark that they told. |
//! | `watch-ID` | `riff watch` for the session ID. | Nothing. Only the lock counts. |
//! | `statusline-ID` | `riff statusline` for the session ID ([`keep_statusline`]). | The last answer of riff-server: its URL, its time, its build and the line. |
//! | `left-ID` | The `leave` tool. The `join` tool removes it. It is in [`marks`], not in [`dir`]. | Nothing. The file counts. |
//! | `update.lock` | The update of riff by itself (see [`crate::auto_update`]). | Nothing. Only the lock counts. |
//! | `update-tried` | The same update. | The release tag that it tried last. |
//! | `update.log` | The same update. | Its output. |
//! | `needs-you` | The riff daemon at each start, and `riff update --check` (see [`crate::update_steps`]). | The first step of an update that only a person can do, with its fix. No file when there is none. |
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
//! The folder is private: mode 0700, and each file in it mode 0600
//! (01M56RB1DQ396DQ75Q23G87C28). A writer uses [`write()`], [`make_dir`]
//! or [`options`], from [`riff_client::files`]. Each riff process gives
//! a wider mode 0700 or 0600 at its start ([`secure_here`]).
//!
//! # An agent inside a session
//!
//! A Bash command of a session can start a second agent, for example
//! `claude -p`. That agent finds the record of the session above it,
//! so it would act as the session: its start hook would free the
//! claims of the session, and its end hook would end the session. So
//! an agent with a live record above its own process is no session
//! of the riff ([`outer`], 01M517N75C0Z5BAFQAWRN3YTYB). Its hooks
//! and its `riff mcp` act for no session.
//!
//! ```mermaid
//! flowchart LR
//!     C[Claude Code, PID 42] --> M[riff mcp] -- "holds" --> F[(mcp-42)]
//!     C --> B[shell] --> N[claude -p, PID 77]
//!     N --> H[riff hook session-start]
//!     H -- "PID 42 is above PID 77: no start" --> F
//! ```
//!
//! ```standalone_crate
//! let run = tempfile::tempdir()?;
//! let agent = std::process::id();
//! let mcp = riff::local::record(run.path(), agent, "a6cf")?.expect("free");
//! assert_eq!(riff::local::recorded(run.path(), agent).as_deref(), Some("a6cf"));
//! drop(mcp);
//! assert_eq!(riff::local::recorded(run.path(), agent), None);
//! # Ok::<(), std::io::Error>(())
//! ```

use std::ffi::OsString;
use std::fs::{File, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

pub use riff_client::files::{make_dir, options, private, secure, write};

/// The most processes above this one that a reader looks at.
pub const MAX_DEPTH: usize = 8;

/// A lock on a file. It ends when this value drops or the process ends.
///
/// The lock belongs to the open file, not to this value. A child process
/// that another thread starts gets a copy of the open file, and holds the
/// lock until it runs its program. So the lock can outlive the drop for a
/// short time.
///
/// Through a cargo test runner, rustdoc runs the merged doc tests as
/// threads of one process. So each doc test that drops a lock and then
/// checks that the lock is free is a `standalone_crate`: it runs in a
/// process of its own, with no other thread. A unit test waits for the
/// end of the lock (01M513N34NJ2G3TK3EH91M10FJ).
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

/// Gives the local folders of riff ([`dir`] and [`marks`]) mode 0700,
/// and each file in them mode 0600 (01M56RB1FZ575KQYHW9FF716CZ). Each
/// riff process calls it at its start. A folder that does not exist
/// stays so. The error names the folder that riff cannot fix.
pub fn secure_here() -> Result<(), String> {
    for dir in [dir(), marks()].into_iter().flatten() {
        if dir.exists() {
            secure(&dir).map_err(|e| crate::text::local_mode(&dir, &e.to_string()))?;
        }
    }
    Ok(())
}

/// Records `session` as the session of the agent process `agent`, and
/// holds the record while the result lives. `None` when a live process
/// holds the record of `agent` already. It also records the build of
/// this riff for the agent ([`recorded_build`]).
pub fn record(dir: &Path, agent: u32, session: &str) -> io::Result<Option<Held>> {
    let Some(Held(mut file)) = lock(&dir.join(format!("mcp-{agent}")))? else {
        return Ok(None);
    };
    file.set_len(0)?;
    file.write_all(session.as_bytes())?;
    write(
        &dir.join(format!("build-{agent}")),
        riff_core::build::VERSION,
    )?;
    if let Some(start) = started(agent) {
        write(&dir.join(format!("start-{agent}")), start.to_string())?;
    }
    Ok(Some(Held(file)))
}

/// True when the agent process `agent` had a `riff mcp` that ended,
/// and still runs (01M43F5KE7G2A2A9PSVRJPPNET): its `mcp-PID` has no
/// lock, and `start-PID` holds the start time of this same process,
/// not of an earlier process with the same PID. False when no
/// `riff mcp` ever ran for it, and on a system with no `/proc`.
///
/// ```standalone_crate
/// let run = tempfile::tempdir()?;
/// let agent = std::process::id();
/// assert!(!riff::local::mcp_gone(run.path(), agent));
/// let mcp = riff::local::record(run.path(), agent, "a6cf")?.expect("free");
/// assert!(!riff::local::mcp_gone(run.path(), agent));
/// drop(mcp);
/// # if std::path::Path::new("/proc/self/stat").exists() {
/// assert!(riff::local::mcp_gone(run.path(), agent));
/// // An earlier process with the same PID.
/// std::fs::write(run.path().join(format!("start-{agent}")), "1")?;
/// assert!(!riff::local::mcp_gone(run.path(), agent));
/// # }
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn mcp_gone(dir: &Path, agent: u32) -> bool {
    let Ok(file) = File::open(dir.join(format!("mcp-{agent}"))) else {
        return false;
    };
    if locked(&file) {
        return false;
    }
    let recorded = std::fs::read_to_string(dir.join(format!("start-{agent}")));
    let recorded = recorded.ok().and_then(|r| r.trim().parse::<u64>().ok());
    recorded.is_some() && recorded == started(agent)
}

/// [`mcp_gone`] for the nearest process above this one with an
/// `mcp-PID` file.
pub fn mcp_gone_above(dir: &Path) -> bool {
    ancestors()
        .find(|pid| dir.join(format!("mcp-{pid}")).exists())
        .is_some_and(|pid| mcp_gone(dir, pid))
}

/// The start time of the process `pid`, in clock ticks after the boot.
/// `None` with no `/proc`.
fn started(pid: u32) -> Option<u64> {
    start_in_stat(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

/// The start time in the text of `/proc/PID/stat`: field 22.
///
/// ```
/// use riff::local::start_in_stat;
///
/// let stat = "1234 (a b) S 42 1234 1234 0 -1 4194560 1 0 0 0 0 0 0 0 20 0 1 0 98765 1 2";
/// assert_eq!(start_in_stat(stat), Some(98765));
/// assert_eq!(start_in_stat("garbage"), None);
/// ```
pub fn start_in_stat(stat: &str) -> Option<u64> {
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

/// The build of the live `riff mcp` of the agent process `agent`
/// (01M3NJCRVZW5BFQYZ9N185K2D2). `None` when no live `riff mcp` has a
/// record for it. `Some(None)` when it has, but names no build: a riff
/// older than this record.
///
/// ```standalone_crate
/// use riff_core::build::Build;
///
/// let run = tempfile::tempdir()?;
/// let agent = std::process::id();
/// assert_eq!(riff::local::recorded_build(run.path(), agent), None);
/// let mcp = riff::local::record(run.path(), agent, "a6cf")?.expect("free");
/// assert_eq!(riff::local::recorded_build(run.path(), agent), Some(Some(Build::this())));
/// std::fs::remove_file(run.path().join(format!("build-{agent}")))?;
/// assert_eq!(riff::local::recorded_build(run.path(), agent), Some(None));
/// drop(mcp);
/// assert_eq!(riff::local::recorded_build(run.path(), agent), None);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn recorded_build(dir: &Path, agent: u32) -> Option<Option<riff_core::build::Build>> {
    recorded(dir, agent)?;
    let build = std::fs::read_to_string(dir.join(format!("build-{agent}")));
    Some(build.ok().and_then(|b| b.trim().parse().ok()))
}

/// [`recorded_build`] for the nearest process above this one with a
/// live `riff mcp`.
pub fn recorded_build_above(dir: &Path) -> Option<Option<riff_core::build::Build>> {
    ancestors().find_map(|pid| recorded_build(dir, pid))
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

/// The nearest process above this one with a live `riff mcp`: the agent
/// process of the session.
pub fn agent_above(dir: &Path) -> Option<u32> {
    ancestors().find(|&pid| recorded(dir, pid).is_some())
}

/// True the first time that riff tells the session of the agent process
/// `agent` that its `riff mcp` is stuck on the replaced riff before the
/// new riff `inode` (01M53QQRAJMB24B0804YHPMN18). It keeps the inode in
/// `stuck-PID`, so each new riff gets one line.
///
/// ```
/// let run = tempfile::tempdir()?;
/// assert!(riff::local::first_tell(run.path(), 42, 7));
/// assert!(!riff::local::first_tell(run.path(), 42, 7));
/// assert!(riff::local::first_tell(run.path(), 42, 8));
/// assert!(riff::local::first_tell(run.path(), 43, 8));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn first_tell(dir: &Path, agent: u32, inode: u64) -> bool {
    first(&dir.join(format!("stuck-{agent}")), &inode.to_string())
}

/// True the first time that `file` gets `key`: it keeps `key`.
fn first(file: &Path, key: &str) -> bool {
    let told = std::fs::read_to_string(file).is_ok_and(|t| t.trim() == key);
    !told && write(file, key).is_ok()
}

/// Keeps the mark that the session of the agent process `agent` has no
/// riff tools: the tools refused the `tools/list` of Claude Code
/// (01M5512RKABQ70QPGC48S5F67W). `none` false removes the mark, after a
/// list that works. The mark holds the start time of the agent process
/// and the time of the mark, so a later process with the same PID finds
/// no mark, and each new mark is new to [`first_no_tools_tell`].
///
/// ```
/// let run = tempfile::tempdir()?;
/// let agent = std::process::id();
/// assert_eq!(riff::local::no_tools(run.path(), agent), None);
/// # if std::path::Path::new("/proc/self/stat").exists() {
/// riff::local::set_no_tools(run.path(), agent, true)?;
/// assert!(riff::local::no_tools(run.path(), agent).is_some());
/// riff::local::set_no_tools(run.path(), agent, false)?;
/// assert_eq!(riff::local::no_tools(run.path(), agent), None);
/// riff::local::set_no_tools(run.path(), agent, false)?;
///
/// // A mark of an earlier process with the same PID.
/// std::fs::write(run.path().join(format!("notools-{agent}")), "1 2")?;
/// assert_eq!(riff::local::no_tools(run.path(), agent), None);
/// # }
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn set_no_tools(dir: &Path, agent: u32, none: bool) -> io::Result<()> {
    let file = dir.join(format!("notools-{agent}"));
    if none {
        let start = started(agent).unwrap_or_default();
        let now = crate::usage::now_ms();
        return write(&file, format!("{start} {now}"));
    }
    match std::fs::remove_file(file) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        removed => removed,
    }
}

/// The mark of [`set_no_tools`] of the agent process `agent`, when it
/// is a mark of this same process.
pub fn no_tools(dir: &Path, agent: u32) -> Option<String> {
    let mark = std::fs::read_to_string(dir.join(format!("notools-{agent}"))).ok()?;
    let start = mark.split_whitespace().next()?.parse::<u64>().ok()?;
    (Some(start) == started(agent)).then(|| mark.trim().to_owned())
}

/// True the first time that riff tells the session of the agent process
/// `agent` that it has no riff tools, for each new mark of
/// [`set_no_tools`] (01M5512RKABQ70QPGC48S5F67W). It keeps the mark in
/// `notools-told-PID`.
///
/// ```
/// let run = tempfile::tempdir()?;
/// let agent = std::process::id();
/// assert!(!riff::local::first_no_tools_tell(run.path(), agent));
/// # if std::path::Path::new("/proc/self/stat").exists() {
/// riff::local::set_no_tools(run.path(), agent, true)?;
/// assert!(riff::local::first_no_tools_tell(run.path(), agent));
/// assert!(!riff::local::first_no_tools_tell(run.path(), agent));
/// # }
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn first_no_tools_tell(dir: &Path, agent: u32) -> bool {
    no_tools(dir, agent)
        .is_some_and(|mark| first(&dir.join(format!("notools-told-{agent}")), &mark))
}

/// The session of a live `riff mcp` of a process above the agent
/// process `agent`. Then `agent` runs inside that session, and it is
/// no session of the riff (01M517N75C0Z5BAFQAWRN3YTYB). The record of
/// `agent` itself does not count.
///
/// ```standalone_crate
/// let run = tempfile::tempdir()?;
/// let me = std::process::id();
/// let mut child = std::process::Command::new("sleep").arg("30").spawn()?;
/// assert_eq!(riff::local::outer(run.path(), child.id()), None);
///
/// // A live record of the agent itself: not inside a session.
/// let own = riff::local::record(run.path(), child.id(), "n7")?.expect("free");
/// assert_eq!(riff::local::outer(run.path(), child.id()), None);
/// drop(own);
///
/// // A live record of a process above the agent: inside that session.
/// let outer = riff::local::record(run.path(), me, "a6cf")?.expect("free");
/// assert_eq!(riff::local::outer(run.path(), child.id()).as_deref(), Some("a6cf"));
/// assert_eq!(riff::local::outer(run.path(), me), None);
/// drop(outer);
/// child.kill()?;
/// child.wait()?;
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn outer(dir: &Path, agent: u32) -> Option<String> {
    up_from(parent_of(agent)).find_map(|pid| recorded(dir, pid))
}

/// Takes the watch lock of `session`. `None` when another watch holds it
/// (R169).
pub fn watch(dir: &Path, session: &str) -> io::Result<Option<Held>> {
    lock(&watch_file(dir, session))
}

/// True while a watch holds the lock of `session`.
///
/// ```standalone_crate
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

/// The last answer of riff-server to `riff statusline` of one session
/// (01M53TZ0RBSJQ9ZZRXKMBM5R3E).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Kept {
    /// The URL of the riff-server that answered.
    pub server: String,
    /// The time of the answer, in milliseconds since 1970.
    pub at_ms: u64,
    /// The build of that riff-server.
    pub build: Option<String>,
    /// True when the session is a lead.
    pub lead: bool,
    /// The words of the status line that came from the answer
    /// ([`crate::text::statusline_answer`]).
    pub line: String,
}

/// The kept answer of `server` to the status line of `session`, when it
/// is less than `keep` old at `now_ms` (01M53TZ0RBSJQ9ZZRXKMBM5R3E).
///
/// ```
/// use std::time::Duration;
/// use riff::local::{Kept, keep_statusline, kept_statusline};
///
/// let run = tempfile::tempdir()?;
/// let (url, keep) = ("http://127.0.0.1:7878", Duration::from_secs(10));
/// let kept = Kept { server: url.into(), at_ms: 1_000, build: None, lead: true, line: "lead".into() };
/// keep_statusline(run.path(), "a6cf", &kept)?;
/// assert_eq!(kept_statusline(run.path(), "a6cf", url, 10_999, keep), Some(kept));
/// // Too old, another server, another session: nothing.
/// assert_eq!(kept_statusline(run.path(), "a6cf", url, 11_000, keep), None);
/// assert_eq!(kept_statusline(run.path(), "a6cf", "http://other", 2_000, keep), None);
/// assert_eq!(kept_statusline(run.path(), "5b1e", url, 2_000, keep), None);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn kept_statusline(
    dir: &Path,
    session: &str,
    server: &str,
    now_ms: u64,
    keep: std::time::Duration,
) -> Option<Kept> {
    let text = std::fs::read_to_string(statusline_file(dir, session)).ok()?;
    let kept: Kept = serde_json::from_str(&text).ok()?;
    let age = now_ms.checked_sub(kept.at_ms)?;
    (kept.server == server && u128::from(age) < keep.as_millis()).then_some(kept)
}

/// Keeps the answer of riff-server to the status line of `session`.
pub fn keep_statusline(dir: &Path, session: &str, kept: &Kept) -> io::Result<()> {
    let text = serde_json::to_string(kept).map_err(io::Error::other)?;
    write(&statusline_file(dir, session), text)
}

fn statusline_file(dir: &Path, session: &str) -> PathBuf {
    dir.join(format!("statusline-{}", riff_core::name::sanitize(session)))
}

/// Takes the update lock of this machine
/// (01M3N7JJH0SXXQYYBAHWPCNQGX). `None` when another update holds it.
///
/// ```standalone_crate
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
/// ```standalone_crate
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
    write(&dir.join("update-tried"), tag)
}

/// The log of the update of riff by itself.
pub fn update_log(dir: &Path) -> PathBuf {
    dir.join("update.log")
}

/// The first step of an update that only a person can do, with its fix
/// (01M51QFPW37EXVH4AXH70MSXBT, [`crate::update_steps::needs_you`]).
/// `None` when there is none.
///
/// ```
/// let run = tempfile::tempdir()?;
/// assert_eq!(riff::local::needs_you(run.path()), None);
/// riff::local::set_needs_you(run.path(), Some("podman: sudo apt install podman"))?;
/// assert_eq!(riff::local::needs_you(run.path()).as_deref(), Some("podman: sudo apt install podman"));
/// riff::local::set_needs_you(run.path(), None)?;
/// assert_eq!(riff::local::needs_you(run.path()), None);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn needs_you(dir: &Path) -> Option<String> {
    let line = std::fs::read_to_string(dir.join("needs-you")).ok()?;
    Some(line.trim().to_owned()).filter(|l| !l.is_empty())
}

/// Records `line` as the step that needs a person. `None` removes the
/// record.
pub fn set_needs_you(dir: &Path, line: Option<&str>) -> io::Result<()> {
    let path = dir.join("needs-you");
    match line {
        Some(line) => write(&path, line),
        None => match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    }
}

/// The directory of the mark of a leave: `$RIFF_HOME/state` (see
/// [`home`](crate::home)), else `$XDG_STATE_HOME/riff`, else
/// `$HOME/.local/state/riff`. `None` without `HOME`. It is never
/// `$XDG_RUNTIME_DIR`: the system clears that directory at a logout,
/// and the mark must hold until a join (01M3XQVJVJDX81QY38219SN96B).
pub fn marks() -> Option<PathBuf> {
    if let Some(home) = crate::home::dir() {
        return Some(home.join("state"));
    }
    marks_from(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
}

/// [`marks`] from the values of `XDG_STATE_HOME` and `HOME`. An empty
/// value counts as unset.
///
/// ```
/// use riff::local::marks_from;
/// use std::path::PathBuf;
///
/// let some = |s: &str| Some(s.into());
/// assert_eq!(marks_from(some("/s"), some("/h")), Some(PathBuf::from("/s/riff")));
/// assert_eq!(marks_from(some(""), some("/h")), Some(PathBuf::from("/h/.local/state/riff")));
/// assert_eq!(marks_from(None, None), None);
/// ```
pub fn marks_from(state: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    dir_from(None, state, home)
}

/// True when the session `session` left the riff, with the mark in
/// [`marks`] ([`riff_client::marks`]). False with no directory.
pub fn left_here(session: &str) -> bool {
    marks().is_some_and(|dir| riff_client::marks::left(&dir, session))
}

/// Takes the lock of the compact of the lead in `repo` (`OWNER/REPO`),
/// so that only one check acts at a time (01M3Q88G1K7N2EMPBA07X069A7).
/// `None` when another check holds it.
///
/// ```standalone_crate
/// let run = tempfile::tempdir()?;
/// let held = riff::local::compact_lock(run.path(), "o/r")?;
/// assert!(held.is_some());
/// assert!(riff::local::compact_lock(run.path(), "o/r")?.is_none());
/// drop(held);
/// assert!(riff::local::compact_lock(run.path(), "o/r")?.is_some());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn compact_lock(dir: &Path, repo: &str) -> io::Result<Option<Held>> {
    lock(&dir.join(format!("compact-{}.lock", repo.replace('/', "-"))))
}

/// Takes the lock of the monitor of the machine, so that one monitor
/// runs at a time (01M421QQ1K7EFDV2PVPTSTE5FK). `None` when another
/// monitor holds it.
///
/// ```standalone_crate
/// let run = tempfile::tempdir()?;
/// let held = riff::local::monitor_lock(run.path())?;
/// assert!(held.is_some());
/// assert!(riff::local::monitor_lock(run.path())?.is_none());
/// drop(held);
/// assert!(riff::local::monitor_lock(run.path())?.is_some());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn monitor_lock(dir: &Path) -> io::Result<Option<Held>> {
    lock(&dir.join("monitor.lock"))
}

/// Opens `path` and takes its lock. `None` when another open file holds
/// the lock.
fn lock(path: &Path) -> io::Result<Option<Held>> {
    if let Some(dir) = path.parent() {
        make_dir(dir)?;
    }
    let file = options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    private(&file)?;
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
    up_from(Some(std::os::unix::process::parent_id()))
}

/// The process `first` and the processes above it, nearest first. It
/// stops before the first process of the system.
fn up_from(first: Option<u32>) -> impl Iterator<Item = u32> {
    std::iter::successors(first, |&pid| parent_of(pid))
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
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    /// [`record`] when the lock is free. Other tests start child
    /// processes, so a dropped lock can live on for a short time (see
    /// [`Held`]).
    fn record_when_free(dir: &Path, agent: u32, session: &str) -> Held {
        let span = isolated::Span::start();
        loop {
            if let Some(held) = record(dir, agent, session).unwrap() {
                return held;
            }
            assert!(span.within(Duration::from_secs(10)), "no free lock");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn each_new_mark_of_no_tools_is_told_once() {
        let run = tempfile::tempdir().unwrap();
        let agent = std::process::id();
        set_no_tools(run.path(), agent, true).unwrap();
        assert!(first_no_tools_tell(run.path(), agent));
        assert!(!first_no_tools_tell(run.path(), agent));
        // The line of a stuck riff mcp keeps its own file.
        assert!(first_tell(run.path(), agent, 7));
        assert!(!first_no_tools_tell(run.path(), agent));
        let told = no_tools(run.path(), agent).unwrap();
        let span = isolated::Span::start();
        while no_tools(run.path(), agent).as_deref() == Some(told.as_str()) {
            assert!(span.within(Duration::from_secs(10)), "no new mark");
            std::thread::sleep(Duration::from_millis(2));
            set_no_tools(run.path(), agent, true).unwrap();
        }
        assert!(first_no_tools_tell(run.path(), agent));
        set_no_tools(run.path(), agent, false).unwrap();
        assert!(!first_no_tools_tell(run.path(), agent));
    }

    #[test]
    fn a_mark_of_no_tools_of_another_process_does_not_count() {
        let run = tempfile::tempdir().unwrap();
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        set_no_tools(run.path(), child.id(), true).unwrap();
        assert!(no_tools(run.path(), child.id()).is_some());
        assert_eq!(no_tools(run.path(), std::process::id()), None);
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(no_tools(run.path(), child.id()), None);
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
    fn a_lock_and_a_record_are_private_and_a_link_keeps_its_target() {
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let run = tempfile::tempdir().unwrap();
        let dir = run.path().join("riff");
        let _held = record_when_free(&dir, 42, "a6cf");
        assert_eq!(mode(&dir), 0o700);
        for file in ["mcp-42", "build-42"] {
            assert_eq!(mode(&dir.join(file)), 0o600, "{file}");
        }
        // A link in the folder: its target outside keeps its mode.
        let outside = run.path().join("outside");
        std::fs::write(&outside, "").unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("link")).unwrap();
        secure(&dir).unwrap();
        assert_eq!(mode(&outside), 0o644);
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

    /// Each value that a doc test of this file drops is a lock. So each
    /// doc test here with a `drop(` is a `standalone_crate` (see
    /// [`Held`], 01M513N34NJ2G3TK3EH91M10FJ).
    #[test]
    fn each_doc_test_that_drops_a_lock_is_a_crate_of_its_own() {
        let mut fence: Option<(usize, &str)> = None;
        let mut shared = Vec::new();
        for (at, line) in include_str!("local.rs").lines().enumerate() {
            let line = line.trim_start();
            let Some(doc) = line.strip_prefix("///").or(line.strip_prefix("//!")) else {
                continue;
            };
            let doc = doc.trim_start();
            if let Some(tag) = doc.strip_prefix("```") {
                fence = if fence.is_some() {
                    None
                } else {
                    Some((at + 1, tag))
                };
            } else if let Some((start, tag)) = fence
                && doc.starts_with("drop(")
                && tag != "standalone_crate"
            {
                shared.push(start);
            }
        }
        assert!(shared.is_empty(), "the doc tests at the lines {shared:?}");
    }
}
