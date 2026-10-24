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
//!
//! # The caps of a container
//!
//! The same file holds the caps of each new container of the host
//! (01M56Q4AQ86V99RC8CA1KGCMSZ): its CPUs, which are also its build jobs
//! in `CARGO_BUILD_JOBS`, and its memory. The daemon puts them in the
//! spec of a session when it starts it ([`Limit::caps`]). So a session
//! that runs keeps the caps of its start. The defaults are 8 CPUs and
//! 8 GB.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

use crate::runtime::{Env, SessionSpec};

/// The default number of live sessions of a host
/// (01M4Z60B7Y4XD6TJCYEYJT2R4S).
pub const SESSIONS: u32 = 3;

/// The default floor of free memory, in GB (01M4Z60B7Y4XD6TJCYEYJT2R4S).
pub const FLOOR_GB: u64 = 8;

/// The default CPUs and build jobs of each container
/// (01M56Q4AQ86V99RC8CA1KGCMSZ).
pub const CPUS: u32 = 8;

/// The default memory cap of each container, in GB
/// (01M56Q4AQ86V99RC8CA1KGCMSZ).
pub const MEMORY_GB: u64 = 8;

/// The variable of the build jobs of a container.
pub const JOBS_VAR: &str = "CARGO_BUILD_JOBS";

/// The file of the limit, in the folder of the daemon.
pub const LIMIT_FILE: &str = "limit.json";

/// How often a start that waits looks for room again, when no session
/// ended. The free memory changes with no sign.
pub const LOOK_EVERY: Duration = Duration::from_secs(5);

/// One GB: 2^30 bytes, as `MemAvailable` counts.
const GB: u64 = 1 << 30;

/// The effort of a Claude Code session: the levels of `claude --effort`.
/// The default is `xhigh`.
///
/// ```
/// use riff::room::Effort;
///
/// assert_eq!("xhigh".parse(), Ok(Effort::Xhigh));
/// assert_eq!(Effort::default().to_string(), "xhigh");
/// let e = "huge".parse::<Effort>().unwrap_err();
/// assert!(e.contains("low, medium, high, xhigh or max"), "{e}");
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    #[default]
    Xhigh,
    Max,
}

impl std::fmt::Display for Effort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        })
    }
}

impl std::str::FromStr for Effort {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        Ok(match s {
            "low" => Effort::Low,
            "medium" => Effort::Medium,
            "high" => Effort::High,
            "xhigh" => Effort::Xhigh,
            "max" => Effort::Max,
            _ => {
                return Err(format!(
                    "riff does not know the effort {s}: Claude Code has low, medium, high, xhigh or max"
                ));
            }
        })
    }
}

/// The role of a session for its effort. The item of the session tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Lead,
    Verify,
    Work,
}

impl Role {
    /// The role of the session of `item`: `lead`, `verify-issue-N` or
    /// any other item.
    ///
    /// ```
    /// use riff::room::Role;
    /// assert_eq!(Role::of("lead"), Role::Lead);
    /// assert_eq!(Role::of("verify-issue-7"), Role::Verify);
    /// assert_eq!(Role::of("issue-7"), Role::Work);
    /// ```
    pub fn of(item: &str) -> Role {
        match item {
            "lead" => Role::Lead,
            _ if item.starts_with("verify-") => Role::Verify,
            _ => Role::Work,
        }
    }
}

/// The limit of the host: at most `sessions` live sessions, and no start
/// while the free memory is below `floor_gb`. Each new container gets
/// `cpus` CPUs and build jobs, and `memory_gb` GB of memory. It also
/// holds the effort of the three roles: the daemon starts each new
/// session with `claude --effort LEVEL` ([`Limit::effort`]), `xhigh` by
/// default (a requirement of the book). A session that runs keeps its
/// effort. A value that the file does not have is the default.
///
/// ```
/// use riff::room::Limit;
///
/// let limit = Limit::default();
/// assert_eq!((limit.sessions, limit.floor_gb, limit.cpus, limit.memory_gb), (3, 8, 8, 8));
/// assert_eq!(
///     serde_json::to_string(&limit).unwrap(),
///     r#"{"sessions":3,"floor_gb":8,"cpus":8,"memory_gb":8,"effort":"xhigh","effort_lead":"xhigh","effort_verify":"xhigh"}"#
/// );
/// let old: Limit = serde_json::from_str(r#"{"sessions":2,"floor_gb":4}"#).unwrap();
/// assert_eq!(old, Limit { sessions: 2, floor_gb: 4, ..Limit::default() });
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Limit {
    /// The most live sessions.
    pub sessions: u32,
    /// The floor of free memory, in GB.
    pub floor_gb: u64,
    /// The CPUs and the build jobs of each new container.
    pub cpus: u32,
    /// The memory cap of each new container, in GB.
    pub memory_gb: u64,
    /// The effort of a work item.
    pub effort: Effort,
    /// The effort of the lead.
    pub effort_lead: Effort,
    /// The effort of a verify.
    pub effort_verify: Effort,
}

impl Default for Limit {
    fn default() -> Self {
        Limit {
            sessions: SESSIONS,
            floor_gb: FLOOR_GB,
            cpus: CPUS,
            memory_gb: MEMORY_GB,
            effort: Effort::default(),
            effort_lead: Effort::default(),
            effort_verify: Effort::default(),
        }
    }
}

/// The values that `riff daemon limit` sets. `None` keeps the value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Change {
    /// `--sessions`.
    pub sessions: Option<u32>,
    /// `--floor`, in GB.
    pub floor_gb: Option<u64>,
    /// `--cpus`.
    pub cpus: Option<u32>,
    /// `--memory`, in GB.
    pub memory_gb: Option<u64>,
    /// `--effort`: the effort of work items.
    pub effort: Option<Effort>,
    /// `--effort-lead`.
    pub effort_lead: Option<Effort>,
    /// `--effort-verify`.
    pub effort_verify: Option<Effort>,
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

    /// Puts the caps of a container in `spec`: `--cpus`, the same number
    /// in `CARGO_BUILD_JOBS`, and `--memory` (01M56Q4AQ86V99RC8CA1KGCMSZ).
    ///
    /// ```
    /// use riff::room::Limit;
    /// use riff::runtime::{Env, SessionSpec};
    ///
    /// let mut spec = SessionSpec {
    ///     name: "s".into(),
    ///     image: "i".into(),
    ///     home: "/h".into(),
    ///     workdir: "/w".into(),
    ///     binds: vec![],
    ///     env: vec![Env::Value("CARGO_BUILD_JOBS".into(), "8".into())],
    ///     memory: "8g".into(),
    ///     cpus: 8,
    ///     command: vec![],
    /// };
    /// Limit { cpus: 12, memory_gb: 16, ..Limit::default() }.caps(&mut spec);
    /// assert_eq!((spec.cpus, spec.memory.as_str()), (12, "16g"));
    /// assert_eq!(spec.env, [Env::Value("CARGO_BUILD_JOBS".into(), "12".into())]);
    /// ```
    pub fn caps(&self, spec: &mut SessionSpec) {
        spec.cpus = self.cpus;
        spec.memory = format!("{}g", self.memory_gb);
        spec.env
            .retain(|e| !matches!(e, Env::Value(name, _) if name == JOBS_VAR));
        spec.env
            .push(Env::Value(JOBS_VAR.into(), self.cpus.to_string()));
    }

    /// The effort of a session of `role`.
    ///
    /// ```
    /// use riff::room::{Effort, Limit, Role};
    ///
    /// let limit = Limit { effort_lead: Effort::Max, ..Limit::default() };
    /// assert_eq!(limit.effort_of(Role::Lead), Effort::Max);
    /// assert_eq!(limit.effort_of(Role::Work), Effort::Xhigh);
    /// ```
    pub fn effort_of(&self, role: Role) -> Effort {
        match role {
            Role::Lead => self.effort_lead,
            Role::Verify => self.effort_verify,
            Role::Work => self.effort,
        }
    }

    /// Puts `--effort LEVEL` after `claude` in the command of `spec`, for
    /// the session of `item`. A command with no `claude` stays as it is.
    ///
    /// ```
    /// use riff::room::{Effort, Limit};
    /// use riff::runtime::SessionSpec;
    ///
    /// let mut spec = SessionSpec {
    ///     name: "s".into(), image: "i".into(), home: "/h".into(), workdir: "/w".into(),
    ///     binds: vec![], env: vec![], memory: "8g".into(), cpus: 8,
    ///     command: vec!["claude".into(), "-p".into()],
    /// };
    /// Limit { effort_verify: Effort::Max, ..Limit::default() }.effort(&mut spec, "verify-issue-7");
    /// assert_eq!(spec.command, ["claude", "--effort", "max", "-p"]);
    /// ```
    pub fn effort(&self, spec: &mut SessionSpec, item: &str) {
        if let Some(at) = spec.command.iter().position(|a| a == "claude") {
            let level = self.effort_of(Role::of(item)).to_string();
            spec.command
                .splice(at + 1..at + 1, ["--effort".to_owned(), level]);
        }
    }

    /// Has the host room for one more session, with `live` sessions and
    /// slots, and `free` bytes of free memory? With no number for the
    /// free memory, only the sessions count.
    ///
    /// ```
    /// use riff::room::{Full, Limit};
    ///
    /// let gb = 1u64 << 30;
    /// let limit = Limit { sessions: 2, floor_gb: 8, ..Limit::default() };
    /// assert_eq!(limit.room(1, Some(9 * gb)), Ok(()));
    /// assert_eq!(limit.room(2, Some(9 * gb)), Err(Full::Sessions { live: 2, limit: 2 }));
    /// assert_eq!(limit.room(0, Some(7 * gb)), Err(Full::Memory { free_gb: 7, floor_gb: 8 }));
    /// assert_eq!(limit.room(0, Some(8 * gb)), Ok(()));
    /// assert_eq!(limit.room(0, None), Ok(()));
    /// assert_eq!(Limit { sessions: 0, floor_gb: 0, ..Limit::default() }.room(0, None), Err(Full::Sessions { live: 0, limit: 0 }));
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

/// `riff daemon limit`: sets the values of `change` in the folder `dir`
/// of the daemon, and gives the limit (01M4ZV3B6HAFTSC2K63FMC5FYA,
/// 01M56Q4ASHTDWVA2MS2CBQ2PAM). With no value, it changes nothing. It
/// refuses 0 CPUs and 0 GB of memory.
///
/// ```
/// use riff::room::{Change, Effort, Limit, set};
///
/// let dir = tempfile::tempdir()?;
/// let limit = set(dir.path(), Change::default())?;
/// assert_eq!(limit, Limit::default());
/// assert!(!dir.path().join(riff::room::LIMIT_FILE).exists());
/// let limit = set(dir.path(), Change { sessions: Some(5), ..Change::default() })?;
/// assert_eq!((limit.sessions, limit.floor_gb), (5, 8));
/// let limit = set(dir.path(), Change { cpus: Some(6), memory_gb: Some(12), ..Change::default() })?;
/// assert_eq!((limit.sessions, limit.cpus, limit.memory_gb), (5, 6, 12));
/// assert!(set(dir.path(), Change { cpus: Some(0), ..Change::default() }).is_err());
/// assert!(set(dir.path(), Change { memory_gb: Some(0), ..Change::default() }).is_err());
/// let max = Change { effort_lead: Some(Effort::Max), ..Change::default() };
/// assert_eq!(set(dir.path(), max)?.effort_lead, Effort::Max);
/// assert_eq!(Limit::read(dir.path())?.cpus, 6);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn set(dir: &Path, change: Change) -> Result<Limit> {
    if change.cpus == Some(0) {
        bail!("riff refuses --cpus 0: a container needs 1 CPU or more");
    }
    if change.memory_gb == Some(0) {
        bail!("riff refuses --memory 0: a container needs 1 GB of memory or more");
    }
    let mut limit = Limit::read(dir)?;
    if change == Change::default() {
        return Ok(limit);
    }
    let Change {
        sessions,
        floor_gb,
        cpus,
        memory_gb,
        effort,
        effort_lead,
        effort_verify,
    } = change;
    limit.sessions = sessions.unwrap_or(limit.sessions);
    limit.floor_gb = floor_gb.unwrap_or(limit.floor_gb);
    limit.cpus = cpus.unwrap_or(limit.cpus);
    limit.memory_gb = memory_gb.unwrap_or(limit.memory_gb);
    limit.effort = effort.unwrap_or(limit.effort);
    limit.effort_lead = effort_lead.unwrap_or(limit.effort_lead);
    limit.effort_verify = effort_verify.unwrap_or(limit.effort_verify);
    limit.write(dir)?;
    Ok(limit)
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
            ..Limit::default()
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
            ..Limit::default()
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
            ..Limit::default()
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
