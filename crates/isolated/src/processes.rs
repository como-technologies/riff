//! The processes of an [`Isolated`](crate::Isolated) environment: no
//! test leaves a process after it ends (01M77HHYZWY1WWK1T6W6KX2SHV).
//!
//! # Design
//!
//! A test can start a process that outlives the command of the test: a
//! fake `systemctl` starts a `riff daemon` in the background, as systemd
//! does, and a fake `podman` starts a `riff relay`. Such a process gets
//! the parent 1 when its parent ends, and it does not end when the temp
//! dir of the test goes. Each one holds tasks of the machine, so many
//! test runs fill the task limit of a session container, and each fork
//! fails.
//!
//! So each command of an environment has the variable [`MARK`], with
//! the path of the temp dir of the environment as its value. Each child
//! of the command inherits it, also a child whose parent ends. When the
//! environment drops, it ends each process with its mark
//! ([`end_marked`]), before its temp dir goes:
//!
//! ```mermaid
//! flowchart LR
//!     T[the test] --> R["riff, with ISOLATED_DIR"]
//!     R --> S[a fake systemctl]
//!     S --> D["riff daemon: parent 1"]
//!     T -- "the end of the test:<br/>Isolated drops" --> K["a SIGKILL to each process<br/>with its ISOLATED_DIR"]
//!     K --> D
//! ```
//!
//! [`end_marked`] sends a SIGKILL: a `riff relay` catches a SIGTERM. It
//! looks again until it finds no process with the mark, so a child that
//! a process starts while it ends also ends. A process that starts a
//! program (`exec`) has an empty environment for a moment, so a look
//! does not find its mark. While such a process lives, [`end_marked`]
//! looks again, for up to [`SETTLE`]. The shared environment
//! ([`Isolated::shared`](crate::Isolated::shared)) never drops: a test
//! that starts a process that can outlive the test, or a child of such a
//! process, uses an environment of its own.
//!
//! A guard test runs a sample of the tests of its binary with
//! [`left_after`] (01M77HHZ2FP07ES4ZYG5KYVZ7E): the run has the variable
//! [`RUN`], and each process of the run inherits it, also a process that
//! no environment started. A process with it after the end of the run is
//! a process that a test left.
//!
//! On Linux, [`marked`] reads the environment of each process in
//! `/proc`. On another OS, it finds no process.

use std::ffi::OsStr;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// The variable of each command of an environment: the path of its temp
/// dir. Each child of the command inherits it.
pub const MARK: &str = "ISOLATED_DIR";

/// The variable of a run of [`left_after`]: each process of the run
/// inherits it.
pub const RUN: &str = "ISOLATED_RUN";

/// A process with a mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marked {
    /// The ID of the process.
    pub pid: i32,
    /// Its command line, with a space between two arguments.
    pub command: String,
}

/// The time for which [`end_marked`] waits for a process that starts a
/// program, when it finds no process with the mark.
pub const SETTLE: Duration = Duration::from_millis(200);

/// One look at the processes: each process but this one with the mark,
/// and the number of the processes that start a program at this moment.
#[derive(Debug, Default)]
struct Look {
    marked: Vec<Marked>,
    starting: usize,
}

/// Each process but this one whose environment has the variable `name`
/// with the value `value`. A process that ended, also one that waits for
/// its parent to read its exit, has no environment, so it does not come.
/// Neither does a process that starts a program at this moment (`exec`):
/// its environment is empty until the program runs.
///
/// ```
/// use std::process::Command;
///
/// let value = format!("a-doc-test-{}", std::process::id());
/// let mut child = Command::new("sleep").arg("60").env("A_MARK", &value).spawn()?;
/// let found = isolated::marked("A_MARK", &value);
/// assert_eq!(found.len(), usize::from(cfg!(target_os = "linux")));
/// if let Some(sleep) = found.first() {
///     assert_eq!(sleep.pid, child.id() as i32);
///     assert_eq!(sleep.command, "sleep 60");
/// }
/// child.kill()?;
/// child.wait()?;
/// assert!(isolated::marked("A_MARK", &value).is_empty());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn marked(name: &str, value: impl AsRef<OsStr>) -> Vec<Marked> {
    look(name, value.as_ref()).marked
}

/// The [`Look`] for the variable `name` with the value `value`.
#[cfg(target_os = "linux")]
fn look(name: &str, value: &OsStr) -> Look {
    use std::os::unix::ffi::OsStrExt;
    let mut entry = name.as_bytes().to_vec();
    entry.push(b'=');
    entry.extend_from_slice(value.as_bytes());
    let me = std::process::id();
    let mut look = Look::default();
    let Ok(procs) = std::fs::read_dir("/proc") else {
        return look;
    };
    for proc in procs.flatten() {
        let Some(pid) = proc
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<i32>().ok())
        else {
            continue;
        };
        if pid.unsigned_abs() == me {
            continue;
        }
        // An error: the process ended, or it is of another user.
        let Ok(environ) = std::fs::read(proc.path().join("environ")) else {
            continue;
        };
        if environ.is_empty() {
            let stat = std::fs::read_to_string(proc.path().join("stat")).unwrap_or_default();
            look.starting += usize::from(starts_a_program(&stat));
            continue;
        }
        if !environ.split(|b| *b == 0).any(|var| var == entry) {
            continue;
        }
        let line = std::fs::read(proc.path().join("cmdline")).unwrap_or_default();
        let args: Vec<_> = line
            .split(|b| *b == 0)
            .filter(|arg| !arg.is_empty())
            .map(String::from_utf8_lossy)
            .collect();
        look.marked.push(Marked {
            pid,
            command: args.join(" "),
        });
    }
    look.marked.sort_by_key(|p| p.pid);
    look
}

/// On an OS other than Linux, a look finds no process.
#[cfg(not(target_os = "linux"))]
fn look(name: &str, value: &OsStr) -> Look {
    let _ = (name, value);
    Look::default()
}

/// True when `stat`, the text of `/proc/PID/stat` of a process whose
/// environment reads empty, says that the process starts a program: it
/// lives, and it is no kernel thread. A process that ended waits for its
/// parent (`Z`), or is dead (`X`). The fields come after the name in
/// parentheses: the state first, and the flags seventh.
///
/// ```
/// use isolated::processes::starts_a_program;
///
/// assert!(starts_a_program("42 (sh) R 1 42 42 0 -1 4194560 0 0"));
/// // A name with a space and a parenthesis.
/// assert!(starts_a_program("42 (a) b) S 1 42 42 0 -1 4194560 0 0"));
/// assert!(!starts_a_program("42 (sh) Z 1 42 42 0 -1 4227332 0 0"));
/// // A kernel thread: the flag 0x00200000.
/// assert!(!starts_a_program("2 (kthreadd) S 0 0 0 0 -1 2129984 0 0"));
/// assert!(!starts_a_program(""));
/// ```
pub fn starts_a_program(stat: &str) -> bool {
    /// The flag of a kernel thread in the flags of `stat`.
    const KERNEL_THREAD: u64 = 0x0020_0000;
    // The name can hold a space or a parenthesis: the fields start after
    // the last one.
    let Some((_, fields)) = stat.rsplit_once(") ") else {
        return false;
    };
    let fields: Vec<&str> = fields.split_whitespace().collect();
    let Some(flags) = fields.get(6).and_then(|f| f.parse::<u64>().ok()) else {
        return false;
    };
    !matches!(fields.first(), Some(&"Z" | &"X")) && flags & KERNEL_THREAD == 0
}

/// Ends each process of [`marked`] with a SIGKILL, and looks again until
/// it finds none, or for [`HANG`](crate::HANG). While a look finds no
/// process with the mark and a process starts a program, it looks again
/// for up to [`SETTLE`]: that process can have the mark. Gives each
/// process that it ended, once. It never panics: an environment calls it
/// when it drops, also in a test that failed.
///
/// ```
/// use std::process::Command;
///
/// let value = format!("an-end-{}", std::process::id());
/// // sh ends at once, and its child sleep gets another parent.
/// let status = Command::new("sh")
///     .args(["-c", "sleep 60 > /dev/null 2>&1 &"])
///     .env("A_MARK", &value)
///     .status()?;
/// assert!(status.success());
/// let ended = isolated::end_marked("A_MARK", &value);
/// assert_eq!(ended.len(), usize::from(cfg!(target_os = "linux")));
/// assert!(isolated::marked("A_MARK", &value).is_empty());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn end_marked(name: &str, value: impl AsRef<OsStr>) -> Vec<Marked> {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;
    let value = value.as_ref();
    let start = Instant::now();
    let mut ended: Vec<Marked> = Vec::new();
    // The start of the looks that found no process with the mark.
    let mut quiet: Option<Instant> = None;
    loop {
        let look = look(name, value);
        if start.elapsed() >= crate::HANG {
            return ended;
        }
        if look.marked.is_empty() {
            let since = *quiet.get_or_insert_with(Instant::now);
            if look.starting == 0 || since.elapsed() >= SETTLE {
                return ended;
            }
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        quiet = None;
        for process in look.marked {
            // A process that ended in the meantime fails the kill.
            let _ = kill(Pid::from_raw(process.pid), Signal::SIGKILL);
            if !ended.iter().any(|p| p.pid == process.pid) {
                ended.push(process);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Runs the tests `tests` of the test binary `exe`, each by its exact
/// name, with a new value of [`RUN`]. Gives each process of the run that
/// is left after its end, and ends it ([`end_marked`]). A guard test
/// calls it with its own binary, `std::env::current_exe()`.
///
/// # Panics
///
/// When the binary does not run, when a test fails, or when the run does
/// not run each test: for example a test that has another name now.
pub fn left_after(exe: &Path, tests: &[&str]) -> Vec<Marked> {
    static RUNS: AtomicU64 = AtomicU64::new(0);
    let value = format!(
        "{}-{}",
        std::process::id(),
        RUNS.fetch_add(1, Ordering::Relaxed)
    );
    let out = std::process::Command::new(exe)
        .args(tests)
        .arg("--exact")
        .env(RUN, &value)
        .output()
        .expect("the test binary runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let passed = format!("{} passed", tests.len());
    assert!(
        out.status.success() && text.contains(&passed),
        "the run of {tests:?} did not pass:\n{text}"
    );
    end_marked(RUN, &value)
}
