//! `riff worktrees clean`: riff tidies the worktrees of a clone by
//! facts, not by the word of an agent.
//!
//! # Design
//!
//! A session that ends leaves its worktree, often with a lock of the
//! agent tool. The auto mode check refuses a raw `git worktree unlock`
//! or `git worktree remove` of an agent, and it is right to: the agent
//! cannot prove that the worktree is free. riff proves it in code
//! (01M3ZV0TKSHNW5QC2NG1XTJEJB). For each linked worktree:
//!
//! ```mermaid
//! flowchart TD
//!     L{"a lock whose process<br/>is gone?"} -- yes --> U["unlock"]
//!     L -- no --> O
//!     U --> O{"a live owner: a lock of a live process,<br/>a live session in it, or this process?"}
//!     O -- yes --> K["keep"]
//!     O -- no --> D{"work that is not committed?"}
//!     D -- "yes, on a branch" --> W["WIP commit, push,<br/>a note to the lead"]
//!     D -- "yes, detached" --> K
//!     D -- no --> M{"the pull request of the branch is merged,<br/>and its head is HEAD?<br/>detached: HEAD is on origin?"}
//!     M -- yes --> R["remove the worktree<br/>and its branch"]
//!     M -- no --> K
//! ```
//!
//! - riff acts only on a worktree of the agent tool, in
//!   `.claude/worktrees` of the main worktree ([`AGENT_DIR`]). A person
//!   made each other worktree.
//! - A lock of the agent tool names its process: `(pid N start S)`. S
//!   is the start of the process in clock ticks after the boot. A lock
//!   is dead when no process has the ID N, or when the process N has
//!   another start: a new process, or a new boot. A lock with no
//!   process ID is of a person: riff keeps it ([`Lock::of`]).
//! - A live session owns a worktree when `riff who` shows it live, on
//!   this host, in this repository, in that worktree.
//! - riff deletes the branch only while it points at HEAD
//!   (`git update-ref -d`). It never forces.
//! - `riff workers start` and the start of `riff workers host` run it
//!   too (01M3ZV0TM7ANJ1QQ7XTBDJQE1V). A workers host and the `riff mcp`
//!   of the lead run it each 10 minutes ([`crate::tidy`],
//!   01M41A118QPQKFAAHGQFFX4F3B).
//!
//! ```
//! use riff::worktrees::{Facts, Lock, Pr, decide, Step};
//!
//! let facts = Facts { agent: true, lock: Lock::Dead, owned: false, changed: false, branch: true, pr: Some(Pr::MergedAtHead(40)), on_origin: true };
//! assert_eq!(decide(&facts), [Step::Unlock, Step::Remove]);
//! let facts = Facts { agent: true, lock: Lock::Live, ..facts };
//! assert_eq!(decide(&facts), [Step::Keep("a live process holds its lock".into())]);
//! ```

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::Result;
use serde::Deserialize;

use crate::pr::Gh;
use crate::workload;

/// The directory of the worktrees of the agent tool in the main
/// worktree.
pub const AGENT_DIR: &str = ".claude/worktrees";

/// The lock of a worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lock {
    /// No lock.
    None,
    /// A lock of a process that lives.
    Live,
    /// A lock of a process that is gone.
    Dead,
    /// A lock with no process ID: of a person.
    Person,
}

impl Lock {
    /// The lock with the reason `reason`, or [`Lock::None`]. `start`
    /// gives the start of a process, or `None` when it is gone.
    ///
    /// ```
    /// use riff::worktrees::Lock;
    ///
    /// let start = |pid| (pid == 14704).then_some(15813);
    /// assert_eq!(Lock::of(Some("claude session issue-12 (pid 14704 start 15813)"), start), Lock::Live);
    /// assert_eq!(Lock::of(Some("claude session issue-12 (pid 14704 start 99)"), start), Lock::Dead);
    /// assert_eq!(Lock::of(Some("claude session issue-12 (pid 777 start 15813)"), start), Lock::Dead);
    /// assert_eq!(Lock::of(Some("claude agent a1 (pid 14704)"), start), Lock::Live);
    /// assert_eq!(Lock::of(Some("on a USB disk"), start), Lock::Person);
    /// assert_eq!(Lock::of(Some(""), start), Lock::Person);
    /// assert_eq!(Lock::of(None, start), Lock::None);
    /// ```
    pub fn of(reason: Option<&str>, start: impl Fn(u32) -> Option<u64>) -> Lock {
        let Some(reason) = reason else {
            return Lock::None;
        };
        let Some(at) = reason.rfind("(pid ") else {
            return Lock::Person;
        };
        let inner = reason[at + 5..].trim_end_matches(')');
        let mut words = inner.split_whitespace();
        let Some(pid) = words.next().and_then(|p| p.parse::<u32>().ok()) else {
            return Lock::Person;
        };
        let locked_start = match (words.next(), words.next()) {
            (Some("start"), Some(s)) => s.parse::<u64>().ok(),
            _ => None,
        };
        match (start(pid), locked_start) {
            (None, _) => Lock::Dead,
            (Some(now), Some(then)) if now != then => Lock::Dead,
            _ => Lock::Live,
        }
    }
}

/// The pull request of the branch of a worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pr {
    /// Merged, and its head is the `HEAD` of the worktree.
    MergedAtHead(u64),
    /// Merged, with another head.
    MergedElsewhere(u64),
    /// Open or closed, not merged.
    NotMerged(u64),
    /// No pull request, or `gh` failed.
    Unknown(String),
}

/// The facts of one worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// It is a worktree of an agent tool: in `.claude/worktrees` of the
    /// main worktree. riff keeps each other worktree: a person made it.
    pub agent: bool,
    pub lock: Lock,
    /// A live session works in it, or this process does.
    pub owned: bool,
    /// It has files that are not committed.
    pub changed: bool,
    /// It is on a branch, not detached.
    pub branch: bool,
    /// The pull request of its branch. `None` when riff did not look.
    pub pr: Option<Pr>,
    /// Its `HEAD` is on a branch of `origin`.
    pub on_origin: bool,
}

/// One step for a worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Unlock,
    Remove,
    /// A WIP commit of the work, and a push of the branch.
    Save,
    /// Keep the worktree, for this reason.
    Keep(String),
}

/// The steps for a worktree with `facts` (01M3ZV0TKSHNW5QC2NG1XTJEJB).
///
/// ```
/// use riff::worktrees::{Facts, Lock, Pr, decide, Step};
///
/// let free = Facts { agent: true, lock: Lock::None, owned: false, changed: false, branch: true, pr: None, on_origin: false };
/// let changed = Facts { changed: true, ..free.clone() };
/// assert_eq!(decide(&changed), [Step::Save]);
/// let detached = Facts { branch: false, ..changed.clone() };
/// assert_eq!(decide(&detached), [Step::Keep("it has work on no branch".into())]);
/// let verify = Facts { branch: false, on_origin: true, ..free.clone() };
/// assert_eq!(decide(&verify), [Step::Remove]);
/// let open = Facts { pr: Some(Pr::NotMerged(41)), ..free.clone() };
/// assert_eq!(decide(&open), [Step::Keep("pull request #41 is not merged".into())]);
/// let owned = Facts { owned: true, lock: Lock::Dead, ..free };
/// assert_eq!(decide(&owned), [Step::Unlock, Step::Keep("a live session works in it".into())]);
/// ```
pub fn decide(facts: &Facts) -> Vec<Step> {
    if !facts.agent {
        return vec![Step::Keep(
            "it is not a worktree of an agent session".into(),
        )];
    }
    let mut steps = Vec::new();
    match facts.lock {
        Lock::Dead => steps.push(Step::Unlock),
        Lock::Live => return vec![Step::Keep("a live process holds its lock".into())],
        Lock::Person => return vec![Step::Keep("a person locked it".into())],
        Lock::None => {}
    }
    let last = if facts.owned {
        Step::Keep("a live session works in it".into())
    } else if facts.changed && facts.branch {
        Step::Save
    } else if facts.changed {
        Step::Keep("it has work on no branch".into())
    } else if !facts.branch {
        if facts.on_origin {
            Step::Remove
        } else {
            Step::Keep("its commit is on no branch of origin".into())
        }
    } else {
        match &facts.pr {
            Some(Pr::MergedAtHead(_)) => Step::Remove,
            Some(Pr::MergedElsewhere(n)) => {
                Step::Keep(format!("its HEAD is not the head of pull request #{n}"))
            }
            Some(Pr::NotMerged(n)) => Step::Keep(format!("pull request #{n} is not merged")),
            Some(Pr::Unknown(why)) => Step::Keep(why.clone()),
            None => Step::Keep("riff found no pull request".into()),
        }
    };
    steps.push(last);
    steps
}

/// A linked worktree of a clone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    pub path: PathBuf,
    /// The short name of its branch, or `None` when it is detached.
    pub branch: Option<String>,
    /// The commit of its `HEAD`.
    pub head: String,
    /// The reason of its lock, when it has one.
    pub lock: Option<String>,
}

/// The linked worktrees in the text of `git worktree list --porcelain`.
/// It skips the main worktree and each worktree whose directory is gone.
///
/// ```
/// let text = "worktree /r\nHEAD 1111\nbranch refs/heads/main\n\n\
///     worktree /r/.claude/worktrees/issue-12\nHEAD 2222\nbranch refs/heads/worktree-issue-12\n\
///     locked claude session issue-12 (pid 5 start 6)\n\n\
///     worktree /r/.claude/worktrees/verify-issue-9-a6cf\nHEAD 3333\ndetached\n\n\
///     worktree /gone\nHEAD 4444\ndetached\nprunable gitdir file points to non-existent location\n";
/// let trees = riff::worktrees::parse(text);
/// assert_eq!(trees.len(), 2);
/// assert_eq!(trees[0].branch.as_deref(), Some("worktree-issue-12"));
/// assert_eq!(trees[0].lock.as_deref(), Some("claude session issue-12 (pid 5 start 6)"));
/// assert_eq!(trees[1].branch, None);
/// assert_eq!(trees[1].head, "3333");
/// ```
pub fn parse(text: &str) -> Vec<Tree> {
    text.split("\n\n")
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
            let lock = block.lines().find_map(|l| {
                (l == "locked")
                    .then(String::new)
                    .or_else(|| l.strip_prefix("locked ").map(str::to_owned))
            });
            Some(Tree {
                path: PathBuf::from(field("worktree ")?),
                branch: field("branch refs/heads/").map(str::to_owned),
                head: field("HEAD ")?.to_owned(),
                lock,
            })
        })
        .collect()
}

/// What riff did with one worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub path: PathBuf,
    /// The line to print.
    pub line: String,
    /// True when riff saved work that no live session owns: the lead
    /// gets a note.
    pub saved: bool,
}

/// The pull request of `branch`, as `gh pr view --json` gives it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrView {
    number: u64,
    state: String,
    head_ref_oid: String,
}

/// The pull request of `branch` with `gh`, compared with `head`.
fn pr_of(gh: &Gh, branch: &str, head: &str) -> Pr {
    let args = ["pr", "view", branch, "--json", "number,state,headRefOid"];
    match gh.json::<PrView>(&args) {
        Ok(pr) if pr.state == "MERGED" && pr.head_ref_oid == head => Pr::MergedAtHead(pr.number),
        Ok(pr) if pr.state == "MERGED" => Pr::MergedElsewhere(pr.number),
        Ok(pr) => Pr::NotMerged(pr.number),
        Err(e) => Pr::Unknown(format!("riff found no pull request: {}", first_line(&e))),
    }
}

fn first_line(e: &anyhow::Error) -> String {
    let text = format!("{e:#}");
    crate::text::safe(text.lines().next().unwrap_or_default())
}

/// Cleans each linked worktree of the clone of `main`
/// (01M3ZV0TKSHNW5QC2NG1XTJEJB). `owned` tells whether a live session
/// works in a worktree. `here` is the directory of this process: riff
/// never acts on its worktree. Returns one [`Done`] for each worktree.
pub fn clean(main: &Path, here: &Path, gh: &Gh, owned: impl Fn(&Tree) -> bool) -> Vec<Done> {
    let Ok(list) = git(main, &["worktree", "list", "--porcelain"]) else {
        return Vec::new();
    };
    let start = |pid| workload::read(pid, "").map(|p| p.start);
    parse(&list)
        .into_iter()
        .map(|tree| {
            let lock = Lock::of(tree.lock.as_deref(), start);
            let here = here.starts_with(&tree.path);
            let changed = git(&tree.path, &["status", "--porcelain"])
                .map(|out| !out.trim().is_empty())
                .unwrap_or(true);
            let mut facts = Facts {
                agent: tree.path.starts_with(main.join(AGENT_DIR)),
                lock,
                owned: here || owned(&tree),
                changed,
                branch: tree.branch.is_some(),
                pr: None,
                on_origin: false,
            };
            let free = matches!(facts.lock, Lock::None | Lock::Dead) && !facts.owned;
            if free && !facts.changed {
                match &tree.branch {
                    Some(branch) => facts.pr = Some(pr_of(gh, branch, &tree.head)),
                    None => {
                        let on = git(main, &["branch", "-r", "--contains", &tree.head]);
                        facts.on_origin = on.is_ok_and(|out| !out.trim().is_empty());
                    }
                }
            }
            act(main, &tree, &facts)
        })
        .collect()
}

/// The worktrees of the agent tool in the clone of `main` that no live
/// owner has: no lock of a live process or of a person, no live
/// session in it (`owned`), and not the worktree of `here`
/// (01M41A11BB4HAD8595DNSBAZ0D).
pub fn ownerless(main: &Path, here: &Path, owned: impl Fn(&Tree) -> bool) -> Vec<Tree> {
    let Ok(list) = git(main, &["worktree", "list", "--porcelain"]) else {
        return Vec::new();
    };
    let start = |pid| workload::read(pid, "").map(|p| p.start);
    parse(&list)
        .into_iter()
        .filter(|tree| {
            tree.path.starts_with(main.join(AGENT_DIR))
                && matches!(
                    Lock::of(tree.lock.as_deref(), start),
                    Lock::None | Lock::Dead
                )
                && !here.starts_with(&tree.path)
                && !owned(tree)
        })
        .collect()
}

/// Does the steps of `facts` for `tree`.
fn act(main: &Path, tree: &Tree, facts: &Facts) -> Done {
    let path = tree.path.display().to_string();
    let mut words = Vec::new();
    let mut saved = false;
    for step in decide(facts) {
        let path_arg = path.as_str();
        let did = match &step {
            Step::Unlock => git(main, &["worktree", "unlock", path_arg])
                .map(|_| "unlocked: the process of its lock is gone".to_owned()),
            Step::Remove => remove(main, tree),
            Step::Save => save(tree).inspect(|_| saved = true),
            Step::Keep(why) => Ok(format!("kept: {why}")),
        };
        match did {
            Ok(line) => words.push(line),
            Err(why) => {
                words.push(format!("kept: {why}"));
                break;
            }
        }
    }
    Done {
        path: tree.path.clone(),
        line: format!("{path}: {}", words.join("; ")),
        saved,
    }
}

/// Removes the worktree, then its branch while it points at `HEAD`.
fn remove(main: &Path, tree: &Tree) -> std::result::Result<String, String> {
    let path = tree.path.display().to_string();
    git(main, &["worktree", "remove", &path])?;
    let Some(branch) = &tree.branch else {
        return Ok("removed: it holds no work, and its commit is on origin".into());
    };
    let reference = format!("refs/heads/{branch}");
    git(main, &["update-ref", "-d", &reference, &tree.head])?;
    Ok(format!(
        "removed with its branch {branch}: its pull request is merged"
    ))
}

/// Commits the work of the worktree as WIP, and pushes its branch.
fn save(tree: &Tree) -> std::result::Result<String, String> {
    let branch = tree.branch.as_deref().unwrap_or_default();
    git(&tree.path, &["add", "-A"])?;
    git(
        &tree.path,
        &[
            "commit",
            "-q",
            "--no-verify",
            "-m",
            "WIP: riff worktrees clean saved the work of a session that ended",
        ],
    )?;
    git(&tree.path, &["push", "-q", "-u", "origin", "HEAD"])?;
    Ok(format!(
        "saved: a WIP commit on {branch}, pushed to origin. No live session owns it"
    ))
}

fn git(dir: &Path, args: &[&str]) -> std::result::Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let why = stderr.lines().next().unwrap_or("git failed").trim();
    Err(format!(
        "git {} failed: {}",
        args[0],
        crate::text::safe(why)
    ))
}

/// The worktrees that a live session in `who` owns: live, on `host`, in
/// the repository `repo`, in a worktree with the name of the last part
/// of the path.
///
/// ```
/// use riff::worktrees::{Tree, owned_by};
/// use riff_core::wire::SessionInfo;
///
/// let info = |uri: &str, live| SessionInfo { uri: uri.parse().unwrap(), live, idle_secs: 0, status: None, worker: false, stopping: false, claims_secs: 0, must_clear: false, fresh_secs: None, state: Default::default(), work: None, waits: None, blocked: None };
/// let who = [
///     info("riff://mike@pangolin/o/r?session=a1#issue-12", true),
///     info("riff://mike@pangolin/o/r?session=b2#issue-13", false),
///     info("riff://mike@thelio/o/r?session=c3#issue-14", true),
/// ];
/// let tree = |name: &str| Tree { path: format!("/r/.claude/worktrees/{name}").into(), branch: None, head: "1".into(), lock: None };
/// let owned = |name| owned_by(&who, "pangolin", "o/r", &tree(name));
/// assert!(owned("issue-12"));
/// assert!(!owned("issue-13"), "not live");
/// assert!(!owned("issue-14"), "on another host");
/// ```
pub fn owned_by(who: &[riff_core::wire::SessionInfo], host: &str, repo: &str, tree: &Tree) -> bool {
    let name = tree.path.file_name().and_then(|n| n.to_str());
    who.iter().any(|s| {
        s.live
            && s.uri.place().host() == host
            && s.uri.place().repo_text() == repo
            && s.uri.place().worktree().is_some()
            && s.uri.place().worktree() == name
    })
}

/// Cleans the worktrees of the clone of `dir` for the person `me`:
/// [`clean`] with the live sessions of `riff who`. It posts one note to
/// the lead when it saved work (01M3ZV0TKSHNW5QC2NG1XTJEJB). Returns
/// the lines to print.
pub async fn clean_here(
    api: &crate::api::Api,
    me: &riff_core::name::SessionUri,
    dir: &Path,
) -> Result<Vec<String>> {
    let main = crate::identity::main_worktree(dir)
        .ok_or_else(|| anyhow::anyhow!("run it in a git repository"))?;
    let who = api.who(me, false).await?;
    let host = me.place().host().to_owned();
    let repo = me.place().repo_text();
    // git and gh: keep them off the runtime.
    let done = {
        let (here, repo) = (dir.to_owned(), repo.clone());
        tokio::task::spawn_blocking(move || {
            clean(&main, &here, &Gh::default(), |tree| {
                owned_by(&who, &host, &repo, tree)
            })
        })
        .await?
    };
    let saved: Vec<&str> = done
        .iter()
        .filter(|d| d.saved)
        .map(|d| d.line.as_str())
        .collect();
    if !saved.is_empty() {
        let to = riff_core::selector::Selector::lead(me.who().user(), &repo);
        let note = crate::text::worktrees_saved(&saved);
        if let Err(e) = api
            .post(me, None, &[to], &note, riff_core::wire::Kind::Note)
            .await
        {
            eprintln!("riff: cannot post the note to the lead: {e:#}");
        }
    }
    Ok(done.into_iter().map(|d| d.line).collect())
}

/// [`clean_here`] in `dir` as the person of this host
/// (01M3ZV0TKSHNW5QC2NG1XTJEJB): `riff worktrees clean`, `riff workers
/// start` and the start of `riff workers host`
/// (01M3ZV0TM7ANJ1QQ7XTBDJQE1V).
pub async fn clean_as_person(dir: &Path, server: &str) -> Result<Vec<String>> {
    let here = crate::identity::place(dir)?;
    let me = crate::identity::person(&here, server)?;
    let api = crate::api::Api::new(server).signed_in(None)?;
    clean_here(&api, &me, dir).await
}
