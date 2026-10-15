//! The limit of the host (#719, 01M4Z60B7Y4XD6TJCYEYJT2R4S): the riff
//! daemon starts a session only when the host has room for it.
//!
//! # Design
//!
//! Each container has its own `--memory` and `--cpus`. They do not limit
//! the sum: three builds of 6 GB each fill a host of 30 GB with no swap.
//! So the daemon keeps one limit for the whole host ([`Limit`]): the
//! number of live sessions, and a floor of free memory. A host has room
//! when fewer sessions than the limit live, and the free memory
//! (`MemAvailable` of `/proc/meminfo`) is at or above the floor.
//!
//! Each start of a session takes a [`Slot`] first ([`Room::admit`]). The
//! starts wait in order: the first that asks is the first that starts. A
//! slot counts as a live session from the admit until the daemon owns
//! the session; then the session counts. So two starts never take the
//! same room. A start of `riff run` and a start of a wave
//! ([`crate::wave`]) wait in the same line.
//!
//! ```mermaid
//! flowchart LR
//!     A["a start asks"] --> G["the gate: one start at a time, in order"]
//!     G --> R{"room? live + slots < sessions,<br/>free memory >= floor"}
//!     R -- no --> W["wait: a session ends, or LOOK_EVERY"] --> R
//!     R -- yes --> S["a slot: the start goes on"]
//!     S --> O["the daemon owns the session:<br/>the slot goes"]
//! ```
//!
//! # The setting
//!
//! `riff daemon limit` writes the limit to [`LIMIT_FILE`] in the folder
//! of the daemon. The daemon reads the file at each look for room, so a
//! change counts at once, with no restart. With no file, the limit is
//! [`Limit::default`]: 3 sessions and a floor of 8 GB.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

/// The default number of live sessions of a host
/// (01M4Z60B7Y4XD6TJCYEYJT2R4S).
pub const SESSIONS: u32 = 3;

/// The default floor of free memory, in GB (01M4Z60B7Y4XD6TJCYEYJT2R4S).
pub const FLOOR_GB: u64 = 8;

/// The file of the limit, in the folder of the daemon.
pub const LIMIT_FILE: &str = "limit.json";

/// How often a start that waits looks for room again, when no session
/// ended. The free memory changes with no sign.
pub const LOOK_EVERY: Duration = Duration::from_secs(5);

/// One GB: 2^30 bytes, as `MemAvailable` counts.
const GB: u64 = 1 << 30;

/// The limit of the host: at most `sessions` live sessions, and no start
/// while the free memory is below `floor_gb`.
///
/// ```
/// use riff::room::Limit;
///
/// let limit = Limit::default();
/// assert_eq!((limit.sessions, limit.floor_gb), (3, 8));
/// assert_eq!(serde_json::to_string(&limit).unwrap(), r#"{"sessions":3,"floor_gb":8}"#);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limit {
    /// The most live sessions.
    pub sessions: u32,
    /// The floor of free memory, in GB.
    pub floor_gb: u64,
}

impl Default for Limit {
    fn default() -> Self {
        Limit {
            sessions: SESSIONS,
            floor_gb: FLOOR_GB,
        }
    }
}

/// Why a host has no room now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Full {
    /// `live` sessions live, and the limit is `limit`.
    Sessions { live: usize, limit: u32 },
    /// The free memory is `free_gb`, below the floor `floor_gb`.
    Memory { free_gb: u64, floor_gb: u64 },
}

impl Limit {
    /// The limit in the folder `dir` of the daemon: its [`LIMIT_FILE`],
    /// or [`Limit::default`] when the file is not there. A file that riff
    /// cannot read is an error.
    pub fn read(dir: &Path) -> Result<Limit> {
        let path = dir.join(LIMIT_FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("cannot read the limit in {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Limit::default()),
            Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    /// Writes the limit to [`LIMIT_FILE`] in the folder `dir`, in place
    /// of the old one.
    pub fn write(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
        let path = dir.join(LIMIT_FILE);
        let tmp = path.with_extension("json.new");
        std::fs::write(&tmp, serde_json::to_vec(self)?)
            .with_context(|| format!("cannot write {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))
    }

    /// Has the host room for one more session, with `live` sessions and
    /// slots, and `free` bytes of free memory? With no number for the
    /// free memory, only the sessions count.
    ///
    /// ```
    /// use riff::room::{Full, Limit};
    ///
    /// let gb = 1u64 << 30;
    /// let limit = Limit { sessions: 2, floor_gb: 8 };
    /// assert_eq!(limit.room(1, Some(9 * gb)), Ok(()));
    /// assert_eq!(limit.room(2, Some(9 * gb)), Err(Full::Sessions { live: 2, limit: 2 }));
    /// assert_eq!(limit.room(0, Some(7 * gb)), Err(Full::Memory { free_gb: 7, floor_gb: 8 }));
    /// assert_eq!(limit.room(0, Some(8 * gb)), Ok(()));
    /// assert_eq!(limit.room(0, None), Ok(()));
    /// assert_eq!(Limit { sessions: 0, floor_gb: 0 }.room(0, None), Err(Full::Sessions { live: 0, limit: 0 }));
    /// ```
    pub fn room(&self, live: usize, free: Option<u64>) -> Result<(), Full> {
        if live >= usize::try_from(self.sessions).unwrap_or(usize::MAX) {
            return Err(Full::Sessions {
                live,
                limit: self.sessions,
            });
        }
        match free {
            Some(free) if free < self.floor_gb.saturating_mul(GB) => Err(Full::Memory {
                free_gb: free / GB,
                floor_gb: self.floor_gb,
            }),
            _ => Ok(()),
        }
    }
}

/// The free memory of the text of `/proc/meminfo`, in bytes:
/// `MemAvailable`.
///
/// ```
/// let text = "MemTotal:       32768000 kB\nMemFree:  1000 kB\nMemAvailable:   16384000 kB\n";
/// assert_eq!(riff::room::mem_available(text), Some(16_384_000 * 1024));
/// assert_eq!(riff::room::mem_available("MemTotal: 1 kB\n"), None);
/// ```
pub fn mem_available(meminfo: &str) -> Option<u64> {
    meminfo.lines().find_map(|line| {
        let kb = line.strip_prefix("MemAvailable:")?;
        let kb: u64 = kb.trim().strip_suffix("kB")?.trim().parse().ok()?;
        kb.checked_mul(1024)
    })
}

/// The free memory of this host, in bytes, or `None` when the host does
/// not say.
pub fn free_memory() -> Option<u64> {
    mem_available(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

/// The source of the free memory: [`free_memory`], or a fake in a test.
pub type Memory = Arc<dyn Fn() -> Option<u64> + Send + Sync>;

/// The line of starts that wait for room, and the slots that they took.
pub struct Room {
    /// The folder of the daemon: the limit is there.
    dir: PathBuf,
    memory: Memory,
    /// One start at a time looks for room, in the order of the asks.
    gate: tokio::sync::Mutex<()>,
    /// A slot went, or a session ended.
    changed: Notify,
    /// The slots that count as live sessions now.
    slots: AtomicU64,
    /// The starts that wait for room now, by the number of their ask.
    waiting: Mutex<BTreeMap<u64, String>>,
    asks: AtomicU64,
}

/// The room of one start: it counts as a live session until it goes.
/// Drop it when the daemon owns the session, or when the start fails.
pub struct Slot {
    room: Arc<Room>,
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.room.slots.fetch_sub(1, Ordering::SeqCst);
        self.room.changed.notify_waiters();
    }
}

/// Takes the start `ask` out of the line of the waits when it ends.
struct Waiting<'a> {
    room: &'a Room,
    ask: u64,
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.room
            .waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.ask);
    }
}

impl Room {
    /// The room of the daemon in the folder `dir`, with the free memory
    /// of `memory`.
    pub fn new(dir: PathBuf, memory: Memory) -> Arc<Room> {
        Arc::new(Room {
            dir,
            memory,
            gate: tokio::sync::Mutex::new(()),
            changed: Notify::new(),
            slots: AtomicU64::new(0),
            waiting: Mutex::new(BTreeMap::new()),
            asks: AtomicU64::new(0),
        })
    }

    /// The limit now: the file in the folder of the daemon. A file that
    /// riff cannot read gives the default, and the daemon says why.
    pub fn limit(&self) -> Limit {
        Limit::read(&self.dir).unwrap_or_else(|e| {
            eprintln!("riff daemon: {e:#}");
            Limit::default()
        })
    }

    /// The starts that wait for room, in order: the name of each.
    pub fn waiting(&self) -> Vec<String> {
        self.waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .cloned()
            .collect()
    }

    /// Tells the starts that wait: a session ended.
    pub fn ended(&self) {
        self.changed.notify_waiters();
    }

    /// Waits for room for the start `name`, in order, and gives its slot.
    /// `live` counts the sessions that the daemon owns. The first time
    /// that the host has no room, it gives `full` the reason.
    pub async fn admit(
        self: &Arc<Self>,
        name: &str,
        live: impl Fn() -> usize,
        mut full: impl FnMut(Full),
    ) -> Slot {
        let ask = self.asks.fetch_add(1, Ordering::SeqCst);
        self.waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(ask, name.to_owned());
        let _waiting = Waiting { room: self, ask };
        let _gate = self.gate.lock().await;
        let mut told = false;
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let slots = usize::try_from(self.slots.load(Ordering::SeqCst)).unwrap_or(usize::MAX);
            match self.limit().room(live() + slots, (self.memory)()) {
                Ok(()) => {
                    self.slots.fetch_add(1, Ordering::SeqCst);
                    return Slot { room: self.clone() };
                }
                Err(why) => {
                    if !told {
                        told = true;
                        full(why);
                    }
                    tokio::select! {
                        () = changed => {}
                        () = tokio::time::sleep(LOOK_EVERY) => {}
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(dir: &Path, free_gb: Arc<AtomicU64>) -> Arc<Room> {
        Room::new(
            dir.to_owned(),
            Arc::new(move || Some(free_gb.load(Ordering::SeqCst) * GB)),
        )
    }

    #[test]
    fn a_limit_goes_to_its_file_and_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Limit::read(dir.path()).unwrap(), Limit::default());
        let limit = Limit {
            sessions: 5,
            floor_gb: 0,
        };
        limit.write(dir.path()).unwrap();
        assert_eq!(Limit::read(dir.path()).unwrap(), limit);
        std::fs::write(dir.path().join(LIMIT_FILE), "no").unwrap();
        assert!(Limit::read(dir.path()).is_err());
    }

    #[tokio::test]
    async fn a_start_waits_for_a_slot_and_the_order_holds() {
        let dir = tempfile::tempdir().unwrap();
        Limit {
            sessions: 1,
            floor_gb: 0,
        }
        .write(dir.path())
        .unwrap();
        let room = room(dir.path(), Arc::new(AtomicU64::new(0)));
        let first = room.admit("a", || 0, |_| panic!("room")).await;
        let mut said = Vec::new();
        let second = {
            let room = room.clone();
            tokio::spawn(async move {
                let mut full = None;
                let slot = room.admit("b", || 0, |why| full = Some(why)).await;
                (slot, full)
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(room.waiting(), ["b"]);
        assert!(!second.is_finished());
        drop(first);
        let (slot, full) = second.await.unwrap();
        said.push(full);
        assert_eq!(said, [Some(Full::Sessions { live: 1, limit: 1 })]);
        assert!(room.waiting().is_empty());
        drop(slot);
    }

    #[tokio::test]
    async fn a_start_waits_while_the_free_memory_is_below_the_floor() {
        let dir = tempfile::tempdir().unwrap();
        Limit {
            sessions: 3,
            floor_gb: 8,
        }
        .write(dir.path())
        .unwrap();
        let free = Arc::new(AtomicU64::new(2));
        let room = room(dir.path(), free.clone());
        let waits = {
            let room = room.clone();
            tokio::spawn(async move { room.admit("a", || 0, |_| {}).await })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!waits.is_finished());
        free.store(9, Ordering::SeqCst);
        room.ended();
        let _slot = tokio::time::timeout(LOOK_EVERY * 2, waits)
            .await
            .unwrap()
            .unwrap();
    }

    #[test]
    fn the_free_memory_of_this_host_is_a_number() {
        if cfg!(target_os = "linux") {
            assert!(free_memory().is_some_and(|free| free > 0));
        }
    }
}
