//! The git of riff on the host, and its one guard (#2138, #2140,
//! 01M7DZ429QFWTSEA6JH1DVZJM0, 01M7E7ACTYP4RX8TCJ9XSYMMPM).
//!
//! # Design
//!
//! A session writes its clone, and so the `.git/config` of its clone. A
//! git config can name a program that git runs: `core.fsmonitor` for
//! `git status`, `credential.helper` for a fetch, `core.sshCommand` for a
//! fetch over SSH, and a hook for a commit. In the container that program
//! runs in the container. On the host it runs as the person, out of each
//! container. So the host runs no git in a folder that a session wrote.
//! The lead is a session too: it writes its clone in the folder of the
//! leads.
//!
//! - Each git call of riff goes through [`guard`]: [`git`], [`output`]
//!   and [`within`] call it. No other code of riff starts `git`, and a
//!   test of this module reads the code of riff to check it.
//! - [`guard`] refuses a folder under a folder of [`guarded`]: the folder
//!   of the sessions, `sessions` in the data folder of riff
//!   ([`crate::run::sessions_dir`]), the folder of the leads, `leads`
//!   there ([`crate::lead::LEADS`]), and each folder that a daemon gave
//!   with [`guard_also`]. A link counts as the folder that it names
//!   ([`real`]). The error names the folder ([`text::host_git_refused`]).
//! - A session writes more than its clone: each bind with write and each
//!   volume of `riff.toml`. A `riff.toml` can name each volume of podman.
//!   So the daemon gives the guard the source of each bind with write
//!   before the start of each container, and the folders of the volumes
//!   of podman at its start, before the claim of an item of a wave with a
//!   volume, and before the start of each container with a volume
//!   (`Daemon::guard_writes`, [`crate::runtime::Runtime::volume_folders`],
//!   01M7EAZ9JS8K3TBWG8ZXKV00J9). When podman cannot say them, no
//!   container with a volume starts, and the wave claims no item with a
//!   volume (`Daemon::guard_volumes`, 01M7EW2106JGGTSZKMRG4N9SH0,
//!   01M7EZCTKYFPS4QDQ1N3K3BT23).
//! - The daemon takes no folder for git from a file that a session
//!   wrote: the main clone of a lead comes from the records of the daemon
//!   ([`crate::lead::main_of`], 01M7EAZ9N9DY7JPXZP1V8JY01E).
//! - A container session sees its clone at the path of the host, under
//!   the folder of the sessions or of the leads. Its git runs in the
//!   container. So in a container session (`RIFF_SOCKET`,
//!   [`crate::cycle::in_container`]) these two folders are not in
//!   [`guarded`].
//!
//! ```mermaid
//! flowchart LR
//!     C["a git call of riff in DIR"] --> G{"DIR, with each link followed,<br/>under a guarded folder?"}
//!     G -- yes --> R["refused: the error names DIR"]
//!     G -- no --> X["git -C DIR runs"]
//! ```
//!
//! The steps of the host that need git for a session use other folders:
//! the clone of a session is made in a folder of its own, then moved
//! ([`crate::run::new_clone_of`], 01M7DZ42C8SY2GV5ZQJZHM2VTV). The sweep
//! looks into no clone ([`crate::daemon::sweep`],
//! 01M7DZ4E02PX5Y0WMHFCJ9KS14). The look for earlier work and the council
//! of a verify read the main clone ([`crate::council::pick::of_pull`],
//! 01M7DZ4E5BZAD38E3K9A3N277E). The new clone of a lead is made in a
//! folder of its own, then takes the place of the old clone
//! ([`crate::lead::fresh_clone`], 01M7E7A8QM8SRPQN3G1BXVJW1R).
//!
//! # The git that the host needs
//!
//! A person can still fetch the work of a session from its clone: see
//! "Get back the work of a clone" in the book. That `git fetch` runs
//! `git upload-pack` in the clone. Only a git with the fixes of May 2024
//! (CVE-2024-32004) runs no program there that the config of the clone
//! names: the first release of each line in [`FIXED`], or a later one.
//! So `riff init` and each start of the riff daemon check the version
//! of the git of the host ([`version`], [`Version::fixed`],
//! [`crate::first_run::machine_checks`], 01M7E6A47JB516A5FF30DNE47X).
//!
//! ```mermaid
//! flowchart LR
//!     V["git --version"] --> L{"the release of FIXED<br/>of its line, or later?"}
//!     L -- yes --> P["pass"]
//!     L -- "no, or no version" --> F["riff init: fail, with the fix<br/>the daemon: needs you"]
//! ```
//!
//! ```
//! use riff::host_git::refused;
//!
//! let root = tempfile::tempdir()?;
//! let sessions = root.path().join("sessions");
//! let clone = sessions.join("issue-12-1a2b3c4d/clone");
//! std::fs::create_dir_all(&clone)?;
//! let main = root.path().join("main");
//! std::fs::create_dir_all(&main)?;
//! let guarded = [sessions.clone()];
//! assert_eq!(refused(&guarded, &clone), Some(sessions.canonicalize()?));
//! assert_eq!(refused(&guarded, &sessions), Some(sessions.canonicalize()?));
//! assert_eq!(refused(&guarded, &main), None);
//! // A link to a session folder is the session folder.
//! std::os::unix::fs::symlink(&clone, root.path().join("link"))?;
//! assert!(refused(&guarded, &root.path().join("link")).is_some());
//! // A folder that is not there yet counts by its path.
//! assert!(refused(&guarded, &sessions.join("issue-13-2b3c4d5e/clone")).is_some());
//! # Ok::<(), std::io::Error>(())
//! ```

use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::text;

/// The folders of [`guard_also`].
static ALSO: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Adds `folder` to the folders where the host runs no git: the folder
/// of the sessions of a daemon, and each folder that a session can write
/// by a bind or a volume (01M7EAZ9JS8K3TBWG8ZXKV00J9). It stays for the
/// life of the process. A folder under a folder of [`guarded`] adds
/// nothing, so the clone of each new session adds nothing.
///
/// ```
/// let root = tempfile::tempdir()?;
/// let volume = root.path().join("volumes/riff-cargo/_data");
/// riff::host_git::guard_also(&root.path().join("volumes"));
/// riff::host_git::guard_also(&volume);
/// let held = |f: &std::path::Path| riff::host_git::guarded().iter().filter(|g| g.as_path() == f).count();
/// assert_eq!(held(&root.path().join("volumes")), 1);
/// assert_eq!(held(&volume), 0);
/// assert!(riff::host_git::guard(&volume).is_err());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn guard_also(folder: &Path) {
    if refused(&guarded(), folder).is_some() {
        return;
    }
    let mut also = ALSO.lock().unwrap_or_else(PoisonError::into_inner);
    if !also.iter().any(|f| f == folder) {
        also.push(folder.to_owned());
    }
}

/// The folders where the host runs no git: [`guarded_in`] of the data
/// folder of riff when this process is no container session, and each
/// folder of [`guard_also`].
pub fn guarded() -> Vec<PathBuf> {
    let data = (!crate::cycle::in_container())
        .then(crate::run::data_dir)
        .and_then(Result::ok);
    let mut folders = data.as_deref().map(guarded_in).unwrap_or_default();
    folders.extend(
        ALSO.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned(),
    );
    folders
}

/// The folders of the data folder `data` of riff where a session writes:
/// the folder of the sessions and the folder of the leads.
///
/// ```
/// use std::path::Path;
///
/// assert_eq!(
///     riff::host_git::guarded_in(Path::new("/d/riff")),
///     [Path::new("/d/riff/sessions"), Path::new("/d/riff/leads")]
/// );
/// ```
pub fn guarded_in(data: &Path) -> Vec<PathBuf> {
    vec![data.join("sessions"), data.join(crate::lead::LEADS)]
}

/// The path of `path` with each link followed: the real path of the
/// longest part that is there, then the rest. A path that is not
/// absolute starts at the folder of this process.
///
/// ```
/// let root = tempfile::tempdir()?;
/// let real = root.path().canonicalize()?;
/// std::os::unix::fs::symlink(&real, root.path().join("link"))?;
/// assert_eq!(riff::host_git::real(&root.path().join("link/new/x")), real.join("new/x"));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn real(path: &Path) -> PathBuf {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    let mut rest = Vec::new();
    let mut at = path.as_path();
    loop {
        if let Ok(found) = at.canonicalize() {
            return rest.iter().rev().fold(found, |path, part| path.join(part));
        }
        match (at.parent(), at.components().next_back()) {
            (Some(parent), Some(Component::Normal(part))) => {
                rest.push(part.to_owned());
                at = parent;
            }
            _ => return path,
        }
    }
}

/// The folder of `guarded` that holds `dir`, as [`real`] gives both, or
/// `None` when no folder holds it.
pub fn refused(guarded: &[PathBuf], dir: &Path) -> Option<PathBuf> {
    let dir = real(dir);
    guarded
        .iter()
        .map(|folder| real(folder))
        .find(|folder| dir.starts_with(folder))
}

/// The guard of each git call of riff on the host: an error that names
/// `dir` when it is under a folder of [`guarded`].
///
/// ```
/// let root = tempfile::tempdir()?;
/// let sessions = root.path().join("daemon-sessions");
/// riff::host_git::guard_also(&sessions);
/// let clone = sessions.join("issue-12-1a2b3c4d/clone");
/// let error = riff::host_git::guard(&clone).unwrap_err().to_string();
/// assert!(error.contains(&clone.display().to_string()), "{error}");
/// assert!(riff::host_git::guard(root.path()).is_ok());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn guard(dir: &Path) -> Result<()> {
    match refused(&guarded(), dir) {
        Some(folder) => bail!("{}", text::host_git_refused(dir, &folder)),
        None => Ok(()),
    }
}

/// `git -C DIR`, after the [`guard`] of `dir`.
pub fn git(dir: &Path) -> Result<Command> {
    guard(dir)?;
    let mut git = Command::new("git");
    git.arg("-C").arg(dir);
    Ok(git)
}

/// Runs `git ARGS` in `dir` after the [`guard`], and gives its stdout
/// with no space at the ends. An error names the command and its
/// stderr.
pub fn output(dir: &Path, args: &[&str]) -> Result<String> {
    let out = git(dir)?
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| text::no_program("git"))?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// [`output`] with no prompt for a credential and the time limit `limit`
/// (`run::output_within`).
pub fn within(dir: &Path, args: &[&str], limit: Duration) -> Result<String> {
    guard(dir)?;
    let dir_text = dir.to_string_lossy();
    let program = ["GIT_TERMINAL_PROMPT=0", "git", "-C", &dir_text];
    let all = [&program[..], args].concat();
    crate::run::output_within("env", dir, &all, limit)
}

/// The name of the check of the git of the host in the checks of the
/// machine ([`crate::first_run::machine_checks`]).
pub const CHECK: &str = "git";

/// A version of git: the major, the minor and the patch number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32, pub u32, pub u32);

/// The first release with the fixes of May 2024 (CVE-2024-32004) in each
/// line of git that got them, oldest first. Each line after the last one
/// has them from its start.
pub const FIXED: [Version; 7] = [
    Version(2, 39, 4),
    Version(2, 40, 2),
    Version(2, 41, 1),
    Version(2, 42, 2),
    Version(2, 43, 4),
    Version(2, 44, 1),
    Version(2, 45, 1),
];

/// The number at the start of `part`, for example 0 of `0-rc1`.
fn number(part: &str) -> Option<u32> {
    let end = part
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(part.len());
    part[..end].parse().ok()
}

impl Version {
    /// The version in the first line `line` of `git --version`: the
    /// numbers after `git version`. A version with no patch number has
    /// the patch 0. What comes after the patch number does not count, for
    /// example `.windows.1`, `.rc1` or ` (Apple Git-154)`.
    ///
    /// ```
    /// use riff::host_git::Version;
    ///
    /// assert_eq!(Version::parse("git version 2.43.0"), Some(Version(2, 43, 0)));
    /// assert_eq!(Version::parse("git version 2.39.5 (Apple Git-154)"), Some(Version(2, 39, 5)));
    /// assert_eq!(Version::parse("git version 2.45.1.windows.1"), Some(Version(2, 45, 1)));
    /// assert_eq!(Version::parse("git version 2.51.0.rc1"), Some(Version(2, 51, 0)));
    /// assert_eq!(Version::parse("git version 3.0"), Some(Version(3, 0, 0)));
    /// assert_eq!(Version::parse("git version unknown"), None);
    /// assert_eq!(Version::parse("podman version 5.7.0"), None);
    /// ```
    pub fn parse(line: &str) -> Option<Version> {
        let word = line
            .trim()
            .strip_prefix("git version ")?
            .split_whitespace()
            .next()?;
        let mut parts = word.split('.');
        let major = number(parts.next()?)?;
        let minor = number(parts.next()?)?;
        let patch = parts.next().map_or(Some(0), number)?;
        Some(Version(major, minor, patch))
    }

    /// True when this git has the fixes of May 2024: it is the release of
    /// [`FIXED`] of its line or a later one, or its line comes after each
    /// line of [`FIXED`].
    ///
    /// ```
    /// use riff::host_git::Version;
    ///
    /// // An old git.
    /// assert!(!Version(2, 34, 1).fixed());
    /// assert!(!Version(2, 39, 3).fixed());
    /// assert!(!Version(2, 43, 0).fixed());
    /// assert!(!Version(2, 45, 0).fixed());
    /// // The first fixed release of its line.
    /// assert!(Version(2, 39, 4).fixed());
    /// assert!(Version(2, 43, 4).fixed());
    /// assert!(Version(2, 45, 1).fixed());
    /// // A newer git.
    /// assert!(Version(2, 39, 5).fixed());
    /// assert!(Version(2, 46, 0).fixed());
    /// assert!(Version(3, 0, 0).fixed());
    /// ```
    pub fn fixed(self) -> bool {
        let Version(major, minor, _) = self;
        let last = FIXED[FIXED.len() - 1];
        match FIXED
            .iter()
            .find(|first| (first.0, first.1) == (major, minor))
        {
            Some(first) => self >= *first,
            None => (major, minor) > (last.0, last.1),
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// The first line of `git --version` of the host, or why riff cannot run
/// it (01M7E6A47JB516A5FF30DNE47X). It runs in `/`, so the config of no
/// repository counts.
pub fn version() -> Result<String, String> {
    let out = Command::new("git")
        .arg("--version")
        .current_dir("/")
        .stdin(Stdio::null())
        .output()
        .map_err(|_| text::INIT_NO_GIT.to_owned())?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first = stdout.lines().next().unwrap_or_default().trim();
    if !out.status.success() || first.is_empty() {
        return Err(text::init_git_failed(
            String::from_utf8_lossy(&out.stderr).trim(),
        ));
    }
    Ok(first.to_owned())
}

/// Makes a clone at `clone`, with the origin `o/r` on GitHub and one
/// commit that `origin/main` has, as a session can leave it (#2138): its
/// git config names a program for `core.fsmonitor` (`git status`),
/// `credential.helper` (a fetch or a push over HTTPS) and
/// `core.sshCommand` (a fetch or a push over SSH), and as the hook
/// `pre-commit`. The program writes the file `mark`. No step of the host
/// may make that file.
#[cfg(test)]
pub(crate) fn hostile_clone(clone: &Path, mark: &Path) {
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(clone)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "riff test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "riff test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    std::fs::create_dir_all(mark.parent().unwrap()).unwrap();
    std::fs::create_dir_all(clone).unwrap();
    git(&["init", "-q", "-b", "main"]);
    git(&["commit", "-q", "--allow-empty", "-m", "start"]);
    git(&["remote", "add", "origin", "https://github.com/o/r.git"]);
    git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    let program = isolated::write_program(
        mark.with_extension("sh"),
        format!("#!/bin/sh\necho \"$0 $*\" >> '{}'\n", mark.display()),
    );
    let program = program.display();
    let config = clone.join(".git/config");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str(&format!(
        "[core]\n\tfsmonitor = {program}\n\tsshCommand = {program}\n\
         [credential]\n\thelper = !{program}\n"
    ));
    std::fs::write(&config, text).unwrap();
    let hook = clone.join(".git/hooks/pre-commit");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    isolated::copy_program(mark.with_extension("sh"), &hook);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The clone of [`hostile_clone`] runs its program at a `git status`
    /// on the host: the tests that look for no mark look at a real one.
    #[test]
    fn the_program_of_a_hostile_clone_runs_at_a_git_status() {
        let root = tempfile::tempdir().unwrap();
        let (clone, mark) = (root.path().join("clone"), root.path().join("mark"));
        hostile_clone(&clone, &mark);
        assert!(!mark.exists());
        let status = Command::new("git")
            .arg("-C")
            .arg(&clone)
            .args(["status", "--porcelain"])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(status.status.success(), "{status:?}");
        assert!(mark.exists(), "the fsmonitor of the clone did not run");
    }

    /// The production code of `file`, with its line numbers: no comment
    /// line, and no item of `#[cfg(test)]`. Such an item ends at its own
    /// `;`, or at the `}` with the indent of the attribute.
    pub(crate) fn production(file: &str) -> Vec<(usize, &str)> {
        let mut lines = Vec::new();
        let mut test: Option<String> = None;
        let mut skip: Option<String> = None;
        for (n, line) in file.lines().enumerate() {
            if let Some(end) = &skip {
                if line == end {
                    skip = None;
                }
                continue;
            }
            if let Some(indent) = test.take() {
                if !line.trim_end().ends_with(';') {
                    skip = Some(format!("{indent}}}"));
                }
                continue;
            }
            let trimmed = line.trim_start();
            if trimmed == "#[cfg(test)]" {
                test = Some(line[..line.len() - trimmed.len()].to_owned());
                continue;
            }
            if !trimmed.starts_with("//") {
                lines.push((n + 1, line));
            }
        }
        lines
    }

    /// Each file `.rs` of `dir` and of its folders.
    pub(crate) fn sources(dir: &Path, all: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                sources(&path, all);
            } else if path.extension().is_some_and(|e| e == "rs") {
                all.push(path);
            }
        }
    }

    /// 01M7DZ429QFWTSEA6JH1DVZJM0: no code of riff but this module starts
    /// `git`, so each git call of riff goes through the guard.
    #[test]
    fn each_git_call_of_riff_goes_through_the_guard() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut all = Vec::new();
        sources(&src, &mut all);
        let mut found = Vec::new();
        for path in all.iter().filter(|p| !p.ends_with("host_git.rs")) {
            let file = std::fs::read_to_string(path).unwrap();
            for (n, line) in production(&file) {
                if line.contains("\"git\"") {
                    found.push(format!("{}:{n}: {}", path.display(), line.trim()));
                }
            }
        }
        assert!(found.is_empty(), "git with no guard:\n{}", found.join("\n"));
        assert!(all.len() > 100, "the code of riff: {}", all.len());
    }

    #[test]
    fn the_scan_skips_comments_and_the_items_of_tests() {
        let file = "fn a() {}\n/// Command::new(\"git\")\n#[cfg(test)]\nfn t() {\n    \"git\"\n}\n\
                    fn b() { \"git\" }\n#[cfg(test)]\nuse c::d;\nfn e() {}\n";
        let lines: Vec<usize> = production(file).iter().map(|(n, _)| *n).collect();
        assert_eq!(lines, [1, 7, 10]);
    }

    #[test]
    fn the_guard_refuses_a_git_call_in_a_folder_of_a_daemon_and_names_it() {
        let root = tempfile::tempdir().unwrap();
        let sessions = root.path().join("sessions");
        let clone = sessions.join("issue-12-1a2b3c4d/clone");
        let mark = root.path().join("mark");
        hostile_clone(&clone, &mark);
        guard_also(&sessions);
        for error in [
            output(&clone, &["status"]).unwrap_err(),
            within(&clone, &["status"], Duration::from_secs(10)).unwrap_err(),
            git(&clone).map(|_| ()).unwrap_err(),
        ] {
            let error = error.to_string();
            assert!(
                error.contains(&clone.display().to_string()) && error.contains("no git"),
                "{error}"
            );
        }
        assert!(!mark.exists(), "the program of the config ran");
    }

    /// 01M7E7ACTYP4RX8TCJ9XSYMMPM: the clone of a lead is in the folder of
    /// the leads, where the guard refuses git, and the program of its git
    /// config does not run. The new clone of a lead is in no such folder.
    #[test]
    fn the_guard_refuses_a_git_call_in_the_clone_of_a_lead() {
        let root = tempfile::tempdir().unwrap();
        let leads = root.path().join(crate::lead::LEADS);
        let clone = leads.join("o-r/clone");
        let mark = root.path().join("mark");
        hostile_clone(&clone, &mark);
        let guarded = guarded_in(root.path());
        assert_eq!(refused(&guarded, &clone), Some(real(&leads)));
        let new = crate::lead::new_clone_of(&clone).unwrap();
        assert_eq!(refused(&guarded, &new), None);

        guard_also(&leads);
        let error = output(&clone, &["status"]).unwrap_err().to_string();
        assert_eq!(error, text::host_git_refused(&clone, &real(&leads)));
        assert!(!mark.exists(), "the program of the config ran");
    }
}
