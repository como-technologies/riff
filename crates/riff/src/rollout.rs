//! riff runs the free work by itself: the rollout of the lead.
//!
//! # Design
//!
//! `riff mcp` of the lead runs the rollout (01M3Q5QE01DB0FJQJWFKR450KQ).
//! So the start of work does not wait for an agent that remembers a
//! step. Once each [`INTERVAL`], it looks at the riff, and asks the riff
//! daemon of its machine to run the wave of the repository
//! ([`crate::wave`], 01M53NGVE839TRH7XB7CDPQNZE). The daemon runs each
//! item in a container. The rollout starts no worker in tmux
//! (01M599XR3NMJD9RAJ6ZAMW6KG5).
//!
//! ```mermaid
//! flowchart TD
//!     T["each interval"] --> L{"this session is the lead?"}
//!     L -- no --> T
//!     L -- yes --> R{"the riff and the repository run?"}
//!     R -- "no: paused" --> T
//!     R -- yes --> D["the daemon of this machine, at most 5 s:<br/>does a wave of the repository run?"]
//!     D -- "it runs" --> T
//!     D -- "no" --> W["free work (gh): a fix item of a broken main first,<br/>pull requests that wait for a verify,<br/>free items of the current wave, no held item"]
//!     W --> I{"free work?"}
//!     I -- no --> T
//!     I -- yes --> C["ask the daemon: have the session image<br/>(podman, and a pull when it is missing)"]
//!     C -- "it cannot" --> N
//!     C -- "it has it" --> S["ask the daemon: run the wave of the repository"]
//!     S --> N["a note to the lead: the daemon runs the wave,<br/>or why it does not"]
//!     N --> T
//! ```
//!
//! - **Free work** ([`free_items`], [`waiting_verifies`],
//!   01M3Q5QE2EWGKVD57Y6YCWA4BR). The current wave is the open milestone
//!   `Wave N` with the lowest N. A free item is an open issue of it that
//!   no session claims, that has no comment `Merged in #`, and whose
//!   `Needs:` issues are all closed. A pull request waits for a verify
//!   when it names an issue ([`Pull::issue`]), it is not a draft, its
//!   head has no status `riff/verify`, and no session claims
//!   `verify-issue-N` for it. The pull request of an issue with the
//!   label [`NO_VERIFY`] is no work (01M5157V0P92EBHF4M7N9K0RGR).
//! - **The issue of a pull request** ([`Pull::issue`],
//!   01M51BF5S2YKB8JG10DF6YVT42). Its trailer `Issue: #N` names it, as
//!   for `riff verify`. With no trailer, its branch names it
//!   (`worktree-issue-N`). So a pull request on a branch that names no
//!   issue, for example the release branch `release-v1.5.0`, still
//!   counts for its item: the rollout gives no build of it while the
//!   pull request waits for a verify or for the merge.
//! - **No held item** ([`free_work_of`], 01M51A55Q2MDZT38JDCEJ1QECP). A
//!   held item is no free work: `issue-N` and `verify-issue-N`, each by
//!   its own hold. Each look reads the holds of the repository thread
//!   again ([`Api::plan_show`]), so a free of the item makes it free work
//!   at the next look.
//! - **A fix item first** ([`fix_items`], 01M51QEYS538XA58KDRY844T52). An
//!   open issue with the label [`BROKEN_MAIN`] fixes a broken `main`: the
//!   look of the lead files it when the Gate of `main` fails
//!   ([`crate::look::MainWatch`]). It comes before each other item, its
//!   verify first, also outside the current wave. The wave of the daemon
//!   takes the same order ([`crate::wave`]).
//! - **One count for an item** ([`Verify`], [`in_verify`],
//!   01M3Z9N5HHHS1E17NFGMVBKZ0K). The author of a pull request releases
//!   its item at the verify request. So an open issue with no claim can
//!   have a pull request. riff reads the state of that pull request
//!   from the status `riff/verify` of its head
//!   (01M3Z9MY0CDBB1G749XBVMVV8X):
//!
//!   | Status `riff/verify` | State | Work |
//!   |---|---|---|
//!   | none | asked | a verify |
//!   | `success` | passed | none: the merge waits |
//!   | `failure` | failed | a build: the item is free, with its earlier work |
//! - **The wave of the daemon** ([`View::wave`], [`Env::run_wave`]). The
//!   daemon of the machine of the lead tells the waves that it runs
//!   ([`crate::wave::runs`]). A daemon that does not answer runs no
//!   wave. The look waits at most [`crate::daemon::STATUS_WAIT`] for
//!   that answer ([`wave_state`], 01M558EY88CACNZ55FWMRT8RP7): a daemon
//!   that takes the connection and never answers stops no look. While a
//!   wave of the repository runs there, the rollout asks nothing, and
//!   makes no call of `gh`. The claims on riff-server keep the wave and
//!   the sessions of a person from one item
//!   (01M53NGWTNC3655ZZJ76M1R5R7). The limit of the containers is the
//!   limit of the daemon (01M4ZV3B6HAFTSC2K63FMC5FYA). A second machine
//!   runs its own wave with `riff wave run` (01M53NGVH8A19QS3CQ79ENVVXT).
//! - **A machine that cannot run a container**
//!   (01M558EYAJFZK7Y822NVWGBS9H). Before it asks for the wave, the
//!   rollout asks the daemon to have the session image: podman runs, and
//!   the image is on the machine, or the daemon pulls it
//!   ([`crate::wave::ask`]). When the daemon cannot, the rollout makes no
//!   ask of the wave, and the lead gets the note of a failed ask. So a
//!   machine with no podman runs no wave, and no job of it sends a
//!   message.
//! - **Notes** (01M5515CA4CZ6KV4ZD6Y0744Z7). After the ask, the lead
//!   gets a note ([`text::rollout_wave`]). When the ask fails, the lead
//!   gets one note with the reason ([`text::rollout_no_wave`]), and the
//!   rollout asks again at each look. The next note comes for a new
//!   reason, or after an ask that worked.
//! - **A stop of the wave** (01M5515CFHRA6AJG4GKX0ZV77W). `riff wave
//!   stop` stops the wave of the daemon. The rollout asks for it again
//!   at its next look with free work. To keep it stopped, pause the
//!   repository.
//! - **Pause** (01M3Q5QEBTNM90SPYXNVTT7RJA). A pause stops the rollout
//!   within one look. A look that started before the pause can ask for
//!   the wave one more time. The wave then starts nothing while the
//!   repository is paused.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use riff_core::name::SessionUri;
use riff_core::selector::Selector;
use riff_core::wire::{Kind, RiffState, SessionInfo};
use serde::Deserialize;

use crate::identity;
use crate::pr::Gh;
use crate::{text, usage};
use riff_client::api::Api;

/// The time between two looks of the rollout.
pub const INTERVAL: Duration = Duration::from_secs(10);

/// The variable that gives the seconds between two looks, for tests.
pub const INTERVAL_VAR: &str = "RIFF_ROLLOUT_INTERVAL";

/// What the rollout sees at one look.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct View {
    /// True when the riff runs.
    pub running: bool,
    /// The number of free work items: the free items and the pull
    /// requests that wait for a verify.
    pub work: usize,
    /// The host of the lead.
    pub host: String,
    /// The repository of the lead, as `OWNER/REPO`.
    pub repo: String,
    /// The wave of the repository on the riff daemon of the machine of
    /// the lead.
    pub wave: WaveState,
}

/// The wave of the repository of the lead on the riff daemon of its
/// machine, at one look ([`crate::wave::runs`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum WaveState {
    /// The daemon runs no wave of the repository.
    #[default]
    None,
    /// The daemon runs a wave of the repository.
    Runs,
    /// No daemon answers, for this reason.
    NoDaemon(String),
}

/// The wave of the repository `repo` (`OWNER/REPO`) on the riff daemon in
/// the folder `dir`. The look waits at most `limit` for the status of the
/// daemon ([`crate::daemon::STATUS_WAIT`], 01M558EY88CACNZ55FWMRT8RP7). A
/// daemon that gives no answer in that time runs no wave: the look goes
/// on with no daemon facts.
pub async fn wave_state(dir: &Path, repo: &str, limit: Duration) -> WaveState {
    match crate::daemon::status_within(dir, limit).await {
        Ok(status) if crate::wave::runs(&status, repo) => WaveState::Runs,
        Ok(_) => WaveState::None,
        Err(e) => WaveState::NoDaemon(format!("{e:#}")),
    }
}

/// What the rollout needs from the world. [`Live`] is the real one.
pub trait Env {
    /// The time between two looks.
    fn interval(&self) -> Duration;
    /// One look, or `None` when this session is not the lead.
    fn look(&self) -> impl Future<Output = Result<Option<View>>> + Send;
    /// Asks the riff daemon of this machine to run the wave of the
    /// repository of the lead (01M53NGVE839TRH7XB7CDPQNZE).
    fn run_wave(&self) -> impl Future<Output = Result<()>> + Send;
    /// Gives the note `body` to the lead.
    fn tell(&self, body: &str) -> impl Future<Output = Result<()>> + Send;
}

/// Runs the rollout until the task ends. It waits one interval, looks,
/// and asks the riff daemon for the wave when free work waits, again and
/// again. It starts no worker in tmux (01M599XR3NMJD9RAJ6ZAMW6KG5). It
/// prints an error once, not again until the error changes.
///
/// The lead gets a note for each ask of the wave, and one note for each
/// new reason why the rollout cannot ask: no daemon answers, or the ask
/// failed (01M5515CA4CZ6KV4ZD6Y0744Z7). A wave that runs, or an ask that
/// works, lets the next reason give a note again.
pub async fn run(env: impl Env) {
    let mut last_error = None;
    // The reason of the last note that the rollout cannot ask for the
    // wave.
    let mut no_wave: Option<String> = None;
    loop {
        tokio::time::sleep(env.interval()).await;
        let step = async {
            let Some(view) = env.look().await?.filter(|view| view.running) else {
                return anyhow::Ok(());
            };
            let why = match &view.wave {
                WaveState::Runs => {
                    no_wave = None;
                    return Ok(());
                }
                WaveState::NoDaemon(why) => why.clone(),
                WaveState::None if view.work > 0 => match env.run_wave().await {
                    Ok(()) => {
                        no_wave = None;
                        eprintln!("riff: the rollout asked the riff daemon to run the wave");
                        return env.tell(&text::rollout_wave(&view.host, &view.repo)).await;
                    }
                    Err(e) => format!("{e:#}"),
                },
                WaveState::None => return Ok(()),
            };
            if no_wave.as_ref() != Some(&why) {
                env.tell(&text::rollout_no_wave(&view.host, &view.repo, &why))
                    .await?;
                no_wave = Some(why);
            }
            Ok(())
        };
        match step.await {
            Ok(()) => last_error = None,
            Err(e) => {
                let e = format!("{e:#}");
                if last_error.as_ref() != Some(&e) {
                    eprintln!("riff: the rollout: {e}");
                }
                last_error = Some(e);
            }
        }
    }
}

/// An open milestone.
#[derive(Debug, Clone, Deserialize)]
pub struct Milestone {
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub title: String,
}

/// The current wave: the open milestone `Wave N` with the lowest N. A
/// name can follow the number, as in `Wave 5: Cloud`.
///
/// ```
/// use riff::rollout::{Milestone, current_wave};
///
/// let m = |t: &str| Milestone { title: t.into() };
/// let open = [m("Backlog"), m("Wave 14"), m("Wave 13: Workers"), m("Wave 2x")];
/// assert_eq!(current_wave(&open), Some("Wave 13: Workers"));
/// assert_eq!(current_wave(&[m("Backlog")]), None);
/// ```
pub fn current_wave(open: &[Milestone]) -> Option<&str> {
    open.iter()
        .filter_map(|m| {
            let rest = m.title.strip_prefix("Wave ")?;
            let number = rest.split(':').next()?.trim().parse::<u64>().ok()?;
            Some((number, m.title.as_str()))
        })
        .min_by_key(|(n, _)| *n)
        .map(|(_, title)| title)
}

/// The label of an issue that gets no verify: riff offers no verify of
/// its pull request, and `riff run` sends no verify request for it
/// (01M5157V0P92EBHF4M7N9K0RGR). The scratch item of the real-session
/// test has it.
pub const NO_VERIFY: &str = "no-verify";

/// The label of a fix item: an issue to fix a broken `main`
/// (01M51QEYPGD6W345MZ4F85C38F). The look of the lead files it when the
/// Gate of `main` fails ([`crate::look::MainWatch`]). It is free work
/// before each other item ([`free_work_of`], 01M51QEYS538XA58KDRY844T52).
pub const BROKEN_MAIN: &str = "broken-main";

/// An open issue, as `gh issue list --json
/// number,body,comments,milestone,labels` gives it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Issue {
    pub number: u64,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub comments: Vec<Comment>,
    #[serde(default)]
    pub milestone: Option<Milestone>,
    #[serde(default)]
    pub labels: Vec<Label>,
}

/// A label of an issue.
#[derive(Debug, Clone, Deserialize)]
pub struct Label {
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub name: String,
}

/// A comment of an issue.
#[derive(Debug, Clone, Deserialize)]
pub struct Comment {
    pub body: String,
}

impl Issue {
    /// True when a comment says `Merged in #PR`: the issue waits only for
    /// its checks after the release.
    pub fn merged(&self) -> bool {
        self.comments
            .iter()
            .any(|c| c.body.trim_start().starts_with("Merged in #"))
    }

    /// True when the issue has the label [`NO_VERIFY`]
    /// (01M5157V0P92EBHF4M7N9K0RGR).
    pub fn no_verify(&self) -> bool {
        self.labels.iter().any(|l| l.name == NO_VERIFY)
    }

    /// True when the issue is a fix item: it has the label
    /// [`BROKEN_MAIN`] (01M51QEYPGD6W345MZ4F85C38F).
    pub fn broken_main(&self) -> bool {
        self.labels.iter().any(|l| l.name == BROKEN_MAIN)
    }
}

/// The issues of the `Needs:` line of `body`. `Needs: nothing` and no
/// line give none.
///
/// ```
/// assert_eq!(riff::rollout::needs("Text.\n\nNeeds: #12, #15\n"), [12, 15]);
/// assert_eq!(riff::rollout::needs("Needs: nothing"), Vec::<u64>::new());
/// assert_eq!(riff::rollout::needs("No line."), Vec::<u64>::new());
/// ```
pub fn needs(body: &str) -> Vec<u64> {
    body.lines()
        .filter_map(|l| l.trim().strip_prefix("Needs:"))
        .flat_map(|rest| rest.split(|c: char| !c.is_ascii_alphanumeric() && c != '#'))
        .filter_map(|word| word.strip_prefix('#')?.parse().ok())
        .collect()
}

/// The free items of the wave `wave`, from all `open` issues of the
/// repository, the `claims` of all sessions and the open `pulls`: an
/// open issue of the wave that no session claims, with no comment
/// `Merged in #`, and with each issue of its `Needs:` line closed. An
/// open need blocks the item, also a need outside the wave, and also a
/// need that is merged but not closed.
///
/// An item whose pull request ([`Pull::issue`]) waits for a verify or
/// for the merge is not free: it is work for a verify, not for a build
/// (01M3Z9N5HHHS1E17NFGMVBKZ0K). An item whose verify failed is free,
/// with its earlier work.
///
/// ```
/// use std::collections::HashSet;
/// use riff::rollout::{Check, Comment, Issue, Milestone, Pull, free_items};
///
/// let issue = |number, wave: &str, body: &str, merged| Issue {
///     number,
///     body: body.into(),
///     comments: if merged { vec![Comment { body: "Merged in #9 (abc)".into() }] } else { vec![] },
///     milestone: Some(Milestone { title: wave.into() }),
///     labels: vec![],
/// };
/// let open = [
///     issue(1, "Wave 2", "", false),
///     issue(2, "Wave 2", "", true),
///     issue(3, "Wave 2", "Needs: #1", false),
///     issue(4, "Wave 2", "Needs: #99", false),
///     issue(5, "Wave 2", "", false),
///     issue(6, "Wave 2", "Needs: #2", false),
///     issue(7, "Wave 2", "Needs: #50", false),
///     issue(50, "Backlog", "", false),
///     issue(51, "Wave 3", "", false),
/// ];
/// let claims: HashSet<String> = ["issue-5".to_owned()].into();
/// // 99 is closed. 2 is merged but open. 50 is open outside the wave.
/// assert_eq!(free_items(&open, "Wave 2", &claims, &[]), [1, 4]);
///
/// // The author of 1 asked for a verify and released the item. The
/// // verify of 4 failed.
/// let pull = |number, issue: u64, state: Option<&str>| Pull {
///     number,
///     branch: format!("worktree-issue-{issue}"),
///     checks: state.map(Check::verify).into_iter().collect(),
///     ..Pull::default()
/// };
/// let pulls = [pull(40, 1, None), pull(41, 4, Some("FAILURE"))];
/// assert_eq!(free_items(&open, "Wave 2", &claims, &pulls), [4]);
/// let pulls = [pull(40, 1, Some("SUCCESS"))];
/// assert_eq!(free_items(&open, "Wave 2", &claims, &pulls), [4]);
///
/// // The branch of a release names no issue: its trailer `Issue: #1`
/// // names the item (01M51BF5S2YKB8JG10DF6YVT42).
/// let release = |state| Pull { branch: "release-v1.5.0".into(), trailer: Some(1), ..pull(42, 1, state) };
/// assert_eq!(free_items(&open, "Wave 2", &claims, &[release(None)]), [4]);
/// assert_eq!(free_items(&open, "Wave 2", &claims, &[release(Some("SUCCESS"))]), [4]);
/// assert_eq!(free_items(&open, "Wave 2", &claims, &[release(Some("FAILURE"))]), [1, 4]);
/// ```
pub fn free_items(
    open: &[Issue],
    wave: &str,
    claims: &HashSet<String>,
    pulls: &[Pull],
) -> Vec<u64> {
    free_of(open, claims, pulls, |i| {
        i.milestone.as_ref().is_some_and(|m| m.title == wave)
    })
}

/// The free fix items of `open` (01M51QEYS538XA58KDRY844T52): each open
/// issue with the label [`BROKEN_MAIN`] that is free as an item of
/// [`free_items`] is, in each milestone and with no milestone.
///
/// ```
/// use std::collections::HashSet;
/// use riff::rollout::{Issue, Label, Milestone, fix_items};
///
/// let issue = |number, wave: Option<&str>, label: &str| Issue {
///     number,
///     milestone: wave.map(|title| Milestone { title: title.into() }),
///     labels: vec![Label { name: label.into() }],
///     ..Issue::default()
/// };
/// let open = [
///     issue(1, Some("Wave 2"), "bug"),
///     issue(9, None, "broken-main"),
///     issue(10, Some("Wave 7"), "broken-main"),
///     issue(11, Some("Wave 2"), "broken-main"),
/// ];
/// let claims: HashSet<String> = ["issue-11".to_owned()].into();
/// assert_eq!(fix_items(&open, &claims, &[]), [9, 10]);
/// ```
pub fn fix_items(open: &[Issue], claims: &HashSet<String>, pulls: &[Pull]) -> Vec<u64> {
    free_of(open, claims, pulls, Issue::broken_main)
}

/// The issues of `open` that `pick` takes and that are free: no claim,
/// no comment `Merged in #`, no pull request that waits for a verify or
/// for the merge, and no open need.
fn free_of(
    open: &[Issue],
    claims: &HashSet<String>,
    pulls: &[Pull],
    pick: impl Fn(&Issue) -> bool,
) -> Vec<u64> {
    let numbers: HashSet<u64> = open.iter().map(|i| i.number).collect();
    let in_verify = in_verify(pulls);
    open.iter()
        .filter(|i| pick(i))
        .filter(|i| !i.merged())
        .filter(|i| !claims.contains(&format!("issue-{}", i.number)))
        .filter(|i| !in_verify.contains(&i.number))
        .filter(|i| needs(&i.body).iter().all(|n| !numbers.contains(n)))
        .map(|i| i.number)
        .collect()
}

/// The issues with a pull request in `pulls` that waits for a verify
/// or for the merge ([`Verify::Asked`], [`Verify::Passed`]). Such an
/// item is no free work for a build (01M3Z9N5HHHS1E17NFGMVBKZ0K).
///
/// ```
/// use riff::rollout::{Check, Pull, in_verify};
///
/// let pull = |branch: &str, checks| Pull { number: 40, branch: branch.into(), checks, ..Pull::default() };
/// let pulls = [
///     pull("worktree-issue-12", vec![]),
///     pull("worktree-issue-13", vec![Check::verify("SUCCESS")]),
///     pull("worktree-issue-14", vec![Check::verify("FAILURE")]),
///     Pull { draft: true, ..pull("worktree-issue-15", vec![]) },
///     pull("release-v0.8.0", vec![]),
///     Pull { trailer: Some(16), ..pull("release-v0.9.0", vec![Check::verify("SUCCESS")]) },
/// ];
/// let mut issues: Vec<u64> = in_verify(&pulls).into_iter().collect();
/// issues.sort_unstable();
/// assert_eq!(issues, [12, 13, 16]);
/// ```
pub fn in_verify(pulls: &[Pull]) -> HashSet<u64> {
    pulls
        .iter()
        .filter(|p| matches!(p.verify(), Some(Verify::Asked | Verify::Passed)))
        .filter_map(Pull::issue)
        .collect()
}

/// The fields of `gh pr list --json` that [`Pull`] reads.
pub const PULL_FIELDS: &str =
    "number,headRefName,headRefOid,isDraft,statusCheckRollup,mergeable,autoMergeRequest,body";

/// An open pull request, as `gh pr list --json` with [`PULL_FIELDS`]
/// gives it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Pull {
    pub number: u64,
    #[serde(rename = "headRefName", deserialize_with = "crate::text::forge_de")]
    pub branch: String,
    /// The head commit.
    #[serde(
        rename = "headRefOid",
        default,
        deserialize_with = "crate::text::forge_de"
    )]
    pub head: String,
    #[serde(rename = "isDraft", default)]
    pub draft: bool,
    #[serde(rename = "statusCheckRollup", default)]
    pub checks: Vec<Check>,
    /// `MERGEABLE`, `CONFLICTING` or `UNKNOWN`: GitHub finds it after a
    /// push, so it can be `UNKNOWN` for a short time.
    #[serde(default, deserialize_with = "crate::text::forge_de_opt")]
    pub mergeable: Option<String>,
    /// True when auto-merge is on.
    #[serde(rename = "autoMergeRequest", default, deserialize_with = "present")]
    pub auto_merge: bool,
    /// The issue of the trailer `Issue: #N` of the body, when it has
    /// one. riff keeps only the number, not the text of the body.
    #[serde(rename = "body", default, deserialize_with = "trailer")]
    pub trailer: Option<u64>,
}

/// True for a value that is not `null`.
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Option::<serde::de::IgnoredAny>::deserialize(d)?.is_some())
}

/// The issue of the trailer `Issue: #N` of a body ([`crate::pr::trailer_issue`]).
fn trailer<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    Ok(Option::<String>::deserialize(d)?
        .as_deref()
        .and_then(crate::pr::trailer_issue))
}

/// A status or a check of the head of a pull request. Only a status has
/// a context and a state.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Check {
    #[serde(default, deserialize_with = "crate::text::forge_de_opt")]
    pub context: Option<String>,
    /// The state of a status: `SUCCESS`, `FAILURE`, `ERROR`, `PENDING`
    /// or `EXPECTED`.
    #[serde(default, deserialize_with = "crate::text::forge_de_opt")]
    pub state: Option<String>,
    /// The URL of a status. For `riff/verify` it is the comment with
    /// the result.
    #[serde(
        rename = "targetUrl",
        default,
        deserialize_with = "crate::text::forge_de_opt"
    )]
    pub url: Option<String>,
}

impl Check {
    /// The status `riff/verify` with `state`, as `riff verify` sets it.
    pub fn verify(state: &str) -> Self {
        Self {
            context: Some(crate::pr::VERIFY_CONTEXT.into()),
            state: Some(state.into()),
            url: None,
        }
    }
}

/// Where the pull request of an item is on its way to the merge. riff
/// makes it from the status `riff/verify` of the head commit
/// (01M3Z9MY0CDBB1G749XBVMVV8X). So it needs no claim of the author.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verify {
    /// No status: the pull request waits for a verify.
    Asked,
    /// The verify passed: the pull request waits for the merge.
    Passed,
    /// The verify failed: the item is free, with its earlier work.
    Failed,
}

impl Pull {
    /// The issue of this pull request (01M51BF5S2YKB8JG10DF6YVT42): its
    /// trailer `Issue: #N`, as for `riff verify`. With no trailer, the
    /// issue of its branch ([`branch_issue`]).
    ///
    /// ```
    /// use riff::rollout::Pull;
    ///
    /// let pull = |branch: &str, body: &str| -> Pull {
    ///     serde_json::from_value(serde_json::json!({"number": 40, "headRefName": branch, "body": body}))
    ///         .unwrap()
    /// };
    /// let release = "Refs #668\n\nLevel: major.\n\nIssue: #668\nMilestone: Wave 23\n";
    /// assert_eq!(pull("release-2.0.0", release).issue(), Some(668));
    /// assert_eq!(pull("worktree-issue-12", "").issue(), Some(12));
    /// assert_eq!(pull("worktree-issue-12", "Closes #12\n\nIssue: #13\n").issue(), Some(13));
    /// assert_eq!(pull("release-2.0.0", "Refs #668\n").issue(), None);
    /// assert_eq!(pull("main", "").issue(), None);
    /// ```
    pub fn issue(&self) -> Option<u64> {
        self.trailer.or_else(|| branch_issue(&self.branch))
    }

    /// The state of the verify of this pull request. `None` for a
    /// draft, and for a pull request that names no issue
    /// ([`Pull::issue`]): a pull request that a person opened by hand is
    /// no work.
    ///
    /// ```
    /// use riff::rollout::{Check, Pull, Verify};
    ///
    /// let pull = |checks| Pull { number: 40, branch: "worktree-issue-12".into(), checks, ..Pull::default() };
    /// assert_eq!(pull(vec![]).verify(), Some(Verify::Asked));
    /// assert_eq!(pull(vec![Check::default()]).verify(), Some(Verify::Asked));
    /// assert_eq!(pull(vec![Check::verify("SUCCESS")]).verify(), Some(Verify::Passed));
    /// assert_eq!(pull(vec![Check::verify("FAILURE")]).verify(), Some(Verify::Failed));
    /// assert_eq!(pull(vec![Check::verify("ERROR")]).verify(), Some(Verify::Failed));
    /// assert_eq!(Pull { draft: true, ..pull(vec![]) }.verify(), None);
    /// assert_eq!(Pull { branch: "main".into(), ..pull(vec![]) }.verify(), None);
    /// ```
    pub fn verify(&self) -> Option<Verify> {
        if self.draft {
            return None;
        }
        self.issue()?;
        Some(match self.verify_status() {
            None => Verify::Asked,
            Some(check) => match check.state.as_deref() {
                Some("FAILURE" | "ERROR") => Verify::Failed,
                _ => Verify::Passed,
            },
        })
    }

    /// The status `riff/verify` of the head, when it has one.
    pub fn verify_status(&self) -> Option<&Check> {
        self.checks
            .iter()
            .find(|c| c.context.as_deref() == Some(crate::pr::VERIFY_CONTEXT))
    }

    /// True when auto-merge is on and the pull request has a conflict
    /// with the default branch: it cannot merge (01M49Q30XMVRFX42YTM1PHX0RZ).
    /// Only a pull request of an item counts, as for [`Pull::verify`].
    ///
    /// ```
    /// use riff::rollout::Pull;
    ///
    /// let pull = |mergeable: &str, auto_merge| Pull {
    ///     number: 40,
    ///     branch: "worktree-issue-12".into(),
    ///     mergeable: Some(mergeable.into()),
    ///     auto_merge,
    ///     ..Pull::default()
    /// };
    /// assert!(pull("CONFLICTING", true).conflict());
    /// assert!(!pull("CONFLICTING", false).conflict());
    /// assert!(!pull("UNKNOWN", true).conflict());
    /// assert!(!pull("MERGEABLE", true).conflict());
    /// assert!(!Pull { draft: true, ..pull("CONFLICTING", true) }.conflict());
    /// assert!(!Pull { branch: "main".into(), ..pull("CONFLICTING", true) }.conflict());
    /// ```
    pub fn conflict(&self) -> bool {
        self.auto_merge
            && self.mergeable.as_deref() == Some("CONFLICTING")
            && self.verify().is_some()
    }
}

/// The issue of a branch like `worktree-issue-12` or
/// `worktree-issue-207-book`.
///
/// ```
/// use riff::rollout::branch_issue;
///
/// assert_eq!(branch_issue("worktree-issue-12"), Some(12));
/// assert_eq!(branch_issue("worktree-issue-207-book"), Some(207));
/// assert_eq!(branch_issue("main"), None);
/// assert_eq!(branch_issue("worktree-issue-x"), None);
/// ```
pub fn branch_issue(branch: &str) -> Option<u64> {
    let rest = branch.rsplit_once("issue-")?.1;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The issues of the pull requests that wait for a verify
/// ([`Verify::Asked`]): the pull request names an issue
/// ([`Pull::issue`]), it is not a draft, its head has no status
/// `riff/verify`, and no session claims `verify-issue-N` for its issue.
/// A pull request that names no issue is no work.
///
/// ```
/// use std::collections::HashSet;
/// use riff::rollout::{Check, Pull, waiting_verifies};
///
/// let pull = |number, branch: &str, state: Option<&str>| Pull {
///     number,
///     branch: branch.into(),
///     checks: state.map(Check::verify).into_iter().collect(),
///     ..Pull::default()
/// };
/// let pulls = [
///     pull(40, "worktree-issue-12", None),
///     pull(41, "worktree-issue-13", Some("SUCCESS")),
///     pull(42, "worktree-issue-14", None),
///     Pull { draft: true, ..pull(43, "worktree-issue-15", None) },
///     pull(44, "release-v0.8.0", None),
///     pull(45, "worktree-issue-16", Some("FAILURE")),
///     // A release branch: its trailer `Issue: #17` names the issue.
///     Pull { trailer: Some(17), ..pull(46, "release-v0.9.0", None) },
/// ];
/// let claims: HashSet<String> = ["verify-issue-14".to_owned()].into();
/// assert_eq!(waiting_verifies(&pulls, &claims), [12, 17]);
/// ```
pub fn waiting_verifies(pulls: &[Pull], claims: &HashSet<String>) -> Vec<u64> {
    pulls
        .iter()
        .filter(|p| p.verify() == Some(Verify::Asked))
        .filter_map(Pull::issue)
        .filter(|n| !claims.contains(&format!("verify-issue-{n}")))
        .collect()
}

/// The longest wait of a claim for the pull requests of `gh`
/// (01M3Z9N6SPWPSSCBEVDCKBESSV).
pub const PULL_WAIT: Duration = Duration::from_secs(5);

/// The line after the answer to a granted claim of `item`, when the
/// item has an open pull request in `pulls`
/// (01M3Z9N6SPWPSSCBEVDCKBESSV). The author of a pull request releases
/// its item at the verify request, so the next session that claims the
/// item must know where the pull request is. A verify claim, and an
/// item with no pull request, get no line.
///
/// ```
/// use riff::rollout::{Check, Pull, claim_line};
///
/// let pull = |checks| Pull {
///     number: 40,
///     branch: "worktree-issue-12".into(),
///     head: "1a2b3c4d5e6f".into(),
///     checks,
///     ..Pull::default()
/// };
/// let failed = Check { url: Some("https://c".into()), ..Check::verify("FAILURE") };
/// assert_eq!(
///     claim_line("issue-12", &[pull(vec![failed])]).unwrap(),
///     "The verify of pull request #40 of issue-12 failed for commit 1a2b3c4: https://c. Read the \
///      result, go on from the branch, and send a new verify request: see \"Pick up dropped \
///      work\" in the riff skill."
/// );
/// assert_eq!(
///     claim_line("issue-12", &[pull(vec![])]).unwrap(),
///     "Pull request #40 of issue-12 waits for a verify of commit 1a2b3c4. The build is done: \
///      do not build it again. Release issue-12. To verify the work, claim verify-issue-12."
/// );
/// assert_eq!(
///     claim_line("issue-12", &[pull(vec![Check::verify("SUCCESS")])]).unwrap(),
///     "The verify of pull request #40 of issue-12 passed for commit 1a2b3c4, and the merge \
///      waits. The build is done: do not build it again. Release issue-12."
/// );
/// assert_eq!(claim_line("verify-issue-12", &[pull(vec![])]), None);
/// assert_eq!(claim_line("issue-13", &[pull(vec![])]), None);
/// assert_eq!(claim_line("issue-12", &[Pull { draft: true, ..pull(vec![]) }]), None);
/// ```
pub fn claim_line(item: &str, pulls: &[Pull]) -> Option<String> {
    let issue: u64 = item.strip_prefix("issue-")?.parse().ok()?;
    let (pull, state) = pulls
        .iter()
        .filter(|p| p.issue() == Some(issue))
        .find_map(|p| Some((p, p.verify()?)))?;
    let number = pull.number;
    let commit: String = pull.head.chars().take(7).collect();
    Some(match state {
        Verify::Failed => {
            let result = match pull.verify_status().and_then(|c| c.url.as_deref()) {
                Some(url) if !url.is_empty() => format!(": {}", text::safe(url)),
                _ => String::new(),
            };
            format!(
                "The verify of pull request #{number} of {item} failed for commit {commit}{result}. \
                 Read the result, go on from the branch, and send a new verify request: see \
                 \"Pick up dropped work\" in the riff skill."
            )
        }
        Verify::Asked => format!(
            "Pull request #{number} of {item} waits for a verify of commit {commit}. The build is \
             done: do not build it again. Release {item}. To verify the work, claim verify-{item}."
        ),
        Verify::Passed => format!(
            "The verify of pull request #{number} of {item} passed for commit {commit}, and the \
             merge waits. The build is done: do not build it again. Release {item}."
        ),
    })
}

/// The line of [`claim_line`] for a granted claim of `item` in the
/// repository `repo`, from the clone of `dir`. A pull request has a
/// pushed branch. So riff asks `gh` only when the clone knows a pushed
/// branch of the item, after the fetch of [`crate::dropped::at_claim`],
/// and waits at most [`PULL_WAIT`]. With no `gh`, or a slow one, the
/// claim gets no line.
pub async fn at_claim(dir: &Path, repo: &str, item: &str) -> Option<String> {
    if !item.starts_with("issue-") {
        return None;
    }
    let (dir, repo, item) = (dir.to_owned(), repo.to_owned(), item.to_owned());
    let look = tokio::task::spawn_blocking(move || {
        crate::dropped::find(&dir, &item).filter(|earlier| !earlier.pushed.is_empty())?;
        let pulls = pulls(&Gh::default(), &repo).ok()?;
        claim_line(&item, &pulls)
    });
    tokio::time::timeout(PULL_WAIT, look).await.ok()?.ok()?
}

/// Each claim of each session in `sessions`.
pub fn claims(sessions: &[SessionInfo]) -> HashSet<String> {
    sessions
        .iter()
        .flat_map(|s| s.uri.claims().iter().cloned())
        .collect()
}

/// The open pull requests of the repository `repo` (`OWNER/REPO`),
/// with `gh`.
pub fn pulls(gh: &Gh, repo: &str) -> Result<Vec<Pull>> {
    gh.json(&[
        "pr",
        "list",
        "--repo",
        repo,
        "--state",
        "open",
        "--limit",
        "100",
        "--json",
        PULL_FIELDS,
    ])
}

/// The open milestones, issues and pull requests of a repository: the
/// facts of its free work ([`free_work_of`]).
#[derive(Debug, Clone, Default)]
pub struct Open {
    pub milestones: Vec<Milestone>,
    pub issues: Vec<Issue>,
    pub pulls: Vec<Pull>,
}

impl Open {
    /// The open milestones, issues and pull requests of the repository
    /// `repo` (`OWNER/REPO`), with `gh`.
    pub fn of(gh: &Gh, repo: &str) -> Result<Self> {
        let milestones: Vec<Milestone> = gh.json(&[
            "api",
            &format!("repos/{repo}/milestones?state=open&per_page=100"),
        ])?;
        let pulls = pulls(gh, repo)?;
        let issues: Vec<Issue> = gh.json(&[
            "issue",
            "list",
            "--repo",
            repo,
            "--state",
            "open",
            "--limit",
            "1000",
            "--json",
            "number,body,comments,milestone,labels",
        ])?;
        Ok(Self {
            milestones,
            issues,
            pulls,
        })
    }

    /// [`free_work_of`] with these facts.
    pub fn free_work(&self, claims: &HashSet<String>, holds: &HashSet<String>) -> Vec<String> {
        free_work_of(&self.milestones, &self.issues, &self.pulls, claims, holds)
    }
}

/// The free work of the repository `repo` (`OWNER/REPO`), with `gh`:
/// [`free_work_of`] with its open milestones, its open issues and its
/// open pull requests ([`Open`]).
pub fn free_work(
    gh: &Gh,
    repo: &str,
    claims: &HashSet<String>,
    holds: &HashSet<String>,
) -> Result<Vec<String>> {
    Ok(Open::of(gh, repo)?.free_work(claims, holds))
}

/// The free work, by name: the pull requests that wait for a verify
/// (`verify-issue-N`), then the free items of the current wave
/// (`issue-N`). An item counts one time: as a build or as a verify
/// (01M3Z9N5HHHS1E17NFGMVBKZ0K). The pull request of an issue with the
/// label [`NO_VERIFY`] is no work (01M5157V0P92EBHF4M7N9K0RGR). An item
/// in `holds` is no work (01M51A55Q2MDZT38JDCEJ1QECP): a hold names one
/// item, so a hold of `issue-N` keeps `verify-issue-N` free.
///
/// A fix item ([`BROKEN_MAIN`], 01M51QEYS538XA58KDRY844T52) comes before
/// each other item: its verify first, then the item. It is free work in
/// each milestone ([`fix_items`]).
///
/// ```
/// use std::collections::HashSet;
/// use riff::rollout::{Issue, Label, Milestone, Pull, free_work_of};
///
/// let issue = |number, wave: &str, labels: &[&str]| Issue {
///     number,
///     milestone: Some(Milestone { title: wave.into() }),
///     labels: labels.iter().map(|l| Label { name: l.to_string() }).collect(),
///     ..Issue::default()
/// };
/// let pull = |number, issue: u64| Pull {
///     number,
///     branch: format!("worktree-issue-{issue}"),
///     ..Pull::default()
/// };
/// let open = [Milestone { title: "Wave 2".into() }];
/// let issues = [issue(1, "Wave 2", &[]), issue(2, "Backlog", &[]), issue(3, "Backlog", &["no-verify"])];
/// // Each pull request waits for a verify: the one of 3 is no work.
/// let pulls = [pull(40, 2), pull(41, 3)];
/// let none = HashSet::new();
/// assert_eq!(
///     free_work_of(&open, &issues, &pulls, &none, &none),
///     ["verify-issue-2", "issue-1"]
/// );
/// // The lead holds issue-1: it is no work. The verify of 2 stays.
/// let holds: HashSet<String> = ["issue-1".to_owned(), "issue-2".to_owned()].into();
/// assert_eq!(free_work_of(&open, &issues, &pulls, &none, &holds), ["verify-issue-2"]);
/// // A free of the hold: the item is work again.
/// let holds: HashSet<String> = ["verify-issue-2".to_owned()].into();
/// assert_eq!(free_work_of(&open, &issues, &pulls, &none, &holds), ["issue-1"]);
///
/// // main is broken: the fix item 9 comes first, also from the backlog.
/// // The verify of the fix item 8 comes before it.
/// let fix = |number, wave| Issue { labels: vec![Label { name: "broken-main".into() }], ..issue(number, wave, &[]) };
/// let issues = [issue(1, "Wave 2", &[]), issue(2, "Backlog", &[]), fix(8, "Wave 2"), fix(9, "Backlog")];
/// let pulls = [pull(40, 2), pull(41, 8)];
/// assert_eq!(
///     free_work_of(&open, &issues, &pulls, &none, &none),
///     ["verify-issue-8", "issue-9", "verify-issue-2", "issue-1"]
/// );
/// ```
pub fn free_work_of(
    open: &[Milestone],
    issues: &[Issue],
    pulls: &[Pull],
    claims: &HashSet<String>,
    holds: &HashSet<String>,
) -> Vec<String> {
    let items =
        current_wave(open).map_or_else(Vec::new, |wave| free_items(issues, wave, claims, pulls));
    let kept: HashSet<u64> = issues
        .iter()
        .filter(|i| i.no_verify())
        .map(|i| i.number)
        .collect();
    let fixes: HashSet<u64> = issues
        .iter()
        .filter(|i| i.broken_main())
        .map(|i| i.number)
        .collect();
    let mut work: Vec<String> = waiting_verifies(pulls, claims)
        .iter()
        .filter(|n| !kept.contains(n))
        .map(|n| format!("verify-issue-{n}"))
        .chain(
            fix_items(issues, claims, pulls)
                .iter()
                .map(|n| format!("issue-{n}")),
        )
        .chain(
            items
                .iter()
                .filter(|n| !fixes.contains(n))
                .map(|n| format!("issue-{n}")),
        )
        .filter(|item| !holds.contains(item))
        .collect();
    // A stable sort: the fix items first, each in its order of above.
    work.sort_by_key(|item| !usage::issue_of(item).is_some_and(|(n, _)| fixes.contains(&n)));
    work
}

/// The real world of the rollout: the riff, `gh` and the riff daemon.
pub struct Live<M> {
    pub api: Api,
    /// The session now: it can move.
    pub me: M,
    pub gh: Arc<Gh>,
}

impl<M: Fn() -> SessionUri + Send + Sync> Env for Live<M> {
    fn interval(&self) -> Duration {
        std::env::var(INTERVAL_VAR)
            .ok()
            .and_then(|s| s.parse().ok())
            .map_or(INTERVAL, Duration::from_secs)
    }

    async fn look(&self) -> Result<Option<View>> {
        // A session that left the riff is no lead, and makes no call
        // (01M3XQVJXWBC3DKAVWBPXPSGZS).
        if self.api.left() {
            return Ok(None);
        }
        let me = (self.me)();
        let sessions = self.api.who(&me, false).await?;
        let lead = sessions
            .iter()
            .any(|s| s.uri.who() == me.who() && s.uri.lead());
        if !lead {
            return Ok(None);
        }
        let host = me.place().host().to_owned();
        let repo = me.place().repo_text();
        if self.api.riff(&me).await? != RiffState::Running {
            return Ok(Some(View {
                host,
                repo,
                ..View::default()
            }));
        }
        let wave = match crate::daemon::dir() {
            Ok(dir) => wave_state(&dir, &repo, crate::daemon::STATUS_WAIT).await,
            Err(e) => WaveState::NoDaemon(format!("{e:#}")),
        };
        // The free work matters only to the ask of the wave.
        if wave != WaveState::None {
            return Ok(Some(View {
                running: true,
                host,
                repo,
                wave,
                ..View::default()
            }));
        }
        let claims = claims(&sessions);
        let holds = match me.default_thread() {
            Some(thread) => self.api.plan_show(&me, &thread).await?.holds,
            None => BTreeMap::new(),
        };
        let holds: HashSet<String> = holds.into_keys().collect();
        let (gh, of) = (self.gh.clone(), repo.clone());
        let work =
            tokio::task::spawn_blocking(move || free_work(&gh, &of, &claims, &holds)).await??;
        Ok(Some(View {
            running: true,
            work: work.len(),
            host,
            repo,
            wave,
        }))
    }

    async fn run_wave(&self) -> Result<()> {
        let me = (self.me)();
        // `riff mcp` talks on its stdout: the lines of the steps go to
        // its stderr.
        crate::wave::ask(
            &crate::daemon::dir()?,
            me.place(),
            self.api.base(),
            |line| {
                eprintln!("riff: {line}");
            },
        )
        .await?;
        Ok(())
    }

    async fn tell(&self, body: &str) -> Result<()> {
        post_lead(self.api.base(), &(self.me)(), body, Kind::Note).await
    }
}

/// Posts `body` as a note to the lead `me`, in its repository thread,
/// as the person (01M3Q5QEE4MQNCRKVJK3D54G9Z). A note wakes nobody. The
/// lead does not see its own posts, so the person posts it.
pub async fn note_lead(server: &str, me: &SessionUri, body: &str) -> Result<()> {
    post_lead(server, me, body, Kind::Note).await
}

/// Posts `body` as a message to the lead `me`, in its repository
/// thread, as the person. A message wakes the lead.
pub async fn message_lead(server: &str, me: &SessionUri, body: &str) -> Result<()> {
    post_lead(server, me, body, Kind::Message).await
}

/// Posts `body` to the lead `me`, in its repository thread, as the
/// person. A post of the kind `message` wakes the lead.
async fn post_lead(server: &str, me: &SessionUri, body: &str, kind: Kind) -> Result<()> {
    let Some(session) = me.who().session() else {
        bail!("the lead has no session");
    };
    let person = identity::person(me.place(), server)?;
    let api = Api::new(server).signed_in(None)?;
    let to: Selector = format!("session={session}").parse()?;
    api.post(&person, me.default_thread().as_ref(), &[to], body, kind)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A fake world: a riff with free work and a riff daemon. An ask of
    /// the wave makes the wave of the daemon run, unless
    /// [`World::ask_fails`] says why it fails.
    struct Fake {
        state: Mutex<World>,
    }

    #[derive(Clone)]
    struct World {
        interval: Duration,
        lead: bool,
        running: bool,
        work: usize,
        /// The wave of the repository on the daemon.
        wave: WaveState,
        /// Why an ask of the wave fails. `None`: it works, and the wave
        /// runs.
        ask_fails: Option<String>,
        /// The time of each ask of the wave.
        asks: Vec<tokio::time::Instant>,
        /// Each note to the lead.
        told: Vec<String>,
    }

    impl Fake {
        fn new(world: World) -> Arc<Self> {
            Arc::new(Fake {
                state: Mutex::new(world),
            })
        }

        fn with<T>(&self, f: impl FnOnce(&mut World) -> T) -> T {
            f(&mut self.state.lock().unwrap())
        }

        /// The second of each ask of the wave, from `begin`.
        fn asks(&self, begin: tokio::time::Instant) -> Vec<u64> {
            self.with(|w| w.asks.iter().map(|at| (*at - begin).as_secs()).collect())
        }

        fn told(&self) -> Vec<String> {
            self.with(|w| w.told.clone())
        }
    }

    impl Env for Arc<Fake> {
        fn interval(&self) -> Duration {
            self.with(|w| w.interval)
        }

        async fn tell(&self, body: &str) -> Result<()> {
            self.with(|w| w.told.push(body.to_owned()));
            Ok(())
        }

        async fn look(&self) -> Result<Option<View>> {
            Ok(self.with(|w| {
                w.lead.then(|| View {
                    running: w.running,
                    work: w.work,
                    host: "thelio".into(),
                    repo: "o/r".into(),
                    wave: w.wave.clone(),
                })
            }))
        }

        async fn run_wave(&self) -> Result<()> {
            self.with(|w| {
                w.asks.push(tokio::time::Instant::now());
                match &w.ask_fails {
                    Some(why) => bail!("{why}"),
                    None => {
                        w.wave = WaveState::Runs;
                        Ok(())
                    }
                }
            })
        }
    }

    fn world(work: usize) -> World {
        World {
            interval: INTERVAL,
            lead: true,
            running: true,
            work,
            wave: WaveState::None,
            ask_fails: None,
            asks: Vec::new(),
            told: Vec::new(),
        }
    }

    /// Runs the rollout for `secs` seconds of paused time.
    async fn run_for(fake: &Arc<Fake>, secs: u64) {
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(Duration::from_secs(secs) + Duration::from_millis(1)).await;
        task.abort();
    }

    /// The note of an ask of the wave of the fake.
    fn asked() -> String {
        text::rollout_wave("thelio", "o/r")
    }

    /// Free work and no wave on the daemon: the first look asks the
    /// daemon one time to run the wave, and tells the lead. The wave
    /// runs, so no later look asks again. The rollout offers no work to
    /// a worker in tmux: it has no such path (01M599XR3NMJD9RAJ6ZAMW6KG5).
    #[tokio::test(start_paused = true)]
    async fn it_asks_the_daemon_one_time_for_the_wave_and_starts_no_tmux_worker() {
        let fake = Fake::new(world(3));
        let begin = tokio::time::Instant::now();
        run_for(&fake, 60).await;
        assert_eq!(fake.asks(begin), [10]);
        assert_eq!(fake.told(), [asked()]);
    }

    /// 01M558EY88CACNZ55FWMRT8RP7: a daemon that takes the connection and
    /// never answers runs no wave. The look waits only the time limit
    /// for its answer.
    #[tokio::test(start_paused = true)]
    async fn a_daemon_that_never_answers_runs_no_wave_after_the_time_limit() {
        let dir = tempfile::tempdir().unwrap();
        let listener = tokio::net::UnixListener::bind(crate::daemon::socket(dir.path())).unwrap();
        // It takes each connection, keeps it open, and says nothing.
        let silent = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((conn, _)) = listener.accept().await {
                held.push(conn);
            }
        });
        let limit = crate::daemon::STATUS_WAIT;
        let begin = tokio::time::Instant::now();
        let state = wave_state(dir.path(), "o/r", limit).await;
        assert_eq!(state, WaveState::NoDaemon(text::daemon_silent(limit)));
        assert!(begin.elapsed() >= limit, "{:?}", begin.elapsed());
        assert!(begin.elapsed() < limit * 2, "{:?}", begin.elapsed());
        silent.abort();
    }

    /// 01M3Q5QEBTNM90SPYXNVTT7RJA: while the riff or the repository is
    /// paused, the rollout asks nothing. The resume asks.
    #[tokio::test(start_paused = true)]
    async fn a_pause_asks_nothing_and_the_resume_asks() {
        let fake = Fake::new(World {
            running: false,
            ..world(5)
        });
        let begin = tokio::time::Instant::now();
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(Duration::from_secs(65)).await;
        assert!(fake.with(|w| w.asks.is_empty()), "no ask while paused");
        fake.with(|w| w.running = true);
        tokio::time::sleep(Duration::from_secs(10)).await;
        task.abort();
        assert_eq!(fake.asks(begin), [70], "the resume asks for the wave");
    }

    #[tokio::test(start_paused = true)]
    async fn only_the_lead_asks_for_the_wave() {
        let fake = Fake::new(World {
            lead: false,
            ..world(3)
        });
        run_for(&fake, 60).await;
        assert!(fake.with(|w| w.asks.is_empty() && w.told.is_empty()));
    }

    /// A wave of the repository runs on the daemon: no ask, and no note.
    #[tokio::test(start_paused = true)]
    async fn no_ask_while_a_wave_of_the_repository_runs() {
        let fake = Fake::new(World {
            wave: WaveState::Runs,
            ..world(3)
        });
        run_for(&fake, 120).await;
        assert!(fake.with(|w| w.asks.is_empty() && w.told.is_empty()));
    }

    #[tokio::test(start_paused = true)]
    async fn no_ask_with_no_free_work() {
        let fake = Fake::new(world(0));
        run_for(&fake, 60).await;
        assert!(fake.with(|w| w.asks.is_empty()));
    }

    /// The ask fails: the lead gets one note with the reason, and the
    /// rollout asks again at each look. A new reason gives a new note.
    /// Then the ask works: the note of the ask, and the wave runs
    /// (01M5515CA4CZ6KV4ZD6Y0744Z7).
    #[tokio::test(start_paused = true)]
    async fn a_failed_ask_gives_one_note_for_each_reason_and_the_rollout_asks_again() {
        let fake = Fake::new(World {
            ask_fails: Some("no session image".into()),
            ..world(2)
        });
        let begin = tokio::time::Instant::now();
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(Duration::from_secs(35)).await;
        fake.with(|w| w.ask_fails = Some("the daemon is gone".into()));
        tokio::time::sleep(Duration::from_secs(20)).await;
        fake.with(|w| w.ask_fails = None);
        tokio::time::sleep(Duration::from_secs(30)).await;
        task.abort();
        assert_eq!(fake.asks(begin), [10, 20, 30, 40, 50, 60]);
        let no = |why| text::rollout_no_wave("thelio", "o/r", why);
        assert_eq!(
            fake.told(),
            [no("no session image"), no("the daemon is gone"), asked()]
        );
    }

    /// No daemon answers: the lead gets one note with the reason, and
    /// the rollout asks nothing (01M5515CA4CZ6KV4ZD6Y0744Z7).
    #[tokio::test(start_paused = true)]
    async fn no_daemon_gives_one_note_and_no_ask() {
        let fake = Fake::new(World {
            wave: WaveState::NoDaemon("no riff daemon listens".into()),
            ..world(2)
        });
        run_for(&fake, 60).await;
        assert!(fake.with(|w| w.asks.is_empty()));
        assert_eq!(
            fake.told(),
            [text::rollout_no_wave(
                "thelio",
                "o/r",
                "no riff daemon listens"
            )]
        );
    }

    #[test]
    fn the_json_of_gh_parses() {
        let issues: Vec<Issue> = serde_json::from_str(
            r#"[{"number":266,"body":"Needs: #259","comments":[],"milestone":{"number":14,"title":"Wave 13","description":"","dueOn":null}},
                {"number":259,"body":"","comments":[{"author":{"login":"m"},"body":"Merged in #270 (abc)"}],"milestone":{"number":14,"title":"Wave 13","description":"","dueOn":null}},
                {"number":265,"body":"Needs: nothing","comments":[],"milestone":{"number":14,"title":"Wave 13","description":"","dueOn":null}},
                {"number":210,"body":"","comments":[],"milestone":null}]"#,
        )
        .unwrap();
        // 259 is merged but open, so 266 waits.
        assert_eq!(free_items(&issues, "Wave 13", &HashSet::new(), &[]), [265]);
        let pulls: Vec<Pull> = serde_json::from_str(
            r#"[{"number":267,"headRefName":"worktree-issue-265","isDraft":false,
                 "statusCheckRollup":[{"__typename":"CheckRun","name":"Gate","conclusion":"SUCCESS"},
                                      {"__typename":"StatusContext","context":"riff/verify","state":"SUCCESS"}]},
                {"number":268,"headRefName":"worktree-issue-262","isDraft":false,"statusCheckRollup":[]}]"#,
        )
        .unwrap();
        assert_eq!(waiting_verifies(&pulls, &HashSet::new()), [262]);
        // The pull request of 265 waits for the merge: no build.
        assert!(free_items(&issues, "Wave 13", &HashSet::new(), &pulls).is_empty());
    }

    /// The author released the item at its verify request. The item is
    /// work for a verify, and it counts one time
    /// (01M3Z9N5HHHS1E17NFGMVBKZ0K). After a fail it is a build again.
    /// The same is true for a pull request whose branch names no issue:
    /// its trailer `Issue: #12` names the item
    /// (01M51BF5S2YKB8JG10DF6YVT42).
    #[test]
    fn an_item_with_an_open_pull_request_and_no_claim_counts_one_time() {
        let issues: Vec<Issue> = serde_json::from_str(
            r#"[{"number":12,"body":"","comments":[],"milestone":{"title":"Wave 3"}}]"#,
        )
        .unwrap();
        let heads = [
            r#""headRefName":"worktree-issue-12""#,
            r#""headRefName":"release-v1.5.0","body":"Refs #12\n\nLevel: minor.\n\nIssue: #12\nMilestone: Wave 3\n""#,
        ];
        for head in heads {
            let pulls = |status: &str| -> Vec<Pull> {
                serde_json::from_str(&format!(
                    r#"[{{"number":40,{head},"headRefOid":"1a2b3c4d","isDraft":false,
                         "statusCheckRollup":[{{"__typename":"CheckRun","name":"Gate","conclusion":"SUCCESS"}}{status}]}}]"#
                ))
                .unwrap()
            };
            let status = |state: &str| {
                format!(
                    r#",{{"__typename":"StatusContext","context":"riff/verify","state":"{state}","targetUrl":"https://c"}}"#
                )
            };
            let work = |pulls: &[Pull], claims: &[&str]| {
                let claims: HashSet<String> = claims.iter().map(|c| (*c).to_owned()).collect();
                (
                    free_items(&issues, "Wave 3", &claims, pulls),
                    waiting_verifies(pulls, &claims),
                )
            };
            // Asked: one verify, no build.
            let asked = pulls("");
            assert_eq!(work(&asked, &[]), (vec![], vec![12]), "{head}");
            // A verifier holds it: no work.
            assert_eq!(work(&asked, &["verify-issue-12"]), (vec![], vec![]));
            // Passed: the merge waits, no work.
            let passed = pulls(&status("SUCCESS"));
            assert_eq!(work(&passed, &[]), (vec![], vec![]), "{head}");
            // Failed: the item is free for a build, and no verify waits.
            let failed = pulls(&status("FAILURE"));
            assert_eq!(work(&failed, &[]), (vec![12], vec![]), "{head}");
            assert_eq!(
                failed[0].verify_status().unwrap().url.as_deref(),
                Some("https://c")
            );
            // The next session holds it: no work.
            assert_eq!(work(&failed, &["issue-12"]), (vec![], vec![]));
            // The claim of the item names the pull request.
            assert!(
                claim_line("issue-12", &passed)
                    .unwrap()
                    .contains("Release issue-12")
            );
        }
    }

    /// A held item is no free work (01M51A55Q2MDZT38JDCEJ1QECP): the
    /// rollout asks for no wave for it. A hold names one item. A free
    /// makes the item free work at the next look.
    #[test]
    fn a_held_item_is_no_free_work() {
        let open = [Milestone {
            title: "Wave 3".into(),
        }];
        let issues: Vec<Issue> = serde_json::from_str(
            r#"[{"number":12,"body":"","comments":[],"milestone":{"title":"Wave 3"}},
                {"number":13,"body":"","comments":[],"milestone":{"title":"Wave 3"}}]"#,
        )
        .unwrap();
        // The pull request of 13 waits for a verify.
        let pulls: Vec<Pull> = serde_json::from_str(
            r#"[{"number":40,"headRefName":"worktree-issue-13","isDraft":false,"statusCheckRollup":[]}]"#,
        )
        .unwrap();
        let work = |holds: &[&str]| {
            let holds: HashSet<String> = holds.iter().map(|h| (*h).to_owned()).collect();
            free_work_of(&open, &issues, &pulls, &HashSet::new(), &holds)
        };
        assert_eq!(work(&[]), ["verify-issue-13", "issue-12"]);
        assert_eq!(work(&["issue-12"]), ["verify-issue-13"]);
        // A hold of issue-13 does not stop its verify.
        assert_eq!(work(&["issue-13"]), ["verify-issue-13", "issue-12"]);
        assert_eq!(work(&["verify-issue-13"]), ["issue-12"]);
        assert!(work(&["issue-12", "verify-issue-13"]).is_empty());
    }
}
