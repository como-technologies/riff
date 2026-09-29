//! The size and the load of a machine, for the placement of workers.
//!
//! # Design
//!
//! Each machine that runs workers tells four numbers
//! (01M3Q5QE4SQ8VYN2PSF42KB3QJ): its CPU cores, its CPU speed, its
//! memory and its 1-minute load average. A workers host puts them in
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
//! cores is busy ([`Machine::busy`]): riff starts no worker there.
//!
//! ```
//! use riff::machine::Machine;
//!
//! let thelio = Machine { cores: 32, mhz: 5800, mem_gb: 128, load: 3.0 };
//! let pangolin = Machine { cores: 16, mhz: 4500, mem_gb: 32, load: 0.5 };
//! assert!(thelio.free(0) > pangolin.free(0));
//! assert!(!thelio.busy());
//! assert!(Machine { load: 33.0, ..thelio }.busy());
//! ```

use std::fmt;

/// The variable that gives the numbers of this machine, for tests.
pub const MACHINE: &str = "RIFF_MACHINE";

/// The CPU speed of a core that counts 1 in the score.
pub const BASE_MHZ: u32 = 3000;

/// The numbers of a machine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Machine {
    /// The CPU cores.
    pub cores: u16,
    /// The most speed of one core, in MHz.
    pub mhz: u32,
    /// The total memory, in GB.
    pub mem_gb: u32,
    /// The 1-minute load average.
    pub load: f32,
}

impl Machine {
    /// The numbers of this machine. On Linux, riff reads `/proc` and
    /// `/sys`. A number that riff cannot read gets a safe value: the
    /// cores that Rust sees, [`BASE_MHZ`], 2 GB for each core, and no
    /// load. The variable [`MACHINE`] replaces them, in the form of
    /// [`Machine::parse`], so that a test does not depend on the load of
    /// the machine that runs it.
    pub fn here() -> Machine {
        if let Some(m) = std::env::var(MACHINE).ok().and_then(|m| Machine::parse(&m)) {
            return m;
        }
        let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
        let cores = std::thread::available_parallelism()
            .map_or(1, |n| u16::try_from(n.get()).unwrap_or(u16::MAX));
        Machine::from_proc(
            cores,
            &read("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq"),
            &read("/proc/cpuinfo"),
            &read("/proc/meminfo"),
            &read("/proc/loadavg"),
        )
    }

    /// The numbers from the text of the files of Linux: `max_khz` of
    /// `cpuinfo_max_freq`, `cpuinfo`, `meminfo` and `loadavg`. An empty
    /// text gives the safe value of [`Machine::here`].
    ///
    /// ```
    /// use riff::machine::Machine;
    ///
    /// let m = Machine::from_proc(
    ///     32,
    ///     "5883197\n",
    ///     "cpu MHz\t\t: 3694.638\n",
    ///     "MemTotal:       131015912 kB\n",
    ///     "18.96 15.63 8.79 2/3145 201397\n",
    /// );
    /// assert_eq!(m, Machine { cores: 32, mhz: 5883, mem_gb: 124, load: 18.96 });
    ///
    /// let m = Machine::from_proc(8, "", "cpu MHz\t: 2400.0\ncpu MHz\t: 3100.5\n", "", "");
    /// assert_eq!(m, Machine { cores: 8, mhz: 3100, mem_gb: 16, load: 0.0 });
    /// assert_eq!(Machine::from_proc(4, "", "", "", "").mhz, riff::machine::BASE_MHZ);
    /// ```
    pub fn from_proc(
        cores: u16,
        max_khz: &str,
        cpuinfo: &str,
        meminfo: &str,
        loadavg: &str,
    ) -> Machine {
        let max = max_khz.trim().parse::<u32>().ok().map(|khz| khz / 1000);
        let seen = cpuinfo
            .lines()
            .filter(|l| l.starts_with("cpu MHz"))
            .filter_map(|l| l.split(':').nth(1)?.trim().parse::<f64>().ok())
            .map(|mhz| mhz as u32)
            .max();
        let mem_gb = meminfo
            .lines()
            .find_map(|l| l.strip_prefix("MemTotal:"))
            .and_then(|kb| kb.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
            .map(|kb| u32::try_from(kb / 1024 / 1024).unwrap_or(u32::MAX));
        let load = loadavg
            .split_whitespace()
            .next()
            .and_then(|l| l.parse().ok());
        Machine {
            cores,
            mhz: max.or(seen).filter(|&m| m > 0).unwrap_or(BASE_MHZ),
            mem_gb: mem_gb.unwrap_or(u32::from(cores) * 2),
            load: load.unwrap_or(0.0),
        }
    }

    /// The number of workers that this machine runs well.
    ///
    /// ```
    /// use riff::machine::Machine;
    ///
    /// let m = Machine { cores: 8, mhz: 3000, mem_gb: 64, load: 0.0 };
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

    /// The numbers in a status: `cpu 32x5883MHz, mem 124GB, load 18.96`.
    /// [`Machine::parse`] reads it back.
    ///
    /// ```
    /// use riff::machine::Machine;
    ///
    /// let m = Machine { cores: 32, mhz: 5883, mem_gb: 124, load: 18.96 };
    /// assert_eq!(m.to_string(), "cpu 32x5883MHz, mem 124GB, load 18.96");
    /// assert_eq!(Machine::parse(&m.to_string()), Some(m));
    /// assert_eq!(Machine::parse("limit 2"), None);
    /// ```
    pub fn parse(text: &str) -> Option<Machine> {
        let rest = text.strip_prefix("cpu ")?;
        let (cores, rest) = rest.split_once('x')?;
        let (mhz, rest) = rest.split_once("MHz, mem ")?;
        let (mem, load) = rest.split_once("GB, load ")?;
        Some(Machine {
            cores: cores.parse().ok()?,
            mhz: mhz.parse().ok()?,
            mem_gb: mem.parse().ok()?,
            load: load.parse().ok()?,
        })
    }
}

impl fmt::Display for Machine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cpu {}x{}MHz, mem {}GB, load {:.2}",
            self.cores, self.mhz, self.mem_gb, self.load
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
            mem_gb: 128,
            load: 0.0,
        };
        let small = Machine {
            cores: 4,
            mhz: 6000,
            mem_gb: 16,
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
            mem_gb: 4,
            load: 0.0,
        };
        assert_eq!(m.score(), 2.0);
    }

    #[test]
    fn here_reads_sane_numbers() {
        let m = Machine::here();
        assert!(m.cores >= 1);
        assert!(m.mhz > 0);
        assert!(m.mem_gb > 0);
        assert!(m.load >= 0.0);
    }
}
