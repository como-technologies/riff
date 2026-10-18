//! The free disk of a machine, for the build folders of its worktrees.
//!
//! # Design
//!
//! Each worktree of a worker has its own `target`: 30 to 43 GB. So the
//! disk of a workers machine fills fast. riff measures the file system
//! of the main clone ([`here`]) and acts at two marks
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
//! use riff_client::machines::Disk;
//!
//! let disk = Disk { free_gb: 16, total_gb: 455 };
//! assert_eq!(disk.percent(), 3);
//! assert!(disk.tight() && disk.low());
//! let fine = Disk { free_gb: 200, total_gb: 455 };
//! assert!(!fine.tight() && !fine.low());
//! ```

use std::path::Path;

use riff_client::machines::Disk;

/// The variable that gives the disk of this machine, for tests, in the
/// form of [`Disk::parse`].
pub const DISK: &str = "RIFF_DISK";

/// The disk of the file system of `dir`. The variable [`DISK`] replaces
/// it, so that a test does not depend on the disk of the machine that
/// runs it. `None` when riff cannot measure it. The marks of the disk
/// are [`riff_client::machines::TIGHT_PERCENT`] and
/// [`riff_client::machines::LOW_PERCENT`].
pub fn here(dir: &Path) -> Option<Disk> {
    if let Ok(text) = std::env::var(DISK) {
        return Disk::parse(&text);
    }
    let stat = nix::sys::statvfs::statvfs(dir).ok()?;
    let block = stat.fragment_size();
    let gb = |blocks: u64| blocks.saturating_mul(block) / 1024 / 1024 / 1024;
    Some(Disk {
        free_gb: gb(stat.blocks_available()),
        total_gb: gb(stat.blocks()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn here_measures_a_real_file_system() {
        if std::env::var(DISK).is_ok() {
            return;
        }
        let disk = here(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert!(disk.total_gb > 0);
        assert!(disk.free_gb <= disk.total_gb);
    }
}
