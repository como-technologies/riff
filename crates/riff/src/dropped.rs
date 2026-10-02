//! The earlier work on an item: its pushed branch and its worktree.
//!
//! # Design
//!
//! A session can end at each moment, for example when the machine has
//! no memory left. So a session pushes its work as WIP commits
//! (01M3WFYEKTWVVZ1FWVNQMGBNN0), and the next session that claims the
//! item goes on from the branch (01M3WFYEP1H3VPW8G90KQDE6FW). riff shows
//! that work, so that the session does not have to look for it:
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as session a1 (pangolin)
//!     participant O as origin
//!     participant S as riff-server
//!     participant B as session b2 (thelio)
//!     A->>S: claim issue-12
//!     A->>O: push worktree-issue-12 (WIP commits)
//!     Note over A: killed
//!     S->>S: the claim is free
//!     B->>S: claim issue-12
//!     B->>O: git fetch --prune origin
//!     B->>B: "Earlier work on issue-12: the pushed branch ..."
//!     B->>B: goes on from origin/worktree-issue-12
//! ```
//!
//! | Where | What riff shows |
//! |---|---|
//! | The answer to a granted claim ([`at_claim`]) | The earlier work on that item (01M3WFYER9QWA698KY2E1HNTCW). |
//! | The context of a new start ([`start_lines`]) | The earlier work of the clone that no live session owns (01M3WFYETKXPWWE0R0EAKGCD1E). |
//!
//! - A pushed branch is a branch of `origin`. riff reads the
//!   remote-tracking branches of the clone, so it fetches first, for at
//!   most [`FETCH_WAIT`] at a claim. With no remote, or a slow one, it
//!   shows what the clone knows.
//! - A worktree is a linked worktree of the clone on this machine. riff
//!   counts its files that are not committed, and its commits that are
//!   on no branch of `origin`.
//! - A branch or a worktree belongs to an item when its name holds the
//!   item as a whole word ([`is_of`]): `worktree-issue-12` and
//!   `worktree-issue-12-b` for `issue-12`, not `worktree-issue-123`.
//! - A verify worktree holds no work, so riff shows nothing for a
//!   `verify-` name.
//! - A session wrote the subject of a commit, so riff does not show
//!   it. It shows the commit, its age, and whether it is a WIP commit.
//!
//! ```
//! use riff::dropped::{Earlier, Kept, Pushed, is_of};
//!
//! assert!(is_of("worktree-issue-12", "issue-12"));
//! assert!(!is_of("worktree-issue-123", "issue-12"));
//! let earlier = Earlier {
//!     item: "issue-12".into(),
//!     pushed: vec![Pushed {
//!         name: "origin/worktree-issue-12".into(),
//!         commit: "1a2b3c4".into(),
//!         age: "2 hours ago".into(),
//!         wip: true,
//!     }],
//!     kept: vec![Kept { path: "/src/riff/.claude/worktrees/issue-12".into(), changed: 3, not_pushed: 1 }],
//! };
//! assert_eq!(
//!     earlier.text(),
//!     "issue-12: the pushed branch origin/worktree-issue-12 at 1a2b3c4 (a WIP commit, 2 hours ago); \
//!      the worktree /src/riff/.claude/worktrees/issue-12 (3 files not committed, 1 commit not pushed)"
//! );
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use riff_core::name::SessionUri;
use riff_core::wire::SessionInfo;

/// The longest wait of a claim for `git fetch`
/// (01M3WFYER9QWA698KY2E1HNTCW).
pub const FETCH_WAIT: Duration = Duration::from_secs(5);

/// The most items that the start context lists
/// (01M3WFYETKXPWWE0R0EAKGCD1E).
pub const SHOWN: usize = 8;

/// The prefix of the branch that the agent tool makes for a worktree.
const BRANCH: &str = "worktree-";

/// A branch of `origin` with earlier work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pushed {
    /// The remote-tracking branch, for example `origin/worktree-issue-12`.
    pub name: String,
    /// The short ID of its last commit.
    pub commit: String,
    /// The age of that commit, as git says it: `2 hours ago`.
    pub age: String,
    /// True when the subject of that commit has `WIP`.
    pub wip: bool,
}

/// A linked worktree on this machine with earlier work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    /// The path of the worktree.
    pub path: PathBuf,
    /// How many files are not committed.
    pub changed: usize,
    /// How many commits are on no branch of `origin`.
    pub not_pushed: usize,
}

/// The earlier work on one item.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Earlier {
    /// The item, for example `issue-12`.
    pub item: String,
    /// Each pushed branch of the item.
    pub pushed: Vec<Pushed>,
    /// Each worktree of the item on this machine.
    pub kept: Vec<Kept>,
}

impl Earlier {
    /// The item and its work, in one line with no period at the end.
    pub fn text(&self) -> String {
        let count = |n: usize, one: &str| match n {
            1 => format!("1 {one}"),
            n => format!("{n} {one}s"),
        };
        let pushed = self.pushed.iter().map(|p| {
            let kind = if p.wip { "a WIP commit, " } else { "" };
            format!(
                "the pushed branch {} at {} ({kind}{})",
                p.name, p.commit, p.age
            )
        });
        let kept = self.kept.iter().map(|k| {
            let state = match (k.changed, k.not_pushed) {
                (0, 0) => "each file committed and pushed".to_owned(),
                (changed, 0) => format!("{} not committed", count(changed, "file")),
                (0, ahead) => format!("{} not pushed", count(ahead, "commit")),
                (changed, ahead) => format!(
                    "{} not committed, {} not pushed",
                    count(changed, "file"),
                    count(ahead, "commit")
                ),
            };
            format!("the worktree {} ({state})", k.path.display())
        });
        let parts: Vec<String> = pushed.chain(kept).collect();
        format!("{}: {}", self.item, parts.join("; "))
    }

    /// The line after the answer to a granted claim
    /// (01M3WFYER9QWA698KY2E1HNTCW).
    ///
    /// ```
    /// use riff::dropped::{Earlier, Kept};
    ///
    /// let earlier = Earlier {
    ///     item: "issue-12".into(),
    ///     pushed: vec![],
    ///     kept: vec![Kept { path: "/w/issue-12".into(), changed: 0, not_pushed: 0 }],
    /// };
    /// assert_eq!(
    ///     earlier.claim_line(),
    ///     "Earlier work on issue-12: the worktree /w/issue-12 (each file committed and pushed). \
    ///      Go on from it, and do not start again: see \"Pick up dropped work\" in the riff skill."
    /// );
    /// ```
    pub fn claim_line(&self) -> String {
        format!(
            "Earlier work on {}. Go on from it, and do not start again: see \"Pick up dropped \
             work\" in the riff skill.",
            self.text()
        )
    }

    fn is_empty(&self) -> bool {
        self.pushed.is_empty() && self.kept.is_empty()
    }
}

/// True when the branch or worktree `name` belongs to `item`: the name
/// holds the item as a whole word. A `verify-` name belongs to no item.
///
/// ```
/// use riff::dropped::is_of;
///
/// assert!(is_of("origin/worktree-issue-12", "issue-12"));
/// assert!(is_of("issue-12-b", "issue-12"));
/// assert!(is_of("mike/issue-12", "issue-12"));
/// assert!(!is_of("worktree-issue-123", "issue-12"));
/// assert!(!is_of("worktree-reissue-12", "issue-12"));
/// assert!(!is_of("worktree-verify-issue-12-a6cf", "issue-12"));
/// assert!(!is_of("worktree-issue-12", ""));
/// ```
pub fn is_of(name: &str, item: &str) -> bool {
    if item.is_empty() || name.contains("verify-") {
        return false;
    }
    name.match_indices(item).any(|(at, _)| {
        let before = name[..at].chars().next_back();
        let after = name[at + item.len()..].chars().next();
        before.is_none_or(|c| c == '-' || c == '/') && after.is_none_or(|c| c == '-')
    })
}

/// The item of a worktree branch, or `None` for another branch and for
/// a verify worktree.
///
/// ```
/// use riff::dropped::item_of;
///
/// assert_eq!(item_of("origin/worktree-issue-12"), Some("issue-12"));
/// assert_eq!(item_of("worktree-issue-12"), Some("issue-12"));
/// assert_eq!(item_of("origin/main"), None);
/// assert_eq!(item_of("worktree-verify-issue-12-a6cf"), None);
/// ```
pub fn item_of(branch: &str) -> Option<&str> {
    let name = branch.strip_prefix("origin/").unwrap_or(branch);
    name.strip_prefix(BRANCH)
        .filter(|item| !item.is_empty() && !item.starts_with("verify-"))
}

/// Runs `git fetch --prune origin` in the clone of `dir` for at most
/// `wait`. False when the fetch fails or takes longer, for example with
/// no remote.
pub async fn fetch(dir: &Path, wait: Duration) -> bool {
    let fetch = tokio::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["fetch", "--quiet", "--prune", "origin"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn();
    let Ok(mut fetch) = fetch else {
        return false;
    };
    matches!(
        tokio::time::timeout(wait, fetch.wait()).await,
        Ok(Ok(status)) if status.success()
    )
}

/// The earlier work on `item` in the clone of `dir`, or `None` when
/// there is none, and for a verify claim.
pub fn find(dir: &Path, item: &str) -> Option<Earlier> {
    let earlier = Earlier {
        item: item.to_owned(),
        pushed: pushed(dir)
            .into_iter()
            .filter(|p| is_of(&p.name, item))
            .collect(),
        kept: worktrees(dir)
            .into_iter()
            .filter(|w| is_of(&w.name, item) || is_of(&w.branch, item))
            .map(|w| w.kept())
            .collect(),
    };
    (!earlier.is_empty()).then_some(earlier)
}

/// The line after the answer to a granted claim of `item`, when the
/// item has earlier work (01M3WFYER9QWA698KY2E1HNTCW). It fetches
/// first, for at most [`FETCH_WAIT`].
pub async fn at_claim(dir: &Path, item: &str) -> Option<String> {
    if item.starts_with("verify-") {
        return None;
    }
    fetch(dir, FETCH_WAIT).await;
    let (dir, item) = (dir.to_owned(), item.to_owned());
    tokio::task::spawn_blocking(move || find(&dir, &item))
        .await
        .ok()?
        .map(|earlier| earlier.claim_line())
}

/// The earlier work of the clone of `dir`, by item: each worktree
/// branch of `origin`, and each linked worktree. It does not fetch.
pub fn all(dir: &Path) -> Vec<Earlier> {
    let mut items: BTreeMap<String, Earlier> = BTreeMap::new();
    for p in pushed(dir) {
        if let Some(item) = item_of(&p.name).map(str::to_owned) {
            items.entry(item).or_default().pushed.push(p);
        }
    }
    for w in worktrees(dir) {
        let item = match item_of(&w.branch) {
            Some(item) => item.to_owned(),
            None if w.name.starts_with("verify-") => continue,
            None => w.name.clone(),
        };
        items.entry(item).or_default().kept.push(w.kept());
    }
    items
        .into_iter()
        .map(|(item, earlier)| Earlier { item, ..earlier })
        .collect()
}

/// The part of `all` that no live session owns. A live session owns
/// the work on an item when it holds the claim of the item, or when it
/// works in a worktree of the item on this machine. `me` owns nothing:
/// a new start is blank.
///
/// ```
/// use riff::dropped::{Earlier, Kept, without_owner};
/// use riff_core::wire::SessionInfo;
///
/// let info = |uri: &str, live| SessionInfo { uri: uri.parse().unwrap(), live, idle_secs: 0, status: None, worker: false, stopping: false, claims_secs: 0, must_clear: false, fresh_secs: None, state: Default::default() };
/// let work = |item: &str| Earlier {
///     item: item.into(),
///     pushed: vec![],
///     kept: vec![Kept { path: format!("/w/{item}").into(), changed: 1, not_pushed: 0 }],
/// };
/// let me: riff_core::name::SessionUri = "riff://mike@pangolin/o/r?session=a1".parse()?;
/// let who = [
///     info("riff://mike@pangolin/o/r?session=a1", true),
///     info("riff://mike@thelio/o/r?session=b2&claim=issue-12", true),
///     info("riff://mike@pangolin/o/r?session=c3#issue-13", true),
///     info("riff://mike@pangolin/o/r?session=d4&claim=issue-14#issue-14", false),
/// ];
/// let all = vec![work("issue-12"), work("issue-13"), work("issue-14")];
/// let free = without_owner(all, &me, &who);
/// assert_eq!(free.len(), 1);
/// assert_eq!(free[0].item, "issue-14");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn without_owner(all: Vec<Earlier>, me: &SessionUri, who: &[SessionInfo]) -> Vec<Earlier> {
    let owns = |s: &SessionInfo, earlier: &Earlier| {
        let here =
            s.uri.place().host() == me.place().host() && s.uri.place().repo() == me.place().repo();
        let in_worktree = || {
            s.uri.place().worktree().is_some_and(|worktree| {
                earlier
                    .kept
                    .iter()
                    .any(|k| k.path.file_name().is_some_and(|name| name == worktree))
            })
        };
        s.uri.claims().iter().any(|c| is_of(&earlier.item, c)) || (here && in_worktree())
    };
    all.into_iter()
        .filter(|earlier| {
            !who.iter()
                .any(|s| s.live && s.uri.who() != me.who() && owns(s, earlier))
        })
        .collect()
}

/// The lines of the start context for the earlier work that no live
/// session owns, or `None` when there is none
/// (01M3WFYETKXPWWE0R0EAKGCD1E). It lists at most [`SHOWN`] items.
///
/// ```
/// use riff::dropped::{Earlier, Kept, start_lines};
///
/// let earlier = Earlier {
///     item: "issue-12".into(),
///     pushed: vec![],
///     kept: vec![Kept { path: "/w/issue-12".into(), changed: 2, not_pushed: 0 }],
/// };
/// let lines = start_lines(&[earlier]).unwrap();
/// assert!(lines.starts_with("- This clone has earlier work that no live session owns."));
/// assert!(lines.contains("Pick up dropped work"));
/// assert!(lines.ends_with("  - issue-12: the worktree /w/issue-12 (2 files not committed)\n"));
/// assert_eq!(start_lines(&[]), None);
/// ```
pub fn start_lines(free: &[Earlier]) -> Option<String> {
    if free.is_empty() {
        return None;
    }
    let mut out = String::from(
        "- This clone has earlier work that no live session owns. When you claim one of these \
         items, go on from that work, and do not start again: see \"Pick up dropped work\" in \
         the riff skill.\n",
    );
    for earlier in free.iter().take(SHOWN) {
        out.push_str(&format!("  - {}\n", earlier.text()));
    }
    if free.len() > SHOWN {
        out.push_str(&format!("  - and {} more\n", free.len() - SHOWN));
    }
    Some(out)
}

/// A linked worktree of the clone.
struct Worktree {
    path: PathBuf,
    /// The last part of the path.
    name: String,
    /// The short name of its branch, or empty for a detached `HEAD`.
    branch: String,
}

impl Worktree {
    fn kept(self) -> Kept {
        let changed =
            git(&self.path, &["status", "--porcelain"]).map_or(0, |out| out.lines().count());
        let ahead = ["rev-list", "--count", "HEAD", "--not", "--remotes=origin"];
        let not_pushed = git(&self.path, &ahead)
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or(0);
        Kept {
            path: self.path,
            changed,
            not_pushed,
        }
    }
}

/// Each branch of `origin` that the clone of `dir` knows.
fn pushed(dir: &Path) -> Vec<Pushed> {
    let format =
        "--format=%(refname:short)%09%(objectname:short)%09%(committerdate:relative)%09%(subject)";
    let Some(out) = git(dir, &["for-each-ref", format, "refs/remotes/origin"]) else {
        return Vec::new();
    };
    out.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(4, '\t');
            let (name, commit, age) = (parts.next()?, parts.next()?, parts.next()?);
            let subject = parts.next().unwrap_or_default();
            (name != "origin/HEAD" && name != "origin").then(|| Pushed {
                name: name.to_owned(),
                commit: commit.to_owned(),
                age: age.to_owned(),
                wip: subject.contains("WIP"),
            })
        })
        .collect()
}

/// Each linked worktree of the clone of `dir` whose directory is there.
/// The main worktree is not in the list.
fn worktrees(dir: &Path) -> Vec<Worktree> {
    let Some(out) = git(dir, &["worktree", "list", "--porcelain"]) else {
        return Vec::new();
    };
    out.split("\n\n")
        .skip(1)
        .filter_map(|block| {
            let field = |name: &str| {
                block
                    .lines()
                    .find_map(|l| l.strip_prefix(name).map(str::trim))
            };
            if block.lines().any(|l| l.starts_with("prunable")) {
                return None;
            }
            let path = PathBuf::from(field("worktree ")?);
            let name = path.file_name()?.to_string_lossy().into_owned();
            let branch = field("branch refs/heads/").unwrap_or_default().to_owned();
            Some(Worktree { path, name, branch })
        })
        .collect()
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
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
    }

    /// A bare `origin` with one commit in `main`, and a clone of it.
    fn clone(root: &Path) -> PathBuf {
        run(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
        run(root, &["clone", "-q", "origin.git", "clone"]);
        let clone = root.join("clone").canonicalize().unwrap();
        run(&clone, &["commit", "-q", "--allow-empty", "-m", "start"]);
        run(&clone, &["push", "-q", "origin", "HEAD:main"]);
        clone
    }

    /// The linked worktree `.claude/worktrees/NAME` on a new branch
    /// `worktree-NAME`.
    fn worktree(clone: &Path, name: &str) -> PathBuf {
        let path = clone.join(".claude/worktrees").join(name);
        let branch = format!("{BRANCH}{name}");
        run(
            clone,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                &branch,
                path.to_str().unwrap(),
            ],
        );
        path
    }

    #[test]
    fn a_clone_with_no_work_has_no_earlier_work() {
        let root = tempfile::tempdir().unwrap();
        let clone = clone(root.path());
        assert_eq!(find(&clone, "issue-12"), None);
        assert_eq!(all(&clone), []);
        assert_eq!(find(root.path(), "issue-12"), None, "not in git");
    }

    #[test]
    fn find_gives_the_pushed_branch_and_the_worktree_of_the_item() {
        let root = tempfile::tempdir().unwrap();
        let clone = clone(root.path());
        let path = worktree(&clone, "issue-12");
        run(&path, &["commit", "-q", "--allow-empty", "-m", "WIP: one"]);
        run(&path, &["push", "-q", "-u", "origin", "HEAD"]);
        run(&path, &["commit", "-q", "--allow-empty", "-m", "two"]);
        std::fs::write(path.join("a.txt"), "a").unwrap();
        std::fs::write(path.join("b.txt"), "b").unwrap();
        worktree(&clone, "issue-123");

        let earlier = find(&clone, "issue-12").unwrap();
        assert_eq!(earlier.pushed.len(), 1, "{earlier:?}");
        assert_eq!(earlier.pushed[0].name, "origin/worktree-issue-12");
        assert!(earlier.pushed[0].wip);
        assert_eq!(
            earlier.kept,
            [Kept {
                path,
                changed: 2,
                not_pushed: 1
            }]
        );
        let line = earlier.claim_line();
        assert!(
            line.contains("(2 files not committed, 1 commit not pushed)"),
            "{line}"
        );
    }

    #[test]
    fn all_groups_the_work_by_item_and_skips_a_verify_worktree() {
        let root = tempfile::tempdir().unwrap();
        let clone = clone(root.path());
        let path = worktree(&clone, "issue-12");
        run(&path, &["commit", "-q", "--allow-empty", "-m", "done"]);
        run(&path, &["push", "-q", "-u", "origin", "HEAD"]);
        worktree(&clone, "issue-7");
        worktree(&clone, "verify-issue-9-a6cf");

        let all = all(&clone);
        let items: Vec<&str> = all.iter().map(|e| e.item.as_str()).collect();
        assert_eq!(items, ["issue-12", "issue-7"]);
        assert_eq!(all[0].pushed.len(), 1);
        assert!(!all[0].pushed[0].wip);
        assert_eq!(all[0].kept.len(), 1);
        assert!(all[1].pushed.is_empty());
        assert_eq!(find(&clone, "verify-issue-9"), None);
    }

    #[test]
    fn a_worktree_whose_directory_is_gone_is_not_listed() {
        let root = tempfile::tempdir().unwrap();
        let clone = clone(root.path());
        let path = worktree(&clone, "issue-12");
        std::fs::remove_dir_all(&path).unwrap();
        assert_eq!(find(&clone, "issue-12"), None);
    }

    #[test]
    fn the_start_lines_list_at_most_the_limit() {
        let work = |n: usize| Earlier {
            item: format!("issue-{n}"),
            pushed: vec![Pushed {
                name: format!("origin/worktree-issue-{n}"),
                commit: "1a2b3c4".into(),
                age: "1 hour ago".into(),
                wip: false,
            }],
            kept: vec![],
        };
        let free: Vec<Earlier> = (0..SHOWN + 2).map(work).collect();
        let lines = start_lines(&free).unwrap();
        assert_eq!(lines.lines().count(), 1 + SHOWN + 1, "{lines}");
        assert!(lines.ends_with("  - and 2 more\n"), "{lines}");
        assert!(
            lines.contains("the pushed branch origin/worktree-issue-0 at 1a2b3c4 (1 hour ago)")
        );
    }

    #[tokio::test]
    async fn a_fetch_with_no_remote_fails_and_a_claim_still_looks() {
        let root = tempfile::tempdir().unwrap();
        run(root.path(), &["init", "-q", "-b", "main"]);
        run(
            root.path(),
            &["commit", "-q", "--allow-empty", "-m", "start"],
        );
        assert!(!fetch(root.path(), FETCH_WAIT).await);
        let path = worktree(root.path(), "issue-12");
        let line = at_claim(root.path(), "issue-12").await.unwrap();
        assert!(line.contains(&path.display().to_string()), "{line}");
        assert_eq!(at_claim(root.path(), "verify-issue-12").await, None);
    }
}
