//! Time checks in a test that fail only when the logic is slow
//! (01M49HCWEHTKSVQ6176G7C6H2S).
//!
//! # Design
//!
//! A test that runs slow on a loaded machine has not failed. So a test
//! does not compare the wall clock with a tight limit. It measures a
//! [`Span`]:
//!
//! | To check | Use | It fails when |
//! |---|---|---|
//! | A wait for a fact: a line, a message, a file | [`Span::within`], [`in_time`] | the wall clock passes the limit and the CPU pressure is low, or the wall clock passes [`HANG`] |
//! | The speed of a step | [`Span::fast`] | the CPU time and the wall clock pass the limit and the CPU pressure is low, or the wall clock passes [`HANG`] |
//!
//! The CPU pressure is the share of the wall time of the span in which
//! a task of the machine waited for a CPU: the `some` line of
//! `/proc/pressure/cpu`. When it is [`PRESSURE_LIMIT`] or more, the
//! span is slow under load: a check prints `slow under load` one time,
//! and passes until [`HANG`]. A system with no pressure file has no
//! load, so the plain limit holds.
//!
//! ```mermaid
//! flowchart TD
//!     A[a check of the span] --> B{wall under the limit?}
//!     B -- yes --> P[pass]
//!     B -- no --> H{wall under HANG?}
//!     H -- no --> F[fail: a hang]
//!     H -- yes --> C{fast: CPU time under the limit?}
//!     C -- yes --> P
//!     C -- no, or within --> L{CPU pressure over the limit?}
//!     L -- yes --> S[print slow under load, pass]
//!     L -- no --> F2[fail: the logic is slow]
//! ```
//!
//! The CPU time is the user and system time of the test process, and
//! of each child process that it waited for, since the start of the
//! span. Other tests of the same process add to it, so it is an upper
//! bound of the CPU time of the step.

use std::future::IntoFuture;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use nix::sys::resource::{UsageWho, getrusage};

/// The wall-clock limit of each check. A span longer than this is a
/// hang, also under load (01M49HCWH5HR0GPKHXKYZTXPW3).
pub const HANG: Duration = Duration::from_secs(60);

/// The CPU pressure from which a span is slow under load: a task of the
/// machine waited for a CPU in 10 % of the wall time.
pub const PRESSURE_LIMIT: f64 = 0.10;

/// The file of the CPU pressure of the machine.
pub const PRESSURE_FILE: &str = "/proc/pressure/cpu";

/// A span of time in a test, from [`Span::start`].
#[derive(Debug)]
pub struct Span {
    wall: Instant,
    cpu: Duration,
    file: PathBuf,
    stall: Option<Duration>,
    told: AtomicBool,
}

impl Default for Span {
    fn default() -> Self {
        Span::start()
    }
}

impl Span {
    /// Starts a span now.
    pub fn start() -> Span {
        Span::with_pressure_file(PRESSURE_FILE)
    }

    /// Starts a span that reads the CPU pressure from `file`, for a
    /// test of the helper.
    pub fn with_pressure_file(file: impl AsRef<Path>) -> Span {
        let file = file.as_ref().to_owned();
        Span {
            stall: stall_in(&file),
            cpu: cpu_now(),
            wall: Instant::now(),
            file,
            told: AtomicBool::new(false),
        }
    }

    /// The wall time since the start.
    pub fn wall(&self) -> Duration {
        self.wall.elapsed()
    }

    /// The CPU time since the start: the user and system time of this
    /// process and of each child process that it waited for.
    ///
    /// ```standalone_crate
    /// let span = isolated::Span::start();
    /// let mut n = 0u64;
    /// while span.wall() < std::time::Duration::from_millis(50) {
    ///     n = std::hint::black_box(n + 1);
    /// }
    /// assert!(span.cpu() > std::time::Duration::from_millis(10));
    ///
    /// let idle = isolated::Span::start();
    /// std::thread::sleep(std::time::Duration::from_millis(50));
    /// assert!(idle.cpu() < idle.wall());
    /// ```
    pub fn cpu(&self) -> Duration {
        cpu_now().saturating_sub(self.cpu)
    }

    /// The CPU pressure since the start: the share of the wall time in
    /// which a task of the machine waited for a CPU, from 0 to 1. `None`
    /// when the system has no pressure file.
    ///
    /// ```
    /// let dir = tempfile::tempdir()?;
    /// let file = dir.path().join("cpu");
    /// let line = |total: u64| format!("some avg10=0.00 avg60=0.00 avg300=0.00 total={total}\n");
    /// std::fs::write(&file, line(1_000_000))?;
    /// let span = isolated::Span::with_pressure_file(&file);
    /// std::thread::sleep(std::time::Duration::from_millis(100));
    /// // A task waited for a CPU for 1 s, more than the wall time.
    /// std::fs::write(&file, line(2_000_000))?;
    /// assert_eq!(span.pressure(), Some(1.0));
    /// assert!(span.under_load());
    ///
    /// let none = isolated::Span::with_pressure_file(dir.path().join("none"));
    /// assert_eq!(none.pressure(), None);
    /// assert!(!none.under_load());
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn pressure(&self) -> Option<f64> {
        let stalled = stall_in(&self.file)?.saturating_sub(self.stall?);
        let wall = self.wall().as_secs_f64();
        Some(if wall > 0.0 {
            (stalled.as_secs_f64() / wall).min(1.0)
        } else {
            0.0
        })
    }

    /// True when the CPU pressure since the start is [`PRESSURE_LIMIT`]
    /// or more.
    pub fn under_load(&self) -> bool {
        self.pressure().is_some_and(|p| p >= PRESSURE_LIMIT)
    }

    /// True while a wait for a fact is in time: the wall time is under
    /// `limit`, or under [`HANG`] while the span is under load. Use it
    /// in a wait loop and in its assert.
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let file = dir.path().join("cpu");
    /// std::fs::write(&file, "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\n")?;
    /// let span = isolated::Span::with_pressure_file(&file);
    /// assert!(span.within(Duration::from_secs(1)));
    /// std::thread::sleep(Duration::from_millis(20));
    /// // No load: the limit holds.
    /// assert!(!span.within(Duration::from_millis(10)));
    /// // Under load, the wait goes on until the hang limit.
    /// std::fs::write(&file, "some avg10=90.00 avg60=0.00 avg300=0.00 total=20000\n")?;
    /// assert!(span.within(Duration::from_millis(10)));
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn within(&self, limit: Duration) -> bool {
        let wall = self.wall();
        if wall < limit {
            return true;
        }
        wall < HANG && self.slow_under_load(wall, limit)
    }

    /// True when a step is fast: its wall time is under `limit`, or its
    /// CPU time is under `limit`, or it is under load. A wall time of
    /// [`HANG`] or more is never fast. Use it in an assert after the
    /// step.
    ///
    /// ```standalone_crate
    /// use std::time::Duration;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let file = dir.path().join("cpu");
    /// std::fs::write(&file, "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\n")?;
    /// // A step that waits uses little CPU time: it is fast.
    /// let span = isolated::Span::with_pressure_file(&file);
    /// std::thread::sleep(Duration::from_millis(50));
    /// assert!(span.fast(Duration::from_millis(40)));
    /// // A step that computes for longer than the limit is slow.
    /// let span = isolated::Span::with_pressure_file(&file);
    /// let mut n = 0u64;
    /// while span.cpu() < Duration::from_millis(30) {
    ///     n = std::hint::black_box(n + 1);
    /// }
    /// assert!(!span.fast(Duration::from_millis(10)));
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn fast(&self, limit: Duration) -> bool {
        let wall = self.wall();
        if wall < limit {
            return true;
        }
        wall < HANG && (self.cpu() < limit || self.slow_under_load(wall, limit))
    }

    /// True when the span is under load. It says so one time on stderr,
    /// so the log of the test shows why it took longer.
    fn slow_under_load(&self, wall: Duration, limit: Duration) -> bool {
        let Some(pressure) = self.pressure() else {
            return false;
        };
        if pressure < PRESSURE_LIMIT {
            return false;
        }
        if !self.told.swap(true, Ordering::Relaxed) {
            eprintln!(
                "slow under load: {wall:.1?} over the limit {limit:?}, \
                 CPU pressure {:.0} %, CPU time {:.1?}",
                pressure * 100.0,
                self.cpu()
            );
        }
        true
    }
}

/// The error of [`in_time`]: no result in the time of the span.
#[derive(Debug)]
pub struct Late {
    /// The limit of the wait.
    pub limit: Duration,
    /// The wall time of the wait.
    pub wall: Duration,
    /// The CPU pressure of the wait, when the system has a pressure file.
    pub pressure: Option<f64>,
}

impl std::fmt::Display for Late {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no result in {:.1?} (limit {:?}", self.wall, self.limit)?;
        match self.pressure {
            Some(p) => write!(f, ", CPU pressure {:.0} %)", p * 100.0),
            None => write!(f, ")"),
        }
    }
}

impl std::error::Error for Late {}

/// Waits for `future` while the wait is in time (see [`Span::within`]).
/// It is `tokio::time::timeout` for a test: under load, the wait goes on
/// until [`HANG`].
///
/// ```
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// use std::time::Duration;
///
/// let fast = isolated::in_time(Duration::from_secs(5), async { 7 }).await;
/// assert_eq!(fast.unwrap(), 7);
///
/// let never = isolated::in_time(Duration::from_millis(10), std::future::pending::<()>());
/// let late = never.await.unwrap_err();
/// assert!(late.wall >= Duration::from_millis(10), "{late}");
/// # }
/// ```
pub async fn in_time<F: IntoFuture>(limit: Duration, future: F) -> Result<F::Output, Late> {
    let span = Span::start();
    let future = future.into_future();
    tokio::pin!(future);
    let mut wait = limit;
    loop {
        if let Ok(output) = tokio::time::timeout(wait, &mut future).await {
            return Ok(output);
        }
        if !span.within(limit) {
            return Err(Late {
                limit,
                wall: span.wall(),
                pressure: span.pressure(),
            });
        }
        // Under load: look at the pressure again each second.
        wait = Duration::from_secs(1).min(HANG.saturating_sub(span.wall()));
    }
}

/// The CPU time of this process and of its children that it waited for.
fn cpu_now() -> Duration {
    [UsageWho::RUSAGE_SELF, UsageWho::RUSAGE_CHILDREN]
        .into_iter()
        .filter_map(|who| getrusage(who).ok())
        .map(|u| duration(u.user_time()) + duration(u.system_time()))
        .sum()
}

fn duration(t: nix::sys::time::TimeVal) -> Duration {
    Duration::from_secs(t.tv_sec().max(0) as u64) + Duration::from_micros(t.tv_usec().max(0) as u64)
}

/// The total stall time of the `some` line of a pressure file.
fn stall_in(file: &Path) -> Option<Duration> {
    stall(&std::fs::read_to_string(file).ok()?)
}

/// The total stall time of the `some` line of a pressure text, in
/// microseconds.
///
/// ```
/// let text = "some avg10=1.50 avg60=0.03 avg300=0.02 total=577454349\n\
///             full avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
/// assert_eq!(isolated::stall(text), Some(std::time::Duration::from_micros(577454349)));
/// assert_eq!(isolated::stall("full total=3\n"), None);
/// ```
pub fn stall(text: &str) -> Option<Duration> {
    let line = text.lines().find(|l| l.starts_with("some "))?;
    let total = line
        .split_whitespace()
        .find_map(|field| field.strip_prefix("total="))?;
    total.parse().ok().map(Duration::from_micros)
}
