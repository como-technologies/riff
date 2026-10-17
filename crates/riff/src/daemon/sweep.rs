//! The folders of the sessions of `riff run` and of the waves (#938,
//! 01M4Z81ARSTAK8HR96S084SDFD, 01M53NJ5V27BCHDNHK2THQQZZQ,
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
//! A session runs, and its folder stays, while one of these is true:
//!
//! - the daemon has the record of the session;
//! - a job of the daemon starts the session, from before its clone to
//!   its end ([`Daemon::starting`]);
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
//!     T -- "merged, closed, or the issue closed" --> D["riff removes the folder and its lock, and says why"]
//! ```
//!
//! One sweep runs at a time. Each step of git and of `gh` can block, so
//! it runs on a thread for blocking work. A fault at one folder stops no
//! other folder: the daemon logs it and goes on.
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
        let issue: IssueState = gh.json(&["issue", "view", &issue.to_string(), "--json", "state"])?;
        Ok(issue.state == "CLOSED")
    })
}

/// Adds the comment `Merged in #PULL (COMMIT)` to the issue `issue` with
/// `gh`, when no comment of the issue starts with `Merged in #PULL`.
pub fn merged_in_on(gh: &pr::Gh, issue: u64, pull: u64, commit: &str) -> Result<()> {
    let comments: Comments = gh.json(&["issue", "view", &issue.to_string(), "--json", "comments"])?;
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
/// ```
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
/// ([`hold`]). A lock that riff cannot read counts as held: the folder
/// stays.
pub fn held(folder: &Path) -> bool {
    let Ok(file) = File::open(lock_of(folder)) else {
        return false;
    };
    match file.try_lock_shared() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(TryLockError::WouldBlock | TryLockError::Error(_)) => true,
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

/// The folders of the sessions of a daemon: the items of its jobs that
/// start a session, and one sweep at a time.
#[derive(Default)]
pub struct Folders {
    /// The items of the jobs that start, with their count.
    starting: Mutex<BTreeMap<String, usize>>,
    /// Held while a sweep runs.
    sweep: tokio::sync::Mutex<()>,
}

impl Folders {
    fn starting(&self) -> MutexGuard<'_, BTreeMap<String, usize>> {
        self.starting.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A job of the daemon that starts a session of its item: the sweep
/// keeps each folder of the item until the drop.
pub struct Starting<'a> {
    folders: &'a Folders,
    item: String,
}

impl Drop for Starting<'_> {
    fn drop(&mut self) {
        let mut starting = self.folders.starting();
        if let Some(count) = starting.get_mut(&self.item) {
            *count -= 1;
            if *count == 0 {
                starting.remove(&self.item);
            }
        }
    }
}

impl<R: Runtime + 'static> Daemon<R> {
    /// The folder of the session folders: `sessions` beside the folder of
    /// the daemon, as [`run::sessions_dir`] gives it.
    pub fn sessions_dir(&self) -> PathBuf {
        self.dir.with_file_name("sessions")
    }

    /// Keeps each folder of `item` while the job of the item starts its
    /// session: from before its clone to the drop.
    pub(crate) fn starting(&self, item: &str) -> Starting<'_> {
        *self.folders.starting().entry(item.to_owned()).or_default() += 1;
        Starting {
            folders: &self.folders,
            item: item.to_owned(),
        }
    }

    /// Removes the folder of each session that does not run, when its
    /// fate on the forge says so: see the design. With `repo`, only the
    /// folders of that repository. It gives a line for each folder that
    /// it removed, with the reason.
    pub async fn sweep(&self, repo: Option<&str>) -> Vec<String> {
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
        let mut lines = Vec::new();
        for folder in folders {
            match self.sweep_folder(&folder, repo).await {
                Ok(Some(line)) => lines.push(line),
                Ok(None) => {}
                Err(e) => eprintln!("riff daemon: {e:#}"),
            }
        }
        lines
    }

    /// Sweeps the folder `folder`: the line of its removal, or `None`
    /// when it stays.
    async fn sweep_folder(&self, folder: &Path, only: Option<&str>) -> Result<Option<String>> {
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
        let fate = self.world.fate(&repo, issue).await?;
        if let Fate::Merged {
            pull,
            commit,
            refs: true,
        } = &fate
        {
            self.world.merged_in(&repo, issue, *pull, commit).await?;
        }
        let Some(why) = text::swept_why(issue, &fate) else {
            return Ok(None);
        };
        let gone = folder.to_owned();
        blocking(move || {
            std::fs::remove_dir_all(&gone)
                .with_context(|| format!("cannot remove {}", gone.display()))?;
            let _ = std::fs::remove_file(lock_of(&gone));
            Ok(())
        })
        .await?;
        Ok(Some(text::swept(folder, &why)))
    }

    /// True when the session of the folder `folder` of `item` runs: see
    /// the design.
    fn runs(&self, folder: &Path, item: &str) -> bool {
        self.records().iter().any(|r| r.folder == folder)
            || self.folders.starting().contains_key(item)
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
                for line in daemon.sweep(None).await {
                    println!("{line}");
                }
                tokio::time::sleep(every).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::tests::{Here, IMAGE, Server, until};
    use crate::daemon::{Ask, QUIET_LIMIT, ask_done};
    use crate::runtime::Fake;
    use crate::wave::World;

    /// A daemon whose world gives the fate of each issue of `fates`.
    async fn here(fates: &[(u64, Fate)]) -> (Here, Arc<Server>) {
        let server = Arc::new(Server::with_fates(fates));
        let world: Arc<dyn World> = server.clone();
        let fake = Fake::default().with_image(IMAGE);
        let here = Here::with_world(fake, QUIET_LIMIT, Some(world)).await;
        (here, server)
    }

    /// Makes the clone of the folder `name` in the session folders of
    /// `here`, with the origin `o/r` on GitHub. It gives the folder.
    fn folder(here: &Here, name: &str) -> PathBuf {
        let folder = here.daemon.sessions_dir().join(name);
        let clone = folder.join("clone");
        std::fs::create_dir_all(&clone).unwrap();
        origin(&clone, "https://github.com/o/r.git");
        folder
    }

    /// A git clone at `clone` with the origin `url`.
    fn origin(clone: &Path, url: &str) {
        run::output("git", clone, &["init", "-q"]).unwrap();
        run::output("git", clone, &["remote", "add", "origin", url]).unwrap();
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
        // A session that runs: the daemon has its record.
        let start = here.start("issue-3-00000003", "issue-3");
        origin(&start.folder.join("clone"), "https://github.com/o/r.git");
        let runs = start.folder.clone();
        let (_client, _) = here.started(start).await;
        let open = folder(&here, "issue-4-00000004");
        let gone_issue = folder(&here, "issue-5-00000005");
        // The riff run that made the folder holds its lock.
        let locked = folder(&here, "issue-6-00000006");
        let lock = hold(&locked).unwrap();
        // A job of the daemon starts a session of issue-7.
        let starting = folder(&here, "issue-7-00000007");
        let job = here.daemon.starting("issue-7");
        let other = here.daemon.sessions_dir().join("lead-00000008");
        std::fs::create_dir_all(&other).unwrap();

        let lines = here.daemon.sweep(None).await;

        assert!(!gone_merged.exists());
        assert!(!gone_closed.exists());
        assert!(!gone_issue.exists());
        for kept in [&runs, &open, &locked, &starting, &other] {
            assert!(kept.exists(), "{}", kept.display());
        }
        assert_eq!(
            lines,
            [
                text::swept(&gone_merged, "the pull request #41 merged"),
                text::swept(&gone_issue, "the issue #5 is closed, and its branch has no pull request"),
                text::swept(&gone_closed, "the pull request #42 closed with no merge"),
            ]
        );
        assert_eq!(server.comments(), [("o/r".to_owned(), 1, 41, "c41".to_owned())]);

        // The lock and the job end: the next sweep removes their folders.
        drop((lock, job));
        let lines = here.daemon.sweep(None).await;
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(!locked.exists() && !lock_of(&locked).exists());
        assert!(!starting.exists());
        assert!(runs.exists() && open.exists());
    }

    #[tokio::test]
    async fn a_sweep_of_one_repository_keeps_the_folders_of_another() {
        let (here, _) = here(&[(1, Fate::Closed { pull: 41 }), (2, Fate::Closed { pull: 42 })]).await;
        let mine = folder(&here, "issue-1-00000001");
        let theirs = here.daemon.sessions_dir().join("issue-2-00000002");
        std::fs::create_dir_all(theirs.join("clone")).unwrap();
        origin(&theirs.join("clone"), "git@github.com:other/r.git");
        let no_origin = here.daemon.sessions_dir().join("issue-2-00000003");
        std::fs::create_dir_all(no_origin.join("clone")).unwrap();

        let lines = here.daemon.sweep(Some("o/r")).await;
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(!mine.exists());
        assert!(theirs.exists() && no_origin.exists());

        let lines = here.daemon.sweep(None).await;
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(!theirs.exists());
        assert!(no_origin.exists(), "no origin: riff does not know its repository");
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
        assert_eq!(said, [text::swept(&closed, "the pull request #41 closed with no merge")]);
        assert!(open.exists());

        let mut said = Vec::new();
        ask_done(&here.dir, Ask::Sweep, |line| said.push(line.to_owned()))
            .await
            .unwrap();
        assert_eq!(said, [text::SWEPT_NONE]);
    }
}
