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
//! module reads them on this machine: [`here`] reads them with
//! [`crate::platform::machine`], from `/proc` and `/sys` on Linux, and
//! from `sysctl` on macOS.

use riff_client::machines::Machine;

/// The variable that gives the numbers of this machine, for tests.
pub const MACHINE: &str = "RIFF_MACHINE";

/// The numbers of this machine: the hardware threads that Rust sees,
/// and the other numbers of [`crate::platform::machine`]. A number that
/// riff cannot read gets a safe value:
/// [`BASE_MHZ`](riff_client::machines::BASE_MHZ), 2 GB for each thread,
/// all of the memory available, and no load. The variable [`MACHINE`]
/// replaces them, in the form of [`Machine::parse`], so that a test does
/// not depend on the load of the machine that runs it.
pub fn here() -> Machine {
    if let Some(m) = std::env::var(MACHINE).ok().and_then(|m| Machine::parse(&m)) {
        return m;
    }
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| u16::try_from(n.get()).unwrap_or(u16::MAX));
    crate::platform::machine(threads)
}

#[cfg(test)]
mod tests {
    use super::*;

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
