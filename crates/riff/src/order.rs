//! The order of the verifies of a wave (#1710, 01M6WCGNCC3HWS47158WTWNQ8B):
//! the pull requests of the items that change the same files get their
//! verifies one at a time, and their rebases too (#1801,
//! 01M6YPYTGR2JEBDHGX4K1RDCAW, 01M6YPYTKQKSJRS0A8WZQRCCFD).
//!
//! # Design
//!
//! A merge can give each other open pull request that changes the same
//! files a conflict. A conflict costs a rebase session, a new full check
//! and a new verify, and a verify that passed before the conflict is
//! lost. So the wave ([`crate::wave::pick`]) starts the verify or the
//! rebase of one of them, and the others wait until it merges or fails:
//!
//! - A pull request goes first while its verify runs, while a session
//!   holds its item, for example to rebase it, and while its verify
//!   passed and nothing stops its merge ([`Lane::passed`]). A pull request
//!   that cannot merge by itself for another reason
//!   ([`crate::rollout::Stuck`]) does not go first, for example with a
//!   failed check or with auto-merge off.
//! - Then come the pull requests that wait for a verify or for a rebase
//!   ([`Lane::conflict`]). The one whose item had its first verify request
//!   first goes first ([`Lane::asked`], [`crate::wave::requests`]): a new
//!   head after a rebase or a fix keeps the place. A pull request with no
//!   request in the thread comes after those with one. A tie goes by the
//!   number of the pull request.
//! - A verify or a rebase waits for the first pull request before it that
//!   shares a file with it ([`waits`]). So only the next pull request of
//!   the order gets a rebase, and the others keep their conflict until
//!   their turn. A pull request that shares no file starts beside the
//!   others.
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
//! message of its conflict ([`crate::look::PullWatch::news`]).
//!
//! ```mermaid
//! flowchart TD
//!     A["verify requests: PR #41, then PR #42"] --> S["the verify of #41 starts"]
//!     S --> Q{"#42 changes a file that #41 changes?"}
//!     Q -- no --> B["the verify of #42 starts beside it"]
//!     Q -- yes --> W["#42: verify waits for #41"]
//!     W --> E{"#41"}
//!     E -- "merges" --> R["#42: a rebase when it has a conflict,<br/>then its verify"]
//!     E -- "its verify fails, or it cannot merge" --> V["the verify of #42 starts"]
//! ```
//!
//! Three pull requests that change one file, in the order #41, #42, #43:
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
    /// The files that it changes.
    pub files: BTreeSet<String>,
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
}

/// The verifies and the rebases that wait for another pull request: for
/// each issue of `asked`, the pull requests whose verify or rebase the
/// wave can start, the pull request before it that shares a file with it.
/// The pull requests of `first` go first: their verify runs or passed, or
/// a session rebases them. Then `asked` go in the order of the verify
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
        match ahead.iter().find(|before| before.shares(lane)) {
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
        // A session rebases 2: it goes first.
        assert_eq!(waits([&two], [&three]), BTreeMap::from([(3, 42)]));
        // The new head of 2 keeps the place of its item, and waits for a verify.
        let pushed = lane(2, &["tui.rs"], false, Some(2));
        assert_eq!(waits([], [&three, &pushed]), BTreeMap::from([(3, 42)]));
        // 2 merged: the rebase of 3 starts.
        assert!(waits([], [&three]).is_empty());
    }
}
