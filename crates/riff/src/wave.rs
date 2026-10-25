//! Let riff run the wave (#719): the riff daemon starts one session for
//! each free item of the current wave, one for each pull request that
//! waits for a verify (decisions 8 and 34 of #699), one to rebase
//! each pull request with a conflict with `main` (#1082), and one to fix
//! each pull request with a failed check. It tells the lead of a pull
//! request that no session can fix (#279).
//!
//! # Design
//!
//! `riff wave run` in a clone asks the daemon to run the wave of that
//! repository ([`Wave`]). The daemon keeps each wave in [`WAVES_FILE`]
//! in its folder, and starts its loop again after its own restart.
//! `riff wave stop` ends the loop: the daemon starts no new session
//! there, and the sessions that run go on.
//!
//! Each item has one session: it starts, does the item and ends
//! (01M4Z60B7NW4GHKGCYZZDG26R6). A verify is an item too: a new session
//! with a clone at the head commit of the pull request. No session
//! waits idle, and no session clears its context.
//!
//! ```mermaid
//! flowchart TD
//!     A["wait for room (crate::room)"] --> S["sweep: the clone of each item whose work is over goes"]
//!     S --> L["look: the pause, the claims, the holds,<br/>the free work of gh, the full check of each waiting pull request"]
//!     L --> P{"pick"}
//!     P -- "an item, and a fact of the host fails" --> W
//!     P -- "no work" --> W["give the room back; wait for an end, or LOOK_EVERY"] --> A
//!     P -- "an item" --> J["a job: claim, clone, start, wait for the end,<br/>the steps after the end, release"]
//!     J -.-> A
//! ```
//!
//! # The pick
//!
//! [`pick`] takes the first item of the free work of the look
//! ([`look_on`]): the free work of the rollout of the lead
//! ([`crate::rollout::free_work`]) with the fixes ([`fixes`]). A fix
//! item of a broken `main` first
//! ([`crate::rollout::BROKEN_MAIN`], 01M51QEYS538XA58KDRY844T52), then
//! the rebase of each pull request with a conflict (see "A conflict with
//! main"), then the fix of each pull request with a failed check (see
//! "A pull request that cannot merge"), then the pull requests that wait
//! for a verify, then the free items of the current wave. It never
//! starts:
//!
//! - an item while the riff or the repository is paused;
//! - a held item (01M4ZV3B6M7PZRJ39XTXBZP3ZH);
//! - a tracker issue, its build or its verify: it has no code of its
//!   own ([`rollout::Issue::tracker`], 01M5ND16V2SVN8A9K7W90VC6X9);
//! - a build of an issue whose body has no line that starts with
//!   `Done when:` ([`rollout::Issue::has_done_when`],
//!   01M5R0HEP1MXWYY643F1N1DWEM): a session stops at once without it. The
//!   lead gets one message for each such issue ([`Look::no_done_when`],
//!   [`text::wave_no_done_when`]). The wave takes the issue at its next
//!   look after the line comes. A verify and a rebase do not change;
//! - an item that a session claims, or that a job of the wave runs;
//! - the verify of an item while a session holds the item: its author
//!   still works. The verify is a new session: an author never verifies
//!   its own item (01M4ZV3B6NKPEZ5FNE27X95GF0);
//! - a verify before the full check of the head commit is done: each
//!   check that the rules of the default branch require, but `riff/verify`
//!   ([`repo_setup::full_check`], 01M5QYDHN27RC5SRHAB56PFKK0);
//! - an item that started [`STARTS`] times in this run of the wave. The
//!   lead gets one message for it. A start whose claim riff-server
//!   refused for a pause does not count (01M558EYCXPFW8CJV3XJ7HK3NR).
//!   `riff wave run` on the wave clears the starts
//!   (01M57A7A48A70Y7PTD95A3MBYC);
//! - the rebase of a head commit that started one time in this run of
//!   the wave ([`REBASES`], 01M57FJ3WWMQSDBFRRHXN3SMFQ);
//! - the fix of the failed checks of a head commit that started one time
//!   in this run of the wave ([`CHECKS`]);
//! - any item while a fact of the host fails ([`World::host`]): the
//!   sign-in of `gh`, or the sign-in of `claude` with less than
//!   [`run::MIN_MINUTES`] left. No item can start then, so no start
//!   counts. The wave waits, and looks again at [`LOOK_EVERY`]. The lead
//!   gets one message when the wave starts to wait, and `riff daemon
//!   status` shows the reason (01M57A7A1W2K9440WZA1QZ3M9T).
//!
//! # A conflict with main
//!
//! `main` is the default branch of the repository, also in the start text
//! of a verify and in the look of the Gate. The wave reads it from the forge
//! at each look ([`Look::branch`], [`pr::default_branch`],
//! 01M5R56GN3DC09GAPAF69GENEZ), so `master` and `trunk` work the same, and
//! `waves.json` has no branch. A branch that does not read is the error
//! of the look: the wave starts no work at that look.
//!
//! A pull request of an item with auto-merge on and a conflict with
//! `main` ([`rollout::Pull::conflict`]) cannot merge, also after its
//! verify passed. No session holds the item then: the session of a
//! wave ends after its pull request. So the wave starts a build session
//! for the item, with the start text of a rebase
//! ([`text::wave_rebase_start`]): rebase the branch on the default branch, resolve
//! the conflict, run the checks, and push to the same pull request
//! (01M57FJ3WWMQSDBFRRHXN3SMFQ). A new full check and a new verify of the new
//! head follow. The verify is a new session, as each verify is.
//!
//! ```mermaid
//! flowchart LR
//!     C["auto-merge on, CONFLICTING,<br/>no failed verify"] --> R["a build: Work::Rebase<br/>at the head commit"]
//!     R -- "a push" --> V["a new head: a new full check, then a new verify"]
//!     R -- "no push" --> L["a message to the lead"]
//! ```
//!
//! [`fixes`] puts each such item in the free work after the fix items
//! of a broken `main`, before each other item. The wave starts no
//! verify of a head with a conflict: a pass counts only for its commit,
//! and the rebase makes a new one. A pull request whose verify failed is
//! free work as a build already, with the failed result, until a new
//! verify request for the same head comes ([`reasked`], #1157): then it
//! waits for a verify again.
//!
//! Each head commit with a conflict starts one session in a run of the
//! wave: [`Work::key`] names the rebase by the item and the head. A pause
//! or a fault of the host takes the start back, as for each item.
//! `riff wave run` clears the starts, so a person can start the rebase
//! of the same head again. When the session ends with the head that had
//! the conflict, the lead gets a message in place of the verify request
//! (01M57FJ3Z8AFDY9G0KTAYZJESY).
//!
//! # The look
//!
//! Beside its loop, each wave has a look ([`crate::look::look_wave`],
//! 01M5QVH3MAEPPJN9PPVDX6N2EY). It takes no room, so a host that is
//! full still sees a broken `main`. Each minute it reads the open issues
//! and pull requests with `gh` and the claims on riff-server, and posts
//! as the person ([`World::gh`], [`World::claims`], [`World::post`]):
//!
//! ```mermaid
//! flowchart LR
//!     S["riff wave run"] --> L["the loop: the work"]
//!     S --> W["the look, each minute"]
//!     W --> P["a pull request that stops:<br/>a message to its holder or the lead"]
//!     W --> M["the full check of main:<br/>a message, the fix item, a note"]
//!     X["riff wave stop"] --> E["the loop and the look end"]
//! ```
//!
//! # A pull request that cannot merge
//!
//! A conflict is one reason that a pull request cannot merge by itself
//! ([`rollout::Stuck`], #279). The wave reads two more from `gh` at each
//! look, and nobody has to remember to check:
//!
//! ```mermaid
//! flowchart TD
//!     P["an open pull request,<br/>the verify passed"] --> C{"a check of the head failed?<br/>(statusCheckRollup)"}
//!     C -- "yes, an item" --> B["a build: Work::Checks at the head commit,<br/>with the names of the checks"]
//!     C -- "yes, a release" --> M["a message to the lead, with the checks"]
//!     C -- "no" --> A{"auto-merge off?"}
//!     A -- "yes" --> N["a message to the lead,<br/>with the command gh pr merge --auto --squash N"]
//!     A -- "no" --> W["it merges by itself"]
//!     B -- "a push" --> V["a new full check and a new verify of the new head"]
//!     B -- "no push, the check still failed" --> L["a message to the lead"]
//!     B -- "no push, the check runs again" --> T["a note to the lead"]
//! ```
//!
//! - **A failed check after a pass** (01M5QW2KVAS05Q72630RNW47G0).
//!   [`rollout::Pull::verify`] reads only the status `riff/verify`. A
//!   check of the rollup that failed ([`rollout::Check::failed`]), for
//!   example the `Gate`, is no free work: the item has a passed verify.
//!   The wave starts a build session for the item with the start text
//!   [`text::wave_checks_start`]: it names the checks, tells to read the
//!   failed run, and to fix the code and push to the same pull request.
//!   For a known flake it allows one run of the job again. The rollup
//!   does not say which check a rule requires, so each failed check
//!   counts. A check that was cancelled or skipped did not fail.
//! - **Auto-merge off** (01M5QW2M1M5QZJXSP0BY6XG118). A pull request
//!   whose verify passed and that has no auto-merge never merges: nothing
//!   turns it on. A conflict gets no rebase then
//!   ([`rollout::Pull::conflict`] needs auto-merge). The lead gets one
//!   message with the reason and the command
//!   (`gh pr merge --auto --squash N`).
//! - **A release** (01M5QW2M4WX53N29SHRV0S32WM). No session builds a
//!   release (the label `release`, or a branch `release-…`). A failed
//!   check of a release pull request gets a message to the lead, with the
//!   names of the checks, at any state of its verify.
//!
//! Each stuck state comes one time. A message is keyed by the pull
//! request, its head commit and its reason; the build of a failed check
//! by the item and the head ([`Work::key`]). So a new head is a new
//! state. A message is not repeated while the wave runs. A build of a
//! failed check runs one time for a head in a run of the wave
//! ([`CHECKS`]), and `riff wave run` clears the starts. When a pull
//! request has a conflict and a failed check, the rebase comes first:
//! the new head has new checks.
//!
//! The session may not push: for a flake it runs the job again
//! (01M5QW2KYFM4C287D9NMSPH8TA). After its end the daemon reads the pull
//! request. When the head is the same and a check still failed, the lead
//! gets a message ([`text::wave_checks_not_fixed`]); else the lead gets a
//! note ([`text::wave_checks_rerun`]). A push gets the verify request of
//! the new head, as each build does.
//!
//! `riff top` and `riff wave list` show the item of a stuck pull request
//! in the group `stuck`, with the reason, for example
//! `stuck: #250 (checks)`, while no session claims it
//! ([`riff_client::waves::Board`], 01M5QW2M8FPBMMF4S0D8PC39AB).
//!
//! # A job
//!
//! A job does the steps of the host of `riff run` itself
//! ([`crate::run`]), with the sign-in of the person on this machine: a
//! new worker session on riff-server, the claim, the keep-alive,
//! the clone, the start text, and the start of the container in the
//! room that the loop took. The [`Record`] of the session holds the
//! [`Job`], so a new daemon goes on with the keep-alive and does the
//! steps after the end ([`crate::daemon::Daemon::recover`]).
//!
//! When riff-server refuses the claim, the job ends the new worker
//! session at once, so no idle worker with no item stays
//! (01M558EYCXPFW8CJV3XJ7HK3NR). A pause can come between the look and
//! the claim. Its refusal (`paused`) is no fault: the lead gets no
//! message, and the start does not count. Each other refusal is a failed
//! start: the lead gets a message, and the start counts.
//!
//! After the end ([`Live::finish`]):
//!
//! - A build sends a verify request for the open pull request of its
//!   branch to the lead, as `riff run` does. When the issue has the
//!   label [`crate::rollout::NO_VERIFY`], or a lead holds `verify-ITEM`,
//!   the lead gets a note with the reason in place of the verify request
//!   ([`crate::run::no_verify`], 01M51KH0H7J35PEKMHP5KQB84T).
//! - A verify reads the result from the last turn of `claude` in
//!   `events.jsonl`: its first line is `PASS` or `FAIL` ([`verdict`]).
//!   The daemon reports it as `riff verify` does: the comment on the
//!   pull request, the status `riff/verify` of the commit, and the
//!   result to the holder of the item and to the lead.
//! - Then the daemon releases the item and ends the worker session.
//!
//! # The clones
//!
//! The clone of a build stays until the work of its branch is over
//! (01M4Z81ARSTAK8HR96S084SDFD). Before each look, the sweep of the
//! daemon removes the folder of each session of the repository that does
//! not run, when the pull request of its branch merged or closed, or its
//! issue is closed, and its clone holds no work that is not on `origin`
//! ([`crate::daemon::sweep`], 01M53NJ5V27BCHDNHK2THQQZZQ,
//! 01M53S7WAXD46YPN0DR96XSTMG). For a merged pull request with `Refs #N`,
//! it first adds the comment `Merged in #PR (COMMIT)` to the issue, as
//! the verifier of a worker does, so the item is not free work again.
//! The folder of a verify that did not pass goes at its end. The daemon
//! also sweeps with no wave.
//!
//! A job holds `Daemon::in_job` of its item from before the clone of
//! its session to the end of `finish`, so the sweep keeps its folder
//! while a step after the end reads it (01M53S7WDPQ2GWHJWJ6SZXZ5PN). Each
//! `gh` call of the sweep has a time limit, so a `gh` that never ends
//! stops no wave (01M53S7WGM9DQV2DXFHBRSMTP8).
//!
//! # The world
//!
//! The loop reaches the forge and riff-server through a [`World`]. The
//! daemon uses [`Live`]: `gh` and riff-server. A test gives a fake
//! world, with the fake runtime ([`crate::runtime::Fake`]). The daemon
//! also keeps the session of the lead alive through it, and follows the
//! wakes of a session for `riff relay watch` ([`World::wake`]).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use riff_core::name::{Place, SessionUri, ThreadName};
use riff_core::wire::{
    HoldInfo, Kind, PullState, RiffState, StartReason, WORKER_ALIVE_EVERY, Wake,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::daemon::sweep::{self, Fate, InJob};
use crate::daemon::{Daemon, Ended, Out, Record, Say, Start};
use crate::pr::Verdict;
use crate::room::Slot;
use crate::run::{self, Facts};
use crate::runtime::Runtime;
use crate::{identity, look, pr, repo_setup, rollout, session_image, text, usage};
use riff_client::api::{self, Api};

/// The file of the waves that the daemon runs, in its folder.
pub const WAVES_FILE: &str = "waves.json";

/// How often a wave looks for free work when nothing ends.
pub const LOOK_EVERY: Duration = Duration::from_secs(60);

/// The most starts of one item in one run of a wave.
pub const STARTS: u32 = 3;

/// The most starts of the rebase of one head commit in one run of a
/// wave (01M57FJ3WWMQSDBFRRHXN3SMFQ).
pub const REBASES: u32 = 1;

/// The most starts of the fix of the failed checks of one head commit in
/// one run of a wave (#279).
pub const CHECKS: u32 = 1;

/// A wave that the daemon runs: a repository, and where it is on this
/// machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wave {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// The main clone of the repository on this machine.
    pub main: PathBuf,
    /// The riff-server of the riff of the repository.
    pub server: String,
    /// The name of this host in the URI of a session.
    pub host: String,
}

impl Wave {
    /// The place of a session of the wave in the worktree `worktree`.
    ///
    /// ```
    /// use riff::wave::Wave;
    ///
    /// let wave = Wave {
    ///     repo: "o/r".into(),
    ///     main: "/src/r".into(),
    ///     server: "http://127.0.0.1:1".into(),
    ///     host: "thelio".into(),
    /// };
    /// assert_eq!(riff::identity::place_text(&wave.place(Some("issue-12"))?), "thelio/o/r#issue-12");
    /// assert_eq!(wave.thread()?.to_string(), "o/r");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn place(&self, worktree: Option<&str>) -> Result<Place> {
        let worktree = worktree.map(|w| format!("#{w}")).unwrap_or_default();
        identity::place_from_text(&format!("{}/{}{worktree}", self.host, self.repo))
    }

    /// The thread of the repository.
    pub fn thread(&self) -> Result<ThreadName> {
        self.place(None)?
            .default_thread()
            .ok_or_else(|| anyhow!("{}", text::RUN_NO_REPO))
    }
}

/// The waves in [`WAVES_FILE`] of the folder `dir` of the daemon, by
/// repository. With no file, none.
pub fn read_waves(dir: &Path) -> Result<BTreeMap<String, Wave>> {
    let path = dir.join(WAVES_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .with_context(|| format!("cannot read the waves in {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// Writes `waves` to [`WAVES_FILE`] in the folder `dir`, in place of
/// the old file.
pub fn write_waves(dir: &Path, waves: &BTreeMap<String, Wave>) -> Result<()> {
    let path = dir.join(WAVES_FILE);
    let tmp = path.with_extension("json.new");
    std::fs::write(&tmp, serde_json::to_vec_pretty(waves)?)
        .with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))
}

/// `riff wave run`: asks the daemon in the folder `dir` to run the
/// current wave of the repository of the place `here`, on the
/// riff-server `server` (01M4ZV3B6J89KJQ0F14YWTZMJQ).
pub async fn ask_run(dir: &Path, here: &Place, server: &str) -> Result<()> {
    let repo = ask(dir, here, server, |line| println!("{line}")).await?;
    println!("{}", text::wave_asked(&repo));
    Ok(())
}

/// [`ask_run`] with each line of the steps to `show`, for a caller with
/// no stdout of its own, as `riff mcp` (01M53NGVE839TRH7XB7CDPQNZE). It
/// gives the repository, as `OWNER/REPO`.
///
/// Before the ask of the wave, the daemon must have the session image
/// that the repository pins: podman runs, and the image is on the
/// machine, or the daemon pulls it ([`crate::daemon::image`]). When it
/// cannot, the daemon gets no ask of the wave: a machine that cannot
/// start a container runs no wave (01M558EYAJFZK7Y822NVWGBS9H). The git
/// steps of the pin block, so they run on a thread that may block: they
/// stop no task of `riff mcp`.
pub async fn ask(dir: &Path, here: &Place, server: &str, show: impl FnMut(&str)) -> Result<String> {
    ask_with(dir, identity::working_dir()?, here, server, show).await
}

/// [`ask`] for the clone of the folder `work`: the daemon of a host
/// asks it for the lead of another host, in its main clone
/// ([`crate::daemon::remote`]).
pub async fn ask_with(
    dir: &Path,
    work: PathBuf,
    here: &Place,
    server: &str,
    mut show: impl FnMut(&str),
) -> Result<String> {
    if here.default_thread().is_none() {
        bail!("{}", text::RUN_NO_REPO);
    }
    let (main, image, lines) = blocking(move || {
        let (main, _) = run::main_clone(&work)?;
        let mut lines = Vec::new();
        let image = session_image::merged(&main, |line| lines.push(line.to_owned()))?;
        Ok((main, image.to_string(), lines))
    })
    .await?;
    for line in &lines {
        show(line);
    }
    crate::daemon::image(dir, &image, &mut show)
        .await
        .map_err(|e| anyhow!("{}", text::wave_no_container(&format!("{e:#}"))))?;
    let repo = here.repo_text();
    let wave = Wave {
        repo: repo.clone(),
        main,
        server: server.to_owned(),
        host: here.host().to_owned(),
    };
    crate::daemon::ask_done(dir, crate::daemon::Ask::Wave { wave }, show).await?;
    Ok(repo)
}

/// True when the daemon of `status` runs the wave of `repo`
/// (`OWNER/REPO`).
///
/// ```
/// let status: riff::daemon::Status = serde_json::from_value(serde_json::json!({
///     "pid": 7, "build": "3.0.0", "since_ms": 0, "dir": "/d", "sessions": [],
///     "waves": [{"repo": "o/r", "word": "no work"}],
/// }))?;
/// assert!(riff::wave::runs(&status, "o/r"));
/// assert!(!riff::wave::runs(&status, "o/other"));
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn runs(status: &crate::daemon::Status, repo: &str) -> bool {
    status.waves.iter().any(|w| w.repo == repo)
}

/// `riff wave stop`: asks the daemon in the folder `dir` to start no new
/// session for the wave of the repository of the place `here`.
pub async fn ask_stop(dir: &Path, here: &Place) -> Result<()> {
    if here.default_thread().is_none() {
        bail!("{}", text::RUN_NO_REPO);
    }
    let repo = here.repo_text();
    let ask = crate::daemon::Ask::WaveStop { repo: repo.clone() };
    crate::daemon::ask_done(dir, ask, |line| println!("{line}")).await?;
    println!("{}", text::wave_stops(&repo));
    Ok(())
}

/// What a session of a wave does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Work {
    /// The build of the issue on its branch.
    Build {
        /// The issue.
        issue: u64,
    },
    /// The verify of the pull request `pull` of the issue, at the commit
    /// `commit`.
    Verify {
        /// The issue.
        issue: u64,
        /// The pull request.
        pull: u64,
        /// The head commit that the verify tests.
        commit: String,
    },
    /// A build that rebases the pull request `pull` of the issue on
    /// `main`: its head `commit` has a conflict with `main`
    /// (01M57FJ3WWMQSDBFRRHXN3SMFQ).
    Rebase {
        /// The issue.
        issue: u64,
        /// The pull request.
        pull: u64,
        /// The head commit with the conflict.
        commit: String,
    },
    /// A build that fixes the failed checks of the pull request `pull`
    /// of the issue: its verify passed, and the checks `checks` of its
    /// head `commit` failed (#279).
    Checks {
        /// The issue.
        issue: u64,
        /// The pull request.
        pull: u64,
        /// The head commit with the failed checks.
        commit: String,
        /// The names of the checks that failed.
        checks: Vec<String>,
    },
}

impl Work {
    /// The work item: `issue-N` or `verify-issue-N`. A rebase and a fix
    /// of checks are builds: their item is `issue-N`.
    ///
    /// ```
    /// use riff::wave::Work;
    ///
    /// assert_eq!(Work::Build { issue: 12 }.item(), "issue-12");
    /// let verify = Work::Verify { issue: 12, pull: 40, commit: "1a2b".into() };
    /// assert_eq!(verify.item(), "verify-issue-12");
    /// let rebase = Work::Rebase { issue: 12, pull: 40, commit: "1a2b".into() };
    /// assert_eq!(rebase.item(), "issue-12");
    /// let checks = Work::Checks { issue: 12, pull: 40, commit: "1a2b".into(), checks: vec!["Gate".into()] };
    /// assert_eq!(checks.item(), "issue-12");
    /// ```
    pub fn item(&self) -> String {
        match self {
            Work::Build { issue } | Work::Rebase { issue, .. } | Work::Checks { issue, .. } => {
                format!("issue-{issue}")
            }
            Work::Verify { issue, .. } => format!("verify-issue-{issue}"),
        }
    }

    /// The issue.
    pub fn issue(&self) -> u64 {
        match self {
            Work::Build { issue }
            | Work::Verify { issue, .. }
            | Work::Rebase { issue, .. }
            | Work::Checks { issue, .. } => *issue,
        }
    }

    /// The name of the starts of this work in a run of the wave: its
    /// item. A rebase counts for its head commit, so that each conflict
    /// starts one session (01M57FJ3WWMQSDBFRRHXN3SMFQ). The fix of checks
    /// counts for its head commit too, apart from the rebase of the same
    /// head (#279).
    ///
    /// ```
    /// use riff::wave::Work;
    ///
    /// assert_eq!(Work::Build { issue: 12 }.key(), "issue-12");
    /// let rebase = |commit: &str| Work::Rebase { issue: 12, pull: 40, commit: commit.into() };
    /// assert_eq!(rebase("1a2b").key(), "issue-12@1a2b");
    /// assert_ne!(rebase("1a2b").key(), rebase("5e6f").key());
    /// let checks = Work::Checks { issue: 12, pull: 40, commit: "1a2b".into(), checks: vec![] };
    /// assert_eq!(checks.key(), "issue-12@1a2b:checks");
    /// assert_ne!(checks.key(), rebase("1a2b").key());
    /// ```
    pub fn key(&self) -> String {
        match self {
            Work::Rebase { commit, .. } => format!("{}@{commit}", self.item()),
            Work::Checks { commit, .. } => format!("{}@{commit}:checks", self.item()),
            _ => self.item(),
        }
    }

    /// The most starts of this work in a run of the wave: [`STARTS`],
    /// [`REBASES`] for a rebase, or [`CHECKS`] for a fix of checks.
    pub fn most(&self) -> u32 {
        match self {
            Work::Rebase { .. } => REBASES,
            Work::Checks { .. } => CHECKS,
            _ => STARTS,
        }
    }
}

/// The job of a session that the daemon started for a wave. It is in the
/// [`Record`] of the session, with no secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    /// The wave.
    pub wave: Wave,
    /// The work.
    pub work: Work,
}

/// A pull request that waits for a verify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// The pull request.
    pub pull: u64,
    /// Its head commit.
    pub commit: String,
    /// True when the full check of the head is done: passed, or ended.
    pub gate_done: bool,
}

/// A pull request with auto-merge on and a conflict with `main`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The pull request.
    pub pull: u64,
    /// Its head commit, which has the conflict.
    pub commit: String,
}

/// A pull request of an item whose verify passed and whose checks
/// failed (#279).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    /// The pull request.
    pub pull: u64,
    /// Its head commit, whose checks failed.
    pub commit: String,
    /// The names of the checks that failed.
    pub checks: Vec<String>,
}

/// A pull request that no session can fix: the lead gets one message
/// for it (#279).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// The pull request.
    pub pull: u64,
    /// Its branch.
    pub branch: String,
    /// Its head commit.
    pub commit: String,
    /// Why it cannot merge.
    pub stuck: rollout::Stuck,
}

impl Notice {
    /// The key of the message: the pull request, its head and its
    /// reason. A message with the same key goes one time.
    ///
    /// ```
    /// use riff::rollout::Stuck;
    /// use riff::wave::Notice;
    ///
    /// let notice = |commit: &str, stuck| Notice { pull: 40, branch: "b".into(), commit: commit.into(), stuck };
    /// assert_eq!(notice("1a2b", Stuck::AutoMergeOff).key(), "#40@1a2b:auto-merge off");
    /// assert_ne!(notice("1a2b", Stuck::AutoMergeOff).key(), notice("5e6f", Stuck::AutoMergeOff).key());
    /// ```
    pub fn key(&self) -> String {
        format!("#{}@{}:{}", self.pull, self.commit, self.stuck.word())
    }
}

/// The pull requests that cannot merge by themselves, by what the wave
/// does for them (#279).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fixes {
    /// The pull requests to rebase, by issue.
    pub conflicts: BTreeMap<u64, Conflict>,
    /// The pull requests whose checks failed after a pass, by issue:
    /// a build for each.
    pub failed: BTreeMap<u64, Failed>,
    /// The pull requests that the lead hears of.
    pub notices: Vec<Notice>,
}

/// The facts of a wave at one look.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Look {
    /// True when the riff or the repository is paused.
    pub paused: bool,
    /// The free work, in order ([`fixes`]): `verify-issue-N` and
    /// `issue-N`. An `issue-N` in `fixes.conflicts` is a rebase, and one
    /// in `fixes.failed` the fix of its checks.
    pub free: Vec<String>,
    /// The items of the repository that a session claims.
    pub claims: BTreeSet<String>,
    /// The held items of the repository.
    pub holds: BTreeSet<String>,
    /// The pull requests that wait for a verify, by issue.
    pub pulls: BTreeMap<u64, Waiting>,
    /// The pull requests that cannot merge by themselves.
    pub fixes: Fixes,
    /// The issues that would be free work but for the missing line
    /// `Done when:` in their body
    /// ([`rollout::Open::no_done_when`]). The lead hears of each once.
    pub no_done_when: BTreeSet<u64>,
    /// The default branch of the repository on the forge, for example
    /// `main` or `master`. The look reads it at each look, and keeps it
    /// in no file (01M5R56GN3DC09GAPAF69GENEZ).
    pub branch: String,
}

/// The issue of `verify-issue-N`.
fn verify_issue(item: &str) -> Option<u64> {
    item.strip_prefix("verify-issue-")?.parse().ok()
}

/// The next work of a wave, from the facts of `look`, the items that its
/// jobs run (`running`), and the number of starts of each item
/// (`starts`). See "The pick" in the design.
///
/// ```
/// use std::collections::{BTreeMap, BTreeSet};
/// use riff::wave::{Conflict, Failed, Fixes, Look, Waiting, Work, pick};
///
/// let look = Look {
///     free: vec!["verify-issue-9".into(), "issue-12".into(), "issue-13".into()],
///     holds: ["issue-12".to_owned()].into(),
///     claims: ["issue-9".to_owned()].into(),
///     pulls: [(9, Waiting { pull: 40, commit: "1a2b".into(), gate_done: true })].into(),
///     ..Look::default()
/// };
/// let none = BTreeSet::new();
/// // The author of 9 still holds it, and 12 is held.
/// assert_eq!(pick(&look, &none, &BTreeMap::new()), Some(Work::Build { issue: 13 }));
/// let look = Look { claims: BTreeSet::new(), ..look };
/// assert_eq!(
///     pick(&look, &none, &BTreeMap::new()),
///     Some(Work::Verify { issue: 9, pull: 40, commit: "1a2b".into() })
/// );
/// assert_eq!(pick(&Look { paused: true, ..look }, &none, &BTreeMap::new()), None);
///
/// // The pull request of 14 has a conflict: one rebase for its head.
/// let conflict = Conflict { pull: 54, commit: "5e6f".into() };
/// let look = Look { free: vec!["issue-14".into()], fixes: Fixes { conflicts: [(14, conflict)].into(), ..Fixes::default() }, ..Look::default() };
/// let rebase = Work::Rebase { issue: 14, pull: 54, commit: "5e6f".into() };
/// assert_eq!(pick(&look, &none, &BTreeMap::new()), Some(rebase.clone()));
/// assert_eq!(pick(&look, &none, &[(rebase.key(), 1)].into()), None);
/// // The starts of the build do not stop the rebase.
/// assert_eq!(pick(&look, &none, &[("issue-14".to_owned(), 3)].into()), Some(rebase));
///
/// // The checks of the pull request of 15 failed after a pass: one fix for its head.
/// let failed = Failed { pull: 55, commit: "7a8b".into(), checks: vec!["Gate".into()] };
/// let look = Look { free: vec!["issue-15".into()], fixes: Fixes { failed: [(15, failed)].into(), ..Fixes::default() }, ..Look::default() };
/// let fix = Work::Checks { issue: 15, pull: 55, commit: "7a8b".into(), checks: vec!["Gate".into()] };
/// assert_eq!(pick(&look, &none, &BTreeMap::new()), Some(fix.clone()));
/// assert_eq!(pick(&look, &none, &[(fix.key(), 1)].into()), None);
/// ```
pub fn pick(
    look: &Look,
    running: &BTreeSet<String>,
    starts: &BTreeMap<String, u32>,
) -> Option<Work> {
    if look.paused {
        return None;
    }
    let busy = |item: &str| running.contains(item) || look.claims.contains(item);
    look.free.iter().find_map(|item| {
        if look.holds.contains(item) || busy(item) {
            return None;
        }
        let work = if let Some(issue) = verify_issue(item) {
            // The author of the item still works on it.
            if busy(&format!("issue-{issue}")) {
                return None;
            }
            let waiting = look.pulls.get(&issue).filter(|w| w.gate_done)?;
            Work::Verify {
                issue,
                pull: waiting.pull,
                commit: waiting.commit.clone(),
            }
        } else {
            let issue = item.strip_prefix("issue-")?.parse().ok()?;
            match (
                look.fixes.conflicts.get(&issue),
                look.fixes.failed.get(&issue),
            ) {
                (Some(conflict), _) => Work::Rebase {
                    issue,
                    pull: conflict.pull,
                    commit: conflict.commit.clone(),
                },
                (None, Some(failed)) => Work::Checks {
                    issue,
                    pull: failed.pull,
                    commit: failed.commit.clone(),
                    checks: failed.checks.clone(),
                },
                (None, None) => Work::Build { issue },
            }
        };
        (starts.get(&work.key()).copied().unwrap_or(0) < work.most()).then_some(work)
    })
}

/// True when the item `issue-N` of the free work of `look` is a fix of
/// a pull request: a rebase, or the fix of its checks.
fn fix_of(look: &Look, item: &str) -> bool {
    item.strip_prefix("issue-")
        .and_then(|n| n.parse().ok())
        .is_some_and(|n| {
            look.fixes.conflicts.contains_key(&n) || look.fixes.failed.contains_key(&n)
        })
}

/// The items of the free work of `look` that reached [`STARTS`]. A fix
/// of a pull request is not one: it starts one time for each head.
fn spent<'a>(
    look: &'a Look,
    starts: &'a BTreeMap<String, u32>,
) -> impl Iterator<Item = &'a String> {
    look.free
        .iter()
        .filter(|item| !fix_of(look, item))
        .filter(|item| starts.get(*item).copied().unwrap_or(0) >= STARTS)
}

/// A future of a [`World`].
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The wait before the next connection of the wakes of a session.
const WAKE_RETRY: Duration = Duration::from_secs(1);

/// The forge and the riff of the waves: [`Live`], or a fake in a test.
pub trait World: Send + Sync {
    /// The facts of the wave now.
    fn look<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<Look>>;

    /// The session image of the repository of the wave: the merged pin
    /// of its main clone ([`session_image::merged`]).
    fn image<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<String>>;

    /// The facts of the host that no item changes (as `riff run` checks them):
    /// the fault of the first that fails (01M57A7A1W2K9440WZA1QZ3M9T).
    fn host<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<(), run::HostFault>>;

    /// The steps before the start: the checks, the claim, the clone and
    /// the start text. `None` when another session holds the item, or
    /// the item is held. Each other refusal of the claim, for example
    /// `paused`, is the error ([`api::Refusal`]), and the new session
    /// ends. After the claim, an error releases the item.
    fn begin<'a>(
        &'a self,
        wave: &'a Wave,
        work: &'a Work,
        image: &'a str,
    ) -> BoxFuture<'a, Result<Option<Start>>>;

    /// Goes on with the claim of the session of `record` after a new
    /// start of the daemon: its keep-alive.
    fn resume(&self, record: &Record);

    /// The steps after the end of the session of `record`. It gives a
    /// line for each step.
    fn finish<'a>(&'a self, record: &'a Record, ended: Ended)
    -> BoxFuture<'a, Result<Vec<String>>>;

    /// The marks of this machine and the `gh` of the repository `repo`:
    /// the daemon reports the tokens of each container session with them
    /// ([`usage::Meter::session_ended`]). `None` counts nothing.
    fn meter(&self, _repo: &str) -> Option<usage::Meter> {
        None
    }

    /// The folder of the leave marks of this machine
    /// ([`crate::local::marks`]). The daemon posts no refusal of auto
    /// mode of a session with a mark in it
    /// ([`crate::daemon::Daemon::post_denials`]). `None` has no mark.
    fn marks(&self) -> Option<PathBuf> {
        None
    }

    /// The fate of the folder of a session of the issue `issue` of the
    /// repository `repo`, from the forge ([`Fate::of`]).
    fn fate<'a>(&'a self, repo: &'a str, issue: u64) -> BoxFuture<'a, Result<Fate>>;

    /// Deletes each branch `riff-kept/*` of `repo` that is older than 30
    /// days, and gives their names ([`crate::daemon::sweep::delete_old_kept_on`]).
    fn delete_old_kept<'a>(&'a self, _repo: &'a str) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    /// Adds the comment `Merged in #PULL (COMMIT)` to the issue `issue` of
    /// `repo`, when it has none ([`crate::daemon::sweep`]).
    fn merged_in<'a>(
        &'a self,
        repo: &'a str,
        issue: u64,
        pull: u64,
        commit: &'a str,
    ) -> BoxFuture<'a, Result<()>>;

    /// Tells the lead of the wave `body`, as a message.
    fn tell<'a>(&'a self, wave: &'a Wave, body: &'a str) -> BoxFuture<'a, ()>;

    /// The `gh` of the repository `repo` for the look
    /// ([`crate::look::look_wave`]).
    fn gh(&self, repo: &str) -> Arc<pr::Gh>;

    /// The items that a session of the repository of `wave` claims now.
    fn claims<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<HashSet<String>>>;

    /// Posts `body` of `kind` as the person of the wave: to the session
    /// that claims `item`, or to the lead of the person when `item` is
    /// `None`. The error is a post that did not go out.
    fn post<'a>(
        &'a self,
        wave: &'a Wave,
        item: Option<&'a str>,
        body: &'a str,
        kind: Kind,
    ) -> BoxFuture<'a, Result<()>>;

    /// Writes `body` in riff chat as the person of the lead of `record`,
    /// and wakes no session: the lead cannot act on it
    /// (01M5NJ87YEHHGJBANM7CRYV56Z).
    fn chat<'a>(&'a self, record: &'a Record, body: &'a str) -> BoxFuture<'a, ()>;

    /// Ends the session of `record` on riff-server, and stops its
    /// keep-alive: the end of the lead.
    fn leave<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, ()>;

    /// Frees the item of the session of `record` on riff-server at once,
    /// ends that session, and stops its keep-alive: a person ends the
    /// session ([`crate::daemon::Ask::End`], 01M579Z9VRD12FRK6DSYVKDHQB).
    fn free<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, Result<()>>;

    /// Waits for the next wake of the session of `record` on riff-server,
    /// and gives its line, as `riff watch --once` prints it.
    fn wake<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, Result<String>>;

    /// Sends each wake of the session of `record` on riff-server to `to`,
    /// until `to` closes: the wakes of a worker and of a verifier
    /// ([`crate::daemon`], 01M5792DP1KZR2XGSCMRWDHCGZ).
    fn wakes<'a>(
        &'a self,
        record: &'a Record,
        to: mpsc::UnboundedSender<Wake>,
    ) -> BoxFuture<'a, ()>;

    /// Tells the sender of `wake` that the session of `record` ended its
    /// work and does not read the message (01M5792DRA12JD032F3H8JVC0Z).
    fn refuse<'a>(&'a self, record: &'a Record, wake: &'a Wake) -> BoxFuture<'a, ()>;
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The items that the daemon runs for the wave `wave`: its jobs before
/// the start (`taken`), and its live sessions.
fn running<R: Runtime + 'static>(
    daemon: &Daemon<R>,
    wave: &Wave,
    taken: &Mutex<BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut items = lock(taken).clone();
    items.extend(
        daemon
            .records()
            .into_iter()
            .filter(|r| r.job.as_ref().is_some_and(|j| j.wave.repo == wave.repo))
            .map(|r| r.live.item),
    );
    items
}

/// The loop of the wave `wave` in `daemon`, until the daemon stops it.
/// `taken` holds the items of its jobs before the start, `starts` the
/// starts of each item, and `word` what it did last.
pub(crate) async fn run<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    wave: Wave,
    taken: Arc<Mutex<BTreeSet<String>>>,
    starts: Starts,
    word: Arc<Mutex<String>>,
) {
    let say = |text: String| *lock(&word) = text;
    // The spent items that the lead heard of.
    let mut told: BTreeSet<String> = BTreeSet::new();
    // The stuck pull requests that the lead heard of ([`Notice::key`]).
    let mut noticed: BTreeSet<String> = BTreeSet::new();
    // The issues with no `Done when:` line that the lead heard of.
    let mut bare: BTreeSet<u64> = BTreeSet::new();
    // True while the wave waits for a fact of the host: the lead heard
    // of it.
    let mut waits = false;
    loop {
        let slot = daemon
            .room()
            .admit(
                &text::wave_slot(&wave.repo),
                || daemon.live_count(),
                |full| say(text::wave_full(full)),
            )
            .await;
        let world = daemon.world();
        for line in daemon.sweep_news(Some(&wave.repo)).await {
            println!("{line}");
        }
        let next = match world.look(&wave).await {
            Ok(look) => {
                let spent: Vec<String> = spent(&look, &lock(&starts)).cloned().collect();
                // `riff wave run` cleared the starts of these items.
                told.retain(|item| lock(&starts).get(item).is_some_and(|n| *n >= STARTS));
                for item in spent {
                    if told.insert(item.clone()) {
                        let reason = daemon
                            .last_end(&item)
                            .unwrap_or_else(|| text::WAVE_END_UNKNOWN.to_owned());
                        let body = text::wave_spent(&item, STARTS, &reason);
                        world.tell(&wave, &body).await;
                    }
                }
                // An issue that got the line, or left the free work, can come
                // again.
                bare.retain(|n| look.no_done_when.contains(n));
                for n in &look.no_done_when {
                    if bare.insert(*n) {
                        world.tell(&wave, &text::wave_no_done_when(*n)).await;
                    }
                }
                for notice in &look.fixes.notices {
                    if noticed.insert(notice.key()) {
                        let body = text::wave_stuck(
                            notice.pull,
                            &notice.branch,
                            &look.branch,
                            &notice.commit,
                            &notice.stuck,
                        );
                        world.tell(&wave, &body).await;
                    }
                }
                let next = pick(&look, &running(&daemon, &wave, &taken), &lock(&starts));
                // A fault of the host stops each item: the wave waits,
                // and no start counts (01M57A7A1W2K9440WZA1QZ3M9T).
                let fault = match &next {
                    Some(_) => world.host(&wave).await.err(),
                    None => None,
                };
                match &fault {
                    Some(fault) if !waits => {
                        waits = true;
                        let body = text::wave_host_waits(&wave.repo, &fault.wave_text());
                        world.tell(&wave, &body).await;
                    }
                    Some(_) => {}
                    None if next.is_some() => waits = false,
                    None => {}
                }
                say(match (&next, &fault) {
                    _ if look.paused => text::WAVE_PAUSED.into(),
                    (Some(_), Some(fault)) => text::wave_host_word(&fault.wave_text()),
                    (Some(work), None) => text::wave_started(&work.item()),
                    (None, _) => text::WAVE_NO_WORK.into(),
                });
                next.filter(|_| fault.is_none())
            }
            Err(e) => {
                say(text::wave_no_look(&format!("{e:#}")));
                None
            }
        };
        match next {
            Some(work) => {
                *lock(&starts).entry(work.key()).or_default() += 1;
                lock(&taken).insert(work.item());
                let (taken, starts) = (taken.clone(), starts.clone());
                let (daemon, wave) = (daemon.clone(), wave.clone());
                tokio::spawn(job(daemon, wave, work, slot, taken, starts, false));
            }
            None => {
                drop(slot);
                tokio::select! {
                    () = tokio::time::sleep(LOOK_EVERY) => {}
                    () = daemon.changed() => {}
                }
            }
        }
    }
}

/// The look of the wave `wave` in `daemon`, until the daemon stops it
/// (01M5QVH3MAEPPJN9PPVDX6N2EY): [`crate::look::look_wave`] at once, then
/// each [`Daemon::look_every`]. It takes no room: a host that is full
/// still sees a broken `main`. A look that fails prints its fault one
/// time until the fault changes.
pub(crate) async fn watch<R: Runtime + 'static>(daemon: Arc<Daemon<R>>, wave: Wave) {
    let mut watch = look::Watch::default();
    let mut last_error = None;
    loop {
        let world = daemon.world();
        match look::look_wave(&*world, &wave, &mut watch).await {
            Ok(()) => last_error = None,
            Err(e) => {
                let e = format!("{e:#}");
                if last_error.as_ref() != Some(&e) {
                    eprintln!("riff daemon: {}: {}", wave.repo, text::wave_no_look(&e));
                }
                last_error = Some(e);
            }
        }
        tokio::time::sleep(daemon.look_every()).await;
    }
}

/// The starts of each item in one run of a wave, shared by its loop and
/// its jobs. `riff wave run` on a wave that runs clears them
/// (01M57A7A48A70Y7PTD95A3MBYC).
pub(crate) type Starts = Arc<Mutex<BTreeMap<String, u32>>>;

/// True when `error` is a refusal of riff-server for a pause: the riff
/// or the repository is paused.
fn paused(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<api::Refusal>()
        .is_some_and(|refusal| refusal.code.as_deref() == Some("paused"))
}

/// Takes back one start of `work` in `starts`.
fn take_back(starts: &Starts, work: &Work) {
    if let Some(count) = lock(starts).get_mut(&work.key()) {
        *count = count.saturating_sub(1);
    }
}

/// One job of the wave `wave`: the work `work` in the room of `slot`. A
/// claim that riff-server refused for a pause takes back its start in
/// `starts`, and tells the lead nothing (01M558EYCXPFW8CJV3XJ7HK3NR). So
/// does a fault of the host that came after the look of the loop
/// (01M57A7A1W2K9440WZA1QZ3M9T): the next look waits for it.
///
/// A job that the lead asked for (`asked`: `riff run` in the container
/// of the lead, #1338) has no next look. So each case that stops its
/// start tells the lead why.
pub(crate) async fn job<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    wave: Wave,
    work: Work,
    slot: Slot,
    taken: Arc<Mutex<BTreeSet<String>>>,
    starts: Starts,
    asked: bool,
) {
    let item = work.item();
    let held = daemon.in_job(&item);
    let no_start = match start_job(&daemon, &wave, &work, slot).await {
        Ok(Some((record, ended))) => {
            finish(daemon.clone(), record, ended, held).await;
            None
        }
        Ok(None) => {
            println!("{}", text::wave_not_claimed(&wave.repo, &item));
            Some(text::RUN_NOT_CLAIMED.to_owned())
        }
        Err(e) if paused(&e) => {
            println!("{}", text::wave_claim_paused(&wave.repo, &item));
            take_back(&starts, &work);
            Some(text::RUN_PAUSED.to_owned())
        }
        Err(ref e) if let Some(fault) = e.downcast_ref::<run::HostFault>() => {
            let why = fault.wave_text();
            println!("{}", text::wave_host_fault(&wave.repo, &item, &why));
            take_back(&starts, &work);
            Some(why)
        }
        Err(e) => {
            let error = format!("{e:#}");
            eprintln!("riff daemon: {}: {item}: {error}", wave.repo);
            daemon.note_end(&item, text::wave_no_start(&error));
            if !asked {
                let body = text::wave_job_failed(&item, &error);
                daemon.world().tell(&wave, &body).await;
            }
            Some(error)
        }
    };
    if let (true, Some(why)) = (asked, no_start) {
        daemon
            .world()
            .tell(&wave, &text::run_job_failed(&item, &why))
            .await;
    }
    lock(&taken).remove(&item);
    daemon.tell_changed();
}

/// The steps of a job up to the end of its session: the image that the
/// wave pins, the steps before the start, and the session. `None` when
/// the claim failed. Hold [`Daemon::in_job`] of the item from before the
/// call to the end of [`finish`]: the sweep then keeps the folder.
async fn start_job<R: Runtime + 'static>(
    daemon: &Arc<Daemon<R>>,
    wave: &Wave,
    work: &Work,
    slot: Slot,
) -> Result<Option<(Record, Ended)>> {
    let world = daemon.world();
    let image = world.image(wave).await?;
    daemon.have_image(&image, |line| println!("{line}")).await?;
    let Some(start) = world.begin(wave, work, &image).await? else {
        return Ok(None);
    };
    let job = Job {
        wave: wave.clone(),
        work: work.clone(),
    };
    let (out, said) = Out::keep();
    let ended = tokio::spawn(ended_of(said));
    let record = daemon
        .launch(start, Some(job), Some(slot), None, &out)
        .await;
    drop(out);
    let ended = ended.await.unwrap_or(Ended {
        code: None,
        failed: true,
    });
    Ok(Some((record, ended)))
}

/// The end of a session from the lines that the daemon says to one of
/// its followers. A failed start, or no end, is a failed session.
async fn ended_of(mut said: mpsc::UnboundedReceiver<Say>) -> Ended {
    let failed = Ended {
        code: None,
        failed: true,
    };
    while let Some(say) = said.recv().await {
        match say {
            Say::Ended(ended) => return ended,
            Say::Failed { error } => {
                eprintln!("riff daemon: {error}");
                return failed;
            }
            _ => {}
        }
    }
    failed
}

/// Waits for the end of the session of `record` that a new daemon took
/// up, on the lines `said`, and does the steps after the end, in the job
/// `held` of its item.
pub(crate) async fn after<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    record: Record,
    said: mpsc::UnboundedReceiver<Say>,
    held: InJob,
) {
    let ended = ended_of(said).await;
    finish(daemon, record, ended, held).await;
}

/// The steps after the end of the session of `record`, as it `ended`.
/// It gives the line of each step. The job `held` of the item ends after
/// the last step: a step reads the folder of the session, so the sweep
/// keeps it to then (01M53S7WDPQ2GWHJWJ6SZXZ5PN).
pub(crate) async fn finish<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    record: Record,
    ended: Ended,
    held: InJob,
) -> Vec<String> {
    // The reason of the end, for the lead that reads "started N times".
    let end = crate::ends::End::of(&record, ended, &crate::run::Scrub::new([]));
    daemon.note_end(&record.live.item, end.reason());
    let lines = daemon.world().finish(&record, ended).await;
    drop(held);
    daemon.tell_changed();
    lines.unwrap_or_else(|e| {
        let error = format!("{e:#}");
        eprintln!("riff daemon: {}: {error}", record.live.name);
        vec![error]
    })
}

/// The text of the last result of `claude` in the file `events` of a
/// session: the `result` of its last turn.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let events = dir.path().join("events.jsonl");
/// std::fs::write(&events, concat!(
///     r#"{"type":"result","result":"first"}"#, "\n",
///     r#"{"type":"assistant"}"#, "\n",
///     r#"{"type":"result","result":"PASS\n\nAll good."}"#, "\n",
/// ))?;
/// assert_eq!(riff::wave::last_result(&events).as_deref(), Some("PASS\n\nAll good."));
/// assert_eq!(riff::wave::last_result(&dir.path().join("none")), None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn last_result(events: &Path) -> Option<String> {
    let text = std::fs::read_to_string(events).ok()?;
    text.lines().rev().find_map(|line| {
        let event: serde_json::Value = serde_json::from_str(line).ok()?;
        (event["type"] == "result")
            .then(|| event["result"].as_str().map(str::to_owned))
            .flatten()
    })
}

/// The verdict of the result of a verify session, and the result after
/// its verdict line. The verdict is the first line that holds only
/// `PASS` or `FAIL`, also with marks of markdown around it.
///
/// ```
/// use riff::pr::Verdict;
/// use riff::wave::verdict;
///
/// assert_eq!(verdict("PASS\n\n- Done.\n"), Some((Verdict::Pass, "- Done.".into())));
/// assert_eq!(verdict("I checked it.\n\n**FAIL**\n1. fails\n"), Some((Verdict::Fail, "1. fails".into())));
/// assert_eq!(verdict("## PASS"), Some((Verdict::Pass, String::new())));
/// assert_eq!(verdict("It may PASS."), None);
/// ```
pub fn verdict(result: &str) -> Option<(Verdict, String)> {
    let mut lines = result.lines();
    let verdict = lines.by_ref().find_map(|line| {
        match line.trim_matches(|c: char| c.is_whitespace() || "#*_`:".contains(c)) {
            "PASS" => Some(Verdict::Pass),
            "FAIL" => Some(Verdict::Fail),
            _ => None,
        }
    })?;
    let rest: Vec<&str> = lines.collect();
    Some((verdict, rest.join("\n").trim().to_owned()))
}

/// The forge and the riff of the waves on this machine: `gh` with the
/// sign-in of the person, riff-server with the sign-in of riff, and
/// git.
#[derive(Default)]
pub struct Live {
    /// The keep-alive of the claim of each session, by session ID.
    alive: Mutex<HashMap<String, JoinHandle<()>>>,
    /// The last text that the lead of each wave got: a text goes one
    /// time in a row.
    told: Mutex<HashMap<String, String>>,
    /// The `gh` of the look of each repository: its calls share one token
    /// (01M54510SFK1Z9VPFYBYYSV10F).
    ghs: Mutex<HashMap<String, Arc<pr::Gh>>>,
}

/// Posts `body` of `kind` as the person of `wave`: to the session that
/// claims `item`, or to the lead of the person.
pub(crate) async fn post_as_person(
    wave: &Wave,
    item: Option<&str>,
    body: &str,
    kind: Kind,
) -> Result<()> {
    let person = identity::person(&wave.place(None)?, &wave.server)?;
    let api = Api::new(&wave.server).signed_in(None)?;
    let to = match item {
        Some(item) => format!("claim={item},repo={}", wave.repo).parse()?,
        None => riff_core::selector::Selector::lead(person.who().user(), &wave.repo),
    };
    api.post(&person, Some(&wave.thread()?), &[to], body, kind)
        .await?;
    Ok(())
}

/// Runs `work` on a thread that may block: `gh` and git.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| anyhow!("{e}"))?
}

/// The branch of the issue `issue`.
fn branch_of(issue: u64) -> String {
    run::branch(&format!("issue-{issue}"))
}

/// Saves the work of the clone of `record` when its build pushed nothing:
/// a WIP commit on the branch of `issue`, pushed ([`crate::leave::save`],
/// 01M5P26VSA7AA99K1RYG83R8J9). It gives the commit. A fault shows on
/// stderr.
async fn save_work(record: &Record, issue: u64) -> Option<String> {
    let (clone, branch) = (record.folder.join("clone"), branch_of(issue));
    match blocking(move || crate::leave::save(&clone, &branch)).await {
        Ok(saved) => saved.map(|(_, commit)| commit),
        Err(e) => {
            eprintln!("riff daemon: {}: {e:#}", record.live.name);
            None
        }
    }
}

/// What a build fixed on a pull request. When the pull request still
/// has the head commit, the session pushed nothing.
#[derive(Debug, Clone, Copy)]
enum Fix<'a> {
    /// A rebase of the head commit that had the conflict.
    Rebase(&'a str),
    /// The fix of the checks `names` that failed at the head commit.
    Checks(&'a str, &'a [String]),
}

impl Fix<'_> {
    /// The head commit that the build started from.
    fn commit(&self) -> &str {
        match self {
            Fix::Rebase(commit) | Fix::Checks(commit, _) => commit,
        }
    }
}

/// The names of the checks of pull request `number` that failed now,
/// with `gh`.
async fn still_failed(gh: pr::Gh, number: u64) -> Result<Vec<String>> {
    blocking(move || {
        let n = number.to_string();
        let pull: rollout::Pull = gh.json(&["pr", "view", &n, "--json", rollout::PULL_FIELDS])?;
        Ok(pull.failed_checks())
    })
    .await
}

/// The steps after the end of a build, with `gh` for the forge: the
/// verify request for the open pull request of its branch, or a note
/// that it has none. When the issue has the label
/// [`rollout::NO_VERIFY`], or a lead holds `verify-ITEM`, the lead gets
/// a note with the reason in place of the verify request, as `riff run`
/// does ([`run::no_verify`], 01M51KH0H7J35PEKMHP5KQB84T).
///
/// The pair is `(fix, saved)`: the commit of `saved` is the WIP commit
/// that riff pushed for a build with no push.
///
/// After a rebase, `fix` is the head commit that had the conflict. When
/// the pull request still has that head, the session pushed nothing: the
/// lead gets a message in place of the verify request
/// (01M57FJ3Z8AFDY9G0KTAYZJESY).
///
/// After the fix of failed checks, `fix` is the head commit whose checks
/// failed. A session that pushed nothing may have run the job again.
/// So riff reads the checks of the pull request: when one still failed,
/// the lead gets a message, and else a note (#279).
async fn after_build(
    gh: pr::Gh,
    api: &Api,
    record: &Record,
    wave: &Wave,
    issue: u64,
    (fix, saved): (Option<Fix<'_>>, Option<&str>),
    ended: Ended,
) -> Result<Vec<String>> {
    let branch = branch_of(issue);
    let (view, at) = (gh.clone(), branch.clone());
    let pull = blocking(move || {
        Ok(view
            .json::<run::Pull>(&["pr", "view", &at, "--json", "number,url,headRefOid,state"])
            .ok()
            .filter(|p| p.state == "OPEN"))
    })
    .await?;
    let me = &record.me;
    let to = [riff_core::selector::Selector::lead(
        me.who().user(),
        &wave.repo,
    )];
    let thread = wave.thread()?;
    let item = &record.live.item;
    let session = run::short(&record.live.session);
    let mut lines = Vec::new();
    let same = |pull: &run::Pull| fix.is_some_and(|f| f.commit() == pull.head_ref_oid);
    // The checks of the pull request that still failed, after a fix of
    // checks that pushed nothing.
    let still = match (&pull, fix) {
        (Some(pull), Some(Fix::Checks(_, names))) if same(pull) => Some(
            still_failed(gh.clone(), pull.number)
                .await
                .unwrap_or_else(|_| names.to_vec()),
        ),
        _ => None,
    };
    let (body, kind) = match &pull {
        Some(pull) if same(pull) => match &still {
            Some(checks) if checks.is_empty() => (
                text::wave_checks_rerun(item, pull.number, &branch, &pull.head_ref_oid, session),
                Kind::Note,
            ),
            Some(checks) => (
                text::wave_checks_not_fixed(
                    item,
                    pull.number,
                    &branch,
                    &pull.head_ref_oid,
                    session,
                    checks,
                ),
                Kind::Message,
            ),
            None => {
                // The default branch is not in a file: the message says
                // "the default branch" when it does not read.
                let base = blocking({
                    let (gh, repo) = (gh.clone(), wave.repo.clone());
                    move || pr::default_branch(&gh, &repo)
                })
                .await
                .unwrap_or_else(|_| text::WAVE_BASE_UNREAD.to_owned());
                (
                    text::wave_not_rebased(
                        item,
                        pull.number,
                        &branch,
                        &base,
                        &pull.head_ref_oid,
                        session,
                    ),
                    Kind::Message,
                )
            }
        },
        Some(pull) if wip_head(&gh, &wave.repo, &pull.head_ref_oid).await => (
            text::wave_wip_head(item, pull.number, &branch, &pull.head_ref_oid, session),
            Kind::Note,
        ),
        Some(pull) => match no_verify(api, me, &thread, gh, item, issue).await? {
            Some(why) => (
                text::wave_no_verify(
                    item,
                    pull.number,
                    &branch,
                    &pull.head_ref_oid,
                    session,
                    &why,
                ),
                Kind::Note,
            ),
            None => (
                text::wave_verify_request(item, pull.number, &branch, &pull.head_ref_oid, session),
                Kind::Message,
            ),
        },
        None => (
            match saved {
                Some(commit) => {
                    text::wave_saved(item, &branch, commit, &record.folder.join("clone"))
                }
                None => text::wave_no_pull(item, &branch, &record.folder.join("clone")),
            },
            Kind::Note,
        ),
    };
    match &pull {
        Some(pull) => {
            lines.push(text::run_pull(pull.number, &pull.url));
            lines.push(match kind {
                Kind::Message if same(pull) => text::run_lead_told(item),
                Kind::Message => text::run_verify_sent(item, pull.number),
                _ => text::run_lead_noted(item),
            });
        }
        None => {
            lines.push(text::run_no_pull(&branch));
            // A fault of the post does not hide the note to the lead.
            let scrub = crate::run::Scrub::new(crate::session_relay::secrets_here());
            let end = crate::ends::End::of(record, ended, &scrub);
            if let Err(e) = crate::ends::post(api, me, &end).await {
                eprintln!("riff daemon: {}: {e:#}", record.live.name);
            }
        }
    }
    api.post(me, Some(&thread), &to, &body, kind).await?;
    Ok(lines)
}

/// True when the head commit `sha` of the repository `repo` is a WIP
/// commit ([`rollout::head_wip`]). A fault of `gh` is no WIP.
async fn wip_head(gh: &pr::Gh, repo: &str, sha: &str) -> bool {
    let (gh, repo, sha) = (gh.clone(), repo.to_owned(), sha.to_owned());
    blocking(move || Ok(rollout::head_wip(&gh, &repo, &sha)))
        .await
        .unwrap_or(false)
}

/// [`run::no_verify`] now: the labels of the issue with `gh`, then the
/// holds of `thread` on riff-server, as `me`. It asks for the holds only
/// for an issue with no label. A fault of `gh` shows on stderr, and riff
/// goes on to the holds.
async fn no_verify(
    api: &Api,
    me: &SessionUri,
    thread: &ThreadName,
    gh: pr::Gh,
    item: &str,
    issue: u64,
) -> Result<Option<String>> {
    let labelled = blocking(move || run::labelled(&gh, issue))
        .await
        .unwrap_or_else(|e| {
            eprintln!("riff: {e:#}");
            false
        });
    if labelled {
        return Ok(run::no_verify(item, issue, true, &BTreeMap::new()));
    }
    let holds: BTreeMap<String, HoldInfo> = api.plan_show(me, thread).await?.holds;
    Ok(run::no_verify(item, issue, false, &holds))
}

/// The steps after the end of a verify: the report of its result, as
/// `riff verify` does. The folder of a verify that did not pass goes. It
/// gives a line for each step.
async fn after_verify(
    api: &Api,
    record: &Record,
    wave: &Wave,
    pull: u64,
    commit: &str,
) -> Result<Vec<String>> {
    let me = &record.me;
    let thread = wave.thread()?;
    let lead = [riff_core::selector::Selector::lead(
        me.who().user(),
        &wave.repo,
    )];
    let item = &record.live.item;
    let found = last_result(&record.folder.join(crate::daemon::EVENTS));
    let Some((verdict, result)) = found.as_deref().and_then(verdict) else {
        let _ = std::fs::remove_dir_all(&record.folder);
        let body = text::wave_no_result(item, pull);
        api.post(me, Some(&thread), &lead, &body, Kind::Message)
            .await?;
        return Ok(vec![body]);
    };
    let (repo, tested, shown) = (wave.repo.clone(), commit.to_owned(), result.clone());
    let reported = blocking(move || {
        pr::report(
            &pr::Gh::for_repo(&repo),
            &repo,
            pull,
            &tested,
            verdict,
            &shown,
        )
    })
    .await;
    let reported = match reported {
        Ok(reported) => reported,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&record.folder);
            let body = text::wave_no_report(item, pull, &format!("{e:#}"));
            api.post(me, Some(&thread), &lead, &body, Kind::Message)
                .await?;
            return Ok(vec![body]);
        }
    };
    let line = text::verify_reported(verdict, pull, &reported);
    println!("{line}");
    if verdict == Verdict::Fail {
        let _ = std::fs::remove_dir_all(&record.folder);
    }
    let state = match verdict {
        Verdict::Pass => PullState::Passed,
        Verdict::Fail => PullState::Failed,
    };
    crate::look::tell_fact(api, me, reported.issue, pull, state).await;
    let to = pr::result_to(reported.issue, me, &api.who(me, false).await?);
    let body = text::verify_post(verdict, pull, &reported, &result);
    api.post(me, Some(&thread), &to, &body, Kind::Message)
        .await?;
    Ok(vec![line])
}

impl Live {
    /// Starts the keep-alive of the claim of the session `me`, a session
    /// in a container: it is live while the keep-alive comes
    /// (01M510MG0CKDXAKF6YJ768K7E8).
    fn keep_alive(&self, api: Api, me: SessionUri, session: &str) {
        let task = tokio::spawn(async move {
            api::keep_alive_in_container(&api, &me, WORKER_ALIVE_EVERY).await;
        });
        if let Some(old) = lock(&self.alive).insert(session.to_owned(), task) {
            old.abort();
        }
    }

    /// Releases `item` of the session `me`, ends the session, and stops
    /// its keep-alive.
    async fn end(&self, api: &Api, me: &SessionUri, thread: &ThreadName, item: &str) {
        if let Err(e) = api.release(me, thread, item).await {
            eprintln!("riff daemon: {e:#}");
        }
        if let Some(session) = me.who().session()
            && let Some(task) = lock(&self.alive).remove(session)
        {
            task.abort();
        }
        let _ = api.end(me).await;
    }

    /// The steps before the start after the claim: the clone, the
    /// Claude folder, the spec and the start text. `base` is the default
    /// branch of the repository.
    #[allow(clippy::too_many_arguments)]
    async fn prepare(
        wave: &Wave,
        work: &Work,
        image: &str,
        host: &run::Host,
        base: &str,
        milestone: Option<String>,
        session: &str,
        me: &SessionUri,
    ) -> Result<Start> {
        let item = work.item();
        let folder = run::sessions_dir()?.join(format!("{item}-{}", run::short(session)));
        let clone = folder.join("clone");
        let claude = folder.join("claude");
        let sources = session_image::merged_sources(&wave.main)?;
        let auto = crate::permissions::Trust::of(me.place().repo(), &wave.server, &sources);
        run::claude_folder(&claude, &clone, &host.home.join(".claude.json"), &auto)?;
        let (main, common) = run::main_clone(&wave.main)?;
        let start = match work {
            Work::Build { issue } => {
                let issue = *issue;
                let branch = branch_of(issue);
                let (repo, at) = (wave.repo.clone(), clone.clone());
                let earlier = {
                    let (main, branch) = (main.clone(), branch.clone());
                    blocking(move || run::make_clone(&main, &at, &branch)).await?
                };
                let failed = blocking(move || {
                    let pulls = rollout::pulls(&pr::Gh::for_repo(&repo), &repo)?;
                    Ok(failed_verify(&pulls, issue))
                })
                .await
                .unwrap_or_default();
                let milestone = milestone.context("a build has a milestone")?;
                let frame = pr::body(hygiene::Link::Closes(issue), "SUMMARY", &milestone)?;
                let mut text = text::run_start(
                    &wave.repo, base, issue, &branch, session, &frame, &milestone, earlier,
                );
                if let Some(failed) = failed {
                    text.push_str("\n\n");
                    text.push_str(&failed);
                }
                text
            }
            Work::Rebase {
                issue,
                pull,
                commit,
            } => {
                let branch = branch_of(*issue);
                let (main, at, on) = (main.clone(), clone.clone(), branch.clone());
                blocking(move || run::make_clone(&main, &at, &on)).await?;
                text::wave_rebase_start(&wave.repo, base, *issue, *pull, commit, &branch, session)
            }
            Work::Checks {
                issue,
                pull,
                commit,
                checks,
            } => {
                let branch = branch_of(*issue);
                let (main, at, on) = (main.clone(), clone.clone(), branch.clone());
                blocking(move || run::make_clone(&main, &at, &on)).await?;
                text::wave_checks_start(
                    &wave.repo, base, *issue, *pull, commit, checks, &branch, session,
                )
            }
            Work::Verify {
                issue,
                pull,
                commit,
            } => {
                let (at, commit_at) = (clone.clone(), commit.clone());
                let (main_at, pull_at) = (main.clone(), *pull);
                blocking(move || clone_at(&main_at, &at, pull_at, &commit_at)).await?;
                let repo = wave.repo.clone();
                let checks =
                    blocking(move || repo_setup::full_check(&pr::Gh::for_repo(&repo), &repo))
                        .await?;
                text::wave_verify_start(&wave.repo, base, *issue, *pull, commit, &checks)
            }
        };
        let existing = |p: PathBuf| p.exists().then_some(p);
        let facts = Facts {
            session: session.to_owned(),
            item: item.clone(),
            image: image.to_owned(),
            home: host.home.clone(),
            user: host.user.clone(),
            clone,
            objects: common.join("objects"),
            claude,
            sign_in: host.claude.sign_in(),
            gitconfig: existing(host.home.join(".gitconfig")),
            gh_config: existing(host.home.join(".config/gh")),
        };
        let secrets = host.secrets();
        Ok(Start {
            session: session.to_owned(),
            item,
            folder,
            spec: run::spec(&facts),
            start,
            values: host.values(),
            secrets,
            server: wave.server.clone(),
            me: me.clone(),
            terminal: false,
        })
    }
}

/// The line for the start text of a build when the verify of the pull
/// request of its issue failed: where the result is.
fn failed_verify(pulls: &[rollout::Pull], issue: u64) -> Option<String> {
    let pull = pulls.iter().find(|p| p.issue() == Some(issue))?;
    (pull.verify() == Some(rollout::Verify::Failed)).then(|| {
        let url = pull
            .verify_status()
            .and_then(|c| c.url.as_deref())
            .map(text::safe)
            .unwrap_or_default();
        text::wave_failed_verify(pull.number, &pull.head, &url)
    })
}

/// Makes the clone of a verify at `commit`, detached: the head commit of
/// the pull request `pull`. The clone has the branches of `origin` only,
/// and the head of a pull request from a fork is on no such branch. So
/// it fetches `refs/pull/PULL/head` from `origin` first. It checks out
/// the commit, not the ref: a push to the pull request after the look
/// does not change what the verify reads
/// (01M5QVARWYFHTQV5THNRB09SZA).
fn clone_at(main: &Path, clone: &Path, pull: u64, commit: &str) -> Result<()> {
    let remote = run::output("git", main, &["remote", "get-url", "origin"])?;
    let top = main.to_string_lossy();
    let path = clone.to_string_lossy();
    run::output(
        "git",
        main,
        &["clone", "-q", "--reference", &top, "--", &remote, &path],
    )?;
    run::output(
        "git",
        clone,
        &["fetch", "-q", "origin", &format!("refs/pull/{pull}/head")],
    )?;
    run::output(
        "git",
        clone,
        &[
            "-c",
            "advice.detachedHead=false",
            "checkout",
            "-q",
            "--detach",
            commit,
        ],
    )?;
    Ok(())
}

/// The free work of a wave with the fixes of the pull requests that
/// cannot merge by themselves, from the free work `free` of the facts
/// `open` ([`rollout::Open::free_work`]) (01M57FJ3WWMQSDBFRRHXN3SMFQ,
/// #279). They are [`Fixes`]:
///
/// - A pull request to rebase is an open pull request of an item with
///   auto-merge on and a conflict with `main` ([`rollout::Pull::conflict`])
///   whose verify did not fail: a failed verify is free work as a build
///   already. The free work has no verify of a head with a conflict.
/// - A pull request with failed checks is one whose verify passed and
///   whose head has a failed check ([`rollout::Stuck::Checks`]). It is a
///   build, with the names of the checks.
/// - A pull request that no session can fix gets a notice for the lead:
///   one with auto-merge off after a pass ([`rollout::Stuck::AutoMergeOff`]),
///   and a release or a tracker with a failed check. No session builds
///   a release (the label `release`) or a tracker.
///
/// The items `issue-N` of the rebases come after the fix items of a
/// broken `main` (01M51QEYS538XA58KDRY844T52), before each other item,
/// then the items of the failed checks. [`pick`] skips a held or a
/// claimed item, as each item.
///
/// ```
/// use riff::rollout::{Check, Issue, Label, Open, Pull, Stuck};
/// use riff::wave::{Conflict, Failed, Notice, fixes};
///
/// let pull = |number, issue: u64, state: Option<&str>, mergeable: &str| Pull {
///     number,
///     branch: format!("worktree-issue-{issue}"),
///     head: format!("c{issue}"),
///     checks: state.map(Check::verify).into_iter().collect(),
///     mergeable: Some(mergeable.into()),
///     auto_merge: true,
///     ..Pull::default()
/// };
/// let gate = |pull: Pull| Pull { checks: [pull.checks, vec![Check::run("Gate", "FAILURE")]].concat(), ..pull };
/// let open = Open {
///     issues: vec![
///         Issue { number: 9, labels: vec![Label { name: "broken-main".into() }], ..Issue::default() },
///         Issue { number: 8, labels: vec![Label { name: "release".into() }], ..Issue::default() },
///     ],
///     pulls: vec![
///         pull(41, 1, Some("SUCCESS"), "CONFLICTING"),
///         pull(42, 2, None, "CONFLICTING"),
///         pull(43, 3, Some("FAILURE"), "CONFLICTING"),
///         pull(44, 4, Some("SUCCESS"), "MERGEABLE"),
///         Pull { auto_merge: false, ..pull(45, 5, Some("SUCCESS"), "CONFLICTING") },
///         gate(pull(46, 6, Some("SUCCESS"), "MERGEABLE")),
///         gate(pull(47, 8, None, "MERGEABLE")),
///     ],
///     ..Open::default()
/// };
/// let free = ["issue-9", "verify-issue-2", "issue-3", "issue-7"].map(String::from).to_vec();
/// let (free, fixes) = fixes(&open, free);
/// // The rebases of 1 and 2, the fix of the checks of 6.
/// assert_eq!(free, ["issue-9", "issue-1", "issue-2", "issue-6", "issue-3", "issue-7"]);
/// assert_eq!(fixes.conflicts.keys().copied().collect::<Vec<_>>(), [1, 2]);
/// assert_eq!(fixes.conflicts[&1], Conflict { pull: 41, commit: "c1".into() });
/// assert_eq!(fixes.failed[&6], Failed { pull: 46, commit: "c6".into(), checks: vec!["Gate".into()] });
/// // The lead hears of 5 (auto-merge off) and of the release 8.
/// let told: Vec<(u64, Stuck)> = fixes.notices.iter().map(|n| (n.pull, n.stuck.clone())).collect();
/// assert_eq!(told, [(45, Stuck::AutoMergeOff), (47, Stuck::Checks(vec!["Gate".into()]))]);
/// assert_eq!(fixes.notices[0], Notice { pull: 45, branch: "worktree-issue-5".into(), commit: "c5".into(), stuck: Stuck::AutoMergeOff });
/// ```
pub fn fixes(open: &rollout::Open, free: Vec<String>) -> (Vec<String>, Fixes) {
    // No session builds a release or a tracker.
    let no_build: HashSet<u64> = open
        .issues
        .iter()
        .filter(|i| i.release() || i.tracker())
        .map(|i| i.number)
        .collect();
    let mut found = Fixes::default();
    for (p, stuck) in open.stuck() {
        let item = p.issue().filter(|n| !no_build.contains(n));
        match (stuck, item) {
            // A conflict has an issue: its verify is not `None`.
            (rollout::Stuck::Conflict, _) => {
                if let Some(n) = p.issue() {
                    let conflict = Conflict {
                        pull: p.number,
                        commit: p.head.clone(),
                    };
                    found.conflicts.insert(n, conflict);
                }
            }
            (rollout::Stuck::Checks(checks), Some(n)) => {
                let failed = Failed {
                    pull: p.number,
                    commit: p.head.clone(),
                    checks,
                };
                found.failed.insert(n, failed);
            }
            (stuck, _) => found.notices.push(Notice {
                pull: p.number,
                branch: p.branch.clone(),
                commit: p.head.clone(),
                stuck,
            }),
        }
    }
    let fix_items: HashSet<u64> = open
        .issues
        .iter()
        .filter(|i| i.broken_main())
        .map(|i| i.number)
        .collect();
    let fix = |item: &String| usage::issue_of(item).is_some_and(|(n, _)| fix_items.contains(&n));
    let mut free: Vec<String> = free
        .into_iter()
        .filter(|item| verify_issue(item).is_none_or(|n| !found.conflicts.contains_key(&n)))
        .collect();
    let at = free.iter().take_while(|item| fix(item)).count();
    let new: Vec<String> = found
        .conflicts
        .keys()
        .chain(found.failed.keys())
        .map(|n| format!("issue-{n}"))
        .filter(|item| !free.contains(item))
        .collect();
    free.splice(at..at, new);
    (free, found)
}

/// The verify requests that answer a failed verify: the pairs of an
/// issue and a commit (7 characters) whose last verify request in
/// `bodies`, the bodies of the messages of the thread, oldest first,
/// came after the last verify result for them (#1157). A fix of the
/// body of a pull request makes no new commit, so the failed status
/// of the head stays. The new request is the sign that the fail is
/// answered.
///
/// ```
/// use riff::wave::reasked;
///
/// let request = "verify request: issue-12, PR #40, branch worktree-issue-12, commit 1a2b3c4, from the riff daemon (session 65f510d7).";
/// let result = "verify result: FAIL for issue-12, PR #40, commit 1a2b3c4. PR comment https://c, riff/verify failure set.";
/// assert!(reasked(&[request, result]).is_empty());
/// assert!(reasked(&[request, result, request]).contains(&(12, "1a2b3c4".into())));
/// assert!(reasked(&[request, result, request, result]).is_empty());
/// // A result names the full commit.
/// let full = "verify result: FAIL for issue-12, PR #40, commit 1a2b3c4d5e6f7a8b9c0d. PR comment https://c.";
/// assert!(reasked(&[request, full]).is_empty());
/// ```
pub fn reasked(bodies: &[&str]) -> HashSet<(u64, String)> {
    // `issue-N, PR #P, ... commit ABCDEFG`.
    let key = |rest: &str| -> Option<(u64, String)> {
        let issue = rest.strip_prefix("issue-")?;
        let issue = issue
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()?;
        let commit = rest.split_once("commit ")?.1;
        let commit: String = commit
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .take(7)
            .collect();
        Some((issue, commit))
    };
    let mut asked = HashSet::new();
    for body in bodies {
        if let Some(rest) = body.strip_prefix("verify request: ") {
            asked.extend(key(rest));
        } else if let Some(found) = body
            .strip_prefix("verify result: ")
            .and_then(|r| r.split_once(" for "))
            .and_then(|(_, rest)| key(rest))
        {
            asked.remove(&found);
        }
    }
    asked
}

/// The facts of the wave of the repository `repo` from the forge, with
/// `gh`, and the items that a session claims (`claims`) and the held
/// items (`holds`): the default branch ([`pr::default_branch`]), the free
/// work with the fixes ([`fixes`]), and
/// the pull request of each verify among it, with the full check of its
/// head ([`repo_setup::full_check`]).
pub fn look_on(
    gh: &pr::Gh,
    repo: &str,
    claims: BTreeSet<String>,
    holds: BTreeSet<String>,
) -> Result<Look> {
    look_with(gh, repo, claims, holds, &HashSet::new())
}

/// [`look_on`] with the verify requests `asked` that answer a failed
/// verify ([`reasked`]): a pull request with a failed verify whose
/// head is in it waits for a verify again.
pub fn look_with(
    gh: &pr::Gh,
    repo: &str,
    claims: BTreeSet<String>,
    holds: BTreeSet<String>,
    asked: &HashSet<(u64, String)>,
) -> Result<Look> {
    // A branch that does not read is the error of the look: the wave starts
    // no work, and reads it again at the next look.
    let branch = pr::default_branch(gh, repo)?;
    let claimed: HashSet<String> = claims.iter().cloned().collect();
    let held: HashSet<String> = holds.iter().cloned().collect();
    let mut open = rollout::Open::of(gh, repo)?;
    for pull in &mut open.pulls {
        let commit: String = pull.head.chars().take(7).collect();
        pull.reasked = pull.issue().is_some_and(|n| asked.contains(&(n, commit)));
    }
    let (free, fixes) = fixes(&open, open.free_work(&claimed, &held));
    let no_done_when = open.no_done_when(&claimed, &held).into_iter().collect();
    let pulls = waiting(gh, repo, &free, &open)?;
    Ok(Look {
        paused: false,
        free,
        claims,
        holds,
        pulls,
        fixes,
        no_done_when,
        branch,
    })
}

/// The pull request of each verify among the free work `free` of `open`,
/// with the full check of its head (01M5QYDHN27RC5SRHAB56PFKK0). The wave
/// reads the full check of the repository one time, and only when a
/// verify waits ([`repo_setup::full_check`]).
fn waiting(
    gh: &pr::Gh,
    repo: &str,
    free: &[String],
    open: &rollout::Open,
) -> Result<BTreeMap<u64, Waiting>> {
    let mut pulls = BTreeMap::new();
    let mut checks: Option<Vec<String>> = None;
    for issue in free.iter().filter_map(|item| verify_issue(item)) {
        let Some(pull) = open.pulls.iter().find(|p| p.issue() == Some(issue)) else {
            continue;
        };
        if checks.is_none() {
            checks = Some(repo_setup::full_check(gh, repo)?);
        }
        let gate = pr::gate_of(gh, repo, checks.as_deref().unwrap_or_default(), &pull.head)?;
        pulls.insert(
            issue,
            Waiting {
                pull: pull.number,
                commit: pull.head.clone(),
                gate_done: matches!(gate, pr::Gate::Passed | pr::Gate::Ended(_)),
            },
        );
    }
    Ok(pulls)
}

impl World for Live {
    fn look<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<Look>> {
        Box::pin(async move {
            let person = identity::person(&wave.place(None)?, &wave.server)?;
            let thread = wave.thread()?;
            let api = Api::new(&wave.server).signed_in(None)?;
            if api.pauses(&person).await?.state == RiffState::Paused {
                return Ok(Look {
                    paused: true,
                    ..Look::default()
                });
            }
            let claims: BTreeSet<String> = api
                .who(&person, false)
                .await?
                .iter()
                .filter(|s| s.uri.place().repo_text() == wave.repo)
                .flat_map(|s| s.uri.claims().iter().cloned())
                .collect();
            let holds: BTreeSet<String> = api
                .plan_show(&person, &thread)
                .await?
                .holds
                .into_keys()
                .collect();
            let bodies: Vec<String> = api
                .read(&person, &thread, true)
                .await
                .map(|all| all.into_iter().map(|m| m.message.body).collect())
                .unwrap_or_default();
            let asked = reasked(&bodies.iter().map(String::as_str).collect::<Vec<_>>());
            let repo = wave.repo.clone();
            blocking(move || look_with(&pr::Gh::for_repo(&repo), &repo, claims, holds, &asked))
                .await
        })
    }

    fn image<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<String>> {
        let (main, repo) = (wave.main.clone(), wave.repo.clone());
        Box::pin(blocking(move || {
            let image = session_image::merged(&main, |line| println!("{repo}: {line}"))?;
            Ok(image.to_string())
        }))
    }

    fn host<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<(), run::HostFault>> {
        let main = wave.main.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || run::host_facts(&main).map(drop))
                .await
                .unwrap_or_else(|e| Err(run::HostFault::Missing(e.to_string())))
        })
    }

    fn begin<'a>(
        &'a self,
        wave: &'a Wave,
        work: &'a Work,
        image: &'a str,
    ) -> BoxFuture<'a, Result<Option<Start>>> {
        Box::pin(async move {
            let item = work.item();
            let main = wave.main.clone();
            let host = blocking(move || Ok(run::host_facts(&main)?)).await?;
            let base = {
                let repo = wave.repo.clone();
                blocking(move || pr::default_branch(&pr::Gh::for_repo(&repo), &repo)).await?
            };
            let milestone = match work {
                Work::Build { issue } => {
                    let (repo, issue) = (wave.repo.clone(), *issue);
                    Some(blocking(move || run::milestone(&pr::Gh::for_repo(&repo), issue)).await?)
                }
                Work::Verify { .. } | Work::Rebase { .. } | Work::Checks { .. } => None,
            };
            let session = identity::new_session_id();
            let place = wave.place(Some(&item))?;
            let me = identity::agent(&place, &session, &wave.server)?;
            let thread = wave.thread()?;
            let api = Api::new(&wave.server).signed_in(Some(&session))?;
            api.start(&me, StartReason::Process, true).await?;
            // A refused claim ends the new session: else it lives on as
            // an idle worker with no item (01M558EYCXPFW8CJV3XJ7HK3NR).
            let claimed = match api.claim(&me, &thread, &item).await {
                Ok(claimed) => claimed,
                Err(e) => {
                    let _ = api.end(&me).await;
                    return Err(e);
                }
            };
            if !claimed.granted {
                let _ = api.end(&me).await;
                return Ok(None);
            }
            self.keep_alive(api.clone(), me.clone(), &session);
            println!("{}", text::wave_claimed(&wave.repo, &item, &session));
            match Self::prepare(wave, work, image, &host, &base, milestone, &session, &me).await {
                Ok(start) => Ok(Some(start)),
                Err(e) => {
                    self.end(&api, &me, &thread, &item).await;
                    Err(e)
                }
            }
        })
    }

    fn resume(&self, record: &Record) {
        match Api::new(&record.server).signed_in(Some(&record.live.session)) {
            Ok(api) => self.keep_alive(api, record.me.clone(), &record.live.session),
            Err(e) => eprintln!("riff daemon: {}: {e:#}", record.live.name),
        }
    }

    fn finish<'a>(
        &'a self,
        record: &'a Record,
        ended: Ended,
    ) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            let job = record
                .job
                .as_ref()
                .ok_or_else(|| anyhow!("the session {} has no job", record.live.name))?;
            let wave = &job.wave;
            // The steps after the end outlast a restart of riff-server,
            // as in `riff run` (01M4ZQVVGQYXTAVRB69C7YPC3Y).
            let api = Api::new(&record.server)
                .signed_in(Some(&record.live.session))?
                .with_budget(run::END_BUDGET);
            let thread = wave.thread()?;
            println!(
                "{}",
                text::wave_ended(&wave.repo, &record.live.item, ended.code, ended.failed)
            );
            let done = match &job.work {
                Work::Build { issue } => {
                    let gh = pr::Gh::for_repo(&wave.repo);
                    let saved = save_work(record, *issue).await;
                    after_build(
                        gh,
                        &api,
                        record,
                        wave,
                        *issue,
                        (None, saved.as_deref()),
                        ended,
                    )
                    .await
                }
                Work::Rebase { issue, commit, .. } => {
                    let gh = pr::Gh::for_repo(&wave.repo);
                    let fix = Some(Fix::Rebase(commit));
                    after_build(gh, &api, record, wave, *issue, (fix, None), ended).await
                }
                Work::Checks {
                    issue,
                    commit,
                    checks,
                    ..
                } => {
                    let gh = pr::Gh::for_repo(&wave.repo);
                    let fix = Some(Fix::Checks(commit, checks));
                    after_build(gh, &api, record, wave, *issue, (fix, None), ended).await
                }
                Work::Verify { pull, commit, .. } => {
                    after_verify(&api, record, wave, *pull, commit).await
                }
            };
            self.end(&api, &record.me, &thread, &record.live.item).await;
            done
        })
    }

    fn meter(&self, repo: &str) -> Option<usage::Meter> {
        Some(usage::Meter {
            dir: crate::local::marks()?,
            gh: sweep::gh(repo),
        })
    }

    fn marks(&self) -> Option<PathBuf> {
        crate::local::marks()
    }

    fn fate<'a>(&'a self, repo: &'a str, issue: u64) -> BoxFuture<'a, Result<Fate>> {
        let gh = sweep::gh(repo);
        Box::pin(blocking(move || sweep::fate_on(&gh, issue)))
    }

    fn delete_old_kept<'a>(&'a self, repo: &'a str) -> BoxFuture<'a, Result<Vec<String>>> {
        let (gh, repo) = (sweep::gh(repo), repo.to_owned());
        Box::pin(blocking(move || sweep::delete_old_kept_on(&gh, &repo)))
    }

    fn merged_in<'a>(
        &'a self,
        repo: &'a str,
        issue: u64,
        pull: u64,
        commit: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        let (gh, commit) = (sweep::gh(repo), commit.to_owned());
        Box::pin(blocking(move || {
            sweep::merged_in_on(&gh, issue, pull, &commit)
        }))
    }

    fn leave<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if let Some(task) = lock(&self.alive).remove(&record.live.session) {
                task.abort();
            }
            match Api::new(&record.server).signed_in(Some(&record.live.session)) {
                Ok(api) => {
                    let _ = api.end(&record.me).await;
                }
                Err(e) => eprintln!("riff daemon: {}: {e:#}", record.live.name),
            }
        })
    }

    fn free<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let api = Api::new(&record.server).signed_in(Some(&record.live.session))?;
            let thread = record
                .me
                .default_thread()
                .ok_or_else(|| anyhow!("{}", text::RUN_NO_REPO))?;
            api.release(&record.me, &thread, &record.live.item).await?;
            if let Some(task) = lock(&self.alive).remove(&record.live.session) {
                task.abort();
            }
            api.end(&record.me).await
        })
    }

    fn wake<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            use futures::StreamExt;
            let api = Api::new(&record.server).signed_in(Some(&record.live.session))?;
            let me = &record.me;
            let wakes = api::follow(|| api.watch(me), WAKE_RETRY);
            tokio::pin!(wakes);
            loop {
                match wakes.next().await {
                    Some(Ok(wake)) => return Ok(text::wake_line(&wake)),
                    Some(Err(e)) => eprintln!("riff daemon: {}: {e:#}", record.live.name),
                    None => bail!("{}", text::DAEMON_GONE),
                }
            }
        })
    }

    fn wakes<'a>(
        &'a self,
        record: &'a Record,
        to: mpsc::UnboundedSender<Wake>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            use futures::StreamExt;
            let api = match Api::new(&record.server).signed_in(Some(&record.live.session)) {
                Ok(api) => api,
                Err(e) => {
                    eprintln!("riff daemon: {}: {e:#}", record.live.name);
                    return;
                }
            };
            let me = &record.me;
            let wakes = api::follow(|| api.watch(me), WAKE_RETRY);
            tokio::pin!(wakes);
            while let Some(wake) = wakes.next().await {
                match wake {
                    Ok(wake) => {
                        if to.send(wake).is_err() {
                            return;
                        }
                    }
                    Err(e) => eprintln!("riff daemon: {}: {e:#}", record.live.name),
                }
            }
        })
    }

    fn refuse<'a>(&'a self, record: &'a Record, wake: &'a Wake) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let body = text::wake_refused(&record.live.session, wake);
            let told = async {
                let api = Api::new(&record.server).signed_in(Some(&record.live.session))?;
                api.tell(&record.me, &wake.from.to_string(), &body).await
            };
            if let Err(e) = told.await {
                eprintln!("riff daemon: {}: {e:#}", record.live.name);
            }
        })
    }

    fn tell<'a>(&'a self, wave: &'a Wave, body: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if lock(&self.told).get(&wave.repo).is_some_and(|t| t == body) {
                return;
            }
            match post_as_person(wave, None, body, Kind::Message).await {
                Ok(()) => {
                    lock(&self.told).insert(wave.repo.clone(), body.to_owned());
                }
                Err(e) => eprintln!("riff daemon: {}: {e:#}", wave.repo),
            }
        })
    }

    fn gh(&self, repo: &str) -> Arc<pr::Gh> {
        lock(&self.ghs)
            .entry(repo.to_owned())
            .or_insert_with(|| Arc::new(sweep::gh(repo).with_one_token()))
            .clone()
    }

    fn claims<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<HashSet<String>>> {
        Box::pin(async move {
            let person = identity::person(&wave.place(None)?, &wave.server)?;
            let api = Api::new(&wave.server).signed_in(None)?;
            let mut sessions = api.who(&person, false).await?;
            sessions.retain(|s| s.uri.place().repo_text() == wave.repo);
            Ok(rollout::claims(&sessions))
        })
    }

    fn post<'a>(
        &'a self,
        wave: &'a Wave,
        item: Option<&'a str>,
        body: &'a str,
        kind: Kind,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(post_as_person(wave, item, body, kind))
    }

    fn chat<'a>(&'a self, record: &'a Record, body: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let posted = async {
                let person = identity::person(record.me.place(), &record.server)?;
                let api = Api::new(&record.server).signed_in(None)?;
                api.post(
                    &person,
                    Some(&crate::chat::thread()),
                    &[],
                    body,
                    Kind::Message,
                )
                .await?;
                Ok::<(), anyhow::Error>(())
            };
            if let Err(e) = posted.await {
                eprintln!("riff daemon: {}: {e:#}", record.live.name);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    use super::*;
    use crate::daemon::dir_in;
    use crate::room::Limit;
    use crate::runtime::{Bind, Env, Fake, SessionSpec, Source};

    const IMAGE: &str = "localhost/riff-session:test";
    const GB: u64 = 1 << 30;

    /// A pull request of the fake forge.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Pr {
        /// It waits for a verify.
        Asked,
        /// The verify passed.
        Passed,
        /// The verify failed.
        Failed,
        /// The verify failed, and a new verify request for the head came.
        Reasked,
    }

    /// The forge and the riff of a test.
    #[derive(Debug, Default)]
    struct Forge {
        /// The open items of the wave, in order.
        items: Vec<u64>,
        holds: BTreeSet<String>,
        /// The claims: the item, and the session that holds it.
        claims: BTreeMap<String, String>,
        /// The pull requests, by issue.
        pulls: BTreeMap<u64, Pr>,
        /// Each claim of a session: the item, and the session.
        starts: Vec<(String, String)>,
        /// True while the default branch of the repository does not read.
        branch_unread: bool,
        /// The looks that read the default branch.
        branch_reads: usize,
        /// Each item after its end.
        finished: Vec<String>,
        /// Each message to the lead.
        told: Vec<String>,
        sessions: u64,
        /// A build ends with no pull request.
        no_pull: bool,
        /// Each folder is `ITEM-ID8` with a git clone of `o/r`, so that
        /// the sweep acts on it.
        git: bool,
        /// The fate of the folders of each issue, else it stays.
        fates: BTreeMap<u64, Fate>,
        /// The steps after the end wait for a permit of it.
        gate: Option<Arc<tokio::sync::Semaphore>>,
        /// Each item whose steps after the end started.
        finishing: Vec<String>,
        /// The codes of the refusals of the next claims, in order, as
        /// riff-server gives them: for example `paused`.
        refusals: Vec<String>,
        /// Each item whose claim riff-server refused.
        refused: Vec<String>,
        /// `claude` refused the token of the host.
        token_refused: bool,
        /// The minutes that the sign-in of `claude` on the host has
        /// left. `None`: many.
        minutes: Option<i64>,
        /// The number of the next steps before the start that fail for
        /// a sign-in of `claude` that ends soon, after the check of the
        /// host passed.
        late_faults: usize,
        /// The head commit of the pull request of each issue, else
        /// `cN`.
        heads: BTreeMap<u64, String>,
        /// The issues whose pull request has a conflict with `main`.
        conflicting: BTreeSet<u64>,
        /// The issues whose pull request has auto-merge off.
        manual: BTreeSet<u64>,
        /// The issues whose pull request has a failed check `Gate`.
        failing: BTreeSet<u64>,
        /// A rebase and a fix of checks end with no push.
        no_push: bool,
        /// The work of each claim of a session, in order.
        works: Vec<Work>,
        /// The title of each item, else `item N`.
        titles: BTreeMap<u64, String>,
        /// The labels of each item.
        labels: BTreeMap<u64, Vec<String>>,
        /// The items whose body has no `Done when:` line.
        bare: BTreeSet<u64>,
        /// Each post of the look: the item whose holder gets it, or `None`
        /// for the lead, the kind, and the body.
        posts: Vec<(Option<String>, Kind, String)>,
        /// The number of the next posts of the look that fail.
        post_fails: usize,
        /// The full check of a pull request is what the fake `gh` tells
        /// ([`FAKE_GH`]), as for the real look. Else each is done.
        checked: bool,
    }

    impl Forge {
        /// The fault of the host now.
        fn host(&self) -> Result<(), run::HostFault> {
            if self.token_refused {
                return Err(run::HostFault::TokenRefused);
            }
            match self.minutes.and_then(run::HostFault::of_minutes) {
                Some(fault) => Err(fault),
                None => Ok(()),
            }
        }

        /// The head commit of the pull request of `issue`.
        fn head(&self, issue: u64) -> String {
            self.heads
                .get(&issue)
                .cloned()
                .unwrap_or_else(|| format!("c{issue}"))
        }

        /// The open milestone `Wave 1` with the items, and the open pull
        /// requests, as `gh` gives them.
        fn open(&self) -> rollout::Open {
            let wave = || rollout::Milestone {
                title: "Wave 1".into(),
            };
            let issues = self.items.iter().map(|n| rollout::Issue {
                number: *n,
                title: self
                    .titles
                    .get(n)
                    .cloned()
                    .unwrap_or_else(|| format!("item {n}")),
                body: match self.bare.contains(n) {
                    true => "No line.".into(),
                    false => "Done when: it works.".into(),
                },
                milestone: Some(wave()),
                labels: self
                    .labels
                    .get(n)
                    .into_iter()
                    .flatten()
                    .map(|name| rollout::Label { name: name.clone() })
                    .collect(),
                ..rollout::Issue::default()
            });
            let pulls = self.pulls.iter().map(|(n, pr)| rollout::Pull {
                number: 40 + n,
                branch: branch_of(*n),
                head: self.head(*n),
                checks: match pr {
                    Pr::Asked => Vec::new(),
                    Pr::Passed => vec![rollout::Check::verify("SUCCESS")],
                    Pr::Failed | Pr::Reasked => vec![rollout::Check::verify("FAILURE")],
                }
                .into_iter()
                .chain(
                    self.failing
                        .contains(n)
                        .then(|| rollout::Check::run("Gate", "FAILURE")),
                )
                .collect(),
                mergeable: Some(match self.conflicting.contains(n) {
                    true => "CONFLICTING".into(),
                    false => "MERGEABLE".into(),
                }),
                auto_merge: !self.manual.contains(n),
                reasked: *pr == Pr::Reasked,
                ..rollout::Pull::default()
            });
            rollout::Open {
                milestones: vec![wave()],
                issues: issues.collect(),
                pulls: pulls.collect(),
            }
        }
    }

    /// A fake [`World`]: a forge in memory, a claim that fails for a
    /// held item as on riff-server, and a session folder in `root`.
    struct Fakes {
        root: PathBuf,
        forge: Mutex<Forge>,
        /// The fake `gh` of the look ([`FAKE_GH`]).
        gh: Arc<pr::Gh>,
    }

    impl World for Fakes {
        fn look<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<Look>> {
            Box::pin(async move {
                let mut forge = lock(&self.forge);
                forge.branch_reads += 1;
                if forge.branch_unread {
                    bail!("gh api repos/o/r: HTTP 502");
                }
                // The free work of the forge, as `look_on` takes it. The
                // pick skips a held item.
                let open = forge.open();
                let claims: HashSet<String> = forge.claims.keys().cloned().collect();
                let (free, fixes) = fixes(&open, open.free_work(&claims, &HashSet::new()));
                let no_done_when = open.no_done_when(&claims, &HashSet::new());
                let pulls = match forge.checked {
                    true => waiting(&self.gh, "o/r", &free, &open)?,
                    false => forge
                        .pulls
                        .keys()
                        .map(|n| {
                            let waiting = Waiting {
                                pull: 40 + n,
                                commit: forge.head(*n),
                                gate_done: true,
                            };
                            (*n, waiting)
                        })
                        .collect(),
                };
                Ok(Look {
                    paused: false,
                    free,
                    claims: forge.claims.keys().cloned().collect(),
                    holds: forge.holds.clone(),
                    pulls,
                    fixes,
                    no_done_when: no_done_when.into_iter().collect(),
                    branch: "main".into(),
                })
            })
        }

        fn image<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<String>> {
            Box::pin(async { Ok(IMAGE.into()) })
        }

        fn host<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<(), run::HostFault>> {
            let host = lock(&self.forge).host();
            Box::pin(async move { host })
        }

        fn begin<'a>(
            &'a self,
            _: &'a Wave,
            work: &'a Work,
            image: &'a str,
        ) -> BoxFuture<'a, Result<Option<Start>>> {
            Box::pin(async move {
                let item = work.item();
                let mut forge = lock(&self.forge);
                if forge.holds.contains(&item) || forge.claims.contains_key(&item) {
                    return Ok(None);
                }
                // As `run::host_facts` in the steps of `Live`.
                forge.host()?;
                if forge.late_faults > 0 {
                    forge.late_faults -= 1;
                    forge.refused.push(item);
                    return Err(run::HostFault::SignInEnds(42).into());
                }
                if !forge.refusals.is_empty() {
                    let code = forge.refusals.remove(0);
                    forge.refused.push(item);
                    return Err(api::Refusal {
                        op: "claim".into(),
                        status: reqwest::StatusCode::CONFLICT,
                        text: format!("refused: {code}"),
                        code: Some(code),
                    }
                    .into());
                }
                forge.sessions += 1;
                let session = format!("{:08x}", forge.sessions);
                forge.claims.insert(item.clone(), session.clone());
                forge.starts.push((item.clone(), session.clone()));
                forge.works.push(work.clone());
                let name = format!("riff-{item}-{session}");
                let folder = match forge.git {
                    true => self.root.join("sessions").join(format!("{item}-{session}")),
                    false => self.root.join("sessions").join(&name),
                };
                let clone = folder.join("clone");
                std::fs::create_dir_all(&clone)?;
                if forge.git {
                    crate::daemon::sweep::tests::origin(&clone, "https://github.com/o/r.git");
                }
                let spec = SessionSpec {
                    name,
                    image: image.into(),
                    home: "/home/mike".into(),
                    workdir: clone.clone(),
                    binds: vec![Bind {
                        source: Source::Path(clone.clone()),
                        target: clone,
                        write: true,
                    }],
                    env: vec![Env::Host("GH_TOKEN".into())],
                    memory: "8g".into(),
                    cpus: 4,
                    command: vec!["claude".into()],
                };
                let me = format!("riff://mike@thelio/o/r?session={session}#{item}").parse()?;
                Ok(Some(Start {
                    session,
                    item,
                    folder,
                    spec,
                    start: "Do it.".into(),
                    values: vec![("GH_TOKEN".into(), "gho_secret1".into())],
                    secrets: vec!["gho_secret1".into()],
                    server: "http://127.0.0.1:1".into(),
                    me,
                    terminal: false,
                }))
            })
        }

        fn resume(&self, _: &Record) {}

        fn finish<'a>(
            &'a self,
            record: &'a Record,
            _: Ended,
        ) -> BoxFuture<'a, Result<Vec<String>>> {
            Box::pin(async move {
                let gate = {
                    let mut forge = lock(&self.forge);
                    forge.finishing.push(record.live.item.clone());
                    forge.gate.clone()
                };
                if let Some(gate) = gate {
                    gate.acquire().await?.forget();
                }
                let mut forge = lock(&self.forge);
                let job = record.job.as_ref().expect("a job");
                match job.work {
                    Work::Build { .. } if forge.no_pull => None,
                    Work::Build { issue } => forge.pulls.insert(issue, Pr::Asked),
                    Work::Verify { issue, .. } => forge.pulls.insert(issue, Pr::Passed),
                    Work::Rebase { .. } if forge.no_push => None,
                    // The push of the rebase: a new head with no conflict
                    // and no verify.
                    Work::Rebase { issue, .. } => {
                        let head = format!("{}r", forge.head(issue));
                        forge.heads.insert(issue, head);
                        forge.conflicting.remove(&issue);
                        forge.pulls.insert(issue, Pr::Asked)
                    }
                    Work::Checks { .. } if forge.no_push => None,
                    // The push of the fix: a new head with no failed check
                    // and no verify.
                    Work::Checks { issue, .. } => {
                        let head = format!("{}f", forge.head(issue));
                        forge.heads.insert(issue, head);
                        forge.failing.remove(&issue);
                        forge.pulls.insert(issue, Pr::Asked)
                    }
                };
                forge.claims.remove(&record.live.item);
                forge.finished.push(record.live.item.clone());
                Ok(Vec::new())
            })
        }

        fn fate<'a>(&'a self, _: &'a str, issue: u64) -> BoxFuture<'a, Result<Fate>> {
            let fate = lock(&self.forge).fates.get(&issue).cloned();
            Box::pin(async move { Ok(fate.unwrap_or(Fate::Stays)) })
        }

        fn merged_in<'a>(
            &'a self,
            _: &'a str,
            _: u64,
            _: u64,
            _: &'a str,
        ) -> BoxFuture<'a, Result<()>> {
            Box::pin(async { Ok(()) })
        }

        fn tell<'a>(&'a self, _: &'a Wave, body: &'a str) -> BoxFuture<'a, ()> {
            Box::pin(async move { lock(&self.forge).told.push(body.to_owned()) })
        }

        fn gh(&self, _: &str) -> Arc<pr::Gh> {
            self.gh.clone()
        }

        fn claims<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<HashSet<String>>> {
            let claims = lock(&self.forge).claims.keys().cloned().collect();
            Box::pin(async move { Ok(claims) })
        }

        fn post<'a>(
            &'a self,
            _: &'a Wave,
            item: Option<&'a str>,
            body: &'a str,
            kind: Kind,
        ) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
                let mut forge = lock(&self.forge);
                if forge.post_fails > 0 {
                    forge.post_fails -= 1;
                    bail!("riff-server does not answer");
                }
                forge
                    .posts
                    .push((item.map(str::to_owned), kind, body.to_owned()));
                Ok(())
            })
        }

        fn chat<'a>(&'a self, _: &'a Record, _: &'a str) -> BoxFuture<'a, ()> {
            Box::pin(async {})
        }

        fn leave<'a>(&'a self, _: &'a Record) -> BoxFuture<'a, ()> {
            Box::pin(async {})
        }

        fn free<'a>(&'a self, _: &'a Record) -> BoxFuture<'a, Result<()>> {
            Box::pin(async { Ok(()) })
        }

        fn wake<'a>(&'a self, _: &'a Record) -> BoxFuture<'a, Result<String>> {
            Box::pin(std::future::pending())
        }

        fn wakes<'a>(&'a self, _: &'a Record, _: mpsc::UnboundedSender<Wake>) -> BoxFuture<'a, ()> {
            Box::pin(std::future::pending())
        }

        fn refuse<'a>(&'a self, _: &'a Record, _: &'a Wake) -> BoxFuture<'a, ()> {
            Box::pin(async {})
        }
    }

    /// A daemon on the fake runtime with the fake world, in a temp
    /// folder.
    struct Here {
        _root: tempfile::TempDir,
        fake: Fake,
        world: Arc<Fakes>,
        daemon: Arc<Daemon<Fake>>,
    }

    impl Here {
        /// The wave of the open `items`, with the limit `sessions` and
        /// a floor of 8 GB, and the free memory of `free` in GB.
        async fn new(items: &[u64], sessions: u32, free: Arc<AtomicU64>) -> Here {
            Here::new_looking(items, sessions, free, crate::look::LOOK_EVERY).await
        }

        /// [`Here::new`] with a look at `main` and the pull requests each
        /// `every`.
        async fn new_looking(
            items: &[u64],
            sessions: u32,
            free: Arc<AtomicU64>,
            every: Duration,
        ) -> Here {
            let root = tempfile::tempdir().unwrap();
            let dir = dir_in(root.path());
            let fake = Fake::default().with_image(IMAGE);
            let bin = root.path().join("bin");
            std::fs::create_dir(&bin).unwrap();
            let gh = pr::Gh::at(isolated::write_program(bin.join("gh"), FAKE_GH));
            let world = Arc::new(Fakes {
                root: root.path().to_owned(),
                forge: Mutex::new(Forge {
                    items: items.to_vec(),
                    ..Forge::default()
                }),
                gh: Arc::new(gh),
            });
            let memory: crate::room::Memory =
                Arc::new(move || Some(free.load(Ordering::SeqCst) * GB));
            let relay = root.path().join("static-riff");
            riff_client::secrets::mock_store();
            let daemon = Daemon::new(fake.clone(), dir.clone())
                .unwrap()
                .with_memory(memory)
                .with_world(world.clone())
                .with_look_every(every)
                .with_relay(crate::release::fake_riff(&relay, None));
            Limit {
                sessions,
                floor_gb: 8,
                ..Limit::default()
            }
            .write(&dir)
            .unwrap();
            Here {
                _root: root,
                fake,
                world,
                daemon: Arc::new(daemon),
            }
        }

        fn run(&self) {
            self.daemon
                .run_wave(Wave {
                    repo: "o/r".into(),
                    main: "/src/r".into(),
                    server: "http://127.0.0.1:1".into(),
                    host: "thelio".into(),
                })
                .unwrap();
        }

        fn forge(&self) -> MutexGuard<'_, Forge> {
            lock(&self.world.forge)
        }

        /// Waits until `done` holds. It tells the wave to look again
        /// while it waits. At most `limit` sessions live the whole time.
        async fn until(&self, limit: usize, done: impl Fn(&Here) -> bool) {
            let started = Instant::now();
            while !done(self) {
                assert!(self.daemon.live_count() <= limit, "more than {limit} live");
                assert!(started.elapsed() < isolated::HANG, "it did not happen");
                self.daemon.tell_changed();
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert!(self.daemon.live_count() <= limit, "more than {limit} live");
        }

        /// The name of the container of `item`, while it runs.
        fn running(&self, item: &str) -> Option<String> {
            self.daemon
                .records()
                .into_iter()
                .find(|r| r.live.item == item)
                .map(|r| r.live.name)
        }

        /// Ends the live session of `item`.
        async fn end(&self, item: &str) {
            let name = self.running(item).expect("it runs");
            self.fake.stop(&name).await.unwrap();
        }

        /// Lets the wave look for a time. At most `limit` sessions live.
        async fn idle(&self, limit: usize) {
            for _ in 0..20 {
                self.daemon.tell_changed();
                tokio::time::sleep(Duration::from_millis(20)).await;
                assert!(self.daemon.live_count() <= limit, "more than {limit} live");
            }
        }
    }

    #[tokio::test]
    async fn the_limit_of_the_host_holds_while_the_wave_runs() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2, 3], 2, free).await;
        here.run();
        here.until(2, |h| h.daemon.live_count() == 2).await;
        here.idle(2).await;
        assert_eq!(here.forge().starts.len(), 2);
        // Each end makes room for the next work: the verify of an item,
        // or the next item. Two sessions live at most.
        for n in 1..=6 {
            here.until(2, |h| !h.daemon.records().is_empty()).await;
            let item = here.daemon.records()[0].live.item.clone();
            here.end(&item).await;
            here.until(2, |h| h.forge().finished.len() == n).await;
        }
        here.idle(2).await;
        let forge = here.forge();
        assert!(
            forge.pulls.values().all(|pr| *pr == Pr::Passed),
            "{forge:?}"
        );
        assert_eq!(
            forge.starts.len(),
            6,
            "one session for each item and each verify"
        );
    }

    /// 01M53S7WDPQ2GWHJWJ6SZXZ5PN: after the end of a session, the daemon
    /// has no record of it, and the wave sweeps before each look. The
    /// folder still stays until the last step after the end of the job,
    /// which reads the folder.
    #[tokio::test]
    async fn the_folder_of_a_job_stays_until_its_last_step_after_the_end() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 1, free).await;
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        {
            let mut forge = here.forge();
            forge.git = true;
            forge.gate = Some(gate.clone());
            forge.fates.insert(1, Fate::Closed { pull: 41 });
        }
        here.run();
        here.until(1, |h| h.running("issue-1").is_some()).await;
        let folder = here.daemon.records()[0].folder.clone();
        here.end("issue-1").await;
        let finishing = |h: &Here| h.forge().finishing.contains(&"issue-1".to_owned());
        here.until(1, finishing).await;
        here.idle(1).await;

        assert!(here.running("issue-1").is_none());
        assert!(folder.exists(), "the steps after the end run");
        assert!(here.daemon.sweep(None).await.is_empty());

        gate.add_permits(1);
        here.until(1, |h| h.forge().finished.contains(&"issue-1".to_owned()))
            .await;
        here.until(1, |_| !folder.exists()).await;
    }

    /// 01M51STPHXSSZMHF9T7Z85097R: the wave of the daemon takes the
    /// merged pin of its main clone, not the pin of its working tree.
    #[tokio::test]
    async fn the_wave_takes_the_merged_pin_of_its_main_clone() {
        let pin = |n: &str| format!("ghcr.io/o/i@sha256:{}", n.repeat(64));
        let config = |n: &str| format!("[session]\nimage = \"{}\"\n", pin(n));
        let (_dir, main, _origin) = session_image::test_clone(&config("0"));
        std::fs::write(main.join(session_image::FILE), config("1")).unwrap();
        let wave = Wave {
            repo: "acme/app".into(),
            main,
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        assert_eq!(Live::default().image(&wave).await.unwrap(), pin("0"));
    }

    #[tokio::test]
    async fn the_wave_starts_nothing_while_the_free_memory_is_below_the_floor() {
        let free = Arc::new(AtomicU64::new(2));
        let here = Here::new(&[1], 3, free.clone()).await;
        here.run();
        here.idle(0).await;
        assert!(here.forge().starts.is_empty());
        free.store(9, Ordering::SeqCst);
        here.until(1, |h| h.daemon.live_count() == 1).await;
    }

    /// 01M5R56GN3DC09GAPAF69GENEZ (#1404): a default branch that does not
    /// read is the error of the look. The wave starts no work at that
    /// look, says why, and reads the branch again at the next look.
    #[tokio::test]
    async fn a_default_branch_that_does_not_read_starts_no_work_and_the_next_look_reads_it_again() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.forge().branch_unread = true;
        here.run();
        here.until(0, |h| h.forge().branch_reads >= 3).await;
        assert!(here.forge().starts.is_empty());
        let word = here.daemon.waves_shown()[0].word.clone();
        assert_eq!(word, text::wave_no_look("gh api repos/o/r: HTTP 502"));

        // The branch reads again at the next look: the work starts.
        let reads = here.forge().branch_reads;
        here.forge().branch_unread = false;
        here.until(1, |h| h.running("issue-1").is_some()).await;
        assert!(here.forge().branch_reads > reads);
        assert_eq!(here.forge().starts.len(), 1);
    }

    #[tokio::test]
    async fn a_held_item_never_starts() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        here.forge().holds.insert("issue-1".into());
        here.forge().holds.insert("verify-issue-2".into());
        here.run();
        here.until(3, |h| h.running("issue-2").is_some()).await;
        here.end("issue-2").await;
        here.until(3, |h| h.forge().finished == ["issue-2"]).await;
        here.idle(3).await;
        let forge = here.forge();
        let started: Vec<&str> = forge.starts.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(started, ["issue-2"], "no held item and no held verify");
        assert_eq!(forge.pulls.get(&2), Some(&Pr::Asked), "the verify waits");
    }

    /// A tracker issue, by its label or by its title, gets no build and
    /// no verify (01M5ND16V2SVN8A9K7W90VC6X9).
    #[tokio::test]
    async fn a_tracker_never_starts() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2, 3, 4], 4, free).await;
        {
            let mut forge = here.forge();
            forge.labels.insert(1, vec!["bug".into(), "tracker".into()]);
            forge.titles.insert(2, "Tracker: the cloud".into());
            forge.labels.insert(3, vec!["tracker".into()]);
            forge.pulls.insert(3, Pr::Asked);
        }
        here.run();
        here.until(4, |h| h.running("issue-4").is_some()).await;
        here.idle(4).await;
        let forge = here.forge();
        let started: Vec<&str> = forge.starts.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(started, ["issue-4"], "no build and no verify of a tracker");
    }

    /// 01M5R0HEP1MXWYY643F1N1DWEM: the wave starts no build of an issue
    /// whose body has no `Done when:` line, and the lead hears of it one
    /// time, not at each look. A verify is not changed. The wave takes
    /// the issue at its next look after the line comes.
    #[tokio::test]
    async fn an_issue_with_no_done_when_line_is_no_free_work_and_the_lead_hears_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2, 3], 4, free).await;
        {
            let mut forge = here.forge();
            forge.bare.extend([1, 3]);
            forge.pulls.insert(3, Pr::Asked);
        }
        here.run();
        here.until(4, |h| {
            h.running("issue-2").is_some() && h.running("verify-issue-3").is_some()
        })
        .await;
        here.idle(4).await;
        {
            let forge = here.forge();
            let mut started: Vec<&str> = forge.starts.iter().map(|(i, _)| i.as_str()).collect();
            started.sort_unstable();
            assert_eq!(
                started,
                ["issue-2", "verify-issue-3"],
                "no build of issue-1"
            );
            assert_eq!(forge.told, [text::wave_no_done_when(1)], "one message");
        }

        // The line comes: the same issue is free work at the next look.
        here.forge().bare.remove(&1);
        here.until(4, |h| h.running("issue-1").is_some()).await;
        here.idle(4).await;
        assert_eq!(here.forge().told.len(), 1, "no more message");
    }

    #[tokio::test]
    async fn an_author_never_verifies_its_own_item() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        // The author opens its pull request, and works on: no verify
        // starts while it holds the item.
        here.forge().pulls.insert(1, Pr::Asked);
        here.idle(3).await;
        assert_eq!(here.forge().starts.len(), 1);
        here.end("issue-1").await;
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        let forge = here.forge();
        let [(build, author), (verify, verifier)] = &forge.starts[..] else {
            panic!("{forge:?}");
        };
        assert_eq!(
            (build.as_str(), verify.as_str()),
            ("issue-1", "verify-issue-1")
        );
        assert_ne!(author, verifier, "the verify is a new session");
    }

    /// The reason of the end of a session that the fake daemon stops.
    fn stopped() -> String {
        "no exit code: a signal, or the container ended before its relay".into()
    }

    #[tokio::test]
    async fn an_item_starts_at_most_three_times_and_the_lead_hears_of_it() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        // Each session of the item ends with no pull request.
        here.forge().no_pull = true;
        here.run();
        for n in 1..=STARTS {
            here.until(3, |h| h.running("issue-1").is_some()).await;
            here.end("issue-1").await;
            here.until(3, |h| h.forge().finished.len() == n as usize)
                .await;
        }
        here.idle(3).await;
        let forge = here.forge();
        assert_eq!(forge.starts.len(), 3);
        assert_eq!(
            forge.told,
            [text::wave_spent("issue-1", STARTS, &stopped())]
        );
    }

    /// 01M558EYCXPFW8CJV3XJ7HK3NR: riff-server refuses the claim of each
    /// job for a pause, more times than [`STARTS`]. Such a start does not
    /// count, and the lead hears nothing of it: after the pause, the item
    /// starts.
    #[tokio::test]
    async fn a_claim_refused_for_a_pause_is_no_start_of_the_item() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        let pauses = STARTS as usize + 1;
        here.forge().refusals = vec!["paused".into(); pauses];
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        let forge = here.forge();
        assert_eq!(forge.refused, vec!["issue-1"; pauses]);
        assert_eq!(forge.starts.len(), 1);
        assert!(forge.told.is_empty(), "{:?}", forge.told);
    }

    /// 01M558EYCXPFW8CJV3XJ7HK3NR: each other refusal of the claim is a
    /// failed start: it counts, and the lead hears of it.
    #[tokio::test]
    async fn each_other_refusal_of_the_claim_counts_as_a_start() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.forge().refusals = vec!["must_clear".into(); STARTS as usize];
        here.run();
        let reason = text::wave_no_start("claim failed (409 Conflict): refused: must_clear");
        let spent = text::wave_spent("issue-1", STARTS, &reason);
        here.until(3, |h| h.forge().told.contains(&spent)).await;
        here.until(3, |h| h.forge().told.len() == STARTS as usize + 1)
            .await;
        here.idle(3).await;
        let forge = here.forge();
        assert_eq!(forge.refused.len(), STARTS as usize);
        assert!(forge.starts.is_empty(), "{forge:?}");
        let failed = text::wave_job_failed(
            "issue-1",
            "claim failed (409 Conflict): refused: must_clear",
        );
        let count = |text: &str| forge.told.iter().filter(|t| *t == text).count();
        assert_eq!(count(&failed), STARTS as usize, "{:?}", forge.told);
        assert_eq!(count(&spent), 1, "{:?}", forge.told);
    }

    /// 01M57A7A1W2K9440WZA1QZ3M9T: the sign-in of `claude` on the host
    /// ends in 42 minutes. No item starts, no start counts, and the lead
    /// gets one message. When the sign-in has 60 minutes or more, the
    /// wave starts the item.
    #[tokio::test]
    async fn a_sign_in_that_ends_soon_spends_no_start_and_the_lead_hears_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        here.forge().minutes = Some(42);
        here.run();
        let why = run::HostFault::SignInEnds(42).wave_text();
        let word = text::wave_host_word(&why);
        here.until(0, |h| h.daemon.waves_shown()[0].word == word)
            .await;
        // Many looks, and a sign-in with one minute less: still one
        // message.
        here.idle(0).await;
        here.forge().minutes = Some(41);
        here.idle(0).await;
        {
            let forge = here.forge();
            assert!(forge.starts.is_empty(), "{forge:?}");
            assert!(forge.refused.is_empty(), "{forge:?}");
            assert_eq!(forge.told, [text::wave_host_waits("o/r", &why)]);
            assert!(!forge.told[0].contains("riff run "), "{:?}", forge.told);
        }
        here.forge().minutes = Some(60);
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.until(3, |h| h.running("issue-2").is_some()).await;
        assert_eq!(here.forge().told.len(), 1, "{:?}", here.forge().told);
    }

    /// 01M58B7H7HD0H48A0C79F8M4CR: `claude` refused the token of the
    /// host. No item starts, no start counts, and the lead gets one
    /// message with the step. A new token starts the items.
    #[tokio::test]
    async fn a_refused_token_starts_nothing_and_the_lead_hears_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        here.forge().token_refused = true;
        here.run();
        let why = run::HostFault::TokenRefused.wave_text();
        let word = text::wave_host_word(&why);
        here.until(0, |h| h.daemon.waves_shown()[0].word == word)
            .await;
        here.idle(0).await;
        {
            let forge = here.forge();
            assert!(forge.starts.is_empty(), "{forge:?}");
            assert_eq!(forge.told, [text::wave_host_waits("o/r", &why)]);
            assert!(
                forge.told[0].contains("riff claude-token"),
                "{:?}",
                forge.told
            );
        }
        here.forge().token_refused = false;
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().told.len(), 1, "{:?}", here.forge().told);
    }

    /// 01M57A7A1W2K9440WZA1QZ3M9T: the sign-in ends soon after the check
    /// of the host of the loop, more times than [`STARTS`]. Such a start
    /// does not count, and the lead hears nothing of it.
    #[tokio::test]
    async fn a_fault_of_the_host_in_a_job_is_no_start_of_the_item() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        let faults = STARTS as usize + 1;
        here.forge().late_faults = faults;
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        let forge = here.forge();
        assert_eq!(forge.refused, vec!["issue-1"; faults]);
        assert_eq!(forge.starts.len(), 1);
        assert!(forge.told.is_empty(), "{:?}", forge.told);
    }

    /// The lead of `o/r` on this host, as the record of its session.
    fn lead_record() -> Record {
        let wave = Wave {
            repo: "o/r".into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        let who = riff_core::name::Who::new("mike", Some("lead1")).unwrap();
        Record {
            live: riff_client::sessions::Live {
                name: "riff-lead-o-r".into(),
                session: "lead1".into(),
                item: "lead".into(),
                image: IMAGE.into(),
                since_ms: 0,
                phase: Default::default(),
                last: String::new(),
                lead: true,
            },
            folder: PathBuf::new(),
            server: wave.server.clone(),
            me: SessionUri::new(who, wave.place(None).unwrap()),
            inputs: Vec::new(),
            closed: false,
            failed: false,
            job: None,
            terminal: true,
            takeover: false,
            interrupt: false,
        }
    }

    /// The wave of `o/r` in the file of the daemon, with no loop.
    fn keep_wave(here: &Here) {
        let wave = Wave {
            repo: "o/r".into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        write_waves(here.daemon.dir(), &[("o/r".to_owned(), wave)].into()).unwrap();
    }

    /// #1338: `riff run ITEM` in the container of the lead. The daemon
    /// runs the item as a job of the wave, with no loop of the wave:
    /// the claim, the session and the steps after its end. The answer
    /// comes at once.
    #[tokio::test]
    async fn the_lead_runs_an_item_as_a_job_with_no_loop_of_the_wave() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 2, free).await;
        keep_wave(&here);
        let line = here.daemon.run_item(&lead_record(), "issue-5").unwrap();
        assert_eq!(line, serde_json::json!(text::run_asked("issue-5")));
        here.until(2, |h| h.running("issue-5").is_some()).await;
        assert_eq!(here.forge().works, [Work::Build { issue: 5 }]);
        // The item runs: a second ask is refused.
        let again = here.daemon.run_item(&lead_record(), "issue-5");
        assert_eq!(again.unwrap_err().to_string(), text::run_already("issue-5"));
        here.end("issue-5").await;
        here.until(2, |h| h.forge().finished == ["issue-5"]).await;
        assert_eq!(here.forge().pulls.get(&5), Some(&Pr::Asked));
        assert!(here.forge().told.is_empty(), "{:?}", here.forge().told);
    }

    /// #1338: the daemon refuses a session that is not the lead, an item
    /// that is not an issue, and a repository with no wave.
    #[tokio::test]
    async fn the_daemon_refuses_the_run_of_a_work_session_and_of_an_unknown_repository() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 2, free).await;
        let mut worker = lead_record();
        worker.terminal = false;
        let refused = here.daemon.run_item(&worker, "issue-5").unwrap_err();
        assert_eq!(refused.to_string(), text::DAEMON_RUN_ONLY_LEAD);
        let lead = lead_record();
        let refused = here.daemon.run_item(&lead, "verify-issue-5").unwrap_err();
        assert_eq!(
            refused.to_string(),
            text::run_not_an_issue("verify-issue-5")
        );
        let refused = here.daemon.run_item(&lead, "issue-5").unwrap_err();
        assert_eq!(refused.to_string(), text::run_no_wave("o/r"));
        assert!(here.forge().starts.is_empty());
    }

    /// #1338: the daemon serves the repository of the lead, and the wave
    /// is not on: the job uses the wave of the file of the repositories
    /// that it serves.
    #[tokio::test]
    async fn a_run_uses_the_repository_that_the_daemon_serves_when_no_wave_runs() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 2, free).await;
        let wave = Wave {
            repo: "o/r".into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        crate::daemon::remote::serve_in_file(here.daemon.dir(), &wave).unwrap();
        here.daemon.run_item(&lead_record(), "issue-5").unwrap();
        here.until(2, |h| h.running("issue-5").is_some()).await;
    }

    /// #1338: a job that the lead asked for has no next look of a loop.
    /// So each case that stops its start tells the lead why: a hold or
    /// another claim, a pause, a fault of the host, and an error. The
    /// start of the loop of a wave tells less.
    #[tokio::test]
    async fn the_lead_hears_why_the_run_of_an_item_did_not_start() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 2, free).await;
        keep_wave(&here);
        let lead = lead_record();
        let told = |n: usize| {
            let here = &here;
            async move {
                here.until(2, |h| h.forge().told.len() == n).await;
                here.forge().told[n - 1].clone()
            }
        };

        here.forge().holds.insert("issue-5".to_owned());
        here.daemon.run_item(&lead, "issue-5").unwrap();
        let body = told(1).await;
        assert_eq!(body, text::run_job_failed("issue-5", text::RUN_NOT_CLAIMED));
        here.forge().holds.clear();

        here.forge().refusals = vec!["paused".into()];
        here.daemon.run_item(&lead, "issue-5").unwrap();
        let body = told(2).await;
        assert_eq!(body, text::run_job_failed("issue-5", text::RUN_PAUSED));

        here.forge().late_faults = 1;
        here.daemon.run_item(&lead, "issue-6").unwrap();
        let body = told(3).await;
        let fault = run::HostFault::SignInEnds(42).wave_text();
        assert_eq!(body, text::run_job_failed("issue-6", &fault));

        here.forge().refusals = vec!["not_allowed".into()];
        here.daemon.run_item(&lead, "issue-7").unwrap();
        let body = told(4).await;
        assert!(
            body.starts_with("The riff daemon started no session for issue-7: ")
                && body.contains("not_allowed"),
            "{body}"
        );
        // None of them started a session, and none counted.
        assert!(here.forge().starts.is_empty());
    }

    /// #1338: when the loop of the wave runs, the job of a run joins its
    /// items, so the loop does not pick the item and a second ask meets
    /// the first at once.
    #[tokio::test]
    async fn a_run_joins_the_items_of_the_loop_of_the_wave() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 2, free).await;
        here.run();
        let lead = lead_record();
        here.daemon.run_item(&lead, "issue-5").unwrap();
        let again = here.daemon.run_item(&lead, "issue-5").unwrap_err();
        assert_eq!(again.to_string(), text::run_already("issue-5"));
        here.until(2, |h| h.running("issue-5").is_some()).await;
        assert_eq!(here.forge().starts.len(), 1);
    }

    /// 01M57A7A48A70Y7PTD95A3MBYC: a spent item starts again after a
    /// second `riff wave run`, and the lead hears of it again when it is
    /// spent again.
    #[tokio::test]
    async fn a_second_wave_run_clears_the_spent_starts() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.forge().no_pull = true;
        here.run();
        let mut n = 0;
        let mut once = async |here: &Here| {
            for _ in 0..STARTS {
                n += 1;
                here.until(3, |h| h.running("issue-1").is_some()).await;
                here.end("issue-1").await;
                here.until(3, |h| h.forge().finished.len() == n).await;
            }
            here.idle(3).await;
        };
        once(&here).await;
        assert_eq!(here.forge().starts.len(), 3);
        here.run();
        once(&here).await;
        let forge = here.forge();
        assert_eq!(forge.starts.len(), 6, "{forge:?}");
        let spent = text::wave_spent("issue-1", STARTS, &stopped());
        assert_eq!(forge.told, [spent.clone(), spent]);
    }

    /// The wave of the item 1, whose pull request #41 has the verify
    /// `pr`, auto-merge on and a conflict with `main` at `c1`.
    async fn conflict(pr: Pr) -> Here {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.forge().pulls.insert(1, pr);
        here.forge().conflicting.insert(1);
        here
    }

    fn rebase(commit: &str) -> Work {
        Work::Rebase {
            issue: 1,
            pull: 41,
            commit: commit.into(),
        }
    }

    /// 01M57FJ3WWMQSDBFRRHXN3SMFQ: a pull request with a passed verify
    /// and a conflict gets one rebase for its head. The same head gets no
    /// second session. A new head with a conflict is a new conflict.
    #[tokio::test]
    async fn a_passed_pull_request_with_a_conflict_gets_one_rebase_for_each_head() {
        let here = conflict(Pr::Passed).await;
        here.forge().no_push = true;
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [rebase("c1")]);
        here.end("issue-1").await;
        here.until(3, |h| h.forge().finished == ["issue-1"]).await;
        here.idle(3).await;
        {
            let forge = here.forge();
            assert_eq!(
                forge.works,
                [rebase("c1")],
                "the same head: no second session"
            );
            assert!(forge.told.is_empty(), "{:?}", forge.told);
        }
        // main moves again, and a person pushed a new head with a conflict.
        here.forge().heads.insert(1, "c1b".into());
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [rebase("c1"), rebase("c1b")]);
    }

    /// #1157: a FAIL for the head C, then a new verify request for C: the
    /// next start is a verify, not a build. With no new request, the
    /// item is free for a build.
    #[tokio::test]
    async fn a_new_verify_request_after_a_fail_starts_a_verify_not_a_build() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free.clone()).await;
        here.forge().pulls.insert(1, Pr::Reasked);
        here.run();
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1".into(),
        };
        assert_eq!(here.forge().works, [verify]);

        let here = Here::new(&[1], 3, free).await;
        here.forge().pulls.insert(1, Pr::Failed);
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [Work::Build { issue: 1 }]);
    }

    /// The message to the lead about the pull request #41 of the item 1
    /// at the head `commit`.
    fn stuck(commit: &str, why: rollout::Stuck) -> String {
        text::wave_stuck(41, "worktree-issue-1", "main", commit, &why)
    }

    /// 01M57FJ3WWMQSDBFRRHXN3SMFQ, #279: the conflict of a pull request
    /// with auto-merge off gets no rebase: no merge waits for it. The lead
    /// hears of the auto-merge, one time.
    #[tokio::test]
    async fn a_pull_request_with_auto_merge_off_gets_no_rebase() {
        let here = conflict(Pr::Passed).await;
        here.forge().manual.insert(1);
        here.run();
        here.idle(3).await;
        let forge = here.forge();
        assert!(forge.works.is_empty(), "{forge:?}");
        assert_eq!(forge.told, [stuck("c1", rollout::Stuck::AutoMergeOff)]);
    }

    /// #279: a pull request with a passed verify and auto-merge off never
    /// merges. The wave starts no session, and tells the lead one time for
    /// each head, with the reason and the command. A pull request with
    /// auto-merge on gets no message.
    #[tokio::test]
    async fn a_verified_pull_request_with_auto_merge_off_tells_the_lead_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Passed);
            forge.pulls.insert(2, Pr::Passed);
            forge.manual.insert(1);
        }
        here.run();
        here.idle(3).await;
        let message = stuck("c1", rollout::Stuck::AutoMergeOff);
        assert!(
            message.ends_with("gh pr merge --auto --squash 41"),
            "{message}"
        );
        assert_eq!(
            here.forge().told,
            std::slice::from_ref(&message),
            "once, for 1 only"
        );
        assert!(here.forge().works.is_empty());

        // A new head is a new state.
        here.forge().heads.insert(1, "c1b".into());
        here.idle(3).await;
        let second = stuck("c1b", rollout::Stuck::AutoMergeOff);
        assert_eq!(here.forge().told, [message, second]);
        assert!(here.forge().works.is_empty());
    }

    fn checks(commit: &str) -> Work {
        Work::Checks {
            issue: 1,
            pull: 41,
            commit: commit.into(),
            checks: vec!["Gate".into()],
        }
    }

    /// #279: a pull request with a passed verify and a failed check gets
    /// one build session for each head, with the name of the check. The
    /// same head gets no second session, and no message: the pull request
    /// is in the group `stuck`. A new head with a failed check is a new
    /// state.
    #[tokio::test]
    async fn a_failed_check_after_a_pass_gets_one_build_for_each_head() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Passed);
            forge.failing.insert(1);
            forge.no_push = true;
        }
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [checks("c1")]);
        here.end("issue-1").await;
        here.until(3, |h| h.forge().finished == ["issue-1"]).await;
        here.idle(3).await;
        {
            let forge = here.forge();
            assert_eq!(
                forge.works,
                [checks("c1")],
                "the same head: no second session"
            );
            assert!(forge.told.is_empty(), "{:?}", forge.told);
        }
        // A person pushed a new head, and its Gate failed too.
        here.forge().heads.insert(1, "c1b".into());
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [checks("c1"), checks("c1b")]);
    }

    /// #279: the fix pushes a new head with no failed check: the new head
    /// waits for its verify, and a new session verifies it.
    #[tokio::test]
    async fn the_fix_of_a_failed_check_is_verified_by_a_new_session() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.forge().pulls.insert(1, Pr::Passed);
        here.forge().failing.insert(1);
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.end("issue-1").await;
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1f".into(),
        };
        assert_eq!(here.forge().works, [checks("c1"), verify]);
    }

    /// #279: a pull request with a conflict and a failed check gets the
    /// rebase first. A held item gets no fix of its checks. A failed
    /// check with no verify yet, and a failed verify, are no fix of checks.
    #[tokio::test]
    async fn a_conflict_comes_before_the_failed_check_and_a_hold_stops_the_fix() {
        let here = conflict(Pr::Passed).await;
        here.forge().failing.insert(1);
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [rebase("c1")]);

        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2, 3], 3, free).await;
        {
            let mut forge = here.forge();
            for n in 1..=3 {
                forge.failing.insert(n);
            }
            forge.pulls.insert(1, Pr::Passed);
            forge.pulls.insert(2, Pr::Asked);
            forge.pulls.insert(3, Pr::Failed);
            forge.holds.insert("issue-1".into());
            forge.items.retain(|n| *n != 3);
        }
        here.run();
        here.until(3, |h| h.running("verify-issue-2").is_some())
            .await;
        here.idle(3).await;
        let forge = here.forge();
        let started: Vec<String> = forge.works.iter().map(Work::item).collect();
        assert_eq!(started, ["verify-issue-2"], "{:?}", forge.works);
    }

    /// #279: no session builds a release. A failed check of a release
    /// pull request tells the lead, one time, at any state of its verify.
    #[tokio::test]
    async fn a_failed_check_of_a_release_tells_the_lead_and_starts_no_session() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.labels.insert(1, vec!["release".into()]);
            forge.pulls.insert(1, Pr::Asked);
            forge.failing.insert(1);
        }
        here.run();
        here.idle(3).await;
        let forge = here.forge();
        assert!(forge.works.is_empty(), "{forge:?}");
        let checks = rollout::Stuck::Checks(vec!["Gate".into()]);
        assert_eq!(forge.told, [stuck("c1", checks)]);
    }

    /// 01M57FJ3WWMQSDBFRRHXN3SMFQ: a pull request that waits for a verify
    /// and has a conflict gets the rebase, not the verify of the old head.
    /// After the push, a new session verifies the new head: it wrote
    /// neither side.
    #[tokio::test]
    async fn the_rebase_comes_before_the_verify_and_a_new_session_verifies_the_new_head() {
        let here = conflict(Pr::Asked).await;
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.end("issue-1").await;
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        let forge = here.forge();
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1r".into(),
        };
        assert_eq!(forge.works, [rebase("c1"), verify]);
        let [(_, rebaser), (_, verifier)] = &forge.starts[..] else {
            panic!("{forge:?}");
        };
        assert_ne!(rebaser, verifier, "the verify is a new session");
    }

    /// 01M57FJ3WWMQSDBFRRHXN3SMFQ, 01M558EYCXPFW8CJV3XJ7HK3NR: a claim of
    /// the rebase that riff-server refused for a pause is no start: after
    /// the pause, the rebase of the same head starts.
    #[tokio::test]
    async fn a_rebase_refused_for_a_pause_starts_after_the_pause() {
        let here = conflict(Pr::Passed).await;
        here.forge().refusals = vec!["paused".into(); 2];
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        let forge = here.forge();
        assert_eq!(forge.refused, ["issue-1", "issue-1"]);
        assert_eq!(forge.works, [rebase("c1")]);
    }

    /// 01M57FJ3WWMQSDBFRRHXN3SMFQ, 01M57A7A48A70Y7PTD95A3MBYC: `riff wave
    /// run` clears the start of a rebase too: the same head starts again.
    #[tokio::test]
    async fn a_second_wave_run_starts_the_rebase_of_the_same_head_again() {
        let here = conflict(Pr::Passed).await;
        here.forge().no_push = true;
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.end("issue-1").await;
        here.until(3, |h| h.forge().finished.len() == 1).await;
        here.idle(3).await;
        assert_eq!(here.forge().works.len(), 1);
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [rebase("c1"), rebase("c1")]);
    }

    #[tokio::test]
    async fn a_stop_of_the_wave_starts_no_new_session_and_the_sessions_go_on() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 1, free).await;
        here.run();
        here.until(1, |h| h.running("issue-1").is_some()).await;
        assert!(here.daemon.stop_wave("o/r").unwrap());
        assert!(here.running("issue-1").is_some(), "the session goes on");
        here.end("issue-1").await;
        here.until(1, |h| h.forge().finished == ["issue-1"]).await;
        here.idle(1).await;
        assert_eq!(here.forge().starts.len(), 1);
        assert!(!here.daemon.stop_wave("o/r").unwrap());
    }

    /// The run of the Gate in the fake forge.
    const RUN: &str = "https://github.com/o/r/actions/runs/38861280871";

    /// A fake `gh` for the look: it logs each call to `gh.log` on one
    /// line. The file `gate` holds the check runs of the head of
    /// `main`, `jobs` the jobs of its run, `pulls` the open pull
    /// requests, and `issues` the open issues. `issue list` gives the file
    /// `with-fix` after `issue create` and before `issue close`. A file
    /// that is missing gives `[]`: no Gate, no pull request. The repository
    /// `o/r` has `main` as its default branch. The file `rules` holds the
    /// rules of `main`, and the file `docs` the check runs of the check
    /// `checks / docs` on any commit but `main`. The other checks of a
    /// commit passed, and `Gate` has no run.
    const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s' "$*" | tr '\n' ' ' >> "$dir/gh.log"
printf '\n' >> "$dir/gh.log"
show() { if [ -e "$dir/$1" ]; then cat "$dir/$1"; else echo '[]'; fi; }
runs() { if [ -e "$dir/$1" ]; then cat "$dir/$1"; else echo '{"check_runs":[]}'; fi; }
case "$1 $2" in
  api*)
    case "$2" in
      repos/o/r)
        case "$*" in
          *--jq*) echo main ;;
          *) echo '{"default_branch":"main"}' ;;
        esac ;;
      */rules/branches/main*) show rules ;;
      */commits/main/check-runs*) runs gate ;;
      *check_name=Gate) echo '{"check_runs":[]}' ;;
      *check_name=checks%20%2F%20docs) show docs ;;
      *check_name=checks%20%2F%20*) echo '{"check_runs":[{"status":"completed","conclusion":"success"}]}' ;;
      */actions/runs/*/jobs*) show jobs ;;
      */milestones*) echo '[{"title":"Wave 1"}]' ;;
      *) exit 1 ;;
    esac ;;
  "pr list") show pulls ;;
  "issue list")
    if [ -e "$dir/created" ] && [ ! -e "$dir/closed" ]; then show with-fix; else show issues; fi ;;
  "label create") ;;
  "issue create") touch "$dir/created"; echo "https://github.com/o/r/issues/901" ;;
  "issue comment") ;;
  "issue close") touch "$dir/closed" ;;
  *) exit 1 ;;
esac
"#;

    /// The open issues of the fake forge, and the same with the fix item.
    const ISSUES: &str =
        r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[]}]"#;
    const WITH_FIX: &str = r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":901,"body":"Needs: nothing","comments":[],"milestone":{"title":"Wave 1"},"labels":[{"name":"broken-main"}]}]"#;

    /// The failed jobs of the run.
    const JOBS: &str =
        r#"[{"name":"Clippy","conclusion":"failure"},{"name":"Gate","conclusion":"failure"}]"#;

    /// The Gate of the head `commit` of `main` ended with `conclusion`.
    fn gate(commit: &str, conclusion: &str) -> String {
        format!(
            r#"{{"check_runs":[{{"status":"completed","conclusion":"{conclusion}","head_sha":"{commit}","html_url":"{RUN}/job/1"}}]}}"#
        )
    }

    /// The pull request of issue 12 with auto-merge on and a conflict, at
    /// the head `head`.
    fn conflicting(head: &str) -> String {
        format!(
            r#"[{{"number":52,"headRefName":"worktree-issue-12","headRefOid":"{head}","isDraft":false,"statusCheckRollup":[],"mergeable":"CONFLICTING","autoMergeRequest":{{"enabledAt":"2026-10-24T00:00:00Z"}},"body":"Closes #12\n"}}]"#
        )
    }

    impl Here {
        /// A look each 20 ms, with the fake forge of `gh` and no work.
        async fn looking() -> Here {
            let free = Arc::new(AtomicU64::new(64));
            let here = Here::new_looking(&[], 1, free, Duration::from_millis(20)).await;
            here.put("issues", ISSUES);
            here.put("with-fix", WITH_FIX);
            here.put("jobs", JOBS);
            here
        }

        /// Sets the file `name` of the fake forge.
        fn put(&self, name: &str, text: &str) {
            std::fs::write(self.world.root.join("bin").join(name), text).unwrap();
        }

        /// The calls of the fake `gh` that contain `call`.
        fn calls(&self, call: &str) -> Vec<String> {
            std::fs::read_to_string(self.world.root.join("bin/gh.log"))
                .unwrap_or_default()
                .lines()
                .filter(|line| line.contains(call))
                .map(str::to_owned)
                .collect()
        }

        /// The number of looks at the Gate of `main` so far.
        fn looks(&self) -> usize {
            self.calls("/commits/main/check-runs").len()
        }

        /// Waits for `n` more looks.
        async fn looks_more(&self, n: usize) {
            let end = self.looks() + n;
            self.until(0, |h| h.looks() >= end).await;
        }

        /// Each post of the look.
        fn posts(&self) -> Vec<(Option<String>, Kind, String)> {
            self.forge().posts.clone()
        }
    }

    /// #698, 01M5QYDE41FVFX26EC8K5QGYH4, 01M5QYDHN27RC5SRHAB56PFKK0: a
    /// repository with no check named `Gate`. Its ruleset requires the
    /// checks `checks / lint` and `checks / docs`. The wave starts no
    /// verify while one of them runs, and starts it when both passed. It
    /// never asks for the check `Gate`.
    #[tokio::test]
    async fn the_wave_starts_the_verify_when_the_required_checks_passed_and_the_repository_has_no_gate()
     {
        let here = Here::looking().await;
        here.put(
            "rules",
            r#"[{"type":"required_status_checks","parameters":{"required_status_checks":[
                {"context":"checks / lint"},{"context":"checks / docs"},{"context":"riff/verify"}]}}]"#,
        );
        here.put(
            "docs",
            r#"{"check_runs":[{"status":"in_progress","conclusion":null}]}"#,
        );
        {
            let mut forge = here.forge();
            forge.checked = true;
            forge.pulls.insert(1, Pr::Asked);
        }
        here.run();

        // The wave looks at the checks of the head again and again, and
        // starts no verify.
        here.until(0, |h| !h.calls("check_name=checks%20%2F%20docs").is_empty())
            .await;
        here.idle(0).await;
        assert!(here.forge().starts.is_empty(), "{:?}", here.forge());

        here.put(
            "docs",
            r#"{"check_runs":[{"status":"completed","conclusion":"success"}]}"#,
        );
        here.until(1, |h| h.running("verify-issue-1").is_some())
            .await;
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1".into(),
        };
        assert_eq!(here.forge().works, [verify]);
        // Neither the pull request nor the watch of `main` asks for the
        // check `Gate`: both read the required checks.
        assert!(here.calls("check_name=Gate").is_empty());
        assert!(!here.calls("/rules/branches/main").is_empty());
    }

    /// The look of the daemon at `main` (#1351, 01M5QVH3MAEPPJN9PPVDX6N2EY,
    /// 01M51QEYKTD7ZEB0YA0C3MQTPN, 01M51QEYPGD6W345MZ4F85C38F,
    /// 01M51QEYVQN74Q1VDTVHB0445Q): the Gate of the head of `main` fails
    /// at a new commit, and the lead gets one message and riff files the
    /// fix item. It passes again, and riff closes the item.
    #[tokio::test]
    async fn the_daemon_tells_the_lead_when_main_breaks_and_closes_the_fix_item_when_it_passes() {
        let here = Here::looking().await;
        here.put("gate", &gate("c0ffee1234", "success"));
        here.run();
        here.looks_more(3).await;
        assert!(here.posts().is_empty());

        here.put("gate", &gate("5103311abcdef", "failure"));
        here.until(0, |h| h.posts().len() == 1).await;
        let (to, kind, body) = here.posts().remove(0);
        assert_eq!((to, kind), (None, Kind::Message));
        assert_eq!(
            body,
            format!(
                "main is broken: the full check failed at commit 5103311 ({RUN}). Failed jobs: Clippy. \
                 The fix item is #901: the rollout gives it to a worker before each other item. \
                 When a pull request already has the fix, hold #901."
            )
        );
        let created = here.calls("issue create");
        assert_eq!(created.len(), 1, "{created:?}");
        assert!(
            created[0].contains("--label broken-main") && created[0].contains("--milestone Wave 1"),
            "{created:?}"
        );

        // The same Gate at the next looks: no message, no second fix item.
        here.looks_more(3).await;
        assert_eq!(here.posts().len(), 1);
        assert_eq!(here.calls("issue create").len(), 1);
        assert!(here.calls("issue comment").is_empty());

        // The fix merges, and the Gate passes: riff closes the fix item.
        here.put("gate", &gate("d00d1234", "success"));
        here.until(0, |h| h.posts().len() == 2).await;
        let (to, kind, body) = here.posts().remove(1);
        assert_eq!((to, kind), (None, Kind::Note));
        assert_eq!(
            body,
            format!(
                "main passes again: the full check passed at commit d00d123 ({RUN}). riff closed the \
                 fix item #901."
            )
        );
        assert_eq!(here.calls("issue close 901").len(), 1);
        here.looks_more(3).await;
        assert_eq!(here.posts().len(), 2);
        assert_eq!(here.calls("issue close").len(), 1);
    }

    /// #1393, 01M5R13DGMTNWPY88A3EG49WWS: the repository has no check
    /// named `Gate`. Its ruleset requires `checks / test`. A failure of
    /// `checks / test` on the head of `main` tells the lead one time and
    /// files the fix item, and the daemon never asks for the check `Gate`.
    #[tokio::test]
    async fn the_daemon_watches_main_with_the_required_checks_and_not_the_gate() {
        let here = Here::looking().await;
        here.put(
            "rules",
            r#"[{"type":"required_status_checks","parameters":{"required_status_checks":[
                {"context":"checks / test"},{"context":"riff/verify"}]}}]"#,
        );
        here.put(
            "jobs",
            r#"[{"name":"checks / test","conclusion":"failure"}]"#,
        );
        here.put("gate", &gate("5103311abcdef", "failure"));
        here.run();
        here.until(0, |h| h.posts().len() == 1).await;
        let (to, kind, body) = here.posts().remove(0);
        assert_eq!((to, kind), (None, Kind::Message));
        assert_eq!(
            body,
            format!(
                "main is broken: the full check failed at commit 5103311 ({RUN}). Failed jobs: \
                 checks / test. The fix item is #901: the rollout gives it to a worker before \
                 each other item. When a pull request already has the fix, hold #901."
            )
        );
        let created = here.calls("issue create");
        assert_eq!(created.len(), 1, "{created:?}");
        assert!(
            created[0].contains("--title main is broken at 5103311: the full check failed"),
            "{created:?}"
        );

        // The same failure at the next looks: no message, no second fix item.
        here.looks_more(3).await;
        assert_eq!(here.posts().len(), 1);
        assert_eq!(here.calls("issue create").len(), 1);
        assert!(
            !here
                .calls("/commits/main/check-runs?check_name=checks%20%2F%20test")
                .is_empty()
        );
        assert!(here.calls("check_name=Gate").is_empty());
    }

    /// A post that does not go out comes again at the next look, and
    /// riff files no second fix item for it (01M51QEYPGD6W345MZ4F85C38F).
    #[tokio::test]
    async fn a_message_that_did_not_go_out_comes_at_the_next_look_with_one_fix_item() {
        let here = Here::looking().await;
        here.forge().post_fails = 1;
        here.put("gate", &gate("5103311abcdef", "failure"));
        here.run();
        here.until(0, |h| h.posts().len() == 1).await;
        assert!(here.posts()[0].2.contains("The fix item is #901"));
        assert_eq!(here.calls("issue create").len(), 1);
        assert_eq!(here.calls("issue comment 901").len(), 1);
        here.looks_more(3).await;
        assert_eq!(here.posts().len(), 1);
    }

    /// A pull request with a conflict gives one message for each head:
    /// to the session that holds the item, else to the lead
    /// (01M49Q30XMVRFX42YTM1PHX0RZ, 01M49Q31FASDM7CG3JEGPYCZB9).
    #[tokio::test]
    async fn the_daemon_tells_the_holder_or_the_lead_of_a_conflict_once_for_each_head() {
        let here = Here::looking().await;
        here.forge()
            .claims
            .insert("issue-12".into(), "1a2b3c4d".into());
        here.put("pulls", &conflicting("1a2b3c4d5e"));
        here.run();
        here.until(0, |h| h.posts().len() == 1).await;
        let (to, kind, body) = here.posts().remove(0);
        assert_eq!((to.as_deref(), kind), (Some("issue-12"), Kind::Message));
        assert!(
            body.contains("Pull request #52 of issue-12 has a conflict")
                && body.contains("at commit 1a2b3c4,"),
            "{body}"
        );
        here.looks_more(3).await;
        assert_eq!(here.posts().len(), 1);

        // A new head with a conflict, and no holder: the lead.
        here.forge().claims.clear();
        here.put("pulls", &conflicting("5e6f7a8b9c"));
        here.until(0, |h| h.posts().len() == 2).await;
        let (to, _, body) = here.posts().remove(1);
        assert_eq!(to, None);
        assert!(body.contains("No session holds issue-12"), "{body}");
    }

    /// `riff wave stop` ends the look with the loop of the wave.
    #[tokio::test]
    async fn a_stop_of_the_wave_ends_its_look() {
        let here = Here::looking().await;
        here.run();
        here.looks_more(2).await;
        assert!(here.daemon.stop_wave("o/r").unwrap());
        // A call in flight can still end.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let looks = here.looks();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(here.looks(), looks);
    }

    #[tokio::test]
    async fn a_new_ask_with_another_main_clone_runs_the_wave_with_it() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 3, free).await;
        here.run();
        let other = Wave {
            repo: "o/r".into(),
            main: "/src/other".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        here.daemon.run_wave(other.clone()).unwrap();
        let kept = read_waves(here.daemon.dir()).unwrap();
        assert_eq!(kept.get("o/r"), Some(&other));
        assert_eq!(here.daemon.waves_shown().len(), 1);
        here.until(0, |h| h.daemon.waves_shown()[0].word == text::WAVE_NO_WORK)
            .await;
    }

    #[test]
    fn a_wave_goes_to_its_file_and_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_waves(dir.path()).unwrap().is_empty());
        let wave = Wave {
            repo: "o/r".into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        let waves: BTreeMap<String, Wave> = [("o/r".to_owned(), wave)].into();
        write_waves(dir.path(), &waves).unwrap();
        assert_eq!(read_waves(dir.path()).unwrap(), waves);
    }

    #[test]
    fn the_pick_skips_a_verify_until_its_gate_is_done_and_counts_the_starts() {
        let waiting = |gate_done| Waiting {
            pull: 40,
            commit: "c".into(),
            gate_done,
        };
        let look = Look {
            free: vec!["verify-issue-9".into(), "issue-12".into()],
            pulls: [(9, waiting(false))].into(),
            ..Look::default()
        };
        let none = BTreeSet::new();
        assert_eq!(
            pick(&look, &none, &BTreeMap::new()),
            Some(Work::Build { issue: 12 })
        );
        let spent: BTreeMap<String, u32> = [("issue-12".to_owned(), STARTS)].into();
        assert_eq!(pick(&look, &none, &spent), None);
        let running: BTreeSet<String> = ["issue-12".to_owned()].into();
        assert_eq!(pick(&look, &running, &BTreeMap::new()), None);
        let look = Look {
            pulls: [(9, waiting(true))].into(),
            ..look
        };
        assert_eq!(
            pick(&look, &none, &BTreeMap::new()),
            Some(Work::Verify {
                issue: 9,
                pull: 40,
                commit: "c".into()
            })
        );
        // The author of 9 runs in this daemon.
        let running: BTreeSet<String> = ["issue-9".to_owned()].into();
        assert_eq!(
            pick(&look, &running, &BTreeMap::new()),
            Some(Work::Build { issue: 12 })
        );
    }

    #[test]
    fn a_job_goes_into_the_record_and_comes_back() {
        let job = Job {
            wave: Wave {
                repo: "o/r".into(),
                main: "/src/r".into(),
                server: "http://127.0.0.1:1".into(),
                host: "thelio".into(),
            },
            work: Work::Verify {
                issue: 12,
                pull: 40,
                commit: "1a2b".into(),
            },
        };
        let text = serde_json::to_string(&job).unwrap();
        assert!(text.contains(r#""kind":"verify""#), "{text}");
        assert_eq!(serde_json::from_str::<Job>(&text).unwrap(), job);
    }

    /// Runs `git ARGS` in `dir` (with an identity and no signing), and
    /// gives its output.
    fn git_in(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// Commits a change of the file `a` in `dir`, and gives the commit.
    fn commit_a(dir: &Path, words: &str) -> String {
        std::fs::write(dir.join("a"), words).unwrap();
        git_in(dir, &["add", "a"]);
        git_in(dir, &["commit", "-q", "-m", words]);
        git_in(dir, &["rev-parse", "HEAD"])
    }

    /// The git of a test of [`clone_at`]: a bare `origin` that a `file://`
    /// URL names, as a URL of GitHub does (a path would copy each
    /// object), a main clone of it, and the head commit of the pull
    /// request 7 of a fork. `origin` has that commit only on
    /// `refs/pull/7/head`, as GitHub has it.
    struct Fork {
        dir: tempfile::TempDir,
        main: PathBuf,
        /// The work tree of the fork.
        fork: PathBuf,
        head: String,
    }

    impl Fork {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let (origin, fork, main) = (
                dir.path().join("origin.git"),
                dir.path().join("fork"),
                dir.path().join("main"),
            );
            let url = format!("file://{}", origin.display());
            std::fs::create_dir(&fork).unwrap();
            git_in(
                dir.path(),
                &["init", "-q", "--bare", "-b", "main", "origin.git"],
            );
            git_in(&fork, &["init", "-q", "-b", "main"]);
            commit_a(&fork, "one");
            git_in(&fork, &["remote", "add", "origin", &url]);
            git_in(&fork, &["push", "-q", "origin", "main"]);
            git_in(dir.path(), &["clone", "-q", "--", &url, "main"]);
            git_in(&fork, &["switch", "-q", "-c", "patch"]);
            let head = commit_a(&fork, "two");
            git_in(&fork, &["push", "-q", "origin", "HEAD:refs/pull/7/head"]);
            Fork {
                dir,
                main,
                fork,
                head,
            }
        }

        /// Whether the repository `repo` has the commit `commit`.
        fn has(repo: &Path, commit: &str) -> bool {
            std::process::Command::new("git")
                .args(["cat-file", "-e", &format!("{commit}^{{commit}}")])
                .current_dir(repo)
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        }
    }

    /// 01M5QVARWYFHTQV5THNRB09SZA: the head commit of a pull request
    /// from a fork is on no branch of `origin`. The clone of the verify
    /// fetches `refs/pull/N/head` and checks out the commit, detached.
    #[test]
    fn the_clone_of_a_verify_has_the_head_commit_of_a_pull_request_from_a_fork() {
        let fork = Fork::new();
        assert!(!Fork::has(&fork.main, &fork.head), "origin has no branch");
        let clone = fork.dir.path().join("verify/clone");
        clone_at(&fork.main, &clone, 7, &fork.head).unwrap();
        assert_eq!(git_in(&clone, &["rev-parse", "HEAD"]), fork.head);
        assert_eq!(git_in(&clone, &["branch", "--show-current"]), "");
        assert_eq!(std::fs::read_to_string(clone.join("a")).unwrap(), "two");
    }

    /// 01M5QVARWYFHTQV5THNRB09SZA: a push to the pull request after the
    /// look moves `refs/pull/N/head`. The clone still has the commit of
    /// the look.
    #[test]
    fn a_push_to_the_pull_request_after_the_look_does_not_change_the_commit_of_the_clone() {
        let fork = Fork::new();
        commit_a(&fork.fork, "three");
        git_in(
            &fork.fork,
            &["push", "-q", "origin", "HEAD:refs/pull/7/head"],
        );
        let clone = fork.dir.path().join("verify/clone");
        clone_at(&fork.main, &clone, 7, &fork.head).unwrap();
        assert_eq!(git_in(&clone, &["rev-parse", "HEAD"]), fork.head);
        assert_eq!(std::fs::read_to_string(clone.join("a")).unwrap(), "two");
    }

    /// 01M5QVARWYFHTQV5THNRB09SZA: a pull request that `origin` does not
    /// have fails, and the error names the ref.
    #[test]
    fn the_clone_of_a_verify_fails_with_the_ref_when_origin_does_not_have_it() {
        let fork = Fork::new();
        let clone = fork.dir.path().join("verify/clone");
        let error = clone_at(&fork.main, &clone, 8, &fork.head).unwrap_err();
        assert!(error.to_string().contains("refs/pull/8/head"), "{error}");
    }

    /// The riff of a test of [`after_build`]: a riff-server in this
    /// process, the lead `mike/lead1` of `o/r`, and the session of a
    /// build of `issue-12` that ended.
    struct Built {
        api: Api,
        wave: Wave,
        lead: SessionUri,
        record: Record,
        dir: tempfile::TempDir,
    }

    impl Built {
        async fn new() -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let server = format!("http://{}", listener.local_addr().unwrap());
            tokio::spawn(async move {
                axum::serve(listener, riff_server::router()).await.unwrap();
            });
            let api = Api::new(&server);
            let wave = Wave {
                repo: "o/r".into(),
                main: "/src/r".into(),
                server: server.clone(),
                host: "thelio".into(),
            };
            let who = |session| riff_core::name::Who::new("mike", Some(session)).unwrap();
            let lead = SessionUri::new(who("lead1"), wave.place(None).unwrap());
            api.start(&lead, StartReason::Process, false).await.unwrap();
            let session = "9f8e7d6c-0000-4000-8000-000000000000";
            let me = SessionUri::new(who(session), wave.place(Some("issue-12")).unwrap());
            api.start(&me, StartReason::Process, true).await.unwrap();
            let dir = tempfile::tempdir().unwrap();
            let record = Record {
                live: riff_client::sessions::Live {
                    name: "riff-issue-12".into(),
                    session: session.into(),
                    item: "issue-12".into(),
                    image: IMAGE.into(),
                    since_ms: 0,
                    phase: Default::default(),
                    last: String::new(),
                    lead: false,
                },
                folder: dir.path().to_owned(),
                server,
                me,
                inputs: Vec::new(),
                closed: true,
                failed: false,
                job: Some(Job {
                    wave: wave.clone(),
                    work: Work::Build { issue: 12 },
                }),
                terminal: false,
                takeover: false,
                interrupt: false,
            };
            Self {
                api,
                wave,
                lead,
                record,
                dir,
            }
        }

        /// A fake `gh`: the branch of issue 12 has the open pull request
        /// #40, and the issue has the label `no-verify` when `labelled`.
        fn gh(&self, labelled: bool) -> pr::Gh {
            let labels = if labelled {
                r#"[{"name":"no-verify"}]"#
            } else {
                "[]"
            };
            let script = format!(
                r#"#!/bin/sh
case "$*" in
"api repos/o/r --jq .default_branch") echo main ;;
"pr view worktree-issue-12 --json number,url,headRefOid,state")
    echo '{{"number":40,"url":"https://github.com/o/r/pull/40","headRefOid":"1a2b3c4d5e","state":"OPEN"}}' ;;
"issue view 12 --json number,labels")
    echo '{{"number":12,"labels":{labels}}}' ;;
*)
    echo "the fake gh has no reply for: $*" >&2
    exit 1 ;;
esac
"#
            );
            pr::Gh::at(isolated::write_program(self.dir.path().join("gh"), script))
        }

        /// Runs [`after_build`] with `gh`, and gives each message that the
        /// lead can read, with its kind.
        async fn after_build(&self, gh: pr::Gh) -> Vec<(Kind, String)> {
            self.after(gh, None).await.1
        }

        /// [`Built::after_build`] after the fix `fix`, with the line of
        /// each step.
        async fn after(
            &self,
            gh: pr::Gh,
            fix: Option<Fix<'_>>,
        ) -> (Vec<String>, Vec<(Kind, String)>) {
            let lines = after_build(
                gh,
                &self.api,
                &self.record,
                &self.wave,
                12,
                (fix, None),
                Ended {
                    code: Some(1),
                    failed: true,
                },
            )
            .await
            .unwrap();
            let thread = self.wave.thread().unwrap();
            let got = self
                .api
                .read(&self.lead, &thread, true)
                .await
                .unwrap()
                .into_iter()
                .map(|c| (c.message.kind, c.message.body))
                .collect();
            (lines, got)
        }
    }

    /// 01M5A5HXKZTDP5M5HY2FHCEBZA: a build that ends with no open pull
    /// request leaves a record in the thread `ends`: its reason and the
    /// last lines of its output. `riff session why` reads it.
    #[tokio::test]
    async fn a_build_with_no_pull_request_leaves_the_reason_of_its_end() {
        let built = Built::new().await;
        std::fs::write(
            built.dir.path().join(crate::daemon::EVENTS),
            "{\"type\":\"assistant\"}\nlast line of the output\n",
        )
        .unwrap();
        let gh = pr::Gh::at(isolated::write_program(
            built.dir.path().join("gh"),
            "#!/bin/sh\nexit 1\n",
        ));
        let (lines, got) = built.after(gh, None).await;
        assert_eq!(lines, [text::run_no_pull("worktree-issue-12")]);
        assert_eq!(got.len(), 1, "only the note to the lead: {got:?}");
        let records = crate::ends::read(&built.api, &built.lead).await.unwrap();
        let [record] = &records[..] else {
            panic!("one record: {records:?}");
        };
        assert_eq!(record.tag("item"), "issue-12");
        assert_eq!(record.tag("session"), "9f8e7d6c");
        assert_eq!(record.reason, "exit code 1");
        assert_eq!(
            record.tail,
            ["{\"type\":\"assistant\"}", "last line of the output"]
        );
        let shown = crate::ends::why("issue-12", &records);
        assert!(shown.contains("ended: exit code 1."), "{shown}");
    }

    /// 01M5P26VSA7AA99K1RYG83R8J9: a build that pushed nothing, and that
    /// riff saved, gives the lead one note with the branch and the commit.
    #[tokio::test]
    async fn a_saved_build_tells_the_lead_the_branch_and_the_commit() {
        let built = Built::new().await;
        let gh = pr::Gh::at(isolated::write_program(
            built.dir.path().join("gh"),
            "#!/bin/sh\nexit 1\n",
        ));
        let ended = Ended {
            code: Some(0),
            failed: false,
        };
        after_build(
            gh,
            &built.api,
            &built.record,
            &built.wave,
            12,
            (None, Some("1a2b3c4")),
            ended,
        )
        .await
        .unwrap();
        let thread = built.wave.thread().unwrap();
        let got = built.api.read(&built.lead, &thread, true).await.unwrap();
        let [one] = &got[..] else {
            panic!("one message: {got:?}");
        };
        assert!(one.message.body.contains("worktree-issue-12"), "{one:?}");
        assert!(one.message.body.contains("1a2b3c4"), "{one:?}");
    }

    /// 01M5A5HXKZTDP5M5HY2FHCEBZA: a build with an open pull request
    /// leaves no record.
    #[tokio::test]
    async fn a_build_with_a_pull_request_leaves_no_record_of_its_end() {
        let built = Built::new().await;
        let gh = built.gh(false);
        built.after_build(gh).await;
        let records = crate::ends::read(&built.api, &built.lead).await.unwrap();
        assert!(records.is_empty(), "{records:?}");
    }

    /// With no hold and no label, the lead gets the verify request
    /// (01M4ZV3B6Q0Y6AX0KSKC55S31R).
    #[tokio::test]
    async fn a_build_sends_the_verify_request_to_the_lead() {
        let riff = Built::new().await;
        let got = riff.after_build(riff.gh(false)).await;
        assert_eq!(
            got,
            [(
                Kind::Message,
                text::wave_verify_request(
                    "issue-12",
                    40,
                    "worktree-issue-12",
                    "1a2b3c4",
                    "9f8e7d6c"
                )
            )]
        );
    }

    /// The head commit of the pull request is a WIP commit of a pause: the
    /// lead gets a note, and no verify request. "Pause the riff" in the book
    /// shows the same note.
    #[tokio::test]
    async fn a_build_that_ends_with_a_wip_head_sends_no_verify_request() {
        let riff = Built::new().await;
        let script = r#"#!/bin/sh
case "$*" in
"pr view worktree-issue-12 --json number,url,headRefOid,state")
    echo '{"number":40,"url":"https://github.com/o/r/pull/40","headRefOid":"1a2b3c4d5e","state":"OPEN"}' ;;
"api repos/o/r/commits/1a2b3c4d5e --jq .commit.message")
    printf 'WIP: the riff is paused\n\nThe session left the riff.\n' ;;
*) exit 1 ;;
esac
"#;
        let gh = pr::Gh::at(isolated::write_program(riff.dir.path().join("gh"), script));
        let got = riff.after_build(gh).await;
        let note = text::wave_wip_head("issue-12", 40, "worktree-issue-12", "1a2b3c4", "9f8e7d6c");
        assert_eq!(got, [(Kind::Note, note.clone())]);
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/how-it-works.md"
        ))
        .unwrap();
        assert!(book.contains(&note), "{note}");
    }

    /// A lead holds the verify of the item: the lead gets a note with the
    /// hold, and no verify request (01M51KH0H7J35PEKMHP5KQB84T).
    #[tokio::test]
    async fn a_build_whose_verify_is_held_posts_a_note_with_the_hold() {
        let riff = Built::new().await;
        let thread = riff.wave.thread().unwrap();
        riff.api
            .hold(&riff.lead, &thread, "verify-issue-12", "a scratch item")
            .await
            .unwrap();
        let got = riff.after_build(riff.gh(false)).await;
        assert_eq!(got.len(), 1, "{got:?}");
        let (kind, body) = &got[0];
        assert_eq!(*kind, Kind::Note);
        assert!(
            body.starts_with(
                "issue-12: the session of the wave ended, and the item is free. The riff daemon \
                 sends no verify request for PR #40, branch worktree-issue-12, commit 1a2b3c4 \
                 (session 9f8e7d6c): verify-issue-12 is held by the lead (the session \
                 mike/lead1) since "
            ),
            "{body}"
        );
        assert!(body.ends_with(": a scratch item"), "{body}");
        assert!(!body.contains("verify request:"), "{body}");
    }

    /// The issue has the label `no-verify`: the lead gets a note with the
    /// label, and no verify request (01M51KH0H7J35PEKMHP5KQB84T). The book
    /// shows the same note.
    #[tokio::test]
    async fn a_build_of_an_issue_with_the_label_no_verify_posts_a_note() {
        let riff = Built::new().await;
        let got = riff.after_build(riff.gh(true)).await;
        let note = text::wave_no_verify(
            "issue-12",
            40,
            "worktree-issue-12",
            "1a2b3c4",
            "9f8e7d6c",
            &text::no_verify_label(12),
        );
        assert_eq!(got, [(Kind::Note, note.clone())]);
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/how-it-works.md"
        ))
        .unwrap();
        assert!(book.contains(&format!("\n{note}\n")), "{note}");
    }

    /// 01M57FJ3Z8AFDY9G0KTAYZJESY: the rebase session ended, and the pull
    /// request still has the head with the conflict: the lead gets a
    /// message, and no verify request. "Run the wave" in the book shows
    /// the same message.
    #[tokio::test]
    async fn a_rebase_with_no_push_tells_the_lead_and_sends_no_verify_request() {
        let riff = Built::new().await;
        let (lines, got) = riff
            .after(riff.gh(false), Some(Fix::Rebase("1a2b3c4d5e")))
            .await;
        assert_eq!(lines[1], text::run_lead_told("issue-12"));
        let message = text::wave_not_rebased(
            "issue-12",
            40,
            "worktree-issue-12",
            "main",
            "1a2b3c4d5e",
            "9f8e7d6c",
        );
        assert_eq!(got, [(Kind::Message, message.clone())]);
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/development.md"
        ))
        .unwrap();
        let run_the_wave = book
            .split("\n### Run the wave\n")
            .nth(1)
            .and_then(|rest| rest.split("\n### ").next())
            .expect("the how-to Run the wave");
        assert!(
            run_the_wave.contains(&format!("\n{message}\n")),
            "{message}"
        );
    }

    /// 01M5R56GN3DC09GAPAF69GENEZ (#1404): the default branch is not in a
    /// file. When it does not read after a rebase with no push, the lead
    /// still gets the message, with "the default branch" in place of its
    /// name.
    #[tokio::test]
    async fn a_rebase_with_no_push_tells_the_lead_when_the_default_branch_does_not_read() {
        let riff = Built::new().await;
        let gh = pr::Gh::at(isolated::write_program(
            riff.dir.path().join("gh"),
            r#"#!/bin/sh
case "$*" in
"pr view worktree-issue-12 --json number,url,headRefOid,state")
    echo '{"number":40,"url":"https://github.com/o/r/pull/40","headRefOid":"1a2b3c4d5e","state":"OPEN"}' ;;
*) echo "gh: HTTP 502" >&2; exit 1 ;;
esac
"#,
        ));
        let (_, got) = riff.after(gh, Some(Fix::Rebase("1a2b3c4d5e"))).await;
        let message = text::wave_not_rebased(
            "issue-12",
            40,
            "worktree-issue-12",
            text::WAVE_BASE_UNREAD,
            "1a2b3c4d5e",
            "9f8e7d6c",
        );
        assert_eq!(got, [(Kind::Message, message)]);
    }

    /// A fake `gh` for the fix of checks: the pull request #40 of the
    /// branch of issue 12 has the head `1a2b3c4d5e`, and the rollup of its
    /// checks is `rollup`.
    fn gh_with_rollup(riff: &Built, rollup: &str) -> pr::Gh {
        let script = format!(
            r#"#!/bin/sh
case "$*" in
"pr view worktree-issue-12 --json number,url,headRefOid,state")
    echo '{{"number":40,"url":"https://github.com/o/r/pull/40","headRefOid":"1a2b3c4d5e","state":"OPEN"}}' ;;
"pr view 40 --json {fields}")
    echo '{{"number":40,"headRefName":"worktree-issue-12","headRefOid":"1a2b3c4d5e","isDraft":false,"statusCheckRollup":{rollup}}}' ;;
"issue view 12 --json number,labels")
    echo '{{"number":12,"labels":[]}}' ;;
*) echo "the fake gh has no reply for: $*" >&2; exit 1 ;;
esac
"#,
            fields = rollout::PULL_FIELDS
        );
        pr::Gh::at(isolated::write_program(riff.dir.path().join("gh"), script))
    }

    /// #279: the session that fixed a failed check pushed nothing, and the
    /// check still fails at the head: the lead gets a message, and no
    /// verify request. "Run the wave" in the book shows the same message.
    #[tokio::test]
    async fn a_fix_of_checks_with_no_push_and_a_failed_check_tells_the_lead() {
        let riff = Built::new().await;
        let gh = gh_with_rollup(
            &riff,
            r#"[{"name":"Gate","conclusion":"FAILURE"},{"context":"riff/verify","state":"SUCCESS"}]"#,
        );
        let checks = ["Gate".to_owned()];
        let (lines, got) = riff
            .after(gh, Some(Fix::Checks("1a2b3c4d5e", &checks)))
            .await;
        assert_eq!(lines[1], text::run_lead_told("issue-12"));
        let message = text::wave_checks_not_fixed(
            "issue-12",
            40,
            "worktree-issue-12",
            "1a2b3c4d5e",
            "9f8e7d6c",
            &checks,
        );
        assert_eq!(got, [(Kind::Message, message.clone())]);
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/development.md"
        ))
        .unwrap();
        assert!(book.contains(&format!("\n{message}\n")), "{message}");
    }

    /// #279: the session ran the job again and pushed nothing: no check
    /// fails now (the new run goes). The lead gets a note, not a message,
    /// and no verify request: the verify of the head passed.
    #[tokio::test]
    async fn a_fix_of_checks_that_ran_the_job_again_gives_the_lead_a_note() {
        let riff = Built::new().await;
        let gh = gh_with_rollup(
            &riff,
            r#"[{"name":"Gate","conclusion":""},{"context":"riff/verify","state":"SUCCESS"}]"#,
        );
        let checks = ["Gate".to_owned()];
        let (lines, got) = riff
            .after(gh, Some(Fix::Checks("1a2b3c4d5e", &checks)))
            .await;
        assert_eq!(lines[1], text::run_lead_noted("issue-12"));
        let note = text::wave_checks_rerun(
            "issue-12",
            40,
            "worktree-issue-12",
            "1a2b3c4d5e",
            "9f8e7d6c",
        );
        assert_eq!(got, [(Kind::Note, note.clone())]);
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/development.md"
        ))
        .unwrap();
        assert!(book.contains(&format!("\n{note}\n")), "{note}");
    }

    /// #279: when `gh` cannot read the checks, the lead gets the message:
    /// riff does not call a fix good when it cannot see it.
    #[tokio::test]
    async fn a_fix_of_checks_with_no_read_of_the_checks_tells_the_lead() {
        let riff = Built::new().await;
        let checks = ["Gate".to_owned()];
        let (_, got) = riff
            .after(riff.gh(false), Some(Fix::Checks("1a2b3c4d5e", &checks)))
            .await;
        let [(kind, body)] = &got[..] else {
            panic!("{got:?}");
        };
        assert_eq!(*kind, Kind::Message);
        assert!(body.contains("the check Gate still fails"), "{body}");
    }

    /// #279: the fix pushed a new head: the lead gets the verify request
    /// of the new head.
    #[tokio::test]
    async fn a_fix_of_checks_that_pushed_sends_the_verify_request_of_the_new_head() {
        let riff = Built::new().await;
        let checks = ["Gate".to_owned()];
        let (lines, got) = riff
            .after(riff.gh(false), Some(Fix::Checks("0ld0ld0ld0", &checks)))
            .await;
        assert_eq!(lines[1], text::run_verify_sent("issue-12", 40));
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].0, Kind::Message);
        assert!(got[0].1.starts_with("verify request:"), "{got:?}");
    }

    /// 01M57FJ3Z8AFDY9G0KTAYZJESY: the rebase pushed a new head: the lead
    /// gets the verify request of the new head.
    #[tokio::test]
    async fn a_rebase_that_pushed_sends_the_verify_request_of_the_new_head() {
        let riff = Built::new().await;
        let (lines, got) = riff
            .after(riff.gh(false), Some(Fix::Rebase("0ld0ld0ld0")))
            .await;
        assert_eq!(lines[1], text::run_verify_sent("issue-12", 40));
        assert_eq!(
            got,
            [(
                Kind::Message,
                text::wave_verify_request(
                    "issue-12",
                    40,
                    "worktree-issue-12",
                    "1a2b3c4",
                    "9f8e7d6c"
                )
            )]
        );
    }
}
