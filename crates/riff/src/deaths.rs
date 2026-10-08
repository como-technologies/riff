//! A worker that dies again and again stops the new starts on its
//! machine.
//!
//! # Design
//!
//! riff replaces a worker that dies: its claims are free, and the
//! rollout starts a worker for the free work ([`crate::rollout`]). A
//! fault of the machine can kill each new worker too. Then each start
//! costs tokens and gives nothing. So a loop of deaths is a fault, not a
//! reason to start more (01M493YZZEW1FTDBNA090WT2AG).
//!
//! Each machine records the deaths of its workers in the file [`FILE`]
//! of its local dir ([`crate::local::dir`]): one line for each session,
//! with the time. A death is one of these:
//!
//! - `claude` of a worker exits on its own with a fault: an exit code
//!   that is not 0, or a signal. The wrapper records it
//!   ([`crate::worker`]).
//! - The pane of a worker ends with no end call: a memory kill, a crash,
//!   a closed pane. The process that looks after the panes records it
//!   ([`crate::reap`]).
//!
//! A session counts one time, also when both see it. When more than
//! [`LOOP`] workers died in the last [`WINDOW`], the machine has no room:
//! the rollout starts no worker there ([`crate::rollout::Place::room`]).
//! The death that starts the loop sends one message to the lead
//! (01M493Z02KS82B3CVZEVFA3D6E). The machine has room again when the
//! old deaths leave the window.
//!
//! ```mermaid
//! flowchart TD
//!     D["a worker dies"] --> R["record: time and session"]
//!     R --> C{"more than 3 deaths<br/>in the last hour?"}
//!     C -- no --> N["the rollout starts a worker<br/>for the free work"]
//!     C -- "yes, from this death" --> M["one message to the lead"]
//!     M --> S["no start on this machine<br/>until the old deaths are 1 hour old"]
//!     C -- "yes, before this death" --> S
//! ```
//!
//! A workers host tells its count in its status
//! ([`crate::host::HostStatus`]), so the rollout of the lead on another
//! machine sees it.

use std::io::{Read, Seek, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::Result;

/// The file of the deaths in the local dir.
pub const FILE: &str = "worker-deaths";

/// The most deaths in [`WINDOW`] before the machine stops its starts.
pub const LOOP: usize = 3;

/// The time that a death counts.
pub const WINDOW: Duration = Duration::from_secs(60 * 60);

/// The deaths of the workers of a machine: the time in seconds since
/// the epoch, and the session of each.
///
/// ```
/// use riff::deaths::Deaths;
///
/// let deaths = Deaths::parse("100 w1\n200 w2\n200 w2\nbad line\n");
/// assert_eq!(deaths.count(3700), 2);
/// // w1 died more than 1 hour before the time 3800.
/// assert_eq!(deaths.count(3800), 1);
/// assert_eq!(deaths.count(3801), 0);
/// assert_eq!(deaths.to_string(), "100 w1\n200 w2\n");
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Deaths(Vec<(u64, String)>);

impl Deaths {
    /// The deaths in the text of the file. A line that is not a death is
    /// left out. A session counts one time.
    pub fn parse(text: &str) -> Self {
        let mut deaths = Deaths::default();
        for line in text.lines() {
            let Some((at, session)) = line.split_once(' ') else {
                continue;
            };
            let (Ok(at), session) = (at.parse(), session.trim()) else {
                continue;
            };
            if !session.is_empty() && !deaths.has(session) {
                deaths.0.push((at, session.to_owned()));
            }
        }
        deaths
    }

    /// True when the death of `session` is in the record.
    pub fn has(&self, session: &str) -> bool {
        self.0.iter().any(|(_, s)| s == session)
    }

    /// The deaths in the [`WINDOW`] before `now`.
    pub fn count(&self, now: u64) -> usize {
        let since = now.saturating_sub(WINDOW.as_secs());
        self.0.iter().filter(|(at, _)| *at >= since).count()
    }

    /// Keeps only the deaths in the [`WINDOW`] before `now`.
    fn prune(&mut self, now: u64) {
        let since = now.saturating_sub(WINDOW.as_secs());
        self.0.retain(|(at, _)| *at >= since);
    }
}

impl std::fmt::Display for Deaths {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (at, session) in &self.0 {
            writeln!(f, "{at} {session}")?;
        }
        Ok(())
    }
}

/// True when `count` deaths in the [`WINDOW`] stop the starts of a
/// machine.
///
/// ```
/// assert!(!riff::deaths::halted(3));
/// assert!(riff::deaths::halted(4));
/// ```
pub fn halted(count: usize) -> bool {
    count > LOOP
}

/// What a record of a death gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recorded {
    /// The deaths in the [`WINDOW`], with this one.
    pub count: usize,
    /// True when this death starts the loop: the lead gets one message.
    pub starts_loop: bool,
}

/// Records the death of `session` at `now` in the file [`FILE`] of
/// `dir`. A session that is in the record already counts one time, and
/// starts no loop. The file holds only the deaths of the [`WINDOW`]. A
/// lock on the file makes the read and the write one step.
///
/// ```
/// use riff::deaths::{Recorded, record};
///
/// let dir = tempfile::tempdir().unwrap();
/// let at = |n: u64| 1_000_000 + n;
/// for (n, s) in ["w1", "w2", "w3"].iter().enumerate() {
///     assert!(!record(dir.path(), s, at(n as u64)).unwrap().starts_loop);
/// }
/// // The fourth death in one hour starts the loop, one time.
/// assert_eq!(record(dir.path(), "w4", at(10)).unwrap(), Recorded { count: 4, starts_loop: true });
/// assert_eq!(record(dir.path(), "w4", at(11)).unwrap(), Recorded { count: 4, starts_loop: false });
/// assert_eq!(record(dir.path(), "w5", at(12)).unwrap(), Recorded { count: 5, starts_loop: false });
/// assert_eq!(riff::deaths::count_in(dir.path(), at(12)), 5);
/// // One hour later, the old deaths count no more.
/// assert_eq!(riff::deaths::count_in(dir.path(), at(3611)), 1);
/// ```
pub fn record(dir: &Path, session: &str, now: u64) -> Result<Recorded> {
    std::fs::create_dir_all(dir)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join(FILE))?;
    file.lock()?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    let mut deaths = Deaths::parse(&text);
    deaths.prune(now);
    let before = deaths.count(now);
    if deaths.has(session) {
        return Ok(Recorded {
            count: before,
            starts_loop: false,
        });
    }
    deaths.0.push((now, session.to_owned()));
    let count = deaths.count(now);
    file.set_len(0)?;
    file.rewind()?;
    file.write_all(deaths.to_string().as_bytes())?;
    Ok(Recorded {
        count,
        starts_loop: !halted(before) && halted(count),
    })
}

/// The deaths in the [`WINDOW`] before `now`, from the file [`FILE`] of
/// `dir`. No file: 0.
pub fn count_in(dir: &Path, now: u64) -> usize {
    std::fs::read_to_string(dir.join(FILE))
        .map(|text| Deaths::parse(&text).count(now))
        .unwrap_or(0)
}

/// The deaths of the workers of this machine in the last [`WINDOW`]:
/// the deaths in the folder of riff, which riff outside each sandbox
/// writes, and in the own folder of the lead in its sandbox, where its
/// `riff mcp` writes (01M4DWJ0AQX8N7J9T02VJ0XHF1). Each worker counts
/// once.
pub fn here() -> usize {
    let dirs = [crate::local::riff_dir(), crate::local::dir()];
    count_of(dirs.iter().flatten(), crate::monitor::now_secs())
}

/// The deaths in the [`WINDOW`] before `now`, from the file [`FILE`] of
/// each of `dirs`. Each worker counts once.
///
/// ```
/// use riff::deaths::{count_of, record};
///
/// let (a, b) = (tempfile::tempdir()?, tempfile::tempdir()?);
/// record(a.path(), "w1", 100)?;
/// record(b.path(), "w1", 101)?;
/// record(b.path(), "w2", 102)?;
/// assert_eq!(count_of([a.path(), b.path()], 110), 2);
/// assert_eq!(count_of([a.path(), a.path()], 110), 1);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn count_of<P: AsRef<Path>>(dirs: impl IntoIterator<Item = P>, now: u64) -> usize {
    let mut all = Deaths(Vec::new());
    for dir in dirs {
        let text = std::fs::read_to_string(dir.as_ref().join(FILE)).unwrap_or_default();
        for (at, session) in Deaths::parse(&text).0 {
            if !all.has(&session) {
                all.0.push((at, session));
            }
        }
    }
    all.count(now)
}

/// Records the death of the worker `session` of this machine now. An
/// error goes to stderr: the record is best effort.
pub fn record_here(session: &str) -> Option<Recorded> {
    let dir = crate::local::dir()?;
    match record(&dir, session, crate::monitor::now_secs()) {
        Ok(recorded) => Some(recorded),
        Err(e) => {
            eprintln!("riff: cannot record the death of a worker: {e:#}");
            None
        }
    }
}
