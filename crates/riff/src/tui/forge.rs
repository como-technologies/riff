//! The reads of the forge for the Waves tab and the Sessions tab of the
//! TUI (#1638, #1707, 01M6VFGK4P2FWMFQHBZKR3JXSZ,
//! 01M6VT7TXBW8EJKY0FTYQ0H49M).
//!
//! # Design
//!
//! The Waves tab reads `gh` each [`READ_EVERY`] while the person sees
//! it, and at once on `r`. A read of a repository is one run of blocking
//! calls ([`read`]). Its calls go to the tally of the daemon as the
//! caller `tui` ([`crate::pr::Gh::here`]). The calls:
//!
//! | What | Call | Cost |
//! |---|---|---|
//! | The open issues, their titles, waves, labels and `Needs:` lines | a conditional REST read of each page of `repos/R/issues?state=open` ([`open_path`]) | A `304` costs no point. A new page costs one REST call |
//! | The pull requests that wait or are stuck | `gh pr list` with the checks ([`crate::rollout::pulls`]), only when the list of issues has an open pull request, and the ruleset of `main` when a head has a failed check ([`crate::rollout::Pull::required`]) | About 2 GraphQL points. The ruleset is two REST calls, one time in 10 minutes |
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
//! [`Kept`] keeps the `ETag` and the body of each conditional read, so a
//! `304` gives the body of the last reply again. A read that fails keeps
//! the last facts of its repository ([`riff_client::waves::Issues::newest`]).
//!
//! The Sessions tab reads the titles of the open issues with the same
//! conditional read, and with no other call ([`titles`]). The two tabs
//! share one [`Kept`], so a read of one tab after a read of the other
//! gets a `304` too. The list of `#` of the Chat tab and of `riff chat`
//! reads the open issues in the same way ([`open_issues`]).
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
//!     P -- no --> C
//!     L --> C["the closed items of the current wave,<br/>with their ETag"]
//!     C --> M["the full check of main"]
//!     M --> F["Forge: the facts of the board"]
//! ```

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use riff_client::waves::{Issue, Issues, QUESTION, is_tracker, wave_number};
use serde::Deserialize;

use crate::pr::{Gate, Gh, Reply};
use crate::rollout::{Label, Pull, Verify};
use crate::typeahead::OpenIssue;

/// The time between two reads of the forge while the Waves tab shows.
pub const READ_EVERY: Duration = Duration::from_secs(60);

/// The entries of one page of a REST list.
pub const PER_PAGE: usize = 100;

/// The most pages of open issues that a read reads: 1,000 issues and pull
/// requests, as `gh issue list --limit 1000` of `riff top`.
pub const MOST_PAGES: u32 = 10;

/// The title of the milestone of the items for later waves.
pub const BACKLOG: &str = "Backlog";

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
    /// The body. Only its `Needs:` line counts ([`crate::rollout::needs`]).
    #[serde(default)]
    pub body: Option<String>,
}

impl Entry {
    /// The issue of the entry: its title, its wave, whether it is a tracker
    /// or a question, and its needs.
    fn issue(self) -> Issue {
        Issue {
            tracker: is_tracker(&self.title, self.labels.iter().map(|l| l.name.as_str())),
            question: self.labels.iter().any(|l| l.name == QUESTION),
            needs: crate::rollout::needs(self.body.as_deref().unwrap_or_default()),
            number: self.number,
            title: self.title,
            milestone: self.milestone.map(|m| m.title),
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
    /// and keeps their last update for the age of their row.
    ///
    /// ```
    /// use riff::tui::forge::{Entry, Forge};
    ///
    /// let open: Vec<Entry> = serde_json::from_str(r#"[
    ///   {"number": 12, "title": "Show the board", "milestone": {"number": 5, "title": "Wave 3: Boards"}},
    ///   {"number": 13, "title": "Hold it", "milestone": {"number": 5, "title": "Wave 3: Boards"},
    ///    "labels": [{"name": "question"}]},
    ///   {"number": 14, "title": "Later", "milestone": {"number": 6, "title": "Wave 4"},
    ///    "body": "Needs: #15, #2"},
    ///   {"number": 15, "title": "Some day", "milestone": {"number": 1, "title": "Backlog"}},
    ///   {"number": 16, "title": "Release v4.11.0", "milestone": {"number": 5, "title": "Wave 3: Boards"},
    ///    "labels": [{"name": "release"}]},
    ///   {"number": 40, "title": "Show the board", "pull_request": {"url": "x"},
    ///    "updated_at": "2026-11-07T10:00:00Z"}
    /// ]"#)?;
    /// let pulls: Vec<riff::rollout::Pull> = serde_json::from_str(r#"[
    ///   {"number": 40, "headRefName": "worktree-issue-12", "isDraft": false, "statusCheckRollup": []}
    /// ]"#)?;
    /// let forge = Forge::of(open, &pulls);
    /// // The pull request is no item.
    /// assert!(!forge.issues.titles.contains_key(&40));
    /// assert_eq!(forge.issues.wave(), Some(&("Wave 3: Boards".into(), vec![12, 13, 16])));
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
        let issues = entries.into_iter().map(Entry::issue).collect();
        forge.issues = crate::top::with_pulls(Issues::of(issues), pulls);
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
/// path, from one read to the next. A clone shares them: the reads of the
/// Waves tab and of the Sessions tab keep one set.
#[derive(Debug, Clone, Default)]
pub struct Kept {
    replies: Arc<Mutex<BTreeMap<String, (String, String)>>>,
}

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
        let last = self.replies().get(path).cloned();
        match gh.ask_for(path, last.as_ref().map(|(etag, _)| etag.as_str()))? {
            Reply::Same => match last {
                Some((_, body)) => Ok(body),
                None => bail!("gh api {path}: GitHub answered 304 to a read with no ETag"),
            },
            Reply::New { etag, body } => {
                match etag {
                    Some(etag) => {
                        self.replies().insert(path.to_owned(), (etag, body.clone()));
                    }
                    None => {
                        self.replies().remove(path);
                    }
                }
                Ok(body)
            }
        }
    }

    /// The replies that this keeps. The lock holds for no call of `gh`.
    fn replies(&self) -> MutexGuard<'_, BTreeMap<String, (String, String)>> {
        self.replies.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The entries of the REST list at `path` now.
    fn entries(&self, gh: &Gh, path: &str) -> Result<Vec<Entry>> {
        let body = self.body(gh, path)?;
        serde_json::from_str(&body).with_context(|| format!("gh api {path}"))
    }
}

/// The open entries of `repo`: the issues and the pull requests of each
/// page of the REST list of the open issues, with the `ETag` that `kept`
/// keeps, up to [`MOST_PAGES`].
fn open(gh: &Gh, repo: &str, kept: &Kept) -> Result<Vec<Entry>> {
    let mut open = Vec::new();
    for page in 1..=MOST_PAGES {
        let entries = kept.entries(gh, &open_path(repo, page))?;
        let full = entries.len() >= PER_PAGE;
        open.extend(entries);
        if !full {
            break;
        }
    }
    Ok(open)
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
    let issues = open(gh, repo, kept)?
        .into_iter()
        .filter(|e| e.pull_request.is_none())
        .map(Entry::issue)
        .collect();
    Ok(Issues::of(issues))
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
    Ok(open(gh, repo, kept)?
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

/// [`open_issues`] of each repository of `repos`, in order.
pub fn open_issues_each(
    gh: &Gh,
    repos: &[String],
    kept: &Kept,
) -> Vec<(String, Result<Vec<OpenIssue>, String>)> {
    repos
        .iter()
        .map(|repo| {
            let read = open_issues(gh, repo, kept).map_err(|e| format!("{e:#}"));
            (repo.clone(), read)
        })
        .collect()
}

/// The facts of the forge of `repo` (`OWNER/REPO`) for its board, with
/// `gh` and the replies that `kept` keeps. It fails with the error of
/// `gh` when the open issues cannot be read. The other reads give no
/// facts when they fail: the board counts only the claims and the holds,
/// as in `riff top`.
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
///   *) echo 'gh: Not Found (HTTP 404)' >&2; exit 1 ;;
/// esac
/// "#;
/// let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script));
/// let kept = Kept::default();
/// let forge = read(&gh, "o/r", &kept)?;
/// assert_eq!(forge.issues.titles[&12], "Show it");
/// assert_eq!(forge.closed.as_ref().unwrap()[0].number, 11);
/// // No open pull request: no `gh pr list`.
/// let calls = std::fs::read_to_string(dir.path().join("calls"))?;
/// assert!(!calls.contains("pr list"), "{calls}");
/// // A read that changed nothing gives the same facts.
/// assert_eq!(read(&gh, "o/r", &kept)?, forge);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read(gh: &Gh, repo: &str, kept: &Kept) -> Result<Forge> {
    let open = open(gh, repo, kept)?;
    let pulls = match open.iter().any(|e| e.pull_request.is_some()) {
        true => crate::rollout::pulls(gh, repo).unwrap_or_default(),
        false => Vec::new(),
    };
    let mut forge = Forge::of(open, &pulls);
    forge.closed = match forge.wave_milestone() {
        Some(milestone) => kept
            .entries(gh, &closed_path(repo, milestone))
            .ok()
            .map(closed_of),
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

/// [`read`] of each repository of `repos`, in order.
pub fn read_each(gh: &Gh, repos: &[String], kept: &Kept) -> Vec<(String, Result<Forge, String>)> {
    repos
        .iter()
        .map(|repo| {
            let read = read(gh, repo, kept).map_err(|e| format!("{e:#}"));
            (repo.clone(), read)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake `gh` in `dir` that answers the REST reads of o/r from the
    /// files of `dir`: `open` (with its `ETag` in `open.etag`), `closed`,
    /// `pulls`, `runs` and `jobs`. It logs each call to `calls`.
    fn fake(dir: &std::path::Path) -> Gh {
        let script = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/calls"
tag=$(cat "$dir/open.etag" 2>/dev/null)
case "$*" in
  *"If-None-Match: $tag"*'state=open'*) echo 'gh: HTTP 304' >&2; exit 1 ;;
  *'issues?state=open'*) printf 'HTTP/2.0 200 OK\r\nEtag: %s\r\n\r\n' "$tag"; cat "$dir/open" ;;
  *'issues?state=closed'*) printf 'HTTP/2.0 200 OK\r\n\r\n'; cat "$dir/closed" ;;
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
        std::fs::write(dir.join("runs"), r#"{"check_runs":[]}"#).unwrap();
        std::fs::write(dir.join("jobs"), "[]").unwrap();
        Gh::at(isolated::write_program(dir.join("gh"), script))
    }

    fn calls(dir: &std::path::Path) -> String {
        std::fs::read_to_string(dir.join("calls")).unwrap_or_default()
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
        assert!(!calls(dir.path()).contains("pr list"));
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
        assert!(calls(dir.path()).contains("pr list"));
        assert_eq!(read.issues.titles.keys().collect::<Vec<_>>(), [&12]);
        assert_eq!(read.issues.verify, [12].into());
        assert_eq!(read.pulls[&12].number, 40);
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
        let read = read_each(&gh, &["o/r".to_owned()], &Kept::default());
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
