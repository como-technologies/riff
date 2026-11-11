//! The events of the GitHub App in the riff daemon (#1656): the stream of
//! the wakes of each wave, the reads of what a wake names, the safety
//! look, and the dropped events.
//!
//! # Design
//!
//! riff-server takes the events of the session App, and sends a wake for
//! each issue or pull request that changed (`riff_server::hooks`, #1655).
//! The loop of each wave follows the stream `GET /v1/github/watch` of its
//! repository ([`follow`]). Each wake goes into the [`Told`] of the
//! repository, and wakes the wave: at most one look each [`WAKE_GAP`].
//!
//! ```mermaid
//! sequenceDiagram
//!     participant S as riff-server
//!     participant F as the follower of the wave
//!     participant W as the look of the wave
//!     participant G as GitHub
//!     F->>S: GET /v1/github/watch?after=ID
//!     S->>F: a wake: pull_request, #41
//!     F->>F: Told: #41, the ID in github-wakes.json
//!     F->>W: wake up
//!     W->>G: gh pr view 41 (1 point)
//!     W->>W: patch the kept list of the open pull requests
//!     Note over W: each 30 minutes: the safety look, a full read
//! ```
//!
//! - **The ID of the last wake.** The daemon keeps it for each repository
//!   in [`WAKES_FILE`] in its folder. The stream opens with `after=ID`, so
//!   a daemon that was away, or a stream that broke, gets the wakes of the
//!   queue of riff-server. The first stream of a repository has no
//!   `after`: the first look of the wave reads in full.
//! - **Events on.** The events of a repository are on while its last wake
//!   is less than [`EVENTS_FOR`] old ([`Told::on`]): the ID of a wake is
//!   the time of riff-server. A riff with no App, or with no webhook, sends
//!   no wake, so its waves look as before ([`crate::changes`]).
//! - **The reads of a wake** ([`Told::facts`]). While the events are on,
//!   a look reads only what the wakes since the last look named, and
//!   patches the kept reply of the list ([`crate::pr::Gh::keep`]): a pull
//!   request with `gh pr view N`, an issue with `gh issue view N`. A wake
//!   with no number (a status, a check of a commit with no pull request),
//!   two pull requests or more, and a merge while another pull request is
//!   open read the list of the open pull requests: a merge changes the
//!   mergeable state of the others, and GitHub sends no event for it. Then
//!   the look takes the kept replies as new
//!   ([`crate::rollout::renew_open`]), so the look of the daemon and the
//!   sweep take them too.
//! - **The safety look.** A full read comes each [`SAFETY_LOOK_EVERY`],
//!   and an idle wave waits as long ([`crate::wave::look_wait`]). A read
//!   of a wake that fails makes the look read in full.
//! - **A dropped event.** The safety look compares the kept replies with
//!   its full read ([`changes`]). Each change that no wake named waits
//!   [`GRACE`] for its wake: the window of riff-server and the way to the
//!   daemon. Then it is a dropped event ([`Dropped`]): the daemon logs the
//!   repository, the item and the kind, and `riff daemon status` counts the
//!   dropped events of the last [`DROPPED_FOR`] ([`Shown`]).
//!
//! ```mermaid
//! flowchart TD
//!     L["a look of the wave"] --> O{"events on?"}
//!     O -- no --> C["the conditional reads (crate::changes)"]
//!     O -- yes --> D{"the last full read<br/>30 minutes old?"}
//!     D -- no --> P["read what the wakes named,<br/>patch the kept lists"]
//!     D -- yes --> F["the safety look: a full read"]
//!     F --> X["each change that no wake named"]
//!     X -- "no wake in GRACE" --> R["a dropped event: the log,<br/>riff daemon status"]
//! ```

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use riff_client::api::{self, Api};
use riff_client::link::STREAM_RETRY;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{GitHubEvent, GitHubWake};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Notify;

use crate::changes::Facts;
use crate::pr::Gh;
use crate::{rollout, text};

/// The time of the safety look: the longest time between two full reads
/// of the forge while the events of GitHub are on. An idle wave waits as
/// long for its next look.
pub const SAFETY_LOOK_EVERY: Duration = Duration::from_secs(30 * 60);

/// How long the events of a repository stay on after its last wake.
pub const EVENTS_FOR: Duration = Duration::from_secs(24 * 60 * 60);

/// How long a change that a safety look found waits for its wake before
/// it is a dropped event: more than the window of riff-server (5 s) and
/// the way of the wake.
pub const GRACE: Duration = Duration::from_secs(60);

/// The shortest time between two looks of a wave that wakes start: the
/// look takes each wake of the gap.
pub const WAKE_GAP: Duration = Duration::from_secs(30);

/// How long `riff daemon status` counts a dropped event.
pub const DROPPED_FOR: Duration = Duration::from_secs(24 * 60 * 60);

/// The file of the ID of the last wake of each repository, in the folder
/// of the daemon.
pub const WAKES_FILE: &str = "github-wakes.json";

/// How old a kept reply of a list can be for a patch.
const KEPT_FOR: Duration = Duration::from_secs(60 * 60);

/// How old a reply can be for the look after the patches: the look takes
/// the replies that it just made new.
const KEPT_AGE: Duration = Duration::from_secs(60);

/// The keys of an issue whose change an `issues` event tells.
const ISSUE_KEYS: [&str; 4] = ["title", "body", "milestone", "labels"];

/// The keys of a pull request whose change a `pull_request` event tells.
const PULL_KEYS: [&str; 6] = [
    "headRefName",
    "headRefOid",
    "isDraft",
    "autoMergeRequest",
    "reviewDecision",
    "body",
];

/// What the wakes since the last look named: each number with its kinds.
/// `None` is a commit with no pull request, for example a `status`.
pub type Named = BTreeMap<Option<u64>, Vec<GitHubEvent>>;

/// A change of one issue or pull request that a safety look found.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Change {
    /// The number of the issue or the pull request.
    pub number: u64,
    /// True for a pull request.
    pub pull: bool,
    /// The kind of the event that tells the change.
    pub kind: GitHubEvent,
}

/// A dropped event: a change that a safety look found, and that no wake
/// told (#1656).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dropped {
    /// The repository, `OWNER/NAME`.
    pub repo: String,
    /// What changed.
    pub change: Change,
    /// When the safety look found it, in milliseconds since the epoch.
    pub at_ms: i64,
}

/// The events of one repository for `riff daemon status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shown {
    /// The repository, `OWNER/NAME`.
    pub repo: String,
    /// The time of riff-server of the last wake, in milliseconds since the
    /// epoch.
    pub last_ms: i64,
    /// True while the events are on ([`Told::on`]).
    pub on: bool,
    /// The dropped events of the last [`DROPPED_FOR`].
    pub dropped: usize,
    /// The last dropped event.
    #[serde(default)]
    pub last_dropped: Option<Dropped>,
}

/// What the daemon knows of the events of one repository: the ID of the
/// last wake, what the wakes named since the last look, the time of the
/// last full read, and the dropped events (#1656).
#[derive(Debug)]
pub struct Told {
    repo: String,
    /// The file of the IDs ([`WAKES_FILE`]). With none, the ID is kept in
    /// memory only.
    file: Option<PathBuf>,
    safety: Duration,
    grace: Duration,
    gap: Duration,
    state: Mutex<State>,
    woken: Notify,
}

#[derive(Debug, Default)]
struct State {
    last: Option<u64>,
    named: Named,
    /// The start of the last full read while the events were on.
    full_ms: Option<i64>,
    /// The changes of the safety looks that wait for their wake.
    suspects: Vec<(Change, i64)>,
    dropped: VecDeque<Dropped>,
}

impl Told {
    /// The events of `repo`, with no wake yet and no file.
    pub fn new(repo: &str) -> Told {
        Told {
            repo: repo.to_owned(),
            file: None,
            safety: SAFETY_LOOK_EVERY,
            grace: GRACE,
            gap: WAKE_GAP,
            state: Mutex::default(),
            woken: Notify::new(),
        }
    }

    /// The events of `repo` of the daemon in the folder `dir`: the ID of
    /// its last wake comes from [`WAKES_FILE`], and each new one goes
    /// there.
    ///
    /// ```
    /// use riff::hooks::Told;
    /// use riff_core::wire::{GitHubEvent, GitHubWake};
    ///
    /// let dir = tempfile::tempdir()?;
    /// let told = Told::load(dir.path(), "o/r");
    /// assert_eq!(told.last(), None);
    /// let wake = GitHubWake { id: 7, repo: "o/r".parse()?, kinds: vec![GitHubEvent::PullRequest], number: Some(41), events: 1 };
    /// told.add(&wake);
    /// // A new start of the daemon takes the ID from the file.
    /// assert_eq!(Told::load(dir.path(), "o/r").last(), Some(7));
    /// assert_eq!(Told::load(dir.path(), "o/other").last(), None);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn load(dir: &Path, repo: &str) -> Told {
        let file = dir.join(WAKES_FILE);
        let last = read_ids(&file).get(repo).copied();
        let told = Told {
            file: Some(file),
            ..Told::new(repo)
        };
        lock(&told.state).last = last;
        told
    }

    /// The same, with the time of the safety look `safety`, the grace of a
    /// change `grace`, and the gap between two looks of the wakes `gap`:
    /// short times for a test.
    pub fn with_times(self, safety: Duration, grace: Duration, gap: Duration) -> Told {
        Told {
            safety,
            grace,
            gap,
            ..self
        }
    }

    /// The repository, `OWNER/NAME`.
    pub fn repo(&self) -> &str {
        &self.repo
    }

    /// The ID of the last wake, for `after` of the stream.
    pub fn last(&self) -> Option<u64> {
        lock(&self.state).last
    }

    /// The time of the safety look.
    pub fn safety(&self) -> Duration {
        self.safety
    }

    /// True while the events of the repository are on at `now_ms`: its last
    /// wake is less than [`EVENTS_FOR`] old. The ID of a wake is the time
    /// of riff-server when it went out.
    ///
    /// ```
    /// use riff::hooks::Told;
    /// use riff_core::wire::{GitHubEvent, GitHubWake};
    ///
    /// let told = Told::new("o/r");
    /// assert!(!told.on(1_000), "no wake yet");
    /// let at = 1_762_850_000_000;
    /// told.add(&GitHubWake { id: at, repo: "o/r".parse()?, kinds: vec![GitHubEvent::Issues], number: Some(12), events: 1 });
    /// assert!(told.on(at as i64 + 60_000));
    /// assert!(!told.on(at as i64 + 25 * 3_600_000), "a day with no wake");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn on(&self, now_ms: i64) -> bool {
        let for_ms = i64::try_from(EVENTS_FOR.as_millis()).unwrap_or(i64::MAX);
        lock(&self.state)
            .last
            .and_then(|id| i64::try_from(id).ok())
            .is_some_and(|at| now_ms - at < for_ms)
    }

    /// Takes the wake `wake`: its number and kinds go to the next look, its
    /// ID to the file, and it wakes the wave. It clears each change of the
    /// same number that waits for its wake. A wake that is not newer than
    /// the last one changes nothing.
    pub fn add(&self, wake: &GitHubWake) {
        {
            let mut state = lock(&self.state);
            if state.last.is_some_and(|last| wake.id <= last) {
                return;
            }
            state.last = Some(wake.id);
            let kinds = state.named.entry(wake.number).or_default();
            for kind in &wake.kinds {
                if !kinds.contains(kind) {
                    kinds.push(*kind);
                }
            }
            state
                .suspects
                .retain(|(change, _)| Some(change.number) != wake.number);
        }
        if let Some(file) = &self.file
            && let Err(e) = write_id(file, &self.repo, wake.id)
        {
            eprintln!("riff daemon: {}: {e:#}", self.repo);
        }
        self.woken.notify_one();
    }

    /// What the wakes named since the last call: the look takes it.
    pub fn take(&self) -> Named {
        std::mem::take(&mut lock(&self.state).named)
    }

    /// Waits for a wake that came after the last look, and then until
    /// `looked` and the gap ([`WAKE_GAP`]): the end of the wait of a wave.
    pub async fn woken(&self, looked: tokio::time::Instant) {
        self.woken.notified().await;
        tokio::time::sleep_until(looked + self.gap).await;
    }

    /// Forgets a wake that came before the look: the look takes it.
    pub fn looks(&self) {
        let _ = futures::FutureExt::now_or_never(self.woken.notified());
    }

    /// The events went off: the next safety look finds no dropped event,
    /// as nothing told the changes before it.
    pub fn off(&self) {
        lock(&self.state).full_ms = None;
    }

    /// Makes the next look a safety look, for a test: the last full read
    /// gets as old as the time of the safety look.
    #[cfg(test)]
    pub(crate) fn expire(&self) {
        let safety = i64::try_from(self.safety.as_millis()).unwrap_or(i64::MAX);
        if let Some(at) = &mut lock(&self.state).full_ms {
            *at = at.saturating_sub(safety);
        }
    }

    /// True when the next look reads in full at `now_ms`: no full read
    /// since the events are on, or the last one is [`SAFETY_LOOK_EVERY`]
    /// old.
    pub fn due(&self, now_ms: i64) -> bool {
        let safety = i64::try_from(self.safety.as_millis()).unwrap_or(i64::MAX);
        lock(&self.state)
            .full_ms
            .is_none_or(|at| now_ms - at >= safety)
    }

    /// The facts of the forge for the look of a wave at `now_ms`, while the
    /// events are on. `read` reads them with the `gh` that it gets: the
    /// default branch and the open milestones, issues and pull requests.
    ///
    /// Before the safety look ([`Told::due`]), it reads only what the wakes
    /// named, patches the kept replies, makes them new, and gives `read`
    /// a `gh` that takes them. A read that fails makes the look read in
    /// full. The full read compares the kept replies with the new ones:
    /// each change that no wake named waits for its wake ([`Told::confirm`]).
    pub fn facts(
        &self,
        gh: &Gh,
        now_ms: i64,
        read: impl Fn(&Gh) -> Result<Facts>,
    ) -> Result<Facts> {
        let named = self.take();
        if !self.due(now_ms) {
            match patch(gh, &self.repo, &named) {
                Ok(()) => {
                    rollout::renew_open(gh, &self.repo);
                    rollout::renew_merged(gh, &self.repo);
                    return read(&gh.clone().accepting(KEPT_AGE));
                }
                Err(e) => {
                    let e = format!("{e:#}");
                    eprintln!("riff daemon: {}: {}", self.repo, text::github_no_patch(&e));
                }
            }
        }
        let before = Lists::peek(gh, &self.repo);
        let had = lock(&self.state).full_ms.is_some();
        let facts = read(gh)?;
        lock(&self.state).full_ms = Some(now_ms);
        if had && let (Some(before), Some(after)) = (before, Lists::peek(gh, &self.repo)) {
            let told: BTreeSet<u64> = named.keys().flatten().copied().collect();
            let found = changes(&before, &after)
                .into_iter()
                .filter(|change| !told.contains(&change.number));
            lock(&self.state)
                .suspects
                .extend(found.map(|change| (change, now_ms)));
        }
        Ok(facts)
    }

    /// The changes that waited [`GRACE`] for their wake at `now_ms` with
    /// none: they are dropped events now, and the caller logs them. The
    /// daemon keeps them for [`DROPPED_FOR`].
    ///
    /// ```
    /// use std::time::Duration;
    /// use riff::hooks::{Change, Told};
    /// use riff_core::wire::{GitHubEvent, GitHubWake};
    ///
    /// let told = Told::new("o/r").with_times(Duration::ZERO, Duration::from_secs(60), Duration::ZERO);
    /// let change = |number| Change { number, pull: false, kind: GitHubEvent::Issues };
    /// told.found(vec![change(12), change(13)], 0);
    /// assert!(told.confirm(59_000).is_empty(), "they wait for their wake");
    /// // The wake of 12 comes late: it was no dropped event.
    /// told.add(&GitHubWake { id: 1, repo: "o/r".parse()?, kinds: vec![GitHubEvent::Issues], number: Some(12), events: 1 });
    /// let dropped = told.confirm(60_000);
    /// assert_eq!(dropped.len(), 1);
    /// assert_eq!((dropped[0].repo.as_str(), dropped[0].change.number), ("o/r", 13));
    /// assert!(told.confirm(61_000).is_empty(), "one time");
    /// assert_eq!(told.shown(61_000).unwrap().dropped, 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn confirm(&self, now_ms: i64) -> Vec<Dropped> {
        let grace = i64::try_from(self.grace.as_millis()).unwrap_or(i64::MAX);
        let mut state = lock(&self.state);
        let (old, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut state.suspects)
            .into_iter()
            .partition(|(_, at)| now_ms - at >= grace);
        state.suspects = waiting;
        let dropped: Vec<Dropped> = old
            .into_iter()
            .map(|(change, at_ms)| Dropped {
                repo: self.repo.clone(),
                change,
                at_ms,
            })
            .collect();
        state.dropped.extend(dropped.iter().cloned());
        let keep = i64::try_from(DROPPED_FOR.as_millis()).unwrap_or(i64::MAX);
        while state
            .dropped
            .front()
            .is_some_and(|d| now_ms - d.at_ms >= keep)
        {
            state.dropped.pop_front();
        }
        dropped
    }

    /// Takes `changes`, which a safety look found at `now_ms`, as changes
    /// that wait for their wake ([`Told::confirm`]).
    pub fn found(&self, changes: Vec<Change>, now_ms: i64) {
        lock(&self.state)
            .suspects
            .extend(changes.into_iter().map(|change| (change, now_ms)));
    }

    /// The events of the repository at `now_ms` for `riff daemon status`:
    /// `None` before its first wake. The count has each change that waited
    /// [`GRACE`] for its wake.
    pub fn shown(&self, now_ms: i64) -> Option<Shown> {
        let on = self.on(now_ms);
        let grace = i64::try_from(self.grace.as_millis()).unwrap_or(i64::MAX);
        let keep = i64::try_from(DROPPED_FOR.as_millis()).unwrap_or(i64::MAX);
        let state = lock(&self.state);
        let last_ms = i64::try_from(state.last?).unwrap_or(i64::MAX);
        let mut dropped: Vec<Dropped> = state
            .dropped
            .iter()
            .filter(|d| now_ms - d.at_ms < keep)
            .cloned()
            .collect();
        dropped.extend(
            state
                .suspects
                .iter()
                .filter(|(_, at)| now_ms - at >= grace)
                .map(|(change, at_ms)| Dropped {
                    repo: self.repo.clone(),
                    change: change.clone(),
                    at_ms: *at_ms,
                }),
        );
        Some(Shown {
            repo: self.repo.clone(),
            last_ms,
            on,
            dropped: dropped.len(),
            last_dropped: dropped.into_iter().max_by_key(|d| d.at_ms),
        })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Follows the wakes of the GitHub App of `thread` on riff-server as `me`
/// into `told` (#1656). Each connection asks for the wakes after the ID
/// of the last one ([`Told::last`]), so a daemon that was away gets the
/// wakes of the queue. Each [`GRACE`] it logs the new dropped events
/// ([`Told::confirm`]). It connects again after each end of the stream,
/// and never ends. It prints a fault of the stream one time until the
/// fault changes.
pub async fn follow(api: &Api, me: &SessionUri, thread: &ThreadName, told: &Told) {
    use futures::StreamExt;
    let wakes = api::follow(|| api.github_watch(me, thread, told.last()), STREAM_RETRY);
    tokio::pin!(wakes);
    let mut fault = None;
    loop {
        tokio::select! {
            wake = wakes.next() => match wake {
                Some(Ok(wake)) => {
                    fault = None;
                    told.add(&wake);
                }
                Some(Err(e)) => {
                    let e = format!("{e:#}");
                    if fault.as_ref() != Some(&e) {
                        eprintln!("riff daemon: {thread}: {}", text::github_no_wakes(&e));
                    }
                    fault = Some(e);
                }
                None => return,
            },
            () = tokio::time::sleep(told.grace) => {}
        }
        log_dropped(told);
    }
}

/// Logs each new dropped event of `told` now ([`Told::confirm`]).
pub fn log_dropped(told: &Told) {
    for dropped in told.confirm(crate::daemon::now_ms()) {
        eprintln!(
            "riff daemon: {}: {}",
            dropped.repo,
            text::github_dropped(&dropped)
        );
    }
}

/// The IDs of the last wakes in the file `file`, by repository. A file
/// that does not read gives none.
fn read_ids(file: &Path) -> BTreeMap<String, u64> {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// One writer of [`WAKES_FILE`] at a time in this process.
static IDS: Mutex<()> = Mutex::new(());

/// Writes `id` as the ID of the last wake of `repo` in the file `file`, in
/// place of the old file.
fn write_id(file: &Path, repo: &str, id: u64) -> Result<()> {
    let _one = lock(&IDS);
    let mut ids = read_ids(file);
    ids.insert(repo.to_owned(), id);
    let tmp = file.with_extension("json.new");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&ids)?)
        .with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, file).with_context(|| format!("cannot write {}", file.display()))
}

/// True for a kind of event that only a pull request has.
fn of_pull(kind: GitHubEvent) -> bool {
    matches!(
        kind,
        GitHubEvent::PullRequest
            | GitHubEvent::CheckRun
            | GitHubEvent::WorkflowRun
            | GitHubEvent::MergeGroup
    )
}

/// Reads on GitHub what `named` names, and patches the kept replies of
/// the lists of `repo` with it ([`Told::facts`]). It fails when a list
/// has no kept reply, or a read fails.
fn patch(gh: &Gh, repo: &str, named: &Named) -> Result<()> {
    if named.is_empty() {
        return Ok(());
    }
    let open = numbers(&kept(gh, &rollout::pull_args(repo))?);
    let mut pulls = BTreeMap::new();
    let mut issues = BTreeSet::new();
    let mut commit = false;
    for (number, kinds) in named {
        match number {
            None => commit = true,
            Some(n) if open.contains(n) || kinds.iter().any(|k| of_pull(*k)) => {
                pulls.insert(*n, kinds.clone());
            }
            Some(n) => {
                issues.insert(*n);
            }
        }
    }
    for n in issues {
        if patch_issue(gh, repo, n)? {
            pulls.insert(n, Vec::new());
        }
    }
    match pulls.first_key_value() {
        _ if commit || pulls.len() > 1 => read_pulls(gh, repo, &open),
        Some((n, kinds)) => patch_pull(gh, repo, *n, kinds, &open),
        None => Ok(()),
    }
}

/// Reads the issue `n` of `repo` with `gh issue view`, and patches the
/// kept list of the open issues: an open issue goes in, a closed one
/// goes out. True when `n` is a pull request: then the list stays.
fn patch_issue(gh: &Gh, repo: &str, n: u64) -> Result<bool> {
    let number = n.to_string();
    let view: Value = gh.json(&[
        "issue",
        "view",
        &number,
        "--repo",
        repo,
        "--json",
        rollout::ISSUE_VIEW_FIELDS,
    ])?;
    if view["url"]
        .as_str()
        .is_some_and(|url| url.contains("/pull/"))
    {
        return Ok(true);
    }
    let open = view["state"] == "OPEN";
    edit(gh, &rollout::issue_args(repo), |list| {
        put(list, n, open.then_some(view))
    })?;
    Ok(false)
}

/// Reads the pull request `n` of `repo` with `gh pr view`, and patches
/// the kept lists: an open pull request goes in the list of the open ones,
/// a merged one in the list of the merged ones. `kinds` are the kinds of
/// its wake, and `open` the open pull requests before. A merge while
/// another pull request is open reads that list ([`read_pulls`]). A wake
/// of `pull_request` or `merge_group` of an open one reads the merge queue
/// again ([`rollout::queue_of`]).
fn patch_pull(
    gh: &Gh,
    repo: &str,
    n: u64,
    kinds: &[GitHubEvent],
    open: &BTreeSet<u64>,
) -> Result<()> {
    let number = n.to_string();
    let view: Value = gh.json(&[
        "pr",
        "view",
        &number,
        "--repo",
        repo,
        "--json",
        rollout::PULL_VIEW_FIELDS,
    ])?;
    let state = view["state"].as_str().unwrap_or_default().to_owned();
    if state == "MERGED" {
        let merged = view.clone();
        edit(gh, &rollout::merged_args(repo), |list| {
            if !numbers(list).contains(&n) {
                list.insert(0, merged);
                list.truncate(100);
            }
        })?;
    }
    let live = state == "OPEN";
    edit(gh, &rollout::pull_args(repo), |list| {
        put(list, n, live.then_some(view))
    })?;
    if state == "MERGED" && open.iter().any(|other| *other != n) {
        return read_pulls(gh, repo, open);
    }
    let queue = kinds
        .iter()
        .any(|k| matches!(k, GitHubEvent::PullRequest | GitHubEvent::MergeGroup));
    if live && queue {
        rollout::queue_of(gh, repo);
    }
    Ok(())
}

/// Reads the list of the open pull requests of `repo` again, and the
/// list of the merged ones when a pull request of `open` left it.
fn read_pulls(gh: &Gh, repo: &str, open: &BTreeSet<u64>) -> Result<()> {
    let now: BTreeSet<u64> = rollout::pulls(gh, repo)?.iter().map(|p| p.number).collect();
    if open.iter().any(|n| !now.contains(n)) {
        rollout::merges(gh, repo)?;
    }
    Ok(())
}

/// The kept reply of the list `gh ARGS`, as JSON.
fn kept(gh: &Gh, args: &[&str]) -> Result<Vec<Value>> {
    let Some(reply) = gh.peek(args, KEPT_FOR) else {
        bail!("no kept reply of gh {}", args.join(" "));
    };
    serde_json::from_str(&reply).with_context(|| format!("gh {}", args.join(" ")))
}

/// Changes the kept reply of the list `gh ARGS` with `change`, and keeps
/// it as new.
fn edit(gh: &Gh, args: &[&str], change: impl FnOnce(&mut Vec<Value>)) -> Result<()> {
    let mut list = kept(gh, args)?;
    change(&mut list);
    gh.keep(args, &serde_json::to_string(&list)?);
    Ok(())
}

/// Takes the item `n` out of `list`, and puts `item` in its place, when
/// there is one: the newest number first, as `gh` lists them.
fn put(list: &mut Vec<Value>, n: u64, item: Option<Value>) {
    let at = list.iter().position(|v| v["number"] == n);
    if let Some(at) = at {
        list.remove(at);
    }
    if let Some(item) = item {
        let at = at.unwrap_or_else(|| {
            list.iter()
                .position(|v| v["number"].as_u64().is_some_and(|m| m < n))
                .unwrap_or(list.len())
        });
        list.insert(at, item);
    }
}

/// The numbers of the items of `list`.
fn numbers(list: &[Value]) -> BTreeSet<u64> {
    list.iter().filter_map(|v| v["number"].as_u64()).collect()
}

/// The lists of a repository that a safety look compares: the open pull
/// requests, the open issues, and the last merged pull requests, as `gh`
/// gives them.
#[derive(Debug, Clone, Default)]
pub struct Lists {
    pub pulls: Vec<Value>,
    pub issues: Vec<Value>,
    pub merged: Vec<Value>,
}

impl Lists {
    /// The kept replies of the lists of `repo`, when each has one.
    fn peek(gh: &Gh, repo: &str) -> Option<Lists> {
        Some(Lists {
            pulls: kept(gh, &rollout::pull_args(repo)).ok()?,
            issues: kept(gh, &rollout::issue_args(repo)).ok()?,
            merged: kept(gh, &rollout::merged_args(repo)).ok()?,
        })
    }
}

/// Each change from `before` to `after` that an event of the session App
/// tells, with that kind (#1656): an issue that came or went, or whose
/// title, body, milestone or labels changed (`issues`), or whose comments
/// changed (`issue_comment`); a pull request that came, went or merged,
/// or whose branch, head, draft, auto-merge, review or body changed
/// (`pull_request`); a check run (`check_run`) or a status (`status`) of
/// its head. The mergeable state of a pull request, and a milestone, are
/// no change: GitHub sends the App no event for them.
///
/// ```
/// use riff::hooks::{Change, Lists, changes};
/// use riff_core::wire::GitHubEvent;
///
/// let issue = |n: u64, label: &str| serde_json::json!({ "number": n, "title": "t", "labels": [{ "name": label }], "comments": [] });
/// let pull = |n: u64, conclusion: &str| serde_json::json!({
///     "number": n, "headRefOid": "c1", "mergeable": "MERGEABLE",
///     "statusCheckRollup": [{ "name": "Gate", "conclusion": conclusion }],
/// });
/// let before = Lists { issues: vec![issue(12, "a"), issue(13, "a")], pulls: vec![pull(41, "")], merged: vec![] };
/// let mut after = before.clone();
/// after.issues = vec![issue(12, "b"), issue(14, "a")];
/// after.pulls = vec![pull(41, "SUCCESS")];
/// let found = changes(&before, &after);
/// let change = |number, pull, kind| Change { number, pull, kind };
/// assert_eq!(found, [
///     change(12, false, GitHubEvent::Issues),
///     change(13, false, GitHubEvent::Issues),
///     change(14, false, GitHubEvent::Issues),
///     change(41, true, GitHubEvent::CheckRun),
/// ]);
/// // The mergeable state is no change.
/// let mut conflict = before.clone();
/// conflict.pulls[0]["mergeable"] = "CONFLICTING".into();
/// assert!(changes(&before, &conflict).is_empty());
/// ```
pub fn changes(before: &Lists, after: &Lists) -> Vec<Change> {
    let mut found = BTreeSet::new();
    let by = |list: &[Value]| -> BTreeMap<u64, Value> {
        list.iter()
            .filter_map(|v| Some((v["number"].as_u64()?, v.clone())))
            .collect()
    };
    let (old, new) = (by(&before.issues), by(&after.issues));
    for n in old.keys().chain(new.keys()) {
        let kinds = match (old.get(n), new.get(n)) {
            (Some(a), Some(b)) => {
                let mut kinds = Vec::new();
                if ISSUE_KEYS.iter().any(|k| a[k] != b[k]) {
                    kinds.push(GitHubEvent::Issues);
                }
                if a["comments"] != b["comments"] {
                    kinds.push(GitHubEvent::IssueComment);
                }
                kinds
            }
            _ => vec![GitHubEvent::Issues],
        };
        found.extend(kinds.into_iter().map(|kind| Change {
            number: *n,
            pull: false,
            kind,
        }));
    }
    let (old, new) = (by(&before.pulls), by(&after.pulls));
    for n in old.keys().chain(new.keys()) {
        let kinds = match (old.get(n), new.get(n)) {
            (Some(a), Some(b)) => {
                let mut kinds = Vec::new();
                if PULL_KEYS.iter().any(|k| a[k] != b[k]) {
                    kinds.push(GitHubEvent::PullRequest);
                }
                let (runs_a, statuses_a) = rollup(a);
                let (runs_b, statuses_b) = rollup(b);
                if runs_a != runs_b {
                    kinds.push(GitHubEvent::CheckRun);
                }
                if statuses_a != statuses_b {
                    kinds.push(GitHubEvent::Status);
                }
                kinds
            }
            _ => vec![GitHubEvent::PullRequest],
        };
        found.extend(kinds.into_iter().map(|kind| Change {
            number: *n,
            pull: true,
            kind,
        }));
    }
    let merged = numbers(&before.merged);
    found.extend(
        numbers(&after.merged)
            .into_iter()
            .filter(|n| !merged.contains(n))
            .map(|number| Change {
                number,
                pull: true,
                kind: GitHubEvent::PullRequest,
            }),
    );
    found.into_iter().collect()
}

/// The check runs and the statuses of the head of the pull request `pull`.
fn rollup(pull: &Value) -> (Vec<&Value>, Vec<&Value>) {
    pull["statusCheckRollup"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .partition(|check| check.get("context").is_none())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::rollout::Open;

    /// A fake `gh` that serves the lists and the items from the files of
    /// its folder, and logs each call in `gh.log`.
    const FORGE: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/gh.log"
show() { if [ -e "$dir/$1" ]; then cat "$dir/$1"; else echo '[]'; fi; }
case "$1 $2" in
  "pr list")
    case "$*" in
      *"--state merged"*) show merged ;;
      *) show pulls ;;
    esac ;;
  "issue list") show issues ;;
  "pr view") cat "$dir/pull-$3" ;;
  "issue view") cat "$dir/issue-$3" ;;
  "api graphql") echo '{}' ;;
  "api repos/o/r/commits/"*) echo 'a commit' ;;
  *) exit 1 ;;
esac
"#;

    struct Forge {
        dir: tempfile::TempDir,
        gh: Gh,
    }

    impl Forge {
        fn new() -> Forge {
            let dir = tempfile::tempdir().unwrap();
            let gh =
                Gh::at(isolated::write_program(dir.path().join("gh"), FORGE)).with_shared_reads();
            Forge { dir, gh }
        }

        fn put(&self, name: &str, value: &Value) {
            std::fs::write(self.dir.path().join(name), value.to_string()).unwrap();
        }

        /// The calls since the last one, and a new log.
        fn calls(&self) -> Vec<String> {
            let log = self.dir.path().join("gh.log");
            let calls = std::fs::read_to_string(&log).unwrap_or_default();
            let _ = std::fs::remove_file(&log);
            calls.lines().map(str::to_owned).collect()
        }
    }

    fn issue(n: u64, title: &str) -> Value {
        serde_json::json!({ "number": n, "title": title, "body": "Done when: x.", "comments": [], "labels": [], "milestone": { "title": "Wave 1" } })
    }

    fn pull(n: u64, head: &str) -> Value {
        serde_json::json!({ "number": n, "headRefName": format!("worktree-issue-{}", n - 40), "headRefOid": head, "statusCheckRollup": [], "body": "" })
    }

    /// The read of a look: the lists of the forge, with `gh`.
    fn read(gh: &Gh) -> Result<Facts> {
        rollout::merges(gh, "o/r")?;
        Ok(Facts {
            branch: "main".into(),
            open: Open {
                milestones: Vec::new(),
                issues: rollout::issues(gh, "o/r")?,
                pulls: rollout::pulls(gh, "o/r")?,
            },
        })
    }

    fn wake(id: u64, kind: GitHubEvent, number: Option<u64>) -> GitHubWake {
        GitHubWake {
            id,
            repo: "o/r".parse().unwrap(),
            kinds: vec![kind],
            number,
            events: 1,
        }
    }

    const MINUTE: i64 = 60_000;

    /// A wake of an issue reads that issue only, and the look sees it.
    #[test]
    fn a_wake_reads_only_what_it_names() {
        let forge = Forge::new();
        forge.put(
            "issues",
            &serde_json::json!([issue(12, "old"), issue(11, "eleven")]),
        );
        forge.put("pulls", &serde_json::json!([pull(41, "c1")]));
        let told = Told::new("o/r");
        // The first look reads in full.
        let first = told.facts(&forge.gh, 0, read).unwrap();
        assert_eq!(first.open.issues[0].title, "old");
        assert!(forge.calls().iter().any(|c| c.starts_with("issue list")));

        let mut view = issue(12, "new");
        view["state"] = "OPEN".into();
        view["url"] = "https://github.com/o/r/issues/12".into();
        forge.put("issue-12", &view);
        forge.put(
            "issues",
            &serde_json::json!([issue(12, "new on the list"), issue(11, "eleven")]),
        );
        told.add(&wake(1, GitHubEvent::Issues, Some(12)));
        let second = told.facts(&forge.gh, MINUTE, read).unwrap();
        assert_eq!(second.open.issues[0].title, "new");
        assert_eq!(second.open.issues[1].title, "eleven");
        let calls = forge.calls();
        assert_eq!(calls.len(), 1, "{calls:?}");
        assert!(
            calls[0].starts_with("issue view 12 --repo o/r --json"),
            "{calls:?}"
        );

        // A look with no wake reads nothing.
        told.facts(&forge.gh, 2 * MINUTE, read).unwrap();
        assert!(forge.calls().is_empty());
    }

    /// A merged pull request goes out of the open list, into the merged
    /// one. With another one open, the look reads the open list.
    #[test]
    fn a_merge_reads_the_list_only_while_another_pull_request_is_open() {
        let forge = Forge::new();
        forge.put(
            "pulls",
            &serde_json::json!([pull(42, "c2"), pull(41, "c1")]),
        );
        let told = Told::new("o/r");
        told.facts(&forge.gh, 0, read).unwrap();
        forge.calls();
        let merged = serde_json::json!({ "number": 41, "state": "MERGED", "headRefName": "worktree-issue-1", "body": "Refs #1", "mergeCommit": { "oid": "m41" } });
        forge.put("pull-41", &merged);
        forge.put("pulls", &serde_json::json!([pull(42, "c2")]));
        forge.put("merged", &serde_json::json!([merged]));
        told.add(&wake(1, GitHubEvent::PullRequest, Some(41)));
        let facts = told.facts(&forge.gh, MINUTE, read).unwrap();
        assert_eq!(facts.open.pulls.len(), 1);
        let calls = forge.calls();
        assert!(calls[0].starts_with("pr view 41"), "{calls:?}");
        assert!(
            calls
                .iter()
                .any(|c| c.starts_with("pr list") && !c.contains("merged")),
            "{calls:?}"
        );
        let merges =
            rollout::merges(&forge.gh.clone().accepting(Duration::from_secs(60)), "o/r").unwrap();
        assert_eq!(merges[&1].pull, 41);

        // The last open one merges: no list.
        forge.put("pull-42", &serde_json::json!({ "number": 42, "state": "MERGED", "headRefName": "worktree-issue-2", "body": "", "mergeCommit": { "oid": "m42" } }));
        told.add(&wake(2, GitHubEvent::PullRequest, Some(42)));
        let facts = told.facts(&forge.gh, 2 * MINUTE, read).unwrap();
        assert!(facts.open.pulls.is_empty());
        let calls = forge.calls();
        assert_eq!(calls.len(), 1, "{calls:?}");
        assert!(calls[0].starts_with("pr view 42"), "{calls:?}");
    }

    /// A wake with no number reads the list of the open pull requests.
    #[test]
    fn a_wake_of_a_commit_reads_the_open_pull_requests() {
        let forge = Forge::new();
        forge.put("pulls", &serde_json::json!([pull(41, "c1")]));
        let told = Told::new("o/r");
        told.facts(&forge.gh, 0, read).unwrap();
        forge.calls();
        told.add(&wake(1, GitHubEvent::Status, None));
        told.facts(&forge.gh, MINUTE, read).unwrap();
        let calls = forge.calls();
        assert!(
            calls
                .iter()
                .any(|c| c.starts_with("pr list --repo o/r --state open")),
            "{calls:?}"
        );
        assert!(!calls.iter().any(|c| c.starts_with("issue")), "{calls:?}");
    }

    /// A read of a wake that fails makes the look read in full.
    #[test]
    fn a_read_that_fails_reads_in_full() {
        let forge = Forge::new();
        let told = Told::new("o/r");
        told.facts(&forge.gh, 0, read).unwrap();
        forge.calls();
        // No file `issue-12`: the view fails.
        told.add(&wake(1, GitHubEvent::Issues, Some(12)));
        let reads = Cell::new(0);
        told.facts(&forge.gh, MINUTE, |gh| {
            reads.set(reads.get() + 1);
            read(gh)
        })
        .unwrap();
        assert_eq!(reads.get(), 1);
        assert!(forge.calls().iter().any(|c| c.starts_with("issue list")));
    }

    /// The safety look reads in full each 30 minutes, and each change that
    /// no wake named waits for its wake, then is a dropped event.
    #[test]
    fn the_safety_look_finds_a_dropped_event() {
        let forge = Forge::new();
        forge.put("issues", &serde_json::json!([issue(12, "twelve")]));
        let told = Told::new("o/r");
        told.facts(&forge.gh, 0, read).unwrap();
        assert!(!told.due(29 * MINUTE));
        assert!(told.due(30 * MINUTE));
        // Issue 13 comes with no event, issue 12 with one.
        forge.put(
            "issues",
            &serde_json::json!([issue(13, "thirteen"), issue(12, "new")]),
        );
        told.add(&wake(1, GitHubEvent::Issues, Some(12)));
        forge.calls();
        let facts = told.facts(&forge.gh, 30 * MINUTE, read).unwrap();
        assert_eq!(facts.open.issues.len(), 2);
        assert!(forge.calls().iter().any(|c| c.starts_with("issue list")));
        let dropped = told.confirm(31 * MINUTE);
        let change = Change {
            number: 13,
            pull: false,
            kind: GitHubEvent::Issues,
        };
        assert_eq!(
            dropped,
            [Dropped {
                repo: "o/r".into(),
                change,
                at_ms: 30 * MINUTE
            }]
        );
        let shown = told.shown(31 * MINUTE).unwrap();
        assert_eq!(
            (shown.dropped, shown.last_dropped.as_ref()),
            (1, dropped.first())
        );
    }

    /// The first full read after the events came on finds no dropped event:
    /// no event told the changes before it.
    #[test]
    fn the_first_full_read_finds_no_dropped_event() {
        let forge = Forge::new();
        // A look of the conditional reads kept the lists.
        read(&forge.gh).unwrap();
        forge.put("issues", &serde_json::json!([issue(13, "thirteen")]));
        let told = Told::new("o/r");
        told.facts(&forge.gh, 0, read).unwrap();
        assert!(told.confirm(10 * MINUTE).is_empty());
    }

    /// A pull request that a wake of `issue_comment` names reads as a pull
    /// request.
    #[test]
    fn a_comment_on_a_pull_request_reads_the_pull_request() {
        let forge = Forge::new();
        let told = Told::new("o/r");
        told.facts(&forge.gh, 0, read).unwrap();
        forge.calls();
        forge.put("issue-43", &serde_json::json!({ "number": 43, "state": "OPEN", "url": "https://github.com/o/r/pull/43" }));
        let mut view = pull(43, "c3");
        view["state"] = "OPEN".into();
        forge.put("pull-43", &view);
        told.add(&wake(1, GitHubEvent::IssueComment, Some(43)));
        let facts = told.facts(&forge.gh, MINUTE, read).unwrap();
        assert_eq!(facts.open.pulls[0].number, 43);
        assert!(facts.open.issues.is_empty());
    }

    #[test]
    fn put_keeps_the_newest_number_first() {
        let n = |n: u64| serde_json::json!({ "number": n });
        let mut list = vec![n(14), n(12), n(10)];
        put(&mut list, 13, Some(n(13)));
        put(&mut list, 9, Some(n(9)));
        put(&mut list, 12, None);
        put(&mut list, 15, Some(n(15)));
        let order: Vec<u64> = list.iter().filter_map(|v| v["number"].as_u64()).collect();
        assert_eq!(order, [15, 14, 13, 10, 9]);
    }

    #[test]
    fn a_wake_that_is_not_newer_changes_nothing() {
        let told = Told::new("o/r");
        told.add(&wake(5, GitHubEvent::Issues, Some(12)));
        told.add(&wake(5, GitHubEvent::Issues, Some(13)));
        told.add(&wake(4, GitHubEvent::Issues, Some(14)));
        assert_eq!(told.last(), Some(5));
        assert_eq!(told.take().keys().copied().collect::<Vec<_>>(), [Some(12)]);
        assert!(told.take().is_empty());
    }
}
