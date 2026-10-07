//! The limits of the workers of a machine.
//!
//! # Design
//!
//! N workers that each build with all cores start N × cores compile
//! jobs. The machine then runs out of memory, and the OS kills a worker
//! pane with its work. So riff limits the workers, and no worker and no
//! lead has to remember a limit:
//!
//! | Limit | Who sets it | Default | Setting |
//! |---|---|---|---|
//! | Compile jobs and test threads of all workers | `riff workers run` | one pool: physical cores - 1 - workers, 1 or more ([`crate::jobserver`]) | `workers.jobs` |
//! | Test threads of one worker, and its jobs with no pool | `riff workers run` | (physical cores - 1) / workers, 1 or more ([`jobs`]) | `workers.jobs` |
//! | Priority | `riff workers run` | nice 10 | `workers.nice` |
//! | Memory and CPU share of all workers | `riff workers start` | 3/4 of the memory ([`memory`]), CPU weight [`CPU_WEIGHT`] | `workers.memory` |
//! | Available memory for a new worker | `riff workers start`, the rollout | 4 GB ([`crate::machine::Machine::low`]) | `workers.floor` |
//!
//! ```mermaid
//! flowchart TD
//!     S["riff workers start"] --> F{"available memory<br/>less than the floor?"}
//!     F -- yes --> N["start nothing, say why"]
//!     F -- no --> P["systemctl --user set-property --runtime<br/>riff-workers.slice MemoryHigh MemoryMax CPUWeight"]
//!     P -- ok --> T["tmux pane: riff workers run<br/>RIFF_WORKER_SLICE=riff-workers.slice"]
//!     P -- "no systemd" --> U["tmux pane: riff workers run<br/>say it one time"]
//!     T --> Q{"systemd-run --user --scope<br/>works in the pane?"}
//!     Q -- yes --> C["systemd-run --user --scope --slice=riff-workers.slice<br/>nice -n (10 - nice of the wrapper) claude<br/>jobs_env: the pool or the fixed share"]
//!     Q -- "no, say it one time" --> D
//!     U --> D["nice -n (10 - nice of the wrapper) claude<br/>jobs_env: the pool or the fixed share"]
//! ```
//!
//! - **Jobs** (01M3WFYZRK5CT22GJW6ZHYT9CC, 01M3ZGZMJ9RF1C4AHG78GQ2NM4).
//!   The wrapper holds the pool of the machine ([`crate::jobserver`]),
//!   and gives `claude` the variables of [`jobs_env`]: `MAKEFLAGS` that
//!   names the pool, the test runner, and `RUST_TEST_THREADS`. With no
//!   pool, it gives the fixed share in [`JOBS_VARS`], and unsets the
//!   variables of a pool (01M41CR2HJRFW6R7YMJTPVEMJ1). Each build and
//!   each test run of the worker reads them. The wrapper stays for each
//!   next item of the worker, so the variables stay too.
//! - **Nice** (01M3WFYZTX05CGDP2NQF9B356K). The wrapper starts `claude`
//!   through `nice`. Each build of the worker gives way to the other
//!   work of the machine. The value is absolute
//!   (01M407J8R79WVYVABVCSHFAMJ9): `nice -n` adds only the difference to
//!   the nice value of the wrapper ([`nice_by`]).
//! - **Slice** (01M3WFYZX6GVFYW6NTTTKF144R). All workers of a machine
//!   run in the slice [`SLICE`] of the systemd user manager, each in a
//!   scope of its own. The slice has `MemoryHigh`, `MemoryMax` and
//!   `CPUWeight` ([`properties`]). The wrapper stays outside the slice.
//!   So when the OS kills a worker for its memory, the wrapper lives and
//!   tells the lead (01M3WFZ03Z9Y60HPHJJ9ZE6AQZ), and the desktop of the
//!   person goes on.
//! - **No systemd** (01M3WFYZZENNHVH8Z2BAFSR6TS). When `systemctl`
//!   cannot set the slice, the workers run with no scope. riff says so
//!   one time ([`scope`]). When `systemd-run --user --scope` fails in
//!   the pane of a worker, for example with no user bus in the
//!   environment of the tmux server, the wrapper starts `claude` with no
//!   scope, and says so one time in the pane ([`worker_slice`],
//!   01M407J8X25H9AT8M789EG5RQZ).
//! - **Floor** (01M3WFZ01PTAYYKG3T5CFA2W4D). riff starts no worker while
//!   the available memory is less than the floor.
//!
//! - **Cores** (01M3WFYZRK5CT22GJW6ZHYT9CC). riff counts the physical
//!   cores of the machine ([`Cores`]). When it cannot read them, it
//!   counts half of the logical CPUs, and says so one time
//!   ([`say_cores`]).
//! - **Workers.** "Workers" in the table is the worker limit, or the
//!   workers that run when they are more ([`Limits::of`],
//!   [`crate::jobserver::workers`]). So 4 workers with a limit of 2 do
//!   not get twice the cores.
//!
//! riff sets the limits when a worker starts. It does not watch the
//! memory, and it does not change a limit while the workers run.
//!
//! ```
//! use riff::limits::{jobs, memory};
//!
//! // pangolin: 8 physical cores, 30 GB, a limit of 3 workers.
//! assert_eq!(riff::jobserver::tokens(8, 3), 4);
//! assert_eq!(jobs(8, 3, 0), 2);
//! assert_eq!(memory(30, 0), 23);
//! ```

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use anyhow::Result;

use crate::machine::Machine;
use crate::settings;

/// The variables that give a build and a test run their number of
/// jobs with no pool: the compile jobs of cargo and the test threads of
/// a Rust test.
pub const JOBS_VARS: [&str; 2] = ["CARGO_BUILD_JOBS", "RUST_TEST_THREADS"];

/// The variable of cargo that names the test runner of the target of
/// this build: `CARGO_TARGET_<TRIPLE>_RUNNER`.
///
/// ```
/// let var = riff::limits::runner_var();
/// assert!(var.starts_with("CARGO_TARGET_") && var.ends_with("_RUNNER"), "{var}");
/// assert!(!var.contains('-') && var == var.to_uppercase(), "{var}");
/// ```
pub fn runner_var() -> String {
    let triple = env!("RIFF_TARGET").to_uppercase().replace(['-', '.'], "_");
    format!("CARGO_TARGET_{triple}_RUNNER")
}

/// The variables of a worker with `limits`: the value to set, or `None`
/// to unset. With the `MAKEFLAGS` of a pool, each build takes its jobs
/// from the pool (01M3ZGZMJ9RF1C4AHG78GQ2NM4), and the test runner of
/// `riff` takes the test threads from it (01M3ZGZMNH1YM56GYNYBMH7AWM).
/// With no pool, each build and test run gets the fixed share
/// (01M3WFYZRK5CT22GJW6ZHYT9CC), and no `MAKEFLAGS`,
/// `CARGO_MAKEFLAGS` or test runner of another pool
/// (01M41CR2HJRFW6R7YMJTPVEMJ1).
///
/// ```
/// use riff::limits::{jobs_env, runner_var, Limits};
///
/// let limits = Limits { jobs: 2, tokens: 4, counted: 3, nice: 10 };
/// let flags = "-j --jobserver-auth=fifo:/run/riff/jobs/fifo";
/// let env = jobs_env(&limits, Some(flags), "/bin/riff".as_ref());
/// let get = |var: &str| env.iter().find(|(v, _)| v == var).map(|(_, value)| value.clone());
/// assert_eq!(get("MAKEFLAGS"), Some(Some(flags.to_owned())));
/// assert_eq!(get("CARGO_BUILD_JOBS"), Some(None), "no cap below the pool");
/// assert_eq!(get("RUST_TEST_THREADS"), Some(Some("2".to_owned())));
/// assert_eq!(get(&runner_var()), Some(Some("/bin/riff workers test-run".to_owned())));
///
/// let env = jobs_env(&limits, None, "/bin/riff".as_ref());
/// assert_eq!(
///     env,
///     [("MAKEFLAGS".to_owned(), None),
///      ("CARGO_MAKEFLAGS".to_owned(), None),
///      ("CARGO_BUILD_JOBS".to_owned(), Some("2".to_owned())),
///      ("RUST_TEST_THREADS".to_owned(), Some("2".to_owned())),
///      (runner_var(), None)],
/// );
/// ```
pub fn jobs_env(
    limits: &Limits,
    makeflags: Option<&str>,
    riff: &Path,
) -> Vec<(String, Option<String>)> {
    let jobs = Some(limits.jobs.to_string());
    let Some(makeflags) = makeflags else {
        return vec![
            ("MAKEFLAGS".into(), None),
            ("CARGO_MAKEFLAGS".into(), None),
            ("CARGO_BUILD_JOBS".into(), jobs.clone()),
            ("RUST_TEST_THREADS".into(), jobs),
            (runner_var(), None),
        ];
    };
    vec![
        ("MAKEFLAGS".into(), Some(makeflags.into())),
        ("CARGO_MAKEFLAGS".into(), None),
        ("CARGO_BUILD_JOBS".into(), None),
        ("RUST_TEST_THREADS".into(), jobs),
        (
            runner_var(),
            Some(format!("{} workers test-run", riff.display())),
        ),
    ]
}

/// The fewest jobs of one worker.
pub const MIN_JOBS: u16 = 1;

/// The slice of the workers in the systemd user manager.
pub const SLICE: &str = "riff-workers.slice";

/// The variable that names the slice to the wrapper of a worker. With
/// no such variable, the wrapper uses no scope.
pub const SLICE_VAR: &str = "RIFF_WORKER_SLICE";

/// The CPU weight of the slice. The weight of other work is 100.
pub const CPU_WEIGHT: u16 = 50;

/// The file in the local dir that says: riff said that this machine
/// has no systemd.
pub const SAID: &str = "no-systemd";

/// The file in the local dir that says: riff said that it cannot read
/// the physical cores of this machine.
pub const SAID_CORES: &str = "no-physical-cores";

/// The file in the local dir that says: riff said that `systemd-run`
/// cannot make a scope in the pane of a worker.
pub const SAID_SCOPE: &str = "no-scope";

/// Says `line` one time: the file `said` holds that riff said it. With
/// no `line`, the next line is a first time again.
///
/// ```
/// use riff::limits::once;
///
/// let dir = tempfile::tempdir()?;
/// let said = dir.path().join("sub/said");
/// assert_eq!(once(Some(&said), Some("x".into())), Some("x".into()));
/// assert_eq!(once(Some(&said), Some("x".into())), None, "one time");
/// assert_eq!(once(Some(&said), None), None);
/// assert_eq!(once(Some(&said), Some("x".into())), Some("x".into()), "again after a clear");
/// assert_eq!(once(None, Some("x".into())), Some("x".into()), "no local dir: each time");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn once(said: Option<&Path>, line: Option<String>) -> Option<String> {
    let Some(line) = line else {
        if let Some(said) = said {
            let _ = std::fs::remove_file(said);
        }
        return None;
    };
    let first = said.is_none_or(|said| !said.exists());
    if let Some(said) = said {
        if let Some(dir) = said.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(said, "");
    }
    first.then_some(line)
}

/// The fixed share of one worker: `setting`, or with 0 the `physical`
/// cores less 1, divided by the worker `limit`, and [`MIN_JOBS`] or
/// more (01M3WFYZRK5CT22GJW6ZHYT9CC).
///
/// ```
/// use riff::limits::jobs;
///
/// assert_eq!(jobs(8, 3, 0), 2);
/// assert_eq!(jobs(32, 3, 0), 10);
/// assert_eq!(jobs(4, 4, 0), 1, "1 or more");
/// assert_eq!(jobs(8, 0, 0), 7, "no limit counts as one worker");
/// assert_eq!(jobs(8, 3, 6), 6, "the setting wins");
/// ```
pub fn jobs(physical: u16, limit: u16, setting: u16) -> u16 {
    if setting > 0 {
        return setting;
    }
    (physical.saturating_sub(1) / limit.max(1)).max(MIN_JOBS)
}

/// The variable that names the file to read in place of
/// `/proc/cpuinfo`, for tests.
pub const CPUINFO: &str = "RIFF_CPUINFO";

/// The physical cores of a machine, and how riff found them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cores {
    /// The physical cores that riff counts.
    pub physical: u16,
    /// The logical CPUs of the machine.
    pub logical: u16,
    /// False when riff cannot read the physical cores: then `physical`
    /// is half of `logical`.
    pub read: bool,
}

impl Cores {
    /// The cores from the text of `/proc/cpuinfo` and the `logical`
    /// CPUs. When the text has no physical cores, riff uses half of the
    /// logical CPUs, and 1 or more (01M3WFYZRK5CT22GJW6ZHYT9CC).
    ///
    /// ```
    /// use riff::limits::Cores;
    ///
    /// let two_threads = "physical id\t: 0\ncore id\t\t: 0\n\nphysical id\t: 0\ncore id\t\t: 0\n";
    /// assert_eq!(Cores::from(two_threads, 2), Cores { physical: 1, logical: 2, read: true });
    /// assert_eq!(Cores::from("processor\t: 0\n", 16), Cores { physical: 8, logical: 16, read: false });
    /// assert_eq!(Cores::from("", 1).physical, 1, "1 or more");
    /// ```
    pub fn from(cpuinfo: &str, logical: u16) -> Cores {
        match physical_from(cpuinfo) {
            Some(physical) => Cores {
                physical,
                logical,
                read: true,
            },
            None => Cores {
                physical: (logical / 2).max(1),
                logical,
                read: false,
            },
        }
    }

    /// The cores of this machine with the logical CPUs of `machine`. On
    /// Linux, riff counts the pairs of `physical id` and `core id` in
    /// `/proc/cpuinfo`, or in the file that [`CPUINFO`] names. When
    /// [`crate::machine::MACHINE`] gives the numbers and [`CPUINFO`] is
    /// not set, the physical cores are `machine.cores`.
    pub fn here(machine: &Machine) -> Cores {
        let file = std::env::var_os(CPUINFO);
        if file.is_none() && std::env::var_os(crate::machine::MACHINE).is_some() {
            return Cores {
                physical: machine.cores,
                logical: machine.cores,
                read: true,
            };
        }
        let file = file.unwrap_or_else(|| "/proc/cpuinfo".into());
        Cores::from(
            &std::fs::read_to_string(file).unwrap_or_default(),
            machine.cores,
        )
    }

    /// What riff says when it cannot read the physical cores, or `None`.
    ///
    /// ```
    /// use riff::limits::Cores;
    ///
    /// assert_eq!(Cores { physical: 8, logical: 16, read: true }.said(), None);
    /// assert!(Cores { physical: 8, logical: 16, read: false }
    ///     .said()
    ///     .is_some_and(|line| line.contains("half of the 16 logical CPUs: 8")));
    /// ```
    pub fn said(&self) -> Option<String> {
        (!self.read).then(|| crate::text::no_physical_cores(self.logical, self.physical))
    }
}

/// Says one time that riff cannot read the physical `cores`: the file
/// [`SAID_CORES`] in `local` holds that (01M3WFYZRK5CT22GJW6ZHYT9CC).
///
/// ```
/// use riff::limits::{say_cores, Cores};
///
/// let local = tempfile::tempdir()?;
/// let guess = Cores { physical: 8, logical: 16, read: false };
/// assert!(say_cores(&guess, Some(local.path())).is_some());
/// assert_eq!(say_cores(&guess, Some(local.path())), None, "one time");
/// let read = Cores { read: true, ..guess };
/// assert_eq!(say_cores(&read, Some(local.path())), None);
/// assert!(say_cores(&guess, Some(local.path())).is_some(), "again after a read");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn say_cores(cores: &Cores, local: Option<&Path>) -> Option<String> {
    let said = local.map(|dir| dir.join(SAID_CORES));
    once(said.as_deref(), cores.said())
}

/// The physical cores in the text of `/proc/cpuinfo`, or `None`.
///
/// ```
/// use riff::limits::physical_from;
///
/// let two_threads = "physical id\t: 0\ncore id\t\t: 0\n\nphysical id\t: 0\ncore id\t\t: 0\n\n\
///     physical id\t: 0\ncore id\t\t: 1\n";
/// assert_eq!(physical_from(two_threads), Some(2));
/// assert_eq!(physical_from("processor\t: 0\n"), None);
/// ```
pub fn physical_from(cpuinfo: &str) -> Option<u16> {
    let mut cores = std::collections::BTreeSet::new();
    let mut package = None;
    for line in cpuinfo.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "physical id" => package = Some(value.trim().to_owned()),
            "core id" => {
                cores.insert((package.clone(), value.trim().to_owned()));
            }
            _ => {}
        }
    }
    u16::try_from(cores.len()).ok().filter(|&n| n > 0)
}

/// The most memory of all workers in GB: `setting`, or with 0 three
/// quarters of the memory of the machine, and 1 or more
/// (01M3WFYZX6GVFYW6NTTTKF144R).
///
/// ```
/// use riff::limits::memory;
///
/// assert_eq!(memory(30, 0), 23);
/// assert_eq!(memory(124, 0), 93);
/// assert_eq!(memory(1, 0), 1);
/// assert_eq!(memory(30, 12), 12, "the setting wins");
/// ```
pub fn memory(mem_gb: u32, setting: u32) -> u32 {
    if setting > 0 {
        return setting;
    }
    (mem_gb - mem_gb / 4).max(1)
}

/// The properties of the slice for `max_gb` of memory: `MemoryMax` is
/// `max_gb`. `MemoryHigh` is nine tenths of it: there the OS slows the
/// workers and takes memory back, before a kill.
///
/// ```
/// assert_eq!(
///     riff::limits::properties(20),
///     ["MemoryHigh=18432M", "MemoryMax=20480M", "CPUWeight=50"],
/// );
/// ```
pub fn properties(max_gb: u32) -> [String; 3] {
    let max_mb = u64::from(max_gb) * 1024;
    [
        format!("MemoryHigh={}M", max_mb * 9 / 10),
        format!("MemoryMax={max_mb}M"),
        format!("CPUWeight={CPU_WEIGHT}"),
    ]
}

/// The limits of one worker on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The fixed share: the test threads, and the compile jobs when the
    /// worker has no pool.
    pub jobs: u16,
    /// The tokens of the pool of the machine
    /// ([`crate::jobserver::tokens`]). 0 is no pool: the setting
    /// `workers.jobs` turns it off.
    pub tokens: u16,
    /// The workers that the numbers count: the limit, or the workers
    /// that run when they are more.
    pub counted: u16,
    /// The nice value. 0 is no nice.
    pub nice: u8,
}

impl Limits {
    /// The limits from the settings file `path`, the `physical` cores of
    /// the machine ([`Cores`]), and the `workers` that run. When more
    /// workers run than the limit, riff counts the workers that run
    /// (01M3WFYZRK5CT22GJW6ZHYT9CC).
    ///
    /// ```
    /// use riff::limits::Limits;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let path = dir.path().join("config.toml");
    /// riff::settings::set_workers_limit(&path, 3)?;
    /// assert_eq!(Limits::of(&path, 8, 1)?, Limits { jobs: 2, tokens: 4, counted: 3, nice: 10 });
    /// assert_eq!(
    ///     Limits::of(&path, 8, 6)?,
    ///     Limits { jobs: 1, tokens: 1, counted: 6, nice: 10 },
    ///     "6 workers run, more than the limit",
    /// );
    /// riff::settings::set_workers_jobs(&path, 3)?;
    /// riff::settings::set_workers_nice(&path, 0)?;
    /// assert_eq!(Limits::of(&path, 8, 1)?, Limits { jobs: 3, tokens: 0, counted: 3, nice: 0 });
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn of(path: &Path, physical: u16, workers: u16) -> Result<Limits> {
        let counted = settings::workers_limit(path)?.max(workers).max(1);
        let setting = settings::workers_jobs(path)?;
        Ok(Limits {
            jobs: jobs(physical, counted, setting),
            tokens: if setting > 0 {
                0
            } else {
                crate::jobserver::tokens(physical, counted)
            },
            counted,
            nice: settings::workers_nice(path)?,
        })
    }
}

/// The command that runs `claude` with `args` for a worker: through
/// `nice -n` when the increment `nice` is more than 0 ([`nice_by`]),
/// and in a scope of `slice` when the machine has one. The scope has the
/// name `unit` ([`crate::workload::scope_unit`],
/// 01M49SV9W4S1HJ4BYANA388VD2).
///
/// ```
/// use riff::limits::command;
///
/// let args = ["Join the riff.".to_owned()];
/// assert_eq!(command("claude".as_ref(), &args, 0, None, None), ["claude", "Join the riff."]);
/// assert_eq!(
///     command("claude".as_ref(), &args, 10, None, Some("riff-worker-w1.42.scope")),
///     ["nice", "-n", "10", "claude", "Join the riff."],
/// );
/// assert_eq!(
///     command("claude".as_ref(), &args, 10, Some("riff-workers.slice"), Some("riff-worker-w1.42.scope")),
///     [
///         "systemd-run", "--user", "--scope", "--quiet", "--slice=riff-workers.slice",
///         "--unit=riff-worker-w1.42.scope", "--", "nice", "-n", "10", "claude", "Join the riff.",
///     ],
/// );
/// ```
pub fn command(
    claude: &Path,
    args: &[String],
    nice: u8,
    slice: Option<&str>,
    unit: Option<&str>,
) -> Vec<OsString> {
    let mut command: Vec<OsString> = Vec::new();
    if let Some(slice) = slice {
        command.extend(["systemd-run", "--user", "--scope", "--quiet"].map(OsString::from));
        command.push(format!("--slice={slice}").into());
        if let Some(unit) = unit {
            command.push(format!("--unit={unit}").into());
        }
        command.push("--".into());
    }
    if nice > 0 {
        command.extend(["nice".into(), "-n".into(), nice.to_string().into()]);
    }
    command.push(claude.into());
    command.extend(args.iter().map(OsString::from));
    command
}

/// Sets the properties of [`SLICE`] for `max_gb` of memory, until the
/// next start of the machine. The error is the reason in words: no
/// `systemctl`, or no systemd user manager.
pub fn set_slice(max_gb: u32) -> std::result::Result<(), String> {
    let out = Command::new("systemctl")
        .args(["--user", "set-property", "--runtime", SLICE])
        .args(properties(max_gb))
        .output()
        .map_err(|e| format!("cannot run systemctl: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    Err(stderr
        .lines()
        .next()
        .unwrap_or("systemctl failed")
        .trim()
        .to_owned())
}

/// The scope of the next workers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    /// The slice of the workers. `None` on a machine with no systemd.
    pub slice: Option<&'static str>,
    /// What riff says, the first time that the machine has no systemd.
    pub said: Option<String>,
}

/// Makes the slice of the workers ready, with the memory of the
/// settings file `path` on `machine` (01M3WFYZX6GVFYW6NTTTKF144R). On a
/// machine with no systemd, it gives no slice, and the line to say. It
/// gives the line one time: the file [`SAID`] in `local` holds that
/// (01M3WFYZZENNHVH8Z2BAFSR6TS).
pub fn scope(path: &Path, machine: &Machine, local: Option<&Path>) -> Result<Scope> {
    let max_gb = memory(machine.mem_gb, settings::workers_memory(path)?);
    let said = local.map(|dir| dir.join(SAID));
    match set_slice(max_gb) {
        Ok(()) => {
            once(said.as_deref(), None);
            Ok(Scope {
                slice: Some(SLICE),
                said: None,
            })
        }
        Err(why) => Ok(Scope {
            slice: None,
            said: once(said.as_deref(), Some(crate::text::no_systemd(&why))),
        }),
    }
}

/// Checks that `systemd-run --user --scope` can make a scope of `slice`
/// here: it runs `true` in one. The error is the reason in words, for
/// example no user bus in the environment of the tmux server
/// (01M407J8X25H9AT8M789EG5RQZ).
pub fn try_scope(slice: &str) -> std::result::Result<(), String> {
    let out = Command::new("systemd-run")
        .args(["--user", "--scope", "--quiet"])
        .arg(format!("--slice={slice}"))
        .args(["--", "true"])
        .output()
        .map_err(|e| format!("cannot run systemd-run: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    Err(stderr
        .lines()
        .next()
        .unwrap_or("systemd-run failed")
        .trim()
        .to_owned())
}

/// The slice of a worker that the wrapper got in `slice`, when a scope
/// of it works here, and the line to say. With no scope, the line comes
/// one time: the file [`SAID_SCOPE`] in `local` holds that
/// (01M407J8X25H9AT8M789EG5RQZ).
///
/// ```
/// use riff::limits::worker_slice;
///
/// let local = tempfile::tempdir()?;
/// let works = |_: &str| Ok(());
/// let fails = |_: &str| Err("Failed to connect to bus".to_owned());
/// assert_eq!(worker_slice(None, Some(local.path()), fails), (None, None));
/// assert_eq!(worker_slice(Some("s.slice"), Some(local.path()), works), (Some("s.slice".into()), None));
/// let (slice, said) = worker_slice(Some("s.slice"), Some(local.path()), fails);
/// assert_eq!(slice, None);
/// assert!(said.is_some_and(|line| line.contains("Failed to connect to bus")));
/// assert_eq!(worker_slice(Some("s.slice"), Some(local.path()), fails), (None, None), "one time");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn worker_slice(
    slice: Option<&str>,
    local: Option<&Path>,
    try_scope: impl Fn(&str) -> std::result::Result<(), String>,
) -> (Option<String>, Option<String>) {
    let Some(slice) = slice else {
        return (None, None);
    };
    let said = local.map(|dir| dir.join(SAID_SCOPE));
    match try_scope(slice) {
        Ok(()) => {
            once(said.as_deref(), None);
            (Some(slice.to_owned()), None)
        }
        Err(why) => (
            None,
            once(said.as_deref(), Some(crate::text::no_scope(&why))),
        ),
    }
}

/// The nice value in the text of `/proc/PID/stat`, or `None`.
///
/// ```
/// let stat = "4242 (just ci) S 4200 4242 4200 0 -1 4194560 0 0 0 0 0 0 0 0 30 10 1 0 98765 0 0";
/// assert_eq!(riff::limits::nice_from(stat), Some(10));
/// assert_eq!(riff::limits::nice_from("4242 (x"), None);
/// ```
pub fn nice_from(stat: &str) -> Option<i8> {
    // The name of the program can hold spaces and parentheses.
    let (_, rest) = stat.rsplit_once(')')?;
    rest.split_whitespace().nth(16)?.parse().ok()
}

/// The nice value of this process. 0 when riff cannot read it.
pub fn nice_here() -> u8 {
    std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|stat| nice_from(&stat))
        .map_or(0, |nice| nice.max(0).unsigned_abs())
}

/// What `nice -n` adds for a worker with the absolute nice value `nice`
/// in a wrapper at the nice value `here` (01M407J8R79WVYVABVCSHFAMJ9).
/// A process cannot lower its own nice value with no privileges, so a
/// wrapper at a higher value adds 0.
///
/// ```
/// use riff::limits::nice_by;
///
/// assert_eq!(nice_by(10, 0), 10);
/// assert_eq!(nice_by(10, 5), 5, "the wrapper runs at nice 5: claude gets 10");
/// assert_eq!(nice_by(10, 15), 0, "claude keeps 15");
/// assert_eq!(nice_by(0, 5), 0);
/// ```
pub fn nice_by(nice: u8, here: u8) -> u8 {
    nice.saturating_sub(here)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_jobs_of_all_workers_are_at_most_the_cores_less_one() {
        for cores in [4u16, 8, 16, 32, 64] {
            for limit in 1..cores {
                let all = jobs(cores, limit, 0) * limit;
                assert!(all < cores, "{cores} cores, {limit} workers: {all} jobs");
            }
        }
    }

    #[test]
    fn memory_high_is_less_than_memory_max() {
        let [high, max, _] = properties(22);
        assert_eq!(high, "MemoryHigh=20275M");
        assert_eq!(max, "MemoryMax=22528M");
    }

    #[test]
    fn a_scope_with_no_slice_has_no_systemd_run() {
        let command = command("claude".as_ref(), &[], 5, None, None);
        assert_eq!(command, ["nice", "-n", "5", "claude"]);
    }
}
