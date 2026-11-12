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
//! a process starts while it ends also ends. The shared environment
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

/// Each process but this one whose environment has the variable `name`
/// with the value `value`. A process that ended, also one that waits for
/// its parent to read its exit, has no environment, so it does not come.
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
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        let mut entry = name.as_bytes().to_vec();
        entry.push(b'=');
        entry.extend_from_slice(value.as_ref().as_bytes());
        let me = std::process::id();
        let Ok(procs) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        let mut found: Vec<Marked> = procs
            .flatten()
            .filter_map(|proc| {
                let pid: i32 = proc.file_name().to_str()?.parse().ok()?;
                if pid.unsigned_abs() == me {
                    return None;
                }
                let environ = std::fs::read(proc.path().join("environ")).ok()?;
                if !environ.split(|b| *b == 0).any(|var| var == entry) {
                    return None;
                }
                let line = std::fs::read(proc.path().join("cmdline")).unwrap_or_default();
                let args: Vec<_> = line
                    .split(|b| *b == 0)
                    .filter(|arg| !arg.is_empty())
                    .map(String::from_utf8_lossy)
                    .collect();
                Some(Marked {
                    pid,
                    command: args.join(" "),
                })
            })
            .collect();
        found.sort_by_key(|p| p.pid);
        found
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (name, value);
        Vec::new()
    }
}

/// Ends each process of [`marked`] with a SIGKILL, and looks again until
/// it finds none, or for [`HANG`](crate::HANG). Gives each process that it
/// ended, once. It never panics: an environment calls it when it drops,
/// also in a test that failed.
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
    loop {
        let found = marked(name, value);
        if found.is_empty() || start.elapsed() >= crate::HANG {
            return ended;
        }
        for process in found {
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
