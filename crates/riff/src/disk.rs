//! The free disk of a machine, for the build folders of its worktrees.
//!
//! # Design
//!
//! Each worktree of a worker has its own `target`: 30 to 43 GB. So the
//! disk of a workers machine fills fast. riff measures the file system
//! of the main clone ([`Disk::here`]) and acts at two marks
//! (01M41A11BB4HAD8595DNSBAZ0D, 01M41A11DX1QRP48YPTDNT67W4):
//!
//! ```mermaid
//! flowchart TD
//!     M["each look of the host"] --> T{"free disk under 15%?"}
//!     T -- no --> K["nothing"]
//!     T -- yes --> R["remove the target of each worktree<br/>with no live owner, a note to the lead"]
//!     R --> S{"free disk under 5%?"}
//!     S -- no --> K
//!     S -- yes --> N["start no worker on this machine,<br/>one note to the lead"]
//! ```
//!
//! A workers host tells its disk in its status, and `riff workers` shows
//! it for each machine (01M41A11GHP78E2VYN14JSE27P). See
//! [`crate::tidy`].
//!
//! ```
//! use riff::disk::Disk;
//!
//! let disk = Disk { free_gb: 16, total_gb: 455 };
//! assert_eq!(disk.percent(), 3);
//! assert!(disk.tight() && disk.low());
//! let fine = Disk { free_gb: 200, total_gb: 455 };
//! assert!(!fine.tight() && !fine.low());
//! ```

use std::fmt;
use std::path::Path;

/// The variable that gives the disk of this machine, for tests, in the
/// form of [`Disk::parse`].
pub const DISK: &str = "RIFF_DISK";

/// Under this free part of the disk, in percent, riff removes the
/// `target` of each worktree with no live owner
/// (01M41A11BB4HAD8595DNSBAZ0D).
pub const TIGHT_PERCENT: u64 = 15;

/// Under this free part of the disk, in percent, riff starts no worker
/// on the machine (01M41A11DX1QRP48YPTDNT67W4).
pub const LOW_PERCENT: u64 = 5;

/// The size and the free space of a file system, in GB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Disk {
    /// The space that a user can write.
    pub free_gb: u64,
    /// The size of the file system.
    pub total_gb: u64,
}

impl Disk {
    /// The disk of the file system of `dir`. The variable [`DISK`]
    /// replaces it, so that a test does not depend on the disk of the
    /// machine that runs it. `None` when riff cannot measure it.
    pub fn here(dir: &Path) -> Option<Disk> {
        if let Ok(text) = std::env::var(DISK) {
            return Disk::parse(&text);
        }
        let stat = nix::sys::statvfs::statvfs(dir).ok()?;
        let block = u64::from(stat.fragment_size());
        let gb = |blocks: u64| blocks.saturating_mul(block) / 1024 / 1024 / 1024;
        Some(Disk {
            free_gb: gb(u64::from(stat.blocks_available())),
            total_gb: gb(u64::from(stat.blocks())),
        })
    }

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
    /// use riff::disk::Disk;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn here_measures_a_real_file_system() {
        if std::env::var(DISK).is_ok() {
            return;
        }
        let disk = Disk::here(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert!(disk.total_gb > 0);
        assert!(disk.free_gb <= disk.total_gb);
    }

    #[test]
    fn the_marks_are_strict() {
        let at = |free_gb| Disk {
            free_gb,
            total_gb: 100,
        };
        assert!(!at(15).tight());
        assert!(at(14).tight());
        assert!(!at(5).low());
        assert!(at(4).low());
        assert_eq!(Disk { free_gb: 0, total_gb: 0 }.percent(), 100);
    }
}
