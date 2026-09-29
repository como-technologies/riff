//! riff starts workers by itself when the wave has free work.
//!
//! # Design
//!
//! `riff mcp` of the lead runs the rollout (01M3Q5QE01DB0FJQJWFKR450KQ).
//! So the start of work does not wait for an agent that remembers a
//! step. Once each interval ([`crate::settings::workers_interval`],
//! 10 seconds by default, 01M3Q5QE9H42FQKEDC5G9GKCWD), it looks at the
//! riff, and starts at most one worker:
//!
//! ```mermaid
//! flowchart TD
//!     T["each interval"] --> L{"this session is the lead?"}
//!     L -- no --> T
//!     L -- yes --> R{"the riff runs?"}
//!     R -- "no: paused" --> T
//!     R -- yes --> P{"a machine with room?"}
//!     P -- no --> T
//!     P -- yes --> W["free work (gh): free items of the current wave,<br/>pull requests that wait for a verify"]
//!     W --> I{"free work, and no idle worker?"}
//!     I -- no --> T
//!     I -- yes --> S["start 1 worker on the machine<br/>with the most free capacity"]
//!     S --> N["a note to the lead: host, pane, session"]
//!     N --> T
//! ```
//!
//! - **Free work** ([`free_items`], [`waiting_verifies`],
//!   01M3Q5QE2EWGKVD57Y6YCWA4BR). The current wave is the open milestone
//!   `Wave N` with the lowest N. A free item is an open issue of it that
//!   no session claims, that has no comment `Merged in #`, and whose
//!   `Needs:` issues are all closed. A pull request waits for a verify
//!   when its branch names an issue, it is not a draft, its head has no
//!   status `riff/verify`, and no session claims `verify-issue-N` for
//!   it.
//! - **Idle workers** ([`idle`]). A worker pane of the user with no
//!   claim, also one that did not join yet, and a live worker of
//!   another user with no claim. A worker that the server asked to stop
//!   is not idle. riff starts a worker only when no worker is idle
//!   (01M3Q5QEJNP1JGQM7VXXEBJ9J9). So a new worker must claim before
//!   the next one starts. When no worker takes the counted work, one
//!   worker waits idle, the server keeps it (#259), and riff starts no
//!   more: no loop of starts and stops.
//! - **Machines** ([`Place`], [`pick`], 01M3Q5QE76BZ27SZ14FFE8HM1G).
//!   The machine of the lead, when the lead runs in tmux, and each live
//!   workers host of the user. A machine has room when its workers are
//!   fewer than its limit and it is not busy
//!   ([`crate::machine::Machine::busy`]). riff picks the machine with the
//!   most free capacity. On a tie, the machine of the lead wins.
//! - **Pause** (01M3Q5QEBTNM90SPYXNVTT7RJA). While the riff is paused,
//!   riff starts no worker.
//! - **Notes** (01M3Q5QEE4MQNCRKVJK3D54G9Z). On the machine of the lead,
//!   riff posts a note to the lead with the host, the pane and the
//!   session. A workers host posts the same note when it starts a
//!   worker.
//!
//! riff never stops a worker here. The server stops idle workers
//! (#259).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use riff_core::name::SessionUri;
use riff_core::selector::Selector;
use riff_core::wire::{Kind, RiffState, SessionInfo};
use serde::Deserialize;

use crate::api::Api;
use crate::host::{self, Request};
use crate::machine::Machine;
use crate::pr::Gh;
use crate::terminal::{Terminal, Tmux, WorkerPane};
use crate::{identity, settings, text, worker};

/// How often a rollout that is off looks at its setting again.
pub const OFF_WAIT: Duration = Duration::from_secs(10);

/// A machine that can run workers.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    /// The host name.
    pub host: String,
    /// The session of its workers host. `None` for the machine of the
    /// lead.
    pub session: Option<String>,
    /// The most workers on the machine.
    pub limit: u16,
    /// The workers that run there.
    pub workers: usize,
    /// The numbers of the machine, if it tells them.
    pub machine: Option<Machine>,
}

impl Place {
    /// True when the machine can take one more worker: fewer workers
    /// than its limit, and not busy.
    pub fn room(&self) -> bool {
        self.workers < usize::from(self.limit) && !self.machine.is_some_and(|m| m.busy())
    }

    /// The free capacity: the score less the workers. A machine that
    /// does not tell its numbers counts its limit as its score.
    pub fn free(&self) -> f64 {
        match self.machine {
            Some(m) => m.free(self.workers),
            None => f64::from(self.limit) - self.workers as f64,
        }
    }
}

/// What the rollout sees at one look.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct View {
    /// True when the riff runs.
    pub running: bool,
    /// The free work: the free items and the pull requests that wait
    /// for a verify.
    pub work: usize,
    /// The idle workers.
    pub idle: usize,
    /// The machines, the machine of the lead first.
    pub places: Vec<Place>,
}

/// The machine with room and the most free capacity, or `None`. On a
/// tie, the first one wins.
///
/// ```
/// use riff::machine::Machine;
/// use riff::rollout::{Place, pick};
///
/// let place = |host: &str, cores, workers| Place {
///     host: host.into(),
///     session: None,
///     limit: 4,
///     workers,
///     machine: Some(Machine { cores, mhz: 3000, mem_gb: 64, load: 0.0 }),
/// };
/// let places = [place("thelio", 32, 0), place("pangolin", 8, 0)];
/// assert_eq!(pick(&places), Some(0));
/// // thelio is at its limit.
/// let places = [place("thelio", 32, 4), place("pangolin", 8, 0)];
/// assert_eq!(pick(&places), Some(1));
/// ```
pub fn pick(places: &[Place]) -> Option<usize> {
    places
        .iter()
        .enumerate()
        .filter(|(_, p)| p.room())
        .fold(None, |best: Option<(usize, f64)>, (i, p)| match best {
            Some((_, free)) if free >= p.free() => best,
            _ => Some((i, p.free())),
        })
        .map(|(i, _)| i)
}

/// The machine for one new worker, or `None` when riff starts none:
/// the riff is paused, there is no free work, a worker is idle
/// (01M3Q5QEJNP1JGQM7VXXEBJ9J9), or no machine has room.
///
/// ```
/// use riff::rollout::{Place, View, decide};
///
/// let here = Place { host: "thelio".into(), session: None, limit: 2, workers: 0, machine: None };
/// let view = View { running: true, work: 2, idle: 0, places: vec![here] };
/// assert_eq!(decide(&view), Some(0));
/// assert_eq!(decide(&View { running: false, ..view.clone() }), None);
/// assert_eq!(decide(&View { work: 0, ..view.clone() }), None);
/// assert_eq!(decide(&View { idle: 1, ..view.clone() }), None);
/// ```
pub fn decide(view: &View) -> Option<usize> {
    if !view.running || view.work == 0 || view.idle > 0 {
        return None;
    }
    pick(&view.places)
}

/// What the rollout needs from the world. [`Live`] is the real one.
pub trait Env {
    /// The time between two looks. Zero turns the rollout off.
    fn interval(&self) -> Duration;
    /// One look, or `None` when this session is not the lead.
    fn look(&self) -> impl Future<Output = Result<Option<View>>> + Send;
    /// Starts one worker on `place`.
    fn start(&self, place: &Place) -> impl Future<Output = Result<()>> + Send;
}

/// Runs the rollout until the task ends. It waits one interval, looks,
/// and starts at most one worker, again and again. So riff starts at
/// most one worker each interval. It prints an error once, not again
/// until the error changes.
pub async fn run(env: impl Env) {
    let mut last_error = None;
    loop {
        let every = env.interval();
        if every.is_zero() {
            tokio::time::sleep(OFF_WAIT).await;
            continue;
        }
        tokio::time::sleep(every).await;
        let step = async {
            let Some(view) = env.look().await? else {
                return Ok(());
            };
            if let Some(i) = decide(&view) {
                env.start(&view.places[i]).await?;
            }
            anyhow::Ok(())
        };
        match step.await {
            Ok(()) => last_error = None,
            Err(e) => {
                let e = format!("{e:#}");
                if last_error.as_ref() != Some(&e) {
                    eprintln!("riff: the rollout of workers: {e}");
                }
                last_error = Some(e);
            }
        }
    }
}

/// An open milestone.
#[derive(Debug, Clone, Deserialize)]
pub struct Milestone {
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

/// An open issue, as `gh issue list --json
/// number,body,comments,milestone` gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct Issue {
    pub number: u64,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub comments: Vec<Comment>,
    #[serde(default)]
    pub milestone: Option<Milestone>,
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
/// repository and the `claims` of all sessions: an open issue of the
/// wave that no session claims, with no comment `Merged in #`, and with
/// each issue of its `Needs:` line closed. An open need blocks the item,
/// also a need outside the wave, and also a need that is merged but not
/// closed.
///
/// ```
/// use std::collections::HashSet;
/// use riff::rollout::{Comment, Issue, Milestone, free_items};
///
/// let issue = |number, wave: &str, body: &str, merged| Issue {
///     number,
///     body: body.into(),
///     comments: if merged { vec![Comment { body: "Merged in #9 (abc)".into() }] } else { vec![] },
///     milestone: Some(Milestone { title: wave.into() }),
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
/// assert_eq!(free_items(&open, "Wave 2", &claims), [1, 4]);
/// ```
pub fn free_items(open: &[Issue], wave: &str, claims: &HashSet<String>) -> Vec<u64> {
    let numbers: HashSet<u64> = open.iter().map(|i| i.number).collect();
    open.iter()
        .filter(|i| i.milestone.as_ref().is_some_and(|m| m.title == wave))
        .filter(|i| !i.merged())
        .filter(|i| !claims.contains(&format!("issue-{}", i.number)))
        .filter(|i| needs(&i.body).iter().all(|n| !numbers.contains(n)))
        .map(|i| i.number)
        .collect()
}

/// An open pull request, as `gh pr list --json
/// number,headRefName,isDraft,statusCheckRollup` gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct Pull {
    pub number: u64,
    #[serde(rename = "headRefName")]
    pub branch: String,
    #[serde(rename = "isDraft", default)]
    pub draft: bool,
    #[serde(rename = "statusCheckRollup", default)]
    pub checks: Vec<Check>,
}

/// A status or a check of the head of a pull request. Only a status has
/// a context.
#[derive(Debug, Clone, Deserialize)]
pub struct Check {
    #[serde(default)]
    pub context: Option<String>,
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

/// The pull requests that wait for a verify: the branch names an issue,
/// not a draft, no status `riff/verify` on the head, and no session
/// claims `verify-issue-N` for the issue of the branch. A pull request
/// that a person opened by hand names no issue, so it is no work.
///
/// ```
/// use std::collections::HashSet;
/// use riff::rollout::{Check, Pull, waiting_verifies};
///
/// let pull = |number, branch: &str, verified| Pull {
///     number,
///     branch: branch.into(),
///     draft: false,
///     checks: if verified { vec![Check { context: Some("riff/verify".into()) }] } else { vec![] },
/// };
/// let pulls = [
///     pull(40, "worktree-issue-12", false),
///     pull(41, "worktree-issue-13", true),
///     pull(42, "worktree-issue-14", false),
///     Pull { draft: true, ..pull(43, "worktree-issue-15", false) },
///     pull(44, "release-v0.8.0", false),
/// ];
/// let claims: HashSet<String> = ["verify-issue-14".to_owned()].into();
/// assert_eq!(waiting_verifies(&pulls, &claims), [40]);
/// ```
pub fn waiting_verifies(pulls: &[Pull], claims: &HashSet<String>) -> Vec<u64> {
    pulls
        .iter()
        .filter(|p| !p.draft)
        .filter(|p| {
            !p.checks
                .iter()
                .any(|c| c.context.as_deref() == Some(crate::pr::VERIFY_CONTEXT))
        })
        .filter(|p| {
            branch_issue(&p.branch).is_some_and(|n| !claims.contains(&format!("verify-issue-{n}")))
        })
        .map(|p| p.number)
        .collect()
}

/// Each claim of each session in `sessions`.
pub fn claims(sessions: &[SessionInfo]) -> HashSet<String> {
    sessions
        .iter()
        .flat_map(|s| s.uri.claims().iter().cloned())
        .collect()
}

/// The idle workers: each worker pane of `user` in `panes` whose session
/// holds no claim, also one that is not in `sessions` yet, and each
/// live worker of another user with no claim. A worker that the server
/// asked to stop is not idle: it goes away. A pane can hold the short
/// session ID of a host status.
pub fn idle(sessions: &[SessionInfo], user: &str, panes: &[WorkerPane]) -> usize {
    // A worker that holds a claim is busy. A worker that the server
    // asked to stop goes away.
    let busy = |id: &str| {
        sessions.iter().any(|s| {
            s.uri.who().session().is_some_and(|s| s.starts_with(id))
                && (!s.uri.claims().is_empty() || s.stopping)
        })
    };
    let mine = panes.iter().filter(|p| !busy(&p.session)).count();
    let others = sessions
        .iter()
        .filter(|s| s.worker && s.live && s.uri.who().user() != user)
        .filter(|s| s.uri.claims().is_empty() && !s.stopping)
        .count();
    mine + others
}

/// The free work of the repository `repo` (`OWNER/REPO`), with `gh`:
/// the free items of the current wave and the pull requests that wait
/// for a verify.
pub fn free_work(gh: &Gh, repo: &str, claims: &HashSet<String>) -> Result<usize> {
    let open: Vec<Milestone> = gh.json(&[
        "api",
        &format!("repos/{repo}/milestones?state=open&per_page=100"),
    ])?;
    let items = match current_wave(&open) {
        Some(wave) => {
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
                "number,body,comments,milestone",
            ])?;
            free_items(&issues, wave, claims).len()
        }
        None => 0,
    };
    let pulls: Vec<Pull> = gh.json(&[
        "pr",
        "list",
        "--repo",
        repo,
        "--state",
        "open",
        "--limit",
        "100",
        "--json",
        "number,headRefName,isDraft,statusCheckRollup",
    ])?;
    Ok(items + waiting_verifies(&pulls, claims).len())
}

/// The real world of the rollout: the riff, `gh`, and tmux.
pub struct Live<M> {
    pub api: Api,
    /// The session now: it can move.
    pub me: M,
    /// The tmux of the lead, if it runs in tmux.
    pub tmux: Option<Tmux>,
    pub claude: PathBuf,
    pub gh: Arc<Gh>,
}

impl<M: Fn() -> SessionUri + Send + Sync> Env for Live<M> {
    fn interval(&self) -> Duration {
        let seconds = settings::path()
            .and_then(|p| settings::workers_interval(&p))
            .unwrap_or(settings::WORKERS_INTERVAL);
        Duration::from_secs(u64::from(seconds))
    }

    async fn look(&self) -> Result<Option<View>> {
        let me = (self.me)();
        let sessions = self.api.who(&me, false).await?;
        let lead = sessions
            .iter()
            .any(|s| s.uri.who() == me.who() && s.uri.lead());
        if !lead {
            return Ok(None);
        }
        if self.api.riff(&me).await? != RiffState::Running {
            return Ok(Some(View::default()));
        }
        let user = me.who().user();
        let mut places = Vec::new();
        let mut panes = Vec::new();
        let limit = settings::workers_limit(&settings::path()?)?;
        if let Some(tmux) = &self.tmux
            && limit > 0
        {
            let here = tmux.worker_panes()?;
            places.push(Place {
                host: me.place().host().to_owned(),
                session: None,
                limit,
                workers: here.len(),
                machine: Some(Machine::here()),
            });
            panes.extend(here);
        }
        for (info, status) in host::hosts(&sessions, user) {
            if info.uri.place().host() == me.place().host() {
                continue;
            }
            places.push(Place {
                host: info.uri.place().host().to_owned(),
                session: info.uri.who().session().map(str::to_owned),
                limit: status.limit,
                workers: status.workers.len(),
                machine: status.machine,
            });
            panes.extend(status.workers.iter().map(|(pane, short)| WorkerPane {
                pane: pane.clone(),
                session: short.clone(),
            }));
        }
        let idle = idle(&sessions, user, &panes);
        if !places.iter().any(Place::room) {
            return Ok(Some(View {
                running: true,
                idle,
                places,
                work: 0,
            }));
        }
        let claims = claims(&sessions);
        let (gh, repo) = (self.gh.clone(), me.place().repo_text());
        let work = tokio::task::spawn_blocking(move || free_work(&gh, &repo, &claims)).await??;
        Ok(Some(View {
            running: true,
            work,
            idle,
            places,
        }))
    }

    async fn start(&self, place: &Place) -> Result<()> {
        let me = (self.me)();
        match &place.session {
            Some(host) => {
                self.api
                    .tell(&me, host, &Request::Start(1).to_string())
                    .await?;
            }
            None => {
                let Some(tmux) = &self.tmux else {
                    bail!("the lead does not run in tmux");
                };
                let dir = identity::working_dir()?;
                // The start runs git and tmux: keep them off the runtime.
                let (tmux, claude, base) = (
                    tmux.clone(),
                    self.claude.clone(),
                    self.api.base().to_owned(),
                );
                let started = tokio::task::spawn_blocking(move || {
                    worker::start(&tmux, 1, &claude, &base, &dir)
                })
                .await??;
                let started = match started {
                    Ok(started) => started,
                    Err(why) => bail!(why),
                };
                note_lead(
                    self.api.base(),
                    &me,
                    &text::host_started(&place.host, &started),
                )
                .await?;
            }
        }
        Ok(())
    }
}

/// Posts `body` as a note to the lead `me`, in its repository thread,
/// as the person (01M3Q5QEE4MQNCRKVJK3D54G9Z). A note wakes nobody. The
/// lead does not see its own posts, so the person posts it.
pub async fn note_lead(server: &str, me: &SessionUri, body: &str) -> Result<()> {
    let Some(session) = me.who().session() else {
        bail!("the lead has no session");
    };
    let person = identity::person(me.place(), server)?;
    let api = Api::new(server).signed_in(None)?;
    let to: Selector = format!("session={session}").parse()?;
    api.post(
        &person,
        me.default_thread().as_ref(),
        &[to],
        body,
        Kind::Note,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A fake world: a riff with free work, and machines. Each start adds
    /// a worker to its machine. The new worker is idle until it claims an
    /// item, [`World::claim_after`] after its start. With
    /// [`World::keep_idle`], the fake stops idle workers as the server
    /// does (#259).
    struct Fake {
        state: Mutex<World>,
    }

    #[derive(Clone)]
    struct World {
        interval: Duration,
        lead: bool,
        running: bool,
        work: usize,
        /// Idle workers that were there before, and never claim.
        idle: usize,
        places: Vec<Place>,
        /// How long a new worker takes to claim an item. `None`: it never
        /// claims, because no worker takes the counted work.
        claim_after: Option<Duration>,
        /// The idle workers that the server keeps on each host after 60
        /// seconds of idle time. `None`: the server stops none.
        keep_idle: Option<usize>,
        /// Each new worker that did not claim: its host and its start.
        new: Vec<(String, tokio::time::Instant)>,
        /// The host of each start, with the time of the start.
        starts: Vec<(String, tokio::time::Instant)>,
        /// The workers that the server stopped.
        stops: usize,
    }

    impl World {
        /// The claims and the idle stops up to now.
        fn advance(&mut self) {
            let now = tokio::time::Instant::now();
            if let Some(after) = self.claim_after {
                while self.work > 0
                    && let Some(i) = self.new.iter().position(|(_, at)| now >= *at + after)
                {
                    self.new.remove(i);
                    self.work -= 1;
                }
            }
            let Some(keep) = self.keep_idle else {
                return;
            };
            for place in &mut self.places {
                let mut idle: Vec<usize> = (0..self.new.len())
                    .filter(|&i| self.new[i].0 == place.host)
                    .collect();
                // Keep the newest ones; stop the others past 60 s.
                idle.sort_by_key(|&i| std::cmp::Reverse(self.new[i].1));
                let stop: Vec<usize> = idle
                    .into_iter()
                    .skip(keep)
                    .filter(|&i| now >= self.new[i].1 + Duration::from_secs(60))
                    .collect();
                for i in stop.into_iter().rev() {
                    self.new.remove(i);
                    place.workers -= 1;
                    self.stops += 1;
                }
            }
        }
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

        fn hosts(&self) -> Vec<String> {
            self.with(|w| w.starts.iter().map(|(h, _)| h.clone()).collect())
        }
    }

    impl Env for Arc<Fake> {
        fn interval(&self) -> Duration {
            self.with(|w| w.interval)
        }

        async fn look(&self) -> Result<Option<View>> {
            Ok(self.with(|w| {
                w.advance();
                w.lead.then(|| View {
                    running: w.running,
                    work: w.work,
                    idle: w.idle + w.new.len(),
                    places: w.places.clone(),
                })
            }))
        }

        async fn start(&self, place: &Place) -> Result<()> {
            self.with(|w| {
                let p = w.places.iter_mut().find(|p| p.host == place.host).unwrap();
                p.workers += 1;
                let now = tokio::time::Instant::now();
                w.new.push((place.host.clone(), now));
                w.starts.push((place.host.clone(), now));
            });
            Ok(())
        }
    }

    fn machine(cores: u16, mhz: u32, mem_gb: u32) -> Option<Machine> {
        Some(Machine {
            cores,
            mhz,
            mem_gb,
            load: 0.0,
        })
    }

    fn thelio() -> Place {
        Place {
            host: "thelio".into(),
            session: None,
            limit: 4,
            workers: 0,
            machine: machine(32, 6000, 128),
        }
    }

    fn pangolin() -> Place {
        Place {
            host: "pangolin".into(),
            session: Some("h1".into()),
            limit: 4,
            workers: 0,
            machine: machine(16, 4500, 32),
        }
    }

    fn world(work: usize) -> World {
        World {
            interval: Duration::from_secs(10),
            lead: true,
            running: true,
            work,
            idle: 0,
            places: vec![thelio(), pangolin()],
            claim_after: Some(Duration::from_secs(5)),
            keep_idle: None,
            new: Vec::new(),
            starts: Vec::new(),
            stops: 0,
        }
    }

    /// Runs the rollout for `secs` seconds of paused time.
    async fn run_for(fake: &Arc<Fake>, secs: u64) {
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(Duration::from_secs(secs) + Duration::from_millis(1)).await;
        task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn it_starts_one_worker_each_interval_until_each_item_has_one() {
        let fake = Fake::new(world(3));
        let begin = tokio::time::Instant::now();
        run_for(&fake, 60).await;
        let starts = fake.with(|w| w.starts.clone());
        assert_eq!(starts.len(), 3, "one worker for each free item");
        let secs: Vec<u64> = starts
            .iter()
            .map(|(_, at)| (*at - begin).as_secs())
            .collect();
        assert_eq!(secs, [10, 20, 30]);
    }

    #[tokio::test(start_paused = true)]
    async fn the_interval_setting_changes_the_rate() {
        let fake = Fake::new(World {
            interval: Duration::from_secs(30),
            ..world(3)
        });
        run_for(&fake, 60).await;
        assert_eq!(fake.hosts().len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn an_interval_of_zero_starts_nothing() {
        let fake = Fake::new(World {
            interval: Duration::ZERO,
            ..world(3)
        });
        run_for(&fake, 60).await;
        assert!(fake.hosts().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn a_pause_stops_the_rollout() {
        let fake = Fake::new(world(5));
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(Duration::from_secs(25)).await;
        fake.with(|w| w.running = false);
        tokio::time::sleep(Duration::from_secs(60)).await;
        assert_eq!(fake.hosts().len(), 2, "no start while paused");
        fake.with(|w| w.running = true);
        tokio::time::sleep(Duration::from_secs(10)).await;
        task.abort();
        assert_eq!(fake.hosts().len(), 3, "the resume starts the rollout again");
    }

    #[tokio::test(start_paused = true)]
    async fn only_the_lead_starts_workers() {
        let fake = Fake::new(World {
            lead: false,
            ..world(3)
        });
        run_for(&fake, 60).await;
        assert!(fake.hosts().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn no_worker_starts_while_a_worker_is_idle() {
        let fake = Fake::new(World {
            idle: 1,
            ..world(3)
        });
        run_for(&fake, 60).await;
        assert!(fake.hosts().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn the_next_worker_waits_until_the_new_one_claims() {
        let fake = Fake::new(World {
            claim_after: Some(Duration::from_secs(25)),
            ..world(3)
        });
        let begin = tokio::time::Instant::now();
        run_for(&fake, 100).await;
        let secs: Vec<u64> = fake.with(|w| {
            w.starts
                .iter()
                .map(|(_, at)| (*at - begin).as_secs())
                .collect()
        });
        // Each new worker claims 25 s after its start.
        assert_eq!(secs, [10, 40, 70]);
    }

    /// Work that no worker takes gives one idle worker, which the server
    /// keeps (#259): no loop of starts and stops.
    #[tokio::test(start_paused = true)]
    async fn work_that_no_worker_takes_starts_no_endless_loop() {
        let fake = Fake::new(World {
            claim_after: None,
            keep_idle: Some(1),
            ..world(3)
        });
        run_for(&fake, 3600).await;
        assert_eq!(fake.hosts().len(), 1);
        assert_eq!(fake.with(|w| w.stops), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn the_limit_of_each_machine_caps_the_rollout() {
        let mut w = world(20);
        w.places[0].limit = 2;
        w.places[1].limit = 1;
        let fake = Fake::new(w);
        run_for(&fake, 300).await;
        let hosts = fake.hosts();
        assert_eq!(hosts.len(), 3);
        assert_eq!(hosts.iter().filter(|h| *h == "thelio").count(), 2);
        assert_eq!(hosts.iter().filter(|h| *h == "pangolin").count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn the_first_workers_go_to_the_bigger_machine() {
        let mut w = world(20);
        w.places[0].limit = 20;
        w.places[1].limit = 20;
        let fake = Fake::new(w);
        run_for(&fake, 100).await;
        let hosts = fake.hosts();
        assert_eq!(hosts.len(), 10);
        // thelio scores 64 and pangolin 24: pangolin gets none of the
        // first 10.
        assert!(hosts.iter().all(|h| h == "thelio"), "{hosts:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn the_smaller_machine_gets_workers_when_it_has_the_most_room() {
        let mut w = world(3);
        w.places[0].workers = 60;
        w.places[0].limit = 70;
        let fake = Fake::new(w);
        run_for(&fake, 30).await;
        // thelio has 64 - 60 = 4 free, pangolin 24.
        assert_eq!(fake.hosts(), ["pangolin", "pangolin", "pangolin"]);
    }

    #[test]
    fn a_busy_machine_gets_no_worker() {
        let mut busy = thelio();
        busy.machine = busy.machine.map(|m| Machine { load: 40.0, ..m });
        assert_eq!(pick(&[busy.clone(), pangolin()]), Some(1));
        assert_eq!(pick(&[busy]), None);
    }

    #[test]
    fn a_host_with_no_numbers_counts_its_limit() {
        let old = Place {
            machine: None,
            ..pangolin()
        };
        assert_eq!(old.free(), 4.0);
        assert!(old.room());
    }

    #[test]
    fn on_a_tie_the_machine_of_the_lead_wins() {
        let twin = Place {
            host: "twin".into(),
            session: Some("h2".into()),
            ..thelio()
        };
        assert_eq!(pick(&[thelio(), twin]), Some(0));
    }

    fn info(uri: &str, worker: bool) -> SessionInfo {
        SessionInfo {
            uri: uri.parse().unwrap(),
            live: true,
            idle_secs: 0,
            status: None,
            worker,
            stopping: false,
            claims_secs: 0,
        }
    }

    #[test]
    fn idle_counts_new_panes_and_workers_of_other_users() {
        let sessions = [
            info(
                "riff://mike@thelio/o/r?session=aaaa1111-x&claim=issue-1",
                true,
            ),
            info("riff://mike@thelio/o/r?session=bbbb2222-x", true),
            info("riff://brett@kadomony/o/r?session=cccc3333-x", true),
            info(
                "riff://brett@kadomony/o/r?session=dddd4444-x&claim=issue-2",
                true,
            ),
            info(
                "riff://brett@kadomony/o/r?session=eeee5555-x&lead=true",
                false,
            ),
        ];
        let pane = |p: &str, s: &str| WorkerPane {
            pane: p.into(),
            session: s.into(),
        };
        let panes = [
            pane("%1", "aaaa1111"),
            pane("%2", "bbbb2222-x"),
            pane("%3", "ffff6666-not-joined"),
        ];
        // bbbb and ffff of mike, cccc of brett.
        assert_eq!(idle(&sessions, "mike", &panes), 3);
        // A worker that the server asks to stop is not idle.
        let mut stopping = sessions.clone();
        stopping[1].stopping = true;
        stopping[2].stopping = true;
        assert_eq!(idle(&stopping, "mike", &panes), 1);
        assert_eq!(claims(&sessions).len(), 2);
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
        assert_eq!(free_items(&issues, "Wave 13", &HashSet::new()), [265]);
        let pulls: Vec<Pull> = serde_json::from_str(
            r#"[{"number":267,"headRefName":"worktree-issue-265","isDraft":false,
                 "statusCheckRollup":[{"__typename":"CheckRun","name":"Gate","conclusion":"SUCCESS"},
                                      {"__typename":"StatusContext","context":"riff/verify","state":"SUCCESS"}]},
                {"number":268,"headRefName":"worktree-issue-262","isDraft":false,"statusCheckRollup":[]}]"#,
        )
        .unwrap();
        assert_eq!(waiting_verifies(&pulls, &HashSet::new()), [268]);
    }
}
