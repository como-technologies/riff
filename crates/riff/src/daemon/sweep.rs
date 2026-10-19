//! The folders of the sessions of `riff run` and of the waves (#938,
//! #973, 01M4Z81ARSTAK8HR96S084SDFD, 01M53NJ5V27BCHDNHK2THQQZZQ,
//! 01M53NJ5Y2CMY5MWF9328KW7DE).
//!
//! # Design
//!
//! Each session of `riff run` and of a wave has a folder of its own,
//! `ITEM-ID8` in [`crate::run::sessions_dir`]: the clone with its
//! `target`, often some GB. The daemon removes the folder of each session
//! that does not run when the work of its branch is over
//! ([`Daemon::sweep`]):
//!
//! - at its start, after it took up its sessions, and then each
//!   [`EVERY`], also when no wave runs ([`Daemon::sweep_every`]);
//! - at each look of a wave, for the folders of its repository
//!   ([`crate::wave`]);
//! - at once, for `riff daemon sweep` ([`Ask::Sweep`](super::Ask::Sweep)).
//!
//! The forge gives the fate of a folder ([`World::fate`], [`Fate::of`]):
//!
//! | The branch `worktree-ITEM` and the issue | The folder |
//! |---|---|
//! | An open pull request | stays |
//! | No open pull request, a merged one | goes |
//! | No open pull request, a closed one | goes |
//! | No pull request, and the issue is closed | goes |
//! | No pull request, and the issue is open | stays |
//!
//! The folder of a verify, `verify-issue-N-ID8`, has the fate of the
//! branch of its item. A merged pull request with `Refs #N` first gets
//! the comment `Merged in #PR (COMMIT)` on its issue ([`World::merged_in`]),
//! so the item is not free work again. riff never removes a folder whose
//! name has no issue, or whose clone has no `origin`.
//!
//! The fate looks only at the forge. So before riff removes a folder, it
//! looks at the clone too
//! ([`Local::of`], 01M53S7WAXD46YPN0DR96XSTMG). The folder stays when the
//! clone has a file that is not committed, or a commit that no branch of
//! `origin` has: a session that ended before it pushed its work, on a
//! branch with an older merged or closed pull request. The folder also
//! stays when git cannot read the clone. riff says why in
//! `riff daemon sweep`, and one time in the log of the daemon
//! ([`Daemon::sweep_news`]).
//!
//! A session runs, and its folder stays, while one of these is true:
//!
//! - the daemon has the record of the session;
//! - a job of the daemon runs for the item of the folder, from before
//!   the clone of its session to its last step after the end, which reads
//!   `events.jsonl` in the folder (`Daemon::in_job`,
//!   01M53S7WDPQ2GWHJWJ6SZXZ5PN). The daemon has no record of the
//!   session after its end;
//! - the `riff run` that made the folder runs. It holds the lock
//!   `ITEM-ID8.lock` beside the folder from before the clone to its own
//!   end ([`hold`]): its start can wait long for room, and the daemon has
//!   no record of the session in that time.
//!
//! ```mermaid
//! flowchart TD
//!     F["a folder ITEM-ID8"] --> R{"its session runs?"}
//!     R -- yes --> K["it stays"]
//!     R -- no --> O["the repository of the origin of its clone"]
//!     O --> T{"the fate on the forge"}
//!     T -- "an open pull request, or an open issue" --> K
//!     T -- "no answer of gh in GH_LIMIT" --> S["it stays, and riff says why"]
//!     T -- "merged, closed, or the issue closed" --> C{"its clone: a file not committed,<br/>or a commit on no branch of origin?"}
//!     C -- "yes, or git cannot say" --> S
//!     C -- no --> D["riff removes the folder and its lock, and says why"]
//! ```
//!
//! One sweep runs at a time. Each step of git and of `gh` can block, so
//! it runs on a thread for blocking work. Each wave sweeps before each
//! look, so a `gh` that never ends would stop each wave. Each `gh` call
//! of the sweep has the time limit [`GH_LIMIT`] ([`gh`],
//! 01M53S7WGM9DQV2DXFHBRSMTP8). At the end of the limit, riff stops `gh`,
//! keeps the folder and goes on to the next folder. A fault at one
//! folder stops no other folder: the daemon logs it and goes on.
//!
//! [`World::fate`]: crate::wave::World::fate
//! [`World::merged_in`]: crate::wave::World::merged_in

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use super::Daemon;
use crate::runtime::Runtime;
use crate::{identity, pr, rollout, run, text};

/// The time between two sweeps of the daemon.
pub const EVERY: Duration = Duration::from_secs(600);

/// The variable that gives the time between two sweeps in seconds, for
/// tests. 0 turns the sweeps of the daemon off: `riff daemon sweep`
/// still sweeps.
pub const EVERY_VAR: &str = "RIFF_SWEEP_EVERY";

/// The time between two sweeps: [`EVERY`], or the seconds of
/// [`EVERY_VAR`]. `None` when they are off.
pub fn every() -> Option<Duration> {
    match std::env::var(EVERY_VAR).ok().and_then(|s| s.parse().ok()) {
        Some(0) => None,
        Some(s) => Some(Duration::from_secs(s)),
        None => Some(EVERY),
    }
}

/// The time limit of each `gh` call of the sweep
/// (01M53S7WGM9DQV2DXFHBRSMTP8).
pub const GH_LIMIT: Duration = Duration::from_secs(30);

/// The `gh` of the sweep for the repository `repo`: each call has the
/// time limit [`GH_LIMIT`].
///
/// ```
/// use riff::daemon::sweep::{GH_LIMIT, gh};
///
/// assert_eq!(gh("o/r").limit(), Some(GH_LIMIT));
/// ```
pub fn gh(repo: &str) -> pr::Gh {
    pr::Gh::for_repo(repo).with_limit(GH_LIMIT)
}

/// The fate of the folder of a session that does not run, from the
/// forge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fate {
    /// The folder stays: its branch has an open pull request, or no pull
    /// request and an open issue.
    Stays,
    /// The pull request `pull` merged at `commit`. `refs` is true when
    /// its body has the line `Refs #N`.
    Merged {
        /// The number of the pull request.
        pull: u64,
        /// The merge commit.
        commit: String,
        /// The body has the line `Refs #N`.
        refs: bool,
    },
    /// The pull request `pull` closed with no merge.
    Closed {
        /// The number of the pull request.
        pull: u64,
    },
    /// The issue is closed, and its branch has no pull request.
    IssueClosed,
}

/// A pull request of a branch, as
/// `gh pr list --json number,state,body,mergeCommit` gives it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchPull {
    /// The number.
    pub number: u64,
    /// `OPEN`, `CLOSED` or `MERGED`.
    pub state: String,
    /// The body.
    #[serde(default)]
    pub body: String,
    /// The merge commit, when it merged.
    #[serde(default)]
    pub merge_commit: Option<Oid>,
}

/// A commit, as `gh` gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct Oid {
    /// The full hash.
    pub oid: String,
}

impl Fate {
    /// The fate of the folder of a session of the issue `issue`, from
    /// the pull requests of its branch, newest first. It asks `closed`
    /// whether the issue is closed only when the branch has no pull
    /// request.
    ///
    /// ```
    /// use riff::daemon::sweep::{BranchPull, Fate};
    ///
    /// let pulls = |json: &str| serde_json::from_str::<Vec<BranchPull>>(json).unwrap();
    /// let open = || Ok(false);
    /// let merged = pulls(r#"[{"number":40,"state":"MERGED","body":"Refs #12","mergeCommit":{"oid":"abc"}}]"#);
    /// let refs = Fate::Merged { pull: 40, commit: "abc".into(), refs: true };
    /// assert_eq!(Fate::of(12, &merged, open)?, refs);
    /// let closed = pulls(r#"[{"number":41,"state":"CLOSED","body":"Closes #12"}]"#);
    /// assert_eq!(Fate::of(12, &closed, open)?, Fate::Closed { pull: 41 });
    /// let again = pulls(r#"[{"number":42,"state":"OPEN"},{"number":41,"state":"CLOSED"}]"#);
    /// assert_eq!(Fate::of(12, &again, open)?, Fate::Stays, "an open pull request");
    /// assert_eq!(Fate::of(12, &[], open)?, Fate::Stays, "an open issue");
    /// assert_eq!(Fate::of(12, &[], || Ok(true))?, Fate::IssueClosed);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn of(
        issue: u64,
        pulls: &[BranchPull],
        closed: impl FnOnce() -> Result<bool>,
    ) -> Result<Fate> {
        let with = |state: &str| pulls.iter().find(|p| p.state == state);
        if with("OPEN").is_some() {
            return Ok(Fate::Stays);
        }
        if let Some(pull) = with("MERGED") {
            let refs = format!("Refs #{issue}");
            return Ok(Fate::Merged {
                pull: pull.number,
                commit: pull
                    .merge_commit
                    .as_ref()
                    .map(|c| c.oid.clone())
                    .unwrap_or_default(),
                refs: pull.body.lines().any(|l| l.trim() == refs),
            });
        }
        if let Some(pull) = with("CLOSED") {
            return Ok(Fate::Closed { pull: pull.number });
        }
        Ok(if closed()? {
            Fate::IssueClosed
        } else {
            Fate::Stays
        })
    }
}

/// The state of an issue, as `gh issue view --json state` gives it.
#[derive(Debug, Deserialize)]
struct IssueState {
    state: String,
}

/// The comments of an issue, as `gh issue view --json comments` gives
/// them.
#[derive(Debug, Deserialize)]
struct Comments {
    comments: Vec<rollout::Comment>,
}

/// The fate of the folder of a session of the issue `issue` with `gh`
/// ([`Fate::of`]): the pull requests of its branch, and the state of the
/// issue when the branch has none.
pub fn fate_on(gh: &pr::Gh, issue: u64) -> Result<Fate> {
    let branch = run::branch(&format!("issue-{issue}"));
    let pulls: Vec<BranchPull> = gh.json(&[
        "pr",
        "list",
        "--head",
        &branch,
        "--state",
        "all",
        "--json",
        "number,state,body,mergeCommit",
        "--limit",
        "100",
    ])?;
    Fate::of(issue, &pulls, || {
        let issue: IssueState =
            gh.json(&["issue", "view", &issue.to_string(), "--json", "state"])?;
        Ok(issue.state == "CLOSED")
    })
}

/// Adds the comment `Merged in #PULL (COMMIT)` to the issue `issue` with
/// `gh`, when no comment of the issue starts with `Merged in #PULL`.
pub fn merged_in_on(gh: &pr::Gh, issue: u64, pull: u64, commit: &str) -> Result<()> {
    let comments: Comments =
        gh.json(&["issue", "view", &issue.to_string(), "--json", "comments"])?;
    let said = format!("Merged in #{pull}");
    if !comments
        .comments
        .iter()
        .any(|c| c.body.trim_start().starts_with(&said))
    {
        let body = text::wave_merged(pull, commit);
        gh.run(
            &["issue", "comment", &issue.to_string(), "--body-file", "-"],
            Some(&body),
        )?;
    }
    Ok(())
}

/// The work item of the folder `name` of a session: `ITEM-ID8`.
///
/// ```
/// use riff::daemon::sweep::item_of_folder;
///
/// assert_eq!(item_of_folder("issue-12-1a2b3c4d"), Some("issue-12"));
/// assert_eq!(item_of_folder("verify-issue-12-1a2b3c4d"), Some("verify-issue-12"));
/// assert_eq!(item_of_folder("issue-12-1a2b3c4d.lock"), None);
/// assert_eq!(item_of_folder("other"), None);
/// ```
pub fn item_of_folder(name: &str) -> Option<&str> {
    let (item, id) = name.rsplit_once('-')?;
    (id.len() == 8 && id.chars().all(|c| c.is_ascii_hexdigit())).then_some(item)
}

/// The issue of the work item `item`: a build `issue-N`, or a verify
/// `verify-issue-N`.
///
/// ```
/// use riff::daemon::sweep::issue_of;
///
/// assert_eq!(issue_of("issue-12"), Some(12));
/// assert_eq!(issue_of("verify-issue-12"), Some(12));
/// assert_eq!(issue_of("lead"), None);
/// assert_eq!(issue_of("task-iroh"), None);
/// ```
pub fn issue_of(item: &str) -> Option<u64> {
    let issue = item.strip_prefix("verify-").unwrap_or(item);
    issue.strip_prefix("issue-")?.parse().ok()
}

/// The lock file of the folder `folder` of a session: `ITEM-ID8.lock`
/// beside it.
///
/// ```
/// use std::path::Path;
///
/// let folder = Path::new("/d/riff/sessions/issue-12-1a2b3c4d");
/// let lock = riff::daemon::sweep::lock_of(folder);
/// assert_eq!(lock, Path::new("/d/riff/sessions/issue-12-1a2b3c4d.lock"));
/// ```
pub fn lock_of(folder: &Path) -> PathBuf {
    let mut name = folder.file_name().unwrap_or_default().to_owned();
    name.push(".lock");
    folder.with_file_name(name)
}

/// Takes the lock of the folder `folder` of a session for the `riff run`
/// that makes it. Take it before the folder exists, and keep the file
/// open to the end: the sweep keeps the folder while the lock is held
/// ([`held`]).
///
/// The doc test is a `standalone_crate`: it runs in a process of its
/// own, so no fork of another doc test holds the lock after the drop.
///
/// ```standalone_crate
/// use riff::daemon::sweep::{held, hold};
///
/// let sessions = tempfile::tempdir()?;
/// let folder = sessions.path().join("issue-12-1a2b3c4d");
/// assert!(!held(&folder), "no lock file");
/// let lock = hold(&folder)?;
/// assert!(held(&folder));
/// drop(lock);
/// assert!(!held(&folder), "the riff run ended");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn hold(folder: &Path) -> std::io::Result<File> {
    let path = lock_of(folder);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    file.lock()?;
    Ok(file)
}

/// True when a `riff run` holds the lock of the folder `folder`
/// ([`hold`]). No lock file is no lock. A lock that riff cannot read
/// counts as held: the folder stays.
///
/// A lock can stay held for a short time after the drop of the file of
/// [`hold`]. A fork copies each open file into the child, with its
/// lock, and the copy closes only when the child calls `exec`. Then the
/// folder stays, and the next sweep removes it.
pub fn held(folder: &Path) -> bool {
    let file = match File::open(lock_of(folder)) {
        Ok(file) => file,
        Err(e) => return e.kind() != std::io::ErrorKind::NotFound,
    };
    match file.try_lock_shared() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(TryLockError::WouldBlock | TryLockError::Error(_)) => true,
    }
}

/// The work of the clone of a folder that is not on `origin`
/// (01M53S7WAXD46YPN0DR96XSTMG).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Local {
    /// Each file is committed, and a branch of `origin` has each commit.
    None,
    /// Some work is only in the clone.
    Work {
        /// The files that are not committed, as `git status` counts them.
        files: usize,
        /// The commits of `HEAD` and of each local branch that no branch
        /// of `origin` has.
        commits: usize,
    },
    /// git cannot read the clone, for this reason.
    Unknown(String),
}

impl Local {
    /// The work of the clone `clone` that is not on `origin`, as
    /// the claim of an item counts it: `git
    /// status --porcelain`, and `git rev-list HEAD --branches --not
    /// --remotes=origin`.
    ///
    /// ```
    /// use riff::daemon::sweep::Local;
    ///
    /// let git = |dir: &std::path::Path, args: &[&str]| {
    ///     let mut git = std::process::Command::new("git");
    ///     git.arg("-C").arg(dir).args(args).env("GIT_CONFIG_GLOBAL", "/dev/null");
    ///     for (key, value) in [("NAME", "t"), ("EMAIL", "t@t")] {
    ///         git.env(format!("GIT_AUTHOR_{key}"), value).env(format!("GIT_COMMITTER_{key}"), value);
    ///     }
    ///     let out = git.output().unwrap();
    ///     assert!(out.status.success(), "git {args:?}: {out:?}");
    /// };
    /// let root = tempfile::tempdir()?;
    /// let (origin, clone) = (root.path().join("origin"), root.path().join("clone"));
    /// git(root.path(), &["init", "-q", "--bare", origin.to_str().unwrap()]);
    /// git(root.path(), &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);
    /// let commit = |m: &str| git(&clone, &["commit", "-q", "--allow-empty", "-m", m]);
    /// commit("first");
    /// git(&clone, &["push", "-q", "origin", "HEAD"]);
    /// assert_eq!(Local::of(&clone), Local::None);
    /// commit("second");
    /// std::fs::write(clone.join("notes.md"), "text")?;
    /// assert_eq!(Local::of(&clone), Local::Work { files: 1, commits: 1 });
    /// assert!(matches!(Local::of(&root.path().join("gone")), Local::Unknown(_)), "no clone");
    /// // A clone with no commit: git cannot count. The reason is one line.
    /// let empty = root.path().join("empty");
    /// git(root.path(), &["init", "-q", empty.to_str().unwrap()]);
    /// let Local::Unknown(why) = Local::of(&empty) else { panic!("git counts no HEAD") };
    /// assert!(why.starts_with("git rev-list --count HEAD") && !why.contains('\n'), "{why}");
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn of(clone: &Path) -> Local {
        let unknown = |e: anyhow::Error| {
            let text = format!("{e:#}");
            Local::Unknown(text.lines().next().unwrap_or_default().to_owned())
        };
        let status = match run::output("git", clone, &["status", "--porcelain"]) {
            Ok(status) => status,
            Err(e) => return unknown(e),
        };
        let ahead = [
            "rev-list",
            "--count",
            "HEAD",
            "--branches",
            "--not",
            "--remotes=origin",
        ];
        let commits = match run::output("git", clone, &ahead) {
            Ok(n) => match n.parse() {
                Ok(n) => n,
                Err(_) => return Local::Unknown(format!("git rev-list gave {n:?}")),
            },
            Err(e) => return unknown(e),
        };
        match (status.lines().count(), commits) {
            (0, 0) => Local::None,
            (files, commits) => Local::Work { files, commits },
        }
    }

    /// Why the folder of the clone stays, or `None` when the clone holds
    /// no work of its own.
    ///
    /// ```
    /// use riff::daemon::sweep::Local;
    ///
    /// assert_eq!(Local::None.why(), None);
    /// let work = Local::Work { files: 2, commits: 1 };
    /// assert_eq!(work.why().unwrap(), "its clone has 2 files that are not committed and 1 commit that is not pushed");
    /// let commits = Local::Work { files: 0, commits: 3 };
    /// assert_eq!(commits.why().unwrap(), "its clone has 3 commits that are not pushed");
    /// let file = Local::Work { files: 1, commits: 0 };
    /// assert_eq!(file.why().unwrap(), "its clone has 1 file that is not committed");
    /// let unknown = Local::Unknown("git status --porcelain: fatal: bad".into());
    /// assert_eq!(unknown.why().unwrap(), "riff cannot read the work of its clone: git status --porcelain: fatal: bad");
    /// ```
    pub fn why(&self) -> Option<String> {
        let count = |n: usize, one: &str, state: &str| match n {
            1 => format!("1 {one} that is {state}"),
            n => format!("{n} {one}s that are {state}"),
        };
        match self {
            Local::None => None,
            Local::Work { files, commits } => {
                let files = (*files > 0).then(|| count(*files, "file", "not committed"));
                let commits = (*commits > 0).then(|| count(*commits, "commit", "not pushed"));
                let parts: Vec<String> = files.into_iter().chain(commits).collect();
                Some(format!("its clone has {}", parts.join(" and ")))
            }
            Local::Unknown(why) => Some(format!(
                "riff cannot read the work of its clone: {}",
                text::safe(why)
            )),
        }
    }
}

/// The repository `OWNER/REPO` of the `origin` of the clone `clone`.
fn repo_of(clone: &Path) -> Option<String> {
    let origin = run::output("git", clone, &["remote", "get-url", "origin"]).ok()?;
    let (owner, name) = identity::parse_remote(&origin)?;
    Some(format!("{owner}/{name}"))
}

/// Runs `work` on a thread for blocking work: git, `gh` and the removal
/// of a folder.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| anyhow!("{e}"))?
}

/// The folders of the sessions of a daemon: the items of its jobs, one
/// sweep at a time, and the line of each kept folder that the log got.
#[derive(Default)]
pub struct Folders {
    /// The items of the jobs that run, with their count.
    jobs: Mutex<BTreeMap<String, usize>>,
    /// Held while a sweep runs.
    sweep: tokio::sync::Mutex<()>,
    /// The last line of each kept folder that the log got
    /// ([`Daemon::sweep_news`]).
    said: Mutex<BTreeMap<PathBuf, String>>,
}

impl Folders {
    fn jobs(&self) -> MutexGuard<'_, BTreeMap<String, usize>> {
        self.jobs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn said(&self) -> MutexGuard<'_, BTreeMap<PathBuf, String>> {
        self.said.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A job of the daemon for its item: the sweep keeps each folder of the
/// item until the drop (01M53S7WDPQ2GWHJWJ6SZXZ5PN).
pub struct InJob {
    folders: Arc<Folders>,
    item: String,
}

impl Drop for InJob {
    fn drop(&mut self) {
        let mut jobs = self.folders.jobs();
        if let Some(count) = jobs.get_mut(&self.item) {
            *count -= 1;
            if *count == 0 {
                jobs.remove(&self.item);
            }
        }
    }
}

/// What a sweep did with one folder: a line for a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Swept {
    /// The folder.
    pub folder: PathBuf,
    /// Why riff removed the folder, or why it kept a folder whose work
    /// is over.
    pub line: String,
    /// True when the folder stays.
    pub kept: bool,
    /// The repository of the folder, when it stays because its clone
    /// holds work that is not on `origin`: the lead gets a note
    /// (01M57D3FEJPHD962GW0H82N8E6).
    pub work: Option<String>,
}

impl<R: Runtime + 'static> Daemon<R> {
    /// The folder of the session folders: `sessions` beside the folder of
    /// the daemon, as [`run::sessions_dir`] gives it.
    pub fn sessions_dir(&self) -> PathBuf {
        self.dir.with_file_name("sessions")
    }

    /// Keeps each folder of `item` while a job of the item runs: from
    /// before the clone of its session to its last step after the end,
    /// which reads the folder (01M53S7WDPQ2GWHJWJ6SZXZ5PN). Take it
    /// before the job starts, and keep it to the end of
    /// [`crate::wave::finish`].
    pub(crate) fn in_job(&self, item: &str) -> InJob {
        *self.folders.jobs().entry(item.to_owned()).or_default() += 1;
        InJob {
            folders: self.folders.clone(),
            item: item.to_owned(),
        }
    }

    /// Removes the folder of each session that does not run, when its
    /// fate on the forge says so and its clone holds no work of its own:
    /// see the design. With `repo`, only the folders of that repository.
    /// It gives a line for each folder that it removed, and for each
    /// folder whose work is over that it kept, with the reason.
    pub async fn sweep(&self, repo: Option<&str>) -> Vec<Swept> {
        let _one = self.folders.sweep.lock().await;
        let dir = self.sessions_dir();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut folders: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        folders.sort();
        let mut swept = Vec::new();
        for folder in folders {
            match self.sweep_folder(&folder, repo).await {
                Ok(Some(line)) => swept.push(line),
                Ok(None) => {}
                Err(e) => eprintln!("riff daemon: {e:#}"),
            }
        }
        swept
    }

    /// Sweeps as [`Daemon::sweep`], and gives the lines for the log of
    /// the daemon: each removed folder, and each kept folder with a new
    /// reason. A folder that stays for the same reason as at an earlier
    /// sweep gets no new line (01M53S7WAXD46YPN0DR96XSTMG). A new line of
    /// a folder that stays because its clone holds work that is not on
    /// `origin` is also a note for the lead of its repository
    /// (01M57D3FEJPHD962GW0H82N8E6).
    pub async fn sweep_news(&self, repo: Option<&str>) -> Vec<String> {
        let swept = self.sweep(repo).await;
        let mut said = self.folders.said();
        swept
            .into_iter()
            .filter_map(|swept| {
                if !swept.kept {
                    said.remove(&swept.folder);
                } else if said.get(&swept.folder) == Some(&swept.line) {
                    return None;
                } else {
                    said.insert(swept.folder, swept.line.clone());
                }
                if let Some(repo) = &swept.work {
                    self.news.push(repo, text::daemon_kept_work(&swept.line));
                }
                Some(swept.line)
            })
            .collect()
    }

    /// Sweeps the folder `folder`: why it went, or why it stays when its
    /// work is over. `None` when it stays and its work is not over.
    async fn sweep_folder(&self, folder: &Path, only: Option<&str>) -> Result<Option<Swept>> {
        let Some(item) = folder
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(item_of_folder)
        else {
            return Ok(None);
        };
        let Some(issue) = issue_of(item) else {
            return Ok(None);
        };
        if self.runs(folder, item) {
            return Ok(None);
        }
        let clone = folder.join("clone");
        let Some(repo) = blocking(move || Ok(repo_of(&clone))).await? else {
            return Ok(None);
        };
        if only.is_some_and(|only| only != repo) {
            return Ok(None);
        }
        let kept = |line: String, work: Option<String>| {
            Some(Swept {
                folder: folder.to_owned(),
                line,
                kept: true,
                work,
            })
        };
        // A fault of the forge, for example a `gh` with no answer in
        // GH_LIMIT, keeps the folder: the next sweep tries again.
        let forge = async {
            let fate = self.world.fate(&repo, issue).await?;
            if let Fate::Merged {
                pull,
                commit,
                refs: true,
            } = &fate
            {
                self.world.merged_in(&repo, issue, *pull, commit).await?;
            }
            anyhow::Ok(fate)
        };
        let fate = match forge.await {
            Ok(fate) => fate,
            Err(e) => {
                let line = text::swept_forge_failed(folder, &format!("{e:#}"));
                return Ok(kept(line, None));
            }
        };
        let Some(over) = text::swept_why(issue, &fate) else {
            return Ok(None);
        };
        let clone = folder.join("clone");
        if let Some(why) = blocking(move || Ok(Local::of(&clone).why())).await? {
            return Ok(kept(text::swept_kept(folder, &over, &why), Some(repo)));
        }
        let gone = folder.to_owned();
        blocking(move || {
            std::fs::remove_dir_all(&gone)
                .with_context(|| format!("cannot remove {}", gone.display()))?;
            let _ = std::fs::remove_file(lock_of(&gone));
            Ok(())
        })
        .await?;
        Ok(Some(Swept {
            folder: folder.to_owned(),
            line: text::swept(folder, &over),
            kept: false,
            work: None,
        }))
    }

    /// True when the session of the folder `folder` of `item` runs: see
    /// the design. A record names its folder by the path of the host of
    /// `riff run`, so the name `ITEM-ID8` of the folder counts, not its
    /// path.
    fn runs(&self, folder: &Path, item: &str) -> bool {
        let name = folder.file_name();
        self.records().iter().any(|r| r.folder.file_name() == name)
            || self.folders.jobs().contains_key(item)
            || held(folder)
    }

    /// Sweeps at once, then each [`every`], until the daemon stops. Call
    /// it after [`Daemon::recover`], so the sessions that run have their
    /// records.
    pub fn sweep_every(self: &Arc<Self>) {
        let Some(every) = every() else {
            return;
        };
        let daemon = self.clone();
        tokio::spawn(async move {
            loop {
                for line in daemon.sweep_news(None).await {
                    println!("{line}");
                }
                tokio::time::sleep(every).await;
            }
        });
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::time::Instant;

    use super::*;
    use crate::daemon::tests::{Here, IMAGE, Server, until};
    use crate::daemon::{Ask, QUIET_LIMIT, Record, ask_done};
    use crate::runtime::Fake;
    use crate::wave::{Job, Wave, Work, World};
    use riff_client::sessions::Live;

    /// A daemon whose world gives the fate of each issue of `fates`.
    async fn here(fates: &[(u64, Fate)]) -> (Here, Arc<Server>) {
        with(Server::with_fates(fates)).await
    }

    /// A daemon with the world `server`.
    async fn with(server: Server) -> (Here, Arc<Server>) {
        let server = Arc::new(server);
        let world: Arc<dyn World> = server.clone();
        let fake = Fake::default().with_image(IMAGE);
        let here = Here::with_world(fake, QUIET_LIMIT, Some(world)).await;
        (here, server)
    }

    /// Makes the clone of the folder `name` in the session folders of
    /// `here`, with the origin `o/r` on GitHub. It gives the folder.
    fn folder(here: &Here, name: &str) -> PathBuf {
        let folder = here.daemon.sessions_dir().join(name);
        origin(&folder.join("clone"), "https://github.com/o/r.git");
        folder
    }

    /// Runs git in `dir` with no config of the person, and gives its
    /// stdout.
    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
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
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// A git clone at `clone` with the origin `url`, as a session leaves
    /// it after its push: one commit, which `main` of `origin` has.
    pub(crate) fn origin(clone: &Path, url: &str) {
        std::fs::create_dir_all(clone).unwrap();
        git(clone, &["init", "-q", "-b", "main"]);
        git(clone, &["commit", "-q", "--allow-empty", "-m", "start"]);
        git(clone, &["remote", "add", "origin", url]);
        git(clone, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    }

    /// The lines of `swept`.
    fn lines(swept: Vec<Swept>) -> Vec<String> {
        swept.into_iter().map(|s| s.line).collect()
    }

    #[tokio::test]
    async fn with_no_wave_the_daemon_removes_the_folders_whose_pull_request_merged_or_closed() {
        let merged = Fate::Merged {
            pull: 41,
            commit: "c41".into(),
            refs: true,
        };
        let (here, server) = here(&[
            (1, merged),
            (2, Fate::Closed { pull: 42 }),
            (3, Fate::Closed { pull: 43 }),
            (4, Fate::Stays),
            (5, Fate::IssueClosed),
            (6, Fate::Closed { pull: 46 }),
            (7, Fate::Closed { pull: 47 }),
        ])
        .await;
        let gone_merged = folder(&here, "issue-1-00000001");
        let gone_closed = folder(&here, "verify-issue-2-00000002");
        // A session that runs: the daemon has its record. Its host names
        // the folder by another path: a link to the session folders.
        let mut start = here.start("issue-3-00000003", "issue-3");
        origin(&start.folder.join("clone"), "https://github.com/o/r.git");
        let runs = start.folder.clone();
        let link = here.root.path().join("link");
        std::os::unix::fs::symlink(here.daemon.sessions_dir(), &link).unwrap();
        start.folder = link.join("issue-3-00000003");
        let (_client, _) = here.started(start).await;
        let open = folder(&here, "issue-4-00000004");
        let gone_issue = folder(&here, "issue-5-00000005");
        // The riff run that made the folder holds its lock.
        let locked = folder(&here, "issue-6-00000006");
        let lock = hold(&locked).unwrap();
        // A job of the daemon runs for issue-7.
        let in_job = folder(&here, "issue-7-00000007");
        let job = here.daemon.in_job("issue-7");
        let other = here.daemon.sessions_dir().join("lead-00000008");
        std::fs::create_dir_all(&other).unwrap();

        let lines = lines(here.daemon.sweep(None).await);

        assert!(!gone_merged.exists());
        assert!(!gone_closed.exists());
        assert!(!gone_issue.exists());
        for kept in [&runs, &open, &locked, &in_job, &other] {
            assert!(kept.exists(), "{}", kept.display());
        }
        assert_eq!(
            lines,
            [
                text::swept(&gone_merged, "the pull request #41 merged"),
                text::swept(
                    &gone_issue,
                    "the issue #5 is closed, and its branch has no pull request"
                ),
                text::swept(&gone_closed, "the pull request #42 closed with no merge"),
            ]
        );
        assert_eq!(
            server.comments(),
            [("o/r".to_owned(), 1, 41, "c41".to_owned())]
        );

        // The lock and the job end: the next sweep removes their folders.
        drop((lock, job));
        // A fork of another test can hold the lock until its `exec`.
        until(|| !held(&locked)).await;
        let swept = here.daemon.sweep(None).await;
        assert_eq!(swept.len(), 2, "{swept:?}");
        assert!(!locked.exists() && !lock_of(&locked).exists());
        assert!(!in_job.exists());
        assert!(runs.exists() && open.exists());
    }

    /// 01M53S7WAXD46YPN0DR96XSTMG: a folder whose work is over stays
    /// while its clone has a file that is not committed, or a commit that
    /// no branch of `origin` has, or when git cannot read the clone.
    /// `riff daemon sweep` says why.
    #[tokio::test]
    async fn a_clone_with_work_that_is_not_pushed_stays_and_riff_daemon_sweep_says_why() {
        let merged = Fate::Merged {
            pull: 43,
            commit: "c43".into(),
            refs: true,
        };
        let (here, server) = here(&[
            (1, Fate::Closed { pull: 41 }),
            (2, Fate::Closed { pull: 42 }),
            (3, merged),
            (4, Fate::IssueClosed),
            (5, Fate::Closed { pull: 45 }),
            (6, Fate::Closed { pull: 46 }),
        ])
        .await;
        // Each file committed, and each commit on origin: it goes.
        let pushed = folder(&here, "issue-1-00000001");
        // A file that is not committed.
        let changed = folder(&here, "issue-2-00000002");
        std::fs::write(changed.join("clone/notes.md"), "not committed").unwrap();
        // A commit on the branch of the item that the session did not
        // push, on a branch with an older merged pull request.
        let ahead = folder(&here, "issue-3-00000003");
        let clone = ahead.join("clone");
        git(&clone, &["switch", "-q", "-c", "worktree-issue-3"]);
        git(&clone, &["commit", "-q", "--allow-empty", "-m", "one"]);
        git(&clone, &["commit", "-q", "--allow-empty", "-m", "two"]);
        // A commit on another local branch.
        let other = folder(&here, "issue-4-00000004");
        let clone = other.join("clone");
        git(&clone, &["switch", "-q", "-c", "spare"]);
        git(&clone, &["commit", "-q", "--allow-empty", "-m", "spare"]);
        git(&clone, &["switch", "-q", "main"]);
        // The session pushed its branch: origin has each commit.
        let branch = folder(&here, "issue-5-00000005");
        let clone = branch.join("clone");
        git(&clone, &["switch", "-q", "-c", "worktree-issue-5"]);
        git(&clone, &["commit", "-q", "--allow-empty", "-m", "work"]);
        git(
            &clone,
            &["update-ref", "refs/remotes/origin/worktree-issue-5", "HEAD"],
        );
        // git cannot read the clone: a bad index.
        let broken = folder(&here, "issue-6-00000006");
        std::fs::write(broken.join("clone/.git/index"), "not an index").unwrap();

        let mut said = Vec::new();
        ask_done(&here.dir, Ask::Sweep, |line| said.push(line.to_owned()))
            .await
            .unwrap();

        assert!(!pushed.exists() && !branch.exists());
        for kept in [&changed, &ahead, &other, &broken] {
            assert!(kept.exists(), "{}", kept.display());
        }
        let closed = |n: u64| format!("the pull request #{n} closed with no merge");
        assert_eq!(said.len(), 6, "{said:#?}");
        assert_eq!(said[0], text::swept(&pushed, &closed(41)));
        assert_eq!(
            said[1],
            text::swept_kept(
                &changed,
                &closed(42),
                "its clone has 1 file that is not committed"
            )
        );
        assert_eq!(
            said[2],
            text::swept_kept(
                &ahead,
                "the pull request #43 merged",
                "its clone has 2 commits that are not pushed"
            )
        );
        assert_eq!(
            said[3],
            text::swept_kept(
                &other,
                "the issue #4 is closed, and its branch has no pull request",
                "its clone has 1 commit that is not pushed"
            )
        );
        assert_eq!(said[4], text::swept(&branch, &closed(45)));
        let unread = format!(
            "riff kept the folder {}: {}, but riff cannot read the work of its clone: git status \
             --porcelain: ",
            broken.display(),
            closed(46)
        );
        assert!(said[5].starts_with(&unread), "{}", said[5]);
        // The issue of a merged pull request gets its comment, also when
        // the folder stays: the item is not free work again.
        assert_eq!(
            server.comments(),
            [("o/r".to_owned(), 3, 43, "c43".to_owned())]
        );

        // The session of issue-3 pushed after all: the next sweep removes
        // its folder.
        let clone = ahead.join("clone");
        git(
            &clone,
            &["update-ref", "refs/remotes/origin/worktree-issue-3", "HEAD"],
        );
        let swept = here.daemon.sweep(None).await;
        assert!(!ahead.exists());
        assert!(
            swept
                .iter()
                .any(|s| !s.kept && s.line == text::swept(&ahead, "the pull request #43 merged")),
            "{swept:#?}"
        );
    }

    /// The log of the daemon gets the line of a kept folder one time,
    /// and again when the reason changes (01M53S7WAXD46YPN0DR96XSTMG).
    #[tokio::test]
    async fn the_log_gets_the_line_of_a_kept_folder_one_time() {
        let (here, _) = here(&[(1, Fate::Closed { pull: 41 })]).await;
        let kept = folder(&here, "issue-1-00000001");
        let clone = kept.join("clone");
        git(&clone, &["commit", "-q", "--allow-empty", "-m", "one"]);
        let over = "the pull request #41 closed with no merge";
        let one = text::swept_kept(&kept, over, "its clone has 1 commit that is not pushed");

        assert_eq!(
            here.daemon.sweep_news(None).await,
            std::slice::from_ref(&one)
        );
        // One note for the lead of the repository (01M57D3FEJPHD962GW0H82N8E6).
        assert_eq!(here.daemon.news.take("o/r"), [text::daemon_kept_work(&one)]);
        assert!(here.daemon.sweep_news(None).await.is_empty());
        assert!(
            here.daemon.news.take("o/r").is_empty(),
            "one note, not one at each sweep"
        );
        assert_eq!(
            lines(here.daemon.sweep(None).await),
            [one],
            "riff daemon sweep says it each time"
        );

        git(&clone, &["commit", "-q", "--allow-empty", "-m", "two"]);
        let two = text::swept_kept(&kept, over, "its clone has 2 commits that are not pushed");
        assert_eq!(here.daemon.sweep_news(None).await, [two]);

        git(&clone, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        assert_eq!(
            here.daemon.sweep_news(None).await,
            [text::swept(&kept, over)]
        );
        assert!(!kept.exists());
    }

    /// 01M53S7WGM9DQV2DXFHBRSMTP8: a `gh` that gives no answer stops at
    /// its time limit. The sweep removes nothing for its folder, says
    /// why, and goes on to the next folder.
    #[tokio::test]
    async fn a_gh_that_hangs_keeps_its_folder_and_the_sweep_goes_on() {
        let root = tempfile::tempdir().unwrap();
        let script = r#"#!/bin/sh
case "$*" in
*"--head worktree-issue-1 "*) exec sleep 600 ;;
*) echo '[{"number":42,"state":"CLOSED","body":"Closes #2"}]' ;;
esac
"#;
        let limit = Duration::from_secs(1);
        let gh =
            pr::Gh::at(isolated::write_program(root.path().join("gh"), script)).with_limit(limit);
        let (here, _) = with(Server::with_gh(gh)).await;
        let hangs = folder(&here, "issue-1-00000001");
        let closed = folder(&here, "issue-2-00000002");

        let start = Instant::now();
        let swept = here.daemon.sweep(None).await;
        assert!(start.elapsed() < isolated::HANG);

        assert!(hangs.exists());
        assert!(!closed.exists(), "the sweep went on to the next folder");
        let call = "pr list --head worktree-issue-1 --state all --json \
                    number,state,body,mergeCommit --limit 100";
        assert_eq!(
            swept,
            [
                Swept {
                    folder: hangs.clone(),
                    line: text::swept_forge_failed(&hangs, &text::gh_no_answer(call, limit)),
                    kept: true,
                    work: None,
                },
                Swept {
                    folder: closed.clone(),
                    line: text::swept(&closed, "the pull request #42 closed with no merge"),
                    kept: false,
                    work: None,
                },
            ]
        );
    }

    /// 01M53S7WDPQ2GWHJWJ6SZXZ5PN: a session of a job that ended while
    /// no daemon ran. A new daemon does the steps after the end, and the
    /// sweep keeps the folder until the last step: the daemon has no
    /// record of the session then.
    #[tokio::test]
    async fn the_steps_after_the_end_of_a_job_keep_its_folder() {
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let server = Server::with_fates(&[(12, Fate::Closed { pull: 40 })]).with_gate(gate.clone());
        let (here, server) = with(server).await;
        let folder = folder(&here, "issue-12-9f8e7d6c");
        let wave = Wave {
            repo: "o/r".into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        let name = "riff-issue-12-9f8e7d6c";
        let record = Record {
            live: Live {
                name: name.into(),
                session: "9f8e7d6c-0000-4000-8000-000000000000".into(),
                item: "issue-12".into(),
                image: IMAGE.into(),
                since_ms: 0,
                phase: Default::default(),
                last: String::new(),
            },
            folder: folder.clone(),
            server: wave.server.clone(),
            me: crate::daemon::tests::me(name),
            inputs: Vec::new(),
            closed: false,
            failed: false,
            job: Some(Job {
                wave,
                work: Work::Build { issue: 12 },
            }),
            terminal: false,
            takeover: false,
            interrupt: false,
        };
        let path = crate::daemon::record_path(&here.dir, name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string(&record).unwrap()).unwrap();

        here.daemon.recover().await;
        until(|| server.said().contains(&"finish issue-12".to_owned())).await;
        assert!(here.daemon.records().is_empty());
        assert!(here.daemon.sweep(None).await.is_empty());
        assert!(folder.exists(), "the steps after the end run");

        gate.add_permits(1);
        until(|| server.said().contains(&"finished issue-12".to_owned())).await;
        let start = Instant::now();
        while folder.exists() {
            assert!(start.elapsed() < isolated::HANG, "the job did not end");
            here.daemon.sweep(None).await;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn a_sweep_of_one_repository_keeps_the_folders_of_another() {
        let (here, _) = here(&[
            (1, Fate::Closed { pull: 41 }),
            (2, Fate::Closed { pull: 42 }),
        ])
        .await;
        let mine = folder(&here, "issue-1-00000001");
        let theirs = here.daemon.sessions_dir().join("issue-2-00000002");
        origin(&theirs.join("clone"), "git@github.com:other/r.git");
        let no_origin = here.daemon.sessions_dir().join("issue-2-00000003");
        std::fs::create_dir_all(no_origin.join("clone")).unwrap();

        let swept = here.daemon.sweep(Some("o/r")).await;
        assert_eq!(swept.len(), 1, "{swept:?}");
        assert!(!mine.exists());
        assert!(theirs.exists() && no_origin.exists());

        let swept = here.daemon.sweep(None).await;
        assert_eq!(swept.len(), 1, "{swept:?}");
        assert!(!theirs.exists());
        assert!(
            no_origin.exists(),
            "no origin: riff does not know its repository"
        );
    }

    #[tokio::test]
    async fn the_daemon_sweeps_by_itself_at_its_start() {
        let (here, _) = here(&[(1, Fate::Closed { pull: 41 })]).await;
        let closed = folder(&here, "issue-1-00000001");
        here.daemon.sweep_every();
        until(|| !closed.exists()).await;
    }

    #[tokio::test]
    async fn riff_daemon_sweep_gives_each_removed_folder_with_its_reason() {
        let (here, _) = here(&[(1, Fate::Closed { pull: 41 }), (4, Fate::Stays)]).await;
        let closed = folder(&here, "issue-1-00000001");
        let open = folder(&here, "issue-4-00000004");
        let mut said = Vec::new();
        ask_done(&here.dir, Ask::Sweep, |line| said.push(line.to_owned()))
            .await
            .unwrap();
        assert_eq!(
            said,
            [text::swept(
                &closed,
                "the pull request #41 closed with no merge"
            )]
        );
        assert!(open.exists());

        let mut said = Vec::new();
        ask_done(&here.dir, Ask::Sweep, |line| said.push(line.to_owned()))
            .await
            .unwrap();
        assert_eq!(said, [text::SWEPT_NONE]);
    }

    /// The doc test of `hold` checks the lock after the drop. A fork of
    /// another doc test of the same process can hold the lock then. So
    /// the doc test runs in a process of its own.
    #[test]
    fn the_doc_test_of_hold_is_a_crate_of_its_own() {
        let text = include_str!("sweep.rs");
        let at = text.find("\npub fn hold(").unwrap();
        let doc = &text[..at];
        let fence = doc.rfind("/// ```").unwrap();
        let open = doc[..fence].rfind("/// ```").unwrap();
        let tag = doc[open..].lines().next().unwrap();
        assert_eq!(tag, "/// ```standalone_crate");
    }
}
