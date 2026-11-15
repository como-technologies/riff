//! The folders of the sessions of `riff run` and of the waves (#938,
//! #973, 01M4Z81ARSTAK8HR96S084SDFD, 01M53NJ5V27BCHDNHK2THQQZZQ,
//! 01M53NJ5Y2CMY5MWF9328KW7DE, 01M7DZ4E02PX5Y0WMHFCJ9KS14,
//! 01M7GA9BC2DR4C87YSZZGKNY22).
//!
//! # Design
//!
//! Each session of `riff run` and of a wave has a folder of its own,
//! `ITEM-ID8` in [`crate::run::sessions_dir`]: the clone with its
//! `target`, often some GB. The daemon removes the folder of each session
//! that does not run when the work of its branch is over, or is on
//! `origin` ([`Daemon::sweep`]):
//!
//! - at its start, after it took up its sessions, and then each
//!   [`EVERY`], also when no wave runs ([`Daemon::sweep_every`]);
//! - at each look of a wave, for the folders of its repository
//!   ([`crate::wave`]). It asks the forge about a folder that stays one
//!   time in [`EVERY`] only, and it deletes no old `riff-kept` branch
//!   (#1549);
//! - at once, for `riff daemon sweep` ([`Ask::Sweep`](super::Ask::Sweep)).
//!
//! The sweep looks into no clone (#2138). A session writes its clone, and
//! a git config can name a program that git runs on the host
//! ([`crate::host_git`]). So the sweep decides from facts that no session
//! can write:
//!
//! - the repository of the folder: its file [`REPO`], which the host
//!   writes before the clone ([`note_repo`], 01M7DZ42EVHAT8N5MZEV97G3DW).
//!   The container sees only `clone/` and `claude/` of the folder;
//! - the last commit of the clone: its file [`HEAD`], which the daemon
//!   writes from the exit of the relay ([`note_head`],
//!   01M7GA96FQS4PR0X6RNPMCT2V5). git read it in the container, at the end
//!   of `claude` ([`crate::session_relay::clean_head`]);
//! - the fate on the forge: the pull requests of the branch of its item
//!   on `origin`, and the state of its issue ([`World::fate`],
//!   [`Fate::of`]), and whether that branch holds the last commit
//!   ([`World::branch_holds`], [`holds_on`]);
//! - the age of the folder: the time since the last change of the folder
//!   itself and of its `events.jsonl` ([`age`]).
//!
//! | The branch `worktree-ITEM` and the issue | The folder |
//! |---|---|
//! | An open pull request | stays, with no call when a look read the open pull requests in [`EVERY`] ([`fate_on`]) |
//! | No open pull request, a merged one | goes |
//! | No open pull request, a closed one | goes |
//! | No pull request, and the issue is closed | goes |
//! | No pull request, and the issue is open | stays |
//!
//! A folder of a work session `issue-N` that stays by the table also goes
//! when the branch `worktree-issue-N` holds the commit of its file
//! [`HEAD`]: the session pushed its work (01M7GA9BC2DR4C87YSZZGKNY22). So
//! the folder of each round of fixes of an open pull request goes soon
//! after its end. One compare of the forge, from the commit to the branch,
//! asks it. The folder stays with no file [`HEAD`], for example after a
//! change of a tracked file that the session did not commit, and when the
//! branch does not hold the commit. The folder of a verify pushes no work:
//! the table alone counts for it.
//!
//! A folder also goes when it is [`OLD_DAYS`] old, with each fate, and
//! with no call to the forge. A folder with no file [`REPO`] stays until
//! then, and riff says why. The work of the clone that is not on `origin`
//! goes with the folder: riff pushes nothing from a clone. The sweep
//! deletes each branch `riff-kept/*` of an older sweep after
//! [`KEPT_DAYS`] days, and posts one digest note to the lead of the
//! repository (01M5NVKNPD938ASWY4TD5BGK67, 01M5NVKNS35SC8P514GYDJSRQM).
//!
//! The folder of a verify, `verify-issue-N-ID8`, has the fate of the
//! branch of its item. A merged pull request with `Refs #N` first gets
//! the comment `Merged in #PR (COMMIT)` on its issue ([`World::merged_in`]),
//! when the look of the wave did not add it at the merge (#1754). riff
//! never removes a folder whose name has no issue.
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
//!     R -- no --> A{"no change for 30 days?"}
//!     A -- yes --> D["riff removes the folder and its lock, and says why"]
//!     A -- no --> O{"its file repo"}
//!     O -- "none" --> S["it stays, and riff says why"]
//!     O -- "OWNER/REPO" --> T{"the fate on the forge"}
//!     T -- "an open pull request, or an open issue" --> H{"a work session,<br/>and the branch holds the commit of its file head?"}
//!     H -- yes --> D
//!     H -- no --> K
//!     T -- "no answer of gh in GH_LIMIT" --> S
//!     T -- "merged, closed, or the issue closed" --> D
//! ```
//!
//! The sweep of the daemon, with no repository, also removes each old
//! clone of a lead that a start of the lead could not remove
//! ([`crate::lead::sweep_old_clones`], 01M7EBBVE6WYKT5JDVASFCYPQQ):
//! `leads/OWNER-REPO/old-clone` and `old-clone-N`. It skips a lead while
//! a start of the lead runs.
//!
//! A removal goes as the person first. When the person may not delete a
//! file, for example in a read-only folder or a file of a user of the
//! container, it goes with `podman unshare`
//! ([`crate::compile_cache::remove`]).
//!
//! One sweep runs at a time. Each step of `gh` and each removal can
//! block, so it runs on a thread for blocking work. Each wave sweeps
//! before each look, so a `gh` that never ends would stop each wave.
//! Each `gh` call of the sweep has the time limit [`GH_LIMIT`] ([`gh`],
//! 01M53S7WGM9DQV2DXFHBRSMTP8). At the end of the limit, riff stops `gh`,
//! keeps the folder and goes on to the next folder. A fault at one folder
//! stops no other folder: the daemon logs it and goes on.
//!
//! [`World::branch_holds`]: crate::wave::World::branch_holds
//! [`World::fate`]: crate::wave::World::fate
//! [`World::merged_in`]: crate::wave::World::merged_in

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use super::{Daemon, gh_token};
use crate::room::Role;
use crate::runtime::Runtime;
use crate::{pr, rollout, run, text};

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
    pr::Gh::for_repo(repo).by("sweep").with_limit(GH_LIMIT)
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
            return Ok(Fate::Merged {
                pull: pull.number,
                commit: pull
                    .merge_commit
                    .as_ref()
                    .map(|c| c.oid.clone())
                    .unwrap_or_default(),
                refs: rollout::refs(&pull.body, issue),
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

/// The fate of the folder of a session of the issue `issue` of the
/// repository `repo` with `gh` ([`Fate::of`]): the pull requests of its
/// branch, and the state of the issue when the branch has none.
///
/// A branch with an open pull request in a list that a look of the wave
/// read in [`EVERY`] stays, and this call makes no `gh` call for it
/// (#1549). Without that, each folder with an open pull request would cost
/// a call at each sweep.
pub fn fate_on(gh: &pr::Gh, repo: &str, issue: u64) -> Result<Fate> {
    let branch = run::branch(&format!("issue-{issue}"));
    if rollout::open_in_kept(gh, repo, &branch, EVERY) {
        return Ok(Fate::Stays);
    }
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
/// `gh`, when no comment of the issue starts with `Merged in #PULL`
/// ([`rollout::Comment::merged_in`]). The sweep and the look of the wave
/// call it ([`crate::wave::World::merged_in`]).
pub fn merged_in_on(gh: &pr::Gh, issue: u64, pull: u64, commit: &str) -> Result<()> {
    let comments: Comments =
        gh.json(&["issue", "view", &issue.to_string(), "--json", "comments"])?;
    if !comments.comments.iter().any(|c| c.merged_in(pull)) {
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

/// The file in the folder of a session that names its repository
/// `OWNER/REPO` (01M7DZ42EVHAT8N5MZEV97G3DW). The host writes it before
/// the clone. The container sees only `clone/` and `claude/` of the
/// folder, so no session writes it.
pub const REPO: &str = "repo";

/// Writes the repository `repo` (`OWNER/REPO`) of the session of the
/// folder `folder` in its file [`REPO`]. It makes the folder when it is
/// not there.
///
/// ```
/// use riff::daemon::sweep::{note_repo, repo_of};
///
/// let root = tempfile::tempdir()?;
/// let folder = root.path().join("issue-12-1a2b3c4d");
/// assert_eq!(repo_of(&folder), None);
/// note_repo(&folder, "o/r")?;
/// assert_eq!(repo_of(&folder).as_deref(), Some("o/r"));
/// std::fs::write(folder.join("repo"), "not a repository\n")?;
/// assert_eq!(repo_of(&folder), None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn note_repo(folder: &Path, repo: &str) -> Result<()> {
    std::fs::create_dir_all(folder).with_context(|| format!("cannot make {}", folder.display()))?;
    let path = folder.join(REPO);
    std::fs::write(&path, format!("{repo}\n"))
        .with_context(|| format!("cannot write {}", path.display()))
}

/// The repository `OWNER/REPO` of the folder `folder` of a session, from
/// its file [`REPO`]. `None` with no such file, or with a text that is no
/// `OWNER/REPO`. It looks into no clone (01M7DZ4E02PX5Y0WMHFCJ9KS14).
pub fn repo_of(folder: &Path) -> Option<String> {
    let text = std::fs::read_to_string(folder.join(REPO)).ok()?;
    let repo = text.trim();
    let (owner, name) = repo.split_once('/')?;
    let part = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (part(owner) && part(name)).then(|| repo.to_owned())
}

/// The file in the folder of a session that names the head commit of its
/// clone at the end of `claude` (01M7GA96FQS4PR0X6RNPMCT2V5). The daemon
/// writes it from the exit of the relay
/// ([`crate::session_relay::Up::Exit`]). The container does not see it.
pub const HEAD: &str = "head";

/// True when `text` is the full hash of a commit: 40 or 64 hex digits in
/// lower case.
///
/// ```
/// use riff::daemon::sweep::is_commit;
///
/// assert!(is_commit(&"a".repeat(40)));
/// assert!(is_commit(&"0".repeat(64)));
/// assert!(!is_commit("1a2b3c4"), "a short hash");
/// assert!(!is_commit(&"A".repeat(40)));
/// assert!(!is_commit(&format!("{}/", "a".repeat(39))));
/// ```
pub fn is_commit(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Writes the head commit `head` of the clone of the session of the
/// folder `folder` in its file [`HEAD`]. A text that is no commit
/// ([`is_commit`]) is an error.
///
/// ```
/// use riff::daemon::sweep::{head_of, note_head};
///
/// let root = tempfile::tempdir()?;
/// let folder = root.path().join("issue-12-1a2b3c4d");
/// std::fs::create_dir_all(&folder)?;
/// assert_eq!(head_of(&folder), None);
/// let head = "1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b";
/// note_head(&folder, head)?;
/// assert_eq!(head_of(&folder).as_deref(), Some(head));
/// assert!(note_head(&folder, "main").is_err());
/// assert_eq!(head_of(&folder).as_deref(), Some(head));
/// std::fs::write(folder.join("head"), "main\n")?;
/// assert_eq!(head_of(&folder), None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn note_head(folder: &Path, head: &str) -> Result<()> {
    if !is_commit(head) {
        bail!("{}", text::head_no_commit(head));
    }
    let path = folder.join(HEAD);
    std::fs::write(&path, format!("{head}\n"))
        .with_context(|| format!("cannot write {}", path.display()))
}

/// The head commit of the clone of the folder `folder` at the end of its
/// session, from its file [`HEAD`]. `None` with no such file, or with a
/// text that is no commit. It looks into no clone.
pub fn head_of(folder: &Path) -> Option<String> {
    let text = std::fs::read_to_string(folder.join(HEAD)).ok()?;
    let head = text.trim();
    is_commit(head).then(|| head.to_owned())
}

/// True when a branch holds a commit, from the `status` of a compare of
/// GitHub from the commit to the branch: the branch is at the commit, or
/// after it.
///
/// ```
/// use riff::daemon::sweep::holds;
///
/// assert!(holds("identical"));
/// assert!(holds("ahead"), "the branch has more commits");
/// assert!(!holds("behind"));
/// assert!(!holds("diverged"), "a push with --force");
/// ```
pub fn holds(status: &str) -> bool {
    matches!(status, "identical" | "ahead")
}

/// True when the branch `branch` of `repo` holds the commit `head`, with
/// one compare of `gh` ([`holds`], 01M7GA9BC2DR4C87YSZZGKNY22). A commit
/// or a branch that the forge does not have (HTTP 404) is no hold.
///
/// ```
/// use riff::daemon::sweep::holds_on;
///
/// let dir = tempfile::tempdir()?;
/// let script = "#!/bin/sh\n\
///     case \"$*\" in\n\
///       *compare/aaa...worktree-issue-12*) echo ahead ;;\n\
///       *compare/bbb*) echo diverged ;;\n\
///       *compare/ccc*) echo 'gh: Not Found (HTTP 404)' >&2; exit 1 ;;\n\
///       *) echo 'gh: HTTP 502' >&2; exit 1 ;;\n\
///     esac\n";
/// let gh = riff::pr::Gh::at(isolated::write_program(dir.path().join("gh"), script));
/// assert!(holds_on(&gh, "o/r", "worktree-issue-12", "aaa")?);
/// assert!(!holds_on(&gh, "o/r", "worktree-issue-12", "bbb")?);
/// assert!(!holds_on(&gh, "o/r", "worktree-issue-12", "ccc")?, "no such commit");
/// assert!(holds_on(&gh, "o/r", "worktree-issue-12", "ddd").is_err(), "a fault");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn holds_on(gh: &pr::Gh, repo: &str, branch: &str, head: &str) -> Result<bool> {
    let api = format!("repos/{repo}/compare/{head}...{branch}?per_page=1");
    match gh.run(&["api", &api, "--jq", ".status"], None) {
        Ok(status) => Ok(holds(status.trim())),
        Err(e) if format!("{e:#}").contains("HTTP 404") => Ok(false),
        Err(e) => Err(e),
    }
}

/// The days with no change after which a folder of a session goes
/// (01M7DZ4E02PX5Y0WMHFCJ9KS14).
pub const OLD_DAYS: u64 = 30;

/// [`OLD_DAYS`] as a time.
pub const OLD: Duration = Duration::from_secs(OLD_DAYS * 86_400);

/// The time since the last change of the folder `folder` of a session:
/// of the folder itself, and of its `events.jsonl` (01M7DZ4E02PX5Y0WMHFCJ9KS14).
/// The host writes both: the container sees neither. `None` when the
/// folder is not there.
///
/// ```
/// use std::time::{Duration, SystemTime};
/// use riff::daemon::sweep::age;
///
/// let root = tempfile::tempdir()?;
/// let folder = root.path().join("issue-12-1a2b3c4d");
/// assert_eq!(age(&folder), None);
/// std::fs::create_dir_all(&folder)?;
/// let month = SystemTime::now() - Duration::from_secs(31 * 86_400);
/// std::fs::File::open(&folder)?.set_modified(month)?;
/// assert!(age(&folder).unwrap() > Duration::from_secs(30 * 86_400));
/// // A new event of the session is a change.
/// std::fs::write(folder.join("events.jsonl"), "{}\n")?;
/// assert!(age(&folder).unwrap() < Duration::from_secs(60));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn age(folder: &Path) -> Option<Duration> {
    let changed = |path: &Path| {
        std::fs::symlink_metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
    };
    let events = folder.join(super::EVENTS);
    let last = [changed(folder), changed(&events)]
        .into_iter()
        .flatten()
        .max()?;
    Some(last.elapsed().unwrap_or_default())
}

/// The prefix of the branches of `origin` that hold the leftover work of
/// the folders that an older sweep kept. The sweep pushes no new one; it
/// deletes the old ones ([`delete_old_kept_on`]).
pub const KEPT_PREFIX: &str = "riff-kept/";

/// The days after which the sweep deletes a branch of [`KEPT_PREFIX`].
pub const KEPT_DAYS: u64 = 30;

/// Removes the folder `folder` of a session and its lock. A file of the
/// container belongs to another user of the user namespace: riff removes
/// it with `podman unshare` ([`crate::compile_cache::remove`]).
fn remove_folder(folder: &Path) -> Result<()> {
    crate::compile_cache::remove(folder)?;
    let _ = std::fs::remove_file(lock_of(folder));
    Ok(())
}

/// A branch of [`KEPT_PREFIX`] of `origin` with the date of its tip, as
/// seconds since 1970.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptBranch {
    /// The name, `riff-kept/ITEM-ID8`.
    pub name: String,
    /// The date of the commit at its tip.
    pub at: i64,
}

/// The branches of `branches` that are older than [`KEPT_DAYS`] at `now`.
///
/// ```
/// use riff::daemon::sweep::{KeptBranch, old_kept};
///
/// let day = 86_400;
/// let branches = [
///     KeptBranch { name: "riff-kept/issue-1-00000001".into(), at: 0 },
///     KeptBranch { name: "riff-kept/issue-2-00000002".into(), at: 31 * day },
///     KeptBranch { name: "other/x".into(), at: 0 },
/// ];
/// let old = old_kept(&branches, 61 * day);
/// assert_eq!(old, ["riff-kept/issue-1-00000001"]);
/// ```
pub fn old_kept(branches: &[KeptBranch], now: i64) -> Vec<String> {
    let limit = now - (KEPT_DAYS as i64) * 86_400;
    branches
        .iter()
        .filter(|b| b.name.starts_with(KEPT_PREFIX) && b.at < limit)
        .map(|b| b.name.clone())
        .collect()
}

/// The branches of [`KEPT_PREFIX`] of the repository `repo` with `gh`,
/// each with the date of its tip.
pub fn kept_branches_on(gh: &pr::Gh, repo: &str) -> Result<Vec<KeptBranch>> {
    let refs = gh.run(
        &[
            "api",
            &format!("repos/{repo}/git/matching-refs/heads/{KEPT_PREFIX}"),
            "--paginate",
            "--jq",
            r#".[] | .ref + " " + .object.sha"#,
        ],
        None,
    )?;
    let mut all = Vec::new();
    for line in refs.lines() {
        let Some((name, sha)) = line.split_once(' ') else {
            continue;
        };
        let date = gh.run(
            &[
                "api",
                &format!("repos/{repo}/commits/{sha}"),
                "--jq",
                ".commit.committer.date",
            ],
            None,
        )?;
        let at = date_seconds(date.trim())
            .ok_or_else(|| anyhow!("gh api repos/{repo}/commits/{sha}: bad date {date:?}"))?;
        all.push(KeptBranch {
            name: name.trim_start_matches("refs/heads/").to_owned(),
            at,
        });
    }
    Ok(all)
}

/// Seconds since 1970 of a UTC date `YYYY-MM-DDTHH:MM:SSZ`.
///
/// ```
/// use riff::daemon::sweep::date_seconds;
///
/// assert_eq!(date_seconds("1970-01-02T00:00:01Z"), Some(86_401));
/// assert_eq!(date_seconds("2026-10-24T12:00:00Z"), Some(1_792_843_200));
/// assert_eq!(date_seconds("later"), None);
/// ```
pub fn date_seconds(date: &str) -> Option<i64> {
    let (day, time) = date.strip_suffix('Z')?.split_once('T')?;
    let mut d = day.split('-').map(|n| n.parse::<i64>().ok());
    let (y, m, d) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split(':').map(|n| n.parse::<i64>().ok());
    let (h, mi, s) = (t.next()??, t.next()??, t.next()??);
    // Days from civil, after Howard Hinnant.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + mi * 60 + s)
}

/// The UTC date `YYYY-MM-DDTHH:MM:SSZ` of `seconds` since 1970: the
/// reverse of [`date_seconds`].
///
/// ```
/// use riff::daemon::sweep::{date_seconds, date_text};
///
/// assert_eq!(date_text(1_792_843_200), "2026-10-24T12:00:00Z");
/// assert_eq!(date_text(86_401), "1970-01-02T00:00:01Z");
/// assert_eq!(date_seconds(&date_text(1_800_000_007)), Some(1_800_000_007));
/// ```
pub fn date_text(seconds: i64) -> String {
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    // Civil from days, after Howard Hinnant.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Deletes each branch of [`KEPT_PREFIX`] of `repo` that is older than
/// [`KEPT_DAYS`] with `gh`, and gives their names.
pub fn delete_old_kept_on(gh: &pr::Gh, repo: &str) -> Result<Vec<String>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let old = old_kept(&kept_branches_on(gh, repo)?, now);
    for name in &old {
        gh.run(
            &[
                "api",
                "-X",
                "DELETE",
                &format!("repos/{repo}/git/refs/heads/{name}"),
            ],
            None,
        )?;
    }
    Ok(old)
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
    /// When the forge last said that a folder stays ([`Fate::Stays`]). The
    /// sweep at a look of a wave asks the forge again after [`EVERY`] only
    /// (#1549).
    stays: Mutex<BTreeMap<PathBuf, std::time::Instant>>,
}

impl Folders {
    /// True when the forge said in the last [`EVERY`] that `folder` stays.
    fn stayed(&self, folder: &Path) -> bool {
        let stays = self.stays.lock().unwrap_or_else(PoisonError::into_inner);
        stays.get(folder).is_some_and(|at| at.elapsed() < EVERY)
    }

    /// Keeps the answer of the forge for `folder`: it stays, or it does
    /// not.
    fn stays(&self, folder: &Path, stays: bool) {
        let mut all = self.stays.lock().unwrap_or_else(PoisonError::into_inner);
        match stays {
            true => all.insert(folder.to_owned(), std::time::Instant::now()),
            false => all.remove(folder),
        };
    }

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
    /// Why riff removed the folder, or why it kept a folder for a fault:
    /// no file [`REPO`], or no answer of the forge.
    pub line: String,
    /// True when the folder stays.
    pub kept: bool,
    /// The repository whose lead gets this line in its digest: the sweep
    /// deleted an old `riff-kept` branch (01M5NVKNS35SC8P514GYDJSRQM).
    pub digest: Option<String>,
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
    /// fate on the forge says so or when it is [`OLD_DAYS`] old: see the
    /// design. With `repo`, only the folders of that repository. With no
    /// `repo`, also each old clone of a lead that stayed at a start of the
    /// lead (`Daemon::sweep_old_clones`). It gives a line for each folder
    /// that it removed, and for each folder that it kept for a fault, with
    /// the reason.
    pub async fn sweep(&self, repo: Option<&str>) -> Vec<Swept> {
        let _one = self.folders.sweep.lock().await;
        let mut swept = Vec::new();
        let mut repos = BTreeSet::new();
        // With no folder of the sessions, the old clones of the leads still
        // go.
        for folder in self.session_folders() {
            match self.sweep_folder(&folder, repo, &mut repos).await {
                Ok(Some(line)) => swept.push(line),
                Ok(None) => {}
                Err(e) => eprintln!("riff daemon: {e:#}"),
            }
        }
        // An old clone of a lead that a start of the lead could not remove
        // goes at the sweep of the daemon (#2153).
        if repo.is_none() {
            swept.extend(self.sweep_old_clones().await);
        }
        // The old branches of `riff-kept` go at the sweep of the daemon, not
        // at each look of a wave (#1549): they are 30 days old.
        let old_branches = if repo.is_none() {
            repos
        } else {
            BTreeSet::new()
        };
        for repo in old_branches {
            let gh = self.writer(&repo).await;
            match self.world.delete_old_kept(gh, &repo).await {
                Ok(names) => swept.extend(names.into_iter().map(|name| Swept {
                    line: text::swept_branch_deleted(&name, KEPT_DAYS),
                    folder: PathBuf::from(name),
                    kept: false,
                    digest: Some(repo.clone()),
                })),
                Err(e) => eprintln!("riff daemon: {repo}: {e:#}"),
            }
        }
        let mut digests: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for swept in &swept {
            if let Some(repo) = &swept.digest {
                digests
                    .entry(repo.clone())
                    .or_default()
                    .push(swept.line.clone());
            }
        }
        for (repo, lines) in digests {
            if let Some(note) = text::daemon_kept_digest(&lines) {
                self.news.push(&repo, note);
            }
        }
        swept
    }

    /// Removes each old clone of a lead that stayed at a start of the lead
    /// ([`crate::lead::sweep_old_clones`], 01M7EBBVE6WYKT5JDVASFCYPQQ): a
    /// line for each one that went, and for each one that stays, with the
    /// reason.
    async fn sweep_old_clones(&self) -> Vec<Swept> {
        let leads = self.dir.with_file_name(crate::lead::LEADS);
        let old = match blocking(move || Ok(crate::lead::sweep_old_clones(&leads))).await {
            Ok(old) => old,
            Err(e) => {
                eprintln!("riff daemon: {e:#}");
                Vec::new()
            }
        };
        old.into_iter()
            .map(|(folder, gone)| {
                let (line, kept) = match gone {
                    Ok(()) => (text::swept(&folder, text::SWEPT_OLD_CLONE), false),
                    Err(e) => (
                        text::swept_old_clone_stays(&folder, &format!("{e:#}")),
                        true,
                    ),
                };
                Swept {
                    folder,
                    line,
                    kept,
                    digest: None,
                }
            })
            .collect()
    }

    /// The `gh` of the sweep on `repo`, whose writes go as the daemon
    /// ([`gh_token::writer`], 01M6XZJFE2ZTR5RE3AWM30R704): with the token
    /// of the lead App of the riff-server of the wave of `repo`, else of a
    /// session there, else of the last ask for `repo`.
    async fn writer(&self, repo: &str) -> pr::Gh {
        let gh = pr::Gh::clone(&self.world.gh(repo)).by("sweep");
        let server = self.waves_lock().get(repo).map(|r| r.wave.server.clone());
        let server = server.or_else(|| {
            self.records()
                .into_iter()
                .find(|r| r.me.place().repo_text() == repo)
                .map(|r| r.server)
        });
        gh_token::writer(&*self.world, server.as_deref(), repo, gh).await
    }

    /// Sweeps as [`Daemon::sweep`], and gives the lines for the log of
    /// the daemon: each removed folder, and each kept folder with a new
    /// reason. A folder that stays for the same reason as at an earlier
    /// sweep gets no new line (01M7DZ4E02PX5Y0WMHFCJ9KS14).
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
                Some(swept.line)
            })
            .collect()
    }

    /// Sweeps the folder `folder`: why it went, or why it stays for a
    /// fault. `None` when it stays and its work is not over. It looks into
    /// no clone (01M7DZ4E02PX5Y0WMHFCJ9KS14): it reads the file [`REPO`] of
    /// the folder, the [`age`] of the folder, and the forge.
    async fn sweep_folder(
        &self,
        folder: &Path,
        only: Option<&str>,
        repos: &mut BTreeSet<String>,
    ) -> Result<Option<Swept>> {
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
        let repo = repo_of(folder);
        if only.is_some_and(|only| repo.as_deref() != Some(only)) {
            return Ok(None);
        }
        repos.extend(repo.clone());
        let swept = |line: String, kept: bool| {
            Some(Swept {
                folder: folder.to_owned(),
                line,
                kept,
                digest: None,
            })
        };
        // An old folder goes with no call to the forge.
        if age(folder).is_some_and(|age| age >= OLD) {
            let gone = folder.to_owned();
            blocking(move || remove_folder(&gone)).await?;
            return Ok(swept(
                text::swept(folder, &text::swept_old(OLD_DAYS)),
                false,
            ));
        }
        let Some(repo) = repo else {
            return Ok(swept(text::swept_no_repo(folder, OLD_DAYS), true));
        };
        // The sweep at a look of a wave asks the forge about a folder that
        // stays one time in [`EVERY`]: the sweep of the daemon asks each
        // time (#1549).
        if only.is_some() && self.folders.stayed(folder) {
            return Ok(None);
        }
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
                let gh = self.writer(&repo).await;
                self.world
                    .merged_in(gh, &repo, issue, *pull, commit)
                    .await?;
            }
            anyhow::Ok(fate)
        };
        let fate = match forge.await {
            Ok(fate) => fate,
            Err(e) => {
                let line = text::swept_forge_failed(folder, &format!("{e:#}"));
                return Ok(swept(line, true));
            }
        };
        let over = match text::swept_why(issue, &fate) {
            Some(over) => Some(over),
            None => match self.pushed(folder, item, &repo).await {
                Ok(over) => over,
                Err(e) => {
                    let line = text::swept_forge_failed(folder, &format!("{e:#}"));
                    return Ok(swept(line, true));
                }
            },
        };
        self.folders.stays(folder, over.is_none());
        let Some(over) = over else {
            return Ok(None);
        };
        let gone = folder.to_owned();
        blocking(move || remove_folder(&gone)).await?;
        Ok(swept(text::swept(folder, &over), false))
    }

    /// Why the folder `folder` of `item`, whose session does not run, goes
    /// while its work is open: `item` is a work item, and its branch on
    /// the forge holds the commit of the file [`HEAD`] of the folder
    /// (01M7GA9BC2DR4C87YSZZGKNY22). `None` for the folder of a verify,
    /// with no file [`HEAD`], or when the branch does not hold the commit.
    async fn pushed(&self, folder: &Path, item: &str, repo: &str) -> Result<Option<String>> {
        if Role::of(item) != Role::Work {
            return Ok(None);
        }
        let Some(head) = head_of(folder) else {
            return Ok(None);
        };
        let branch = run::branch(item);
        let holds = self.world.branch_holds(repo, &branch, &head).await?;
        Ok(holds.then(|| text::swept_pushed(&head, &branch)))
    }

    /// Removes the folder `name` of one ended session on purpose
    /// (01M5P26VW5GBJDQZGEYW9A77Y0): it never removes a session that
    /// runs. With no `force`, the pull request of its work must have
    /// merged. The leftover work of the clone goes with the folder.
    pub async fn remove_one(&self, name: &str, force: bool) -> Result<String> {
        let _one = self.folders.sweep.lock().await;
        let Some(item) = item_of_folder(name).filter(|_| !name.contains('/')) else {
            bail!("{}", text::remove_bad_name(name));
        };
        let folder = self.sessions_dir().join(name);
        if !folder.is_dir() {
            bail!("{}", text::remove_no_folder(&folder));
        }
        if self.runs(&folder, item) {
            bail!("{}", text::remove_runs(name));
        }
        let why = if force {
            text::REMOVE_FORCED.to_owned()
        } else {
            let (Some(repo), Some(issue)) = (repo_of(&folder), issue_of(item)) else {
                bail!(
                    "{}",
                    text::remove_not_merged(name, "it has no repository or issue")
                );
            };
            match self.world.fate(&repo, issue).await? {
                Fate::Merged { pull, .. } => format!("the pull request #{pull} merged"),
                other => {
                    let why = text::swept_why(issue, &other)
                        .unwrap_or_else(|| "its work is still open".into());
                    bail!("{}", text::remove_not_merged(name, &why));
                }
            }
        };
        let gone = folder.clone();
        blocking(move || remove_folder(&gone)).await?;
        Ok(text::folder_removed(&folder, &why))
    }

    /// The names of the folders of the sessions that do not run, in order
    /// (01M7G430FB68R1322AT8H94KYR), for the note of a low disk. A folder
    /// whose name has no item does not count.
    pub(crate) fn idle_folders(&self) -> Vec<String> {
        self.session_folders()
            .into_iter()
            .filter_map(|path| {
                let name = path.file_name()?.to_str()?.to_owned();
                let item = item_of_folder(&name)?;
                (!self.runs(&path, item)).then_some(name)
            })
            .collect()
    }

    /// Each folder in the folder of the sessions, in order: the folders
    /// that [`Daemon::sweep`] looks at. None when that folder is not there.
    fn session_folders(&self) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = std::fs::read_dir(self.sessions_dir())
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        folders.sort();
        folders
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
                // A main clone that a folder linked to goes after the sweep
                // removed the folder (#1643).
                for line in daemon.retire_all() {
                    println!("{line}");
                }
                tokio::time::sleep(every).await;
            }
        });
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::time::{Instant, SystemTime};

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

    /// Makes the folder `name` in the session folders of `here`, as a
    /// session leaves it: its file `repo` of `o/r`, and a clone whose git
    /// config names a program that writes a mark
    /// ([`crate::host_git::hostile_clone`]). It gives the folder.
    fn folder(here: &Here, name: &str) -> PathBuf {
        let folder = here.daemon.sessions_dir().join(name);
        note_repo(&folder, "o/r").unwrap();
        crate::host_git::hostile_clone(&folder.join("clone"), &mark(here, name));
        folder
    }

    /// The mark of the clone of the folder `name` of `here`.
    fn mark(here: &Here, name: &str) -> PathBuf {
        here.root.path().join("marks").join(name)
    }

    /// Each mark of the clones of `here`: a program of the git config of
    /// a clone ran. The sweep makes none (01M7DZ4E02PX5Y0WMHFCJ9KS14).
    fn marks(here: &Here) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(here.root.path().join("marks")) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_none_or(|e| e != "sh"))
            .collect()
    }

    /// Makes the last change of the folder `folder` `days` days old.
    fn age_by(folder: &Path, days: u64) {
        let then = SystemTime::now() - Duration::from_secs(days * 86_400);
        std::fs::File::open(folder)
            .unwrap()
            .set_modified(then)
            .unwrap();
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
        note_repo(&start.folder, "o/r").unwrap();
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
        assert_eq!(marks(&here), Vec::<PathBuf>::new());
    }

    /// 01M7DZ4E02PX5Y0WMHFCJ9KS14: the sweep looks into no clone. A folder
    /// whose work is over goes with the work of its clone that is not on
    /// `origin`: riff pushes nothing. The git config of each clone names a
    /// program, and that program never runs.
    #[tokio::test]
    async fn the_sweep_looks_into_no_clone_and_the_work_of_a_clone_goes_with_its_folder() {
        let (here, _) = here(&[(1, Fate::Closed { pull: 41 }), (2, Fate::Stays)]).await;
        let over = folder(&here, "issue-1-00000001");
        std::fs::write(over.join("clone/notes.md"), "work").unwrap();
        let open = folder(&here, "issue-2-00000002");
        std::fs::write(open.join("clone/notes.md"), "work").unwrap();

        let news = here.daemon.sweep_news(None).await;

        assert_eq!(
            news,
            [text::swept(
                &over,
                "the pull request #41 closed with no merge"
            )]
        );
        assert!(!over.exists() && open.exists());
        assert!(
            here.daemon.news.take("o/r").is_empty(),
            "no note to the lead"
        );
        assert_eq!(marks(&here), Vec::<PathBuf>::new());
    }

    /// 01M7GA9BC2DR4C87YSZZGKNY22: the folder of an ended work session goes
    /// when its branch on `origin` holds the commit of its file `head`, also
    /// while its pull request is open. A folder whose commit is not on
    /// `origin` stays, and goes after the push. A folder with no file
    /// `head` stays, and so does the folder of a verify. A fault of the
    /// forge keeps the folder, and says why.
    #[tokio::test]
    async fn an_ended_work_session_whose_head_is_on_origin_goes_while_its_pull_request_is_open() {
        let (pushed, later, fault) = ("1".repeat(40), "2".repeat(40), "5".repeat(40));
        let open: Vec<(u64, Fate)> = (1..=5).map(|n| (n, Fate::Stays)).collect();
        let server = Server::with_fates(&open)
            .with_held("worktree-issue-1", &pushed)
            .with_held("worktree-issue-3", &pushed)
            .with_compare_fault(&fault);
        let (here, server) = with(server).await;
        let gone = folder(&here, "issue-1-00000001");
        note_head(&gone, &pushed).unwrap();
        let not_pushed = folder(&here, "issue-2-00000002");
        note_head(&not_pushed, &later).unwrap();
        let verify = folder(&here, "verify-issue-3-00000003");
        note_head(&verify, &pushed).unwrap();
        let no_head = folder(&here, "issue-4-00000004");
        let failed = folder(&here, "issue-5-00000005");
        note_head(&failed, &fault).unwrap();

        let swept = here.daemon.sweep(None).await;

        assert_eq!(
            lines(swept),
            [
                text::swept(&gone, &text::swept_pushed(&pushed, "worktree-issue-1")),
                text::swept_forge_failed(&failed, "gh api compare: HTTP 502"),
            ]
        );
        assert!(!gone.exists());
        for kept in [&not_pushed, &verify, &no_head, &failed] {
            assert!(kept.exists(), "{}", kept.display());
        }

        // The session of issue 2 pushed its commit: the next sweep removes
        // its folder.
        server.push("worktree-issue-2", &later);
        let swept = here.daemon.sweep(None).await;
        let why = text::swept_pushed(&later, "worktree-issue-2");
        assert_eq!(swept[0].line, text::swept(&not_pushed, &why));
        assert!(!not_pushed.exists() && verify.exists() && no_head.exists());
        assert_eq!(marks(&here), Vec::<PathBuf>::new());
    }

    /// 01M7G430FB68R1322AT8H94KYR: the note of a low disk names the folders
    /// of the sessions that do not run, in order. A session with a record,
    /// a held lock or a job runs. A folder whose name has no item, and a
    /// file, do not count.
    #[tokio::test]
    async fn the_idle_folders_are_the_folders_of_the_sessions_that_do_not_run() {
        let (here, _) = here(&[]).await;
        for name in [
            "issue-2-00000002",
            "verify-issue-3-00000003",
            "issue-1-00000001",
            "issue-6-00000006",
        ] {
            folder(&here, name);
        }
        let (_client, _) = here
            .started(here.start("issue-4-00000004", "issue-4"))
            .await;
        let _lock = hold(&folder(&here, "issue-5-00000005")).unwrap();
        let _job = here.daemon.in_job("issue-6");
        let sessions = here.daemon.sessions_dir();
        std::fs::create_dir_all(sessions.join("other")).unwrap();
        std::fs::write(sessions.join("issue-7-00000007"), "a file").unwrap();

        assert_eq!(
            here.daemon.idle_folders(),
            [
                "issue-1-00000001",
                "issue-2-00000002",
                "verify-issue-3-00000003"
            ]
        );
    }

    /// 01M7DZ4E02PX5Y0WMHFCJ9KS14: a folder goes when it had no change for
    /// 30 days, with each fate and with no call to the forge. A folder with
    /// no file `repo` stays until then, and `riff daemon sweep` says why.
    /// A new event of the session is a change.
    #[tokio::test]
    async fn a_folder_with_no_change_for_30_days_goes_and_one_with_no_repo_waits_for_it() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join("log");
        let script = format!(
            "#!/bin/sh\necho \"$*\" >> {}\ncase \"$*\" in\n\
             'pr list --head'*) echo '[]' ;;\n'issue view'*) echo '{{\"state\":\"OPEN\"}}' ;;\n\
             *) echo '[]' ;;\nesac\n",
            log.display()
        );
        let gh = pr::Gh::at(isolated::write_program(root.path().join("gh"), &script));
        let (here, _) = with(Server::with_gh(gh)).await;
        let old = folder(&here, "issue-1-00000001");
        age_by(&old, OLD_DAYS + 1);
        let young = folder(&here, "issue-2-00000002");
        age_by(&young, OLD_DAYS - 1);
        let no_repo = here.daemon.sessions_dir().join("issue-3-00000003");
        std::fs::create_dir_all(no_repo.join("clone")).unwrap();
        let old_no_repo = here.daemon.sessions_dir().join("issue-4-00000004");
        std::fs::create_dir_all(old_no_repo.join("clone")).unwrap();
        age_by(&old_no_repo, OLD_DAYS + 1);
        // An old folder whose session wrote an event today.
        let event = folder(&here, "issue-5-00000005");
        std::fs::write(event.join(crate::daemon::EVENTS), "{}\n").unwrap();
        age_by(&event, OLD_DAYS + 1);

        let swept = here.daemon.sweep(None).await;

        let gone = |folder: &Path| text::swept(folder, &text::swept_old(OLD_DAYS));
        assert_eq!(
            lines(swept),
            [
                gone(&old),
                text::swept_no_repo(&no_repo, OLD_DAYS),
                gone(&old_no_repo),
            ]
        );
        assert!(!old.exists() && !old_no_repo.exists());
        assert!(young.exists() && no_repo.exists() && event.exists());
        let asked = std::fs::read_to_string(&log).unwrap();
        assert!(!asked.contains("worktree-issue-1 "), "{asked}");
        assert!(asked.contains("--head worktree-issue-2 "), "{asked}");
        assert!(
            !asked.contains("worktree-issue-3 "),
            "no repository: {asked}"
        );
        assert_eq!(marks(&here), Vec::<PathBuf>::new());
    }

    /// The log gets the line of a kept folder one time, and again when
    /// the reason changes (01M7DZ4E02PX5Y0WMHFCJ9KS14).
    #[tokio::test]
    async fn the_log_gets_the_line_of_a_kept_folder_one_time() {
        let (here, _) = here(&[(1, Fate::Closed { pull: 41 })]).await;
        let kept = here.daemon.sessions_dir().join("issue-1-00000001");
        std::fs::create_dir_all(kept.join("clone")).unwrap();

        let one = here.daemon.sweep_news(None).await;
        assert_eq!(one, [text::swept_no_repo(&kept, OLD_DAYS)]);
        assert!(here.daemon.sweep_news(None).await.is_empty());
        assert_eq!(
            lines(here.daemon.sweep(None).await),
            one,
            "riff daemon sweep says it each time"
        );
        assert!(
            here.daemon.news.take("o/r").is_empty(),
            "no note to the lead"
        );

        note_repo(&kept, "o/r").unwrap();
        assert_eq!(
            here.daemon.sweep_news(None).await,
            [text::swept(
                &kept,
                "the pull request #41 closed with no merge"
            )]
        );
        assert!(!kept.exists());
    }

    /// 01M5NVKNPD938ASWY4TD5BGK67: the sweep deletes each branch
    /// `riff-kept/*` older than 30 days, and tells the lead.
    #[tokio::test]
    async fn the_sweep_deletes_the_kept_branches_older_than_30_days() {
        let day = 86_400;
        let now = date_seconds("2026-10-24T12:00:00Z").unwrap();
        let kept = |name: &str, days: i64| KeptBranch {
            name: name.to_owned(),
            at: now - days * day,
        };
        let server = Server::with_fates(&[(1, Fate::Closed { pull: 41 })]).with_kept(vec![
            kept("riff-kept/issue-7-00000007", 31),
            kept("riff-kept/issue-8-00000008", 29),
        ]);
        let (here, server) = with(server).await;
        folder(&here, "issue-1-00000001");

        let swept = here.daemon.sweep(None).await;

        assert_eq!(server.kept_left(), ["riff-kept/issue-8-00000008"]);
        let deleted = text::swept_branch_deleted("riff-kept/issue-7-00000007", 30);
        assert_eq!(swept.last().unwrap().line, deleted);
        let note = text::daemon_kept_digest(std::slice::from_ref(&deleted)).unwrap();
        assert_eq!(here.daemon.news.take("o/r"), [note]);
        let note = here.daemon.sweep_news(None).await;
        assert!(note.is_empty(), "{note:?}");
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
                    digest: None,
                },
                Swept {
                    folder: closed.clone(),
                    line: text::swept(&closed, "the pull request #42 closed with no merge"),
                    kept: false,
                    digest: None,
                },
            ]
        );
    }

    /// 01M6GYN97HH70E4C1FGM6HA2B0 (#1549): the sweep at a look of a wave
    /// asks the forge about a folder that stays one time in [`EVERY`]; the
    /// sweep of the daemon asks each time. A branch with an open pull
    /// request in a reply that a look read makes no call.
    #[tokio::test]
    async fn the_sweep_of_a_wave_asks_the_forge_about_a_folder_that_stays_one_time() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join("log");
        let script = format!(
            r#"#!/bin/sh
echo "$*" >> {}
case "$*" in
'pr list --repo o/r'*) echo '[{{"number":41,"headRefName":"worktree-issue-2","headRefOid":"abc","isDraft":false,"statusCheckRollup":[]}}]' ;;
'pr list --head'*) echo '[]' ;;
'issue view'*) echo '{{"state":"OPEN"}}' ;;
*) echo '{{}}' ;;
esac
"#,
            log.display()
        );
        let gh = pr::Gh::at(isolated::write_program(root.path().join("gh"), &script))
            .with_shared_reads();
        // A look of the wave read the open pull requests.
        rollout::pulls(&gh, "o/r").unwrap();
        let (here, _) = with(Server::with_gh(gh)).await;
        // Issue 1 has no pull request and an open issue. Issue 2 has an open
        // pull request in the reply of the look.
        let (one, two) = (
            folder(&here, "issue-1-00000001"),
            folder(&here, "issue-2-00000002"),
        );
        let asked = |branch: &str| {
            let log = std::fs::read_to_string(&log).unwrap();
            log.lines().filter(|l| l.contains(branch)).count()
        };

        here.daemon.sweep(Some("o/r")).await;
        assert_eq!(asked("--head worktree-issue-1 "), 1);
        assert_eq!(asked("issue view 1 "), 1);
        assert_eq!(asked("worktree-issue-2 --state"), 0, "an open pull request");
        // The second sweep of the wave asks nothing.
        here.daemon.sweep(Some("o/r")).await;
        assert_eq!(asked("--head worktree-issue-1 "), 1);
        // The sweep of the daemon asks again.
        here.daemon.sweep(None).await;
        assert_eq!(asked("--head worktree-issue-1 "), 2);
        assert_eq!(asked("worktree-issue-2 --state"), 0);
        assert!(one.exists() && two.exists());
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
                digest: String::new(),
                name: name.into(),
                session: "9f8e7d6c-0000-4000-8000-000000000000".into(),
                item: "issue-12".into(),
                image: IMAGE.into(),
                since_ms: 0,
                phase: Default::default(),
                last: String::new(),
                lead: false,
                cause: None,
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
            model: None,
            github: None,
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
        note_repo(&theirs, "other/r").unwrap();
        let no_repo = here.daemon.sessions_dir().join("issue-2-00000003");
        std::fs::create_dir_all(no_repo.join("clone")).unwrap();

        let swept = here.daemon.sweep(Some("o/r")).await;
        assert_eq!(swept.len(), 1, "{swept:?}");
        assert!(!mine.exists());
        assert!(theirs.exists() && no_repo.exists());

        let swept = here.daemon.sweep(None).await;
        assert_eq!(
            lines(swept),
            [
                text::swept(&theirs, "the pull request #42 closed with no merge"),
                text::swept_no_repo(&no_repo, OLD_DAYS),
            ]
        );
        assert!(!theirs.exists());
        assert!(
            no_repo.exists(),
            "no file repo: riff does not know its repository"
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

    /// 01M5P26VW5GBJDQZGEYW9A77Y0: `--remove` removes the folder of an
    /// ended session whose pull request merged, with its leftover work.
    /// It refuses an open item with no `--force`, and a bad name.
    #[tokio::test]
    async fn remove_takes_one_folder_on_purpose_with_a_merged_branch_or_force() {
        let (here, _) = here(&[
            (
                1,
                Fate::Merged {
                    pull: 7,
                    commit: "abc".into(),
                    refs: false,
                },
            ),
            (4, Fate::Stays),
        ])
        .await;
        let merged = folder(&here, "issue-1-00000001");
        std::fs::write(merged.join("clone/leftover.md"), "work\n").unwrap();
        let open = folder(&here, "issue-4-00000004");

        let mut said = Vec::new();
        let ask = |name: &str, force| Ask::Remove {
            name: name.into(),
            force,
        };
        ask_done(&here.dir, ask("issue-1-00000001", false), |l| {
            said.push(l.to_owned())
        })
        .await
        .unwrap();
        assert_eq!(
            said,
            [text::folder_removed(&merged, "the pull request #7 merged")]
        );
        assert!(!merged.exists());

        let err = ask_done(&here.dir, ask("issue-4-00000004", false), |_| {})
            .await
            .unwrap_err();
        assert!(format!("{err:#}").contains("--force"), "{err:#}");
        assert!(open.exists());
        let err = ask_done(&here.dir, ask("../issue-4-00000004", true), |_| {})
            .await
            .unwrap_err();
        assert!(format!("{err:#}").contains("ITEM-ID8"), "{err:#}");
        assert!(open.exists());

        ask_done(&here.dir, ask("issue-4-00000004", true), |_| {})
            .await
            .unwrap();
        assert!(!open.exists());
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

    /// #2138 (01M7DZ429QFWTSEA6JH1DVZJM0): the folder of a session whose
    /// git config names a program for `core.fsmonitor`, `credential.helper`
    /// and `core.sshCommand`. The sweep, the look for earlier work and the
    /// read of the council run, and that program never runs: each of them
    /// runs git in the main clone, or in no clone. The guard refuses a git
    /// call in that folder, and names it.
    #[tokio::test]
    async fn no_step_of_the_host_runs_a_program_of_the_git_config_of_a_session() {
        let (here, _) = here(&[(12, Fate::Stays)]).await;
        // `origin`: `main` with a member file of the council, and the
        // pushed branch of issue-12. The main clone of the daemon clones it.
        let root = here.root.path();
        let seed = root.join("seed");
        std::fs::create_dir_all(seed.join(".riff/council")).unwrap();
        git(&seed, &["init", "-q", "-b", "main"]);
        std::fs::write(seed.join(".riff/council/big-o.md"), "+++\n+++\n# Big O\n").unwrap();
        git(&seed, &["add", "."]);
        git(&seed, &["commit", "-q", "-m", "start"]);
        git(&seed, &["switch", "-q", "-c", "worktree-issue-12"]);
        git(&seed, &["commit", "-q", "--allow-empty", "-m", "WIP: one"]);
        git(root, &["clone", "-q", "--bare", "seed", "origin.git"]);
        git(root, &["clone", "-q", "origin.git", "main"]);
        let main = root.join("main");
        // The folder of a session of issue-12 that ended: its clone has a
        // member file of its own in the work tree.
        let folder = folder(&here, "issue-12-1a2b3c4d");
        let clone = folder.join("clone");
        std::fs::create_dir_all(clone.join(".riff/council")).unwrap();
        std::fs::write(clone.join(".riff/council/evil.md"), "+++\n+++\n# Evil\n").unwrap();

        // The sweep: the forge says that the folder stays.
        assert!(here.daemon.sweep(None).await.is_empty());
        assert!(folder.exists());

        // The look for earlier work runs in the main clone: in the folder
        // of the session it runs no git.
        assert_eq!(crate::dropped::at_claim(&clone, "issue-12").await, None);
        let line = crate::dropped::at_claim(&main, "issue-12").await.unwrap();
        assert!(
            line.contains("the pushed branch origin/worktree-issue-12"),
            "{line}"
        );
        let next = here.daemon.sessions_dir().join("issue-12-2b3c4d5e/clone");
        assert!(run::make_clone(&main, &next, "worktree-issue-12").unwrap());
        assert_eq!(
            git(&next, &["rev-parse", "--abbrev-ref", "HEAD"]),
            "worktree-issue-12"
        );
        let new = run::new_clone_of(&next).unwrap();
        assert!(!new.exists(), "the new clone moved into the folder");

        // The council of a verify reads the member files of `origin/main`
        // in the main clone.
        let script = r#"#!/bin/sh
case "$*" in
  "pr view 40 --json baseRefName,body,labels")
    printf '%s' '{"baseRefName":"main","body":"Closes #12\n","labels":[]}' ;;
  "api repos/{owner}/{repo}/pulls/40/files?per_page=100 --paginate --jq "*)
    printf 'src/a.rs\n' ;;
  "issue view 12 --json labels") printf '%s' '{"labels":[]}' ;;
  *) exit 1 ;;
esac
"#;
        let gh = pr::Gh::at(isolated::write_program(root.join("gh"), script));
        let (council, branch) =
            crate::council::pick::of_pull(&gh, &main, 40, |line| panic!("{line}")).unwrap();
        assert_eq!(branch, "origin/main");
        let names: Vec<&str> = council
            .seats
            .iter()
            .map(|seat| seat.member.name.as_str())
            .collect();
        assert!(
            names.contains(&"big-o") && !names.contains(&"evil"),
            "{names:?}"
        );

        // The guard refuses each git call in the folder of the session.
        let error = crate::host_git::output(&clone, &["status"]).unwrap_err();
        let error = format!("{error:#}");
        assert!(error.contains(&clone.display().to_string()), "{error}");
        let link = root.join("link");
        std::os::unix::fs::symlink(&clone, &link).unwrap();
        assert!(crate::host_git::output(&link, &["status"]).is_err());

        assert!(folder.exists());
        assert_eq!(marks(&here), Vec::<PathBuf>::new());
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
