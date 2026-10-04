//! The monitor of a machine that runs workers: it tells the lead when
//! the load or the memory crosses a limit, and when a worker is killed.
//!
//! # Design
//!
//! While `monitor.on` is true, the monitor reads the health of its
//! machine each `monitor.every` seconds (01M421QPKWPX00X24F8V6DT8Z3,
//! [`crate::settings::monitor`]): the 1-minute and 5-minute load average
//! of `/proc/loadavg`, the available memory of `/proc/meminfo`, and each
//! kill of `systemd-oomd` or of the kernel in the journal ([`journal`]).
//! A workers host runs it ([`crate::host`]), and the `riff mcp` of the
//! lead runs it on the machine of the lead while its session is the
//! lead. One monitor runs on a machine at a time: it holds the lock
//! `monitor.lock` (01M421QQ1K7EFDV2PVPTSTE5FK).
//!
//! The monitor tells the lead with a message only when a fact changes
//! (01M421QPP5QFBB0YN25HY2MG1Z, [`Watch`]):
//!
//! - the 5-minute load goes over `monitor.load` times the physical
//!   cores, and again when it is good;
//! - the available memory goes under `workers.floor`, and again when it
//!   is good. A floor of 0 turns this off;
//! - a kill of `systemd-oomd` or of the kernel: one message for each.
//!
//! It only reads and tells. It changes no setting and stops no worker:
//! the lead and the person decide (01M421QPRF45DQDA8S4PT1Q12V).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant M as monitor in riff workers host, or riff mcp of the lead
//!     participant P as /proc and the journal
//!     participant S as riff-server
//!     participant L as lead
//!     loop each monitor.every seconds, while monitor.on
//!         M->>P: load, available memory, kills
//!         M->>M: Watch: what changed?
//!         opt a limit crossed, good again, or a kill
//!             M->>S: a message to the lead
//!             S->>L: wake
//!         end
//!         M->>M: save the last look in monitor.json
//!     end
//! ```
//!
//! The last look is in the file `monitor.json` of the local directory
//! ([`Saved`]). `riff workers` and `riff top` read it for the machine
//! where they run. A workers host tells the same numbers in its status
//! ([`Numbers`], 01M421QPX01BB15GJXHFYRETTX).
//!
//! ```
//! use riff::monitor::{Event, Limits, Look, Watch};
//!
//! let limits = Limits { load: 12.0, floor: 4 };
//! let look = |load5, avail_gb| Look { load1: load5, load5, avail_gb, kills: vec![] };
//! let mut watch = Watch::default();
//! assert!(watch.step(&look(3.0, 20), &limits).is_empty(), "good numbers: no message");
//! assert_eq!(
//!     watch.step(&look(13.0, 20), &limits),
//!     [Event::LoadOver { load5: 13.0, limit: 12.0 }],
//! );
//! assert!(watch.step(&look(14.0, 20), &limits).is_empty(), "still over: no message");
//! assert_eq!(
//!     watch.step(&look(9.0, 3), &limits),
//!     [Event::LoadGood { load5: 9.0, limit: 12.0 }, Event::MemoryUnder { avail_gb: 3, floor: 4 }],
//! );
//! assert_eq!(watch.step(&look(9.0, 8), &limits), [Event::MemoryGood { avail_gb: 8, floor: 4 }]);
//! ```

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use riff_core::name::SessionUri;
use riff_core::selector::Selector;
use riff_core::wire::Kind;
use serde::{Deserialize, Serialize};

use crate::api::Api;
use crate::limits::{Cores, Limits as JobLimits};
use crate::machine::Machine;
use crate::{local, settings, text};

/// The variable that names a directory to read in place of `/proc`,
/// for tests: its files `loadavg` and `meminfo`.
pub const PROC: &str = "RIFF_PROC";

/// The name of the file of the last look in the local directory.
pub const SAVED: &str = "monitor.json";

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

/// The kills in `journal`, the text of `journalctl -o short-unix`, that
/// came after `after` (seconds since 1970). Each other line is not a
/// kill.
///
/// ```
/// use riff::monitor::{Kill, kills};
///
/// let journal = "\
/// 1727980000.120000 pangolin systemd-oomd[812]: Killed /user.slice/app.slice/tmux-spawn-f089.scope due to memory pressure\n\
/// 1727980010.500000 pangolin kernel: Out of memory: Killed process 4242 (cargo) total-vm:1kB\n\
/// 1727980020.000000 pangolin kernel: oom-kill:constraint=CONSTRAINT_NONE\n\
/// 1727979000.000000 pangolin systemd-oomd[812]: Killed /old.scope due to memory pressure\n";
/// assert_eq!(kills(journal, 1727979500), [
///     Kill { at: 1727980000, by: "systemd-oomd".into(), what: "tmux-spawn-f089.scope".into() },
///     Kill { at: 1727980010, by: "kernel".into(), what: "cargo".into() },
/// ]);
/// assert!(kills(journal, 1727980010).is_empty());
/// ```
pub fn kills(journal: &str, after: u64) -> Vec<Kill> {
    journal
        .lines()
        .filter_map(|line| {
            let mut words = line.splitn(4, ' ');
            let at: u64 = words.next()?.split('.').next()?.parse().ok()?;
            let _host = words.next()?;
            let ident = words.next()?;
            let rest = words.next()?;
            if at <= after {
                return None;
            }
            if ident.starts_with("systemd-oomd") {
                let scope = rest.strip_prefix("Killed ")?.split(' ').next()?;
                let what = scope.rsplit('/').next().unwrap_or(scope);
                return Some(Kill {
                    at,
                    by: "systemd-oomd".into(),
                    what: what.to_owned(),
                });
            }
            if ident == "kernel:" {
                let (_, process) = rest.split_once("Killed process ")?;
                let name = process.split_once('(')?.1.split_once(')')?.0;
                return Some(Kill {
                    at,
                    by: "kernel".into(),
                    what: name.to_owned(),
                });
            }
            None
        })
        .collect()
}

/// The lines of `systemd-oomd` and of the kernel in the journal since
/// `since` (seconds since 1970), or `None` when riff cannot read them.
pub fn journal(since: u64) -> Option<String> {
    let out = std::process::Command::new("journalctl")
        .args(["-q", "--no-pager", "-o", "short-unix"])
        .arg(format!("--since=@{since}"))
        .args([
            "_SYSTEMD_UNIT=systemd-oomd.service",
            "+",
            "_TRANSPORT=kernel",
        ])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The 1-minute and 5-minute load average in the text of
/// `/proc/loadavg`.
///
/// ```
/// assert_eq!(riff::monitor::loads("13.20 9.80 6.00 2/3145 201397\n"), Some((13.2, 9.8)));
/// assert_eq!(riff::monitor::loads(""), None);
/// ```
pub fn loads(text: &str) -> Option<(f64, f64)> {
    let mut words = text.split_whitespace();
    Some((words.next()?.parse().ok()?, words.next()?.parse().ok()?))
}

/// The directory in place of `/proc`: [`PROC`], else `None`.
fn proc_dir() -> Option<PathBuf> {
    std::env::var_os(PROC).map(PathBuf::from)
}

/// The 1-minute and 5-minute load average of this machine. With
/// [`PROC`], from its `loadavg`. With the variable
/// [`crate::machine::MACHINE`] and no [`PROC`], both are the load of
/// that machine, so that a test does not see the load of the machine
/// that runs it.
pub fn load_here(machine: &Machine) -> (f64, f64) {
    let file = match proc_dir() {
        Some(dir) => dir.join("loadavg"),
        None if std::env::var_os(crate::machine::MACHINE).is_some() => {
            let load = f64::from(machine.load);
            return (load, load);
        }
        None => "/proc/loadavg".into(),
    };
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| loads(&text))
        .unwrap_or_default()
}

/// The available memory of this machine in GB. With [`PROC`], from its
/// `meminfo`.
fn avail_here(machine: &Machine) -> u32 {
    let Some(dir) = proc_dir() else {
        return machine.avail_gb;
    };
    let meminfo = std::fs::read_to_string(dir.join("meminfo")).unwrap_or_default();
    Machine::from_proc(
        machine.cores,
        &crate::machine::Cpufreq::default(),
        "",
        &meminfo,
        "",
    )
    .avail_gb
}

/// One look at the health of the machine.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub load1: f64,
    pub load5: f64,
    pub avail_gb: u32,
    /// The kills since the last look.
    pub kills: Vec<Kill>,
}

/// The limits of the monitor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// The most 5-minute load: `monitor.load` times the physical cores.
    pub load: f64,
    /// The least available memory in GB: `workers.floor`. 0 is no
    /// limit.
    pub floor: u32,
}

/// What the monitor tells the lead.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    LoadOver { load5: f64, limit: f64 },
    LoadGood { load5: f64, limit: f64 },
    MemoryUnder { avail_gb: u32, floor: u32 },
    MemoryGood { avail_gb: u32, floor: u32 },
    Killed(Kill),
}

/// The facts of the last look: over a limit or not. The monitor tells
/// the lead only when a fact changes (01M421QPP5QFBB0YN25HY2MG1Z).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watch {
    pub load_over: bool,
    pub memory_under: bool,
}

impl Watch {
    /// The events of `look`: each fact that changed since the last
    /// look, and each kill.
    pub fn step(&mut self, look: &Look, limits: &Limits) -> Vec<Event> {
        let mut events = Vec::new();
        let load_over = look.load5 > limits.load;
        if load_over != self.load_over {
            events.push(if load_over {
                Event::LoadOver {
                    load5: look.load5,
                    limit: limits.load,
                }
            } else {
                Event::LoadGood {
                    load5: look.load5,
                    limit: limits.load,
                }
            });
        }
        let memory_under = look.avail_gb < limits.floor;
        if memory_under != self.memory_under {
            events.push(if memory_under {
                Event::MemoryUnder {
                    avail_gb: look.avail_gb,
                    floor: limits.floor,
                }
            } else {
                Event::MemoryGood {
                    avail_gb: look.avail_gb,
                    floor: limits.floor,
                }
            });
        }
        events.extend(look.kills.iter().cloned().map(Event::Killed));
        *self = Watch {
            load_over,
            memory_under,
        };
        events
    }
}

/// The last look of the monitor of this machine, in the file
/// [`SAVED`] of the local directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    /// The time of the look, in seconds since 1970.
    pub at: u64,
    pub load1: f64,
    pub load5: f64,
    pub avail_gb: u32,
    pub limits: SavedLimits,
    /// The last kill that the monitor saw.
    pub kill: Option<Kill>,
}

/// [`Limits`] in [`Saved`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SavedLimits {
    pub load: f64,
    pub floor: u32,
}

impl Saved {
    /// The last look in the local directory `dir`, or `None`.
    pub fn read(dir: &Path) -> Option<Saved> {
        let text = std::fs::read_to_string(dir.join(SAVED)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Writes the look to `dir`. A reader never sees a part of it.
    fn write(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        let mut new = tempfile::NamedTempFile::new_in(dir)?;
        std::io::Write::write_all(&mut new, serde_json::to_string(self)?.as_bytes())?;
        new.persist(dir.join(SAVED)).map_err(|e| e.error)?;
        Ok(())
    }
}

/// The numbers of the monitor that a machine tells, after the numbers
/// of [`Machine`] (01M421QPX01BB15GJXHFYRETTX): the monitor on or off,
/// the 5-minute load and its limit, the physical cores, the jobs of
/// each worker, and the last kill. A workers host puts them in its
/// status ([`crate::host::HostStatus`]).
///
/// ```
/// use riff::monitor::{Kill, Numbers};
///
/// let n = Numbers {
///     on: true,
///     load5: 9.8,
///     limit: 12.0,
///     physical: 8,
///     jobs: 2,
///     kill: Some(Kill { at: 1727980000, by: "systemd-oomd".into(), what: String::new() }),
/// };
/// assert_eq!(
///     n.to_string(),
///     "monitor on, load5 9.80 of 12.00, 8 cores, jobs 2, kill 1727980000 systemd-oomd",
/// );
/// assert_eq!(Numbers::parse(&n.to_string()), Some(n.clone()));
/// let off = Numbers { on: false, kill: None, ..n };
/// assert_eq!(off.to_string(), "monitor off, load5 9.80 of 12.00, 8 cores, jobs 2");
/// assert_eq!(Numbers::parse(&off.to_string()), Some(off));
/// assert_eq!(Numbers::parse("disk 16GB free of 455GB"), None);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Numbers {
    /// `monitor.on` of the machine.
    pub on: bool,
    pub load5: f64,
    /// The limit of the 5-minute load.
    pub limit: f64,
    /// The physical cores.
    pub physical: u16,
    /// The jobs of each worker ([`JobLimits::jobs`]).
    pub jobs: u16,
    /// The last kill that the monitor saw. Its `what` is empty.
    pub kill: Option<Kill>,
}

impl Numbers {
    /// The numbers of this machine: the settings file `path`, `machine`,
    /// the `workers` that run, and the last look in the local
    /// directory.
    pub fn here(path: &Path, machine: &Machine, workers: u16) -> Result<Numbers> {
        let monitor = settings::monitor(path)?;
        let physical = Cores::here(machine).physical;
        let saved = local::dir().and_then(|dir| Saved::read(&dir));
        Ok(Numbers {
            on: monitor.on,
            load5: load_here(machine).1,
            limit: monitor.load * f64::from(physical),
            physical,
            jobs: JobLimits::of(path, physical, workers)?.jobs,
            kill: saved.and_then(|s| s.kill).map(|k| Kill {
                what: String::new(),
                ..k
            }),
        })
    }

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
        let (physical, rest) = rest.split_once(" cores, jobs ")?;
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
            jobs: jobs.parse().ok()?,
            kill,
        })
    }
}

impl fmt::Display for Numbers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "monitor {}, load5 {:.2} of {:.2}, {} cores, jobs {}",
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

/// The time now, in seconds since 1970.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The monitor of one process: the facts of the last look, the lock,
/// and the time of the last kill that it told.
pub struct Monitor {
    watch: Watch,
    held: Option<local::Held>,
    since: u64,
    /// True when another monitor held the lock at the last look.
    other: bool,
    kill: Option<Kill>,
}

impl Default for Monitor {
    fn default() -> Self {
        Monitor::new()
    }
}

impl Monitor {
    /// A monitor that tells no kill from before its start.
    pub fn new() -> Monitor {
        Monitor {
            watch: Watch::default(),
            held: None,
            since: now_secs(),
            other: false,
            kill: None,
        }
    }

    /// The time to the next look: `monitor.every`, or the default when
    /// riff cannot read the settings.
    pub fn every() -> Duration {
        let every = settings::path()
            .and_then(|path| settings::monitor(&path))
            .map_or(settings::MONITOR_EVERY, |m| m.every);
        Duration::from_secs(u64::from(every))
    }

    /// One look as `me`, when `monitor.on` is true and no other monitor
    /// runs on the machine. It posts a message to the lead of the user
    /// of `me` for each event, and saves the look. Returns the lines to
    /// print: the texts of the events, or that another monitor runs.
    pub async fn look(&mut self, api: &Api, me: &SessionUri) -> Result<Vec<String>> {
        let path = settings::path()?;
        let monitor = settings::monitor(&path)?;
        let Some(dir) = local::dir() else {
            return Ok(Vec::new());
        };
        if !monitor.on {
            // Off: the next start tells each fact again.
            self.held = None;
            self.watch = Watch::default();
            return Ok(Vec::new());
        }
        if self.held.is_none() {
            self.held = local::monitor_lock(&dir)?;
            if self.held.is_none() {
                // One line for each time that another monitor takes over.
                let told = std::mem::replace(&mut self.other, true);
                return Ok(if told {
                    Vec::new()
                } else {
                    vec![text::MONITOR_RUNS.to_owned()]
                });
            }
            self.other = false;
            self.since = self.since.max(now_secs());
        }
        let machine = Machine::here();
        let (load1, load5) = load_here(&machine);
        let since = self.since;
        let found = tokio::task::spawn_blocking(move || journal(since))
            .await
            .ok()
            .flatten()
            .map(|text| kills(&text, since))
            .unwrap_or_default();
        if let Some(last) = found.iter().map(|k| k.at).max() {
            self.since = last;
        }
        if let Some(last) = found.last() {
            self.kill = Some(last.clone());
        }
        let look = Look {
            load1,
            load5,
            avail_gb: avail_here(&machine),
            kills: found,
        };
        let physical = Cores::here(&machine).physical;
        let limits = Limits {
            load: monitor.load * f64::from(physical),
            floor: settings::workers_floor(&path)?,
        };
        let host = me.place().host().to_owned();
        let texts: Vec<String> = self
            .watch
            .step(&look, &limits)
            .iter()
            .map(|event| text::monitor_event(&host, event, monitor.load, physical))
            .collect();
        let repo = me.place().repo_text();
        let to = [Selector::lead(me.who().user(), &repo)];
        for message in &texts {
            if let Err(e) = api.post(me, None, &to, message, Kind::Message).await {
                eprintln!("riff: cannot tell the lead: {e:#}");
            }
        }
        let saved = Saved {
            at: now_secs(),
            load1,
            load5,
            avail_gb: look.avail_gb,
            limits: SavedLimits {
                load: limits.load,
                floor: limits.floor,
            },
            kill: self
                .kill
                .clone()
                .or_else(|| Saved::read(&dir).and_then(|s| s.kill)),
        };
        if let Err(e) = saved.write(&dir) {
            eprintln!("riff: cannot save the look of the monitor: {e:#}");
        }
        Ok(texts)
    }
}

/// Runs the monitor as `me` until the task ends: one look each
/// [`Monitor::every`]. A workers host runs it.
pub async fn run(api: Api, me: SessionUri) {
    let mut monitor = Monitor::new();
    loop {
        tokio::time::sleep(Monitor::every()).await;
        match monitor.look(&api, &me).await {
            Ok(texts) => texts.iter().for_each(|t| println!("{t}")),
            Err(e) => eprintln!("riff: the monitor cannot look: {e:#}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(load5: f64, avail_gb: u32) -> Look {
        Look {
            load1: load5,
            load5,
            avail_gb,
            kills: vec![],
        }
    }

    #[test]
    fn good_numbers_give_no_message() {
        let limits = Limits {
            load: 12.0,
            floor: 4,
        };
        let mut watch = Watch::default();
        for _ in 0..10 {
            assert!(watch.step(&look(5.0, 30), &limits).is_empty());
        }
    }

    #[test]
    fn a_limit_gives_one_message_and_good_again_one_more() {
        let limits = Limits {
            load: 12.0,
            floor: 4,
        };
        let mut watch = Watch::default();
        let mut all = Vec::new();
        for (load5, avail) in [(5.0, 30), (12.5, 30), (13.0, 30), (12.1, 30), (11.0, 30)] {
            all.extend(watch.step(&look(load5, avail), &limits));
        }
        assert_eq!(
            all,
            [
                Event::LoadOver {
                    load5: 12.5,
                    limit: 12.0
                },
                Event::LoadGood {
                    load5: 11.0,
                    limit: 12.0
                },
            ]
        );
    }

    #[test]
    fn a_floor_of_0_gives_no_message_for_the_memory() {
        let limits = Limits {
            load: 12.0,
            floor: 0,
        };
        let mut watch = Watch::default();
        assert!(watch.step(&look(1.0, 0), &limits).is_empty());
    }

    #[test]
    fn each_kill_gives_one_message() {
        let limits = Limits {
            load: 12.0,
            floor: 4,
        };
        let kill = |at| Kill {
            at,
            by: "kernel".into(),
            what: "cargo".into(),
        };
        let mut watch = Watch::default();
        let mut with_kills = look(1.0, 30);
        with_kills.kills = vec![kill(1), kill(2)];
        assert_eq!(
            watch.step(&with_kills, &limits),
            [Event::Killed(kill(1)), Event::Killed(kill(2))]
        );
        assert!(watch.step(&look(1.0, 30), &limits).is_empty());
    }

    /// 01M421QPX01BB15GJXHFYRETTX: a status with the disk, the numbers
    /// of the monitor and a kill reads back.
    #[test]
    fn a_status_with_a_kill_reads_back() {
        let status = crate::host::HostStatus {
            limit: 2,
            floor: 4,
            machine: Machine::parse("cpu 8x3000MHz, mem 64GB, 60GB available, load 0.50"),
            disk: Some(crate::disk::Disk {
                free_gb: 50,
                total_gb: 455,
            }),
            monitor: Some(Numbers {
                on: true,
                load5: 5.0,
                limit: 12.0,
                physical: 8,
                jobs: 3,
                kill: Some(Kill {
                    at: 1_727_980_001,
                    by: "systemd-oomd".into(),
                    what: String::new(),
                }),
            }),
            workers: vec![],
        };
        assert_eq!(
            crate::host::HostStatus::parse(&status.line()),
            Some(status.clone()),
            "{}",
            status.line()
        );
    }

    /// The status of a host with 10 workers and each number fits in a
    /// status (R183).
    #[test]
    fn a_status_of_10_workers_fits() {
        let status = crate::host::HostStatus {
            limit: 10,
            floor: 16,
            machine: Machine::parse(
                "cpu 128x5883MHz (now 5800MHz), mem 1024GB, 1000GB available, load 133.20",
            ),
            disk: Some(crate::disk::Disk {
                free_gb: 1000,
                total_gb: 4000,
            }),
            monitor: Some(Numbers {
                on: true,
                load5: 110.4,
                limit: 96.0,
                physical: 64,
                jobs: 12,
                kill: Some(Kill {
                    at: 1_727_980_001,
                    by: "systemd-oomd".into(),
                    what: String::new(),
                }),
            }),
            workers: (10..20)
                .map(|n| (format!("%{n}"), "1a2b3c4d".to_owned()))
                .collect(),
        };
        let line = status.line();
        assert!(
            line.chars().count() <= riff_core::wire::STATUS_CHARS,
            "{} characters: {line}",
            line.chars().count()
        );
    }

    #[test]
    fn the_saved_look_survives_a_write_and_a_read() {
        let dir = tempfile::tempdir().unwrap();
        let saved = Saved {
            at: 7,
            load1: 1.0,
            load5: 2.0,
            avail_gb: 3,
            limits: SavedLimits {
                load: 12.0,
                floor: 4,
            },
            kill: None,
        };
        saved.write(dir.path()).unwrap();
        assert_eq!(Saved::read(dir.path()), Some(saved));
    }
}
