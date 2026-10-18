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
//! | A wait for a fact: a line, a message, a file | [`Span::within`], [`in_time`] | the wall clock passes the limit and the pressure is low, or the wall clock passes [`HANG`] |
//! | A step that returns at once | [`Span::within`] after the step | the same |
//!
//! A check never passes on a low CPU time: a step that waits, for a
//! lock, a child or a sleep, uses no CPU time and is still slow.
//! [`Span::cpu`] goes in the message of a failed check, so the log
//! shows whether the step computed or waited.
//!
//! The pressure of a resource is the share of the wall time of the span
//! in which a task of the machine waited for it: the `some` line of its
//! file in `/proc/pressure`. A span reads each of [`RESOURCES`]: the
//! CPU, the IO and the memory. A step that waits for a busy disk uses no
//! CPU time, and the CPU pressure stays low (01M56QW660NSKW0HC5W518EZZQ).
//! When the highest pressure is [`PRESSURE_LIMIT`] or more, the span is
//! slow under load: a check prints `slow under load` one time, and
//! passes until [`HANG`]. A system with no pressure file has no load, so
//! the plain limit holds.
//!
//! ```mermaid
//! flowchart TD
//!     A[a check of the span] --> B{wall under the limit?}
//!     B -- yes --> P[pass]
//!     B -- no --> H{wall under HANG?}
//!     H -- no --> F[fail: a hang]
//!     H -- yes --> L{CPU, IO or memory pressure over the limit?}
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

/// The pressure from which a span is slow under load: a task of the
/// machine waited for a resource in 10 % of the wall time.
pub const PRESSURE_LIMIT: f64 = 0.10;

/// The dir of the pressure files of the machine.
pub const PRESSURE_DIR: &str = "/proc/pressure";

/// The resources whose pressure a span reads: the name of the file in
/// [`PRESSURE_DIR`], and the name in a message.
pub const RESOURCES: [(&str, &str); 3] = [("cpu", "CPU"), ("io", "IO"), ("memory", "memory")];

/// The highest pressure of a span, and its resource.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pressure {
    /// The name of the resource in a message, from [`RESOURCES`].
    pub resource: &'static str,
    /// The share of the wall time in which a task waited, from 0 to 1.
    pub share: f64,
}

impl std::fmt::Display for Pressure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} pressure {:.0} %", self.resource, self.share * 100.0)
    }
}

/// A span of time in a test, from [`Span::start`].
#[derive(Debug)]
pub struct Span {
    wall: Instant,
    cpu: Duration,
    dir: PathBuf,
    stalls: [Option<Duration>; RESOURCES.len()],
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
        Span::with_pressure_dir(PRESSURE_DIR)
    }

    /// Starts a span that reads the pressure files in `dir`, for a test
    /// of the helper.
    pub fn with_pressure_dir(dir: impl AsRef<Path>) -> Span {
        let dir = dir.as_ref().to_owned();
        Span {
            stalls: stalls_in(&dir),
            cpu: cpu_now(),
            wall: Instant::now(),
            dir,
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

    /// The highest pressure since the start, of each of [`RESOURCES`]
    /// that has a file: the share of the wall time in which a task of
    /// the machine waited for the resource. `None` when the system has
    /// no pressure file.
    ///
    /// ```
    /// let dir = tempfile::tempdir()?;
    /// let line = |total: u64| format!("some avg10=0.00 avg60=0.00 avg300=0.00 total={total}\n");
    /// std::fs::write(dir.path().join("cpu"), line(1_000_000))?;
    /// std::fs::write(dir.path().join("io"), line(1_000_000))?;
    /// let span = isolated::Span::with_pressure_dir(dir.path());
    /// std::thread::sleep(std::time::Duration::from_millis(100));
    /// // A task waited for IO for 1 s, more than the wall time. No task
    /// // waited for a CPU.
    /// std::fs::write(dir.path().join("io"), line(2_000_000))?;
    /// let pressure = span.pressure().unwrap();
    /// assert_eq!((pressure.resource, pressure.share), ("IO", 1.0));
    /// assert_eq!(pressure.to_string(), "IO pressure 100 %");
    /// assert!(span.under_load());
    ///
    /// let none = isolated::Span::with_pressure_dir(dir.path().join("none"));
    /// assert_eq!(none.pressure(), None);
    /// assert!(!none.under_load());
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn pressure(&self) -> Option<Pressure> {
        let wall = self.wall().as_secs_f64();
        RESOURCES
            .iter()
            .zip(stalls_in(&self.dir))
            .zip(self.stalls)
            .filter_map(|(((_, resource), now), start)| {
                let stalled = now?.saturating_sub(start?).as_secs_f64();
                let share = if wall > 0.0 {
                    (stalled / wall).min(1.0)
                } else {
                    0.0
                };
                Some(Pressure { resource, share })
            })
            .max_by(|a, b| a.share.total_cmp(&b.share))
    }

    /// True when the highest pressure since the start is
    /// [`PRESSURE_LIMIT`] or more.
    pub fn under_load(&self) -> bool {
        self.pressure().is_some_and(|p| p.share >= PRESSURE_LIMIT)
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
    /// let span = isolated::Span::with_pressure_dir(dir.path());
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

    /// True when the span is under load. It says so one time on stderr,
    /// so the log of the test shows why it took longer.
    fn slow_under_load(&self, wall: Duration, limit: Duration) -> bool {
        let Some(pressure) = self.pressure() else {
            return false;
        };
        if pressure.share < PRESSURE_LIMIT {
            return false;
        }
        if !self.told.swap(true, Ordering::Relaxed) {
            eprintln!(
                "slow under load: {wall:.1?} over the limit {limit:?}, \
                 {pressure}, CPU time {:.1?}",
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
    /// The highest pressure of the wait, when the system has a pressure
    /// file.
    pub pressure: Option<Pressure>,
}

impl std::fmt::Display for Late {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no result in {:.1?} (limit {:?}", self.wall, self.limit)?;
        match self.pressure {
            Some(p) => write!(f, ", {p})"),
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

/// The total stall time of each of [`RESOURCES`] in `dir`.
fn stalls_in(dir: &Path) -> [Option<Duration>; RESOURCES.len()] {
    RESOURCES.map(|(file, _)| stall(&std::fs::read_to_string(dir.join(file)).ok()?))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn line(total: u64) -> String {
        format!("some avg10=0.00 avg60=0.00 avg300=0.00 total={total}\nfull total=0\n")
    }

    #[test]
    fn the_pressure_is_the_highest_of_the_resources() {
        let dir = tempfile::tempdir().unwrap();
        for (file, _) in RESOURCES {
            std::fs::write(dir.path().join(file), line(0)).unwrap();
        }
        let span = Span::with_pressure_dir(dir.path());
        std::thread::sleep(Duration::from_millis(50));
        std::fs::write(dir.path().join("cpu"), line(1_000)).unwrap();
        std::fs::write(dir.path().join("memory"), line(10_000_000)).unwrap();
        let pressure = span.pressure().unwrap();
        assert_eq!((pressure.resource, pressure.share), ("memory", 1.0));
    }

    #[test]
    fn a_resource_with_no_file_does_not_count() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("io"), line(0)).unwrap();
        let span = Span::with_pressure_dir(dir.path());
        std::fs::write(dir.path().join("cpu"), line(10_000_000)).unwrap();
        let pressure = span.pressure().unwrap();
        assert_eq!(pressure.resource, "IO");
        assert!(!span.under_load());
    }
}
