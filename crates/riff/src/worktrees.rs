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
//!     U --> O{"a live owner: a lock of a live process,<br/>a live session in it or with its item,<br/>a running process in it, or this process?"}
//!     O -- yes --> K["keep"]
//!     O -- no --> D{"work that is not committed?"}
//!     D -- "yes, on a branch" --> W["WIP commit, push,<br/>a note to the lead"]
//!     D -- "yes, detached" --> K
//!     D -- no --> A{"HEAD is on the default<br/>branch of origin?"}
//!     A -- yes --> R["remove the worktree<br/>and its branch"]
//!     A -- no --> M{"a merged pull request has HEAD as its head?<br/>detached: or HEAD is on origin?"}
//!     M -- yes --> R
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
//!   this host, in this repository, in that worktree. It also owns the
//!   worktree of an item that it holds, when it works in another
//!   worktree ([`Owner::Claim`], 01M515233TRCW2PK3V4QBR4V17).
//! - A running process owns a worktree when its program or its working
//!   directory is in it, for example a check build of riff that a
//!   systemd unit runs, or the shell of a person ([`Owner::Process`],
//!   01M5152361VDRV7NZ2N2Z7BVNP).
//! - A clean worktree whose `HEAD` is on the default branch of `origin`
//!   has no commit of its own. riff removes it, with or with no pull
//!   request (01M41XFFXEQPEPDVM4HNT69FVP).
//! - riff finds the pull request of a branch by its name. When that
//!   fails, and for a detached worktree, riff finds a merged pull
//!   request by the commit of `HEAD`. So the verify worktree of a merged
//!   pull request goes, also after the merge deleted the branch.
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
//! let facts = Facts { agent: true, lock: Lock::Dead, owner: None, changed: false, branch: true, pr: Some(Pr::MergedAtHead(40)), on_origin: true, on_main: false };
//! assert_eq!(decide(&facts), [Step::Unlock, Step::Remove("its pull request #40 is merged".into())]);
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

/// Who uses a worktree, so that riff keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    /// A live session works in it, or this process does.
    Session,
    /// A live session on this host holds its item, also when that
    /// session works in another worktree (01M515233TRCW2PK3V4QBR4V17).
    Claim(String),
    /// A running process has its program or its working directory in
    /// it (01M5152361VDRV7NZ2N2Z7BVNP).
    Process { pid: u32, name: String },
}

impl Owner {
    /// The reason to keep the worktree.
    ///
    /// ```
    /// use riff::worktrees::Owner;
    ///
    /// assert_eq!(Owner::Session.why(), "a live session works in it");
    /// assert_eq!(Owner::Claim("issue-12".into()).why(), "a live session holds its item issue-12");
    /// let process = Owner::Process { pid: 4242, name: "riff".into() };
    /// assert_eq!(process.why(), "the running process 4242 (riff) uses it");
    /// ```
    pub fn why(&self) -> String {
        match self {
            Owner::Session => "a live session works in it".into(),
            Owner::Claim(item) => format!("a live session holds its item {item}"),
            Owner::Process { pid, name } => format!("the running process {pid} ({name}) uses it"),
        }
    }
}

/// The facts of one worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// It is a worktree of an agent tool: in `.claude/worktrees` of the
    /// main worktree. riff keeps each other worktree: a person made it.
    pub agent: bool,
    pub lock: Lock,
    /// Who uses it, when a live session, this process or a running
    /// process does.
    pub owner: Option<Owner>,
    /// It has files that are not committed.
    pub changed: bool,
    /// It is on a branch, not detached.
    pub branch: bool,
    /// The pull request of its branch. `None` when riff did not look.
    pub pr: Option<Pr>,
    /// Its `HEAD` is on a branch of `origin`.
    pub on_origin: bool,
    /// Its `HEAD` is on the default branch of `origin`: it has no commit
    /// of its own.
    pub on_main: bool,
}

/// One step for a worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Unlock,
    /// Remove the worktree and its branch, for this reason.
    Remove(String),
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
/// use riff::worktrees::Owner;
///
/// let free = Facts { agent: true, lock: Lock::None, owner: None, changed: false, branch: true, pr: None, on_origin: false, on_main: false };
/// let changed = Facts { changed: true, ..free.clone() };
/// assert_eq!(decide(&changed), [Step::Save]);
/// let detached = Facts { branch: false, ..changed.clone() };
/// assert_eq!(decide(&detached), [Step::Keep("it has work on no branch".into())]);
/// let verify = Facts { branch: false, on_origin: true, ..free.clone() };
/// assert_eq!(decide(&verify), [Step::Remove("it holds no work, and its commit is on origin".into())]);
/// let gone = Facts { branch: false, pr: Some(Pr::MergedAtHead(40)), ..free.clone() };
/// assert_eq!(decide(&gone), [Step::Remove("its commit is the head of the merged pull request #40".into())]);
/// let lost = Facts { branch: false, ..free.clone() };
/// assert_eq!(decide(&lost), [Step::Keep("its commit is on no branch of origin and is the head of no merged pull request".into())]);
/// let fresh = Facts { on_main: true, pr: Some(Pr::NotMerged(41)), ..free.clone() };
/// assert_eq!(decide(&fresh), [Step::Remove("its HEAD is on the default branch of origin: it has no commit of its own".into())]);
/// let open = Facts { pr: Some(Pr::NotMerged(41)), ..free.clone() };
/// assert_eq!(decide(&open), [Step::Keep("pull request #41 is not merged".into())]);
/// let owned = Facts { owner: Some(Owner::Session), lock: Lock::Dead, ..free.clone() };
/// assert_eq!(decide(&owned), [Step::Unlock, Step::Keep("a live session works in it".into())]);
/// let held = Facts { owner: Some(Owner::Claim("issue-12".into())), ..fresh.clone() };
/// assert_eq!(decide(&held), [Step::Keep("a live session holds its item issue-12".into())]);
/// let used = Facts { owner: Some(Owner::Process { pid: 7, name: "riff".into() }), ..fresh };
/// assert_eq!(decide(&used), [Step::Keep("the running process 7 (riff) uses it".into())]);
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
    let last = if let Some(owner) = &facts.owner {
        Step::Keep(owner.why())
    } else if facts.changed && facts.branch {
        Step::Save
    } else if facts.changed {
        Step::Keep("it has work on no branch".into())
    } else if facts.on_main {
        Step::Remove(
            "its HEAD is on the default branch of origin: it has no commit of its own".into(),
        )
    } else if !facts.branch {
        match &facts.pr {
            Some(Pr::MergedAtHead(n)) => Step::Remove(format!(
                "its commit is the head of the merged pull request #{n}"
            )),
            _ if facts.on_origin => {
                Step::Remove("it holds no work, and its commit is on origin".into())
            }
            _ => Step::Keep(
                "its commit is on no branch of origin and is the head of no merged pull request"
                    .into(),
            ),
        }
    } else {
        match &facts.pr {
            Some(Pr::MergedAtHead(n)) => Step::Remove(format!("its pull request #{n} is merged")),
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

/// A merged pull request, found by its head commit `head`
/// (01M41XFFXEQPEPDVM4HNT69FVP). Its branch can be gone.
fn merged_at(gh: &Gh, head: &str) -> Option<u64> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Found {
        number: u64,
        head_ref_oid: String,
    }
    let args = [
        "pr",
        "list",
        "--state",
        "merged",
        "--search",
        head,
        "--json",
        "number,headRefOid",
    ];
    let found = gh.json::<Vec<Found>>(&args).ok()?;
    found
        .into_iter()
        .find(|p| p.head_ref_oid == head)
        .map(|p| p.number)
}

fn first_line(e: &anyhow::Error) -> String {
    let text = format!("{e:#}");
    crate::text::safe(text.lines().next().unwrap_or_default())
}

/// A running process: its program and its working directory, from
/// `/proc/PID/exe` and `/proc/PID/cwd`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pub pid: u32,
    /// The name of its program, from `/proc/PID/comm`.
    pub name: String,
    pub exe: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
}

impl Running {
    /// Each process that riff can read: of this user, on Linux. None on
    /// a system with no `/proc`.
    pub fn all() -> Vec<Running> {
        let Ok(dir) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        dir.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok())
            .filter_map(Running::read)
            .collect()
    }

    /// The process `pid`, or `None` when riff can read neither its
    /// program nor its working directory.
    fn read(pid: u32) -> Option<Running> {
        let dir = PathBuf::from(format!("/proc/{pid}"));
        let link = |name: &str| std::fs::read_link(dir.join(name)).ok().map(live_path);
        let (exe, cwd) = (link("exe"), link("cwd"));
        if exe.is_none() && cwd.is_none() {
            return None;
        }
        let name = std::fs::read_to_string(dir.join("comm")).unwrap_or_default();
        Some(Running {
            pid,
            name: crate::text::safe(name.trim()),
            exe,
            cwd,
        })
    }

    /// True when its program or its working directory is in `dir`.
    pub fn uses(&self, dir: &Path) -> bool {
        [&self.exe, &self.cwd]
            .into_iter()
            .flatten()
            .any(|path| path.starts_with(dir))
    }
}

/// The path of a link in `/proc/PID`, with no mark ` (deleted)`: the
/// program of a process still counts after a new build deleted its file.
///
/// ```
/// use riff::worktrees::live_path;
///
/// let deleted = live_path("/r/.claude/worktrees/live-712/target/release/riff (deleted)".into());
/// assert_eq!(deleted, std::path::Path::new("/r/.claude/worktrees/live-712/target/release/riff"));
/// assert_eq!(live_path("/usr/bin/sleep".into()), std::path::Path::new("/usr/bin/sleep"));
/// ```
pub fn live_path(path: PathBuf) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    let bytes = path.as_os_str().as_bytes();
    match bytes.strip_suffix(b" (deleted)") {
        Some(live) => PathBuf::from(std::ffi::OsStr::from_bytes(live)),
        None => path,
    }
}

/// The first process of `running` that uses the worktree `dir`
/// (01M5152361VDRV7NZ2N2Z7BVNP).
///
/// ```
/// use riff::worktrees::{Owner, Running, used_by};
/// use std::path::Path;
///
/// let p = |pid, exe: &str, cwd: &str| Running { pid, name: "riff".into(), exe: Some(exe.into()), cwd: Some(cwd.into()) };
/// let running = [
///     p(10, "/usr/bin/bash", "/r/.claude/worktrees/issue-12"),
///     p(20, "/r/.claude/worktrees/live-712/target/release/riff", "/"),
/// ];
/// let used = |dir: &str| used_by(&running, Path::new(dir));
/// assert_eq!(used("/r/.claude/worktrees/issue-12"), Some(Owner::Process { pid: 10, name: "riff".into() }));
/// assert_eq!(used("/r/.claude/worktrees/live-712"), Some(Owner::Process { pid: 20, name: "riff".into() }));
/// assert_eq!(used("/r/.claude/worktrees/issue-1"), None, "a path in another worktree");
/// ```
pub fn used_by(running: &[Running], dir: &Path) -> Option<Owner> {
    running
        .iter()
        .find(|p| p.uses(dir))
        .map(|p| Owner::Process {
            pid: p.pid,
            name: p.name.clone(),
        })
}

/// The owner of `tree`: this process in `here`, a live session of
/// `owner`, or a process of `running`.
fn owner_of(
    tree: &Tree,
    here: &Path,
    running: &[Running],
    owner: &impl Fn(&Tree) -> Option<Owner>,
) -> Option<Owner> {
    if here.starts_with(&tree.path) {
        return Some(Owner::Session);
    }
    owner(tree).or_else(|| {
        // `/proc` gives each path with no link in it.
        let real = std::fs::canonicalize(&tree.path).unwrap_or_else(|_| tree.path.clone());
        used_by(running, &real)
    })
}

/// Cleans each linked worktree of the clone of `main`
/// (01M3ZV0TKSHNW5QC2NG1XTJEJB). `owner` tells whether a live session
/// works in a worktree or holds its item ([`owner_in`]). riff also keeps
/// a worktree that a running process uses ([`used_by`]). `here` is the
/// directory of this process: riff never acts on its worktree. Returns
/// one [`Done`] for each worktree.
pub fn clean(
    main: &Path,
    here: &Path,
    gh: &Gh,
    owner: impl Fn(&Tree) -> Option<Owner>,
) -> Vec<Done> {
    let Ok(list) = git(main, &["worktree", "list", "--porcelain"]) else {
        return Vec::new();
    };
    let start = |pid| workload::read(pid, "").map(|p| p.start);
    let running = Running::all();
    parse(&list)
        .into_iter()
        .map(|tree| {
            let lock = Lock::of(tree.lock.as_deref(), start);
            let changed = git(&tree.path, &["status", "--porcelain"])
                .map(|out| !out.trim().is_empty())
                .unwrap_or(true);
            let mut facts = Facts {
                agent: tree.path.starts_with(main.join(AGENT_DIR)),
                lock,
                owner: owner_of(&tree, here, &running, &owner),
                changed,
                branch: tree.branch.is_some(),
                pr: None,
                on_origin: false,
                on_main: false,
            };
            let free = matches!(facts.lock, Lock::None | Lock::Dead) && facts.owner.is_none();
            if free && !facts.changed {
                facts.on_main = on_main(main, &tree.head);
                if !facts.on_main {
                    facts.pr = match tree.branch.as_ref().map(|b| pr_of(gh, b, &tree.head)) {
                        Some(Pr::Unknown(why)) => Some(
                            merged_at(gh, &tree.head).map_or(Pr::Unknown(why), Pr::MergedAtHead),
                        ),
                        Some(pr) => Some(pr),
                        None => merged_at(gh, &tree.head).map(Pr::MergedAtHead),
                    };
                    if tree.branch.is_none() {
                        let on = git(main, &["branch", "-r", "--contains", &tree.head]);
                        facts.on_origin = on.is_ok_and(|out| !out.trim().is_empty());
                    }
                }
            }
            act(main, &tree, &facts)
        })
        .collect()
}

/// True when `head` is on the default branch of `origin` in the clone
/// of `main`: the branch of `origin/HEAD`, else `main`.
fn on_main(main: &Path, head: &str) -> bool {
    let branch = crate::permissions::Project::of(main).branch;
    let base = format!("refs/remotes/origin/{branch}");
    git(main, &["merge-base", "--is-ancestor", head, &base]).is_ok()
}

/// The worktrees of the agent tool in the clone of `main` that no live
/// owner has: no lock of a live process or of a person, no live
/// session in it or with its item (`owner`), no running process in it,
/// and not the worktree of `here` (01M41A11BB4HAD8595DNSBAZ0D).
pub fn ownerless(main: &Path, here: &Path, owner: impl Fn(&Tree) -> Option<Owner>) -> Vec<Tree> {
    let Ok(list) = git(main, &["worktree", "list", "--porcelain"]) else {
        return Vec::new();
    };
    let start = |pid| workload::read(pid, "").map(|p| p.start);
    let running = Running::all();
    parse(&list)
        .into_iter()
        .filter(|tree| {
            tree.path.starts_with(main.join(AGENT_DIR))
                && matches!(
                    Lock::of(tree.lock.as_deref(), start),
                    Lock::None | Lock::Dead
                )
                && owner_of(tree, here, &running, &owner).is_none()
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
            Step::Remove(why) => remove(main, tree, why),
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
/// `why` is the reason in the line.
fn remove(main: &Path, tree: &Tree, why: &str) -> std::result::Result<String, String> {
    let path = tree.path.display().to_string();
    git(main, &["worktree", "remove", &path])?;
    let Some(branch) = &tree.branch else {
        return Ok(format!("removed: {why}"));
    };
    let reference = format!("refs/heads/{branch}");
    git(main, &["update-ref", "-d", &reference, &tree.head])?;
    Ok(format!("removed with its branch {branch}: {why}"))
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

/// The owner of `tree` in `who`, from the live sessions on `host` in the
/// repository `repo`. [`Owner::Session`]: a session works in the
/// worktree with the name of the last part of the path.
/// [`Owner::Claim`]: a session holds the item of the worktree, also from
/// another worktree (01M515233TRCW2PK3V4QBR4V17). The item of a worktree
/// is its name, for example `issue-12`. A verify worktree has the first
/// 4 characters of the session ID after the item: the worktree
/// `verify-issue-12-a6cf` is of the claim `verify-issue-12` of the
/// session `a6cf…`.
///
/// ```
/// use riff::worktrees::{Owner, Tree, owner_in};
/// use riff_core::wire::SessionInfo;
///
/// let info = |uri: &str, live| SessionInfo { uri: uri.parse().unwrap(), live, idle_secs: 0, status: None, worker: false, container: false, binary: None, stopping: false, claims_secs: 0, must_clear: false, fresh_secs: None, state: Default::default(), work: None, waits: None, blocked: None, step: None, machine: None };
/// let who = [
///     info("riff://mike@pangolin/o/r?session=a1#issue-12", true),
///     info("riff://mike@pangolin/o/r?session=b2#issue-13", false),
///     info("riff://mike@thelio/o/r?session=c3#issue-14", true),
///     info("riff://mike@pangolin/o/r?session=d4d4d4&claim=issue-756&claim=verify-issue-9#issue-753", true),
///     info("riff://mike@thelio/o/r?session=e5&claim=issue-15", true),
/// ];
/// let tree = |name: &str| Tree { path: format!("/r/.claude/worktrees/{name}").into(), branch: None, head: "1".into(), lock: None };
/// let owner = |name| owner_in(&who, "pangolin", "o/r", &tree(name));
/// assert_eq!(owner("issue-12"), Some(Owner::Session));
/// assert_eq!(owner("issue-13"), None, "not live");
/// assert_eq!(owner("issue-14"), None, "on another host");
/// assert_eq!(owner("issue-756"), Some(Owner::Claim("issue-756".into())), "it works in issue-753");
/// assert_eq!(owner("verify-issue-9-d4d4"), Some(Owner::Claim("verify-issue-9".into())));
/// assert_eq!(owner("verify-issue-9-a1a1"), None, "the verify worktree of another session");
/// assert_eq!(owner("issue-15"), None, "a claim on another host");
/// ```
pub fn owner_in(
    who: &[riff_core::wire::SessionInfo],
    host: &str,
    repo: &str,
    tree: &Tree,
) -> Option<Owner> {
    let name = tree.path.file_name().and_then(|n| n.to_str())?;
    let mut live = who
        .iter()
        .filter(|s| s.live && s.uri.place().host() == host && s.uri.place().repo_text() == repo);
    if live.clone().any(|s| s.uri.place().worktree() == Some(name)) {
        return Some(Owner::Session);
    }
    live.find_map(|s| {
        let short = s.uri.who().session().and_then(|id| id.get(..4));
        s.uri
            .claims()
            .iter()
            .find(|item| {
                name == item.as_str()
                    || short.is_some_and(|short| name == format!("{item}-{short}"))
            })
            .map(|item| Owner::Claim(item.clone()))
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
                owner_in(&who, &host, &repo, tree)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Child;

    /// A shell `program` that waits on its stdin in the directory `dir`,
    /// stopped on drop: also when a test fails.
    struct Waits(Child);

    impl Waits {
        /// It returns when the child has the name of `program`. The
        /// kernel lets the parent go on in the exec before it names the
        /// child, so a read at once can find the name of the test thread
        /// (#826, 01M41A0M2XWCWTWGF7T9DR03W0).
        fn start(program: &Path, dir: &Path) -> Waits {
            let child = Command::new(program)
                .args(["-c", "read line"])
                .current_dir(dir)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let waits = Waits(child);
            let name = program.file_name().unwrap().to_string_lossy();
            let comm = format!("/proc/{}/comm", waits.id());
            let span = isolated::Span::start();
            while std::fs::read_to_string(&comm).unwrap_or_default().trim() != name {
                let limit = std::time::Duration::from_secs(10);
                assert!(span.within(limit), "the child is not {name}");
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            waits
        }

        fn id(&self) -> u32 {
            self.0.id()
        }
    }

    impl Drop for Waits {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn running_has_the_directory_and_the_program_of_this_process() {
        let me = Running::all()
            .into_iter()
            .find(|p| p.pid == std::process::id())
            .expect("this process is in /proc");
        assert_eq!(me.cwd, Some(std::env::current_dir().unwrap()));
        assert_eq!(me.exe, Some(std::env::current_exe().unwrap()));
    }

    #[test]
    fn a_process_in_a_worktree_uses_it() {
        let dir = tempfile::tempdir().unwrap();
        let worktree = std::fs::canonicalize(dir.path()).unwrap();
        let child = Waits::start(Path::new("sh"), &worktree);
        let found = Running::read(child.id()).unwrap();
        assert_eq!(found.name, "sh");
        assert_eq!(found.cwd.as_deref(), Some(worktree.as_path()));
        let running = [found];
        assert_eq!(
            used_by(&running, &worktree),
            Some(Owner::Process {
                pid: child.id(),
                name: "sh".into()
            })
        );
        assert_eq!(used_by(&running, &worktree.join("other")), None);
    }

    /// The case of #762: a systemd unit runs a check build of riff in a
    /// worktree, and a new build deletes the file of the program.
    #[test]
    fn a_program_in_a_worktree_uses_it_also_after_a_new_build_deleted_it() {
        let dir = tempfile::tempdir().unwrap();
        let worktree = std::fs::canonicalize(dir.path()).unwrap();
        let program = worktree.join("target/release/riff");
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        // A copy of `sh` is the build. A child `cp` writes it, so this
        // process holds no write fd of it when it runs (`ETXTBSY`).
        let cp = Command::new("cp")
            .arg("-L")
            .arg("/bin/sh")
            .arg(&program)
            .status()
            .unwrap();
        assert!(cp.success());
        let elsewhere = tempfile::tempdir().unwrap();
        let child = Waits::start(&program, elsewhere.path());
        std::fs::remove_file(&program).unwrap();
        let found = Running::read(child.id()).unwrap();
        assert_eq!(found.name, "riff");
        assert_eq!(found.exe.as_deref(), Some(program.as_path()));
        assert!(found.uses(&worktree));
    }

    #[test]
    fn the_owner_is_this_process_then_a_session_then_a_running_process() {
        let dir = tempfile::tempdir().unwrap();
        let real = std::fs::canonicalize(dir.path()).unwrap().join("issue-12");
        std::fs::create_dir(&real).unwrap();
        // git can give the path of a worktree through a link.
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(dir.path(), &link).unwrap();
        let tree = Tree {
            path: link.join("issue-12"),
            branch: None,
            head: "1".into(),
            lock: None,
        };
        let running = [Running {
            pid: 7,
            name: "bash".into(),
            exe: None,
            cwd: Some(real.join("crates")),
        }];
        let process = Some(Owner::Process {
            pid: 7,
            name: "bash".into(),
        });
        let none = |_: &Tree| None;
        let claim = |_: &Tree| Some(Owner::Claim("issue-12".into()));
        let elsewhere = Path::new("/elsewhere");
        assert_eq!(owner_of(&tree, elsewhere, &running, &none), process);
        assert_eq!(
            owner_of(&tree, elsewhere, &running, &claim),
            Some(Owner::Claim("issue-12".into()))
        );
        assert_eq!(
            owner_of(&tree, &tree.path.join("crates"), &running, &claim),
            Some(Owner::Session)
        );
        assert_eq!(owner_of(&tree, elsewhere, &[], &none), None);
    }
}
