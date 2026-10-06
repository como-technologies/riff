//! Git hygiene of the main clone for workers.
//!
//! # Design
//!
//! Workers start in the main clone, and a new worktree branches from
//! it. A main clone that is behind `origin` gives each worker old
//! project files and an old base. So `riff workers start`, and the
//! clear of a worker before the fresh context ([`crate::next`]),
//! fast-forward the main clone to `origin` first
//! (01M3MNP34M5PAZW9VWAYVGNSV2).
//!
//! ```mermaid
//! flowchart TD
//!     A[riff workers start / the clear of a worker] --> B{origin/HEAD?}
//!     B -- no --> Z[no remote: change nothing, say nothing]
//!     B -- yes --> C{on the default branch, no local changes?}
//!     C -- no --> K[change nothing, say why]
//!     C -- yes --> D[git fetch --prune origin]
//!     D --> E{local commits that origin does not have?}
//!     E -- yes --> K
//!     E -- no --> F[git merge --ff-only origin/BRANCH]
//! ```
//!
//! When the main clone is not on the default branch, has local changes
//! to tracked files, or has commits that `origin` does not have, riff
//! changes nothing and says why (01M3MNP36TZYN3PE00AZJTJSER). The
//! person who runs `riff workers start` reads it. The clear of a
//! worker tells the lead. A step that fails never stops the command or
//! the clear.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What [`fast_forward`] did to the main clone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fresh {
    /// The clone has no `origin/HEAD`, so riff did nothing.
    NoRemote,
    /// The default branch was at `origin` already.
    Current {
        /// The main worktree.
        main: PathBuf,
        /// The default branch.
        branch: String,
    },
    /// riff moved the default branch forward to `origin`.
    Forwarded {
        /// The main worktree.
        main: PathBuf,
        /// The default branch.
        branch: String,
        /// How many commits it moved.
        commits: u64,
    },
    /// riff changed nothing, for the reason.
    Kept {
        /// The main worktree.
        main: PathBuf,
        /// Why riff changed nothing.
        why: String,
    },
}

impl Fresh {
    /// The line for the person or the worker. `None` for
    /// [`Fresh::NoRemote`].
    ///
    /// ```
    /// use riff::hygiene::Fresh;
    ///
    /// let moved = Fresh::Forwarded { main: "/src/riff".into(), branch: "main".into(), commits: 2 };
    /// assert_eq!(
    ///     moved.line().unwrap(),
    ///     "riff: the main clone /src/riff moved 2 commits forward to origin/main."
    /// );
    /// let kept = Fresh::Kept { main: "/src/riff".into(), why: "it has local changes".into() };
    /// assert_eq!(
    ///     kept.line().unwrap(),
    ///     "riff: the main clone /src/riff stays as it is: it has local changes. \
    ///      New worktrees can start from an old base."
    /// );
    /// assert_eq!(Fresh::NoRemote.line(), None);
    /// ```
    pub fn line(&self) -> Option<String> {
        match self {
            Fresh::NoRemote => None,
            Fresh::Current { main, branch } => Some(format!(
                "riff: the main clone {} is at origin/{branch}.",
                main.display()
            )),
            Fresh::Forwarded {
                main,
                branch,
                commits,
            } => {
                let s = if *commits == 1 { "" } else { "s" };
                Some(format!(
                    "riff: the main clone {} moved {commits} commit{s} forward to origin/{branch}.",
                    main.display()
                ))
            }
            Fresh::Kept { main, why } => Some(format!(
                "riff: the main clone {} stays as it is: {why}. New worktrees can start from an \
                 old base.",
                main.display()
            )),
        }
    }

    /// True when the lead must know: riff changed nothing, for a reason.
    ///
    /// ```
    /// use riff::hygiene::Fresh;
    ///
    /// assert!(Fresh::Kept { main: "/r".into(), why: "x".into() }.tells_the_lead());
    /// assert!(!Fresh::NoRemote.tells_the_lead());
    /// ```
    pub fn tells_the_lead(&self) -> bool {
        matches!(self, Fresh::Kept { .. })
    }
}

/// Fast-forwards the default branch of the main clone of `dir` to
/// `origin` (01M3MNP34M5PAZW9VWAYVGNSV2). It changes nothing when the
/// main clone is not on the default branch, has local changes to
/// tracked files, or has commits that `origin` does not have
/// (01M3MNP36TZYN3PE00AZJTJSER).
///
/// ```
/// let dir = isolated::outside_git();
/// assert_eq!(riff::hygiene::fast_forward(dir.path()), riff::hygiene::Fresh::NoRemote);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn fast_forward(dir: &Path) -> Fresh {
    let Some(main) = crate::identity::main_worktree(dir) else {
        return Fresh::NoRemote;
    };
    let Ok(head) = git(
        &main,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    ) else {
        return Fresh::NoRemote;
    };
    let Some(branch) = head.strip_prefix("origin/").map(str::to_owned) else {
        return Fresh::NoRemote;
    };
    let kept = |why: String| Fresh::Kept {
        main: main.clone(),
        why,
    };
    match git(&main, &["symbolic-ref", "--short", "-q", "HEAD"]) {
        Ok(on) if on == branch => {}
        Ok(on) => return kept(format!("it is on the branch {on}, not on {branch}")),
        Err(_) => return kept(format!("it has no branch checked out, not {branch}")),
    }
    match git(&main, &["status", "--porcelain", "--untracked-files=no"]) {
        Ok(changes) if changes.is_empty() => {}
        Ok(_) => return kept("it has local changes".into()),
        Err(e) => return kept(e),
    }
    if let Err(e) = git(&main, &["fetch", "--quiet", "--prune", "origin"]) {
        return kept(e);
    }
    let count = |range: &str| {
        git(&main, &["rev-list", "--count", range])
            .and_then(|n| n.parse::<u64>().map_err(|e| e.to_string()))
    };
    match count(&format!("{head}..HEAD")) {
        Ok(0) => {}
        Ok(n) => {
            let s = if n == 1 { "" } else { "s" };
            return kept(format!("it has {n} commit{s} that {head} does not have"));
        }
        Err(e) => return kept(e),
    }
    let commits = match count(&format!("HEAD..{head}")) {
        Ok(0) => return Fresh::Current { main, branch },
        Ok(n) => n,
        Err(e) => return kept(e),
    };
    match git(&main, &["merge", "--quiet", "--ff-only", &head]) {
        Ok(_) => Fresh::Forwarded {
            main,
            branch,
            commits,
        },
        Err(e) => kept(e),
    }
}

/// Runs git in `dir` with no prompt. The error names the command and
/// its message.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_commit_has_no_plural() {
        let moved = Fresh::Forwarded {
            main: "/r".into(),
            branch: "trunk".into(),
            commits: 1,
        };
        assert!(
            moved
                .line()
                .unwrap()
                .contains("moved 1 commit forward to origin/trunk.")
        );
    }

    #[test]
    fn only_a_kept_clone_tells_the_lead() {
        let current = Fresh::Current {
            main: "/r".into(),
            branch: "main".into(),
        };
        assert!(!current.tells_the_lead());
        assert!(current.line().unwrap().contains("is at origin/main"));
    }
}
