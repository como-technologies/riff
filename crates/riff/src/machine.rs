//! The reads of the numbers of this machine, for the placement of
//! workers: the hardware threads, the clock, the memory and the load.
//!
//! # Design
//!
//! Each machine that runs workers tells six numbers
//! (01M3Q5QE4SQ8VYN2PSF42KB3QJ): its hardware threads, its CPU speed, its
//! clock now, its memory, the memory that is available now, and its
//! 1-minute load average. The CPU speed is the cap of the clock, not the
//! limit of the hardware: a cap or a power profile lowers it
//! (01M419XAX31FF9Z1647E881CSH). The numbers are
//! [`riff_client::machines::Machine`], with their score; a workers host
//! puts them in its status ([`riff_client::machines::HostStatus`]). This
//! module reads them on this machine: [`here`] reads `/proc` and `/sys`
//! of Linux.

use std::path::{Path, PathBuf};

use riff_client::machines::{BASE_MHZ, Machine};

/// The variable that gives the numbers of this machine, for tests.
pub const MACHINE: &str = "RIFF_MACHINE";

/// The variable that names the directory to read in place of
/// `/sys/devices/system/cpu`, for tests.
pub const CPU_SYS: &str = "RIFF_CPU_SYS";

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
    /// use riff::machine::Cpufreq;
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

/// The numbers of this machine. On Linux, riff reads `/proc` and
/// `/sys`, and the hardware threads that Rust sees. A number that riff
/// cannot read gets a safe value:
/// [`BASE_MHZ`], 2 GB for each thread, all of the
/// memory available, and no load. The variable [`MACHINE`] replaces them, in the form of
/// [`Machine::parse`], so that a test does not depend on the load of
/// the machine that runs it. For tests, [`CPU_SYS`] names the
/// directory of the cpufreq files, and [`crate::limits::CPUINFO`]
/// the file in place of `/proc/cpuinfo`.
pub fn here() -> Machine {
    if let Some(m) = std::env::var(MACHINE).ok().and_then(|m| Machine::parse(&m)) {
        return m;
    }
    let read = |path: &Path| std::fs::read_to_string(path).unwrap_or_default();
    let env_or =
        |key: &str, path: &str| std::env::var_os(key).map_or_else(|| path.into(), PathBuf::from);
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| u16::try_from(n.get()).unwrap_or(u16::MAX));
    from_proc(
        threads,
        &Cpufreq::read(&env_or(CPU_SYS, "/sys/devices/system/cpu")),
        &read(&env_or(crate::limits::CPUINFO, "/proc/cpuinfo")),
        &read(Path::new("/proc/meminfo")),
        &read(Path::new("/proc/loadavg")),
    )
}

/// The numbers from the text of the files of Linux: the cpufreq
/// files, `cpuinfo`, `meminfo` and `loadavg`. An empty text gives
/// the safe value of [`here`].
///
/// The CPU speed is the cap (01M419XAX31FF9Z1647E881CSH): the lowest
/// `scaling_max_freq` of the cores. With no cap, it is
/// `cpuinfo_max_freq`, then the most `cpu MHz` of `cpuinfo`, then
/// [`BASE_MHZ`]. The clock now is the mean of `scaling_cur_freq` of
/// the cores, then the mean of `cpu MHz`, then the CPU speed.
///
/// ```
/// use riff::machine::{Cpufreq, from_proc};
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

    #[test]
    fn here_reads_sane_numbers() {
        let m = here();
        assert!(m.threads >= 1);
        assert!(m.mhz > 0);
        assert!(m.now_mhz > 0);
        assert!(m.mem_gb > 0);
        assert!(m.avail_gb <= m.mem_gb);
        assert!(m.load >= 0.0);
    }
}
