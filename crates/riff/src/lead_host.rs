//! Which host runs the lead of a project (#1791, the design #1781):
//! riff-server keeps one lead for each project, as the claim `lead`
//! ([`LEAD_CLAIM`](riff_core::wire::LEAD_CLAIM)) that the riff daemon of one
//! host holds.
//!
//! # Design
//!
//! The daemon of each host that joined a project runs a lead loop
//! ([`run`], 01M77PT9TNP97MA89SYZPBJ6QY) next to the loop of its wave
//! ([`crate::wave`]). A host that joined with `riff host join --no-lead`
//! runs no lead loop, so it claims no lead (01M77Y6ZHM0V6YQ3XJEVJ8J5PC,
//! `Daemon::claims_lead`). Each look
//! ([`look`]) reads the plan of the repository thread on riff-server
//! ([`riff_core::wire::PlanReply`]): the session that holds the claim
//! lead, and the host of the last lead.
//!
//! | The claim lead | The lead of this host | The step ([`step`]) |
//! |---|---|---|
//! | held by the lead of this host | runs | none |
//! | held by another session | runs | stop it ([`Ground::stop`]) |
//! | held by another session | none | none |
//! | free | runs | claim it again for that lead ([`Ground::claim`]) |
//! | free | none | start a new lead with the claim ([`Ground::start`]) |
//!
//! A host whose lead runs claims at once: for example a lead from before
//! the claim, or a lead whose host was away for 5 minutes while no other
//! host took the claim. Before the start of a new lead, the host waits as
//! for a free item of its wave ([`crate::wave::claim_wait`]): 10 s × its
//! live sessions / its limit, so the host with the most free room claims
//! first. A host that is not the host of the last lead waits [`TIE`]
//! more, so the host of the last lead claims first on a tie ([`wait`]).
//! After the wait, it claims only when the claim is still free.
//! riff-server gives the claim to one session only: the daemon asks with
//! `if_free` ([`riff_core::wire::Lead`]), and a second claim gets the
//! refusal `held` ([`Did::Taken`]).
//!
//! The loop looks at each change of the plan of the thread on riff-server
//! ([`crate::wave::World::plan_changes`]) and each [`LOOK_EVERY`]. Its
//! follower of the plan starts again after a failed start, as the
//! followers of the wave do ([`crate::followers`]). The end
//! of the claim lead is a change of the plan: the end of the session of
//! the lead, and 5 minutes after its last sign of life, when riff-server
//! wakes the daemons of the project
//! (`riff_server::engine::Engine::lead_claims_ended`).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as daemon of pangolin: 0 of 4
//!     participant S as riff-server
//!     participant B as daemon of thelio: 2 of 4
//!     A->>S: plan: the claim lead is free
//!     B->>S: plan: the claim lead is free
//!     A->>A: wait 0 s + 1 s
//!     B->>B: wait 5 s + 1 s
//!     A->>S: lead if_free: granted
//!     A->>A: start the lead in riff-lead-OWNER-REPO
//!     B->>S: plan: pangolin holds the claim lead
//!     Note over A: pangolin goes
//!     S->>B: 5 minutes later: a change of the plan
//!     B->>S: lead if_free: granted
//!     B->>B: start a new lead
//!     Note over A: pangolin comes back
//!     A->>S: plan: thelio holds the claim lead
//!     A->>A: stop the old lead
//! ```
//!
//! When no host can run a lead, no lead starts: the wave of the project
//! waits for a lead ([`crate::wave::stopped`]), and so the project is
//! paused until a host claims the lead (01M77PTJW94PSRCHFC999MK990).
//!
//! The end of the container of a lead, and a lead with no container at
//! the start of the daemon, are the business of the daemon
//! (`Daemon::lead_back`): the lead starts again with `claude --resume`
//! while its session holds the claim, or nobody holds it. While that
//! start waits for an open keyring (01M7BN2EPE81NEN3YZ78VSSG3W), the old
//! session is the lead of this host ([`Ground::mine`]): a look claims the
//! lead for it, also after riff-server ended its claim, and starts no new
//! lead.
//!
//! One start of the lead of a project runs at a time on a host
//! ([`crate::lead::one_start`], 01M77Y2BG12SEVF4ZEZ673QBFZ): the start of
//! the daemon, `riff lead move` and the start again of the daemon share a
//! lock in the folder of the lead.
//!
//! # A lead that fails to start
//!
//! A fault of a start of a lead on a host makes the next start there wait,
//! longer at each fault in a row ([`Faults`], #2032,
//! 01M77Y24FEQM5246G3CBXYWW62): 10 s, 30 s, 2 min, then 10 min
//! ([`LEAD_START_WAITS`]). A fault is a start of the lead loop that failed
//! ([`Did::Failed`]), and a lead of the host whose session ended because
//! it ended less than a minute after its start, or because 3 starts again
//! failed (`Daemon::lead_back`). The loop makes no start in the wait
//! ([`Did::Waits`]). So the end of a session that frees the claim makes no
//! new session at once, and another host of the project can claim the
//! lead. A lead of the host that runs for [`LEAD_SHORT`] ends the row
//! ([`Faults::ran`]). At the first fault of a row, the person gets one
//! line of riff-bot in riff chat ([`BotFact::LeadCannotStart`],
//! 01M77Y2BD66Z3VZD4A0B1Y60H1): the fault, and that the log of the daemon
//! has the cause. The row is told only when riff-server took the line
//! ([`Faults::told`], 01M7BN8QHX64NZ8PDQY2K62S6Z): a call that fails,
//! for example on a locked keyring after a restart of the host, goes
//! again after the next look.
//!
//! ```mermaid
//! flowchart LR
//!     F["a fault: the start failed,<br/>or the lead ended at once"] --> W["no start for 10 s,<br/>30 s, 2 min, then 10 min"]
//!     F -- "the first of a row" --> C["one line in riff chat"]
//!     C -- "riff-server could not take it:<br/>again after the next look" --> C
//!     W --> S["the next start"]
//!     S -- "a fault" --> F
//!     S -- "the lead runs for a minute" --> E["the row ends"]
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use anyhow::Result;
use riff_client::api::Api;
use riff_core::name::{Place, SessionUri};
use riff_core::wire::{BotFact, LEAD_START_WAITS, LeadFault, PlanReply};
use tokio::sync::Notify;
use tokio::time::Instant;

use crate::daemon::{Daemon, LEAD_SHORT};
use crate::followers::{self, Follower, Followers};
use crate::runtime::Runtime;
use crate::wave::{BoxFuture, Wave};
use crate::{identity, text};

/// How often the lead loop looks when no change of the plan comes.
pub const LOOK_EVERY: Duration = Duration::from_secs(60);

/// The wait of a host that is not the host of the last lead, on top of
/// the wait of its load: the host of the last lead claims first on a tie.
pub const TIE: Duration = Duration::from_secs(1);

/// The faults in a row of the starts of the lead of each project on a
/// host (#2032). After a fault, the host starts no lead of the project
/// before the wait of [`LEAD_START_WAITS`]: the first fault of a row
/// waits the first, and each fault after the last waits the last.
///
/// ```
/// use std::time::Duration;
/// use riff::lead_host::Faults;
/// use riff_core::wire::LeadFault;
/// use tokio::time::Instant;
///
/// let faults = Faults::default();
/// let now = Instant::now();
/// assert_eq!(faults.left("o/r", now), None);
/// assert_eq!(faults.fault("o/r", LeadFault::Start, now), Duration::from_secs(10));
/// assert_eq!(faults.left("o/r", now), Some(Duration::from_secs(10)));
/// assert_eq!(faults.left("o/r", now + Duration::from_secs(10)), None);
/// let next: Vec<u64> = (0..4).map(|_| faults.fault("o/r", LeadFault::Start, now).as_secs()).collect();
/// assert_eq!(next, [30, 120, 600, 600]);
/// // riff chat hears of a row one time, with its first fault: the row
/// // is untold until riff-server took the line.
/// assert_eq!(faults.untold("o/r"), Some(LeadFault::Start));
/// assert_eq!(faults.untold("o/r"), Some(LeadFault::Start));
/// faults.told("o/r");
/// assert_eq!(faults.untold("o/r"), None);
/// // A lead that runs for a minute ends the row.
/// faults.ran("o/r", Duration::from_secs(59));
/// assert!(faults.left("o/r", now).is_some());
/// faults.ran("o/r", Duration::from_secs(60));
/// assert_eq!(faults.left("o/r", now), None);
/// assert_eq!(faults.fault("o/r", LeadFault::EndedSoon, now), Duration::from_secs(10));
/// assert_eq!(faults.untold("o/r"), Some(LeadFault::EndedSoon));
/// ```
#[derive(Debug, Default)]
pub struct Faults(Mutex<HashMap<String, Row>>);

/// The faults in a row of one project ([`Faults`]).
#[derive(Debug)]
struct Row {
    /// The faults of the row.
    count: usize,
    /// The first fault of the row.
    first: LeadFault,
    /// No start of a lead before this time.
    until: Instant,
    /// True when riff chat heard of the row.
    told: bool,
}

impl Faults {
    fn rows(&self) -> std::sync::MutexGuard<'_, HashMap<String, Row>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A fault `fault` of a start of the lead of `repo` at `now`. It gives
    /// the wait before the next start of a lead of `repo` on this host.
    pub fn fault(&self, repo: &str, fault: LeadFault, now: Instant) -> Duration {
        let mut rows = self.rows();
        let row = rows.entry(repo.to_owned()).or_insert(Row {
            count: 0,
            first: fault,
            until: now,
            told: false,
        });
        let wait = LEAD_START_WAITS[row.count.min(LEAD_START_WAITS.len() - 1)];
        row.count += 1;
        row.until = now + wait;
        wait
    }

    /// The time from `now` until the next start of a lead of `repo` on this
    /// host may come, or `None` when it may come now.
    pub fn left(&self, repo: &str, now: Instant) -> Option<Duration> {
        let rows = self.rows();
        let left = rows.get(repo)?.until.saturating_duration_since(now);
        (!left.is_zero()).then_some(left)
    }

    /// The lead of `repo` on this host runs, and its container started
    /// `ran` ago: after [`LEAD_SHORT`], the row of faults ends.
    pub fn ran(&self, repo: &str, ran: Duration) {
        if ran >= LEAD_SHORT {
            self.rows().remove(repo);
        }
    }

    /// The first fault of the row of `repo`, until riff chat heard of the
    /// row ([`BotFact::LeadCannotStart`], [`Faults::told`]).
    pub fn untold(&self, repo: &str) -> Option<LeadFault> {
        let rows = self.rows();
        rows.get(repo).filter(|row| !row.told).map(|row| row.first)
    }

    /// riff-server took the line of the row of `repo` for riff chat: the
    /// row gets no other line.
    pub fn told(&self, repo: &str) {
        if let Some(row) = self.rows().get_mut(repo) {
            row.told = true;
        }
    }
}

/// What the lead loop of a host can read and do. The daemon gives it
/// with [`Hosted`]; a test gives a fake.
pub trait Ground: Send + Sync {
    /// The plan of the repository thread of `wave` on riff-server now: the
    /// session that holds the claim lead, and the host of the last lead.
    fn plan<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<PlanReply>>;

    /// The session ID of the lead of the repository of `wave` on this
    /// host, while its container runs, and while it starts again: also
    /// while that start waits for an open keyring.
    fn mine(&self, wave: &Wave) -> Option<String>;

    /// How long ago the container of the lead of the repository of `wave`
    /// on this host started, while it runs.
    fn ran(&self, wave: &Wave) -> Option<Duration>;

    /// The faults in a row of the starts of the leads of this host.
    fn faults(&self) -> &Faults;

    /// Tells riff-server the fact `fact` as the person of `wave`: a line of
    /// riff-bot in riff chat.
    fn bot<'a>(&'a self, wave: &'a Wave, fact: BotFact) -> BoxFuture<'a, Result<()>>;

    /// The live sessions of this host, and its limit.
    fn load(&self) -> (usize, u32);

    /// Gives the claim lead to the lead `session` of this host, as the
    /// person, only when it is free. False when another session holds it.
    fn claim<'a>(&'a self, wave: &'a Wave, session: &'a str) -> BoxFuture<'a, Result<bool>>;

    /// Starts a new lead of the repository of `wave` on this host, with the
    /// claim lead only when it is free. False when another session got the
    /// claim first: then no container starts.
    fn start<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<bool>>;

    /// Stops the lead `session` of this host: another session holds the
    /// claim lead. Its session ends (01M77PTJRNASXZ2SWD8HMSV681).
    fn stop<'a>(&'a self, wave: &'a Wave, session: &'a str) -> BoxFuture<'a, Result<()>>;
}

/// The step of a look, from the holder `lead` of the claim lead and the
/// session `mine` of the lead of this host.
///
/// ```
/// use riff::lead_host::{Step, step};
/// use riff_core::name::SessionUri;
///
/// let a: SessionUri = "riff://mike@pangolin/o/r?session=a1".parse()?;
/// assert_eq!(step(Some(&a), Some("a1")), Step::Keep);
/// assert_eq!(step(Some(&a), Some("b2")), Step::Stop("b2".into()));
/// assert_eq!(step(Some(&a), None), Step::Keep);
/// assert_eq!(step(None, Some("b2")), Step::Claim);
/// assert_eq!(step(None, None), Step::Claim);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn step(lead: Option<&SessionUri>, mine: Option<&str>) -> Step {
    match (lead, mine) {
        (Some(lead), Some(mine)) if lead.who().session() != Some(mine) => {
            Step::Stop(mine.to_owned())
        }
        (Some(_), _) => Step::Keep,
        (None, _) => Step::Claim,
    }
}

/// A step of the lead loop ([`step`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Nothing changes.
    Keep,
    /// Stop the lead with this session ID: another session holds the claim.
    Stop(String),
    /// The claim is free: claim it, after the wait.
    Claim,
}

/// What one look did ([`look`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Did {
    /// Nothing: the claim is held.
    Nothing,
    /// This host gave the claim lead again to its lead that runs.
    Claimed,
    /// This host started a new lead, with the claim lead.
    Started,
    /// Another host took the claim first.
    Taken,
    /// This host stopped its old lead: another session holds the claim.
    Stopped,
    /// The claim is free, and this host starts no lead for this time yet:
    /// a fault of a start came before ([`Faults`]).
    Waits(Duration),
    /// The start of a new lead failed for `why`: the next start of a lead
    /// on this host waits `wait` ([`Faults::fault`]).
    Failed { why: String, wait: Duration },
}

/// The wait of a host with `live` sessions and the limit `limit` before
/// the claim lead: the wait of its load
/// ([`crate::wave::claim_wait`] of [`crate::wave::CLAIM_WAIT`]), and
/// [`TIE`] more when it is not the host of the last lead (`last`).
///
/// ```
/// use std::time::Duration;
/// use riff::lead_host::wait;
///
/// assert_eq!(wait(0, 4, true), Duration::ZERO);
/// assert_eq!(wait(0, 4, false), Duration::from_secs(1));
/// assert_eq!(wait(2, 4, false), Duration::from_secs(6));
/// // A full host waits the most.
/// assert_eq!(wait(4, 4, true), Duration::from_secs(10));
/// ```
pub fn wait(live: usize, limit: u32, last: bool) -> Duration {
    let load = crate::wave::claim_wait(live, limit, crate::wave::CLAIM_WAIT);
    if last { load } else { load + TIE }
}

/// One look of the lead loop of the host of `wave` ([`step`]). The lead
/// of this host claims at once. After a fault, no new lead starts before
/// the end of its wait ([`Faults`]). The start of a new lead waits first
/// ([`wait`]), then reads the plan again, and starts the lead only when
/// the claim lead is still free. A start that fails is a fault.
pub async fn look(ground: &dyn Ground, wave: &Wave) -> Result<Did> {
    let plan = ground.plan(wave).await?;
    let mine = ground.mine(wave);
    let faults = ground.faults();
    if let Some(ran) = mine.as_ref().and_then(|_| ground.ran(wave)) {
        faults.ran(&wave.repo, ran);
    }
    let waits = || faults.left(&wave.repo, Instant::now()).map(Did::Waits);
    match step(plan.lead.as_ref(), mine.as_deref()) {
        Step::Keep => Ok(Did::Nothing),
        Step::Stop(session) => {
            ground.stop(wave, &session).await?;
            Ok(Did::Stopped)
        }
        // The host whose lead runs claims at once.
        Step::Claim if let Some(session) = mine => {
            let granted = ground.claim(wave, &session).await?;
            Ok(if granted { Did::Claimed } else { Did::Taken })
        }
        Step::Claim => {
            if let Some(waits) = waits() {
                return Ok(waits);
            }
            let (live, limit) = ground.load();
            let last = plan.last_lead.as_deref() == Some(wave.host.as_str());
            tokio::time::sleep(wait(live, limit, last)).await;
            if ground.plan(wave).await?.lead.is_some() || ground.mine(wave).is_some() {
                return Ok(Did::Taken);
            }
            // A fault in the wait: a lead of this host ended at once.
            if let Some(waits) = waits() {
                return Ok(waits);
            }
            match ground.start(wave).await {
                Ok(true) => Ok(Did::Started),
                Ok(false) => Ok(Did::Taken),
                Err(e) => Ok(Did::Failed {
                    why: format!("{e:#}"),
                    wait: faults.fault(&wave.repo, LeadFault::Start, Instant::now()),
                }),
            }
        }
    }
}

/// The lead loop of the host of `wave`: a look at each change of the plan
/// that `changed` tells, each [`LOOK_EVERY`], and at the end of the wait
/// after a fault ([`Faults::left`]). It prints what a look did, and a
/// fault of a look or of a call for riff chat one time until it changes.
/// At the first fault of a row of starts, it tells riff chat one time
/// ([`BotFact::LeadCannotStart`]): a call that fails goes again after the
/// next look, until riff-server takes it ([`Faults::told`]). It never
/// ends.
pub async fn run(ground: Arc<dyn Ground>, wave: Wave, changed: Arc<Notify>) {
    let (mut fault, mut bot_fault) = (None, None);
    loop {
        match look(&*ground, &wave).await {
            Ok(did) => {
                fault = None;
                if let Some(line) = text::lead_host_did(&wave.repo, &did) {
                    println!("riff daemon: {line}");
                }
            }
            Err(e) => say_once(&wave.repo, &mut fault, &e),
        }
        let faults = ground.faults();
        if let Some(fault) = faults.untold(&wave.repo) {
            let fact = BotFact::LeadCannotStart { fault };
            match ground.bot(&wave, fact).await {
                Ok(()) => {
                    bot_fault = None;
                    faults.told(&wave.repo);
                }
                Err(e) => say_once(&wave.repo, &mut bot_fault, &e),
            }
        }
        let next = faults
            .left(&wave.repo, Instant::now())
            .map_or(LOOK_EVERY, |left| left.min(LOOK_EVERY));
        tokio::select! {
            () = tokio::time::sleep(next) => {}
            () = changed.notified() => {}
        }
    }
}

/// Prints the fault `e` of the lead loop of `repo` when it is not the
/// fault `last` that came before, and keeps it in `last`.
fn say_once(repo: &str, last: &mut Option<String>, e: &anyhow::Error) {
    let e = format!("{e:#}");
    if last.as_ref() != Some(&e) {
        eprintln!("riff daemon: {repo}: the lead: {e}");
    }
    *last = Some(e);
}

/// The lead loop of the daemon for `wave` ([`run`] with [`Hosted`]), with
/// the changes of the plan of its thread on riff-server. A start of the
/// follower of the plan that fails, for example on a locked keyring, goes
/// in `followers`, the followers of the wave, and the follower starts
/// again ([`followers::follow`], 01M7B2RQPTFVK02YAZGMN2SYAR). It never
/// ends.
pub(crate) async fn follow<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    wave: Wave,
    followers: Followers,
) {
    let changed = Arc::new(Notify::new());
    let (world, every) = (daemon.world(), daemon.retry_every());
    let start = || world.plan_changes(&wave, &changed);
    let plans = followers::follow(&wave.repo, Follower::Lead, &followers, every, start);
    let ground: Arc<dyn Ground> = Arc::new(Hosted(daemon));
    tokio::join!(plans, run(ground, wave.clone(), changed.clone()));
}

/// The [`Ground`] of the riff daemon: the plan and the claim on
/// riff-server as the person of the wave, the leads and the load of the
/// daemon, and the start of a lead in its container
/// ([`crate::lead::claim_start`]).
pub struct Hosted<R>(pub Arc<Daemon<R>>);

impl<R: Runtime + 'static> Hosted<R> {
    /// The person of the wave, signed in, and its place.
    fn person(wave: &Wave) -> Result<(Api, SessionUri, Place)> {
        let place = wave.place(None)?;
        let me = identity::person(&place, &wave.server)?;
        Ok((Api::new(&wave.server).signed_in(None)?, me, place))
    }
}

impl<R: Runtime + 'static> Ground for Hosted<R> {
    fn plan<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<PlanReply>> {
        Box::pin(async move {
            let (api, me, _) = Self::person(wave)?;
            api.plan_show(&me, &wave.thread()?).await
        })
    }

    fn mine(&self, wave: &Wave) -> Option<String> {
        self.0.lead_session(&wave.repo)
    }

    fn ran(&self, wave: &Wave) -> Option<Duration> {
        self.0.lead_ran(&wave.repo)
    }

    fn faults(&self) -> &Faults {
        self.0.lead_faults()
    }

    fn bot<'a>(&'a self, wave: &'a Wave, fact: BotFact) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let (api, me, _) = Self::person(wave)?;
            api.bot(&me, fact).await
        })
    }

    fn load(&self) -> (usize, u32) {
        (self.0.live_count(), self.0.room().limit().sessions)
    }

    fn claim<'a>(&'a self, wave: &'a Wave, session: &'a str) -> BoxFuture<'a, Result<bool>> {
        Box::pin(async move {
            let (api, me, _) = Self::person(wave)?;
            Ok(api.claim_lead(&me, session).await?.is_ok())
        })
    }

    fn start<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<bool>> {
        Box::pin(crate::lead::claim_start(self.0.dir(), wave))
    }

    fn stop<'a>(&'a self, wave: &'a Wave, _session: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            for line in self.0.stop_lead(&wave.repo).await? {
                println!("riff daemon: {line}");
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    use riff_core::name::ThreadName;
    use riff_state::state::{CLAIM_GRACE, GONE, State};

    use super::*;

    fn thread() -> ThreadName {
        "acme/app".parse().unwrap()
    }

    /// A fake riff-server: the state of the real one, with a clock of its
    /// own.
    struct Server {
        state: Mutex<State>,
        now: Mutex<Instant>,
    }

    impl Server {
        /// A riff that runs: the person resumed it.
        fn new() -> Arc<Server> {
            let now = Instant::now();
            let mut state = State::default();
            let person: SessionUri = "riff://mike@pangolin/acme/app".parse().unwrap();
            state
                .riff(&person, Some(riff_core::wire::RiffState::Running), now)
                .unwrap();
            Arc::new(Server {
                state: Mutex::new(state),
                now: Mutex::new(now),
            })
        }

        fn now(&self) -> Instant {
            *self.now.lock().unwrap()
        }

        /// The time passes by `by`.
        fn pass(&self, by: Duration) {
            *self.now.lock().unwrap() += by;
        }

        fn state(&self) -> std::sync::MutexGuard<'_, State> {
            self.state.lock().unwrap()
        }

        /// The holder of the claim lead now.
        fn lead(&self) -> Option<SessionUri> {
            let now = self.now();
            self.state().plan(&thread(), now, 0).lead
        }

        /// The look of a wave of the project at the pauses now
        /// ([`crate::wave::stopped`]).
        fn wave_look(&self) -> Option<crate::wave::Look> {
            let person: SessionUri = "riff://mike@pangolin/acme/app".parse().unwrap();
            let now = self.now();
            let pauses = self.state().pauses_at(&person, now);
            crate::wave::stopped(&pauses, &thread())
        }
    }

    /// A fake host with its riff daemon. While the host is up, each look
    /// of its daemon is a sign of life of its lead, as the keep-alive of
    /// the daemon: a host that goes makes no look.
    struct Host {
        name: &'static str,
        server: Arc<Server>,
        live: usize,
        lead: Mutex<Option<String>>,
        starts: AtomicUsize,
        /// The time of each start of a new lead, on the clock of tokio.
        tries: Mutex<Vec<tokio::time::Instant>>,
        /// When true, each start of a new lead fails after the claim: the
        /// new session ends, and the plan changes ([`Host::changed`]).
        broken: bool,
        /// The changes of the plan of the thread, for [`run`].
        changed: Arc<Notify>,
        faults: Faults,
        /// Each fact that riff chat got.
        chats: Mutex<Vec<BotFact>>,
        /// The count of the next calls for riff chat that fail, for
        /// example on a locked keyring.
        bot_fails: AtomicUsize,
        /// The count of the calls for riff chat.
        bot_calls: AtomicUsize,
    }

    impl Host {
        /// The host `name` with `live` sessions of its limit of 4.
        fn new(name: &'static str, server: &Arc<Server>, live: usize) -> Host {
            Host {
                name,
                server: server.clone(),
                live,
                lead: Mutex::new(None),
                starts: AtomicUsize::new(0),
                tries: Mutex::new(Vec::new()),
                broken: false,
                changed: Arc::new(Notify::new()),
                faults: Faults::default(),
                chats: Mutex::new(Vec::new()),
                bot_fails: AtomicUsize::new(0),
                bot_calls: AtomicUsize::new(0),
            }
        }

        /// The host `name` whose start of a lead fails each time, for
        /// example with a broken image.
        fn broken(name: &'static str, server: &Arc<Server>) -> Host {
            Host {
                broken: true,
                ..Host::new(name, server, 0)
            }
        }

        fn wave(&self) -> Wave {
            Wave {
                repo: "acme/app".into(),
                main: "/src/app".into(),
                server: "http://127.0.0.1:9".into(),
                host: self.name.into(),
            }
        }

        fn uri(&self, session: &str) -> SessionUri {
            format!("riff://mike@{}/acme/app?session={session}", self.name)
                .parse()
                .unwrap()
        }

        fn mine(&self) -> Option<String> {
            self.lead.lock().unwrap().clone()
        }

        async fn look(&self) -> Did {
            look(self, &self.wave()).await.unwrap()
        }

        /// The lead of this host ends less than a minute after its start, as
        /// `Daemon::lead_back` does it: a fault first, then the end of its
        /// session. It gives the wait before the next start.
        async fn ends_at_once(&self) -> Duration {
            let session = self.mine().unwrap();
            let now = tokio::time::Instant::now();
            let wait = self.faults.fault("acme/app", LeadFault::EndedSoon, now);
            self.stop(&self.wave(), &session).await.unwrap();
            wait
        }

        /// The time between each two starts of a new lead.
        fn gaps(&self) -> Vec<Duration> {
            let tries = self.tries.lock().unwrap();
            tries.windows(2).map(|w| w[1] - w[0]).collect()
        }

        fn chats(&self) -> Vec<BotFact> {
            self.chats.lock().unwrap().clone()
        }
    }

    impl Ground for Host {
        fn plan<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<PlanReply>> {
            let now = self.server.now();
            let mut state = self.server.state();
            if let Some(session) = self.mine() {
                state.alive(&self.uri(&session), now);
            }
            let plan = state.plan(&thread(), now, 0);
            Box::pin(async move { Ok(plan) })
        }

        fn mine(&self, _: &Wave) -> Option<String> {
            self.mine()
        }

        /// The last start of a new lead on the clock of tokio.
        fn ran(&self, _: &Wave) -> Option<Duration> {
            let last = *self.tries.lock().unwrap().last()?;
            Some(last.elapsed())
        }

        fn faults(&self) -> &Faults {
            &self.faults
        }

        fn bot<'a>(&'a self, _: &'a Wave, fact: BotFact) -> BoxFuture<'a, Result<()>> {
            self.bot_calls.fetch_add(1, Ordering::SeqCst);
            let fails = self
                .bot_fails
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
            if fails.is_ok() {
                return Box::pin(async { anyhow::bail!("the keyring is locked") });
            }
            self.chats.lock().unwrap().push(fact);
            Box::pin(async { Ok(()) })
        }

        fn load(&self) -> (usize, u32) {
            (self.live, 4)
        }

        fn claim<'a>(&'a self, _: &'a Wave, session: &'a str) -> BoxFuture<'a, Result<bool>> {
            let now = self.server.now();
            let granted = self.server.state().claim_lead(&self.uri(session), now);
            Box::pin(async move { granted.map_err(anyhow::Error::msg) })
        }

        fn start<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<bool>> {
            self.tries.lock().unwrap().push(tokio::time::Instant::now());
            let n = self.starts.fetch_add(1, Ordering::SeqCst);
            let session = format!("{}{n}", self.name);
            let me = self.uri(&session);
            let now = self.server.now();
            let mut state = self.server.state();
            state.register(&me, now);
            let granted = state.claim_lead(&me, now);
            if self.broken {
                // The container does not start: the new session ends, and
                // the claim is free again at once.
                state.end(&me, now);
                self.changed.notify_one();
                return Box::pin(async { anyhow::bail!("the image is broken") });
            }
            if granted == Ok(true) {
                *self.lead.lock().unwrap() = Some(session);
            } else {
                state.end(&me, now);
            }
            Box::pin(async move { granted.map_err(anyhow::Error::msg) })
        }

        fn stop<'a>(&'a self, _: &'a Wave, session: &'a str) -> BoxFuture<'a, Result<()>> {
            let now = self.server.now();
            self.server.state().end(&self.uri(session), now);
            *self.lead.lock().unwrap() = None;
            Box::pin(async { Ok(()) })
        }
    }

    /// The test of #1791 with two daemons and a fake server. Both look at
    /// the free claim lead at once: pangolin, with the most free room,
    /// claims it and starts the lead. When pangolin goes, thelio claims
    /// the lead after the end of the claim, 5 minutes after the last sign
    /// of life, when riff-server wakes the daemons, and starts a new lead.
    /// pangolin comes back and stops its old lead. With no host, the
    /// project is paused: its wave waits for a lead.
    #[tokio::test(start_paused = true)]
    async fn a_host_claims_the_lead_and_the_next_host_takes_it_after_the_end_of_the_claim() {
        let server = Server::new();
        let pangolin = Host::new("pangolin", &server, 0);
        let thelio = Host::new("thelio", &server, 2);
        assert_eq!(server.lead(), None);

        let (a, b) = tokio::join!(pangolin.look(), thelio.look());
        assert_eq!((a, b), (Did::Started, Did::Taken));
        let first = server.lead().unwrap();
        assert_eq!(first.place().host(), "pangolin");
        assert_eq!(pangolin.mine().as_deref(), first.who().session());
        assert!(first.lead(), "riff-server makes the session the lead");
        assert_eq!(thelio.mine(), None);
        assert_eq!(server.wave_look(), None, "the wave runs");
        assert_eq!(thelio.look().await, Did::Nothing);

        // pangolin goes: its lead has no sign of life. Before the end of
        // the claim, thelio starts no lead.
        server.pass(CLAIM_GRACE - Duration::from_secs(1));
        assert_eq!(thelio.look().await, Did::Nothing);
        assert!(server.state().lead_claims_ended(server.now()).is_empty());
        // At the end of the claim, riff-server wakes the daemons, and
        // thelio claims the lead and starts a new one.
        server.pass(Duration::from_secs(1));
        let ended = server.state().lead_claims_ended(server.now());
        assert_eq!(ended, [(thread(), first.who().clone())]);
        assert_eq!(thelio.look().await, Did::Started);
        let second = server.lead().unwrap();
        assert_eq!(second.place().host(), "thelio");
        assert_ne!(second.who(), first.who());
        assert_eq!(server.wave_look(), None, "the wave runs");

        // pangolin comes back: its daemon stops the old lead, and the new
        // lead keeps the claim.
        assert_eq!(pangolin.look().await, Did::Stopped);
        assert_eq!(pangolin.mine(), None);
        assert_eq!(
            server.lead().map(|l| l.who().clone()),
            Some(second.who().clone())
        );
        assert_eq!(pangolin.look().await, Did::Nothing);

        // thelio goes too: with no host, no lead starts, and the project
        // is paused.
        server.pass(GONE);
        let paused = server.wave_look().expect("the wave waits");
        assert!(paused.no_lead);
    }

    /// On a tie, the host of the last lead claims first: both hosts are
    /// idle, and thelio ran the last lead.
    #[tokio::test(start_paused = true)]
    async fn on_a_tie_the_host_of_the_last_lead_claims_first() {
        let server = Server::new();
        let pangolin = Host::new("pangolin", &server, 0);
        let thelio = Host::new("thelio", &server, 0);
        assert_eq!(thelio.look().await, Did::Started);
        let last = thelio.mine().unwrap();
        // The lead ended less than a minute after its start: its session
        // ends, and the claim is free.
        thelio.stop(&thelio.wave(), &last).await.unwrap();
        assert_eq!(server.lead(), None);

        let (a, b) = tokio::join!(pangolin.look(), thelio.look());
        assert_eq!((a, b), (Did::Taken, Did::Started));
        assert_eq!(server.lead().unwrap().place().host(), "thelio");
    }

    /// A lead that runs with no claim, for example from before the claim,
    /// takes the claim at once. A second host starts no lead then.
    #[tokio::test(start_paused = true)]
    async fn a_lead_that_runs_with_no_claim_takes_it_at_once() {
        let server = Server::new();
        let pangolin = Host::new("pangolin", &server, 4);
        let thelio = Host::new("thelio", &server, 0);
        let old = pangolin.uri("old");
        server.state().register(&old, server.now());
        *pangolin.lead.lock().unwrap() = Some("old".into());

        let (a, b) = tokio::join!(pangolin.look(), thelio.look());
        assert_eq!((a, b), (Did::Claimed, Did::Nothing));
        assert_eq!(server.lead().unwrap().who(), old.who());
        assert_eq!(thelio.mine(), None);
    }

    /// The test of #2032 with a fake clock: each start of the lead fails,
    /// for example with a broken image, and the end of each new session
    /// frees the claim and changes the plan at once. The lead loop waits
    /// longer before each next start: 10 s, 30 s, 2 min, then 10 min. riff
    /// chat gets one line, with the fault.
    #[tokio::test(start_paused = true)]
    async fn a_start_that_fails_waits_longer_each_time_and_riff_chat_gets_one_line() {
        let server = Server::new();
        let host = Arc::new(Host::broken("pangolin", &server));
        let ground: Arc<dyn Ground> = host.clone();
        let task = tokio::spawn(run(ground, host.wave(), host.changed.clone()));
        tokio::time::sleep(Duration::from_secs(25 * 60)).await;
        task.abort();

        let secs: Vec<u64> = host.gaps().iter().map(Duration::as_secs).collect();
        assert_eq!(secs, [10, 30, 120, 600, 600]);
        let fault = LeadFault::Start;
        assert_eq!(host.chats(), [BotFact::LeadCannotStart { fault }]);
        assert_eq!(server.lead(), None);
    }

    /// #2100: the first call for the line of riff chat fails, for example
    /// on a locked keyring after a restart of the host. The row stays
    /// untold, the call goes again after the next look, and riff chat gets
    /// the line one time.
    #[tokio::test(start_paused = true)]
    async fn a_call_for_riff_chat_that_fails_goes_again_and_riff_chat_gets_one_line() {
        let server = Server::new();
        let host = Arc::new(Host::broken("pangolin", &server));
        host.bot_fails.store(1, Ordering::SeqCst);
        let ground: Arc<dyn Ground> = host.clone();
        let task = tokio::spawn(run(ground, host.wave(), host.changed.clone()));
        tokio::time::sleep(Duration::from_secs(25 * 60)).await;
        task.abort();

        let fault = LeadFault::Start;
        assert_eq!(host.chats(), [BotFact::LeadCannotStart { fault }]);
        assert_eq!(host.bot_calls.load(Ordering::SeqCst), 2);
        assert_eq!(host.faults.untold("acme/app"), None);
        let secs: Vec<u64> = host.gaps().iter().map(Duration::as_secs).collect();
        assert_eq!(secs, [10, 30, 120, 600, 600]);
    }

    /// #2032: a lead that ends less than a minute after its start is a
    /// fault too. The host starts no new lead before the end of the wait,
    /// and the wait grows. A lead that then runs for a minute ends the
    /// row: the next fault waits 10 s again.
    #[tokio::test(start_paused = true)]
    async fn a_lead_that_ends_at_once_waits_longer_each_time_until_a_lead_runs_a_minute() {
        let server = Server::new();
        let host = Host::new("pangolin", &server, 0);
        assert_eq!(host.look().await, Did::Started);
        for want in [10, 30, 120, 600, 600] {
            let wait = host.ends_at_once().await;
            assert_eq!(wait, Duration::from_secs(want));
            assert_eq!(host.look().await, Did::Waits(wait));
            tokio::time::sleep(wait - Duration::from_secs(1)).await;
            assert_eq!(host.look().await, Did::Waits(Duration::from_secs(1)));
            tokio::time::sleep(Duration::from_secs(1)).await;
            assert_eq!(host.look().await, Did::Started);
        }
        assert_eq!(host.faults.untold("acme/app"), Some(LeadFault::EndedSoon));

        tokio::time::sleep(LEAD_SHORT).await;
        assert_eq!(host.look().await, Did::Nothing);
        assert_eq!(host.ends_at_once().await, Duration::from_secs(10));
        assert_eq!(host.faults.untold("acme/app"), Some(LeadFault::EndedSoon));
    }

    /// #2032: while a host waits after a fault, another host of the
    /// project claims the lead and starts it.
    #[tokio::test(start_paused = true)]
    async fn while_a_host_waits_after_a_fault_another_host_starts_the_lead() {
        let server = Server::new();
        let pangolin = Host::new("pangolin", &server, 0);
        let thelio = Host::new("thelio", &server, 2);
        assert_eq!(pangolin.look().await, Did::Started);
        pangolin.ends_at_once().await;

        let (a, b) = tokio::join!(pangolin.look(), thelio.look());
        assert_eq!(a, Did::Waits(Duration::from_secs(10)));
        assert_eq!(b, Did::Started);
        assert_eq!(server.lead().unwrap().place().host(), "thelio");
    }
}
