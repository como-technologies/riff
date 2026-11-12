//! Which host runs the lead of a project (#1791, the design #1781):
//! riff-server keeps one lead for each project, as the claim `lead`
//! ([`LEAD_CLAIM`](riff_core::wire::LEAD_CLAIM)) that the riff daemon of one
//! host holds.
//!
//! # Design
//!
//! The daemon of each host that joined a project runs a lead loop
//! ([`run`]) next to the loop of its wave ([`crate::wave`]). Each look
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
//! ([`crate::wave::World::plan_changes`]) and each [`LOOK_EVERY`]. The end
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
//! paused until a host claims the lead.
//!
//! The end of the container of a lead, and a lead with no container at
//! the start of the daemon, are the business of the daemon
//! (`Daemon::lead_back`): the lead starts again with `claude --resume`
//! while its session holds the claim, or nobody holds it.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use riff_client::api::Api;
use riff_core::name::{Place, SessionUri};
use riff_core::wire::PlanReply;
use tokio::sync::Notify;

use crate::daemon::Daemon;
use crate::runtime::Runtime;
use crate::wave::{BoxFuture, Wave};
use crate::{identity, text};

/// How often the lead loop looks when no change of the plan comes.
pub const LOOK_EVERY: Duration = Duration::from_secs(60);

/// The wait of a host that is not the host of the last lead, on top of
/// the wait of its load: the host of the last lead claims first on a tie.
pub const TIE: Duration = Duration::from_secs(1);

/// What the lead loop of a host can read and do. The daemon gives it
/// with [`Hosted`]; a test gives a fake.
pub trait Ground: Send + Sync {
    /// The plan of the repository thread of `wave` on riff-server now: the
    /// session that holds the claim lead, and the host of the last lead.
    fn plan<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<PlanReply>>;

    /// The session ID of the lead of the repository of `wave` on this
    /// host, while its container runs.
    fn mine(&self, wave: &Wave) -> Option<String>;

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
    /// claim lead. Its session ends.
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
/// of this host claims at once. The start of a new lead waits first
/// ([`wait`]), then reads the plan again, and starts the lead only when
/// the claim lead is still free.
pub async fn look(ground: &dyn Ground, wave: &Wave) -> Result<Did> {
    let plan = ground.plan(wave).await?;
    let mine = ground.mine(wave);
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
            let (live, limit) = ground.load();
            let last = plan.last_lead.as_deref() == Some(wave.host.as_str());
            tokio::time::sleep(wait(live, limit, last)).await;
            if ground.plan(wave).await?.lead.is_some() || ground.mine(wave).is_some() {
                return Ok(Did::Taken);
            }
            let granted = ground.start(wave).await?;
            Ok(if granted { Did::Started } else { Did::Taken })
        }
    }
}

/// The lead loop of the host of `wave`: a look at each change of the plan
/// that `changed` tells, and each [`LOOK_EVERY`]. It prints what a look
/// did, and a fault one time until it changes. It never ends.
pub async fn run(ground: Arc<dyn Ground>, wave: Wave, changed: Arc<Notify>) {
    let mut fault = None;
    loop {
        match look(&*ground, &wave).await {
            Ok(did) => {
                fault = None;
                if let Some(line) = text::lead_host_did(&wave.repo, &did) {
                    println!("riff daemon: {line}");
                }
            }
            Err(e) => {
                let e = format!("{e:#}");
                if fault.as_ref() != Some(&e) {
                    eprintln!("riff daemon: {}: the lead: {e}", wave.repo);
                }
                fault = Some(e);
            }
        }
        tokio::select! {
            () = tokio::time::sleep(LOOK_EVERY) => {}
            () = changed.notified() => {}
        }
    }
}

/// The lead loop of the daemon for `wave` ([`run`] with [`Hosted`]), with
/// the changes of the plan of its thread on riff-server. It never ends.
pub(crate) async fn follow<R: Runtime + 'static>(daemon: Arc<Daemon<R>>, wave: Wave) {
    let changed = Arc::new(Notify::new());
    let world = daemon.world();
    let plans = world.plan_changes(&wave, &changed);
    let ground: Arc<dyn Ground> = Arc::new(Hosted(daemon));
    tokio::join!(plans, run(ground, wave.clone(), changed.clone()));
}

/// The [`Ground`] of the riff daemon: the plan and the claim on
/// riff-server as the person of the wave, the leads and the load of the
/// daemon, and the start of a lead as `riff lead start` does it
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

        fn load(&self) -> (usize, u32) {
            (self.live, 4)
        }

        fn claim<'a>(&'a self, _: &'a Wave, session: &'a str) -> BoxFuture<'a, Result<bool>> {
            let now = self.server.now();
            let granted = self.server.state().claim_lead(&self.uri(session), now);
            Box::pin(async move { granted.map_err(anyhow::Error::msg) })
        }

        fn start<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<bool>> {
            let n = self.starts.fetch_add(1, Ordering::SeqCst);
            let session = format!("{}{n}", self.name);
            let me = self.uri(&session);
            let now = self.server.now();
            let mut state = self.server.state();
            state.register(&me, now);
            let granted = state.claim_lead(&me, now);
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
}
