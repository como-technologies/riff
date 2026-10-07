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
//! `hardware threads - 2 - workers` tokens, 1 or more ([`tokens`]). The
//! workers are the worker limit, or the workers that run when they are
//! more. Then all builds of all workers stay at or below
//! `hardware threads - 2`. A compile job waits for memory and disk a
//! part of its time, so the second thread of a core does work too.
//!
//! ```mermaid
//! flowchart TD
//!     W["riff workers run<br/>(one for each worker)"] --> H["Pool::hold:<br/>make or join the pool"]
//!     H --> F[("named pipe<br/>tokens + + + +")]
//!     H --> C["claude<br/>MAKEFLAGS=-j --jobserver-auth=fifo:PATH<br/>CARGO_TARGET_..._RUNNER=riff workers test-run"]
//!     C --> B["cargo build"]
//!     C --> T["cargo test"]
//!     B -- "a token for each compile job" --> F
//!     T --> R["riff workers test-run<br/>takes the free tokens:<br/>one test thread for each"]
//!     R -- "tokens back at the end,<br/>also when the test is killed" --> F
//!     G["the first worker each 5 s:<br/>memory pressure above the limit?"] -- "yes: keep each free token" --> F
//!     G -- "no: give them back" --> F
//! ```
//!
//! - **Hold** ([`Pool::hold`]). Each `riff workers run` holds the pool
//!   while its worker lives. The first one makes the pipe and writes the
//!   tokens. The others join it. A pipe loses its bytes when no process
//!   holds it open, so the pool ends with the last worker of the machine.
//!   Two lock files keep the start and the end in order.
//! - **Workers that run.** Each `riff workers run` puts a lock file in
//!   [`WORKERS`] while its worker lives ([`Member`]). The pool keeps the
//!   number of workers that its size counts. When more workers run, each
//!   worker after the first ones keeps one token out of the pool, and
//!   gives it back when it is within the count again ([`share`]). So a
//!   lower limit, with workers over it, and a higher limit, with
//!   a pool of the old size, both keep the builds at or below
//!   `hardware threads - 2`.
//! - **Tests** (01M3ZGZMNH1YM56GYNYBMH7AWM). The test runner of Rust
//!   does not read tokens. So `riff workers test-run` ([`test_run`])
//!   takes tokens for a test program, runs it with one test thread for
//!   each token, and writes the tokens back at its end. It waits for
//!   one token, then takes each free token, at most the pool
//!   ([`take_free`]). A `RUST_TEST_THREADS` that a person sets wins: the
//!   runner then takes that number. One runner at a time collects
//!   tokens, so two runners never each hold a part and wait for the
//!   other. A runner waits at most [`WAIT`], then runs with the tokens
//!   that it has. Only a program in a `deps` directory is a test
//!   program: `cargo run` of a program takes no tokens.
//! - **Memory pressure** (01M49XNPMXD3SF6JHBYV9DN59M). The first worker
//!   reads the memory pressure ([`pressure_here`]) each
//!   [`SHARE_EVERY`]. While it is above [`PRESSURE_LIMIT`], the worker
//!   keeps each free token out of the pool, also each token that comes
//!   back. So the pool gives out no new token. Under the limit, it gives
//!   them back ([`hold_back`]). This is the signal that systemd-oomd
//!   acts on, so the builds slow down before oomd kills a worker.
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
//! // pangolin: 16 hardware threads and a limit of 4 workers.
//! assert_eq!(tokens(16, 4), 10);
//!
//! let dir = tempfile::tempdir()?;
//! let pool = Pool::hold(dir.path(), 4, 1)?;
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

/// The file of the workers that the pool counts in [`DIR`].
const COUNTED: &str = "counted";

/// The directory of the workers that run, in [`DIR`]: one lock file for
/// each worker ([`Member`]).
pub const WORKERS: &str = "workers";

/// The time between two checks of the share of a worker ([`share`]).
pub const SHARE_EVERY: Duration = Duration::from_secs(5);

/// The file in the local dir that says: riff said that this machine
/// has no pool.
pub const SAID: &str = "no-jobserver";

/// One token in the pipe.
pub const TOKEN: u8 = b'+';

/// The variable of the test threads of a Rust test program.
pub const THREADS_VAR: &str = "RUST_TEST_THREADS";

/// The longest wait of a test runner for its tokens.
pub const WAIT: Duration = Duration::from_secs(600);

/// The file of the memory pressure of the machine (PSI).
pub const PSI: &str = "/proc/pressure/memory";

/// The variable that names another file for [`PSI`], for a test.
pub const PSI_VAR: &str = "RIFF_PSI";

/// The memory pressure above which the pool gives out no new token: the
/// percent of the last 10 s in which some task waited for memory
/// (`some avg10`) (01M49XNPMXD3SF6JHBYV9DN59M).
pub const PRESSURE_LIMIT: f32 = 10.0;

/// The tokens of the pool for `threads` hardware threads (the logical
/// CPUs) and a worker `limit`: `threads - 2 - limit`, and 1 or more. No
/// limit counts as one worker (01M3ZGZMJ9RF1C4AHG78GQ2NM4).
///
/// ```
/// use riff::jobserver::tokens;
///
/// assert_eq!(tokens(16, 4), 10, "pangolin");
/// assert_eq!(tokens(32, 4), 26);
/// assert_eq!(tokens(4, 4), 1, "1 or more");
/// assert_eq!(tokens(8, 0), 5, "no limit counts as one worker");
/// ```
pub fn tokens(threads: u16, limit: u16) -> u16 {
    threads
        .saturating_sub(2)
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
    number_in(dir, SIZE)
}

fn number_in(dir: &Path, file: &str) -> Option<u16> {
    std::fs::read_to_string(dir.join(file))
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
    counted: u16,
    fifo: Option<File>,
    hold: Option<Flock<File>>,
}

impl Pool {
    /// Holds the pool in `dir`. When no other process holds it, it makes
    /// a new pipe with `size` tokens for `counted` workers. Else it joins
    /// the pool, with the size and the count of that pool.
    pub fn hold(dir: &Path, size: u16, counted: u16) -> io::Result<Pool> {
        std::fs::create_dir_all(dir)?;
        let _init = lock(&dir.join(INIT), FlockArg::LockExclusive)?;
        let path = dir.join(FIFO);
        let hold = open_lock(&dir.join(HOLD))?;
        let (hold, size, counted, new) = match Flock::lock(hold, FlockArg::LockExclusiveNonblock) {
            Ok(hold) => {
                match std::fs::remove_file(&path) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                    _ => {}
                }
                nix::unistd::mkfifo(&path, Mode::S_IRUSR | Mode::S_IWUSR)?;
                std::fs::write(dir.join(SIZE), size.to_string())?;
                std::fs::write(dir.join(COUNTED), counted.to_string())?;
                (hold, size, counted, true)
            }
            Err((file, Errno::EWOULDBLOCK)) => {
                let hold = Flock::lock(file, FlockArg::LockShared).map_err(|(_, e)| e)?;
                let counted = number_in(dir, COUNTED).unwrap_or(counted);
                (hold, size_in(dir).unwrap_or(size), counted, false)
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
            counted,
            fifo: Some(fifo),
            hold: Some(hold),
        })
    }

    /// The tokens of the pool.
    pub fn size(&self) -> u16 {
        self.size
    }

    /// The workers that the size of the pool counts.
    pub fn counted(&self) -> u16 {
        self.counted
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
    /// The workers that the size of the pool counts.
    pub counted: u16,
    /// The workers that run now ([`workers`]).
    pub workers: u16,
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
        counted: number_in(dir, COUNTED).unwrap_or(1),
        workers: workers(dir),
    })
}

/// One worker that runs on the machine: a locked file in [`WORKERS`]
/// while the worker lives. The name of the file holds the time of the
/// start, so the workers have an order ([`Member::rank`]).
#[derive(Debug)]
pub struct Member {
    path: PathBuf,
    _lock: Flock<File>,
}

impl Member {
    /// Puts this worker in the workers of the pool in `dir`.
    pub fn join(dir: &Path) -> io::Result<Member> {
        let workers = dir.join(WORKERS);
        std::fs::create_dir_all(&workers)?;
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let name = format!("{since:020}-{}", std::process::id());
        // Lock first, then show the file: no other process sees it
        // without its lock.
        let hidden = workers.join(format!(".{name}"));
        let lock = lock(&hidden, FlockArg::LockExclusiveNonblock)?;
        let path = workers.join(name);
        std::fs::rename(&hidden, &path)?;
        Ok(Member { path, _lock: lock })
    }

    /// The place of this worker among the workers that run, by the time
    /// of the start: 1 for the first.
    pub fn rank(&self) -> usize {
        let Some(dir) = self.path.parent().and_then(Path::parent) else {
            return 1;
        };
        members(dir)
            .iter()
            .position(|path| *path == self.path)
            .map_or(1, |at| at + 1)
    }
}

impl Drop for Member {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// The files of the workers that run in the pool dir `dir`, in the
/// order of their start. It removes the file of a worker that ended.
fn members(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir.join(WORKERS)) else {
        return Vec::new();
    };
    let mut live: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .map(|entry| entry.path())
        .filter(|path| {
            let Ok(file) = File::open(path) else {
                return false;
            };
            match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
                Ok(_ended) => {
                    let _ = std::fs::remove_file(path);
                    false
                }
                Err(_) => true,
            }
        })
        .collect();
    live.sort();
    live
}

/// The workers that run on the machine: the [`Member`]s of the pool dir
/// `dir` (01M3WFYZRK5CT22GJW6ZHYT9CC).
///
/// ```
/// use riff::jobserver::{workers, Member};
///
/// let dir = tempfile::tempdir()?;
/// assert_eq!(workers(dir.path()), 0);
/// let one = Member::join(dir.path())?;
/// let two = Member::join(dir.path())?;
/// assert_eq!((workers(dir.path()), one.rank(), two.rank()), (2, 1, 2));
/// drop(one);
/// assert_eq!((workers(dir.path()), two.rank()), (1, 1));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn workers(dir: &Path) -> u16 {
    u16::try_from(members(dir).len()).unwrap_or(u16::MAX)
}

/// One check of the share of the worker `member` in the pool of the pipe
/// `fifo` for `counted` workers (01M3ZGZMJ9RF1C4AHG78GQ2NM4). A worker
/// past the count keeps one token out of the pool in `kept`, so the
/// builds of all workers stay at or below the physical cores less 1. A
/// worker within the count gives its token back. A worker that finds no
/// free token tries again at the next check.
///
/// ```
/// use riff::jobserver::{share, state, Member, Pool};
///
/// let dir = tempfile::tempdir()?;
/// let pool = Pool::hold(dir.path(), 3, 1)?;
/// let (one, two) = (Member::join(dir.path())?, Member::join(dir.path())?);
/// let (mut kept_one, mut kept_two) = (None, None);
/// share(&pool.fifo(), pool.counted(), &one, &mut kept_one);
/// share(&pool.fifo(), pool.counted(), &two, &mut kept_two);
/// assert!(kept_one.is_none() && kept_two.is_some(), "the second worker is past the count");
/// assert_eq!(state(dir.path()).map(|s| (s.free, s.workers)), Some((2, 2)));
/// drop(one);
/// share(&pool.fifo(), pool.counted(), &two, &mut kept_two);
/// assert!(kept_two.is_none(), "now within the count");
/// assert_eq!(state(dir.path()).map(|s| s.free), Some(3));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn share(fifo: &Path, counted: u16, member: &Member, kept: &mut Option<Tokens>) {
    let past = member.rank() > usize::from(counted.max(1));
    if !past {
        *kept = None;
    } else if kept.is_none() {
        *kept = take(fifo, 1, Duration::ZERO)
            .ok()
            .flatten()
            .filter(|tokens| tokens.count() > 0);
    }
}

/// The memory pressure in the text of a PSI file: the `avg10` of the
/// line `some`, or `None`.
///
/// ```
/// use riff::jobserver::pressure_from;
///
/// let psi = "some avg10=12.50 avg60=3.00 avg300=1.00 total=99\n\
///            full avg10=4.00 avg60=1.00 avg300=0.50 total=42\n";
/// assert_eq!(pressure_from(psi), Some(12.5));
/// assert_eq!(pressure_from(""), None);
/// ```
pub fn pressure_from(text: &str) -> Option<f32> {
    text.lines()
        .find_map(|line| line.strip_prefix("some "))?
        .split_whitespace()
        .find_map(|field| field.strip_prefix("avg10="))?
        .parse()
        .ok()
}

/// The memory pressure of this machine, from [`PSI`] or the file that
/// [`PSI_VAR`] names. `None` when riff cannot read it, for example with
/// no PSI in the kernel: then the pool does not hold tokens back.
pub fn pressure_here() -> Option<f32> {
    let file = std::env::var_os(PSI_VAR).unwrap_or_else(|| PSI.into());
    pressure_from(&std::fs::read_to_string(file).ok()?)
}

/// One check of the memory `pressure` for the pool of the pipe `fifo`
/// (01M49XNPMXD3SF6JHBYV9DN59M). Above [`PRESSURE_LIMIT`], it takes each
/// free token into `held`, so the pool gives out no new token. At or
/// under the limit, and with no pressure to read, it gives them back.
///
/// ```
/// use riff::jobserver::{hold_back, state, Pool};
///
/// let dir = tempfile::tempdir()?;
/// let pool = Pool::hold(dir.path(), 4, 1)?;
/// let mut held = None;
/// hold_back(&pool.fifo(), Some(30.0), &mut held);
/// assert_eq!(state(dir.path()).map(|s| s.free), Some(0), "no new token");
/// hold_back(&pool.fifo(), Some(2.0), &mut held);
/// assert_eq!(state(dir.path()).map(|s| s.free), Some(4), "the tokens are back");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn hold_back(fifo: &Path, pressure: Option<f32>, held: &mut Option<Tokens>) {
    if !pressure.is_some_and(|p| p > PRESSURE_LIMIT) {
        *held = None;
        return;
    }
    let Ok(Some(more)) = take(fifo, u16::MAX, Duration::ZERO) else {
        return;
    };
    match held {
        Some(held) => held.absorb(more),
        None => *held = Some(more),
    }
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

    /// Adds the tokens of `other` to these tokens.
    fn absorb(&mut self, mut other: Tokens) {
        self.count += std::mem::take(&mut other.count);
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

/// Takes the free tokens of the pool of the pipe `fifo`, at most the
/// size of the pool (01M3ZGZMNH1YM56GYNYBMH7AWM). It waits at most
/// `wait` for the first token, as [`take`] does. Then it takes each
/// token that is free, with no wait. `None` when no process holds the
/// pool.
///
/// ```
/// use std::time::Duration;
/// use riff::jobserver::{take, take_free, Pool};
///
/// let dir = tempfile::tempdir()?;
/// let pool = Pool::hold(dir.path(), 6, 1)?;
/// let build = take(&pool.fifo(), 2, Duration::ZERO)?.unwrap();
/// let test = take_free(&pool.fifo(), Duration::ZERO)?.unwrap();
/// assert_eq!((build.count(), test.count()), (2, 4), "each free token");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn take_free(fifo: &Path, wait: Duration) -> io::Result<Option<Tokens>> {
    let Some(mut tokens) = take(fifo, 1, wait)? else {
        return Ok(None);
    };
    let most = fifo
        .parent()
        .and_then(size_in)
        .map_or(1, usize::from)
        .saturating_sub(tokens.count);
    let mut buf = vec![0; most];
    while !buf.is_empty() {
        match tokens.fifo.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                tokens.count += n;
                buf.truncate(buf.len() - n);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) => return Err(e),
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
/// (01M3ZGZMNH1YM56GYNYBMH7AWM). For a test program, it takes tokens
/// from the pool that `MAKEFLAGS` names: each free token
/// ([`take_free`]), or the `RUST_TEST_THREADS` that a person set. Then
/// it runs the program with one test thread for each token, and gives
/// each signal to it. At the end it gives the tokens back, also when a
/// signal killed the program. Returns the exit code: the code of the
/// program, or 128 and the signal.
pub async fn test_run(program: &Path, args: &[String]) -> anyhow::Result<i32> {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;
    use tokio::signal::unix::{SignalKind, signal};

    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    let mut hup = signal(SignalKind::hangup())?;
    let fifo = std::env::var("MAKEFLAGS").ok().and_then(|m| fifo_of(&m));
    let want: Option<u16> = std::env::var(THREADS_VAR)
        .ok()
        .and_then(|n| n.parse().ok());
    let tokens = match fifo.filter(|_| is_test_program(program)) {
        Some(fifo) => tokio::task::spawn_blocking(move || match want {
            Some(want) => take(&fifo, want, WAIT),
            None => take_free(&fifo, WAIT),
        })
        .await?
        .unwrap_or_else(|e| {
            eprintln!("riff: the test runs with no token from the pool: {e}");
            None
        }),
        None => None,
    };
    let mut command = tokio::process::Command::new(program);
    command.args(args);
    if want.is_none()
        && let Some(tokens) = &tokens
    {
        command.env(THREADS_VAR, tokens.count().max(1).to_string());
    }
    let mut child = command.spawn()?;
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
    fn the_pool_is_the_threads_less_2_less_the_limit() {
        let at_limit_4: Vec<u16> = [4u16, 8, 16, 32].map(|threads| tokens(threads, 4)).into();
        assert_eq!(at_limit_4, [1, 2, 10, 26]);
    }

    #[test]
    fn all_builds_stay_at_or_below_the_threads_less_2() {
        for threads in [8u16, 16, 32] {
            for limit in 1..threads / 2 - 1 {
                // Each build has one job with no token.
                let most = tokens(threads, limit) + limit;
                assert!(most <= threads - 2, "{threads} threads, {limit} workers: {most}");
            }
        }
    }

    #[test]
    fn under_pressure_the_tokens_that_come_back_stay_out() {
        let dir = tempfile::tempdir().unwrap();
        let pool = Pool::hold(dir.path(), 3, 1).unwrap();
        let build = take(&pool.fifo(), 2, WAIT).unwrap().unwrap();
        let mut held = None;
        hold_back(&pool.fifo(), Some(50.0), &mut held);
        assert_eq!(held.as_ref().map(Tokens::count), Some(1));
        drop(build);
        hold_back(&pool.fifo(), Some(50.0), &mut held);
        assert_eq!(held.as_ref().map(Tokens::count), Some(3));
        assert_eq!(state(dir.path()).unwrap().free, 0);
        hold_back(&pool.fifo(), None, &mut held);
        assert!(held.is_none(), "no pressure to read: no hold");
        assert_eq!(state(dir.path()).unwrap().free, 3);
    }

    #[test]
    fn a_second_holder_joins_the_pool_and_the_last_ends_it() {
        let dir = tempfile::tempdir().unwrap();
        let one = Pool::hold(dir.path(), 3, 1).unwrap();
        let two = Pool::hold(dir.path(), 9, 1).unwrap();
        assert_eq!(two.size(), 3, "the size of the pool that runs");
        assert_eq!(
            state(dir.path()),
            Some(State {
                size: 3,
                free: 3,
                counted: 1,
                workers: 0
            })
        );
        drop(one);
        assert_eq!(
            state(dir.path()),
            Some(State {
                size: 3,
                free: 3,
                counted: 1,
                workers: 0
            })
        );
        drop(two);
        assert_eq!(state(dir.path()), None);
        let three = Pool::hold(dir.path(), 5, 1).unwrap();
        assert_eq!(
            state(dir.path()),
            Some(State {
                size: 5,
                free: 5,
                counted: 1,
                workers: 0
            }),
            "a new pool"
        );
        drop(three);
    }

    #[test]
    fn tokens_go_back_when_they_drop() {
        let dir = tempfile::tempdir().unwrap();
        let pool = Pool::hold(dir.path(), 4, 1).unwrap();
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
