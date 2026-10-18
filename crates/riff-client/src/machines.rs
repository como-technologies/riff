//! The machines model: the numbers of each host in `riff top` and
//! `riff workers` (01M56Y8KAPHJMSK89ZW1PA8B0E, 01M56Y8KF7EDWHX1JE5RA0XMCW).
//!
//! # Design
//!
//! A machine that runs workers tells its numbers in one status line of
//! riff-server ([`HostStatus::line`]): a `riff workers host` in its
//! status, and the `riff mcp` of the lead in the numbers of its machine
//! (01M515KSWVGCBS76AWZV74TJQ3). [`HostStatus::parse`] reads the line
//! back. The numbers are plain data with serde:
//!
//! | Type | Holds |
//! |---|---|
//! | [`HostStatus`] | The limit, the floor, the deaths and the workers of a host, with the parts below. |
//! | [`Machine`] | The hardware threads, the clock, the memory and the load (01M3Q5QE4SQ8VYN2PSF42KB3QJ). |
//! | [`Disk`] | The free disk of the main clone (01M41A11GHP78E2VYN14JSE27P). |
//! | [`Numbers`] | The numbers of the monitor (01M421QPX01BB15GJXHFYRETTX), with its [`Jobs`] and its last [`Kill`]. |
//! | [`Host`] | The user and the host of a machine, and its [`HostStatus`]. |
//!
//! [`machines`] makes the model from the sessions of `who`
//! (01M515KST9Z0B4B1D4HJV3Y2CM). The UI measures its own machine: the
//! reads of `/proc`, of the disk and of the settings stay in `riff`.
//!
//! ```mermaid
//! flowchart LR
//!     H["riff workers host on thelio"] -->|"its status: HostStatus::line"| S[riff-server]
//!     L["riff mcp of the lead on pangolin"] -->|"the numbers of its machine"| S
//!     S -->|who| M["machines: HostStatus::parse"]
//!     U["the machine of the UI"] -->|"its own numbers"| M
//!     M --> T["riff top: a line under each host"]
//! ```
//!
//! ```
//! use riff_client::machines::{HostStatus, Machine};
//!
//! let m = Machine { threads: 16, mhz: 4500, now_mhz: 4400, mem_gb: 32, avail_gb: 24, load: 1.5 };
//! let status = HostStatus { machine: Some(m), ..HostStatus::default() };
//! assert_eq!(
//!     status.line(),
//!     "workers host: limit 0, floor 4GB, cpu 16x4500MHz (now 4400MHz), mem 32GB, \
//!      24GB available, load 1.50, no workers"
//! );
//! assert_eq!(HostStatus::parse(&status.line()), Some(status));
//! ```

use std::fmt;
use std::time::Duration;

use riff_core::wire::{STATUS_CHARS, SessionInfo};
use serde::{Deserialize, Serialize};

use crate::sessions::Sessions;

/// The start of the status of a host.
pub const MARK: &str = "workers host";

/// The characters of the session ID of a worker in the status of a host.
pub const LONG_ID: usize = 8;

/// The characters of the session ID of a worker in the status of a host
/// that has no room for [`LONG_ID`] (01M517W4NP2WKSRCK52ZBVWBD3).
pub const SHORT_ID: usize = 4;

/// The default floor of available memory in GB, under which a host
/// starts no worker (01M3WFZ01PTAYYKG3T5CFA2W4D). A host of the release
/// before tells no floor: it gets this one.
pub const WORKERS_FLOOR: u32 = 4;

/// The CPU speed of a core that counts 1 in the score.
pub const BASE_MHZ: u32 = 3000;

/// Under this free part of the disk, in percent, riff removes the
/// `target` of each worktree with no live owner
/// (01M41A11BB4HAD8595DNSBAZ0D).
pub const TIGHT_PERCENT: u64 = 15;

/// Under this free part of the disk, in percent, riff starts no worker
/// on the machine (01M41A11DX1QRP48YPTDNT67W4).
pub const LOW_PERCENT: u64 = 5;

/// The oldest numbers that a session sent that [`machines`] takes
/// (01M515KST9Z0B4B1D4HJV3Y2CM). Older numbers count as none: the
/// session stopped sending them.
pub const NUMBERS_TTL: Duration = Duration::from_secs(180);

/// The numbers of a machine.
///
/// The score of a machine is the number of workers that it can run
/// well (01M3Q5QE76BZ27SZ14FFE8HM1G). One worker needs one hardware
/// thread and 2 GB of memory. A thread at [`BASE_MHZ`] counts 1, a
/// faster thread counts more:
///
/// ```text
/// score = min(threads, memory GB / 2) × MHz / 3000
/// ```
///
/// The score less the workers that run there is the free capacity
/// ([`Machine::free`]). A machine whose load average is more than its
/// hardware threads is busy ([`Machine::busy`]): riff starts no worker there. A
/// machine whose available memory is less than its floor is low
/// ([`Machine::low`]): riff starts no worker there too.
///
/// ```
/// use riff_client::machines::Machine;
///
/// let thelio = Machine { threads: 32, mhz: 5800, now_mhz: 4100, mem_gb: 128, avail_gb: 100, load: 3.0 };
/// let pangolin = Machine { threads: 16, mhz: 4500, now_mhz: 4400, mem_gb: 32, avail_gb: 3, load: 0.5 };
/// assert!(thelio.free(0) > pangolin.free(0));
/// assert!(!thelio.busy());
/// assert!(Machine { load: 33.0, ..thelio }.busy());
/// assert!(pangolin.low(4));
/// assert!(!thelio.low(4));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Machine {
    /// The hardware threads: the logical CPUs that the kernel runs at
    /// one time. A core with two threads counts 2. The physical cores
    /// are in [`Numbers::physical`].
    // The key `cores` is the wire name of 3.x: the daemon facts of
    // each host carry it (01M3N73ECKSDSH6820AKW9J8F7).
    #[serde(rename = "cores")]
    pub threads: u16,
    /// The most speed of one core, in MHz: the cap of the clock.
    pub mhz: u32,
    /// The clock now: the mean of the cores, in MHz.
    pub now_mhz: u32,
    /// The total memory, in GB.
    pub mem_gb: u32,
    /// The memory that is available now, in GB.
    pub avail_gb: u32,
    /// The 1-minute load average.
    pub load: f32,
}

impl Machine {
    /// The number of workers that this machine runs well. It counts
    /// the cap of the clock, not the clock now.
    ///
    /// ```
    /// use riff_client::machines::Machine;
    ///
    /// let m = Machine { threads: 8, mhz: 3000, now_mhz: 1400, mem_gb: 64, avail_gb: 64, load: 0.0 };
    /// assert_eq!(m.score(), 8.0);
    /// assert_eq!(Machine { mem_gb: 8, ..m }.score(), 4.0);
    /// assert_eq!(Machine { mhz: 6000, ..m }.score(), 16.0);
    /// ```
    pub fn score(&self) -> f64 {
        let size = f64::from(self.threads).min(f64::from(self.mem_gb) / 2.0);
        size * f64::from(self.mhz) / f64::from(BASE_MHZ)
    }

    /// The score less `workers`: the room for more workers.
    pub fn free(&self, workers: usize) -> f64 {
        self.score() - workers as f64
    }

    /// True when the load average is more than the hardware threads.
    pub fn busy(&self) -> bool {
        f64::from(self.load) > f64::from(self.threads)
    }

    /// True when the available memory is less than `floor_gb`
    /// (01M3WFZ01PTAYYKG3T5CFA2W4D): riff starts no worker on the machine.
    pub fn low(&self, floor_gb: u32) -> bool {
        self.avail_gb < floor_gb
    }

    /// The numbers in a status:
    /// `cpu 32x5883MHz (now 4100MHz), mem 124GB, 100GB available, load 18.96`.
    /// [`Machine::parse`] reads it back.
    ///
    /// ```
    /// use riff_client::machines::Machine;
    ///
    /// let m = Machine { threads: 32, mhz: 5883, now_mhz: 4100, mem_gb: 124, avail_gb: 100, load: 18.96 };
    /// assert_eq!(
    ///     m.to_string(),
    ///     "cpu 32x5883MHz (now 4100MHz), mem 124GB, 100GB available, load 18.96"
    /// );
    /// assert_eq!(Machine::parse(&m.to_string()), Some(m));
    /// assert_eq!(Machine::parse("limit 2"), None);
    /// // The release before tells no clock now: it is the CPU speed
    /// // (01M419XAZBPV0Y08CAR51KQSZS).
    /// let old = Machine::parse("cpu 32x5883MHz, mem 124GB, 100GB available, load 18.96").unwrap();
    /// assert_eq!((old.mhz, old.now_mhz), (5883, 5883));
    /// // A release before 1.0.0 tells no available memory: all of it counts.
    /// let old = Machine::parse("cpu 32x5883MHz, mem 124GB, load 18.96").unwrap();
    /// assert_eq!(old.avail_gb, 124);
    /// ```
    pub fn parse(text: &str) -> Option<Machine> {
        let rest = text.strip_prefix("cpu ")?;
        let (threads, rest) = rest.split_once('x')?;
        let (speed, rest) = rest.split_once(", mem ")?;
        // A host of the release before tells no clock now
        // (01M419XAZBPV0Y08CAR51KQSZS).
        let (mhz, now_mhz) = match speed.split_once("MHz (now ") {
            Some((mhz, now)) => (mhz.parse().ok()?, now.strip_suffix("MHz)")?.parse().ok()?),
            None => {
                let mhz = speed.strip_suffix("MHz")?.parse().ok()?;
                (mhz, mhz)
            }
        };
        let (mem, rest) = rest.split_once("GB, ")?;
        let mem_gb = mem.parse().ok()?;
        // A host of the release before tells no available memory
        // (01M407J917F9AH072C8DE80CRJ).
        let (avail_gb, load) = match rest.strip_prefix("load ") {
            Some(load) => (mem_gb, load),
            None => {
                let (avail, load) = rest.split_once("GB available, load ")?;
                (avail.parse().ok()?, load)
            }
        };
        Some(Machine {
            threads: threads.parse().ok()?,
            mhz,
            now_mhz,
            mem_gb,
            avail_gb,
            load: load.parse().ok()?,
        })
    }
}

impl fmt::Display for Machine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cpu {}x{}MHz (now {}MHz), mem {}GB, {}GB available, load {:.2}",
            self.threads, self.mhz, self.now_mhz, self.mem_gb, self.avail_gb, self.load
        )
    }
}

/// The size and the free space of a file system, in GB.
///
/// ```
/// use riff_client::machines::Disk;
///
/// let disk = Disk { free_gb: 16, total_gb: 455 };
/// assert_eq!(disk.percent(), 3);
/// assert!(disk.tight() && disk.low());
/// let fine = Disk { free_gb: 200, total_gb: 455 };
/// assert!(!fine.tight() && !fine.low());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Disk {
    /// The space that a user can write.
    pub free_gb: u64,
    /// The size of the file system.
    pub total_gb: u64,
}

impl Disk {
    /// The free part of the disk in percent. A disk of size zero counts
    /// as free.
    pub fn percent(&self) -> u64 {
        match self.total_gb {
            0 => 100,
            total => self.free_gb * 100 / total,
        }
    }

    /// True under [`TIGHT_PERCENT`]: riff removes build folders.
    pub fn tight(&self) -> bool {
        self.percent() < TIGHT_PERCENT
    }

    /// True under [`LOW_PERCENT`]: riff starts no worker.
    pub fn low(&self) -> bool {
        self.percent() < LOW_PERCENT
    }

    /// The disk in a status: `disk 16GB free of 455GB (3%)`. The part in
    /// brackets is optional.
    ///
    /// ```
    /// use riff_client::machines::Disk;
    ///
    /// let disk = Disk { free_gb: 16, total_gb: 455 };
    /// assert_eq!(disk.to_string(), "disk 16GB free of 455GB (3%)");
    /// assert_eq!(Disk::parse(&disk.to_string()), Some(disk));
    /// assert_eq!(Disk::parse("disk 16GB free of 455GB"), Some(disk));
    /// assert_eq!(Disk::parse("cpu 8x3000MHz"), None);
    /// ```
    pub fn parse(text: &str) -> Option<Disk> {
        let rest = text.strip_prefix("disk ")?;
        let (free, rest) = rest.split_once("GB free of ")?;
        let total = rest.split_once("GB").map_or(rest, |(total, _)| total);
        Some(Disk {
            free_gb: free.parse().ok()?,
            total_gb: total.parse().ok()?,
        })
    }
}

impl fmt::Display for Disk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "disk {}GB free of {}GB ({}%)",
            self.free_gb,
            self.total_gb,
            self.percent()
        )
    }
}

/// One kill of a process by `systemd-oomd` or by the kernel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kill {
    /// The time of the kill, in seconds since 1970.
    pub at: u64,
    /// `systemd-oomd` or `kernel`.
    pub by: String,
    /// What was killed: the scope of `systemd-oomd`, or the process of
    /// the kernel. Empty in a status.
    pub what: String,
}

/// The build jobs of a machine: the pool of its jobserver, or the jobs
/// of each worker (01M515PDFZZXQ9ZCQE7SQ6KB8G).
///
/// ```
/// use riff_client::machines::Jobs;
///
/// let pool = Jobs::Pool { size: 20, used: 3 };
/// assert_eq!(pool.to_string(), "pool 20 (3 in use)");
/// assert_eq!(pool.short(), "pool 20");
/// assert_eq!(Jobs::parse("pool 20 (3 in use)"), Some(pool));
/// assert_eq!(Jobs::Fixed(2).to_string(), "jobs 2");
/// assert_eq!(Jobs::Fixed(2).short(), "jobs 2");
/// assert_eq!(Jobs::parse("jobs 2"), Some(Jobs::Fixed(2)));
/// assert_eq!(Jobs::parse("pool 20"), None);
/// assert_eq!(Jobs::parse("jobs two"), None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Jobs {
    /// The pool of the machine: its tokens, and the tokens that are not
    /// in the pool now.
    Pool { size: u16, used: u16 },
    /// The jobs and test threads of each worker with no pool.
    Fixed(u16),
}

impl Jobs {
    /// The jobs in the form of their [`fmt::Display`], or `None` for
    /// another text.
    pub fn parse(text: &str) -> Option<Jobs> {
        if let Some(jobs) = text.strip_prefix("jobs ") {
            return jobs.parse().ok().map(Jobs::Fixed);
        }
        let (size, used) = text
            .strip_prefix("pool ")?
            .strip_suffix(" in use)")?
            .split_once(" (")?;
        Some(Jobs::Pool {
            size: size.parse().ok()?,
            used: used.parse().ok()?,
        })
    }

    /// The jobs with no tokens in use, for a line with little room:
    /// `pool 20` or `jobs 2`.
    pub fn short(&self) -> String {
        match self {
            Jobs::Pool { size, .. } => format!("pool {size}"),
            Jobs::Fixed(jobs) => format!("jobs {jobs}"),
        }
    }
}

impl fmt::Display for Jobs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Jobs::Pool { size, used } => write!(f, "pool {size} ({used} in use)"),
            Jobs::Fixed(jobs) => write!(f, "jobs {jobs}"),
        }
    }
}

/// The numbers of the monitor that a machine tells, after the numbers
/// of [`Machine`] (01M421QPX01BB15GJXHFYRETTX): the monitor on or off,
/// the 5-minute load and its limit, the physical cores, the jobs
/// ([`Jobs`]), and the last kill. A workers host puts them in its
/// status ([`HostStatus`]).
///
/// ```
/// use riff_client::machines::{Jobs, Kill, Numbers};
///
/// let n = Numbers {
///     on: true,
///     load5: 9.8,
///     limit: 12.0,
///     physical: 8,
///     jobs: Jobs::Pool { size: 10, used: 3 },
///     kill: Some(Kill { at: 1727980000, by: "systemd-oomd".into(), what: String::new() }),
/// };
/// assert_eq!(
///     n.to_string(),
///     "monitor on, load5 9.80 of 12.00, 8 cores, pool 10 (3 in use), kill 1727980000 systemd-oomd",
/// );
/// assert_eq!(Numbers::parse(&n.to_string()), Some(n.clone()));
/// let off = Numbers { on: false, kill: None, jobs: Jobs::Fixed(2), ..n };
/// assert_eq!(off.to_string(), "monitor off, load5 9.80 of 12.00, 8 cores, jobs 2");
/// assert_eq!(Numbers::parse(&off.to_string()), Some(off));
/// assert_eq!(Numbers::parse("disk 16GB free of 455GB"), None);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Numbers {
    /// `monitor.on` of the machine.
    pub on: bool,
    pub load5: f64,
    /// The limit of the 5-minute load.
    pub limit: f64,
    /// The physical cores.
    pub physical: u16,
    /// The pool of the machine, or the jobs of each worker.
    pub jobs: Jobs,
    /// The last kill that the monitor saw. Its `what` is empty.
    pub kill: Option<Kill>,
}

impl Numbers {
    /// The numbers in a status, or `None` for another text.
    pub fn parse(text: &str) -> Option<Numbers> {
        let rest = text.strip_prefix("monitor ")?;
        let (on, rest) = rest.split_once(", load5 ")?;
        let on = match on {
            "on" => true,
            "off" => false,
            _ => return None,
        };
        let (load5, rest) = rest.split_once(" of ")?;
        let (limit, rest) = rest.split_once(", ")?;
        let (physical, rest) = rest.split_once(" cores, ")?;
        let (jobs, kill) = match rest.split_once(", kill ") {
            Some((jobs, kill)) => {
                let (at, by) = kill.split_once(' ')?;
                let kill = Kill {
                    at: at.parse().ok()?,
                    by: by.to_owned(),
                    what: String::new(),
                };
                (jobs, Some(kill))
            }
            None => (rest, None),
        };
        Some(Numbers {
            on,
            load5: load5.parse().ok()?,
            limit: limit.parse().ok()?,
            physical: physical.parse().ok()?,
            jobs: Jobs::parse(jobs)?,
            kill,
        })
    }
}

impl fmt::Display for Numbers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "monitor {}, load5 {:.2} of {:.2}, {} cores, {}",
            if self.on { "on" } else { "off" },
            self.load5,
            self.limit,
            self.physical,
            self.jobs
        )?;
        if let Some(kill) = &self.kill {
            write!(f, ", kill {} {}", kill.at, kill.by)?;
        }
        Ok(())
    }
}

/// What the status of a host tells: its limit, its floor of available
/// memory (01M3WFZ01PTAYYKG3T5CFA2W4D), the numbers of its machine
/// (01M3Q5QE4SQ8VYN2PSF42KB3QJ), and the pane and short session ID of
/// each worker.
///
/// ```
/// use riff_client::machines::{Disk, HostStatus, Jobs, Machine, Numbers};
///
/// let none = HostStatus { limit: 2, floor: 4, deaths: 0, machine: None, disk: None, monitor: None, more: 0, workers: vec![] };
/// assert_eq!(none.line(), "workers host: limit 2, floor 4GB, no workers");
/// let two = HostStatus {
///     limit: 3,
///     floor: 4,
///     deaths: 0,
///     machine: Some(Machine { threads: 16, mhz: 4500, now_mhz: 4400, mem_gb: 32, avail_gb: 24, load: 1.5 }),
///     disk: None,
///     monitor: None,
///     more: 0,
///     workers: vec![("%3".into(), "1a2b3c4d".into()), ("%4".into(), "5e6f7a8b".into())],
/// };
/// assert_eq!(
///     two.line(),
///     "workers host: limit 3, floor 4GB, cpu 16x4500MHz (now 4400MHz), mem 32GB, 24GB available, \
///      load 1.50, workers: %3 1a2b3c4d, %4 5e6f7a8b",
/// );
/// assert_eq!(HostStatus::parse(&two.line()), Some(two.clone()));
/// assert_eq!(HostStatus::parse(&none.line()), Some(none.clone()));
/// assert_eq!(HostStatus::parse("idle: waits for work"), None);
///
/// // The disk of the host (01M41A11GHP78E2VYN14JSE27P).
/// let disk = Some(Disk { free_gb: 16, total_gb: 455 });
/// let with_disk = HostStatus { disk, ..two.clone() };
/// assert!(with_disk.line().contains(", load 1.50, disk 16GB free of 455GB (3%), workers: %3"));
/// assert_eq!(HostStatus::parse(&with_disk.line()), Some(with_disk.clone()));
/// let only_disk = HostStatus { disk, ..none.clone() };
/// assert_eq!(only_disk.line(), "workers host: limit 2, floor 4GB, disk 16GB free of 455GB (3%), no workers");
/// assert_eq!(HostStatus::parse(&only_disk.line()), Some(only_disk.clone()));
///
/// // The numbers of the monitor come after the disk
/// // (01M421QPX01BB15GJXHFYRETTX).
/// // The pool of build jobs, or the jobs of each worker
/// // (01M515PDFZZXQ9ZCQE7SQ6KB8G).
/// let jobs = Jobs::Pool { size: 10, used: 3 };
/// let monitor = Some(Numbers { on: true, load5: 9.8, limit: 12.0, physical: 8, jobs, kill: None });
/// let with_monitor = HostStatus { monitor: monitor.clone(), ..with_disk.clone() };
/// assert!(with_monitor.line().contains(
///     "(3%), monitor on, load5 9.80 of 12.00, 8 cores, pool 10 (3 in use), workers: %3"
/// ));
/// assert_eq!(HostStatus::parse(&with_monitor.line()), Some(with_monitor));
/// let monitor = monitor.map(|n| Numbers { jobs: Jobs::Fixed(2), ..n });
/// let only_monitor = HostStatus { monitor, ..none.clone() };
/// assert_eq!(
///     only_monitor.line(),
///     "workers host: limit 2, floor 4GB, monitor on, load5 9.80 of 12.00, 8 cores, jobs 2, no workers"
/// );
/// assert_eq!(HostStatus::parse(&only_monitor.line()), Some(only_monitor));
///
/// // A host of the release before: no floor, no available memory
/// // (01M407J917F9AH072C8DE80CRJ).
/// let old = HostStatus::parse(
///     "workers host: limit 3, cpu 16x4500MHz, mem 32GB, load 1.50, workers: %3 1a2b3c4d",
/// )
/// .unwrap();
/// assert_eq!((old.limit, old.floor), (3, riff_client::machines::WORKERS_FLOOR));
/// assert_eq!(old.machine.map(|m| m.avail_gb), Some(32));
/// assert_eq!(old.workers, [("%3".to_owned(), "1a2b3c4d".to_owned())]);
/// assert!(HostStatus::parse("workers host: limit 2, no workers").is_some());
///
/// // The deaths of the last hour come after the floor, when there are
/// // any (01M493YZZEW1FTDBNA090WT2AG).
/// let dying = HostStatus { deaths: 4, ..with_disk.clone() };
/// assert!(dying.line().starts_with("workers host: limit 3, floor 4GB, deaths 4, cpu 16x4500MHz"));
/// assert_eq!(HostStatus::parse(&dying.line()), Some(dying));
/// let only_deaths = HostStatus { deaths: 1, ..none.clone() };
/// assert_eq!(only_deaths.line(), "workers host: limit 2, floor 4GB, deaths 1, no workers");
/// assert_eq!(HostStatus::parse(&only_deaths.line()), Some(only_deaths));
///
/// // At most 400 characters at each size (01M517W4NP2WKSRCK52ZBVWBD3).
/// // With no room for 8 characters of each session ID, the line has 4.
/// // With no room for 4, it lists the first workers and counts the
/// // others.
/// let line = |n: u16| {
///     let workers = (0..n).map(|i| (format!("%{}", 100 + i), format!("{i:04}abcd"))).collect();
///     HostStatus { limit: 30, workers, ..with_disk.clone() }.line()
/// };
/// assert!(line(10).ends_with("workers: %100 0000abcd, %101 0001abcd, %102 0002abcd, %103 0003abcd, \
///     %104 0004abcd, %105 0005abcd, %106 0006abcd, %107 0007abcd, %108 0008abcd, %109 0009abcd"));
/// assert!(line(20).ends_with("workers: %100 0000, %101 0001, %102 0002, %103 0003, %104 0004, \
///     %105 0005, %106 0006, %107 0007, %108 0008, %109 0009, %110 0010, %111 0011, %112 0012, \
///     %113 0013, %114 0014, %115 0015, %116 0016, %117 0017, %118 0018, %119 0019"));
/// let thirty = line(30);
/// assert!(thirty.len() <= 400 && thirty.ends_with(", %122 0022, 7 more"), "{thirty}");
/// let parsed = HostStatus::parse(&thirty).unwrap();
/// assert_eq!((parsed.workers.len(), parsed.more, parsed.runs()), (23, 7, 30));
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostStatus {
    pub limit: u16,
    /// The available memory in GB under which the host starts no
    /// worker.
    pub floor: u32,
    /// The deaths of its workers in the last hour.
    pub deaths: usize,
    /// The numbers of the machine. `None` from a host that does not
    /// tell them.
    pub machine: Option<Machine>,
    /// The disk of the main clone (01M41A11GHP78E2VYN14JSE27P). `None`
    /// from a host that does not tell it.
    pub disk: Option<Disk>,
    /// The numbers of the monitor (01M421QPX01BB15GJXHFYRETTX). `None`
    /// from a host that does not tell them.
    pub monitor: Option<Numbers>,
    /// The pane and the first 8 characters of the session ID of each
    /// worker that the status lists: the first 4 when the line has no
    /// room for 8 (01M517W4NP2WKSRCK52ZBVWBD3).
    pub workers: Vec<(String, String)>,
    /// The workers that the status does not list, as the line has no
    /// room for them (01M517W4NP2WKSRCK52ZBVWBD3).
    pub more: usize,
}

impl Default for HostStatus {
    /// A host with a limit of 0, the default floor, and no numbers.
    fn default() -> Self {
        HostStatus {
            limit: 0,
            floor: WORKERS_FLOOR,
            deaths: 0,
            machine: None,
            disk: None,
            monitor: None,
            workers: Vec::new(),
            more: 0,
        }
    }
}

impl HostStatus {
    /// The workers of the host: the listed ones and the
    /// [`more`](Self::more).
    pub fn runs(&self) -> usize {
        self.workers.len() + self.more
    }

    /// The status line. It has at most [`STATUS_CHARS`] characters at
    /// each size (01M517W4NP2WKSRCK52ZBVWBD3). When the workers do not
    /// fit, each gets the first [`SHORT_ID`] characters of its session
    /// ID. When they still do not fit, the line lists the first ones and
    /// ends with the count of the others: `N more`.
    pub fn line(&self) -> String {
        let machine = self.machine.map(|m| format!("{m}, ")).unwrap_or_default();
        let disk = self.disk.map(|d| format!("{d}, ")).unwrap_or_default();
        let monitor = self
            .monitor
            .as_ref()
            .map(|n| format!("{n}, "))
            .unwrap_or_default();
        let deaths = match self.deaths {
            0 => String::new(),
            n => format!("deaths {n}, "),
        };
        let head = format!(
            "{MARK}: limit {}, floor {}GB, {deaths}{machine}{disk}{monitor}",
            self.limit, self.floor
        );
        if self.runs() == 0 {
            return format!("{head}no workers");
        }
        let line = |listed: usize, id: usize| {
            let mut workers: Vec<String> = self.workers[..listed]
                .iter()
                .map(|(pane, short)| {
                    format!("{pane} {}", short.chars().take(id).collect::<String>())
                })
                .collect();
            let more = self.runs() - listed;
            if more > 0 {
                workers.push(format!("{more} more"));
            }
            format!("{head}workers: {}", workers.join(", "))
        };
        let fits = |line: &String| line.chars().count() <= STATUS_CHARS;
        let all = self.workers.len();
        std::iter::once((all, LONG_ID))
            .chain((0..=all).rev().map(|listed| (listed, SHORT_ID)))
            .map(|(listed, id)| line(listed, id))
            .find(fits)
            .unwrap_or_else(|| line(0, SHORT_ID))
    }

    /// The host status in a status step, or `None` for another status.
    pub fn parse(step: &str) -> Option<Self> {
        let rest = step.strip_prefix(MARK)?.strip_prefix(": limit ")?;
        let (limit, rest) = rest.split_once(", ")?;
        let limit = limit.parse().ok()?;
        // A host of the release before tells no floor
        // (01M407J917F9AH072C8DE80CRJ).
        let (floor, mut rest) = match rest.strip_prefix("floor ") {
            Some(rest) => {
                let (floor, rest) = rest.split_once("GB, ")?;
                (floor.parse().ok()?, rest)
            }
            None => (WORKERS_FLOOR, rest),
        };
        let mut deaths = 0;
        if let Some(after) = rest.strip_prefix("deaths ") {
            let (count, after) = after.split_once(", ")?;
            deaths = count.parse().ok()?;
            rest = after;
        }
        // The disk comes after the numbers of the machine
        // (01M41A11GHP78E2VYN14JSE27P). A host of the release before
        // tells none.
        let mut disk = None;
        let without;
        if let Some(at) = rest.find("disk ") {
            let end = at + rest[at..].find(", ")?;
            disk = Some(Disk::parse(&rest[at..end])?);
            without = format!("{}{}", &rest[..at], &rest[end + 2..]);
            rest = &without;
        }
        // The numbers of the monitor come after the disk
        // (01M421QPX01BB15GJXHFYRETTX). A host of the release before
        // tells none.
        let mut monitor = None;
        let without_monitor;
        if let Some(at) = rest.find("monitor ") {
            let end = at
                + rest[at..]
                    .find(", no workers")
                    .or_else(|| rest[at..].find(", workers: "))?;
            monitor = Some(Numbers::parse(&rest[at..end])?);
            without_monitor = format!("{}{}", &rest[..at], &rest[end + 2..]);
            rest = &without_monitor;
        }
        let mut machine = None;
        if rest.starts_with("cpu ") {
            let end = rest
                .find(", no workers")
                .or_else(|| rest.find(", workers: "))?;
            machine = Some(Machine::parse(&rest[..end])?);
            rest = &rest[end + 2..];
        }
        if rest == "no workers" {
            return Some(HostStatus {
                limit,
                floor,
                deaths,
                machine,
                disk,
                monitor,
                workers: Vec::new(),
                more: 0,
            });
        }
        let mut listed: Vec<&str> = rest.strip_prefix("workers: ")?.split(", ").collect();
        let mut more = 0;
        if let Some(count) = listed.last().and_then(|w| w.strip_suffix(" more")) {
            more = count.parse().ok()?;
            listed.pop();
        }
        let workers = listed
            .into_iter()
            .map(|w| {
                w.split_once(' ')
                    .map(|(pane, short)| (pane.to_owned(), short.to_owned()))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(HostStatus {
            limit,
            floor,
            deaths,
            machine,
            disk,
            monitor,
            workers,
            more,
        })
    }
}

/// The numbers of one machine that runs workers: its user, its host and
/// its status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Host {
    pub user: String,
    pub host: String,
    pub status: HostStatus,
}

/// The machines that run workers, with their numbers
/// (01M515KST9Z0B4B1D4HJV3Y2CM). Each machine is a user and a host. Its
/// first source of these wins:
///
/// 1. the status of a live workers host of the machine on the server;
/// 2. `here`, the machine of the UI;
/// 3. the numbers that a live session of the machine sent in the last
///    [`NUMBERS_TTL`]: the lead sends the numbers of its machine
///    (01M515KSWVGCBS76AWZV74TJQ3).
///
/// ```
/// use riff_client::machines::{Host, HostStatus, machines};
/// use riff_client::sessions::Sessions;
/// use riff_core::wire::{MachineInfo, SessionInfo, Status, StatusInfo};
///
/// let status = HostStatus { limit: 2, ..HostStatus::default() };
/// let mut host: SessionInfo = serde_json::from_value(serde_json::json!({
///     "uri": "riff://mike@thelio/o/r?session=h1", "live": true,
/// }))?;
/// host.status = Some(StatusInfo { status: Status { step: status.line() }, age_secs: 1, stale: false });
/// let one = |sessions: Vec<SessionInfo>| Sessions::new(sessions, vec![]);
/// let here = Host { user: "mike".into(), host: "pangolin".into(), status: status.clone() };
/// let found = machines(&one(vec![host.clone()]), Some(here.clone()));
/// assert_eq!(found.len(), 2);
/// assert_eq!((found[0].user.as_str(), found[0].host.as_str()), ("mike", "thelio"));
/// assert_eq!(found[1], here);
/// // A workers host on the machine of the UI wins over the local numbers.
/// let there = Host { host: "thelio".into(), ..here.clone() };
/// assert_eq!(machines(&one(vec![host.clone()]), Some(there)).len(), 1);
///
/// // The lead on kadomony sent the numbers of its machine.
/// let sent = HostStatus { limit: 3, ..status.clone() };
/// let lead = SessionInfo {
///     uri: "riff://mike@kadomony/o/r?session=l1&lead=true".parse()?,
///     status: None,
///     machine: Some(MachineInfo { numbers: sent.line(), age_secs: 10 }),
///     ..host.clone()
/// };
/// let found = machines(&one(vec![host.clone(), lead.clone()]), None);
/// assert_eq!(found[1], Host { user: "mike".into(), host: "kadomony".into(), status: sent });
/// // Numbers older than NUMBERS_TTL count as none.
/// let old = MachineInfo { numbers: status.line(), age_secs: 181 };
/// let lead = SessionInfo { machine: Some(old), ..lead };
/// assert_eq!(machines(&one(vec![host, lead]), None).len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn machines(sessions: &Sessions, here: Option<Host>) -> Vec<Host> {
    let of = |s: &SessionInfo, numbers: &str| {
        Some(Host {
            user: s.uri.who().user().to_owned(),
            host: s.uri.place().host().to_owned(),
            status: HostStatus::parse(numbers)?,
        })
    };
    let live = || sessions.on_server().filter(|s| s.live);
    let hosts = live().filter_map(|s| of(s, &s.status.as_ref()?.status.step));
    let sent = live().filter_map(|s| {
        let machine = s.machine.as_ref()?;
        (machine.age_secs <= NUMBERS_TTL.as_secs()).then_some(())?;
        of(s, &machine.numbers)
    });
    let mut found: Vec<Host> = Vec::new();
    for machine in hosts.chain(here).chain(sent) {
        if !found
            .iter()
            .any(|m| m.user == machine.user && m.host == machine.host)
        {
            found.push(machine);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sample reply of `who`: a workers host that tells its status,
    /// and a lead that sent the numbers of its machine.
    const WHO: &str = r#"[
        {"uri": "riff://mike@thelio/o/r?session=h1", "live": true,
         "status": {"step": "workers host: limit 2, floor 4GB, cpu 16x4500MHz (now 4400MHz), mem 32GB, 24GB available, load 1.50, workers: %3 1a2b3c4d",
                    "age_secs": 1}},
        {"uri": "riff://mike@pangolin/o/r?session=l1&lead=true", "live": true,
         "machine": {"numbers": "workers host: limit 3, floor 8GB, no workers", "age_secs": 10}},
        {"uri": "riff://ann@heron/o/r?session=w1", "live": true, "worker": true}
    ]"#;

    #[test]
    fn the_model_comes_from_a_reply_of_who() {
        let who: Vec<SessionInfo> = serde_json::from_str(WHO).unwrap();
        let found = machines(&Sessions::new(who, vec![]), None);
        let hosts: Vec<(&str, &str)> = found
            .iter()
            .map(|m| (m.user.as_str(), m.host.as_str()))
            .collect();
        assert_eq!(hosts, [("mike", "thelio"), ("mike", "pangolin")]);
        assert_eq!(found[0].status.runs(), 1);
        assert_eq!(found[0].status.machine.map(|m| m.threads), Some(16));
        assert_eq!((found[1].status.limit, found[1].status.floor), (3, 8));
    }

    #[test]
    fn the_model_is_plain_data_with_serde() {
        let who: Vec<SessionInfo> = serde_json::from_str(WHO).unwrap();
        let found = machines(&Sessions::new(who, vec![]), None);
        let json = serde_json::to_string(&found).unwrap();
        let back: Vec<Host> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, found);
    }

    #[test]
    fn a_small_fast_machine_loses_to_a_big_one() {
        let big = Machine {
            threads: 32,
            mhz: 4000,
            now_mhz: 4000,
            mem_gb: 128,
            avail_gb: 128,
            load: 0.0,
        };
        let small = Machine {
            threads: 4,
            mhz: 6000,
            now_mhz: 6000,
            mem_gb: 16,
            avail_gb: 16,
            load: 0.0,
        };
        assert!(big.free(0) > small.free(0));
        // Each worker takes one from the free capacity.
        assert_eq!(small.free(3), small.score() - 3.0);
    }

    #[test]
    fn the_marks_of_the_disk_are_strict() {
        let at = |free_gb| Disk {
            free_gb,
            total_gb: 100,
        };
        assert!(!at(15).tight());
        assert!(at(14).tight());
        assert!(!at(5).low());
        assert!(at(4).low());
        let none = Disk {
            free_gb: 0,
            total_gb: 0,
        };
        assert_eq!(none.percent(), 100);
    }

    #[test]
    fn little_memory_caps_the_score() {
        let m = Machine {
            threads: 16,
            mhz: 3000,
            now_mhz: 3000,
            mem_gb: 4,
            avail_gb: 4,
            load: 0.0,
        };
        assert_eq!(m.score(), 2.0);
    }
}
