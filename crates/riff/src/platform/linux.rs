//! The reads of the OS on Linux (01M6YQDWT387EV7Y7XMT2G6YVM): `/proc`
//! and `/sys`. The service manager of the daemon is systemd
//! ([`super::systemd`]). See [`crate::platform`].

use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use riff_client::machines::{BASE_MHZ, Machine};

use super::{Cgroups, Memory, Process};

/// The folder of the processes.
const PROC: &str = "/proc";

/// The ticks of the clock in one second: the unit of the start time in
/// `/proc/PID/stat`. It is 100 on each Linux that riff supports.
const TICKS: u64 = 100;

/// The service manager of the daemon on Linux: systemd.
pub type Manager = super::systemd::Systemd;

pub(super) fn service_manager() -> anyhow::Result<Manager> {
    super::systemd::Systemd::here()
}

pub(super) fn process(pid: u32) -> Option<Process> {
    let root = Path::new(PROC);
    let own = std::fs::read_link(root.join("self/ns/pid")).ok();
    read_process(root, pid, uptime(root), own.as_deref())
}

pub(super) fn processes() -> Vec<Process> {
    processes_in(Path::new(PROC), std::process::id())
}

/// The processes in `root`, a folder with the layout of `/proc`, of the
/// user of the process `me`, in the order of their IDs. A process in a
/// container of podman has [`Process::in_container`].
///
/// ```
/// use riff::platform::linux::processes_in;
///
/// // A folder with the layout of /proc: the daemon, a shell, and a
/// // process of a container.
/// let root = tempfile::tempdir()?;
/// let write = |pid: u32, ppid: u32, comm: &str, args: &str, start: u64| -> std::io::Result<()> {
///     let dir = root.path().join(pid.to_string());
///     std::fs::create_dir_all(&dir)?;
///     let mut stat = format!("{pid} ({comm}) S {ppid}");
///     for _ in 0..15 { stat.push_str(" 0"); }
///     stat.push_str(&format!(" 20 0 {start} 0"));
///     std::fs::write(dir.join("stat"), stat)?;
///     std::fs::write(dir.join("cmdline"), args.replace(' ', "\0") + "\0")?;
///     Ok(())
/// };
/// std::fs::write(root.path().join("uptime"), "1000.00 900.00\n")?;
/// write(10, 1, "riff", "/usr/bin/riff daemon", 10_000)?;
/// write(20, 1, "bash", "bash", 50_000)?;
/// write(30, 1, "claude", "claude -p", 60_000)?;
/// std::fs::write(root.path().join("30/cgroup"), "0::/user.slice/libpod-ab12.scope/container\n")?;
/// let procs = processes_in(root.path(), 10);
/// let ids: Vec<(u32, u32, u64, &str)> =
///     procs.iter().map(|p| (p.pid, p.ppid, p.secs, p.name.as_str())).collect();
/// assert_eq!(ids, [(10, 1, 900, "riff"), (20, 1, 500, "bash"), (30, 1, 400, "claude")]);
/// assert_eq!(procs[0].args, ["/usr/bin/riff", "daemon"]);
/// assert_eq!(procs[0].start, 10_000);
/// let contained: Vec<bool> = procs.iter().map(|p| p.in_container).collect();
/// assert_eq!(contained, [false, false, true]);
/// assert!(processes_in(root.path(), 99).is_empty());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn processes_in(root: &Path, me: u32) -> Vec<Process> {
    let Ok(mine) = std::fs::metadata(root.join(me.to_string())) else {
        return Vec::new();
    };
    let own = std::fs::read_link(root.join(me.to_string()).join("ns/pid")).ok();
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let up = uptime(root);
    let mut all: Vec<Process> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            std::fs::metadata(root.join(pid.to_string())).is_ok_and(|m| m.uid() == mine.uid())
        })
        .filter_map(|pid| read_process(root, pid, up, own.as_deref()))
        .collect();
    all.sort_by_key(|process| process.pid);
    all
}

/// The seconds after the boot, from the file `uptime` in `root`. 0 when
/// riff cannot read it.
fn uptime(root: &Path) -> f64 {
    std::fs::read_to_string(root.join("uptime"))
        .ok()
        .and_then(|text| text.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// The process `pid` in `root`, a folder with the layout of `/proc`,
/// `uptime` seconds after the boot. `own` is the PID namespace of this
/// process. `None` when it ended or riff cannot read it.
fn read_process(root: &Path, pid: u32, uptime: f64, own: Option<&Path>) -> Option<Process> {
    let dir = root.join(pid.to_string());
    let stat = parse_stat(&std::fs::read_to_string(dir.join("stat")).ok()?)?;
    let uid = std::fs::metadata(&dir).ok()?.uid();
    let link = std::fs::read_link(dir.join("exe")).ok();
    let up_ticks = (uptime * TICKS as f64) as u64;
    Some(Process {
        pid,
        ppid: stat.ppid,
        uid,
        start: stat.start,
        secs: up_ticks.saturating_sub(stat.start) / TICKS,
        name: stat.name,
        args: split_nul(&std::fs::read(dir.join("cmdline")).unwrap_or_default()),
        replaced: link.as_deref().is_some_and(deleted),
        exe: link.map(crate::binary::on_disk),
        in_container: in_container(&dir, own),
    })
}

/// The fields of `/proc/PID/stat` that riff reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stat {
    /// The name of the program: field 2, in parentheses.
    pub name: String,
    /// The parent: field 4.
    pub ppid: u32,
    /// The start, in ticks after the boot: field 22.
    pub start: u64,
}

/// The [`Stat`] in the text of `/proc/PID/stat`. The name of the
/// program can hold spaces and parentheses, so the fields after it start
/// after the last `)`.
///
/// ```
/// use riff::platform::linux::{Stat, parse_stat};
///
/// let stat = "1234 (a b) S 42 1234 1234 0 -1 4194560 1 0 0 0 0 0 0 0 20 0 1 0 98765 1 2";
/// assert_eq!(parse_stat(stat), Some(Stat { name: "a b".into(), ppid: 42, start: 98765 }));
/// let odd = "1234 (a) b (c) S 7 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 5 1 2";
/// assert_eq!(parse_stat(odd).map(|s| (s.name, s.ppid)), Some(("a) b (c".into(), 7)));
/// assert_eq!(parse_stat("1234 (riff) S 42 1234"), None);
/// assert_eq!(parse_stat("garbage"), None);
/// ```
pub fn parse_stat(stat: &str) -> Option<Stat> {
    let (head, tail) = stat.rsplit_once(')')?;
    let name = head.split_once('(')?.1.to_owned();
    let fields: Vec<&str> = tail.split_whitespace().collect();
    Some(Stat {
        name,
        ppid: fields.get(1)?.parse().ok()?,
        start: fields.get(19)?.parse().ok()?,
    })
}

/// The strings of `raw`, each with a NUL after it, as `cmdline` holds
/// them. An empty string is not there.
fn split_nul(raw: &[u8]) -> Vec<String> {
    raw.split(|&b| b == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect()
}

/// True when the process in `dir` runs in a container of podman: its
/// cgroup is a `libpod-ID.scope` but not the scope of conmon, or its PID
/// namespace is not `own`.
fn in_container(dir: &Path, own: Option<&Path>) -> bool {
    let cgroup = std::fs::read_to_string(dir.join("cgroup")).unwrap_or_default();
    let scoped = cgroup
        .split(['/', '\n'])
        .any(|part| part.starts_with("libpod-") && !part.starts_with("libpod-conmon-"));
    let other = match (std::fs::read_link(dir.join("ns/pid")), own) {
        (Ok(ns), Some(own)) => ns != own,
        _ => false,
    };
    scoped || other
}

/// True when the link `exe` names a file that was replaced or removed:
/// Linux names it `PATH (deleted)`.
fn deleted(exe: &Path) -> bool {
    exe.to_str()
        .is_some_and(|path| path.ends_with(" (deleted)"))
}

/// True when the link `exe`, for example `/proc/self/exe`, names a file
/// that was replaced or removed: Linux names it `PATH (deleted)`. A link
/// that riff cannot read is not replaced.
///
/// ```
/// use std::os::fd::AsRawFd;
/// use std::path::PathBuf;
/// use riff::platform::linux::replaced;
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("riff");
/// std::fs::write(&path, "old")?;
/// // An open file has a link in /proc, as a process has for its binary.
/// let file = std::fs::File::open(&path)?;
/// let link = PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()));
/// assert!(!replaced(&link));
/// // A new file in its place, as `riff update` does.
/// std::fs::write(dir.path().join("new"), "new")?;
/// std::fs::rename(dir.path().join("new"), &path)?;
/// assert!(replaced(&link));
/// assert!(!replaced(&dir.path().join("no link")));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn replaced(exe: &Path) -> bool {
    std::fs::read_link(exe).is_ok_and(|path| deleted(&path))
}

pub(super) fn own_replaced() -> bool {
    replaced(Path::new("/proc/self/exe"))
}

pub(super) fn running_binary() -> PathBuf {
    PathBuf::from("/proc/self/exe")
}

/// The children of each thread of `pid`, from `/proc/PID/task/*/children`.
pub(super) fn children(pid: u32) -> Vec<u32> {
    let Ok(tasks) = std::fs::read_dir(format!("{PROC}/{pid}/task")) else {
        return Vec::new();
    };
    tasks
        .flatten()
        .filter_map(|task| std::fs::read_to_string(task.path().join("children")).ok())
        .flat_map(|list| {
            list.split_whitespace()
                .filter_map(|p| p.parse().ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(super) fn environ(pid: u32) -> Option<Vec<u8>> {
    std::fs::read(format!("{PROC}/{pid}/environ")).ok()
}

pub(super) fn cwd(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("{PROC}/{pid}/cwd")).ok()
}

pub(super) fn open_files(pid: u32) -> Vec<PathBuf> {
    std::fs::read_dir(format!("{PROC}/{pid}/fd"))
        .map(|fds| {
            fds.filter_map(|fd| std::fs::read_link(fd.ok()?.path()).ok())
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn socket_path(folder: &File, _dir: &Path, name: &str) -> std::io::Result<PathBuf> {
    Ok(PathBuf::from(format!(
        "{PROC}/self/fd/{}/{name}",
        folder.as_raw_fd()
    )))
}

pub(super) fn memory() -> Option<Memory> {
    memory_in(&std::fs::read_to_string(format!("{PROC}/meminfo")).ok()?)
}

/// The [`Memory`] in the text of `/proc/meminfo`: `MemTotal` and
/// `MemAvailable`, in kB.
///
/// ```
/// use riff::platform::{Memory, linux::memory_in};
///
/// let text = "MemTotal:       32768000 kB\nMemFree:  1000 kB\nMemAvailable:   16384000 kB\n";
/// let memory = Memory { total: 32_768_000 * 1024, available: 16_384_000 * 1024 };
/// assert_eq!(memory_in(text), Some(memory));
/// assert_eq!(memory_in("MemTotal: 1 kB\n"), None);
/// ```
pub fn memory_in(meminfo: &str) -> Option<Memory> {
    let bytes = |key: &str| {
        meminfo.lines().find_map(|line| {
            let kb = line.strip_prefix(key)?;
            let kb: u64 = kb.trim().strip_suffix("kB")?.trim().parse().ok()?;
            kb.checked_mul(1024)
        })
    };
    Some(Memory {
        total: bytes("MemTotal:")?,
        available: bytes("MemAvailable:")?,
    })
}

/// The folder in the variable `var`, else `default`. An empty value
/// counts as unset.
fn dir_of(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from(default), PathBuf::from)
}

pub(super) fn cgroups(uid: u32) -> Cgroups {
    let root = dir_of(crate::first_run::CGROUP_VAR, "/sys/fs/cgroup");
    let read = |path: PathBuf| std::fs::read_to_string(path).ok();
    Cgroups {
        root: read(root.join("cgroup.controllers")),
        user: read(user_cgroup(&root, uid)),
    }
}

/// The `cgroup.controllers` of `user@UID.service` in the root `root` of
/// cgroup v2.
///
/// ```
/// use std::path::Path;
///
/// assert_eq!(
///     riff::platform::linux::user_cgroup(Path::new("/sys/fs/cgroup"), 1000),
///     Path::new("/sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service/cgroup.controllers")
/// );
/// ```
pub fn user_cgroup(root: &Path, uid: u32) -> PathBuf {
    root.join(format!(
        "user.slice/user-{uid}.slice/user@{uid}.service/cgroup.controllers"
    ))
}

pub(super) fn machine(threads: u16) -> Machine {
    let read = |path: &Path| std::fs::read_to_string(path).unwrap_or_default();
    from_proc(
        threads,
        &Cpufreq::read(&dir_of(CPU_SYS, "/sys/devices/system/cpu")),
        &read(&dir_of(CPUINFO, "/proc/cpuinfo")),
        &read(Path::new("/proc/meminfo")),
        &read(Path::new("/proc/loadavg")),
    )
}

/// The variable that names the directory to read in place of
/// `/sys/devices/system/cpu`, for tests.
pub const CPU_SYS: &str = "RIFF_CPU_SYS";

/// The variable that names the file to read in place of
/// `/proc/cpuinfo`, for tests.
pub const CPUINFO: &str = "RIFF_CPUINFO";

/// The text of the cpufreq files of Linux, in kHz.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cpufreq {
    /// `cpuinfo_max_freq` of the first core: the limit of the hardware.
    pub hardware: String,
    /// `scaling_max_freq` of each core: the cap.
    pub caps: Vec<String>,
    /// `scaling_cur_freq` of each core: the clock now.
    pub now: Vec<String>,
}

impl Cpufreq {
    /// The files under `dir`, the directory `/sys/devices/system/cpu`
    /// of Linux. A file that riff cannot read gives an empty text.
    ///
    /// ```
    /// use riff::platform::linux::Cpufreq;
    ///
    /// let sys = tempfile::tempdir()?;
    /// for (core, cap) in [("cpu0", "3000000"), ("cpu1", "3200000")] {
    ///     let dir = sys.path().join(core).join("cpufreq");
    ///     std::fs::create_dir_all(&dir)?;
    ///     std::fs::write(dir.join("scaling_max_freq"), cap)?;
    /// }
    /// std::fs::create_dir(sys.path().join("cpufreq"))?; // not a core
    /// let mut freq = Cpufreq::read(sys.path());
    /// freq.caps.sort();
    /// assert_eq!(freq.caps, ["3000000", "3200000"]);
    /// assert_eq!(freq.now, ["", ""]);
    /// assert_eq!(freq.hardware, "");
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn read(dir: &Path) -> Cpufreq {
        let read = |path: PathBuf| std::fs::read_to_string(path).unwrap_or_default();
        let mut freq = Cpufreq {
            hardware: read(dir.join("cpu0/cpufreq/cpuinfo_max_freq")),
            ..Cpufreq::default()
        };
        for core in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = core.file_name();
            let number = name.to_str().and_then(|n| n.strip_prefix("cpu"));
            if number.is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) {
                let cpufreq = core.path().join("cpufreq");
                freq.caps.push(read(cpufreq.join("scaling_max_freq")));
                freq.now.push(read(cpufreq.join("scaling_cur_freq")));
            }
        }
        freq
    }
}

/// The numbers in kHz of `texts`, in MHz. A text that is not a number,
/// or is 0, gives none.
fn mhz_of(texts: &[String]) -> Vec<u32> {
    texts
        .iter()
        .filter_map(|t| t.trim().parse::<u32>().ok())
        .map(|khz| khz / 1000)
        .filter(|&mhz| mhz > 0)
        .collect()
}

/// The mean of `mhz`, or none when it is empty.
fn mean(mhz: &[u32]) -> Option<u32> {
    let n = u64::try_from(mhz.len()).ok().filter(|&n| n > 0)?;
    let sum: u64 = mhz.iter().map(|&m| u64::from(m)).sum();
    u32::try_from(sum / n).ok()
}

/// The numbers from the text of the files of Linux: the cpufreq
/// files, `cpuinfo`, `meminfo` and `loadavg`. An empty text gives
/// the safe value of [`crate::machine::here`].
///
/// The CPU speed is the cap (01M419XAX31FF9Z1647E881CSH): the lowest
/// `scaling_max_freq` of the cores. With no cap, it is
/// `cpuinfo_max_freq`, then the most `cpu MHz` of `cpuinfo`, then
/// [`BASE_MHZ`]. The clock now is the mean of `scaling_cur_freq` of
/// the cores, then the mean of `cpu MHz`, then the CPU speed.
///
/// ```
/// use riff::platform::linux::{Cpufreq, from_proc};
/// use riff_client::machines::{BASE_MHZ, Machine};
///
/// let texts = |t: &[&str]| t.iter().map(|s| s.to_string()).collect::<Vec<_>>();
/// // A cap of 3.0 GHz below a hardware limit of 4.0 GHz: the cap counts.
/// let capped = Cpufreq {
///     hardware: "4001000\n".into(),
///     caps: texts(&["3000000\n", "3000000\n", "3200000\n"]),
///     now: texts(&["2990000\n", "2980000\n", "3000000\n"]),
/// };
/// let m = from_proc(
///     16,
///     &capped,
///     "cpu MHz\t\t: 3694.638\n",
///     "MemTotal:       32505856 kB\nMemAvailable:   25165824 kB\n",
///     "0.50 0.40 0.30 2/3145 201397\n",
/// );
/// assert_eq!(m, Machine { threads: 16, mhz: 3000, now_mhz: 2990, mem_gb: 31, avail_gb: 24, load: 0.5 });
///
/// // No cap files: the old order, `cpuinfo_max_freq` first.
/// let hardware = Cpufreq { hardware: "5883197\n".into(), ..Cpufreq::default() };
/// let m = from_proc(32, &hardware, "cpu MHz\t\t: 3694.638\n", "", "18.96 1 1\n");
/// assert_eq!((m.mhz, m.now_mhz, m.load), (5883, 3694, 18.96));
///
/// // Then the most `cpu MHz`. The clock now is their mean.
/// let cpuinfo = "cpu MHz\t: 2400.0\ncpu MHz\t: 3100.5\n";
/// let m = from_proc(8, &Cpufreq::default(), cpuinfo, "", "");
/// assert_eq!(m, Machine { threads: 8, mhz: 3100, now_mhz: 2750, mem_gb: 16, avail_gb: 16, load: 0.0 });
///
/// // Then BASE_MHZ.
/// let m = from_proc(4, &Cpufreq::default(), "", "", "");
/// assert_eq!((m.mhz, m.now_mhz), (BASE_MHZ, BASE_MHZ));
/// ```
pub fn from_proc(
    threads: u16,
    freq: &Cpufreq,
    cpuinfo: &str,
    meminfo: &str,
    loadavg: &str,
) -> Machine {
    let seen: Vec<u32> = cpuinfo
        .lines()
        .filter(|l| l.starts_with("cpu MHz"))
        .filter_map(|l| l.split(':').nth(1)?.trim().parse::<f64>().ok())
        .map(|mhz| mhz as u32)
        .filter(|&mhz| mhz > 0)
        .collect();
    let mhz = mhz_of(&freq.caps)
        .into_iter()
        .min()
        .or_else(|| mhz_of(std::slice::from_ref(&freq.hardware)).pop())
        .or_else(|| seen.iter().copied().max())
        .unwrap_or(BASE_MHZ);
    let gb = |key: &str| {
        meminfo
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|kb| kb.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
            .map(|kb| u32::try_from(kb / 1024 / 1024).unwrap_or(u32::MAX))
    };
    let mem_gb = gb("MemTotal:").unwrap_or(u32::from(threads) * 2);
    let load = loadavg
        .split_whitespace()
        .next()
        .and_then(|l| l.parse().ok());
    Machine {
        threads,
        mhz,
        now_mhz: mean(&mhz_of(&freq.now))
            .or_else(|| mean(&seen))
            .unwrap_or(mhz),
        mem_gb,
        avail_gb: gb("MemAvailable:").unwrap_or(mem_gb),
        load: load.unwrap_or(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 01M419XAX31FF9Z1647E881CSH: a cap of 3000 MHz gives a lower
    /// score than the same machine with no cap.
    #[test]
    fn a_cap_of_the_clock_lowers_the_score() {
        let texts = |t: &[&str]| t.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let meminfo = "MemTotal:       32505856 kB\nMemAvailable:   25165824 kB\n";
        let uncapped = Cpufreq {
            hardware: "4001000".into(),
            caps: texts(&["4001000"; 16]),
            now: texts(&["3900000"; 16]),
        };
        let capped = Cpufreq {
            caps: texts(&["3000000"; 16]),
            now: texts(&["2990000"; 16]),
            ..uncapped.clone()
        };
        let free = from_proc(16, &uncapped, "", meminfo, "0.5");
        let cap = from_proc(16, &capped, "", meminfo, "0.5");
        assert_eq!((free.mhz, cap.mhz), (4001, 3000));
        assert_eq!(cap.now_mhz, 2990);
        assert!(cap.score() < free.score(), "{cap:?} {free:?}");
        // Only one core with a cap also caps the machine.
        let mut one = uncapped.clone();
        one.caps[7] = "3000000".into();
        assert_eq!(from_proc(16, &one, "", meminfo, "").mhz, 3000);
    }

    /// Writes the process `pid` in a folder `root` with the layout of
    /// `/proc`: its parent, the name of its program, its command line and
    /// its start in ticks after the boot. `root/uptime` says 1,000
    /// seconds.
    fn fake_process(root: &Path, (pid, ppid): (u32, u32), comm: &str, args: &str, start: u64) {
        let dir = root.join(pid.to_string());
        std::fs::create_dir_all(&dir).unwrap();
        // The 17 fields between the parent and the start time.
        let stat = format!("{pid} ({comm}) S {ppid}{} 20 0 {start} 0", " 0".repeat(15));
        std::fs::write(dir.join("stat"), stat).unwrap();
        std::fs::write(dir.join("cmdline"), args.replace(' ', "\0") + "\0").unwrap();
        std::fs::write(root.join("uptime"), "1000.00 900.00\n").unwrap();
    }

    /// 01M6QBXNMEBANX439DF7J1PQ3S (#1610): a process in a container shows
    /// in the `/proc` of the host. `riff ps` leaves it to the part of the
    /// container: it shows the daemon and a command outside a container
    /// only. The scope of conmon is not a container.
    #[test]
    fn a_process_in_a_container_is_not_a_command_outside_it() {
        let root = tempfile::tempdir().unwrap();
        let cgroup = |pid: u32, path: &str| {
            std::fs::write(root.path().join(pid.to_string()).join("cgroup"), path).unwrap();
        };
        fake_process(root.path(), (10, 1), "riff", "riff daemon", 10_000);
        fake_process(
            root.path(),
            (50, 40),
            "riff",
            "/opt/riff/riff relay -- claude",
            20_000,
        );
        cgroup(
            50,
            "0::/user.slice/user@1000.service/user.slice/libpod-abc123.scope/container\n",
        );
        fake_process(
            root.path(),
            (40, 1),
            "conmon",
            "conmon --api-version 1",
            20_000,
        );
        cgroup(
            40,
            "0::/user.slice/user@1000.service/user.slice/libpod-conmon-abc123.scope\n",
        );
        fake_process(root.path(), (60, 1), "riff", "riff top", 30_000);
        cgroup(
            60,
            "0::/user.slice/user@1000.service/app.slice/tmux.scope\n",
        );
        // Another PID namespace than the daemon: a container too.
        fake_process(root.path(), (70, 1), "riff", "riff relay", 30_000);
        let ns = |pid: u32, id: u32| {
            let dir = root.path().join(pid.to_string()).join("ns");
            std::fs::create_dir_all(&dir).unwrap();
            std::os::unix::fs::symlink(format!("pid:[{id}]"), dir.join("pid")).unwrap();
        };
        ns(10, 1);
        ns(60, 1);
        ns(70, 2);
        let all = processes_in(root.path(), 10);
        let contained: Vec<u32> = all
            .iter()
            .filter(|p| p.in_container)
            .map(|p| p.pid)
            .collect();
        assert_eq!(contained, [50, 70]);
        let pids: Vec<u32> = crate::ps::host(&all, 10).iter().map(|p| p.pid).collect();
        // conmon (40) is the parent of the relay in the container, but the
        // chain of a riff process in a container is not read at all.
        assert_eq!(pids, [10, 60]);
    }

    /// The daemon that `/proc` does not show gives no process: `riff ps`
    /// says so.
    #[test]
    fn a_folder_without_the_daemon_gives_no_process() {
        let root = tempfile::tempdir().unwrap();
        fake_process(root.path(), (60, 1), "riff", "riff top", 30_000);
        assert!(processes_in(root.path(), 10).is_empty());
        assert!(processes_in(&root.path().join("no-folder"), 10).is_empty());
    }
}
