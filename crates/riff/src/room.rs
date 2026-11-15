//! The limit of the host (#719, 01M4Z60B7Y4XD6TJCYEYJT2R4S): the riff
//! daemon starts a session only when the host has room for it.
//!
//! # Design
//!
//! Each container has its own `--memory` and `--cpus`. They do not limit
//! the sum: three builds of 6 GB each fill a host of 30 GB with no swap.
//! So the daemon keeps one limit for the whole host ([`Limit`]): the
//! number of live sessions, a floor of free memory and a floor of free
//! disk. A host has room when the free disk ([`Room::space`]) is at or
//! above its floor, fewer sessions than the limit live, and the free
//! memory ([`crate::platform::Memory::available`]) is at or above its
//! floor.
//!
//! The free disk is the free space of the disk of each folder that the
//! room watches ([`Room::watch`], 01M7G43090V1F263GYK3GQEXCZ): the folder
//! of the daemon, the folder of its sessions, and the storage of the
//! runtime ([`crate::runtime::Runtime::storage`]). A build in a session
//! writes its clone, and podman writes the layers of each container and
//! its volumes. When the folders are on more than one disk, the disk with
//! the least free space counts.
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
//!     G --> R{"room? free disk >= its floor,<br/>live + slots < sessions,<br/>free memory >= its floor"}
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
//! [`Limit::default`]: 3 sessions, a floor of 8 GB of memory and a floor
//! of 20 GB of disk.
//!
//! # The caps of a container
//!
//! The same file holds the caps of each new container of the host
//! (01M56Q4AQ86V99RC8CA1KGCMSZ): its CPUs and its memory. The daemon puts
//! them in the spec of a session when it starts it ([`Limit::caps`]). So a
//! session that runs keeps the caps of its start. The defaults are 8 CPUs
//! and 8 GB. riff sets no build jobs: a tool reads the CPUs of its
//! container, and a repository can set a variable in its `riff.toml`
//! (01M5SMW28KT5D5HPRH7KAZ8TVC).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

use riff_client::machines::Disk;

use crate::runtime::SessionSpec;

/// The default number of live sessions of a host
/// (01M4Z60B7Y4XD6TJCYEYJT2R4S).
pub const SESSIONS: u32 = 3;

/// The default floor of free memory, in GB (01M4Z60B7Y4XD6TJCYEYJT2R4S).
pub const FLOOR_GB: u64 = 8;

/// The default floor of free disk, in GB (01M7G43090V1F263GYK3GQEXCZ).
pub const DISK_GB: u64 = 20;

/// The default CPUs of each container
/// (01M56Q4AQ86V99RC8CA1KGCMSZ).
pub const CPUS: u32 = 8;

/// The default memory cap of each container, in GB
/// (01M56Q4AQ86V99RC8CA1KGCMSZ).
pub const MEMORY_GB: u64 = 8;

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

/// The limit of the host: at most `sessions` live sessions, no start
/// while the free memory is below `floor_gb`, and no start while the free
/// disk is below `disk_gb`. Each new container gets
/// `cpus` CPUs, and `memory_gb` GB of memory. It also
/// holds the effort of the three roles: the daemon starts each new
/// session with `claude --effort LEVEL` ([`Limit::effort`]), `xhigh` by
/// default (a requirement of the book). A session that runs keeps its
/// effort. A value that the file does not have is the default.
///
/// ```
/// use riff::room::Limit;
///
/// let limit = Limit::default();
/// assert_eq!((limit.sessions, limit.floor_gb, limit.disk_gb), (3, 8, 20));
/// assert_eq!((limit.cpus, limit.memory_gb), (8, 8));
/// assert_eq!(
///     serde_json::to_string(&limit).unwrap(),
///     r#"{"sessions":3,"floor_gb":8,"disk_gb":20,"cpus":8,"memory_gb":8,"effort":"xhigh","effort_lead":"xhigh","effort_verify":"xhigh"}"#
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
    /// The floor of free disk, in GB.
    pub disk_gb: u64,
    /// The CPUs of each new container.
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
            disk_gb: DISK_GB,
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
    /// `--disk`, in GB.
    pub disk_gb: Option<u64>,
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
    /// The free disk is `free_gb`, below the floor `floor_gb`.
    Disk { free_gb: u64, floor_gb: u64 },
}

/// The free disk of the host ([`Room::space`]): the disk with the least
/// free space of the folders that the room watches, and one of its
/// folders (01M7G430CFEK4B0FDG1552G8VE).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Space {
    /// A folder on the disk.
    pub folder: PathBuf,
    /// The free space and the size of the disk.
    pub disk: Disk,
}

impl Space {
    /// True when the free space is below the floor of `limit`.
    ///
    /// ```
    /// use riff::room::{Limit, Space};
    /// use riff_client::machines::Disk;
    ///
    /// let space = |free_gb| Space { folder: "/h".into(), disk: Disk { free_gb, total_gb: 915 } };
    /// let limit = Limit { disk_gb: 20, ..Limit::default() };
    /// assert!(space(19).low(&limit));
    /// assert!(!space(20).low(&limit));
    /// ```
    pub fn low(&self, limit: &Limit) -> bool {
        limit.disk_low(&self.disk)
    }
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

    /// Puts the caps of a container in `spec`: `--cpus` and `--memory`
    /// (01M56Q4AQ86V99RC8CA1KGCMSZ). It sets no variable: a tool reads the
    /// CPUs of its container (01M5SMW28KT5D5HPRH7KAZ8TVC).
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
    ///     env: vec![Env::Host("GH_TOKEN".into())],
    ///     memory: "8g".into(),
    ///     cpus: 8,
    ///     command: vec![],
    /// };
    /// Limit { cpus: 12, memory_gb: 16, ..Limit::default() }.caps(&mut spec);
    /// assert_eq!((spec.cpus, spec.memory.as_str()), (12, "16g"));
    /// assert_eq!(spec.env, [Env::Host("GH_TOKEN".into())]);
    /// ```
    pub fn caps(&self, spec: &mut SessionSpec) {
        spec.cpus = self.cpus;
        spec.memory = format!("{}g", self.memory_gb);
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

    /// True when the free space of `disk` is below the floor of free disk:
    /// the one rule of the floor (01M7G43090V1F263GYK3GQEXCZ).
    ///
    /// ```
    /// use riff::room::Limit;
    /// use riff_client::machines::Disk;
    ///
    /// let limit = Limit { disk_gb: 20, ..Limit::default() };
    /// assert!(limit.disk_low(&Disk { free_gb: 19, total_gb: 915 }));
    /// assert!(!limit.disk_low(&Disk { free_gb: 20, total_gb: 915 }));
    /// ```
    pub fn disk_low(&self, disk: &Disk) -> bool {
        disk.free_gb < self.disk_gb
    }

    /// Has the host room for one more session, with `live` sessions and
    /// slots, `free` bytes of free memory, and the free disk `disk`? With
    /// no number for the free memory or no disk, that one does not count.
    /// The free disk counts first: the end of a session does not make room
    /// on the disk.
    ///
    /// ```
    /// use riff::room::{Full, Limit};
    /// use riff_client::machines::Disk;
    ///
    /// let gb = 1u64 << 30;
    /// let disk = |free_gb| Some(Disk { free_gb, total_gb: 915 });
    /// let limit = Limit { sessions: 2, floor_gb: 8, disk_gb: 20, ..Limit::default() };
    /// assert_eq!(limit.room(1, Some(9 * gb), disk(20).as_ref()), Ok(()));
    /// assert_eq!(limit.room(2, Some(9 * gb), disk(50).as_ref()), Err(Full::Sessions { live: 2, limit: 2 }));
    /// assert_eq!(limit.room(0, Some(7 * gb), disk(50).as_ref()), Err(Full::Memory { free_gb: 7, floor_gb: 8 }));
    /// assert_eq!(limit.room(0, Some(8 * gb), disk(50).as_ref()), Ok(()));
    /// assert_eq!(limit.room(2, Some(9 * gb), disk(5).as_ref()), Err(Full::Disk { free_gb: 5, floor_gb: 20 }));
    /// assert_eq!(limit.room(0, None, None), Ok(()));
    /// let none = Limit { sessions: 0, floor_gb: 0, ..Limit::default() };
    /// assert_eq!(none.room(0, None, None), Err(Full::Sessions { live: 0, limit: 0 }));
    /// ```
    pub fn room(&self, live: usize, free: Option<u64>, disk: Option<&Disk>) -> Result<(), Full> {
        if let Some(disk) = disk.filter(|disk| self.disk_low(disk)) {
            return Err(Full::Disk {
                free_gb: disk.free_gb,
                floor_gb: self.disk_gb,
            });
        }
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
/// let limit = set(dir.path(), Change { disk_gb: Some(40), ..Change::default() })?;
/// assert_eq!((limit.sessions, limit.disk_gb), (5, 40));
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
        disk_gb,
        cpus,
        memory_gb,
        effort,
        effort_lead,
        effort_verify,
    } = change;
    limit.sessions = sessions.unwrap_or(limit.sessions);
    limit.floor_gb = floor_gb.unwrap_or(limit.floor_gb);
    limit.disk_gb = disk_gb.unwrap_or(limit.disk_gb);
    limit.cpus = cpus.unwrap_or(limit.cpus);
    limit.memory_gb = memory_gb.unwrap_or(limit.memory_gb);
    limit.effort = effort.unwrap_or(limit.effort);
    limit.effort_lead = effort_lead.unwrap_or(limit.effort_lead);
    limit.effort_verify = effort_verify.unwrap_or(limit.effort_verify);
    limit.write(dir)?;
    Ok(limit)
}

/// The free memory of this host, in bytes, or `None` when the host does
/// not say: [`Memory::available`](crate::platform::Memory::available).
pub fn free_memory() -> Option<u64> {
    crate::platform::memory().map(|memory| memory.available)
}

/// The source of the free memory: [`free_memory`], or a fake in a test.
pub type Memory = Arc<dyn Fn() -> Option<u64> + Send + Sync>;

/// The source of the free disk of the file system of a folder:
/// [`crate::disk::here`], or a fake in a test.
pub type DiskOf = Arc<dyn Fn(&Path) -> Option<Disk> + Send + Sync>;

/// The line of starts that wait for room, and the slots that they took.
pub struct Room {
    /// The folder of the daemon: the limit is there.
    dir: PathBuf,
    memory: Memory,
    disk: DiskOf,
    /// The folders whose disk counts for the floor of free disk
    /// ([`Room::watch`]).
    folders: Mutex<Vec<PathBuf>>,
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
    /// of `memory` and the free disk of `disk`. It watches the disk of
    /// `dir`.
    pub fn new(dir: PathBuf, memory: Memory, disk: DiskOf) -> Arc<Room> {
        Arc::new(Room {
            folders: Mutex::new(vec![dir.clone()]),
            dir,
            memory,
            disk,
            gate: tokio::sync::Mutex::new(()),
            changed: Notify::new(),
            slots: AtomicU64::new(0),
            waiting: Mutex::new(BTreeMap::new()),
            asks: AtomicU64::new(0),
        })
    }

    /// A new room with the free memory of `memory`, and the folder, the
    /// disk and the watched folders of this one, for a test.
    pub fn with_memory(&self, memory: Memory) -> Arc<Room> {
        let room = Room::new(self.dir.clone(), memory, self.disk.clone());
        *room.folders_lock() = self.folders_lock().clone();
        room
    }

    /// A new room with the free disk of `disk`, and the folder, the
    /// memory and the watched folders of this one, for a test.
    pub fn with_disk(&self, disk: DiskOf) -> Arc<Room> {
        let room = Room::new(self.dir.clone(), self.memory.clone(), disk);
        *room.folders_lock() = self.folders_lock().clone();
        room
    }

    fn folders_lock(&self) -> std::sync::MutexGuard<'_, Vec<PathBuf>> {
        self.folders.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The disk of `folder` counts for the floor of free disk from now on
    /// (01M7G43090V1F263GYK3GQEXCZ). A folder that the room watches
    /// already changes nothing.
    pub fn watch(&self, folder: PathBuf) {
        let mut folders = self.folders_lock();
        if !folders.contains(&folder) {
            folders.push(folder);
        }
    }

    /// The free disk now: the disk with the least free space of the
    /// folders that the room watches, and the first such folder. `None`
    /// when riff cannot measure the disk of any of them.
    ///
    /// ```
    /// use std::path::Path;
    /// use std::sync::Arc;
    /// use riff::room::Room;
    /// use riff_client::machines::Disk;
    ///
    /// let disk = Arc::new(|folder: &Path| match folder.to_str() {
    ///     Some("/d") => Some(Disk { free_gb: 40, total_gb: 900 }),
    ///     Some("/s") => Some(Disk { free_gb: 12, total_gb: 500 }),
    ///     _ => None,
    /// });
    /// let room = Room::new("/d".into(), Arc::new(|| None), disk);
    /// assert_eq!(room.space().unwrap().disk.free_gb, 40);
    /// room.watch("/none".into());
    /// room.watch("/s".into());
    /// let space = room.space().unwrap();
    /// assert_eq!((space.folder.to_str(), space.disk.free_gb), (Some("/s"), 12));
    /// ```
    pub fn space(&self) -> Option<Space> {
        let folders = self.folders_lock().clone();
        folders
            .into_iter()
            .filter_map(|folder| (self.disk)(&folder).map(|disk| Space { folder, disk }))
            .min_by_key(|space| space.disk.free_gb)
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

    /// Tells the starts that wait to look for room again: a session ended,
    /// or the free disk is at or above its floor again
    /// ([`crate::daemon::Daemon::look_disk`], 01M7G430FB68R1322AT8H94KYR).
    pub fn wake(&self) {
        self.changed.notify_waiters();
    }

    /// Waits for room for the start `name`, in order, and gives its slot.
    /// `live` counts the sessions that the daemon owns. The first time
    /// that the host has no room, it gives `full` the reason, and again
    /// each time that the kind of the reason changes
    /// (01M7G430CFEK4B0FDG1552G8VE): for example a session ended, and the
    /// disk is full now.
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
        let mut told = None;
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let slots = usize::try_from(self.slots.load(Ordering::SeqCst)).unwrap_or(usize::MAX);
            let space = self.space();
            let disk = space.as_ref().map(|space| &space.disk);
            match self.limit().room(live() + slots, (self.memory)(), disk) {
                Ok(()) => {
                    self.slots.fetch_add(1, Ordering::SeqCst);
                    return Slot { room: self.clone() };
                }
                Err(why) => {
                    let kind = std::mem::discriminant(&why);
                    if told != Some(kind) {
                        told = Some(kind);
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

    /// A room with `free_gb` GB of free memory, and no number for the
    /// disk.
    fn room(dir: &Path, free_gb: Arc<AtomicU64>) -> Arc<Room> {
        Room::new(
            dir.to_owned(),
            Arc::new(move || Some(free_gb.load(Ordering::SeqCst) * GB)),
            Arc::new(|_| None),
        )
    }

    /// 01M7G43090V1F263GYK3GQEXCZ, 01M7G430CFEK4B0FDG1552G8VE: a start
    /// waits while the free disk is below its floor, also after the end of
    /// a session, and it says the new reason. Over the floor, it starts.
    #[tokio::test]
    async fn a_start_waits_while_the_free_disk_is_below_the_floor() {
        let dir = tempfile::tempdir().unwrap();
        Limit {
            sessions: 1,
            floor_gb: 0,
            disk_gb: 20,
            ..Limit::default()
        }
        .write(dir.path())
        .unwrap();
        let free_disk = Arc::new(AtomicU64::new(100));
        let disk = {
            let free_disk = free_disk.clone();
            Arc::new(move |_: &Path| {
                Some(Disk {
                    free_gb: free_disk.load(Ordering::SeqCst),
                    total_gb: 915,
                })
            })
        };
        let room = room(dir.path(), Arc::new(AtomicU64::new(64))).with_disk(disk);
        let first = room.admit("a", || 0, |_| panic!("room")).await;
        let said = Arc::new(Mutex::new(Vec::new()));
        let second = {
            let (room, said) = (room.clone(), said.clone());
            tokio::spawn(async move { room.admit("b", || 0, |why| lock(&said).push(why)).await })
        };
        until(|| lock(&said).len() == 1).await;
        free_disk.store(5, Ordering::SeqCst);
        drop(first);
        until(|| lock(&said).len() == 2).await;
        assert!(!second.is_finished());
        let disk = Full::Disk {
            free_gb: 5,
            floor_gb: 20,
        };
        assert_eq!(*lock(&said), [Full::Sessions { live: 1, limit: 1 }, disk]);
        // The look at the disk wakes the start: it does not wait for
        // LOOK_EVERY.
        free_disk.store(20, Ordering::SeqCst);
        room.wake();
        let _slot = tokio::time::timeout(LOOK_EVERY / 2, second)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(lock(&said).len(), 2);
    }

    fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        mutex.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits until `done`, and looks again each few milliseconds. A test
    /// fails after 10 seconds.
    async fn until(done: impl Fn() -> bool) {
        let look = async {
            while !done() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(10), look)
            .await
            .unwrap();
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
        room.wake();
        let _slot = tokio::time::timeout(LOOK_EVERY * 2, waits)
            .await
            .unwrap()
            .unwrap();
    }

    #[test]
    fn the_free_memory_of_this_host_is_a_number() {
        assert!(free_memory().is_some_and(|free| free > 0));
    }
}
