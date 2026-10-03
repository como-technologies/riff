//! The pool of build jobs of a machine: one jobserver for all workers.
//!
//! # Design
//!
//! A fixed share of jobs for each worker leaves cores idle: when only
//! one worker builds, the shares of the other workers wait. So the
//! workers of a machine take their jobs from one pool
//! (01M3ZGZMJ9RF1C4AHG78GQ2NM4). The pool is a named pipe in the form of
//! GNU make 4.4 (`--jobserver-auth=fifo:PATH`). Each byte in the pipe is
//! one token. A `cargo` reads a token before it starts a compile job,
//! and writes it back at the end of the job.
//!
//! A jobserver client has one job with no token. So K builds at once
//! run at most `tokens + K` jobs. The pool holds
//! `physical cores - 1 - worker limit` tokens, 1 or more ([`tokens`]).
//! Then all builds of all workers stay at or below `physical cores - 1`.
//!
//! ```mermaid
//! flowchart TD
//!     W["riff workers run<br/>(one for each worker)"] --> H["Pool::hold:<br/>make or join the pool"]
//!     H --> F[("named pipe<br/>tokens + + + +")]
//!     H --> C["claude<br/>MAKEFLAGS=-j --jobserver-auth=fifo:PATH<br/>CARGO_TARGET_..._RUNNER=riff workers test-run"]
//!     C --> B["cargo build"]
//!     C --> T["cargo test"]
//!     B -- "a token for each compile job" --> F
//!     T --> R["riff workers test-run<br/>takes RUST_TEST_THREADS tokens"]
//!     R -- "tokens back at the end,<br/>also when the test is killed" --> F
//! ```
//!
//! - **Hold** ([`Pool::hold`]). Each `riff workers run` holds the pool
//!   while its worker lives. The first one makes the pipe and writes the
//!   tokens. The others join it. A pipe loses its bytes when no process
//!   holds it open, so the pool ends with the last worker of the machine.
//!   Two lock files keep the start and the end in order.
//! - **Tests** (01M3ZGZMNH1YM56GYNYBMH7AWM). The test runner of Rust
//!   does not read tokens. So `riff workers test-run` ([`test_run`])
//!   takes `RUST_TEST_THREADS` tokens for a test program, runs it, and
//!   writes the tokens back at its end. One runner at a time collects
//!   tokens, so two runners never each hold a part and wait for the
//!   other. A runner waits at most [`WAIT`], then runs with the tokens
//!   that it has. Only a program in a `deps` directory is a test
//!   program: `cargo run` of a program takes no tokens.
//! - **Fallback** (01M3ZGZMRHXRBP762QPVCV0YX8). When riff cannot make
//!   the pool, each worker gets the fixed share of
//!   [`crate::limits::jobs`]. `riff workers start` says so one time
//!   ([`check`]).
//! - **Show** (01M3ZGZMV78G3BNVFGHAZWQQDX). `riff workers jobs` shows
//!   the size of the pool and the tokens in use ([`state`]).
//!
//! ```
//! use riff::jobserver::{state, tokens, Pool};
//!
//! // pangolin: 8 physical cores and a limit of 3 workers.
//! assert_eq!(tokens(8, 3), 4);
//!
//! let dir = tempfile::tempdir()?;
//! let pool = Pool::hold(dir.path(), 4)?;
//! assert!(pool.makeflags().starts_with("-j --jobserver-auth=fifo:"));
//! assert_eq!(state(dir.path()).map(|s| (s.size, s.free)), Some((4, 4)));
//! drop(pool);
//! assert_eq!(state(dir.path()), None, "the pool ends with its last holder");
//! # Ok::<(), std::io::Error>(())
//! ```

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::fcntl::{Flock, FlockArg, OFlag};
use nix::sys::stat::Mode;

/// The directory of the pool in the local dir of riff.
pub const DIR: &str = "jobs";

/// The named pipe of the pool in [`DIR`].
pub const FIFO: &str = "fifo";

/// The lock that each holder of the pool keeps shared while it lives.
const HOLD: &str = "hold.lock";

/// The lock that keeps the start and the end of a hold in order.
const INIT: &str = "init.lock";

/// The lock of the runner that collects tokens now.
const TAKE: &str = "take.lock";

/// The file that holds the size of the pool.
const SIZE: &str = "size";

/// The file in the local dir that says: riff said that this machine
/// has no pool.
pub const SAID: &str = "no-jobserver";

/// One token in the pipe.
pub const TOKEN: u8 = b'+';

/// The longest wait of a test runner for its tokens.
pub const WAIT: Duration = Duration::from_secs(600);

/// The tokens of the pool for `physical` cores and a worker `limit`:
/// `physical - 1 - limit`, and 1 or more. No limit counts as one worker
/// (01M3ZGZMJ9RF1C4AHG78GQ2NM4).
///
/// ```
/// use riff::jobserver::tokens;
///
/// assert_eq!(tokens(8, 3), 4);
/// assert_eq!(tokens(16, 4), 11);
/// assert_eq!(tokens(4, 4), 1, "1 or more");
/// assert_eq!(tokens(8, 0), 6, "no limit counts as one worker");
/// ```
pub fn tokens(physical: u16, limit: u16) -> u16 {
    physical
        .saturating_sub(1)
        .saturating_sub(limit.max(1))
        .max(1)
}

/// The directory of the pool in the local dir `local`.
pub fn dir(local: &Path) -> PathBuf {
    local.join(DIR)
}

/// The value of `MAKEFLAGS` for the pipe `fifo`.
///
/// ```
/// assert_eq!(
///     riff::jobserver::makeflags("/run/riff/jobs/fifo".as_ref()),
///     "-j --jobserver-auth=fifo:/run/riff/jobs/fifo",
/// );
/// ```
pub fn makeflags(fifo: &Path) -> String {
    format!("-j --jobserver-auth=fifo:{}", fifo.display())
}

/// The pipe that `MAKEFLAGS` names, or `None`.
///
/// ```
/// use riff::jobserver::fifo_of;
///
/// assert_eq!(
///     fifo_of("-j --jobserver-auth=fifo:/run/riff/jobs/fifo"),
///     Some("/run/riff/jobs/fifo".into()),
/// );
/// assert_eq!(fifo_of("-j --jobserver-auth=3,4"), None, "not a named pipe");
/// assert_eq!(fifo_of("-k"), None);
/// ```
pub fn fifo_of(makeflags: &str) -> Option<PathBuf> {
    makeflags
        .split_whitespace()
        .rev()
        .find_map(|word| word.strip_prefix("--jobserver-auth=fifo:"))
        .map(PathBuf::from)
}

fn open_lock(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
}

fn lock(path: &Path, arg: FlockArg) -> io::Result<Flock<File>> {
    Flock::lock(open_lock(path)?, arg).map_err(|(_, e)| e.into())
}

/// Opens the pipe for read and write: the open does not wait for a
/// writer, and the pipe keeps its bytes while it is open.
fn open_fifo(path: &Path, nonblock: bool) -> io::Result<File> {
    let flags = if nonblock {
        OFlag::O_NONBLOCK.bits()
    } else {
        0
    };
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(flags)
        .open(path)?;
    if !file.metadata()?.file_type().is_fifo() {
        return Err(io::Error::other(format!(
            "{} is not a named pipe",
            path.display()
        )));
    }
    Ok(file)
}

/// True when a process holds the pool in `dir`.
fn held(dir: &Path) -> bool {
    let Ok(file) = open_lock(&dir.join(HOLD)) else {
        return false;
    };
    Flock::lock(file, FlockArg::LockExclusiveNonblock).is_err()
}

fn size_in(dir: &Path) -> Option<u16> {
    std::fs::read_to_string(dir.join(SIZE))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// The pool of a machine, held by one `riff workers run`.
#[derive(Debug)]
pub struct Pool {
    dir: PathBuf,
    size: u16,
    fifo: Option<File>,
    hold: Option<Flock<File>>,
}

impl Pool {
    /// Holds the pool in `dir`. When no other process holds it, it makes
    /// a new pipe with `size` tokens. Else it joins the pool, with the
    /// size of that pool.
    pub fn hold(dir: &Path, size: u16) -> io::Result<Pool> {
        std::fs::create_dir_all(dir)?;
        let _init = lock(&dir.join(INIT), FlockArg::LockExclusive)?;
        let path = dir.join(FIFO);
        let hold = open_lock(&dir.join(HOLD))?;
        let (hold, size, new) = match Flock::lock(hold, FlockArg::LockExclusiveNonblock) {
            Ok(hold) => {
                match std::fs::remove_file(&path) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                    _ => {}
                }
                nix::unistd::mkfifo(&path, Mode::S_IRUSR | Mode::S_IWUSR)?;
                std::fs::write(dir.join(SIZE), size.to_string())?;
                (hold, size, true)
            }
            Err((file, Errno::EWOULDBLOCK)) => {
                let hold = Flock::lock(file, FlockArg::LockShared).map_err(|(_, e)| e)?;
                (hold, size_in(dir).unwrap_or(size), false)
            }
            Err((_, e)) => return Err(e.into()),
        };
        let mut fifo = open_fifo(&path, false)?;
        if new {
            fifo.write_all(&vec![TOKEN; size.into()])?;
            hold.relock(FlockArg::LockShared)?;
        }
        Ok(Pool {
            dir: dir.to_owned(),
            size,
            fifo: Some(fifo),
            hold: Some(hold),
        })
    }

    /// The tokens of the pool.
    pub fn size(&self) -> u16 {
        self.size
    }

    /// The named pipe of the pool.
    pub fn fifo(&self) -> PathBuf {
        self.dir.join(FIFO)
    }

    /// The value of `MAKEFLAGS` that names the pool.
    pub fn makeflags(&self) -> String {
        makeflags(&self.fifo())
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        // In order with a new hold: a new holder that sees this hold
        // also finds the pipe open.
        let _init = lock(&self.dir.join(INIT), FlockArg::LockExclusive);
        self.fifo.take();
        self.hold.take();
    }
}

/// The state of a pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    /// The tokens of the pool.
    pub size: u16,
    /// The tokens in the pipe now: the others are in use.
    pub free: u16,
}

/// The state of the pool in `dir`, or `None` when no process holds it
/// (01M3ZGZMV78G3BNVFGHAZWQQDX). A pipe does not tell how many bytes it
/// holds, so riff reads the free tokens and writes them back at once.
pub fn state(dir: &Path) -> Option<State> {
    if !held(dir) {
        return None;
    }
    let size = size_in(dir)?;
    let mut fifo = open_fifo(&dir.join(FIFO), true).ok()?;
    let mut buf = vec![0; usize::from(size) + 64];
    let free = match fifo.read(&mut buf) {
        Ok(n) => n,
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => 0,
        Err(_) => return None,
    };
    fifo.write_all(&buf[..free]).ok()?;
    Some(State {
        size,
        free: u16::try_from(free).unwrap_or(u16::MAX),
    })
}

/// Checks that riff can make a pool in the local dir `local`
/// (01M3ZGZMRHXRBP762QPVCV0YX8). When it cannot, it gives the line to
/// say, one time: the file [`SAID`] in `local` holds that.
///
/// ```
/// let local = tempfile::tempdir()?;
/// assert_eq!(riff::jobserver::check(Some(local.path())), None);
/// let line = riff::jobserver::check(None).expect("no local dir: no pool");
/// assert!(line.contains("fixed share"), "{line}");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn check(local: Option<&Path>) -> Option<String> {
    let Some(local) = local else {
        return Some(crate::text::no_jobserver("riff has no local dir"));
    };
    let said = local.join(SAID);
    let made = (|| -> io::Result<()> {
        let dir = dir(local);
        std::fs::create_dir_all(&dir)?;
        let test = dir.join(format!("check-{}", std::process::id()));
        let _ = std::fs::remove_file(&test);
        nix::unistd::mkfifo(&test, Mode::S_IRUSR | Mode::S_IWUSR)?;
        std::fs::remove_file(&test)
    })();
    match made {
        Ok(()) => {
            let _ = std::fs::remove_file(said);
            None
        }
        Err(e) => {
            let first = !said.exists();
            let _ = std::fs::create_dir_all(local);
            let _ = std::fs::write(&said, "");
            first.then(|| crate::text::no_jobserver(&e.to_string()))
        }
    }
}

/// Tokens taken from a pool. They go back to the pool when the value
/// drops.
#[derive(Debug)]
pub struct Tokens {
    fifo: File,
    count: usize,
}

impl Tokens {
    /// The number of tokens.
    pub fn count(&self) -> usize {
        self.count
    }
}

impl Drop for Tokens {
    fn drop(&mut self) {
        let _ = self.fifo.write_all(&vec![TOKEN; self.count]);
    }
}

/// Takes `want` tokens from the pool of the pipe `fifo`, at most the
/// size of the pool (01M3ZGZMNH1YM56GYNYBMH7AWM). `None` when no process
/// holds the pool. Only one taker at a time collects tokens. After
/// `wait`, it gives the tokens that it has.
pub fn take(fifo: &Path, want: u16, wait: Duration) -> io::Result<Option<Tokens>> {
    let Some(dir) = fifo.parent() else {
        return Ok(None);
    };
    if want == 0 || !held(dir) {
        return Ok(None);
    }
    let want = usize::from(want.min(size_in(dir).unwrap_or(want)));
    let end = Instant::now() + wait;
    let pause = Duration::from_millis(20);
    let mut file = Some(open_lock(&dir.join(TAKE))?);
    let _lock = loop {
        let Some(next) = file.take() else { break None };
        match Flock::lock(next, FlockArg::LockExclusiveNonblock) {
            Ok(lock) => break Some(lock),
            Err((next, Errno::EWOULDBLOCK)) if Instant::now() < end => {
                file = Some(next);
                std::thread::sleep(pause);
            }
            Err((_, Errno::EWOULDBLOCK)) => break None,
            Err((_, e)) => return Err(e.into()),
        }
    };
    let mut tokens = Tokens {
        fifo: open_fifo(fifo, true)?,
        count: 0,
    };
    let mut buf = vec![0; want];
    while tokens.count < want {
        match tokens.fifo.read(&mut buf[..want - tokens.count]) {
            Ok(n) => tokens.count += n,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }
        if tokens.count < want {
            if Instant::now() >= end {
                break;
            }
            std::thread::sleep(pause);
        }
    }
    Ok(Some(tokens))
}

/// True when `program` is a test program of cargo: it is in a `deps`
/// directory.
///
/// ```
/// use riff::jobserver::is_test_program;
///
/// assert!(is_test_program("target/debug/deps/riff-1a2b3c".as_ref()));
/// assert!(!is_test_program("target/debug/riff".as_ref()), "cargo run");
/// ```
pub fn is_test_program(program: &Path) -> bool {
    program
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "deps")
}

/// `riff workers test-run PROGRAM ARGS`: the test runner of a worker
/// (01M3ZGZMNH1YM56GYNYBMH7AWM). For a test program, it takes
/// `RUST_TEST_THREADS` tokens from the pool that `MAKEFLAGS` names.
/// Then it runs the program, and gives each signal to it. At the end it
/// gives the tokens back, also when a signal killed the program.
/// Returns the exit code: the code of the program, or 128 and the
/// signal.
pub async fn test_run(program: &Path, args: &[String]) -> anyhow::Result<i32> {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;
    use tokio::signal::unix::{SignalKind, signal};

    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    let mut hup = signal(SignalKind::hangup())?;
    let fifo = std::env::var("MAKEFLAGS").ok().and_then(|m| fifo_of(&m));
    let want = std::env::var("RUST_TEST_THREADS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);
    let tokens = match fifo.filter(|_| is_test_program(program)) {
        Some(fifo) => tokio::task::spawn_blocking(move || take(&fifo, want, WAIT))
            .await?
            .unwrap_or_else(|e| {
                eprintln!("riff: the test runs with no token from the pool: {e}");
                None
            }),
        None => None,
    };
    let mut child = tokio::process::Command::new(program).args(args).spawn()?;
    let pid = child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .map(Pid::from_raw);
    let send = |sig: Signal| {
        if let Some(pid) = pid {
            let _ = kill(pid, sig);
        }
    };
    let status = loop {
        tokio::select! {
            status = child.wait() => break status?,
            _ = term.recv() => send(Signal::SIGTERM),
            _ = int.recv() => send(Signal::SIGINT),
            _ = hup.recv() => send(Signal::SIGHUP),
        }
    };
    drop(tokens);
    Ok(status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builds_stay_at_or_below_the_physical_cores_less_one() {
        for physical in [4u16, 8, 16, 32] {
            for limit in 1..physical / 2 {
                // Each build has one job with no token.
                let most = tokens(physical, limit) + limit;
                assert!(most < physical, "{physical} cores, {limit} workers: {most}");
            }
        }
    }

    #[test]
    fn a_second_holder_joins_the_pool_and_the_last_ends_it() {
        let dir = tempfile::tempdir().unwrap();
        let one = Pool::hold(dir.path(), 3).unwrap();
        let two = Pool::hold(dir.path(), 9).unwrap();
        assert_eq!(two.size(), 3, "the size of the pool that runs");
        assert_eq!(state(dir.path()), Some(State { size: 3, free: 3 }));
        drop(one);
        assert_eq!(state(dir.path()), Some(State { size: 3, free: 3 }));
        drop(two);
        assert_eq!(state(dir.path()), None);
        let three = Pool::hold(dir.path(), 5).unwrap();
        assert_eq!(
            state(dir.path()),
            Some(State { size: 5, free: 5 }),
            "a new pool"
        );
        drop(three);
    }

    #[test]
    fn tokens_go_back_when_they_drop() {
        let dir = tempfile::tempdir().unwrap();
        let pool = Pool::hold(dir.path(), 4).unwrap();
        let tokens = take(&pool.fifo(), 3, WAIT).unwrap().unwrap();
        assert_eq!(tokens.count(), 3);
        assert_eq!(state(dir.path()).unwrap().free, 1);
        let most = take(&pool.fifo(), 9, Duration::from_millis(100))
            .unwrap()
            .unwrap();
        assert_eq!(
            most.count(),
            1,
            "the wait ends with the tokens that are free"
        );
        drop((tokens, most));
        assert_eq!(state(dir.path()).unwrap().free, 4);
    }

    #[test]
    fn with_no_holder_there_are_no_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join(FIFO);
        assert!(take(&fifo, 2, WAIT).unwrap().is_none());
    }
}
