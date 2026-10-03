//! The size and the load of a machine, for the placement of workers.
//!
//! # Design
//!
//! Each machine that runs workers tells six numbers
//! (01M3Q5QE4SQ8VYN2PSF42KB3QJ): its CPU cores, its CPU speed, its
//! clock now, its memory, the memory that is available now, and its
//! 1-minute load average. The CPU speed is the cap of the clock, not the
//! limit of the hardware: a cap or a power profile lowers it
//! (01M419XAX31FF9Z1647E881CSH). A workers host puts them in
//! its status ([`crate::host::HostStatus`]). The lead reads its own
//! machine with [`Machine::here`].
//!
//! The score of a machine is the number of workers that it can run
//! well (01M3Q5QE76BZ27SZ14FFE8HM1G). One worker needs one core and
//! 2 GB of memory. A core at [`BASE_MHZ`] counts 1, a faster core
//! counts more:
//!
//! ```text
//! score = min(cores, memory GB / 2) × MHz / 3000
//! ```
//!
//! The score less the workers that run there is the free capacity
//! ([`Machine::free`]). A machine whose load average is more than its
//! cores is busy ([`Machine::busy`]): riff starts no worker there. A
//! machine whose available memory is less than its floor is low
//! ([`Machine::low`], see [`crate::limits`]): riff starts no worker
//! there too.
//!
//! ```
//! use riff::machine::Machine;
//!
//! let thelio = Machine { cores: 32, mhz: 5800, now_mhz: 4100, mem_gb: 128, avail_gb: 100, load: 3.0 };
//! let pangolin = Machine { cores: 16, mhz: 4500, now_mhz: 4400, mem_gb: 32, avail_gb: 3, load: 0.5 };
//! assert!(thelio.free(0) > pangolin.free(0));
//! assert!(!thelio.busy());
//! assert!(Machine { load: 33.0, ..thelio }.busy());
//! assert!(pangolin.low(4));
//! assert!(!thelio.low(4));
//! ```

use std::fmt;
use std::path::{Path, PathBuf};

/// The variable that gives the numbers of this machine, for tests.
pub const MACHINE: &str = "RIFF_MACHINE";

/// The variable that names the directory to read in place of
/// `/sys/devices/system/cpu`, for tests.
pub const CPU_SYS: &str = "RIFF_CPU_SYS";

/// The CPU speed of a core that counts 1 in the score.
pub const BASE_MHZ: u32 = 3000;

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

/// The numbers of a machine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Machine {
    /// The CPU cores.
    pub cores: u16,
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
    /// The numbers of this machine. On Linux, riff reads `/proc` and
    /// `/sys`. A number that riff cannot read gets a safe value: the
    /// cores that Rust sees, [`BASE_MHZ`], 2 GB for each core, all of the
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
        let cores = std::thread::available_parallelism()
            .map_or(1, |n| u16::try_from(n.get()).unwrap_or(u16::MAX));
        Machine::from_proc(
            cores,
            &Cpufreq::read(&env_or(CPU_SYS, "/sys/devices/system/cpu")),
            &read(&env_or(crate::limits::CPUINFO, "/proc/cpuinfo")),
            &read(Path::new("/proc/meminfo")),
            &read(Path::new("/proc/loadavg")),
        )
    }

    /// The numbers from the text of the files of Linux: the cpufreq
    /// files, `cpuinfo`, `meminfo` and `loadavg`. An empty text gives
    /// the safe value of [`Machine::here`].
    ///
    /// The CPU speed is the cap (01M419XAX31FF9Z1647E881CSH): the lowest
    /// `scaling_max_freq` of the cores. With no cap, it is
    /// `cpuinfo_max_freq`, then the most `cpu MHz` of `cpuinfo`, then
    /// [`BASE_MHZ`]. The clock now is the mean of `scaling_cur_freq` of
    /// the cores, then the mean of `cpu MHz`, then the CPU speed.
    ///
    /// ```
    /// use riff::machine::{Cpufreq, Machine};
    ///
    /// let texts = |t: &[&str]| t.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    /// // A cap of 3.0 GHz below a hardware limit of 4.0 GHz: the cap counts.
    /// let capped = Cpufreq {
    ///     hardware: "4001000\n".into(),
    ///     caps: texts(&["3000000\n", "3000000\n", "3200000\n"]),
    ///     now: texts(&["2990000\n", "2980000\n", "3000000\n"]),
    /// };
    /// let m = Machine::from_proc(
    ///     16,
    ///     &capped,
    ///     "cpu MHz\t\t: 3694.638\n",
    ///     "MemTotal:       32505856 kB\nMemAvailable:   25165824 kB\n",
    ///     "0.50 0.40 0.30 2/3145 201397\n",
    /// );
    /// assert_eq!(m, Machine { cores: 16, mhz: 3000, now_mhz: 2990, mem_gb: 31, avail_gb: 24, load: 0.5 });
    ///
    /// // No cap files: the old order, `cpuinfo_max_freq` first.
    /// let hardware = Cpufreq { hardware: "5883197\n".into(), ..Cpufreq::default() };
    /// let m = Machine::from_proc(32, &hardware, "cpu MHz\t\t: 3694.638\n", "", "18.96 1 1\n");
    /// assert_eq!((m.mhz, m.now_mhz, m.load), (5883, 3694, 18.96));
    ///
    /// // Then the most `cpu MHz`. The clock now is their mean.
    /// let cpuinfo = "cpu MHz\t: 2400.0\ncpu MHz\t: 3100.5\n";
    /// let m = Machine::from_proc(8, &Cpufreq::default(), cpuinfo, "", "");
    /// assert_eq!(m, Machine { cores: 8, mhz: 3100, now_mhz: 2750, mem_gb: 16, avail_gb: 16, load: 0.0 });
    ///
    /// // Then BASE_MHZ.
    /// let m = Machine::from_proc(4, &Cpufreq::default(), "", "", "");
    /// assert_eq!((m.mhz, m.now_mhz), (riff::machine::BASE_MHZ, riff::machine::BASE_MHZ));
    /// ```
    pub fn from_proc(
        cores: u16,
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
        let mem_gb = gb("MemTotal:").unwrap_or(u32::from(cores) * 2);
        let load = loadavg
            .split_whitespace()
            .next()
            .and_then(|l| l.parse().ok());
        Machine {
            cores,
            mhz,
            now_mhz: mean(&mhz_of(&freq.now))
                .or_else(|| mean(&seen))
                .unwrap_or(mhz),
            mem_gb,
            avail_gb: gb("MemAvailable:").unwrap_or(mem_gb),
            load: load.unwrap_or(0.0),
        }
    }

    /// The number of workers that this machine runs well. It counts
    /// the cap of the clock, not the clock now.
    ///
    /// ```
    /// use riff::machine::Machine;
    ///
    /// let m = Machine { cores: 8, mhz: 3000, now_mhz: 1400, mem_gb: 64, avail_gb: 64, load: 0.0 };
    /// assert_eq!(m.score(), 8.0);
    /// assert_eq!(Machine { mem_gb: 8, ..m }.score(), 4.0);
    /// assert_eq!(Machine { mhz: 6000, ..m }.score(), 16.0);
    /// ```
    pub fn score(&self) -> f64 {
        let size = f64::from(self.cores).min(f64::from(self.mem_gb) / 2.0);
        size * f64::from(self.mhz) / f64::from(BASE_MHZ)
    }

    /// The score less `workers`: the room for more workers.
    pub fn free(&self, workers: usize) -> f64 {
        self.score() - workers as f64
    }

    /// True when the load average is more than the cores.
    pub fn busy(&self) -> bool {
        f64::from(self.load) > f64::from(self.cores)
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
    /// use riff::machine::Machine;
    ///
    /// let m = Machine { cores: 32, mhz: 5883, now_mhz: 4100, mem_gb: 124, avail_gb: 100, load: 18.96 };
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
        let (cores, rest) = rest.split_once('x')?;
        let (speed, rest) = rest.split_once("MHz, mem ")?;
        // A host of the release before tells no clock now
        // (01M419XAZBPV0Y08CAR51KQSZS).
        let (mhz, now_mhz) = match speed.split_once("MHz (now ") {
            Some((mhz, now)) => (mhz.parse().ok()?, now.strip_suffix(')')?.parse().ok()?),
            None => {
                let mhz = speed.parse().ok()?;
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
            cores: cores.parse().ok()?,
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
            self.cores, self.mhz, self.now_mhz, self.mem_gb, self.avail_gb, self.load
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_fast_machine_loses_to_a_big_one() {
        let big = Machine {
            cores: 32,
            mhz: 4000,
            now_mhz: 4000,
            mem_gb: 128,
            avail_gb: 128,
            load: 0.0,
        };
        let small = Machine {
            cores: 4,
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
    fn little_memory_caps_the_score() {
        let m = Machine {
            cores: 16,
            mhz: 3000,
            now_mhz: 3000,
            mem_gb: 4,
            avail_gb: 4,
            load: 0.0,
        };
        assert_eq!(m.score(), 2.0);
    }

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
        let free = Machine::from_proc(16, &uncapped, "", meminfo, "0.5");
        let cap = Machine::from_proc(16, &capped, "", meminfo, "0.5");
        assert_eq!((free.mhz, cap.mhz), (4001, 3000));
        assert_eq!(cap.now_mhz, 2990);
        assert!(cap.score() < free.score(), "{cap:?} {free:?}");
        // Only one core with a cap also caps the machine.
        let mut one = uncapped.clone();
        one.caps[7] = "3000000".into();
        assert_eq!(Machine::from_proc(16, &one, "", meminfo, "").mhz, 3000);
    }

    #[test]
    fn here_reads_sane_numbers() {
        let m = Machine::here();
        assert!(m.cores >= 1);
        assert!(m.mhz > 0);
        assert!(m.now_mhz > 0);
        assert!(m.mem_gb > 0);
        assert!(m.avail_gb <= m.mem_gb);
        assert!(m.load >= 0.0);
    }
}
