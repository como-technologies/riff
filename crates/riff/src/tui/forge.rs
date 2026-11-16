//! The reads of the forge for the Waves tab, the Sessions tab and the
//! Decisions tab of the TUI (#1638, #1707, #2188,
//! 01M6VFGK4P2FWMFQHBZKR3JXSZ, 01M6VT7TXBW8EJKY0FTYQ0H49M).
//!
//! # Design
//!
//! The Waves tab reads `gh` each [`READ_EVERY`] while the person sees
//! it, and at once on `r`. While another tab shows, it reads `gh` each
//! [`super::reads::AWAY_EVERY`] ([`super::reads`]). A read of a
//! repository is one run of blocking calls ([`read`]). Its calls go to
//! the tally of the daemon as the caller `tui` ([`crate::pr::Gh::here`]).
//! The calls:
//!
//! | What | Call | Cost |
//! |---|---|---|
//! | The open issues, their titles, waves, labels and `Needs:` lines | a conditional REST read of each page of `repos/R/issues?state=open` ([`open_path`]) | A `304` costs no point. A new page costs one REST call |
//! | The pull requests that wait or are stuck | `gh pr list` with the checks ([`crate::rollout::pulls`]), only when the list of issues has an open pull request, and the ruleset of `main` when a head has a failed check ([`crate::rollout::Pull::required`]) | About 2 GraphQL points. The ruleset is two REST calls, one time in 10 minutes |
//! | The last 100 merged pull requests, as the free work reads them | `gh pr list --state merged` ([`crate::rollout::merged_pulls`]), only when the list of the open issues changed since the last read of them ([`Kept::merges`]) | About 1 GraphQL point |
//! | The comments of each open issue of the current wave that has a comment | a conditional REST read of `repos/R/issues/N/comments` ([`comments_path`]) | A `304` costs no point |
//! | The items that closed in the current wave | a conditional REST read of `repos/R/issues?state=closed&milestone=N` ([`closed_path`]) | A `304` costs no point |
//! | The full check of `main` | the check runs of each required check ([`crate::look::read_main`]), and the jobs of a run that goes or failed | A few REST calls |
//!
//! The REST list of the issues of GitHub has the pull requests too: an
//! entry with the key `pull_request`. The read drops them from the items
//! ([`Forge::of`]). It keeps only their number and their last update, for
//! the age of the row of the item of each pull request. So the read
//! knows with no other call whether a pull request is open.
//!
//! The checks and the merge state of a pull request do not change its
//! entry in the list (#1566). So while a pull request is open, each read
//! calls `gh pr list`, as `riff top` does each minute. With no open pull
//! request, a read that changed nothing costs no point.
//!
//! The merged pull requests and the comments give the board the facts of
//! `riff wave list` (01M7DYX2Q2VJW5V4RNQEEHCVKS,
//! 01M7E25WB42HQQ34CRNXPZFHHR). An open issue that a merged pull request
//! keeps out of the free work ([`crate::rollout::Issue::merged_pull`]) is
//! in [`Issues::merged`] ([`crate::top::issues_of`]), and the Waves tab
//! shows it as a `merged` row ([`super::waves`]). A merge takes its pull
//! request out of the list of the open issues, so it changes the `ETag`
//! of a page of that list. While each page gives the `ETag` of the last
//! read of the merged pull requests, the read keeps them. The wave adds
//! the comment `Merged in #PR` at its first look after a merge, so an
//! older merge counts by its comment. The comment `More work after #PR`
//! of `riff wave next` takes the pull request out again.
//!
//! [`Kept`] keeps the `ETag` and the body of each conditional read, so a
//! `304` gives the body of the last reply again. A read that fails keeps
//! the last facts of its repository ([`riff_client::waves::Issues::newest`]).
//!
//! The Sessions tab reads the titles of the open issues with the same
//! conditional read, and with no other call ([`titles`]). The two tabs
//! share one [`Kept`], so a read of one tab after a read of the other
//! gets a `304` too. The list of `#` of the Chat tab and of `riff chat`
//! reads the open issues in the same way ([`open_issues`]), and so does
//! the Decisions tab ([`decisions`]).
//!
//! An open issue with the label [`crate::cycle::LABEL`] is a release
//! whose checks after the deploy have not passed: the release issue
//! closes only when `riff release check` passes ([`crate::cycle`]).
//!
//! ```mermaid
//! flowchart TD
//!     R["a read of a repository"] --> O["the open issues: each page,<br/>with its ETag (a 304 is free)"]
//!     O --> P{"an open pull request<br/>in the list?"}
//!     P -- yes --> L["gh pr list with the checks"]
//!     P -- no --> G
//!     L --> G{"the ETags of the open issues<br/>as at the last read of the merged ones?"}
//!     G -- no --> D["gh pr list --state merged"]
//!     G -- yes --> S
//!     D --> S["the comments of each issue of the current wave<br/>with a comment, with their ETag"]
//!     S --> C["the closed items of the current wave,<br/>with their ETag"]
//!     C --> M["the full check of main"]
//!     M --> F["Forge: the facts of the board"]
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use riff_client::waves::{Issues, wave_number};
use riff_core::name::ThreadName;
use serde::Deserialize;

use super::decisions::Decision;
use crate::pr::{Gate, Gh, Reply};
use crate::rollout::{Comment, Label, Merge, MergedPull, Pull, Verify};
use crate::typeahead::OpenIssue;

/// The time between two reads of `gh` of the tab that shows
/// ([`super::reads::Read::every`]).
pub const READ_EVERY: Duration = Duration::from_secs(60);

/// The entries of one page of a REST list.
pub const PER_PAGE: usize = 100;

/// The most pages of open issues that a read reads: 1,000 issues and pull
/// requests, as `gh issue list --limit 1000` of `riff top`.
pub const MOST_PAGES: u32 = 10;

/// The title of the milestone of the items for later waves.
pub const BACKLOG: &str = "Backlog";

/// The read `read` of each repository of `repos`, in order, with the
/// error of `gh` in one line: [`read`] for the Waves tab,
/// [`open_issues`] for the Chat tab, and [`decisions`] for the Decisions
/// tab.
///
/// ```
/// use riff::tui::forge::each;
///
/// let read = each(&["o/r", "o/s"], |repo| match *repo {
///     "o/r" => Ok(1),
///     _ => Err(anyhow::anyhow!("gh: HTTP 502")),
/// });
/// assert_eq!(read, [("o/r", Ok(1)), ("o/s", Err("gh: HTTP 502".to_owned()))]);
/// ```
pub fn each<K: Clone, T>(
    repos: &[K],
    read: impl Fn(&K) -> Result<T>,
) -> Vec<(K, Result<T, String>)> {
    repos
        .iter()
        .map(|repo| (repo.clone(), read(repo).map_err(|e| format!("{e:#}"))))
        .collect()
}

/// The last good read of `gh` of each repository `K`, the time of the
/// last read, and the fault of the last read that failed, while it holds:
/// the reads of the Waves tab and of the Decisions tab
/// (01M6VFGK4P2FWMFQHBZKR3JXSZ, 01M7G7SHRDEA6BCNSYQA8RGZ23).
///
/// ```
/// use riff::tui::forge::Reads;
///
/// let mut reads: Reads<&str, u32> = Reads::default();
/// assert_eq!(reads.take(vec![("o/r", Ok(1)), ("o/s", Ok(2))], 1_000), None);
/// assert_eq!(reads.at(), Some(1_000));
/// // A read that fails keeps the last read of its repository, and the
/// // note line gets its fault once.
/// let failed = || vec![("o/r", Ok(3)), ("o/s", Err("gh: HTTP 502".to_owned()))];
/// assert_eq!(reads.take(failed(), 60_000).as_deref(), Some("riff: gh: HTTP 502"));
/// assert_eq!(reads.take(failed(), 120_000), None);
/// assert_eq!((reads.get(&"o/r"), reads.get(&"o/s")), (Some(&3), Some(&2)));
/// // A repository that the read does not name goes.
/// reads.take(vec![("o/r", Ok(4))], 180_000);
/// assert_eq!(reads.iter().collect::<Vec<_>>(), [(&"o/r", &4)]);
/// assert_eq!(reads.at(), Some(180_000));
/// ```
#[derive(Debug, Clone)]
pub struct Reads<K, T> {
    /// The last good read of each repository.
    each: BTreeMap<K, T>,
    /// The time of the last read, in milliseconds since the epoch.
    at_ms: Option<i64>,
    /// The fault of the last read that failed, while it holds.
    fault: Option<String>,
}

impl<K, T> Default for Reads<K, T> {
    fn default() -> Self {
        Reads {
            each: BTreeMap::new(),
            at_ms: None,
            fault: None,
        }
    }
}

impl<K: Ord, T> Reads<K, T> {
    /// Takes the read `read` of each repository at `at_ms`. A good read
    /// replaces the last one of its repository. A read that failed keeps
    /// it. A repository that `read` does not name goes. It gives the line
    /// of the first fault when it is new, for the note line.
    pub fn take(&mut self, read: Vec<(K, Result<T, String>)>, at_ms: i64) -> Option<String> {
        let mut last = std::mem::take(&mut self.each);
        let mut fault = None;
        for (repo, read) in read {
            match read {
                Ok(found) => {
                    self.each.insert(repo, found);
                }
                Err(e) => {
                    fault.get_or_insert(crate::text::top_gh_fault(&e));
                    if let Some(found) = last.remove(&repo) {
                        self.each.insert(repo, found);
                    }
                }
            }
        }
        self.at_ms = Some(at_ms);
        let new = fault.clone().filter(|f| self.fault.as_ref() != Some(f));
        self.fault = fault;
        new
    }

    /// The last good read of `repo`.
    pub fn get<Q: Ord + ?Sized>(&self, repo: &Q) -> Option<&T>
    where
        K: std::borrow::Borrow<Q>,
    {
        self.each.get(repo)
    }

    /// The last good read of each repository, in the order of `K`.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &T)> {
        self.each.iter()
    }

    /// The time of the last read.
    pub fn at(&self) -> Option<i64> {
        self.at_ms
    }
}

/// The REST path of the page `page` of the open issues of `repo`.
///
/// ```
/// assert_eq!(
///     riff::tui::forge::open_path("o/r", 2),
///     "repos/o/r/issues?state=open&per_page=100&page=2"
/// );
/// ```
pub fn open_path(repo: &str, page: u32) -> String {
    format!("repos/{repo}/issues?state=open&per_page={PER_PAGE}&page={page}")
}

/// The REST path of the closed issues of the milestone `milestone` of
/// `repo`.
///
/// ```
/// assert_eq!(
///     riff::tui::forge::closed_path("o/r", 54),
///     "repos/o/r/issues?state=closed&milestone=54&per_page=100"
/// );
/// ```
pub fn closed_path(repo: &str, milestone: u64) -> String {
    format!("repos/{repo}/issues?state=closed&milestone={milestone}&per_page={PER_PAGE}")
}

/// The REST path of the comments of the issue `issue` of `repo`: its
/// first 100 comments.
///
/// ```
/// assert_eq!(
///     riff::tui::forge::comments_path("o/r", 872),
///     "repos/o/r/issues/872/comments?per_page=100"
/// );
/// ```
pub fn comments_path(repo: &str, issue: u64) -> String {
    format!("repos/{repo}/issues/{issue}/comments?per_page={PER_PAGE}")
}

/// An entry of the REST list of the issues: an issue, or a pull request.
#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub number: u64,
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub title: String,
    #[serde(default)]
    pub labels: Vec<Label>,
    #[serde(default)]
    pub milestone: Option<Milestone>,
    /// Set for a pull request.
    #[serde(default)]
    pub pull_request: Option<serde::de::IgnoredAny>,
    /// `completed`, `not_planned` or `duplicate`, for a closed issue.
    #[serde(default, deserialize_with = "crate::text::forge_de_opt")]
    pub state_reason: Option<String>,
    /// The last update, for example `2026-11-07T10:00:00Z`.
    #[serde(default, deserialize_with = "crate::text::forge_de_opt")]
    pub updated_at: Option<String>,
    /// The time it opened, for example `2026-11-07T10:00:00Z`.
    #[serde(default, deserialize_with = "crate::text::forge_de_opt")]
    pub created_at: Option<String>,
    /// The body. Only its `Needs:` line ([`crate::rollout::needs`]), its
    /// `Live:` line ([`crate::rollout::has_live_check`]) and its headings
    /// `Questions for PERSON` ([`super::decisions::names`]) count.
    #[serde(default)]
    pub body: Option<String>,
    /// The number of its comments.
    #[serde(default)]
    pub comments: u64,
    /// Its comments, when [`read`] read them: only for an issue of the
    /// current wave that has a comment ([`comments_path`]).
    #[serde(skip)]
    pub said: Vec<Comment>,
    /// The merge of its merged pull request, when [`read`] found one
    /// ([`Kept::merges`]).
    #[serde(skip)]
    pub merge: Option<Merge>,
}

impl Entry {
    /// The open issue of the entry, as the free work of the wave reads it
    /// ([`crate::rollout::Issue`]): with its comments and its merge, when
    /// [`read`] found them.
    fn open(self) -> crate::rollout::Issue {
        crate::rollout::Issue {
            number: self.number,
            title: self.title,
            body: self.body.unwrap_or_default(),
            comments: self.said,
            milestone: self
                .milestone
                .map(|m| crate::rollout::Milestone { title: m.title }),
            labels: self.labels,
            merge: self.merge,
        }
    }
}

/// The milestone of an entry.
#[derive(Debug, Clone, Deserialize)]
pub struct Milestone {
    pub number: u64,
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub title: String,
}

/// The pull request of an item, as its row shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemPull {
    /// The number of the pull request.
    pub number: u64,
    /// What it waits for: `verify`, `merge`, `build` for a draft or a WIP
    /// head, `failed` for a failed verify, or why it is stuck, for
    /// example `conflict` ([`crate::rollout::Stuck::word`]).
    pub word: String,
    /// The time of its last update, in seconds since the epoch.
    pub updated: Option<i64>,
}

/// An item that closed in the current wave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closed {
    pub number: u64,
    pub title: String,
    /// True when it closed as completed: done. Else it closed as not
    /// planned or as a duplicate.
    pub done: bool,
}

/// The full check of the head of `main`, when it needs a look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Main {
    /// The full check of `commit` runs: `done` of its `jobs` jobs ended.
    /// No jobs when riff could not read them.
    Runs {
        commit: String,
        done: usize,
        jobs: usize,
    },
    /// The full check of `commit` failed: `main` is broken. `failed` names
    /// the jobs that did not pass.
    Broken { commit: String, failed: Vec<String> },
    /// The full check of `commit` ended with no success and no failure,
    /// for example `cancelled`.
    Ended { commit: String, conclusion: String },
}

impl Main {
    /// The line of the full check of `main` (01M6VFGJZF7XGNGVWTECKAH429).
    /// `short` gives the line of the narrow layout.
    ///
    /// ```
    /// use riff::tui::forge::Main;
    ///
    /// let runs = Main::Runs { commit: "3f2a91c0ffee".into(), done: 4, jobs: 9 };
    /// assert_eq!(runs.line(false), "main: the full check of 3f2a91c runs · 4 of 9 jobs done");
    /// assert_eq!(runs.line(true), "main: 3f2a91c runs · 4 of 9 jobs done");
    /// let unread = Main::Runs { commit: "3f2a91c".into(), done: 0, jobs: 0 };
    /// assert_eq!(unread.line(false), "main: the full check of 3f2a91c runs");
    /// let broken = Main::Broken { commit: "3f2a91c".into(), failed: vec!["Clippy".into(), "Tests 1/4".into()] };
    /// assert_eq!(broken.line(false), "main is broken: the full check of 3f2a91c failed: Clippy, Tests 1/4");
    /// assert_eq!(broken.line(true), "main is broken: 3f2a91c failed");
    /// let ended = Main::Ended { commit: "3f2a91c".into(), conclusion: "cancelled".into() };
    /// assert_eq!(ended.line(false), "main: the full check of 3f2a91c ended: cancelled");
    /// ```
    pub fn line(&self, short: bool) -> String {
        let id = |commit: &str| crate::text::safe(&commit.chars().take(7).collect::<String>());
        match (self, short) {
            (Main::Runs { commit, done, jobs }, _) => {
                let head = match short {
                    true => format!("main: {} runs", id(commit)),
                    false => format!("main: the full check of {} runs", id(commit)),
                };
                match jobs {
                    0 => head,
                    _ => format!("{head} · {done} of {jobs} jobs done"),
                }
            }
            (Main::Broken { commit, .. }, true) => {
                format!("main is broken: {} failed", id(commit))
            }
            (Main::Broken { commit, failed }, false) => {
                let mut line = format!("main is broken: the full check of {} failed", id(commit));
                if !failed.is_empty() {
                    line.push_str(&format!(": {}", crate::text::safe(&failed.join(", "))));
                }
                line
            }
            (Main::Ended { commit, conclusion }, true) => {
                format!(
                    "main: {} ended: {}",
                    id(commit),
                    crate::text::safe(conclusion)
                )
            }
            (Main::Ended { commit, conclusion }, false) => format!(
                "main: the full check of {} ended: {}",
                id(commit),
                crate::text::safe(conclusion)
            ),
        }
    }

    /// True when `main` is broken.
    pub fn broken(&self) -> bool {
        matches!(self, Main::Broken { .. })
    }
}

/// The facts of the forge of one repository for its board.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Forge {
    /// The open issues: their titles and waves, the items in verify and the
    /// stuck items.
    pub issues: Issues,
    /// The milestone number of each wave with an open issue, by its title.
    pub milestones: BTreeMap<String, u64>,
    /// The number of open issues in the [`BACKLOG`].
    pub backlog: usize,
    /// The pull request of each item that has an open one, by the item.
    pub pulls: BTreeMap<u64, ItemPull>,
    /// The items that closed in the current wave, in number order. `None`
    /// when the read of them failed.
    pub closed: Option<Vec<Closed>>,
    /// The full check of `main`, when it needs a look: it runs, failed or
    /// ended with no success. `None` when it passed, has no run, or riff
    /// could not read it.
    pub main: Option<Main>,
    /// The open release issues: their number and title.
    pub releases: Vec<(u64, String)>,
}

/// The time in seconds since the epoch of `at`, for example
/// `2026-11-07T10:00:00Z`.
fn secs_of(at: &str) -> Option<i64> {
    Some(chrono::DateTime::parse_from_rfc3339(at).ok()?.timestamp())
}

impl Forge {
    /// The facts of the open entries `open` of the REST list of the
    /// issues, with the open pull requests `pulls` of `gh pr list`. The
    /// pull requests of the list are no items: [`Forge::of`] drops them,
    /// and keeps their last update for the age of their row. The issues
    /// are as `riff wave list` reads them ([`crate::top::issues_of`]): an
    /// issue that its comments or its merge keep out of the free work is
    /// in [`Issues::merged`] (01M7E25WB42HQQ34CRNXPZFHHR).
    ///
    /// ```
    /// use riff::rollout::Comment;
    /// use riff::tui::forge::{Entry, Forge};
    /// use riff_client::waves::Merged;
    ///
    /// let mut open: Vec<Entry> = serde_json::from_str(r#"[
    ///   {"number": 12, "title": "Show the board", "milestone": {"number": 5, "title": "Wave 3: Boards"}},
    ///   {"number": 13, "title": "Hold it", "milestone": {"number": 5, "title": "Wave 3: Boards"},
    ///    "labels": [{"name": "question"}]},
    ///   {"number": 14, "title": "Later", "milestone": {"number": 6, "title": "Wave 4"},
    ///    "body": "Needs: #15, #2"},
    ///   {"number": 15, "title": "Some day", "milestone": {"number": 1, "title": "Backlog"}},
    ///   {"number": 16, "title": "Release v4.11.0", "milestone": {"number": 5, "title": "Wave 3: Boards"},
    ///    "labels": [{"name": "release"}]},
    ///   {"number": 40, "title": "Show the board", "pull_request": {"url": "x"},
    ///    "updated_at": "2026-11-07T10:00:00Z"},
    ///   {"number": 17, "title": "Run it", "milestone": {"number": 5, "title": "Wave 3: Boards"},
    ///    "body": "Done when:\n- Live: it runs.", "comments": 1}
    /// ]"#)?;
    /// // The comment of the merge of #39, as `read` reads it.
    /// open[6].said = vec![Comment { body: "Merged in #39 (abc)".into() }];
    /// let pulls: Vec<riff::rollout::Pull> = serde_json::from_str(r#"[
    ///   {"number": 40, "headRefName": "worktree-issue-12", "isDraft": false, "statusCheckRollup": []}
    /// ]"#)?;
    /// let forge = Forge::of(open, &pulls);
    /// // The pull request is no item.
    /// assert!(!forge.issues.titles.contains_key(&40));
    /// assert_eq!(forge.issues.wave(), Some(&("Wave 3: Boards".into(), vec![12, 13, 16, 17])));
    /// assert_eq!(forge.milestones["Wave 3: Boards"], 5);
    /// assert_eq!(forge.issues.questions, [13].into());
    /// // #15 is open, and #2 is not.
    /// assert_eq!(forge.issues.needs, [(14, vec![15])].into());
    /// assert_eq!(forge.backlog, 1);
    /// assert_eq!(forge.releases, [(16, "Release v4.11.0".to_owned())]);
    /// assert_eq!(forge.issues.verify, [12].into());
    /// let pull = &forge.pulls[&12];
    /// assert_eq!((pull.number, pull.word.as_str()), (40, "verify"));
    /// assert_eq!(pull.updated, Some(1_794_045_600));
    /// // #17 is merged in #39, and waits for its check after the release.
    /// assert_eq!(forge.issues.merged, [(17, Merged { pull: 39, live: true })].into());
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    pub fn of(open: Vec<Entry>, pulls: &[Pull]) -> Forge {
        let (listed, entries): (Vec<Entry>, Vec<Entry>) =
            open.into_iter().partition(|e| e.pull_request.is_some());
        let updated: BTreeMap<u64, i64> = listed
            .iter()
            .filter_map(|p| Some((p.number, secs_of(p.updated_at.as_deref()?)?)))
            .collect();
        let mut forge = Forge::default();
        for entry in &entries {
            if let Some(milestone) = &entry.milestone {
                if wave_number(&milestone.title).is_some() {
                    forge
                        .milestones
                        .insert(milestone.title.clone(), milestone.number);
                }
                if milestone.title == BACKLOG {
                    forge.backlog += 1;
                }
            }
            if entry.labels.iter().any(|l| l.name == crate::cycle::LABEL) {
                forge.releases.push((entry.number, entry.title.clone()));
            }
        }
        forge.releases.sort_unstable();
        let issues = entries.into_iter().map(Entry::open).collect();
        forge.issues = crate::top::with_pulls(crate::top::issues_of(issues), pulls);
        for pull in pulls {
            let Some(item) = pull.issue() else {
                continue;
            };
            let word = match (pull.stuck(false), pull.verify()) {
                (Some(stuck), _) => stuck.word(),
                (None, Some(Verify::Asked)) => "verify",
                (None, Some(Verify::Passed)) => "merge",
                (None, Some(Verify::Failed)) => "failed",
                (None, None) => "build",
            };
            forge.pulls.insert(
                item,
                ItemPull {
                    number: pull.number,
                    word: word.to_owned(),
                    updated: updated.get(&pull.number).copied(),
                },
            );
        }
        forge
    }

    /// The number of the milestone of the current wave, if any.
    pub fn wave_milestone(&self) -> Option<u64> {
        let (title, _) = self.issues.wave()?;
        self.milestones.get(title).copied()
    }
}

/// The closed items in the entries `entries` of the REST list of the
/// closed issues of a milestone, in number order: the pull requests are
/// no items.
///
/// ```
/// use riff::tui::forge::{Closed, Entry, closed_of};
///
/// let entries: Vec<Entry> = serde_json::from_str(r#"[
///   {"number": 13, "title": "Not now", "state_reason": "not_planned"},
///   {"number": 12, "title": "Done", "state_reason": "completed"},
///   {"number": 40, "title": "Done", "pull_request": {}}
/// ]"#)?;
/// let closed = closed_of(entries);
/// assert_eq!(closed, [
///     Closed { number: 12, title: "Done".into(), done: true },
///     Closed { number: 13, title: "Not now".into(), done: false },
/// ]);
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn closed_of(entries: Vec<Entry>) -> Vec<Closed> {
    let mut closed: Vec<Closed> = entries
        .into_iter()
        .filter(|e| e.pull_request.is_none())
        .map(|e| Closed {
            done: e.state_reason.as_deref() == Some("completed"),
            number: e.number,
            title: e.title,
        })
        .collect();
    closed.sort_unstable_by_key(|c| c.number);
    closed
}

/// The `ETag` and the body of each conditional read of the TUI, by its
/// path, from one read to the next, and the last merged pull requests of
/// each repository ([`Kept::merges`]). A clone shares them: the reads of
/// the Waves tab, the Sessions tab, the Chat tab and the Decisions tab
/// keep one set.
#[derive(Debug, Clone, Default)]
pub struct Kept {
    replies: Arc<Mutex<BTreeMap<String, (String, String)>>>,
    /// The last merged pull requests of each repository.
    merged: Arc<Mutex<BTreeMap<String, MergedRead>>>,
}

/// The last merged pull requests of a repository, with the `ETag` of each
/// page of its open issues at their read ([`Kept::merges`]).
type MergedRead = (Vec<String>, Vec<MergedPull>);

impl Kept {
    /// The body of the REST path `path` now ([`Gh::ask_for`]): for a
    /// `304`, the body that this keeps, for no point. A new reply with an
    /// `ETag` is kept for the next read, also for each clone.
    ///
    /// ```
    /// use riff::pr::Gh;
    /// use riff::tui::forge::Kept;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let script = r#"#!/bin/sh
    /// echo "$*" >> "$(dirname "$0")/calls"
    /// case "$*" in
    ///   *'If-None-Match: "v1"'*) echo 'gh: HTTP 304' >&2; exit 1 ;;
    ///   *) printf 'HTTP/2.0 200 OK\r\nEtag: "v1"\r\n\r\n[1]\n' ;;
    /// esac
    /// "#;
    /// let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script));
    /// let kept = Kept::default();
    /// assert_eq!(kept.body(&gh, "repos/o/r/issues")?, "[1]\n");
    /// // The second read, of a clone, is a `304`: the same body.
    /// assert_eq!(kept.clone().body(&gh, "repos/o/r/issues")?, "[1]\n");
    /// let calls = std::fs::read_to_string(dir.path().join("calls"))?;
    /// assert!(calls.lines().nth(1).unwrap().contains(r#"If-None-Match: "v1""#), "{calls}");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn body(&self, gh: &Gh, path: &str) -> Result<String> {
        Ok(self.tagged(gh, path)?.0)
    }

    /// [`Kept::body`], and the `ETag` of the reply: `None` for a reply
    /// with none.
    fn tagged(&self, gh: &Gh, path: &str) -> Result<(String, Option<String>)> {
        let last = self.replies().get(path).cloned();
        match gh.ask_for(path, last.as_ref().map(|(etag, _)| etag.as_str()))? {
            Reply::Same => match last {
                Some((etag, body)) => Ok((body, Some(etag))),
                None => bail!("gh api {path}: GitHub answered 304 to a read with no ETag"),
            },
            Reply::New { etag, body } => {
                match &etag {
                    Some(etag) => {
                        self.replies()
                            .insert(path.to_owned(), (etag.clone(), body.clone()));
                    }
                    None => {
                        self.replies().remove(path);
                    }
                }
                Ok((body, etag))
            }
        }
    }

    /// The replies that this keeps. The lock holds for no call of `gh`.
    fn replies(&self) -> MutexGuard<'_, BTreeMap<String, (String, String)>> {
        self.replies.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The entries of the REST list at `path` now, and the `ETag` of the
    /// reply.
    fn entries(&self, gh: &Gh, path: &str) -> Result<(Vec<Entry>, Option<String>)> {
        let (body, etag) = self.tagged(gh, path)?;
        let entries = serde_json::from_str(&body).with_context(|| format!("gh api {path}"))?;
        Ok((entries, etag))
    }

    /// The comments of the issue `issue` of `repo` now, with the
    /// conditional read of [`comments_path`].
    fn comments(&self, gh: &Gh, repo: &str, issue: u64) -> Result<Vec<Comment>> {
        let path = comments_path(repo, issue);
        let body = self.body(gh, &path)?;
        serde_json::from_str(&body).with_context(|| format!("gh api {path}"))
    }

    /// The merges of the last 100 merged pull requests of `repo`, by
    /// issue, as the free work counts them ([`crate::rollout::merges`]).
    /// `open` is the `ETag` of each page of the open issues of `repo`
    /// now, `None` when a page has none. A merge takes its pull request out
    /// of the open issues, so it gives a page a new `ETag`. While `open` is
    /// as at the last read of the merged pull requests, this gives the
    /// merges of that read, and calls no `gh`. Else it reads them again
    /// with `gh pr list` ([`crate::rollout::merged_pulls`]), and keeps
    /// them with `open`. A read that fails keeps nothing, and gives the
    /// merges of the last read, or none (01M7E25WB42HQQ34CRNXPZFHHR).
    ///
    /// ```
    /// use riff::pr::Gh;
    /// use riff::tui::forge::Kept;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let script = r#"#!/bin/sh
    /// echo "$*" >> "$(dirname "$0")/calls"
    /// printf '%s\n' '[{"number":882,"state":"MERGED","headRefName":"worktree-issue-872","body":"Refs #872\n","mergeCommit":{"oid":"abc"}}]'
    /// "#;
    /// let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script));
    /// let calls = || std::fs::read_to_string(dir.path().join("calls")).unwrap().lines().count();
    /// let kept = Kept::default();
    /// let v1 = vec![r#""v1""#.to_owned()];
    /// assert_eq!(kept.merges(&gh, "o/r", Some(&v1))[&872].pull, 882);
    /// assert_eq!(calls(), 1);
    /// // The open issues did not change: no call.
    /// assert_eq!(kept.merges(&gh, "o/r", Some(&v1))[&872].pull, 882);
    /// assert_eq!(calls(), 1);
    /// // They changed, or a page has no ETag: a read.
    /// kept.merges(&gh, "o/r", Some(&[r#""v2""#.to_owned()]));
    /// kept.merges(&gh, "o/r", None);
    /// assert_eq!(calls(), 3);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn merges(&self, gh: &Gh, repo: &str, open: Option<&[String]>) -> HashMap<u64, Merge> {
        let last = self.merged().get(repo).cloned();
        let pulls = match (last, open) {
            (Some((tags, pulls)), Some(open)) if tags == open => pulls,
            (last, _) => match crate::rollout::merged_pulls(gh, repo) {
                Ok(pulls) => {
                    if let Some(open) = open {
                        self.merged()
                            .insert(repo.to_owned(), (open.to_vec(), pulls.clone()));
                    }
                    pulls
                }
                Err(_) => last.map(|(_, pulls)| pulls).unwrap_or_default(),
            },
        };
        crate::rollout::merges_of(&pulls, crate::daemon::now_ms())
    }

    /// The merged pull requests that this keeps. The lock holds for no
    /// call of `gh`.
    fn merged(&self) -> MutexGuard<'_, BTreeMap<String, MergedRead>> {
        self.merged.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The open entries of `repo`: the issues and the pull requests of each
/// page of the REST list of the open issues, with the `ETag` that `kept`
/// keeps, up to [`MOST_PAGES`]. With them, the `ETag` of each page:
/// `None` when a page has none.
fn open(gh: &Gh, repo: &str, kept: &Kept) -> Result<(Vec<Entry>, Option<Vec<String>>)> {
    let mut open = Vec::new();
    let mut tags = Some(Vec::new());
    for page in 1..=MOST_PAGES {
        let (entries, etag) = kept.entries(gh, &open_path(repo, page))?;
        tags = tags.zip(etag).map(|(mut tags, etag)| {
            tags.push(etag);
            tags
        });
        let full = entries.len() >= PER_PAGE;
        open.extend(entries);
        if !full {
            break;
        }
    }
    Ok((open, tags))
}

/// The open issues of `repo` for the Sessions tab (#1707,
/// 01M6VGHCDTDJ69B7C02B5K61PW, 01M6VT7TXBW8EJKY0FTYQ0H49M): their
/// titles, their waves, the trackers and the questions. It makes the
/// conditional read of the open issues of [`read`], and no read of the
/// pull requests, so a read that changed nothing costs no point. It
/// fails with the error of `gh`.
///
/// ```
/// use riff::pr::Gh;
/// use riff::tui::forge::{Kept, titles};
///
/// let dir = tempfile::tempdir()?;
/// let script = r#"#!/bin/sh
/// echo "$*" >> "$(dirname "$0")/calls"
/// case "$*" in
///   *'If-None-Match: "v1"'*) echo 'gh: HTTP 304' >&2; exit 1 ;;
///   *'issues?state=open'*)
///     printf 'HTTP/2.0 200 OK\r\nEtag: "v1"\r\n\r\n[{"number":12,"title":"Show it","milestone":{"number":5,"title":"Wave 3"}},
///       {"number":40,"title":"Show it","pull_request":{}}]' ;;
///   *) echo 'gh: Not Found (HTTP 404)' >&2; exit 1 ;;
/// esac
/// "#;
/// let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script));
/// let kept = Kept::default();
/// let issues = titles(&gh, "o/r", &kept)?;
/// assert_eq!(issues.titles.keys().collect::<Vec<_>>(), [&12]);
/// assert_eq!(issues.wave(), Some(&("Wave 3".into(), vec![12])));
/// // The second read is a `304`: the same titles.
/// assert_eq!(titles(&gh, "o/r", &kept)?, issues);
/// let calls = std::fs::read_to_string(dir.path().join("calls"))?;
/// assert_eq!(calls.lines().count(), 2, "{calls}");
/// assert!(calls.lines().nth(1).unwrap().contains(r#"If-None-Match: "v1""#), "{calls}");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn titles(gh: &Gh, repo: &str, kept: &Kept) -> Result<Issues> {
    let (open, _) = open(gh, repo, kept)?;
    let issues = open
        .into_iter()
        .filter(|e| e.pull_request.is_none())
        .map(Entry::open)
        .collect();
    Ok(crate::top::issues_of(issues))
}

/// The open issues of `repo` for the list of `#` in `riff chat` and in
/// the Chat tab ([`crate::typeahead`]): the number, the title and the
/// time of each. It makes the conditional read of the open issues of
/// [`read`], and no other call, so a read that changed nothing costs no
/// point. It fails with the error of `gh`.
///
/// ```
/// use riff::pr::Gh;
/// use riff::tui::forge::{Kept, open_issues};
///
/// let dir = tempfile::tempdir()?;
/// let script = r#"#!/bin/sh
/// echo "$*" >> "$(dirname "$0")/calls"
/// case "$*" in
///   *'If-None-Match: "v1"'*) echo 'gh: HTTP 304' >&2; exit 1 ;;
///   *'issues?state=open'*)
///     printf 'HTTP/2.0 200 OK\r\nEtag: "v1"\r\n\r\n[{"number":12,"title":"Show it","created_at":"2026-11-07T10:00:00Z"},
///       {"number":40,"title":"Show it","pull_request":{}}]' ;;
///   *) echo 'gh: Not Found (HTTP 404)' >&2; exit 1 ;;
/// esac
/// "#;
/// let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script));
/// let kept = Kept::default();
/// let issues = open_issues(&gh, "o/r", &kept)?;
/// // The pull request is no issue.
/// assert_eq!(issues.len(), 1);
/// let issue = &issues[0];
/// assert_eq!((issue.repo.as_str(), issue.number, issue.title.as_str()), ("o/r", 12, "Show it"));
/// assert_eq!(issue.opened, Some(1_794_045_600));
/// // The second read is a `304`: the same issues.
/// assert_eq!(open_issues(&gh, "o/r", &kept)?, issues);
/// let calls = std::fs::read_to_string(dir.path().join("calls"))?;
/// assert_eq!(calls.lines().count(), 2, "{calls}");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn open_issues(gh: &Gh, repo: &str, kept: &Kept) -> Result<Vec<OpenIssue>> {
    let (open, _) = open(gh, repo, kept)?;
    Ok(open
        .into_iter()
        .filter(|e| e.pull_request.is_none())
        .map(|e| OpenIssue {
            opened: e.created_at.as_deref().and_then(secs_of),
            repo: repo.to_owned(),
            number: e.number,
            title: e.title,
        })
        .collect())
}

/// The decisions of `repo` for the Decisions tab (#2188,
/// 01M7FYVHB1APFXPXA71PMD18EH): each open issue with the label
/// `question` or `design`, with its wave, its labels and the names of
/// its questions ([`Decision::of`]). It makes the conditional read of the
/// open issues of [`read`], and no other call, so a read that changed
/// nothing costs no point. It fails with the error of `gh`.
///
/// ```
/// use riff::pr::Gh;
/// use riff::tui::forge::{Kept, decisions};
///
/// let dir = tempfile::tempdir()?;
/// let script = r###"#!/bin/sh
/// echo "$*" >> "$(dirname "$0")/calls"
/// case "$*" in
///   *'If-None-Match: "v1"'*) echo 'gh: HTTP 304' >&2; exit 1 ;;
///   *'issues?state=open'*)
///     printf 'HTTP/2.0 200 OK\r\nEtag: "v1"\r\n\r\n[
///       {"number":12,"title":"Which name?","labels":[{"name":"question"}],
///        "milestone":{"number":1,"title":"Backlog"},"body":"## Questions for mike"},
///       {"number":13,"title":"Design: the board","labels":[{"name":"design"}],
///        "milestone":{"number":5,"title":"Wave 3"}},
///       {"number":14,"title":"Fix it","milestone":{"number":5,"title":"Wave 3"}},
///       {"number":40,"title":"Which name?","labels":[{"name":"question"}],"pull_request":{}}]' ;;
///   *) echo 'gh: Not Found (HTTP 404)' >&2; exit 1 ;;
/// esac
/// "###;
/// let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script));
/// let kept = Kept::default();
/// let repo = "o/r".parse()?;
/// let found = decisions(&gh, &repo, &kept)?;
/// let numbers: Vec<u64> = found.iter().map(|d| d.issue().number).collect();
/// assert_eq!(numbers, [12, 13]);
/// assert_eq!(found[0].named(), ["mike"]);
/// // The second read is a `304`: the same decisions, for no point.
/// assert_eq!(decisions(&gh, &repo, &kept)?, found);
/// let calls = std::fs::read_to_string(dir.path().join("calls"))?;
/// assert_eq!(calls.lines().count(), 2, "{calls}");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn decisions(gh: &Gh, repo: &ThreadName, kept: &Kept) -> Result<Vec<Decision>> {
    let (open, _) = open(gh, &repo.to_string(), kept)?;
    Ok(open
        .into_iter()
        .filter_map(|entry| Decision::of(repo, entry))
        .collect())
}

/// The issues of the current wave in the open entries `open`: the
/// `Wave N` milestone with the lowest N, as [`Issues::wave`] finds it.
fn current_wave(open: &[Entry]) -> BTreeSet<u64> {
    let wave = |e: &Entry| wave_number(&e.milestone.as_ref()?.title);
    let issues = || open.iter().filter(|e| e.pull_request.is_none());
    let Some(first) = issues().filter_map(wave).min() else {
        return BTreeSet::new();
    };
    issues()
        .filter(|e| wave(e) == Some(first))
        .map(|e| e.number)
        .collect()
}

/// The facts of the forge of `repo` (`OWNER/REPO`) for its board, with
/// `gh` and the replies that `kept` keeps. It fails with the error of
/// `gh` when the open issues cannot be read. The other reads give no
/// facts when they fail: the board counts only the claims and the holds,
/// as in `riff top`. Each issue gets the merge of its merged pull request
/// ([`Kept::merges`]), and each issue of the current wave with a comment
/// gets its comments ([`comments_path`]), as `riff wave list` reads them
/// (01M7E25WB42HQQ34CRNXPZFHHR). A read of the comments that fails gives
/// none.
///
/// ```
/// use riff::pr::Gh;
/// use riff::tui::forge::{Kept, read};
///
/// let dir = tempfile::tempdir()?;
/// let script = r#"#!/bin/sh
/// echo "$*" >> "$(dirname "$0")/calls"
/// case "$*" in
///   *'If-None-Match'*) echo 'gh: HTTP 304' >&2; exit 1 ;;
///   *'issues?state=open'*)
///     printf 'HTTP/2.0 200 OK\r\nEtag: "v1"\r\n\r\n[{"number":12,"title":"Show it","milestone":{"number":5,"title":"Wave 3"}}]' ;;
///   *'issues?state=closed'*)
///     printf 'HTTP/2.0 200 OK\r\nEtag: "v2"\r\n\r\n[{"number":11,"title":"Old","state_reason":"completed"}]' ;;
///   'pr list --repo o/r --state merged '*) echo '[]' ;;
///   *) echo 'gh: Not Found (HTTP 404)' >&2; exit 1 ;;
/// esac
/// "#;
/// let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script));
/// let kept = Kept::default();
/// let forge = read(&gh, "o/r", &kept)?;
/// assert_eq!(forge.issues.titles[&12], "Show it");
/// assert_eq!(forge.closed.as_ref().unwrap()[0].number, 11);
/// // No open pull request: no `gh pr list` of the open ones. One of the
/// // merged ones.
/// let pr_lists = || {
///     let calls = std::fs::read_to_string(dir.path().join("calls")).unwrap();
///     calls.lines().filter(|c| c.starts_with("pr list")).map(String::from).collect::<Vec<_>>()
/// };
/// let merged = format!("pr list --repo o/r --state merged --limit 100 --json {}", riff::rollout::MERGED_FIELDS);
/// assert_eq!(pr_lists(), [merged]);
/// // A read that changed nothing gives the same facts, with no `gh pr list`.
/// assert_eq!(read(&gh, "o/r", &kept)?, forge);
/// assert_eq!(pr_lists().len(), 1);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read(gh: &Gh, repo: &str, kept: &Kept) -> Result<Forge> {
    let (mut open, tags) = open(gh, repo, kept)?;
    let pulls = match open.iter().any(|e| e.pull_request.is_some()) {
        true => crate::rollout::pulls(gh, repo).unwrap_or_default(),
        false => Vec::new(),
    };
    let mut merges = kept.merges(gh, repo, tags.as_deref());
    let wave = current_wave(&open);
    for entry in open.iter_mut().filter(|e| e.pull_request.is_none()) {
        entry.merge = merges.remove(&entry.number);
        if entry.comments > 0 && wave.contains(&entry.number) {
            entry.said = kept.comments(gh, repo, entry.number).unwrap_or_default();
        }
    }
    let mut forge = Forge::of(open, &pulls);
    forge.closed = match forge.wave_milestone() {
        Some(milestone) => kept
            .entries(gh, &closed_path(repo, milestone))
            .ok()
            .map(|(entries, _)| closed_of(entries)),
        None => Some(Vec::new()),
    };
    forge.main = read_main(gh, repo).ok().flatten();
    Ok(forge)
}

/// The full check of the head of the default branch of `repo`, when it
/// needs a look ([`Forge::main`]).
fn read_main(gh: &Gh, repo: &str) -> Result<Option<Main>> {
    #[derive(Deserialize)]
    struct Repository {
        #[serde(deserialize_with = "crate::text::forge_de")]
        default_branch: String,
    }
    let path = format!("repos/{repo}");
    // The default branch changes rarely: a read in Gh::SLOW, shared with
    // the read of the rules.
    let reply = gh.read_within(&["api", &path], Gh::SLOW)?;
    let branch: Repository =
        serde_json::from_str(&reply).with_context(|| format!("gh api {path}"))?;
    let Some(gate) = crate::look::read_main(gh, repo, &branch.default_branch)? else {
        return Ok(None);
    };
    let commit = gate.commit.clone();
    let run = gate.run.as_deref();
    Ok(match &gate.gate {
        Gate::Passed | Gate::Missing => None,
        Gate::Running => {
            let jobs = run
                .and_then(|run| crate::look::read_jobs(gh, repo, run).ok())
                .unwrap_or_default();
            Some(Main::Runs {
                commit,
                done: jobs.iter().filter(|j| j.conclusion.is_some()).count(),
                jobs: jobs.len(),
            })
        }
        Gate::Ended(_) if gate.broken() => Some(Main::Broken {
            commit,
            failed: run
                .and_then(|run| crate::look::read_failed_jobs(gh, repo, run).ok())
                .unwrap_or_default(),
        }),
        Gate::Ended(conclusion) => Some(Main::Ended {
            commit,
            conclusion: conclusion.clone(),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake `gh` in `dir` that answers the REST reads of o/r from the
    /// files of `dir`: `open` (with its `ETag` in `open.etag`), `closed`,
    /// `pulls`, `merged`, `runs` and `jobs`, and the comments of each
    /// issue N from `comments-N` (with its `ETag` in `comments-N.etag`, or
    /// `[]`). It logs each call to `calls`.
    fn fake(dir: &std::path::Path) -> Gh {
        let script = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/calls"
tag=$(cat "$dir/open.etag" 2>/dev/null)
case "$*" in
  *"If-None-Match: $tag"*'state=open'*) echo 'gh: HTTP 304' >&2; exit 1 ;;
  *'issues?state=open'*) printf 'HTTP/2.0 200 OK\r\nEtag: %s\r\n\r\n' "$tag"; cat "$dir/open" ;;
  *'issues?state=closed'*) printf 'HTTP/2.0 200 OK\r\n\r\n'; cat "$dir/closed" ;;
  *'/comments?'*)
    all="$*"; n=${all##*issues/}; n=${n%%/*}
    said=$(cat "$dir/comments-$n.etag" 2>/dev/null)
    if [ -n "$said" ]; then
      case "$all" in
        *"If-None-Match: $said "*) echo 'gh: HTTP 304' >&2; exit 1 ;;
      esac
      printf 'HTTP/2.0 200 OK\r\nEtag: %s\r\n\r\n' "$said"
    else
      printf 'HTTP/2.0 200 OK\r\n\r\n'
    fi
    cat "$dir/comments-$n" 2>/dev/null || echo '[]' ;;
  'pr list --repo o/r --state merged '*) cat "$dir/merged" ;;
  'pr list'*) cat "$dir/pulls" ;;
  'api repos/o/r') echo '{"default_branch":"main"}' ;;
  'api repos/o/r/rules/branches/main'*) echo '[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"Gate"}]}}]' ;;
  'api repos/o/r/commits/main/check-runs'*) cat "$dir/runs" ;;
  'api repos/o/r/actions/runs/7/jobs'*) cat "$dir/jobs" ;;
  *) echo '[]' ;;
esac
"#;
        std::fs::write(dir.join("open.etag"), "\"v1\"").unwrap();
        std::fs::write(dir.join("closed"), "[]").unwrap();
        std::fs::write(dir.join("pulls"), "[]").unwrap();
        std::fs::write(dir.join("merged"), "[]").unwrap();
        std::fs::write(dir.join("runs"), r#"{"check_runs":[]}"#).unwrap();
        std::fs::write(dir.join("jobs"), "[]").unwrap();
        Gh::at(isolated::write_program(dir.join("gh"), script))
    }

    fn calls(dir: &std::path::Path) -> String {
        std::fs::read_to_string(dir.join("calls")).unwrap_or_default()
    }

    /// The start of the call of `gh pr list` for the open pull requests.
    const OPEN_PULLS: &str = "pr list --repo o/r --state open";

    /// The calls of `gh pr list` for the merged pull requests.
    fn merged_reads(dir: &std::path::Path) -> usize {
        let merged = crate::rollout::merged_args("o/r").join(" ");
        calls(dir).lines().filter(|c| *c == merged).count()
    }

    /// The list of issues has the pull requests too: the read drops them
    /// from the items, and calls `gh pr list` only when one is open. A
    /// read that changed nothing asks with the `ETag` and gets a `304`
    /// (#1638).
    #[test]
    fn the_read_drops_the_pull_requests_and_reads_them_only_when_one_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake(dir.path());
        std::fs::write(
            dir.path().join("open"),
            r#"[{"number":12,"title":"Show it","milestone":{"number":5,"title":"Wave 3"}}]"#,
        )
        .unwrap();
        let kept = Kept::default();
        let first = read(&gh, "o/r", &kept).unwrap();
        assert_eq!(first.issues.wave(), Some(&("Wave 3".into(), vec![12])));
        assert!(!calls(dir.path()).contains(OPEN_PULLS));
        let again = read(&gh, "o/r", &kept).unwrap();
        assert_eq!(again, first);
        assert!(
            calls(dir.path()).contains(r#"If-None-Match: "v1""#),
            "{}",
            calls(dir.path())
        );

        // A pull request opens: a new ETag, and `gh pr list`.
        std::fs::write(dir.path().join("open.etag"), "\"v2\"").unwrap();
        std::fs::write(
            dir.path().join("open"),
            r#"[{"number":12,"title":"Show it","milestone":{"number":5,"title":"Wave 3"}},
                {"number":40,"title":"Show it","pull_request":{},"updated_at":"2026-11-07T10:00:00Z"}]"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("pulls"),
            r#"[{"number":40,"headRefName":"worktree-issue-12","isDraft":false,"statusCheckRollup":[]}]"#,
        )
        .unwrap();
        let read = read(&gh, "o/r", &kept).unwrap();
        assert!(calls(dir.path()).contains(OPEN_PULLS));
        assert_eq!(read.issues.titles.keys().collect::<Vec<_>>(), [&12]);
        assert_eq!(read.issues.verify, [12].into());
        assert_eq!(read.pulls[&12].number, 40);
    }

    /// 01M7E25WB42HQQ34CRNXPZFHHR (#2145): the read gives the board the
    /// facts of `riff wave list`. 872 has the comment `Merged in #882`, and
    /// the pull request #1866 of 1854 merged with `Refs #1854`. The "Done
    /// when" of 1854 has a `Live:` line. The read reads the comments of
    /// 872 only: 1854 and 873 have none, and 900 is in a later wave. A read
    /// that changed nothing reads no merged pull request again, and gets a
    /// `304` for the comments. After the comment of `riff wave next 872`,
    /// 872 is merged no more. A read of the merged pull requests that fails
    /// keeps the last ones.
    #[test]
    fn the_read_gives_the_merged_items_of_riff_wave_list() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake(dir.path());
        let wave = |n: u64| format!(r#"{{"number":{n},"title":"Wave {n}"}}"#);
        let open = |said: u64| {
            format!(
                r#"[{{"number":872,"title":"Two parts","milestone":{},"body":"Done when:\n- A test.","comments":{said}}},
                    {{"number":1854,"title":"Run it","milestone":{},"body":"Done when:\n- Live: it runs."}},
                    {{"number":873,"title":"Show it","milestone":{},"body":"Done when:\n- A test."}},
                    {{"number":900,"title":"Later","milestone":{},"comments":2}}]"#,
                wave(3),
                wave(3),
                wave(3),
                wave(4)
            )
        };
        let said = |bodies: &[&str]| {
            let comments: Vec<serde_json::Value> = bodies
                .iter()
                .map(|b| serde_json::json!({ "body": b, "user": { "login": "m" } }))
                .collect();
            serde_json::to_string(&comments).unwrap()
        };
        let write = |name: &str, text: &str| std::fs::write(dir.path().join(name), text).unwrap();
        write("open", &open(1));
        write("comments-872", &said(&["Merged in #882 (abc)"]));
        write("comments-872.etag", r#""c1""#);
        write(
            "merged",
            r#"[{"number":1866,"state":"MERGED","headRefName":"worktree-issue-1854",
                "body":"Refs #1854\n","mergeCommit":{"oid":"def"}}]"#,
        );
        let merged = |forge: &Forge| -> Vec<(u64, u64, bool)> {
            let merged = forge.issues.merged.iter();
            merged.map(|(n, m)| (*n, m.pull, m.live)).collect()
        };
        let comments = || -> Vec<String> {
            let log = calls(dir.path());
            let reads = log.lines().filter(|c| c.contains("/comments?"));
            reads.map(String::from).collect()
        };
        let kept = Kept::default();
        let first = read(&gh, "o/r", &kept).unwrap();
        assert_eq!(merged(&first), [(872, 882, false), (1854, 1866, true)]);
        assert_eq!(
            comments(),
            ["api -i repos/o/r/issues/872/comments?per_page=100"]
        );
        assert_eq!(merged_reads(dir.path()), 1);

        // Nothing changed: no read of the merged pull requests, and a
        // `304` for the comments.
        assert_eq!(read(&gh, "o/r", &kept).unwrap(), first);
        assert_eq!(merged_reads(dir.path()), 1);
        assert_eq!(
            comments()[1],
            r#"api -i -H If-None-Match: "c1" repos/o/r/issues/872/comments?per_page=100"#
        );

        // `riff wave next 872` adds its comment: 872 and its comments
        // change.
        write("open", &open(2));
        write("open.etag", r#""v2""#);
        let more = crate::text::wave_more_work(882);
        write("comments-872", &said(&["Merged in #882 (abc)", &more]));
        write("comments-872.etag", r#""c2""#);
        let next = read(&gh, "o/r", &kept).unwrap();
        assert_eq!(merged(&next), [(1854, 1866, true)]);
        assert_eq!(merged_reads(dir.path()), 2, "the open issues changed");

        // The read of the merged pull requests fails: the last ones stay.
        write("merged", "not json");
        write("open.etag", r#""v3""#);
        let failed = read(&gh, "o/r", &kept).unwrap();
        assert_eq!(merged(&failed), [(1854, 1866, true)]);
        assert_eq!(merged_reads(dir.path()), 3);
    }

    /// The Sessions tab reads the titles with the conditional read of the
    /// open issues, and no `gh issue list` and no `gh pr list`. A second
    /// read with no change gets a `304`, for no point, also when the
    /// read of the Waves tab comes next: the two share the kept replies
    /// (#1707, 01M6VGHCDTDJ69B7C02B5K61PW, 01M6VT7TXBW8EJKY0FTYQ0H49M).
    #[test]
    fn the_titles_come_from_the_conditional_read_and_a_second_read_costs_no_point() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake(dir.path());
        std::fs::write(
            dir.path().join("open"),
            r#"[{"number":12,"title":"Show it","milestone":{"number":5,"title":"Wave 3"}},
                {"number":40,"title":"Show it","pull_request":{},"updated_at":"2026-11-07T10:00:00Z"}]"#,
        )
        .unwrap();
        let kept = Kept::default();
        let first = titles(&gh, "o/r", &kept).unwrap();
        assert_eq!(first.titles.keys().collect::<Vec<_>>(), [&12]);
        assert_eq!(first.titles[&12], "Show it");
        assert_eq!(titles(&gh, "o/r", &kept.clone()).unwrap(), first);
        let log = calls(dir.path());
        assert_eq!(
            log.lines().collect::<Vec<_>>(),
            [
                "api -i repos/o/r/issues?state=open&per_page=100&page=1",
                r#"api -i -H If-None-Match: "v1" repos/o/r/issues?state=open&per_page=100&page=1"#,
            ]
        );
        // The Waves tab reads the same list next: a `304` too.
        let forge = read(&gh, "o/r", &kept).unwrap();
        assert_eq!(forge.issues.titles, first.titles);
        let log = calls(dir.path());
        let open: Vec<&str> = log.lines().filter(|l| l.contains("state=open")).collect();
        assert_eq!(open.len(), 3, "{log}");
        assert!(open[2].contains(r#"If-None-Match: "v1""#), "{log}");
    }

    /// A full page of open issues asks for the next page.
    #[test]
    fn a_full_page_reads_the_next_page() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake(dir.path());
        let page: Vec<String> = (1..=PER_PAGE)
            .map(|n| format!(r#"{{"number":{n},"title":"t"}}"#))
            .collect();
        std::fs::write(dir.path().join("open"), format!("[{}]", page.join(","))).unwrap();
        let kept = Kept::default();
        // The fake gives the same full page each time: the read stops at
        // the most pages.
        read(&gh, "o/r", &kept).unwrap();
        let pages = calls(dir.path())
            .lines()
            .filter(|l| l.contains("state=open"))
            .count();
        assert_eq!(pages, usize::try_from(MOST_PAGES).unwrap());
    }

    /// A read of the open issues that fails is an error with the words of
    /// `gh`.
    #[test]
    fn a_read_that_fails_names_the_error_of_gh() {
        let dir = tempfile::tempdir().unwrap();
        let gh = Gh::at(isolated::write_program(
            dir.path().join("gh"),
            "#!/bin/sh\necho 'gh: API rate limit exceeded (HTTP 403)' >&2\nexit 1\n",
        ));
        let read = each(&["o/r"], |repo| super::read(&gh, repo, &Kept::default()));
        let error = read[0].1.as_ref().unwrap_err();
        assert!(error.contains("API rate limit exceeded"), "{error}");
    }

    /// The full check of `main`: a run that goes counts its jobs, and a
    /// run that passed needs no look (01M6VFGJZF7XGNGVWTECKAH429).
    #[test]
    fn the_full_check_of_main_shows_only_when_it_runs_or_failed() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake(dir.path());
        std::fs::write(dir.path().join("open"), "[]").unwrap();
        let run = |status: &str, conclusion: &str| {
            format!(
                r#"{{"check_runs":[{{"status":"{status}","conclusion":{conclusion},"head_sha":"3f2a91c0ffee",
                "html_url":"https://github.com/o/r/actions/runs/7/job/1"}}]}}"#
            )
        };
        std::fs::write(dir.path().join("runs"), run("in_progress", "null")).unwrap();
        std::fs::write(
            dir.path().join("jobs"),
            r#"[{"name":"Lint","conclusion":"success"},{"name":"Test","conclusion":null},{"name":"Gate","conclusion":null}]"#,
        )
        .unwrap();
        let kept = Kept::default();
        let forge = read(&gh, "o/r", &kept).unwrap();
        assert_eq!(
            forge.main,
            Some(Main::Runs {
                commit: "3f2a91c0ffee".into(),
                done: 1,
                jobs: 3
            })
        );
        std::fs::write(dir.path().join("runs"), run("completed", "\"failure\"")).unwrap();
        std::fs::write(
            dir.path().join("jobs"),
            r#"[{"name":"Lint","conclusion":"failure"},{"name":"Gate","conclusion":"failure"}]"#,
        )
        .unwrap();
        let forge = read(&gh, "o/r", &kept).unwrap();
        assert_eq!(
            forge.main,
            Some(Main::Broken {
                commit: "3f2a91c0ffee".into(),
                failed: vec!["Lint".into()]
            })
        );
        std::fs::write(dir.path().join("runs"), run("completed", "\"success\"")).unwrap();
        assert_eq!(read(&gh, "o/r", &kept).unwrap().main, None);
    }
}
