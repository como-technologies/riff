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
//! | Compile jobs and test threads of one worker | `riff workers run` | cores / worker limit, 2 or more ([`jobs`]) | `workers.jobs` |
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
//!     T --> C["systemd-run --user --scope --slice=riff-workers.slice<br/>nice -n 10 claude<br/>CARGO_BUILD_JOBS, RUST_TEST_THREADS"]
//!     U --> D["nice -n 10 claude<br/>CARGO_BUILD_JOBS, RUST_TEST_THREADS"]
//! ```
//!
//! - **Jobs** (01M3WFYZRK5CT22GJW6ZHYT9CC). The wrapper gives `claude`
//!   the variables [`JOBS_VARS`]. Each build and each test run of the
//!   worker reads them. The wrapper stays for each next item of the
//!   worker, so the variables stay too.
//! - **Nice** (01M3WFYZTX05CGDP2NQF9B356K). The wrapper starts `claude`
//!   through `nice`. Each build of the worker gives way to the other
//!   work of the machine.
//! - **Slice** (01M3WFYZX6GVFYW6NTTTKF144R). All workers of a machine
//!   run in the slice [`SLICE`] of the systemd user manager, each in a
//!   scope of its own. The slice has `MemoryHigh`, `MemoryMax` and
//!   `CPUWeight` ([`properties`]). The wrapper stays outside the slice.
//!   So when the OS kills a worker for its memory, the wrapper lives and
//!   tells the lead (01M3WFZ03Z9Y60HPHJJ9ZE6AQZ), and the desktop of the
//!   person goes on.
//! - **No systemd** (01M3WFYZZENNHVH8Z2BAFSR6TS). When `systemctl`
//!   cannot set the slice, the workers run with no scope. riff says so
//!   one time ([`scope`]).
//! - **Floor** (01M3WFZ01PTAYYKG3T5CFA2W4D). riff starts no worker while
//!   the available memory is less than the floor.
//!
//! riff sets the limits when a worker starts. It does not watch the
//! memory, and it does not change a limit while the workers run.
//!
//! ```
//! use riff::limits::{jobs, memory};
//!
//! // pangolin: 16 cores, 30 GB, a limit of 4 workers.
//! assert_eq!(jobs(16, 4, 0), 4);
//! assert_eq!(memory(30, 0), 23);
//! ```

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use anyhow::Result;

use crate::machine::Machine;
use crate::settings;

/// The variables that give a build and a test run their number of
/// jobs: the compile jobs of cargo and the test threads of a Rust test.
pub const JOBS_VARS: [&str; 2] = ["CARGO_BUILD_JOBS", "RUST_TEST_THREADS"];

/// The fewest jobs of one worker.
pub const MIN_JOBS: u16 = 2;

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

/// The jobs of one worker: `setting`, or with 0 the `cores` divided by
/// the worker `limit`, and [`MIN_JOBS`] or more
/// (01M3WFYZRK5CT22GJW6ZHYT9CC).
///
/// ```
/// use riff::limits::jobs;
///
/// assert_eq!(jobs(16, 4, 0), 4);
/// assert_eq!(jobs(32, 3, 0), 10);
/// assert_eq!(jobs(4, 4, 0), 2, "2 or more");
/// assert_eq!(jobs(16, 0, 0), 16, "no limit counts as one worker");
/// assert_eq!(jobs(16, 4, 6), 6, "the setting wins");
/// ```
pub fn jobs(cores: u16, limit: u16, setting: u16) -> u16 {
    if setting > 0 {
        return setting;
    }
    (cores / limit.max(1)).max(MIN_JOBS)
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
    /// The compile jobs and the test threads.
    pub jobs: u16,
    /// The nice value. 0 is no nice.
    pub nice: u8,
}

impl Limits {
    /// The limits from the settings file `path` and the cores of
    /// `machine`.
    ///
    /// ```
    /// use riff::limits::Limits;
    /// use riff::machine::Machine;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let path = dir.path().join("config.toml");
    /// let m = Machine { cores: 16, mhz: 4500, mem_gb: 30, avail_gb: 24, load: 0.0 };
    /// riff::settings::set_workers_limit(&path, 4)?;
    /// assert_eq!(Limits::of(&path, &m)?, Limits { jobs: 4, nice: 10 });
    /// riff::settings::set_workers_jobs(&path, 3)?;
    /// riff::settings::set_workers_nice(&path, 0)?;
    /// assert_eq!(Limits::of(&path, &m)?, Limits { jobs: 3, nice: 0 });
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn of(path: &Path, machine: &Machine) -> Result<Limits> {
        Ok(Limits {
            jobs: jobs(
                machine.cores,
                settings::workers_limit(path)?,
                settings::workers_jobs(path)?,
            ),
            nice: settings::workers_nice(path)?,
        })
    }
}

/// The command that runs `claude` with `args` for a worker: through
/// `nice` when `nice` is more than 0, and in a scope of `slice` when
/// the machine has one.
///
/// ```
/// use riff::limits::command;
///
/// let args = ["Join the riff.".to_owned()];
/// assert_eq!(command("claude".as_ref(), &args, 0, None), ["claude", "Join the riff."]);
/// assert_eq!(
///     command("claude".as_ref(), &args, 10, None),
///     ["nice", "-n", "10", "claude", "Join the riff."],
/// );
/// assert_eq!(
///     command("claude".as_ref(), &args, 10, Some("riff-workers.slice")),
///     [
///         "systemd-run", "--user", "--scope", "--quiet", "--slice=riff-workers.slice", "--",
///         "nice", "-n", "10", "claude", "Join the riff.",
///     ],
/// );
/// ```
pub fn command(claude: &Path, args: &[String], nice: u8, slice: Option<&str>) -> Vec<OsString> {
    let mut command: Vec<OsString> = Vec::new();
    if let Some(slice) = slice {
        command.extend(["systemd-run", "--user", "--scope", "--quiet"].map(OsString::from));
        command.push(format!("--slice={slice}").into());
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
            if let Some(said) = said {
                let _ = std::fs::remove_file(said);
            }
            Ok(Scope {
                slice: Some(SLICE),
                said: None,
            })
        }
        Err(why) => {
            let first = said.as_ref().is_none_or(|said| !said.exists());
            if let Some(said) = said {
                if let Some(dir) = said.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(said, "");
            }
            Ok(Scope {
                slice: None,
                said: first.then(|| crate::text::no_systemd(&why)),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_jobs_of_all_workers_are_at_most_the_cores() {
        for cores in [4u16, 8, 16, 32, 64] {
            for limit in 1..=cores / MIN_JOBS {
                let all = jobs(cores, limit, 0) * limit;
                assert!(all <= cores, "{cores} cores, {limit} workers: {all} jobs");
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
        let command = command("claude".as_ref(), &[], 5, None);
        assert_eq!(command, ["nice", "-n", "5", "claude"]);
    }
}
