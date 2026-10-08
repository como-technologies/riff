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
//!     A[riff workers start / the clear of a worker] --> B{an origin?}
//!     B -- no --> Z[no remote: change nothing, say nothing]
//!     B -- yes --> L[git ls-remote --symref origin HEAD: the default branch]
//!     L --> C{on the default branch, no local changes?}
//!     C -- no --> K[change nothing, say why]
//!     C -- yes --> D[git fetch --prune origin]
//!     D --> E{local commits that origin does not have?}
//!     E -- yes --> K
//!     E -- no --> F[git merge --ff-only refs/remotes/origin/BRANCH]
//! ```
//!
//! When the main clone is not on the default branch, has local changes
//! to tracked files, or has commits that `origin` does not have, riff
//! changes nothing and says why (01M3MNP36TZYN3PE00AZJTJSER). The
//! person who runs `riff workers start` reads it. The clear of a
//! worker tells the lead. A step that fails never stops the command or
//! the clear.
//!
//! A session writes the refs of the clone, also
//! `refs/remotes/origin/HEAD`. So riff takes the default branch from
//! `origin` itself ([`default_branch`]), and names each ref in full,
//! for example `refs/remotes/origin/main` (01M4DVXP20SHYTFE1D4NVF0FSF).
//! A tag or a branch `origin/main` of a session does not change what
//! riff moves. riff reads other refs only in a sandbox: in the start
//! hook and `leave` of a session, and in the rollout of the lead.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

/// What [`fast_forward`] did to the main clone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fresh {
    /// The dir is not the top of a git worktree, or the clone has no
    /// `origin`, so riff did nothing.
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

/// The time that riff waits for `origin` to name its default branch.
pub const REMOTE_WAIT: Duration = Duration::from_secs(10);

/// The default branch of `origin` in the clone of `dir`, from `origin`
/// itself: `git ls-remote --symref origin HEAD` (01M4DVXP20SHYTFE1D4NVF0FSF).
/// riff never takes it from `refs/remotes/origin/HEAD`: a session writes
/// the refs of the clone. A session writes no config, so the URL of
/// `origin` is a fact of riff. The name passes [`branch_name`]. It fails
/// when `origin` gives no answer in `wait`.
///
/// ```
/// let dir = isolated::outside_git();
/// let wait = std::time::Duration::from_secs(5);
/// assert!(riff::hygiene::default_branch(dir.path(), wait).is_err());
/// ```
pub fn default_branch(dir: &Path, wait: Duration) -> Result<String, String> {
    let args = ["ls-remote", "--symref", "--", "origin", "HEAD"];
    let out = git_for(dir, &args, wait)?;
    let branch = remote_head(&out).ok_or("origin names no default branch")?;
    branch_name(branch).map(str::to_owned)
}

/// The branch that `HEAD` names in the text of `git ls-remote --symref`.
///
/// ```
/// let text = "ref: refs/heads/trunk\tHEAD\n8c67\tHEAD\n";
/// assert_eq!(riff::hygiene::remote_head(text), Some("trunk"));
/// assert_eq!(riff::hygiene::remote_head("8c67\tHEAD\n"), None);
/// assert_eq!(riff::hygiene::remote_head("ref: refs/tags/v1\tHEAD\n"), None);
/// ```
pub fn remote_head(text: &str) -> Option<&str> {
    text.lines().find_map(|line| {
        line.strip_prefix("ref: refs/heads/")?
            .strip_suffix("\tHEAD")
    })
}

/// `name` when riff takes it as the name of a branch, else why not
/// (01M4DVXP20SHYTFE1D4NVF0FSF). riff refuses a name that git can read
/// as an option (a start with `-`), a name that git refuses, and the
/// name `HEAD`.
///
/// ```
/// use riff::hygiene::branch_name;
///
/// assert_eq!(branch_name("worktree-issue-12"), Ok("worktree-issue-12"));
/// assert_eq!(branch_name("feature/x.y"), Ok("feature/x.y"));
/// for bad in ["", "-f", "--upload-pack=x", ".x", "a..b", "a b", "a~1", "a^", "a:b", "a?", "a*",
///     "a[", "a\\b", "a/", "/a", "a//b", "a.lock", "a@{1}", "@", "HEAD", "a\u{7f}", "a/.b"] {
///     assert!(branch_name(bad).is_err(), "{bad:?}");
/// }
/// ```
pub fn branch_name(name: &str) -> Result<&str, String> {
    let bad = name.is_empty()
        || name.starts_with(['-', '.', '/'])
        || name.ends_with(['/', '.'])
        || name.ends_with(".lock")
        || name.contains("..")
        || name.contains("//")
        || name.contains("/.")
        || name.contains("@{")
        || name == "@"
        || name == "HEAD"
        || name
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c));
    if bad {
        Err(format!(
            "riff takes no branch with the name {:?}",
            crate::text::safe(name)
        ))
    } else {
        Ok(name)
    }
}

/// Fast-forwards the default branch of the main clone of `dir` to
/// `origin` (01M3MNP34M5PAZW9VWAYVGNSV2). It changes nothing when the
/// main clone is not on the default branch, has local changes to
/// tracked files, or has commits that `origin` does not have
/// (01M3MNP36TZYN3PE00AZJTJSER). It changes nothing in a repository
/// above `dir`: `dir` must be the top of a git worktree
/// (01M49JW9Y8SNT3J242SF646DF4).
///
/// The default branch comes from `origin` ([`default_branch`]), and
/// each ref has its full name, for example `refs/remotes/origin/main`:
/// a tag or a symbolic ref that a session wrote does not move the clone
/// (01M4DVXP20SHYTFE1D4NVF0FSF).
///
/// ```
/// use riff::hygiene::{Fresh, fast_forward};
///
/// let dir = isolated::outside_git();
/// assert_eq!(fast_forward(dir.path()), Fresh::NoRemote);
/// std::process::Command::new("git").arg("init").arg("-q").arg(dir.path()).status()?;
/// let sub = dir.path().join("sub");
/// std::fs::create_dir(&sub)?;
/// assert_eq!(fast_forward(&sub), Fresh::NoRemote);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn fast_forward(dir: &Path) -> Fresh {
    if !crate::identity::is_top(dir) {
        return Fresh::NoRemote;
    }
    let Some(main) = crate::identity::main_worktree(dir) else {
        return Fresh::NoRemote;
    };
    if git(&main, &["remote", "get-url", "--", "origin"]).is_err() {
        return Fresh::NoRemote;
    }
    let kept = |why: String| Fresh::Kept {
        main: main.clone(),
        why,
    };
    let branch = match default_branch(&main, REMOTE_WAIT) {
        Ok(branch) => branch,
        Err(e) => return kept(e),
    };
    let local = format!("refs/heads/{branch}");
    let remote = format!("refs/remotes/origin/{branch}");
    match git(&main, &["symbolic-ref", "-q", "HEAD"]) {
        Ok(on) if on == local => {}
        Ok(on) => {
            let on = on.strip_prefix("refs/heads/").unwrap_or(&on);
            return kept(format!("it is on the branch {on}, not on {branch}"));
        }
        Err(_) => return kept(format!("it has no branch checked out, not {branch}")),
    }
    match git(&main, &["status", "--porcelain", "--untracked-files=no"]) {
        Ok(changes) if changes.is_empty() => {}
        Ok(_) => return kept("it has local changes".into()),
        Err(e) => return kept(e),
    }
    if let Err(e) = git(&main, &["fetch", "--quiet", "--prune", "--", "origin"]) {
        return kept(e);
    }
    let commit = format!("{remote}^{{commit}}");
    let verify = ["rev-parse", "--verify", "--quiet", "--end-of-options"];
    if git(&main, &[&verify[..], &[commit.as_str()]].concat()).is_err() {
        return kept(format!("origin has no branch {branch}"));
    }
    let count = |range: &str| {
        git(&main, &["rev-list", "--count", "--end-of-options", range])
            .and_then(|n| n.parse::<u64>().map_err(|e| e.to_string()))
    };
    match count(&format!("{remote}..{local}")) {
        Ok(0) => {}
        Ok(n) => {
            let s = if n == 1 { "" } else { "s" };
            return kept(format!(
                "it has {n} commit{s} that origin/{branch} does not have"
            ));
        }
        Err(e) => return kept(e),
    }
    let commits = match count(&format!("{local}..{remote}")) {
        Ok(0) => return Fresh::Current { main, branch },
        Ok(n) => n,
        Err(e) => return kept(e),
    };
    match git(&main, &["merge", "--quiet", "--ff-only", "--", &remote]) {
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
    git_for(dir, args, Duration::MAX)
}

/// [`git`] for at most `wait`. riff stops git after `wait`.
fn git_for(dir: &Path, args: &[&str], wait: Duration) -> Result<String, String> {
    let mut child = crate::confine::git_in(dir)
        .map_err(|e| format!("{e:#}"))?
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run git: {e}"))?;
    // Read the pipes while git runs: a full pipe stops git.
    fn read(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut text = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut text);
            }
            text
        })
    }
    let stdout = read(child.stdout.take());
    let stderr = read(child.stderr.take());
    let end = Instant::now().checked_add(wait);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if end.is_some_and(|end| Instant::now() >= end) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "git {} gave no answer in {} seconds",
                    args.join(" "),
                    wait.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("cannot run git: {e}")),
        }
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    if status.success() {
        Ok(String::from_utf8_lossy(&stdout).trim().to_owned())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&stderr).trim()
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
