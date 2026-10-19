//! The clone of a check: `riff build` (01M5084K79M5XJT3MP8FFVQKDX,
//! 01M56P380A5PXGMVJVCSD8KBRG).
//!
//! A check after a merge can need a new build of riff, for example of
//! the daemon. The build is in the one clone `~/.cache/riff/check` of
//! the machine, outside `.claude/worktrees`. Claude Code refuses a git
//! step of a session in a worktree that targets another folder. So the
//! session runs one `riff build`, and riff runs the git steps and the
//! build:
//!
//! ```mermaid
//! flowchart TD
//!     B["riff build"] --> X{"~/.cache/riff/check is there?"}
//!     X -- no --> C["git clone of the origin of this folder"]
//!     X -- yes --> F["git fetch -q origin"]
//!     C --> F
//!     F --> D["git checkout -q --detach origin/HEAD"]
//!     D --> J["just static"]
//!     J --> P["print the path of the riff of the build"]
//! ```
//!
//! ```
//! use std::path::Path;
//!
//! let check = riff::check::dir(Path::new("/home/mike"));
//! assert_eq!(check, Path::new("/home/mike/.cache/riff/check"));
//! assert_eq!(
//!     riff::check::riff_of(&check),
//!     Path::new("/home/mike/.cache/riff/check/target/static/release/riff")
//! );
//! ```

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, bail};

use crate::run::output;
use crate::text;

/// The one clone of a check on the machine of `home`.
pub fn dir(home: &Path) -> PathBuf {
    home.join(".cache/riff/check")
}

/// The variable that names the check clone, when `~/.cache` is not
/// writable: the lead in a container has it (01M59269HHZ5C82DR3RG3QHAM3).
pub const VAR: &str = "RIFF_CHECK";

/// The check clone: the folder in `value` of [`VAR`], else [`dir`].
///
/// ```
/// use std::path::Path;
///
/// let home = Path::new("/home/mike");
/// assert_eq!(riff::check::dir_from(None, home), home.join(".cache/riff/check"));
/// assert_eq!(riff::check::dir_from(Some("".into()), home), home.join(".cache/riff/check"));
/// assert_eq!(riff::check::dir_from(Some("/c".into()), home), Path::new("/c"));
/// ```
pub fn dir_from(value: Option<std::ffi::OsString>, home: &Path) -> PathBuf {
    value
        .filter(|v| !v.is_empty())
        .map_or_else(|| dir(home), PathBuf::from)
}

/// The riff of the build in the clone `check`.
pub fn riff_of(check: &Path) -> PathBuf {
    check.join("target/static/release/riff")
}

/// Puts the clone `check` on the default branch of `origin`, and builds
/// its static riff with `just`. When `check` is not there, it clones the
/// `origin` of the repository of `here` into it. Gives the path of the
/// riff of the build.
pub fn build(here: &Path, check: &Path, just: &Path) -> Result<PathBuf> {
    if !check.exists() {
        let remote = output("git", here, &["remote", "get-url", "origin"])?;
        let parent = check.parent().context("the check clone has no parent")?;
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot make {}", parent.display()))?;
        let path = check.to_string_lossy();
        output("git", parent, &["clone", "-q", "--", &remote, &path])?;
    }
    output("git", check, &["fetch", "-q", "origin"])?;
    output("git", check, &["checkout", "-q", "--detach", "origin/HEAD"])?;
    let status = std::process::Command::new(just)
        .arg("static")
        .current_dir(check)
        .stdin(Stdio::null())
        .status()
        .with_context(|| text::no_program(&just.to_string_lossy()))?;
    if !status.success() {
        bail!("just static in {}: {status}", check.display());
    }
    Ok(riff_of(check))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_riff_of_a_check_is_in_its_static_release_folder() {
        let check = dir(Path::new("/h"));
        assert_eq!(
            riff_of(&check),
            Path::new("/h/.cache/riff/check/target/static/release/riff")
        );
    }

    #[test]
    fn a_build_with_no_origin_fails_and_makes_no_clone() {
        let here = isolated::outside_git();
        let home = isolated::outside_git();
        let check = dir(home.path());
        let err = build(here.path(), &check, Path::new("just")).unwrap_err();
        assert!(
            format!("{err:#}").contains("git remote get-url origin"),
            "{err:#}"
        );
        assert!(!check.exists());
    }
}
