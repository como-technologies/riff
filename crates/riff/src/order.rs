//! The order of the verifies of a wave (#1710, 01M6WCGNCC3HWS47158WTWNQ8B):
//! the pull requests of the items whose heads conflict get their verifies
//! one at a time, and their rebases too (#1801, #1895,
//! 01M6YPYTGR2JEBDHGX4K1RDCAW, 01M6YPYTKQKSJRS0A8WZQRCCFD,
//! 01M74EP20TDQEXW42MK0XE73QF, 01M750ED646BKBZSA9KHF0953Y).
//!
//! # Design
//!
//! The merge of a pull request gives each other open pull request whose
//! head conflicts with its head a conflict. A conflict costs a rebase
//! session, a new full check and a new verify, and a verify that passed
//! before the conflict is lost. So the wave ([`crate::wave::pick`]) starts
//! the verify or the rebase of one of them, and the others wait until it
//! merges or fails:
//!
//! - A pull request goes first while its verify runs, and while its
//!   verify passed and nothing stops its merge ([`Lane::passed`]). A pull
//!   request that cannot merge by itself for another reason
//!   ([`crate::rollout::Stuck`]) does not go first, for example with a
//!   failed check or with auto-merge off. A session of its item does not
//!   make it go first (01M74EP20TDQEXW42MK0XE73QF, #1894): it keeps its
//!   place while the session rebases it, and else it has no place while
//!   the session runs, as its verify cannot start.
//! - Then come the pull requests that wait for a verify or for a rebase
//!   ([`Lane::conflict`]). The one whose item had its first verify request
//!   first goes first ([`Lane::asked`], [`crate::wave::requests`]): a new
//!   head after a rebase or a fix keeps the place. A pull request with no
//!   request in the thread comes after those with one. A tie goes by the
//!   number of the pull request.
//! - A verify or a rebase waits for the first pull request before it in
//!   its group ([`waits`]): the two change a file in common, and their
//!   heads conflict ([`Lane::clashes`]). So only the next pull request of
//!   the order gets a rebase, and the others keep their conflict until
//!   their turn. A pull request that is in the group of no pull request
//!   before it starts beside the others.
//! - Two pull requests that change one file often merge with no conflict,
//!   for example a new requirement in each of two sections of
//!   `requirements.md`. So the wave tests the merge of each two heads that
//!   change a file in common ([`merges`], #1895): `git merge-tree` of the
//!   two head commits in the main clone of the daemon. A merge with no
//!   conflict puts the two in two groups ([`Lane::clean`]). A test that
//!   cannot run keeps them in one group.
//! - A stale head, with a conflict with the default branch, conflicts
//!   with each newer head that holds the default branch. So a head with
//!   a conflict and a head with none merge over the merge base of the
//!   other head and the default branch (01M7E06YMNMF92TDW21XED5JZ8,
//!   #2128): a conflict that only the default branch gives does not
//!   count, and the rebase of the stale head starts at once.
//! - A pull request whose state GitHub does not know yet keeps its place
//!   ([`Lane::unknown`]): for a short time after the push of its rebase,
//!   GitHub does not say that it has no conflict now.
//! - The files come from GitHub, one REST read for each head commit
//!   ([`files`]). The wave reads them only when two pull requests or more
//!   are on their way to the merge, and one of them waits for a verify or
//!   for a rebase. A read that fails gives no file: that pull request
//!   waits for nobody.
//!
//! The daemon tells the waits in the facts of its wave
//! ([`riff_client::daemons::Wave::waits`]), and `riff wave list` shows
//! each one as `verify waits for #N` or `rebase waits for #N`
//! ([`riff_client::waves::Board::groups`]). The look of the daemon sends
//! no message "waits for a verify" for such a pull request, and no
//! message of its conflict ([`crate::look::PullWatch::news`]). The loop
//! of the wave sends one message for each rebase that waits
//! ([`crate::text::wave_rebase_waits`], 01M7E075M743X8TE00T5PFMTVR).
//!
//! ```mermaid
//! flowchart TD
//!     A["verify requests: PR #41, then PR #42"] --> S["the verify of #41 starts"]
//!     S --> Q{"#42 changes a file that #41 changes?"}
//!     Q -- no --> B["the verify of #42 starts beside it"]
//!     Q -- yes --> M{"git merge-tree of the two heads"}
//!     M -- "no conflict" --> B
//!     M -- "a conflict, or no test" --> W["#42: verify waits for #41"]
//!     W --> E{"#41"}
//!     E -- "merges" --> R["#42: a rebase when it has a conflict,<br/>then its verify"]
//!     E -- "its verify fails, or it cannot merge" --> V["the verify of #42 starts"]
//! ```
//!
//! Three pull requests whose heads conflict, in the order #41, #42, #43:
//!
//! ```mermaid
//! flowchart TD
//!     M["#41 merges: #42 and #43 have a conflict"] --> R["the rebase of #42 starts"]
//!     M --> X["#43: rebase waits for #42"]
//!     R --> V["the verify of the new head of #42"]
//!     V --> N["#42 merges"]
//!     X --> N
//!     N --> T["the rebase of #43 starts"]
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::Result;

use crate::pr::Gh;

/// An open pull request of an item on its way to the merge: its verify
/// waits, runs or passed, or it waits for a rebase.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lane {
    /// The issue of the item.
    pub issue: u64,
    /// The pull request.
    pub pull: u64,
    /// Its head commit.
    pub head: String,
    /// The files that it changes.
    pub files: BTreeSet<String>,
    /// The pull requests that change a file of it, and whose heads merge
    /// with its head with no conflict ([`merges`], #1895). It does not
    /// wait for them, and they do not wait for it.
    pub clean: BTreeSet<u64>,
    /// True when its verify passed and nothing stops its merge: it waits
    /// for the merge.
    pub passed: bool,
    /// True when it has a conflict with the default branch
    /// ([`crate::rollout::Pull::conflict`]): its rebase comes at its turn
    /// (#1801).
    pub conflict: bool,
    /// True when GitHub does not know yet if it has a conflict
    /// ([`crate::rollout::Pull::unknown`]): it keeps its place, and its
    /// verify or its rebase waits for the next look.
    pub unknown: bool,
    /// The place of the first verify request of its item among the
    /// messages of the thread, oldest first. A new head keeps it. `None`:
    /// the thread has none.
    pub asked: Option<usize>,
}

impl Lane {
    /// True when this pull request and `other` change a file in common.
    ///
    /// ```
    /// use riff::order::Lane;
    ///
    /// let lane = |pull, files: &[&str]| Lane {
    ///     pull,
    ///     files: files.iter().map(|f| f.to_string()).collect(),
    ///     ..Lane::default()
    /// };
    /// assert!(lane(41, &["src/a.rs", "src/b.rs"]).shares(&lane(42, &["src/b.rs"])));
    /// assert!(!lane(41, &["src/a.rs"]).shares(&lane(42, &["src/b.rs"])));
    /// assert!(!lane(41, &[]).shares(&lane(42, &[])));
    /// ```
    pub fn shares(&self, other: &Lane) -> bool {
        self.pull != other.pull && !self.files.is_disjoint(&other.files)
    }

    /// True when this pull request and `other` are in one group: they
    /// change a file in common ([`Lane::shares`]), and no merge test says
    /// that their heads merge with no conflict ([`Lane::clean`]).
    ///
    /// ```
    /// use riff::order::Lane;
    ///
    /// let lane = |pull| Lane {
    ///     pull,
    ///     files: ["docs/src/requirements.md".to_owned()].into(),
    ///     ..Lane::default()
    /// };
    /// assert!(lane(41).clashes(&lane(42)));
    /// // The heads of #41 and #42 merge with no conflict.
    /// let clean = Lane { clean: [42].into(), ..lane(41) };
    /// assert!(!clean.clashes(&lane(42)));
    /// assert!(!lane(42).clashes(&clean));
    /// ```
    pub fn clashes(&self, other: &Lane) -> bool {
        self.shares(other) && !self.clean.contains(&other.pull) && !other.clean.contains(&self.pull)
    }
}

/// The verifies and the rebases that wait for another pull request: for
/// each issue of `asked`, the pull requests whose verify or rebase the
/// wave can start, the pull request before it in its group
/// ([`Lane::clashes`]). The pull requests of `first` go first: their
/// verify runs or passed. Then `asked` go in the order of the verify
/// requests of their items ([`Lane::asked`]). A pull request that waits
/// for nobody goes before the next ones.
///
/// ```
/// use std::collections::BTreeMap;
/// use riff::order::{Lane, waits};
///
/// let lane = |issue, files: &[&str], asked| Lane {
///     issue,
///     pull: 40 + issue,
///     files: files.iter().map(|f| f.to_string()).collect(),
///     asked,
///     ..Lane::default()
/// };
/// // The verify request of 2 came first: 1 waits for its pull request #42.
/// let one = lane(1, &["tui.rs"], Some(5));
/// let two = lane(2, &["tui.rs", "view.rs"], Some(3));
/// let three = lane(3, &["wave.rs"], Some(4));
/// assert_eq!(waits([], [&one, &two, &three]), BTreeMap::from([(1, 42)]));
/// // The verify of 4 runs: each verify with a file of it waits for #44.
/// let four = lane(4, &["wave.rs"], None);
/// assert_eq!(waits([&four], [&one, &two, &three]), BTreeMap::from([(1, 42), (3, 44)]));
/// // A request with no place in the thread comes last.
/// let five = lane(5, &["view.rs"], None);
/// assert_eq!(waits([], [&five, &two]), BTreeMap::from([(5, 42)]));
/// // A rebase takes its place as a verify does (#1801): 2 waits for the rebase of 6.
/// let six = Lane { conflict: true, ..lane(6, &["tui.rs"], Some(1)) };
/// assert_eq!(waits([], [&two, &six]), BTreeMap::from([(2, 46)]));
/// // #1895: the heads of 1 and 2 merge with no conflict. 1 waits for nobody.
/// let clean = Lane { clean: [42].into(), ..one.clone() };
/// assert!(waits([], [&clean, &two]).is_empty());
/// ```
pub fn waits<'a>(
    first: impl IntoIterator<Item = &'a Lane>,
    asked: impl IntoIterator<Item = &'a Lane>,
) -> BTreeMap<u64, u64> {
    let mut ahead: Vec<&Lane> = first.into_iter().collect();
    let mut asked: Vec<&Lane> = asked.into_iter().collect();
    asked.sort_by_key(|lane| (lane.asked.unwrap_or(usize::MAX), lane.pull));
    let mut waits = BTreeMap::new();
    for lane in asked {
        match ahead.iter().find(|before| before.clashes(lane)) {
            Some(before) => {
                waits.insert(lane.issue, before.pull);
            }
            None => ahead.push(lane),
        }
    }
    waits
}

/// The files that the pull request `pull` of `repo` changes at its head
/// commit `head`, with `gh`. It is one REST read for each head
/// ([`Gh::read_while`]): the files of a pull request change only with
/// its head. GitHub lists at most 3,000 files of a pull request.
///
/// ```
/// use riff::pr::Gh;
///
/// let dir = tempfile::tempdir()?;
/// let script = "#!/bin/sh\ncase \"$2\" in\n  repos/o/r/pulls/40/files*) printf 'src/a.rs\\ndocs/b.md\\n' ;;\n  *) exit 1 ;;\nesac\n";
/// let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script));
/// let files = riff::order::files(&gh, "o/r", 40, "1a2b3c4")?;
/// assert_eq!(files, ["docs/b.md".to_owned(), "src/a.rs".to_owned()].into());
/// assert!(riff::order::files(&gh, "o/r", 41, "5e6f7a8").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn files(gh: &Gh, repo: &str, pull: u64, head: &str) -> Result<BTreeSet<String>> {
    let path = format!("repos/{repo}/pulls/{pull}/files?per_page=100");
    let args = ["api", &path, "--paginate", "--jq", ".[].filename"];
    let reply = gh.read_while(&args, head, Gh::LONG)?;
    Ok(reply
        .lines()
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The most time of the fetch of the heads of [`merges`].
pub const FETCH_WAIT: Duration = Duration::from_secs(60);

/// The merge test of each two pull requests of `lanes` that change a file
/// in common (#1895, 01M750ED646BKBZSA9KHF0953Y): `git merge-tree` of
/// their two head commits in the main clone `main`. Two heads that merge
/// with no conflict go into [`Lane::clean`] of each other. One fetch of
/// `refs/pull/N/head` from `origin` brings each head that the clone does
/// not have, also a head from a fork. A test that cannot run leaves the
/// two in one group, for example with no clone, with a failed fetch, or
/// with a head that moved after the look.
///
/// ```
/// use std::collections::BTreeMap;
/// use std::process::Command;
/// use riff::order::{Lane, merges};
///
/// let dir = tempfile::tempdir()?;
/// let main = dir.path();
/// let git = |args: &[&str]| {
///     let out = Command::new("git")
///         .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
///         .args(args)
///         .current_dir(main)
///         .output()
///         .unwrap();
///     assert!(out.status.success(), "git {args:?}: {out:?}");
///     String::from_utf8_lossy(&out.stdout).trim().to_owned()
/// };
/// // A page of two sections. Each head adds a line to the end of one.
/// let page = |a: &str, b: &str| format!("## A\n\n- one\n{a}\n## B\n\n- two\n{b}");
/// git(&["init", "-q", "-b", "main"]);
/// std::fs::write(main.join("requirements.md"), page("", ""))?;
/// git(&["add", "."]);
/// git(&["commit", "-q", "-m", "base"]);
/// let head = |name: &str, text: String| {
///     git(&["switch", "-q", "-c", name, "main"]);
///     std::fs::write(main.join("requirements.md"), text).unwrap();
///     git(&["commit", "-q", "-am", name]);
///     git(&["rev-parse", "HEAD"])
/// };
/// let (a, b, c) = (
///     head("a", page("- three\n", "")),
///     head("b", page("", "- four\n")),
///     head("c", page("- five\n", "")),
/// );
/// let lane = |issue, head: &str| {
///     let files = ["requirements.md".to_owned()].into();
///     (issue, Lane { issue, pull: 40 + issue, head: head.into(), files, ..Lane::default() })
/// };
/// let mut lanes: BTreeMap<u64, Lane> =
///     [lane(1, &a), lane(2, &b), lane(3, &c), lane(4, "1a2b3c4d")].into();
/// merges(main, "main", &mut lanes);
/// // 1 and 2 add to two sections: no conflict. 1 and 3 add to one: a conflict.
/// assert_eq!(lanes[&1].clean, [42].into());
/// assert_eq!(lanes[&2].clean, [41, 43].into());
/// assert!(!lanes[&1].clashes(&lanes[&2]));
/// assert!(lanes[&1].clashes(&lanes[&3]));
/// // The clone has no commit 1a2b3c4d, and no `origin` to fetch it from.
/// assert!(lanes[&4].clean.is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// A head with a conflict with the default branch `branch`
/// ([`Lane::conflict`]) conflicts with each newer head that holds the
/// change of the default branch (#2128,
/// 01M7E06YMNMF92TDW21XED5JZ8). So the test of such a head and a head
/// with no conflict takes as its base the merge base of the other head
/// and `refs/remotes/origin/BRANCH`, after a fetch of the branch: only a
/// conflict of the changes of the two pull requests counts. The test puts
/// the changes of the stale head over the lines of the default branch,
/// and keeps the lines of both where the two changed the same lines
/// (01M7E7ER1Z43M1Q07GWT8RSBMC). So a
/// newer head that changes lines next to a change of the default branch
/// does not clash with the stale head. Two heads with a conflict each, or
/// with none, have the test of `git merge-tree` with no base.
///
/// ```
/// use std::collections::BTreeMap;
/// use std::process::Command;
/// use riff::order::{Lane, merges};
///
/// let dir = tempfile::tempdir()?;
/// let main = dir.path();
/// let git = |args: &[&str]| {
///     let out = Command::new("git")
///         .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
///         .args(args)
///         .current_dir(main)
///         .output()
///         .unwrap();
///     assert!(out.status.success(), "git {args:?}: {out:?}");
///     String::from_utf8_lossy(&out.stdout).trim().to_owned()
/// };
/// let write = |name: &str, text: &str| std::fs::write(main.join(name), text).unwrap();
/// git(&["init", "-q", "-b", "main"]);
/// write("lib.rs", "one\ntwo\nthree\n");
/// write("wave.rs", "a\nb\n");
/// git(&["add", "."]);
/// git(&["commit", "-q", "-m", "base"]);
/// // The stale head changes line one of lib.rs. main then changes it too,
/// // and adds a line to wave.rs.
/// git(&["switch", "-q", "-c", "stale"]);
/// write("lib.rs", "one of the stale head\ntwo\nthree\n");
/// git(&["commit", "-q", "-am", "stale"]);
/// let stale = git(&["rev-parse", "HEAD"]);
/// git(&["switch", "-q", "main"]);
/// write("lib.rs", "one of main\ntwo\nthree\n");
/// write("wave.rs", "a\nb\nc of main\n");
/// git(&["commit", "-q", "-am", "main moves"]);
/// git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
/// // Two newer heads on main: one adds a line next to the line of main in
/// // wave.rs, the other changes line one of lib.rs.
/// let fresh = |name: &str, lib: &str, wave: &str| {
///     git(&["switch", "-q", "-c", name, "main"]);
///     write("lib.rs", lib);
///     write("wave.rs", wave);
///     git(&["commit", "-q", "-am", name]);
///     git(&["rev-parse", "HEAD"])
/// };
/// let next = fresh("next", "one of main\ntwo\nthree\n", "a\nb\nc of main\nd of next\n");
/// let same = fresh("same", "one of same\ntwo\nthree\n", "a\nb\nc of main\n");
/// let lane = |issue, head: &str, conflict| {
///     let files = ["lib.rs".to_owned(), "wave.rs".to_owned()].into();
///     (issue, Lane { issue, pull: 860 + issue, head: head.into(), files, conflict, ..Lane::default() })
/// };
/// // A plain merge-tree of the stale head and each newer head gives a conflict.
/// let plain = |head: &str| Command::new("git").current_dir(main).args(["merge-tree", "--write-tree", &stale, head]).output().unwrap().status.code();
/// assert_eq!((plain(&next), plain(&same)), (Some(1), Some(1)));
/// let mut lanes: BTreeMap<u64, Lane> = [lane(1, &stale, true), lane(2, &next, false), lane(3, &same, false)].into();
/// merges(main, "main", &mut lanes);
/// // The clash of 1 with 2 is only of main. 1 and 3 change line one of lib.rs both.
/// assert_eq!(lanes[&1].clean, [862].into());
/// assert!(!lanes[&1].clashes(&lanes[&2]));
/// assert!(lanes[&1].clashes(&lanes[&3]));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn merges(main: &Path, branch: &str, lanes: &mut BTreeMap<u64, Lane>) {
    let pairs: Vec<(u64, u64)> = lanes
        .values()
        .flat_map(|one| {
            lanes
                .values()
                .filter(move |two| one.pull < two.pull && one.shares(two))
                .map(move |two| (one.issue, two.issue))
        })
        .collect();
    let tested: BTreeSet<u64> = pairs.iter().flat_map(|&(one, two)| [one, two]).collect();
    let mut missing: Vec<String> = tested
        .iter()
        .filter_map(|issue| lanes.get(issue))
        .filter(|lane| commit(&lane.head) && !has(main, &lane.head))
        .map(|lane| format!("refs/pull/{}/head", lane.pull))
        .collect();
    // A pair of a head with a conflict and a head with none tests over the
    // default branch (#2128): the fetch brings it too.
    let stale = |one: &Lane, two: &Lane| one.conflict != two.conflict;
    let over = branch_name(branch)
        && pairs
            .iter()
            .any(|(one, two)| match (lanes.get(one), lanes.get(two)) {
                (Some(a), Some(b)) => stale(a, b),
                _ => false,
            });
    let tip = format!("refs/remotes/origin/{branch}");
    if over {
        missing.push(format!("+refs/heads/{branch}:{tip}"));
    }
    if !missing.is_empty() {
        let mut args = vec!["fetch", "-q", "--no-write-fetch-head", "origin"];
        args.extend(missing.iter().map(String::as_str));
        // A failed fetch brings no head: the merges of those heads do not
        // run, and their pull requests stay in one group. So does a fetch
        // that would ask a locked `gh` for a credential
        // (01M78TSH2VNHA75RR1HXC5Z27G).
        if crate::git_credential::keyring_open(main, "origin", &args).is_ok() {
            let _ = crate::host_git::within(main, &args, FETCH_WAIT);
        }
    }
    let union = over.then(union_attributes).flatten();
    for (one, two) in pairs {
        let (Some(a), Some(b)) = (lanes.get(&one), lanes.get(&two)) else {
            continue;
        };
        let merge = match (a.conflict, b.conflict, &union) {
            (true, false, Some(file)) => clean_over(main, &a.head, &b.head, &tip, file.path()),
            (false, true, Some(file)) => clean_over(main, &b.head, &a.head, &tip, file.path()),
            _ => clean(main, &a.head, &b.head),
        };
        if !merge {
            continue;
        }
        let (a, b) = (a.pull, b.pull);
        if let Some(lane) = lanes.get_mut(&one) {
            lane.clean.insert(b);
        }
        if let Some(lane) = lanes.get_mut(&two) {
            lane.clean.insert(a);
        }
    }
}

/// True when `head` has the form of a commit: hex digits only.
fn commit(head: &str) -> bool {
    !head.is_empty() && head.bytes().all(|b| b.is_ascii_hexdigit())
}

/// True when the clone `main` has the commit `head`.
fn has(main: &Path, head: &str) -> bool {
    git_code(main, &["cat-file", "-e", &format!("{head}^{{commit}}")]) == Some(0)
}

/// True when `git merge-tree` of the commits `one` and `two` in the clone
/// `main` gives no conflict. Its code 1 is a conflict, and each other code
/// but 0 is no test, for example with a commit that the clone does not
/// have, or with a `git` older than `--write-tree`.
fn clean(main: &Path, one: &str, two: &str) -> bool {
    let args = [
        "merge-tree",
        "--write-tree",
        "--no-messages",
        "--name-only",
        one,
        two,
    ];
    commit(one) && commit(two) && git_code(main, &args) == Some(0)
}

/// True when the changes of the pull request of the head `stale`, which
/// has a conflict with the default branch, merge with the changes of the
/// pull request of the head `fresh`, which has none (#2128): a three-way
/// merge of the two over the merge base of `fresh` and `tip`, the default
/// branch in the clone `main`. A conflict that only the default branch
/// gives `stale` does not count. No merge base is no test.
///
/// `git merge-tree --merge-base` needs git 2.40. So the test makes a
/// stand-in commit on that merge base, with a fixed author and time, and
/// gives it to `git merge-tree` ([`clean`]): the merge base of the
/// stand-in and `fresh` is that base. The tree of the stand-in is a merge
/// of `stale` into the base, with the attributes file `union`
/// ([`union_attributes`]): it has the lines of the default branch and the
/// changes of `stale`. Where the two changed the same lines, it has the
/// lines of both, those of `stale` first. So a change of `fresh` next to a
/// change of the default branch merges, and a change of `fresh` next to a
/// change of `stale` does not.
fn clean_over(main: &Path, stale: &str, fresh: &str, tip: &str, union: &Path) -> bool {
    if !(commit(stale) && commit(fresh)) {
        return false;
    }
    let Some(base) = git_line(main, &["merge-base", fresh, tip]).filter(|b| commit(b)) else {
        return false;
    };
    let attributes = format!("core.attributesFile={}", union.display());
    let merge = [
        "-c",
        &attributes,
        "merge-tree",
        "--write-tree",
        "--no-messages",
        "--name-only",
        stale,
        &base,
    ];
    // Code 1 is a conflict that a union does not solve, for example of a
    // file that one side removes: the tree has one side of it.
    let Some(tree) = git_first(main, &merge)
        .filter(|(code, tree)| matches!(code, 0 | 1) && commit(tree))
        .map(|(_, tree)| tree)
    else {
        return false;
    };
    let args = [
        "-c",
        "user.name=riff",
        "-c",
        "user.email=riff@localhost",
        "commit-tree",
        "--no-gpg-sign",
        &tree,
        "-p",
        &base,
        "-m",
        "riff: the head over the default branch",
    ];
    git_line(main, &args)
        .filter(|stand_in| commit(stand_in))
        .is_some_and(|stand_in| clean(main, &stand_in, fresh))
}

/// A new attributes file that gives each path the merge driver `union`
/// of git: a merge keeps the lines of both sides of a conflict, with no
/// markers. A path can name another driver in its `.gitattributes`.
/// `None` when the file cannot be written: then the pull requests stay
/// in one group.
fn union_attributes() -> Option<tempfile::NamedTempFile> {
    let mut file = tempfile::NamedTempFile::new().ok()?;
    std::io::Write::write_all(&mut file, b"* merge=union\n").ok()?;
    Some(file)
}

/// True when `name` can be a branch of the forge in a refspec: not empty,
/// with no `-` first, and only letters, digits and `/._-`. Another name
/// gives no fetch and no test over the branch.
///
/// ```
/// assert!(riff::order::branch_name("main"));
/// assert!(riff::order::branch_name("release/2.x"));
/// assert!(!riff::order::branch_name(""));
/// assert!(!riff::order::branch_name("--upload-pack=x"));
/// assert!(!riff::order::branch_name("a:b"));
/// ```
pub fn branch_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
}

/// The exit code of `git ARGS` in `dir`, with no prompt and no output.
/// `None` when `git` does not run, or a signal ends it.
fn git_code(dir: &Path, args: &[&str]) -> Option<i32> {
    crate::host_git::git(dir)
        .ok()?
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?
        .code()
}

/// The first line of the output of `git ARGS` in `dir`, with no prompt,
/// when it ends with code 0 ([`git_first`]).
fn git_line(dir: &Path, args: &[&str]) -> Option<String> {
    git_first(dir, args).and_then(|(code, line)| (code == 0).then_some(line))
}

/// The exit code and the first line of the output of `git ARGS` in
/// `dir`, with no prompt. A commit that it makes has a fixed time, so the
/// same tree and parent give the same commit. `None` when `git` does not
/// run, or a signal ends it.
fn git_first(dir: &Path, args: &[&str]) -> Option<(i32, String)> {
    const TIME: &str = "1700000000 +0000";
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_DATE", TIME)
        .env("GIT_COMMITTER_DATE", TIME)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next().unwrap_or_default().trim().to_owned();
    Some((out.status.code()?, line))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane(issue: u64, files: &[&str], passed: bool, asked: Option<usize>) -> Lane {
        Lane {
            issue,
            pull: 40 + issue,
            files: files.iter().map(|f| (*f).to_owned()).collect(),
            passed,
            asked,
            ..Lane::default()
        }
    }

    /// Wave 50: the tab items each change `tui.rs`. One verify at a time,
    /// in the order of the requests.
    #[test]
    fn the_verifies_of_pull_requests_with_a_file_in_common_go_one_at_a_time() {
        let items: Vec<Lane> = (1..=4)
            .map(|n| lane(n, &["crates/riff/src/tui.rs"], false, Some(n as usize)))
            .collect();
        assert_eq!(
            waits([], &items),
            BTreeMap::from([(2, 41), (3, 41), (4, 41)])
        );
        // The verify of 1 passed: the others still wait for its merge.
        let passed = Lane {
            passed: true,
            ..items[0].clone()
        };
        assert_eq!(
            waits([&passed], &items[1..]),
            BTreeMap::from([(2, 41), (3, 41), (4, 41)])
        );
        // 1 merged: 2 goes, the others wait for it.
        assert_eq!(waits([], &items[1..]), BTreeMap::from([(3, 42), (4, 42)]));
    }

    #[test]
    fn the_verifies_of_pull_requests_with_no_file_in_common_go_side_by_side() {
        let a = lane(1, &["a.rs"], false, Some(1));
        let b = lane(2, &["b.rs"], false, Some(2));
        let unread = lane(3, &[], false, Some(0));
        let running = lane(4, &["c.rs"], false, None);
        assert!(waits([&running], [&a, &b, &unread]).is_empty());
    }

    /// #1895, Wave 54: the pull requests 1 to 4 each change
    /// `requirements.md`. The heads of 1, 2 and 3 merge with no conflict,
    /// and the head of 4 conflicts with the head of 1. The verifies of 1,
    /// 2 and 3 go side by side, and the verify of 4 waits for 1.
    #[test]
    fn the_verifies_of_pull_requests_whose_heads_merge_with_no_conflict_go_side_by_side() {
        let page = &["docs/src/requirements.md"];
        let with = |issue: u64, clean: &[u64]| Lane {
            clean: clean.iter().map(|n| 40 + n).collect(),
            ..lane(issue, page, false, Some(issue as usize))
        };
        let lanes = [
            with(1, &[2, 3]),
            with(2, &[1, 3, 4]),
            with(3, &[1, 2, 4]),
            with(4, &[2, 3]),
        ];
        assert_eq!(waits([], &lanes), BTreeMap::from([(4, 41)]));
        // The verify of 1 passed: 4 still waits for its merge.
        let passed = Lane {
            passed: true,
            ..lanes[0].clone()
        };
        assert_eq!(waits([&passed], &lanes[1..]), BTreeMap::from([(4, 41)]));
        // 1 merged: the verify of 4 starts.
        assert!(waits([], &lanes[1..]).is_empty());
    }

    /// A verify waits for the first pull request before it with a file in
    /// common, also when that one waits itself.
    #[test]
    fn a_verify_waits_for_the_first_pull_request_with_a_file_in_common() {
        let first = lane(1, &["a.rs"], true, None);
        let second = lane(2, &["a.rs", "b.rs"], false, Some(1));
        let third = lane(3, &["b.rs"], false, Some(2));
        // 2 waits for 1, and 3 starts: it shares no file with 1.
        assert_eq!(
            waits([&first], [&second, &third]),
            BTreeMap::from([(2, 41)])
        );
        // With no 1, 2 starts and 3 waits for it.
        assert_eq!(waits([], [&second, &third]), BTreeMap::from([(3, 42)]));
    }

    /// #1801: the merge of 1 gave 2 and 3 a conflict. Only the rebase of 2
    /// starts. While a session rebases 2, and after its push, 3 waits.
    #[test]
    fn only_the_next_rebase_of_the_order_starts() {
        let conflict = |issue, asked| Lane {
            conflict: true,
            ..lane(issue, &["tui.rs"], false, Some(asked))
        };
        let (two, three) = (conflict(2, 2), conflict(3, 3));
        assert_eq!(waits([], [&two, &three]), BTreeMap::from([(3, 42)]));
        // A session rebases 2: it keeps its place (#1894).
        assert_eq!(waits([], [&three, &two]), BTreeMap::from([(3, 42)]));
        // The new head of 2 keeps the place of its item, and waits for a verify.
        let pushed = lane(2, &["tui.rs"], false, Some(2));
        assert_eq!(waits([], [&three, &pushed]), BTreeMap::from([(3, 42)]));
        // 2 merged: the rebase of 3 starts.
        assert!(waits([], [&three]).is_empty());
    }

    /// The forms of a commit that the merge test takes: no option and no
    /// path reaches `git`.
    #[test]
    fn only_hex_digits_are_a_commit() {
        assert!(commit("1a2b3c4d"));
        assert!(!commit(""));
        assert!(!commit("--output=x"));
        assert!(!commit("main"));
    }

    /// A merge test with no clone gives no clean pair: the pull requests
    /// stay in one group.
    #[test]
    fn a_merge_test_with_no_clone_keeps_the_group() {
        let dir = tempfile::tempdir().unwrap();
        let none = dir.path().join("none");
        let with = |issue| Lane {
            head: format!("{issue}a2b3c4"),
            ..lane(issue, &["a.rs"], false, Some(issue as usize))
        };
        let mut lanes: BTreeMap<u64, Lane> = [(1, with(1)), (2, with(2))].into();
        merges(&none, "main", &mut lanes);
        assert!(lanes.values().all(|lane| lane.clean.is_empty()));
        assert_eq!(waits([], lanes.values()), BTreeMap::from([(2, 41)]));
    }
}
