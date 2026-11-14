//! The log of the daemon in a file of its own, with a size cap (#1858,
//! 01M749XM23FTKZ95QEZCMVQ2K5).
//!
//! # Design
//!
//! The daemon writes each line of its log to its stdout and its stderr.
//! systemd puts them in the journal of the user. On a busy host, that
//! journal can rotate in a few hours, and the log of a past fault is gone.
//! So on a host where the service manager keeps the log in a journal
//! ([`crate::service::ServiceManager::keeps_log`]), [`tee`] puts a pipe in
//! place of the stdout and of the stderr of the daemon at its start. A
//! thread of each pipe writes each byte to where it went before, and each
//! line, after its time in UTC, to [`FILE`] in the folder of the daemon
//! ([`Capped`]). When the file has [`CAP`] bytes or more, the next line
//! moves it to [`OLD`] and starts a new file. So the log of the daemon
//! holds the last 10 to 20 MB of lines. The processes that the daemon
//! starts write into the same pipes. launchd writes the log of the daemon
//! on a Mac to a file already: there, the daemon puts no pipe in place.
//!
//! Only the daemon that holds the socket puts the pipes in place
//! (01M74C23RM3VA63DX65BY64ET6): a second `riff daemon` gives its
//! refusal on its own stderr, and writes nothing to the log of the first.
//! At its stop, the daemon puts its old stdout and stderr back
//! ([`Tee::end`]), and waits up to [`DRAIN`] for the threads to copy the
//! last lines in the pipes. So its last line is not lost when the process
//! ends, and an error after it goes in full to the old stderr.
//!
//! ```mermaid
//! flowchart LR
//!     D["riff daemon: its lines, the lines of its children"] --> P["the pipes of stdout and stderr"]
//!     P --> J["the journal of the user, as before"]
//!     P --> F["daemon.log in the folder of the daemon, each line with its time"]
//!     F -->|"at 10 MB, at the next line"| O["daemon.log.1"]
//! ```

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

/// The log file of the daemon, in its folder.
pub const FILE: &str = "daemon.log";

/// The older log file of the daemon, in its folder.
pub const OLD: &str = "daemon.log.1";

/// The size of the log file at which the next line starts a new file.
pub const CAP: u64 = 10 * 1024 * 1024;

/// The longest time that [`Tee::end`] waits for the threads to copy the
/// last lines in the pipes. A child of the daemon that still runs keeps
/// a pipe open: its thread does not end.
pub const DRAIN: Duration = Duration::from_secs(2);

/// A log file with a size cap. Each line starts with its time. When the
/// file has its cap or more, the next line moves it to the older file
/// ([`OLD`]), and starts a new file: a line is never cut in two files. A
/// fault of the disk loses the line, and stops no writer.
///
/// ```
/// use riff::daemon::log::{Capped, FILE, OLD};
///
/// let dir = tempfile::tempdir()?;
/// let mut log = Capped::new(dir.path().join(FILE), 60);
/// log.write(b"riff daemon: a first line\nand a part", "2026-11-10T15:36:12Z");
/// log.write(b" of a line\n", "2026-11-10T15:36:13Z");
/// let text = std::fs::read_to_string(dir.path().join(FILE))?;
/// assert_eq!(
///     text,
///     "2026-11-10T15:36:12Z riff daemon: a first line\n\
///      2026-11-10T15:36:12Z and a part of a line\n"
/// );
/// // The file has its cap: the next line starts a new file.
/// log.write(b"a new line\n", "2026-11-10T15:36:14Z");
/// assert_eq!(std::fs::read_to_string(dir.path().join(OLD))?, text);
/// assert_eq!(
///     std::fs::read_to_string(dir.path().join(FILE))?,
///     "2026-11-10T15:36:14Z a new line\n"
/// );
/// # Ok::<(), std::io::Error>(())
/// ```
pub struct Capped {
    path: PathBuf,
    cap: u64,
    file: Option<File>,
    size: u64,
    /// True when the next byte starts a line.
    fresh: bool,
}

impl Capped {
    /// The log in the file `path` with the cap `cap`. A file that is
    /// there already gets the new lines at its end.
    pub fn new(path: PathBuf, cap: u64) -> Capped {
        Capped {
            path,
            cap,
            file: None,
            size: 0,
            fresh: true,
        }
    }

    /// Writes `bytes`. Each line that starts in them starts with `time`.
    pub fn write(&mut self, bytes: &[u8], time: &str) {
        for piece in bytes.split_inclusive(|b| *b == b'\n') {
            if self.fresh {
                self.open();
                if self.size >= self.cap {
                    self.start_again();
                }
                self.put(time.as_bytes());
                self.put(b" ");
            }
            self.put(piece);
            self.fresh = piece.ends_with(b"\n");
        }
    }

    /// Opens the file when it is not open, and reads its size.
    fn open(&mut self) {
        if self.file.is_some() {
            return;
        }
        let opened = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path);
        if let Ok(file) = opened {
            self.size = file.metadata().map_or(0, |m| m.len());
            self.file = Some(file);
        }
    }

    /// Writes `bytes` at the end of the file.
    fn put(&mut self, bytes: &[u8]) {
        self.open();
        if let Some(file) = &mut self.file
            && file.write_all(bytes).is_ok()
        {
            self.size += bytes.len() as u64;
        }
    }

    /// Moves the file to the older file, and starts a new one.
    fn start_again(&mut self) {
        self.file = None;
        let _ = std::fs::rename(&self.path, self.path.with_file_name(OLD));
        self.size = 0;
    }
}

/// Writes `line` and its end to `out` in one write. `eprintln!` writes
/// each part of its format in a write of its own, and the stderr of the
/// daemon has no buffer. Its stdout and its stderr go to the same journal
/// or file: a line on stdout from another task can come between two
/// parts, and cut the line in two. A fault of `out` loses the line, and
/// stops no writer.
///
/// ```
/// use std::io::Write;
///
/// /// Keeps each write.
/// #[derive(Default)]
/// struct Writes(Vec<String>);
///
/// impl Write for Writes {
///     fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
///         self.0.push(String::from_utf8_lossy(buf).into_owned());
///         Ok(buf.len())
///     }
///     fn flush(&mut self) -> std::io::Result<()> {
///         Ok(())
///     }
/// }
///
/// let (repo, what) = ("o/r", "cannot follow the changes of the plan");
/// let mut one = Writes::default();
/// riff::daemon::log::line(&mut one, &format!("riff daemon: {repo}: {what}"));
/// assert_eq!(one.0, ["riff daemon: o/r: cannot follow the changes of the plan\n"]);
/// // `writeln!` writes each part on its own.
/// let mut parts = Writes::default();
/// writeln!(parts, "riff daemon: {repo}: {what}")?;
/// assert_eq!(parts.0.concat(), one.0[0]);
/// assert!(parts.0.len() > 1);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn line(out: &mut impl Write, line: &str) {
    let _ = out.write_all(format!("{line}\n").as_bytes());
}

/// The time of a line of the log: now, in UTC, to the second.
fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Puts a file in place of the stdout or of the stderr of this process.
type Dup2 = fn(BorrowedFd<'_>) -> nix::Result<()>;

/// The pipes that [`tee`] put in place of the stdout and of the stderr
/// of this process. [`Tee::end`] puts the old ones back.
#[must_use = "Tee::end puts the old stdout and stderr back"]
pub struct Tee {
    pipes: Vec<Pipe>,
}

/// One pipe of a [`Tee`].
struct Pipe {
    /// Puts a file in place of the stdout or of the stderr.
    dup2: Dup2,
    /// Where the stdout or the stderr went before the pipe.
    before: OwnedFd,
    /// Gets no value: it ends when the thread of the pipe ends.
    done: mpsc::Receiver<()>,
}

impl Tee {
    /// Puts the old stdout and stderr back: a line after this goes only
    /// where it went before the pipes. Waits up to [`DRAIN`] for the
    /// threads to copy the lines that are still in the pipes.
    pub fn end(self) {
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        for pipe in &self.pipes {
            let _ = (pipe.dup2)(pipe.before.as_fd());
        }
        let until = Instant::now() + DRAIN;
        for pipe in &self.pipes {
            let left = until.saturating_duration_since(Instant::now());
            let _ = pipe.done.recv_timeout(left);
        }
    }
}

/// Puts a pipe in place of the stdout and of the stderr of this process.
/// A thread of each pipe writes each byte to where it went before, and
/// each line to the log [`FILE`] in the folder `dir`, with its time.
pub fn tee(dir: &Path) -> Result<Tee> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let log = Arc::new(Mutex::new(Capped::new(dir.join(FILE), CAP)));
    // The lines that wait in the buffers go out before the pipes.
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    let stdout: Dup2 = |fd| nix::unistd::dup2_stdout(fd);
    let stderr: Dup2 = |fd| nix::unistd::dup2_stderr(fd);
    let out = pipe_in_place(std::io::stdout().as_fd(), stdout, log.clone())?;
    let mut tee = Tee { pipes: vec![out] };
    match pipe_in_place(std::io::stderr().as_fd(), stderr, log) {
        Ok(err) => tee.pipes.push(err),
        Err(e) => {
            tee.end();
            return Err(e);
        }
    }
    Ok(tee)
}

/// Puts a new pipe in place of `fd` with `dup2`, and starts the thread
/// that copies it to where `fd` went before, and to `log`.
fn pipe_in_place(fd: BorrowedFd<'_>, dup2: Dup2, log: Arc<Mutex<Capped>>) -> Result<Pipe> {
    let copy = || {
        fd.try_clone_to_owned()
            .context("cannot copy the stdout or the stderr of the daemon")
    };
    let (before, to) = (copy()?, copy()?);
    let (mut reader, writer) = std::io::pipe().context("cannot make a pipe for the log")?;
    dup2(writer.as_fd()).context("cannot put the pipe of the log in place")?;
    drop(writer);
    let (ended, done) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        // The end of the thread drops it: `done` ends.
        let _ended = ended;
        let mut to = File::from(to);
        let mut buf = [0u8; 8192];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) => return,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return,
            };
            let _ = to.write_all(&buf[..n]);
            log.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .write(&buf[..n], &now());
        }
    });
    Ok(Pipe { dup2, before, done })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1858, 01M749XM23FTKZ95QEZCMVQ2K5: a log file that is there gets
    /// the new lines at its end, and counts its size from its bytes: the
    /// cap holds over a new start of the daemon.
    #[test]
    fn a_new_start_writes_at_the_end_of_the_log_and_keeps_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        std::fs::write(&path, "x".repeat(30) + "\n").unwrap();
        let mut log = Capped::new(path.clone(), 40);
        log.write(b"one\n", "T1");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "x".repeat(30) + "\nT1 one\n"
        );
        log.write(b"two\nthree\n", "T2");
        assert_eq!(
            std::fs::read_to_string(dir.path().join(OLD)).unwrap(),
            "x".repeat(30) + "\nT1 one\nT2 two\n"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "T2 three\n");
        // A new start with a file over the cap starts a new file at its
        // first line. Only one older file stays: the next one replaces it.
        let mut log = Capped::new(path.clone(), 4);
        log.write(b"four\nfive\n", "T3");
        assert_eq!(
            std::fs::read_to_string(dir.path().join(OLD)).unwrap(),
            "T3 four\n"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "T3 five\n");
    }

    /// A line to an output that fails is lost, and does not stop the
    /// writer.
    #[test]
    fn a_line_to_a_failed_output_is_lost() {
        struct Fails;
        impl Write for Fails {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        line(&mut Fails, "riff daemon: o/r: a line");
    }

    /// A log in a folder that is gone loses its lines, and does not fail.
    #[test]
    fn a_log_that_cannot_write_loses_the_line() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = Capped::new(dir.path().join("gone").join(FILE), 40);
        log.write(b"a line\n", "T1");
        assert!(!dir.path().join("gone").exists());
    }
}
