//! riff runs the free work by itself: the rollout of the lead.
//!
//! # Design
//!
//! `riff mcp` of the lead runs the rollout (01M3Q5QE01DB0FJQJWFKR450KQ).
//! So the start of work does not wait for an agent that remembers a
//! step. Once each interval ([`crate::settings::workers_interval`],
//! 10 seconds by default, 01M3Q5QE9H42FQKEDC5G9GKCWD), it looks at the
//! riff, gives free work to the idle workers in tmux, and asks the riff
//! daemon of its machine to run the wave of the repository for the rest
//! ([`crate::wave`], 01M53NGVE839TRH7XB7CDPQNZE). The daemon runs each
//! item in a container. The rollout starts no worker in tmux: a person
//! starts one with `riff workers start` (01M53NGVQDKCTNMG54YDFE22T2).
//!
//! ```mermaid
//! flowchart TD
//!     T["each interval"] --> L{"this session is the lead?"}
//!     L -- no --> T
//!     L -- yes --> R{"the riff and the repository run?"}
//!     R -- "no: paused" --> T
//!     R -- yes --> D["the daemon of this machine:<br/>does a wave of the repository run?"]
//!     D --> P{"an idle worker that joined,<br/>or no wave runs?"}
//!     P -- no --> T
//!     P -- yes --> W["free work (gh): a fix item of a broken main first,<br/>pull requests that wait for a verify,<br/>free items of the current wave, no held item"]
//!     W --> O["each idle worker with no open request:<br/>tell it request: claim ITEM"]
//!     O --> I{"free work, no idle worker,<br/>and no wave runs?<br/>(a worker that refused does not count)"}
//!     I -- no --> T
//!     I -- yes --> S["ask the daemon: run the wave of the repository"]
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
//!   its own hold. The rollout sends no request for it, and runs no wave
//!   for it. Each look reads the holds of the repository thread
//!   again ([`Api::plan_show`]), so a free of the item makes it free work
//!   at the next look.
//! - **A fix item first** ([`fix_items`], 01M51QEYS538XA58KDRY844T52). An
//!   open issue with the label [`BROKEN_MAIN`] fixes a broken `main`: the
//!   look of the lead files it when the Gate of `main` fails
//!   ([`crate::look::MainWatch`]). It comes before each other item, its
//!   verify first, also outside the current wave. So the first idle
//!   worker gets it. The wave of the daemon takes the same order
//!   ([`crate::wave`]).
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
//! - **Idle workers** ([`idle`]). A worker pane of the user with no
//!   claim, also one that did not join yet, and a live worker of
//!   another user with no claim. A worker on a request of the lead with
//!   no work item holds a task claim `task-NAME`, so it is busy
//!   (01M518P1N8R893MG9Z3VAQZR6Z). A worker that the server asked to stop
//!   is not idle. A worker in another repository is not idle: it cannot
//!   take the free work (01M3W27BJYFQCHY5MTZ2J4SKW4). The free work goes
//!   to the idle workers first. riff asks for the wave only when free
//!   work is left and no worker is idle ([`asks_wave`],
//!   01M5515CA4CZ6KV4ZD6Y0744Z7).
//! - **Requests** ([`Offers`], 01M49ZK19GQP79Z8HH14PK85QQ to
//!   01M49ZK1EG52YAKG974XH201RK). An idle worker that joined sleeps on
//!   its watch: only a wake makes it read. So the rollout gives it free
//!   work with a request of the lead, as the lead does by hand. A
//!   worker that does not claim in [`offer_wait`] refused the item: it
//!   does not count as idle, when it has no other item to take. So the
//!   item goes to the wave.
//! - **No verify for the author** ([`Authors`], [`authored`],
//!   01M51533BJSD0RB7BFQR9CAFYW). A session that held `issue-N` never
//!   gets a request for `verify-issue-N`. riff knows the author from two
//!   facts. The rollout keeps each session that it saw with the claim
//!   ([`holders`]): a `/clear` keeps the session ID, so the author stays
//!   known after its clear. The release of a claim puts a usage comment
//!   on the issue with the first characters of the session ID
//!   ([`crate::usage`]): it stays after the end of the session and after
//!   a new start of `riff mcp` ([`authors`]). When the author is the only
//!   idle worker, it has no item to take, and the rollout runs the wave
//!   for the verify. A verify of the wave is a new session
//!   (01M4ZV3B6NKPEZ5FNE27X95GF0).
//! - **The wave of the daemon** ([`View::wave`], [`Env::run_wave`]). The
//!   daemon of the machine of the lead tells the waves that it runs
//!   ([`crate::wave::runs`]). A daemon that does not answer runs no
//!   wave. While a wave of the repository runs there, the rollout asks
//!   nothing, and with no idle worker that joined, it makes no call of
//!   `gh`. The claims on riff-server keep the wave and the workers in
//!   tmux from one item (01M53NGWTNC3655ZZJ76M1R5R7). The limit of the
//!   containers is the limit of the daemon (01M4ZV3B6HAFTSC2K63FMC5FYA).
//!   A second machine runs its own wave with `riff wave run`
//!   (01M53NGVH8A19QS3CQ79ENVVXT).
//! - **Notes** (01M5515CA4CZ6KV4ZD6Y0744Z7). After the ask, the lead
//!   gets a note ([`text::rollout_wave`]). When the ask fails, the lead
//!   gets one note with the reason ([`text::rollout_no_wave`]), and the
//!   rollout asks again at each look. The next note comes for a new
//!   reason, or after an ask that worked.
//! - **A stop of the wave** (01M5515CFHRA6AJG4GKX0ZV77W). `riff wave
//!   stop` stops the wave of the daemon. The rollout asks for it again
//!   at its next look with free work. To keep it stopped, turn the
//!   rollout off (`riff workers interval 0`), or pause the repository.
//! - **Pause** (01M3Q5QEBTNM90SPYXNVTT7RJA). A pause stops the rollout
//!   within one look. A look that started before the pause can ask for
//!   the wave one more time. The wave then starts nothing while the
//!   repository is paused.
//! - **Machines** ([`Place`], [`pick`]). The machine of the lead, when
//!   the lead runs in tmux and riff is on in its main clone
//!   (01M3XY2T542DCHBN95H9PX4AGQ), and each live workers host of the
//!   user. The rollout starts no worker there. It uses them for the
//!   message of a higher limit while the rollout is off ([`effect`]):
//!   then a person starts the workers.
//!
//! The rollout never stops a worker. The server stops idle workers
//! (#259). A worker over the limit of its machine ends after its item
//! ([`crate::next`], 01M402VFGAJQM1QW8B42NKMJM4).
//!
//! # A change of the worker settings
//!
//! The lead conducts the workers, so it must know what each machine can
//! run. At each look, also while the rollout is off, `riff mcp` of the
//! lead reads the worker settings ([`Seen`]): the limit of its machine
//! and of each live workers host, the interval and the MCP servers of
//! its machine, and the idle settings of the server. When a value is
//! not the value of the look before ([`Seen::changes`]), the lead gets
//! one message for the change: the setting, the old value, the new
//! value and the host (01M3X30KHKB6W11C3NBAW7KCGW). The first look gives no message.
//!
//! ```mermaid
//! flowchart TD
//!     T["each look"] --> S["read the settings"]
//!     S --> C{"a value changed?"}
//!     C -- no --> T
//!     C -- yes --> E{"what does the change do?"}
//!     E -- "a higher limit gives room, and the rollout is off" --> W["message that wakes the lead:<br/>riff workers start N"]
//!     E -- "more workers run than the new limit" --> N2["note: the workers over the limit end after their item"]
//!     E -- "nothing" --> N3["note: the change"]
//! ```
//!
//! [`effect`] finds what the change does (01M3X30R4PSBP3RQWM02BJ6GK3). The message is a note
//! when the lead has nothing to do. It wakes the lead only when work
//! waits that the change lets start and the rollout is off (01M3X30RA3X08JBJ2JBVCCNEH3).
//! While the rollout is on, a higher limit starts nothing: the rollout
//! starts no worker in tmux, and the wave of the daemon takes the free
//! work (01M5515CCV7WNKVZVAMQ375J0M).
//! One look uses each limit one time: the view of the look takes the
//! limits that the look read for the changes ([`View::with_limits`]).
//! So the message of a change says what the limit of that look does
//! (01M3XFHSYJEN9V6QEKWJGJWQ8Q). A
//! workers host sets its status at once when its limit changes, and
//! posts the note for a change of its own MCP servers
//! ([`crate::host`], 01M3X30RJS8YE5TXJBQDC2FT0C).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use riff_core::name::SessionUri;
use riff_core::selector::Selector;
use riff_core::wire::{Idle, Kind, RiffState, SessionInfo};
use serde::Deserialize;

use crate::api::Api;
use crate::disk::Disk;
use crate::host;
use crate::machine::Machine;
use crate::pr::Gh;
use crate::terminal::{Terminal, Tmux, WorkerPane};
use crate::{identity, settings, text, usage, worker};

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
    /// The available memory in GB under which the machine starts no
    /// worker.
    pub floor: u32,
    /// The deaths of its workers in the last hour ([`crate::deaths`]).
    pub deaths: usize,
    /// The numbers of the machine, if it tells them.
    pub machine: Option<Machine>,
    /// The disk of the machine, if it tells it.
    pub disk: Option<Disk>,
}

impl Place {
    /// True when the machine can take one more worker: fewer workers
    /// than its limit, not busy, not low on memory, not low on disk
    /// (01M41A11DX1QRP48YPTDNT67W4), and no loop of deaths
    /// (01M493YZZEW1FTDBNA090WT2AG).
    ///
    /// ```
    /// use riff::disk::Disk;
    /// use riff::rollout::Place;
    ///
    /// let place = Place { host: "pangolin".into(), session: None, limit: 2, workers: 0, floor: 4, deaths: 0, machine: None, disk: None };
    /// assert!(place.room());
    /// assert!(Place { disk: Some(Disk { free_gb: 50, total_gb: 455 }), ..place.clone() }.room());
    /// assert!(!Place { disk: Some(Disk { free_gb: 16, total_gb: 455 }), ..place.clone() }.room());
    /// // 3 deaths in the last hour leave room; 4 stop the starts.
    /// assert!(Place { deaths: 3, ..place.clone() }.room());
    /// assert!(!Place { deaths: 4, ..place }.room());
    /// ```
    pub fn room(&self) -> bool {
        self.workers < usize::from(self.limit)
            && !crate::deaths::halted(self.deaths)
            && !self.machine.is_some_and(|m| m.busy() || m.low(self.floor))
            && !self.disk.is_some_and(|d| d.low())
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
    /// The free work by name, the verifies first: `verify-issue-N` and
    /// `issue-N`.
    pub items: Vec<String>,
    /// The idle workers that can get a request ([`idlers`]).
    pub idlers: Vec<Idler>,
    /// The sessions that held each item: of the claims of this look
    /// ([`holders`]) and of the usage comments ([`authors`]).
    pub authors: Authors,
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

/// The sessions that held the work claim `issue-N`, by the issue N
/// (01M51533BJSD0RB7BFQR9CAFYW). A session is its ID, or the first
/// characters of its ID as a usage comment names it
/// ([`crate::usage::Report`]).
pub type Authors = BTreeMap<u64, BTreeSet<String>>;

/// Adds each session of `from` to `into`.
fn add(into: &mut Authors, from: &Authors) {
    for (issue, sessions) in from {
        into.entry(*issue)
            .or_default()
            .extend(sessions.iter().cloned());
    }
}

/// True when `idler` can be the author of the verify `item`
/// (01M51533BJSD0RB7BFQR9CAFYW): it works in the worktree `issue-N`, or
/// `authors` names it for the issue N. A name of `authors` is the
/// session ID or its start. An item that is no verify has no author.
///
/// ```
/// use riff::rollout::{Authors, Idler, authored};
///
/// let idler = |session: &str, worktree: Option<&str>| Idler {
///     session: session.into(),
///     worktree: worktree.map(str::to_owned),
/// };
/// let authors: Authors = [(12, ["a6cf0123".to_owned()].into())].into();
/// // A usage comment names the first characters of the session ID.
/// assert!(authored(&idler("a6cf0123-4567", None), "verify-issue-12", &authors));
/// assert!(authored(&idler("w1", Some("issue-12")), "verify-issue-12", &Authors::new()));
/// assert!(!authored(&idler("b7d01234-4567", None), "verify-issue-12", &authors));
/// assert!(!authored(&idler("a6cf0123-4567", None), "verify-issue-13", &authors));
/// assert!(!authored(&idler("a6cf0123-4567", None), "issue-12", &authors));
/// ```
pub fn authored(idler: &Idler, item: &str, authors: &Authors) -> bool {
    let Some(own) = item.strip_prefix("verify-") else {
        return false;
    };
    if idler.worktree.as_deref() == Some(own) {
        return true;
    }
    let Some((issue, usage::Kind::Work)) = usage::issue_of(own) else {
        return false;
    };
    authors.get(&issue).is_some_and(|sessions| {
        sessions
            .iter()
            .any(|s| !s.is_empty() && idler.session.starts_with(s.as_str()))
    })
}

impl View {
    /// The view with the limits of `seen`: each machine gets the limit
    /// that the look read for the changes (01M3XFHSYJEN9V6QEKWJGJWQ8Q).
    /// A machine that `seen` does not have keeps its limit.
    ///
    /// ```
    /// use riff::rollout::{Place, Seen, View};
    ///
    /// let place = |host: &str| Place {
    ///     host: host.into(),
    ///     session: None,
    ///     limit: 2,
    ///     workers: 1,
    ///     floor: 4,
    ///     deaths: 0,
    ///     machine: None,
    ///     disk: None,
    /// };
    /// let view = View { running: true, work: 1, idle: 0, places: vec![place("a"), place("b")], ..View::default() };
    /// // A person set the limit of `a` from 1 to 2 after the look read it.
    /// let seen = Seen { limits: [("a".to_owned(), 1)].into(), ..Seen::default() };
    /// let view = view.with_limits(&seen);
    /// assert_eq!(view.places[0].limit, 1);
    /// assert_eq!(view.places[1].limit, 2);
    /// ```
    pub fn with_limits(mut self, seen: &Seen) -> View {
        for place in &mut self.places {
            if let Some(&limit) = seen.limits.get(&place.host) {
                place.limit = limit;
            }
        }
        self
    }
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
///     floor: 4,
///     deaths: 0,
///     machine: Some(Machine { cores, mhz: 3000, now_mhz: 3000, mem_gb: 64, avail_gb: 64, load: 0.0 }),
///     disk: None,
/// };
/// let places = [place("thelio", 32, 0), place("pangolin", 8, 0)];
/// assert_eq!(pick(&places), Some(0));
/// // thelio is at its limit.
/// let places = [place("thelio", 32, 4), place("pangolin", 8, 0)];
/// assert_eq!(pick(&places), Some(1));
/// // pangolin has less available memory than its floor.
/// let mut low = place("pangolin", 8, 0);
/// low.machine = low.machine.map(|m| Machine { avail_gb: 3, ..m });
/// assert_eq!(pick(&[place("thelio", 32, 4), low]), None);
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

/// The machine with room for one more worker, or `None`: the riff is
/// paused, there is no free work, a worker is idle
/// (01M3Q5QEJNP1JGQM7VXXEBJ9J9), or no machine has room. The rollout
/// starts no worker in tmux (01M53NGVE839TRH7XB7CDPQNZE): it uses this
/// for the message of a higher limit while it is off ([`effect`]).
///
/// ```
/// use riff::rollout::{Place, View, decide};
///
/// let here = Place { host: "thelio".into(), session: None, limit: 2, workers: 0, floor: 4, deaths: 0, machine: None, disk: None };
/// let view = View { running: true, work: 2, idle: 0, places: vec![here], ..View::default() };
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

/// True when the rollout asks the riff daemon to run the wave of the
/// repository (01M53NGVE839TRH7XB7CDPQNZE, 01M5515CA4CZ6KV4ZD6Y0744Z7):
/// the riff runs, free work is left that no idle worker takes, and no
/// wave of the repository runs on the daemon. The places do not count:
/// the limit of the containers is the limit of the daemon.
///
/// ```
/// use riff::rollout::{View, WaveState, asks_wave};
///
/// let view = View { running: true, work: 2, ..View::default() };
/// assert!(asks_wave(&view));
/// assert!(!asks_wave(&View { wave: WaveState::Runs, ..view.clone() }));
/// // No daemon answers: there is no daemon to ask.
/// let gone = WaveState::NoDaemon("no riff daemon".into());
/// assert!(!asks_wave(&View { wave: gone, ..view.clone() }));
/// assert!(!asks_wave(&View { running: false, ..view.clone() }));
/// assert!(!asks_wave(&View { work: 0, ..view.clone() }));
/// // An idle worker takes the free work first.
/// assert!(!asks_wave(&View { idle: 1, ..view }));
/// ```
pub fn asks_wave(view: &View) -> bool {
    view.running && view.work > 0 && view.idle == 0 && view.wave == WaveState::None
}

/// An idle worker of the user of the lead that joined the riff. The
/// rollout can give it free work with a request
/// (01M49ZK19GQP79Z8HH14PK85QQ).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Idler {
    /// The session ID.
    pub session: String,
    /// The worktree of the session, for example `issue-12`.
    pub worktree: Option<String>,
}

/// How long a worker has to claim the item of a request: 6 intervals of
/// the rollout (01M49ZK1C1P817KE63EXV3EJDC).
///
/// ```
/// use std::time::Duration;
/// use riff::rollout::offer_wait;
///
/// assert_eq!(offer_wait(Duration::from_secs(10)), Duration::from_secs(60));
/// ```
pub fn offer_wait(interval: Duration) -> Duration {
    interval * 6
}

/// The request that gives `item` to a worker.
///
/// ```
/// assert_eq!(riff::rollout::request("verify-issue-12"), "request: claim verify-issue-12");
/// ```
pub fn request(item: &str) -> String {
    format!("request: claim {item}")
}

/// The requests of the rollout to idle workers
/// (01M49ZK19GQP79Z8HH14PK85QQ to 01M49ZK1EG52YAKG974XH201RK).
///
/// At each look, [`Offers::plan`] gives each idle worker with no open
/// request one free item: a verify first. An item goes to one worker at
/// a time. A session that held `issue-N` gets no request for
/// `verify-issue-N` ([`authored`], 01M51533BJSD0RB7BFQR9CAFYW): the
/// offers keep each author of each view, so a `/clear` of the author
/// changes nothing. A worker in the worktree `issue-N` gets none either:
/// it can be the author. A request is open until its
/// worker claims or goes, or its item is no longer free. A worker that
/// does not claim in [`offer_wait`] refused the item. It never gets that
/// request again. It does not count as idle, when it has no other item
/// to take: so the item goes to the wave of the daemon ([`asks_wave`]).
///
/// ```
/// use std::time::{Duration, Instant};
/// use riff::rollout::{Idler, Offers, View};
///
/// let w1 = Idler { session: "w1".into(), worktree: None };
/// let view = View { running: true, work: 1, idle: 1, items: vec!["verify-issue-12".into()], idlers: vec![w1], ..View::default() };
/// let (mut offers, wait, t0) = (Offers::default(), Duration::from_secs(60), Instant::now());
/// let (requests, seen) = offers.plan(&view, t0, wait);
/// assert_eq!(requests, [("w1".to_owned(), "verify-issue-12".to_owned())]);
/// // The worker has the request: it is idle, and no worker starts.
/// assert_eq!(seen.idle, 1);
/// // No second request within the wait.
/// assert_eq!(offers.plan(&view, t0 + Duration::from_secs(30), wait).0, []);
/// // No claim in the wait: the worker refused the item. It does not
/// // count as idle, so the rollout can run the wave for the item.
/// let (requests, seen) = offers.plan(&view, t0 + wait, wait);
/// assert_eq!((requests, seen.idle, seen.work), (vec![], 0, 1));
///
/// // w1 held issue-7 at a look. After its release and its `/clear`, it
/// // is idle, and the verify of issue-7 waits: w1 gets no request for
/// // it, and it does not count as idle, so the wave takes the verify.
/// let held = View { authors: [(7, ["w1".to_owned()].into())].into(), ..View::default() };
/// let mut offers = Offers::default();
/// offers.plan(&held, t0, wait);
/// let verify = View { items: vec!["verify-issue-7".into()], ..view };
/// let (requests, seen) = offers.plan(&verify, t0, wait);
/// assert_eq!((requests, seen.idle), (vec![], 0));
/// ```
#[derive(Debug, Default)]
pub struct Offers {
    /// Each open request: the worker, the item and the time of the
    /// request.
    open: Vec<(String, String, Instant)>,
    /// The workers that refused each item.
    refused: BTreeMap<String, BTreeSet<String>>,
    /// Each author of each view (01M51533BJSD0RB7BFQR9CAFYW).
    authors: Authors,
}

impl Offers {
    /// The new requests of this look `view` at `now`, as pairs of
    /// worker and item, and the view for [`asks_wave`]: an idle worker
    /// with no request and no item to take is not idle.
    pub fn plan(
        &mut self,
        view: &View,
        now: Instant,
        wait: Duration,
    ) -> (Vec<(String, String)>, View) {
        add(&mut self.authors, &view.authors);
        let idle: HashSet<&str> = view.idlers.iter().map(|i| i.session.as_str()).collect();
        let free: HashSet<&str> = view.items.iter().map(String::as_str).collect();
        self.refused.retain(|item, _| free.contains(item.as_str()));
        let mut open = Vec::new();
        for (worker, item, at) in std::mem::take(&mut self.open) {
            if !idle.contains(worker.as_str()) || !free.contains(item.as_str()) {
                continue;
            }
            if now.saturating_duration_since(at) >= wait {
                self.refused.entry(item).or_default().insert(worker);
            } else {
                open.push((worker, item, at));
            }
        }
        self.open = open;
        let mut requests = Vec::new();
        let mut spent = 0;
        for idler in &view.idlers {
            if self.open.iter().any(|(w, _, _)| *w == idler.session) {
                continue;
            }
            let item = view.items.iter().find(|item| {
                !self.open.iter().any(|(_, i, _)| i == *item)
                    && !self
                        .refused
                        .get(*item)
                        .is_some_and(|r| r.contains(&idler.session))
                    && !authored(idler, item, &self.authors)
            });
            match item {
                Some(item) => {
                    self.open.push((idler.session.clone(), item.clone(), now));
                    requests.push((idler.session.clone(), item.clone()));
                }
                None => spent += 1,
            }
        }
        let view = View {
            idle: view.idle.saturating_sub(spent),
            ..view.clone()
        };
        (requests, view)
    }

    /// Forgets the open request to `worker`: its send failed.
    pub fn cancel(&mut self, worker: &str) {
        self.open.retain(|(w, _, _)| w != worker);
    }
}

/// The worker settings that the lead sees at one look.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Seen {
    /// The host name of the machine of the lead.
    pub host: String,
    /// `workers.interval` of the machine of the lead.
    pub interval: u16,
    /// `workers.mcp` of the machine of the lead.
    pub mcp: Vec<String>,
    /// The idle settings of the server. `None` when the server did not
    /// tell them.
    pub idle: Option<Idle>,
    /// The limit of each machine by its host name: the machine of the
    /// lead and each live workers host.
    pub limits: BTreeMap<String, u16>,
}

/// One change of a worker setting (01M3X30KHKB6W11C3NBAW7KCGW).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// `workers.limit` of the machine `host`.
    Limit { host: String, old: u16, new: u16 },
    /// `workers.interval` of the machine of the lead.
    Interval { host: String, old: u16, new: u16 },
    /// `workers.mcp` of the machine `host`.
    Mcp {
        host: String,
        old: Vec<String>,
        new: Vec<String>,
    },
    /// The idle settings of the server.
    Idle { old: Idle, new: Idle },
}

impl Seen {
    /// Each change from `self`, the settings of the look before, to
    /// `now`. A host that only one of the two has gives no change.
    ///
    /// ```
    /// use riff::rollout::{Change, Seen};
    ///
    /// let before = Seen {
    ///     host: "thelio".into(),
    ///     interval: 10,
    ///     mcp: vec!["riff".into()],
    ///     idle: None,
    ///     limits: [("thelio".to_owned(), 2), ("pangolin".to_owned(), 3)].into(),
    /// };
    /// assert_eq!(before.changes(&before), []);
    /// let mut now = before.clone();
    /// now.limits.insert("pangolin".into(), 4);
    /// now.limits.insert("kadomony".into(), 1);
    /// now.interval = 0;
    /// assert_eq!(
    ///     before.changes(&now),
    ///     [
    ///         Change::Limit { host: "pangolin".into(), old: 3, new: 4 },
    ///         Change::Interval { host: "thelio".into(), old: 10, new: 0 },
    ///     ]
    /// );
    /// ```
    pub fn changes(&self, now: &Seen) -> Vec<Change> {
        let mut changes: Vec<Change> = now
            .limits
            .iter()
            .filter_map(|(host, &new)| {
                let old = *self.limits.get(host)?;
                (old != new).then(|| Change::Limit {
                    host: host.clone(),
                    old,
                    new,
                })
            })
            .collect();
        if self.interval != now.interval {
            changes.push(Change::Interval {
                host: now.host.clone(),
                old: self.interval,
                new: now.interval,
            });
        }
        if self.mcp != now.mcp {
            changes.push(Change::Mcp {
                host: now.host.clone(),
                old: self.mcp.clone(),
                new: now.mcp.clone(),
            });
        }
        if let (Some(old), Some(new)) = (self.idle, now.idle)
            && old != new
        {
            changes.push(Change::Idle { old, new });
        }
        changes
    }

    /// Takes the values of `now`. It keeps the limit of a host that
    /// `now` does not have, and the idle settings when `now` has none.
    /// So a host that stops, gets a new limit and starts again gives a
    /// change.
    ///
    /// ```
    /// use riff::rollout::{Change, Seen};
    ///
    /// let with = |limit| Seen {
    ///     limits: [("pangolin".to_owned(), limit)].into(),
    ///     ..Seen::default()
    /// };
    /// let mut known = with(3);
    /// known.keep(Seen::default());
    /// assert_eq!(
    ///     known.changes(&with(4)),
    ///     [Change::Limit { host: "pangolin".into(), old: 3, new: 4 }]
    /// );
    /// ```
    pub fn keep(&mut self, now: Seen) {
        let mut limits = std::mem::take(&mut self.limits);
        limits.extend(
            now.limits
                .iter()
                .map(|(host, limit)| (host.clone(), *limit)),
        );
        let idle = now.idle.or(self.idle);
        *self = Seen {
            limits,
            idle,
            ..now
        };
    }
}

/// What a change of a setting does (01M3X30R4PSBP3RQWM02BJ6GK3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Nothing more than the change.
    Nothing,
    /// Free work waits that the change lets start, and the rollout is
    /// off: the lead starts `count` workers. `remote` is true for a
    /// workers host.
    Waits { count: usize, remote: bool },
    /// This many workers run on the machine: more than its new limit.
    /// The workers over the limit end after their item
    /// (01M402VFGAJQM1QW8B42NKMJM4).
    Over(usize),
}

impl Effect {
    /// True when the message wakes the lead: the lead has a step to do
    /// (01M3X30RA3X08JBJ2JBVCCNEH3).
    pub fn wakes(&self) -> bool {
        matches!(self, Effect::Waits { .. })
    }
}

/// What `change` does, with the `view` of the look that found it. `on`
/// is true when the rollout is on (01M3X30R4PSBP3RQWM02BJ6GK3).
///
/// While the rollout is off, a higher limit lets a person start a worker
/// when no machine had room with the old limit, and one has room with
/// the new limit. While it is on, a higher limit does nothing: the
/// rollout starts no worker in tmux, and the wave of the daemon takes
/// the free work (01M5515CCV7WNKVZVAMQ375J0M). A lower limit stops no
/// worker.
///
/// ```
/// use riff::rollout::{Change, Effect, Place, View, effect};
///
/// let pangolin = Place {
///     host: "pangolin".into(),
///     session: Some("h1".into()),
///     limit: 4,
///     workers: 3,
///     floor: 4,
///     deaths: 0,
///     machine: None,
///     disk: None,
/// };
/// let view = View { running: true, work: 2, idle: 0, places: vec![pangolin], ..View::default() };
/// let raise = Change::Limit { host: "pangolin".into(), old: 3, new: 4 };
/// assert_eq!(effect(&raise, &view, false), Effect::Waits { count: 1, remote: true });
/// // The rollout is on: the wave of the daemon takes the free work.
/// assert_eq!(effect(&raise, &view, true), Effect::Nothing);
/// // No free work: the change starts nothing.
/// assert_eq!(effect(&raise, &View { work: 0, ..view.clone() }, false), Effect::Nothing);
/// // The machine had room before the change.
/// let more = Change::Limit { host: "pangolin".into(), old: 4, new: 5 };
/// let mut wide = view.clone();
/// wide.places[0].limit = 5;
/// assert_eq!(effect(&more, &wide, false), Effect::Nothing);
/// let lower = Change::Limit { host: "pangolin".into(), old: 4, new: 2 };
/// assert_eq!(effect(&lower, &view, true), Effect::Over(3));
/// ```
pub fn effect(change: &Change, view: &View, on: bool) -> Effect {
    let Change::Limit { host, old, new } = change else {
        return Effect::Nothing;
    };
    let Some(i) = view.places.iter().position(|p| &p.host == host) else {
        return Effect::Nothing;
    };
    let place = &view.places[i];
    if new < old {
        return if place.workers > usize::from(*new) {
            Effect::Over(place.workers)
        } else {
            Effect::Nothing
        };
    }
    let mut before = view.clone();
    before.places[i].limit = *old;
    if on || decide(&before).is_some() || decide(view).is_none() {
        return Effect::Nothing;
    }
    let room = usize::from(place.limit).saturating_sub(place.workers);
    Effect::Waits {
        count: view.work.min(room),
        remote: place.session.is_some(),
    }
}

/// What the rollout needs from the world. [`Live`] is the real one.
pub trait Env {
    /// The time between two looks. Zero turns the rollout off.
    fn interval(&self) -> Duration;
    /// The worker settings now, or `None` when this session is not the
    /// lead.
    fn seen(&self) -> impl Future<Output = Result<Option<Seen>>> + Send;
    /// One look, or `None` when this session is not the lead.
    fn look(&self) -> impl Future<Output = Result<Option<View>>> + Send;
    /// Asks the riff daemon of this machine to run the wave of the
    /// repository of the lead (01M53NGVE839TRH7XB7CDPQNZE).
    fn run_wave(&self) -> impl Future<Output = Result<()>> + Send;
    /// Gives `body` to the lead: a message that wakes it when `wake` is
    /// true, else a note.
    fn tell(&self, body: &str, wake: bool) -> impl Future<Output = Result<()>> + Send;
    /// Gives `item` to the idle worker `worker` with a request of the
    /// lead (01M49ZK19GQP79Z8HH14PK85QQ).
    fn offer(&self, worker: &str, item: &str) -> impl Future<Output = Result<()>> + Send;
}

/// Runs the rollout until the task ends. It waits one interval, looks,
/// gives free work to the idle workers ([`Offers`]), and asks the riff
/// daemon for the wave when free work is left ([`asks_wave`]), again and
/// again. It starts no worker in tmux (01M53NGVE839TRH7XB7CDPQNZE). It
/// prints an error once, not again until the error changes.
///
/// At each look it tells the lead each change of a worker setting
/// (01M3X30KHKB6W11C3NBAW7KCGW). A rollout that is off looks at the settings each
/// [`OFF_WAIT`], and asks for no wave.
///
/// The lead gets a note for each ask of the wave, and one note for each
/// new reason why the rollout cannot ask: no daemon answers, or the ask
/// failed (01M5515CA4CZ6KV4ZD6Y0744Z7). A wave that runs, or an ask that
/// works, lets the next reason give a note again.
pub async fn run(env: impl Env) {
    let mut last_error = None;
    let mut known: Option<Seen> = None;
    let mut offers = Offers::default();
    // The reason of the last note that the rollout cannot ask for the
    // wave.
    let mut no_wave: Option<String> = None;
    loop {
        let every = env.interval();
        tokio::time::sleep(if every.is_zero() { OFF_WAIT } else { every }).await;
        let step = async {
            let Some(seen) = env.seen().await? else {
                known = None;
                return anyhow::Ok(());
            };
            let on = seen.interval > 0;
            let host = seen.host.clone();
            let changed = known.as_ref().map(|k| k.changes(&seen)).unwrap_or_default();
            // One look uses each limit one time: a limit that changes
            // after `seen` has no effect before the next look.
            let view = if on || !changed.is_empty() {
                env.look().await?.map(|view| view.with_limits(&seen))
            } else {
                None
            };
            for change in &changed {
                let effect = match &view {
                    Some(view) => effect(change, view, on),
                    None => Effect::Nothing,
                };
                env.tell(&text::setting_changed(change, &effect), effect.wakes())
                    .await?;
            }
            // The lead has each change now: a failed start tells none again.
            match &mut known {
                Some(known) => known.keep(seen),
                None => known = Some(seen),
            }
            let Some(view) = view.filter(|view| on && view.running) else {
                return Ok(());
            };
            let now = tokio::time::Instant::now().into_std();
            let (requests, view) = offers.plan(&view, now, offer_wait(every));
            for (worker, item) in requests {
                if let Err(e) = env.offer(&worker, &item).await {
                    offers.cancel(&worker);
                    return Err(e);
                }
            }
            let why = match &view.wave {
                WaveState::Runs => {
                    no_wave = None;
                    return Ok(());
                }
                WaveState::NoDaemon(why) => why.clone(),
                WaveState::None if asks_wave(&view) => match env.run_wave().await {
                    Ok(()) => {
                        no_wave = None;
                        eprintln!("riff: the rollout asked the riff daemon to run the wave");
                        return env.tell(&text::rollout_wave(&host, &view.repo), false).await;
                    }
                    Err(e) => format!("{e:#}"),
                },
                WaveState::None => return Ok(()),
            };
            if no_wave.as_ref() != Some(&why) {
                env.tell(&text::rollout_no_wave(&host, &view.repo, &why), false)
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

/// The authors of the issues `of` from their usage comments in `open`
/// (01M51533BJSD0RB7BFQR9CAFYW): the session of each valid report of a
/// work claim `issue-N` on the issue N ([`crate::usage::Report`]). The
/// comment stays after the end of the session that held the claim, and
/// after a new start of `riff mcp`. Each person who can comment can
/// write one. A false one only keeps a request from a worker.
///
/// ```
/// use riff::rollout::{Comment, Issue, authors};
/// use riff::usage::{Claim, Report};
///
/// let comment = |item: &str, session: &str| {
///     let claim = Claim { thread: "o/r".into(), item: item.into(), from_ms: 1, to_ms: Some(2), models: Default::default() };
///     Comment { body: Report::of(&claim, session).unwrap().1.comment() }
/// };
/// let issue = Issue {
///     number: 12,
///     comments: vec![
///         comment("issue-12", "a6cf0123-4567"),
///         // A verify, and a report of another issue: no author.
///         comment("verify-issue-12", "b7d01234-4567"),
///         comment("issue-13", "c8e01234-4567"),
///         Comment { body: "A comment of a person.".into() },
///     ],
///     ..Issue::default()
/// };
/// let authors = authors(&[issue], &[12, 14]);
/// assert_eq!(authors, [(12, ["a6cf0123".to_owned()].into())].into());
/// ```
pub fn authors(open: &[Issue], of: &[u64]) -> Authors {
    let mut authors = Authors::new();
    for issue in open.iter().filter(|i| of.contains(&i.number)) {
        let sessions: BTreeSet<String> = issue
            .comments
            .iter()
            .filter_map(|c| usage::Report::parse(&c.body))
            .filter(|r| r.valid(issue.number) && r.kind == usage::Kind::Work)
            .map(|r| r.session)
            .collect();
        if !sessions.is_empty() {
            authors.insert(issue.number, sessions);
        }
    }
    authors
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

/// The authors of the claims in `sessions` for the lead `me`
/// (01M51533BJSD0RB7BFQR9CAFYW): each session in the repository of `me`
/// with a claim `issue-N`, by N. A `/clear` keeps the session ID, so
/// [`Offers`] knows the author after its clear.
///
/// ```
/// use riff::rollout::holders;
/// use riff_core::wire::SessionInfo;
///
/// let session = |uri: &str| SessionInfo {
///     uri: uri.parse().unwrap(),
///     live: true,
///     idle_secs: 0,
///     status: None,
///     worker: true,
///     container: false,
///     binary: None,
///     stopping: false,
///     claims_secs: 0,
///     must_clear: false,
///     fresh_secs: None,
///     state: None,
///     work: None,
///     waits: None,
///     blocked: None,
///     step: None,
///     machine: None,
/// };
/// let me = "riff://mike@pangolin/o/riff?session=l1&lead=true".parse().unwrap();
/// let sessions = [
///     session("riff://mike@pangolin/o/riff?session=w1&claim=issue-12#issue-12"),
///     session("riff://mike@pangolin/o/riff?session=w2&claim=verify-issue-7"),
///     session("riff://brett@kadomony/o/riff?session=w3&claim=issue-7"),
///     session("riff://mike@pangolin/o/strata?session=w4&claim=issue-12"),
/// ];
/// assert_eq!(
///     holders(&sessions, &me),
///     [(7, ["w3".to_owned()].into()), (12, ["w1".to_owned()].into())].into()
/// );
/// ```
pub fn holders(sessions: &[SessionInfo], me: &SessionUri) -> Authors {
    let mut authors = Authors::new();
    for s in sessions
        .iter()
        .filter(|s| s.uri.place().repo() == me.place().repo())
    {
        let Some(session) = s.uri.who().session() else {
            continue;
        };
        for claim in s.uri.claims() {
            if let Some((issue, usage::Kind::Work)) = usage::issue_of(claim) {
                authors.entry(issue).or_default().insert(session.to_owned());
            }
        }
    }
    authors
}

/// The idle workers for the lead `me`: each worker pane in `panes` whose
/// session holds no claim, also one that is not in `sessions` yet, and
/// each live worker of another user with no claim. A worker that the
/// server asked to stop is not idle: it goes away. A worker in another
/// repository than `me` is not idle: it cannot take the free work
/// (01M3W27BJYFQCHY5MTZ2J4SKW4). A pane can hold the short session ID of a host status.
///
/// ```
/// use riff::rollout::idle;
/// use riff_core::wire::SessionInfo;
///
/// let worker = |uri: &str| SessionInfo {
///     uri: uri.parse().unwrap(),
///     live: true,
///     idle_secs: 0,
///     status: None,
///     worker: true,
///     container: false,
///     binary: None,
///     stopping: false,
///     claims_secs: 0,
///     must_clear: false,
///     fresh_secs: None,
///     state: None,
///     work: None,
///     waits: None,
///     blocked: None,
///     step: None,
///     machine: None,
/// };
/// let me = "riff://mike@pangolin/o/riff?session=l1&lead=true".parse().unwrap();
/// let strata = worker("riff://brett@kadomony/o/strata?session=w1");
/// assert_eq!(idle(&[strata], &me, &[]), 0);
/// let riff = worker("riff://brett@kadomony/o/riff?session=w2");
/// assert_eq!(idle(std::slice::from_ref(&riff), &me, &[]), 1);
/// // A worker on a task of the lead with no work item holds a task
/// // claim: it is busy (01M518P1N8R893MG9Z3VAQZR6Z).
/// let task = worker("riff://brett@kadomony/o/riff?session=w3&claim=task-iroh-listener");
/// assert_eq!(idle(&[task], &me, &[]), 0);
/// // A session in a container takes no free work: the daemon gives it
/// // its item (01M510MG808MQT08W6VSPKM1A6).
/// assert_eq!(idle(&[SessionInfo { container: true, ..riff }], &me, &[]), 0);
/// ```
pub fn idle(sessions: &[SessionInfo], me: &SessionUri, panes: &[WorkerPane]) -> usize {
    let (user, repo) = (me.who().user(), me.place().repo());
    // A worker that holds a claim is busy. A worker that the server
    // asked to stop goes away. A worker in another repository cannot
    // take the free work. Nor can a session in a container: the daemon
    // gives it its item (01M510MG808MQT08W6VSPKM1A6).
    let away = |s: &SessionInfo| {
        !s.uri.claims().is_empty() || s.stopping || s.container || s.uri.place().repo() != repo
    };
    let of = |pane: &WorkerPane, s: &SessionInfo| {
        s.uri
            .who()
            .session()
            .is_some_and(|id| id.starts_with(&pane.session))
    };
    let mine = panes
        .iter()
        .filter(|p| !sessions.iter().any(|s| of(p, s) && away(s)))
        .count();
    let others = sessions
        .iter()
        .filter(|s| s.worker && s.live && s.uri.who().user() != user)
        .filter(|s| !away(s))
        .count();
    mine + others
}

/// The idle workers of the user of the lead `me` that joined the riff
/// and can take a request (01M49ZK19GQP79Z8HH14PK85QQ): live, in the
/// repository of the lead, with no claim, not asked to stop, and with
/// no clear of the context to wait for. A task claim `task-NAME` is a
/// claim too (01M518P1N8R893MG9Z3VAQZR6Z). A session in a container
/// takes no request (01M510MG808MQT08W6VSPKM1A6).
///
/// ```
/// use riff::rollout::{Idler, idlers};
/// use riff_core::wire::SessionInfo;
///
/// let worker = |uri: &str| SessionInfo {
///     uri: uri.parse().unwrap(),
///     live: true,
///     idle_secs: 0,
///     status: None,
///     worker: true,
///     container: false,
///     binary: None,
///     stopping: false,
///     claims_secs: 0,
///     must_clear: false,
///     fresh_secs: None,
///     state: None,
///     work: None,
///     waits: None,
///     blocked: None,
///     step: None,
///     machine: None,
/// };
/// let me = "riff://mike@pangolin/o/riff?session=l1&lead=true".parse().unwrap();
/// let sessions = [
///     worker("riff://mike@pangolin/o/riff?session=w1#issue-12"),
///     worker("riff://mike@pangolin/o/riff?session=w2&claim=issue-7"),
///     worker("riff://mike@pangolin/o/strata?session=w3"),
///     worker("riff://brett@kadomony/o/riff?session=w4"),
///     SessionInfo { must_clear: true, ..worker("riff://mike@pangolin/o/riff?session=w5") },
///     SessionInfo { container: true, ..worker("riff://mike@pangolin/o/riff?session=w6") },
///     worker("riff://mike@pangolin/o/riff?session=w7&claim=task-iroh-listener"),
/// ];
/// assert_eq!(idlers(&sessions, &me), [Idler { session: "w1".into(), worktree: Some("issue-12".into()) }]);
/// ```
pub fn idlers(sessions: &[SessionInfo], me: &SessionUri) -> Vec<Idler> {
    sessions
        .iter()
        .filter(|s| s.worker && s.live && !s.stopping && !s.must_clear && !s.container)
        .filter(|s| s.uri.who().user() == me.who().user())
        .filter(|s| s.uri.place().repo() == me.place().repo())
        .filter(|s| s.uri.claims().is_empty())
        .filter_map(|s| {
            Some(Idler {
                session: s.uri.who().session()?.to_owned(),
                worktree: s.uri.place().worktree().map(str::to_owned),
            })
        })
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

/// The free work of the repository `repo` (`OWNER/REPO`), with `gh`:
/// [`free_work_of`] with its open milestones, its open issues and its
/// open pull requests. The usage comments of the issue of each verify
/// name its authors ([`authors`], 01M51533BJSD0RB7BFQR9CAFYW).
pub fn free_work(
    gh: &Gh,
    repo: &str,
    claims: &HashSet<String>,
    holds: &HashSet<String>,
) -> Result<Work> {
    let open: Vec<Milestone> = gh.json(&[
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
    Ok(Work {
        items: free_work_of(&open, &issues, &pulls, claims, holds),
        authors: authors(&issues, &waiting_verifies(&pulls, claims)),
    })
}

/// The free work of a look ([`free_work`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Work {
    /// The free work by name, the verifies first: `verify-issue-N` and
    /// `issue-N` ([`free_work_of`]).
    pub items: Vec<String>,
    /// The authors of the issue of each verify, from its usage comments
    /// ([`authors`]).
    pub authors: Authors,
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

/// The real world of the rollout: the riff, `gh`, tmux and the riff
/// daemon.
pub struct Live<M> {
    pub api: Api,
    /// The session now: it can move.
    pub me: M,
    /// The tmux of the lead, if it runs in tmux.
    pub tmux: Option<Tmux>,
    pub gh: Arc<Gh>,
}

impl<M: Fn() -> SessionUri + Send + Sync> Env for Live<M> {
    fn interval(&self) -> Duration {
        let seconds = settings::path()
            .and_then(|p| settings::workers_interval(&p))
            .unwrap_or(settings::WORKERS_INTERVAL);
        Duration::from_secs(u64::from(seconds))
    }

    async fn seen(&self) -> Result<Option<Seen>> {
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
        let settings = settings::path()?;
        let host = me.place().host().to_owned();
        let mut limits = BTreeMap::new();
        limits.insert(host.clone(), settings::workers_limit(&settings)?);
        for (info, status) in host::hosts(&sessions, me.who().user()) {
            limits
                .entry(info.uri.place().host().to_owned())
                .or_insert(status.limit);
        }
        Ok(Some(Seen {
            host,
            interval: settings::workers_interval(&settings)?,
            mcp: settings::workers_mcp(&settings)?,
            // A server that does not answer gives no change.
            idle: self.api.idle(&me, None, None).await.ok(),
            limits,
        }))
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
        let settings = settings::path()?;
        let limit = settings::workers_limit(&settings)?;
        // A worker of this machine starts in the main clone: where riff
        // is off there, the machine is no place for one
        // (01M3XY2T542DCHBN95H9PX4AGQ).
        let dir = identity::working_dir()?;
        if let Some(tmux) = &self.tmux
            && limit > 0
            && worker::off(&dir).is_none()
        {
            let here = tmux.worker_panes()?;
            places.push(Place {
                host: me.place().host().to_owned(),
                session: None,
                limit,
                workers: here.len(),
                floor: settings::workers_floor(&settings)?,
                deaths: crate::deaths::here(),
                machine: Some(Machine::here()),
                disk: identity::main_worktree(&dir).and_then(|main| Disk::here(&main)),
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
                workers: status.runs(),
                floor: status.floor,
                deaths: status.deaths,
                machine: status.machine,
                disk: status.disk,
            });
            panes.extend(status.workers.iter().map(|(pane, short)| WorkerPane {
                pane: pane.clone(),
                session: short.clone(),
            }));
        }
        let idle = idle(&sessions, &me, &panes);
        let idlers = idlers(&sessions, &me);
        let mut authors = holders(&sessions, &me);
        let repo = me.place().repo_text();
        let wave = match crate::daemon::dir() {
            Ok(dir) => match crate::daemon::status(&dir).await {
                Ok(status) if crate::wave::runs(&status, &repo) => WaveState::Runs,
                Ok(_) => WaveState::None,
                Err(e) => WaveState::NoDaemon(format!("{e:#}")),
            },
            Err(e) => WaveState::NoDaemon(format!("{e:#}")),
        };
        // The free work matters to an idle worker, to the ask of the
        // wave, and to a person who can start a worker on a machine with
        // room while no wave runs ([`effect`]). Else: no call of gh.
        let matters = match &wave {
            WaveState::None => true,
            WaveState::Runs => false,
            WaveState::NoDaemon(_) => places.iter().any(Place::room),
        };
        if idlers.is_empty() && !matters {
            return Ok(Some(View {
                running: true,
                idle,
                places,
                authors,
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
        add(&mut authors, &work.authors);
        Ok(Some(View {
            running: true,
            work: work.items.len(),
            idle,
            places,
            items: work.items,
            idlers,
            authors,
            repo,
            wave,
        }))
    }

    async fn run_wave(&self) -> Result<()> {
        let me = (self.me)();
        // `riff mcp` talks on its stdout: the lines of the steps go to
        // its stderr.
        crate::wave::ask(&crate::daemon::dir()?, me.place(), self.api.base(), |line| {
            eprintln!("riff: {line}");
        })
        .await?;
        Ok(())
    }

    async fn tell(&self, body: &str, wake: bool) -> Result<()> {
        let kind = if wake { Kind::Message } else { Kind::Note };
        post_lead(self.api.base(), &(self.me)(), body, kind).await
    }

    async fn offer(&self, worker: &str, item: &str) -> Result<()> {
        self.api.tell(&(self.me)(), worker, &request(item)).await?;
        Ok(())
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

    /// A fake world: a riff with free work, machines and a riff daemon.
    /// An ask of the wave makes the wave of the daemon run, unless
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
        /// The idle workers: each one with no claim, also one that did
        /// not join yet.
        idle: usize,
        places: Vec<Place>,
        /// The wave of the repository on the daemon.
        wave: WaveState,
        /// Why an ask of the wave fails. `None`: it works, and the wave
        /// runs.
        ask_fails: Option<String>,
        /// The time of each ask of the wave.
        asks: Vec<tokio::time::Instant>,
        /// Each message to the lead, with true when it wakes the lead.
        told: Vec<(String, bool)>,
        /// A person sets this limit on the first machine in the middle
        /// of the next look: after its `seen`, before its `look`.
        limit_in_look: Option<u16>,
        /// The free work by name, for the requests.
        items: Vec<String>,
        /// The idle workers that joined. Each one is also in `idle`.
        idlers: Vec<Idler>,
        /// Each request: the worker and the item, with its time.
        offers: Vec<(String, String, tokio::time::Instant)>,
        /// The authors that a look sees.
        authors: Authors,
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

        fn told(&self) -> Vec<(String, bool)> {
            self.with(|w| w.told.clone())
        }
    }

    impl Env for Arc<Fake> {
        fn interval(&self) -> Duration {
            self.with(|w| w.interval)
        }

        async fn seen(&self) -> Result<Option<Seen>> {
            Ok(self.with(|w| {
                let seen = w.lead.then(|| Seen {
                    host: w.places[0].host.clone(),
                    interval: w.interval.as_secs() as u16,
                    mcp: vec!["riff".into()],
                    idle: Some(Idle::default()),
                    limits: w.places.iter().map(|p| (p.host.clone(), p.limit)).collect(),
                });
                if let Some(limit) = w.limit_in_look.take() {
                    w.places[0].limit = limit;
                }
                seen
            }))
        }

        async fn tell(&self, body: &str, wake: bool) -> Result<()> {
            self.with(|w| w.told.push((body.to_owned(), wake)));
            Ok(())
        }

        async fn look(&self) -> Result<Option<View>> {
            Ok(self.with(|w| {
                w.lead.then(|| View {
                    running: w.running,
                    work: w.work,
                    idle: w.idle,
                    places: w.places.clone(),
                    items: w.items.clone(),
                    idlers: w.idlers.clone(),
                    authors: w.authors.clone(),
                    repo: "o/r".into(),
                    wave: w.wave.clone(),
                })
            }))
        }

        async fn offer(&self, worker: &str, item: &str) -> Result<()> {
            self.with(|w| {
                let now = tokio::time::Instant::now();
                w.offers.push((worker.to_owned(), item.to_owned(), now));
            });
            Ok(())
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

    fn machine(cores: u16, mhz: u32, mem_gb: u32) -> Option<Machine> {
        Some(Machine {
            cores,
            mhz,
            now_mhz: mhz,
            mem_gb,
            avail_gb: mem_gb,
            load: 0.0,
        })
    }

    fn thelio() -> Place {
        Place {
            host: "thelio".into(),
            session: None,
            limit: 4,
            workers: 0,
            floor: 4,
            deaths: 0,
            machine: machine(32, 6000, 128),
            disk: None,
        }
    }

    fn pangolin() -> Place {
        Place {
            host: "pangolin".into(),
            session: Some("h1".into()),
            limit: 4,
            workers: 0,
            floor: 4,
            deaths: 0,
            machine: machine(16, 4500, 32),
            disk: None,
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
            wave: WaveState::None,
            ask_fails: None,
            asks: Vec::new(),
            told: Vec::new(),
            limit_in_look: None,
            items: Vec::new(),
            idlers: Vec::new(),
            offers: Vec::new(),
            authors: Authors::new(),
        }
    }

    /// Runs the rollout for `secs` seconds of paused time.
    async fn run_for(fake: &Arc<Fake>, secs: u64) {
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(Duration::from_secs(secs) + Duration::from_millis(1)).await;
        task.abort();
    }

    /// The note of an ask of the wave of the fake.
    fn asked() -> (String, bool) {
        (text::rollout_wave("thelio", "o/r"), false)
    }

    /// Free work and no wave on the daemon: the first look asks the
    /// daemon one time to run the wave, and tells the lead. The wave
    /// runs, so no later look asks again. No worker starts in tmux
    /// (01M53NGVE839TRH7XB7CDPQNZE, 01M5515CA4CZ6KV4ZD6Y0744Z7).
    #[tokio::test(start_paused = true)]
    async fn it_asks_the_daemon_one_time_for_the_wave_and_starts_no_worker() {
        let fake = Fake::new(world(3));
        let begin = tokio::time::Instant::now();
        run_for(&fake, 60).await;
        assert_eq!(fake.asks(begin), [10]);
        assert_eq!(fake.told(), [asked()]);
        assert!(fake.with(|w| w.places.iter().all(|p| p.workers == 0)));
    }

    #[tokio::test(start_paused = true)]
    async fn the_interval_setting_changes_the_time_of_the_ask() {
        let fake = Fake::new(World {
            interval: Duration::from_secs(30),
            ..world(3)
        });
        let begin = tokio::time::Instant::now();
        run_for(&fake, 60).await;
        assert_eq!(fake.asks(begin), [30]);
    }

    #[tokio::test(start_paused = true)]
    async fn an_interval_of_zero_asks_nothing() {
        let fake = Fake::new(World {
            interval: Duration::ZERO,
            ..world(3)
        });
        run_for(&fake, 60).await;
        assert!(fake.with(|w| w.asks.is_empty()));
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

    /// An idle worker that did not join yet can take the free work: no
    /// ask while it is idle. Its claim ends the wait
    /// (01M5515CA4CZ6KV4ZD6Y0744Z7).
    #[tokio::test(start_paused = true)]
    async fn no_ask_while_a_worker_is_idle() {
        let fake = Fake::new(World {
            idle: 1,
            ..world(3)
        });
        let begin = tokio::time::Instant::now();
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(Duration::from_secs(45)).await;
        assert!(fake.with(|w| w.asks.is_empty()));
        fake.with(|w| w.idle = 0);
        tokio::time::sleep(Duration::from_secs(10)).await;
        task.abort();
        assert_eq!(fake.asks(begin), [50]);
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
        let no = |why| (text::rollout_no_wave("thelio", "o/r", why), false);
        assert_eq!(
            fake.told(),
            [no("no session image"), no("the daemon is gone"), asked()]
        );
    }

    /// No daemon answers: the lead gets one note with the reason, and
    /// the rollout asks nothing. An idle worker still gets its request
    /// (01M53NGVE839TRH7XB7CDPQNZE, 01M5515CA4CZ6KV4ZD6Y0744Z7).
    #[tokio::test(start_paused = true)]
    async fn no_daemon_gives_one_note_and_the_idle_worker_still_gets_work() {
        let fake = Fake::new(World {
            wave: WaveState::NoDaemon("no riff daemon listens".into()),
            ..one_idle("issue-7")
        });
        let begin = tokio::time::Instant::now();
        run_for(&fake, 60).await;
        assert!(fake.with(|w| w.asks.is_empty()));
        assert_eq!(
            fake.told(),
            [(
                text::rollout_no_wave("thelio", "o/r", "no riff daemon listens"),
                false
            )]
        );
        assert_eq!(offers(&fake, begin), [("w1".into(), "issue-7".into(), 10)]);
    }

    fn idler(session: &str) -> Idler {
        Idler {
            session: session.into(),
            worktree: None,
        }
    }

    /// One idle worker that joined, and the free work `item`.
    fn one_idle(item: &str) -> World {
        World {
            idle: 1,
            idlers: vec![idler("w1")],
            items: vec![item.into()],
            ..world(1)
        }
    }

    /// The requests up to now: the worker, the item and the second.
    fn offers(fake: &Fake, begin: tokio::time::Instant) -> Vec<(String, String, u64)> {
        fake.with(|w| {
            w.offers
                .iter()
                .map(|(s, i, at)| (s.clone(), i.clone(), (*at - begin).as_secs()))
                .collect()
        })
    }

    /// 01M53NGVE839TRH7XB7CDPQNZE: an idle worker in tmux still gets free
    /// work, as in riff 1.x, and the rollout asks for no wave for it.
    #[tokio::test(start_paused = true)]
    async fn an_idle_worker_gets_a_request_for_a_waiting_verify() {
        let fake = Fake::new(one_idle("verify-issue-12"));
        let begin = tokio::time::Instant::now();
        run_for(&fake, 15).await;
        assert_eq!(
            offers(&fake, begin),
            [("w1".into(), "verify-issue-12".into(), 10)]
        );
        assert!(
            fake.with(|w| w.asks.is_empty()),
            "the idle worker has the work"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_idle_worker_gets_a_request_for_a_free_item() {
        let fake = Fake::new(one_idle("issue-7"));
        let begin = tokio::time::Instant::now();
        run_for(&fake, 15).await;
        assert_eq!(offers(&fake, begin), [("w1".into(), "issue-7".into(), 10)]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_worker_gets_no_second_request_for_the_same_item() {
        let fake = Fake::new(one_idle("issue-7"));
        run_for(&fake, 600).await;
        assert_eq!(fake.with(|w| w.offers.len()), 1);
    }

    /// The worker does not claim in 6 intervals: it refused the item, so
    /// the item goes to the wave of the daemon. A new worker that joins
    /// later still gets the request (01M49ZK1C1P817KE63EXV3EJDC).
    #[tokio::test(start_paused = true)]
    async fn an_item_that_a_worker_refuses_goes_to_the_wave() {
        let fake = Fake::new(one_idle("issue-7"));
        let begin = tokio::time::Instant::now();
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(Duration::from_secs(75)).await;
        assert_eq!(fake.asks(begin), [70]);
        fake.with(|w| {
            w.idle += 1;
            w.idlers.push(idler("w2"));
        });
        tokio::time::sleep(Duration::from_secs(600)).await;
        task.abort();
        assert_eq!(fake.asks(begin), [70], "the wave runs: no second ask");
        assert_eq!(
            offers(&fake, begin),
            [
                ("w1".into(), "issue-7".into(), 10),
                ("w2".into(), "issue-7".into(), 80)
            ]
        );
    }

    #[test]
    fn each_worker_gets_its_own_item_and_no_verify_of_its_worktree() {
        let author = Idler {
            session: "w1".into(),
            worktree: Some("issue-12".into()),
        };
        let view = View {
            running: true,
            work: 2,
            idle: 3,
            items: vec!["verify-issue-12".into(), "issue-7".into()],
            idlers: vec![author, idler("w2"), idler("w3")],
            ..View::default()
        };
        let mut offers = Offers::default();
        let (requests, seen) = offers.plan(&view, Instant::now(), Duration::from_secs(60));
        assert_eq!(
            requests,
            [
                ("w1".to_owned(), "issue-7".to_owned()),
                ("w2".to_owned(), "verify-issue-12".to_owned())
            ]
        );
        // w3 has no item to take: it does not count as idle.
        assert_eq!(seen.idle, 2);
    }

    /// w1 held issue-12. It released the item at its verify request, and
    /// riff cleared its context: the session ID stays, and the worktree
    /// is gone. w1 never gets a request for the verify. So the rollout
    /// asks the daemon for the wave. A new worker w2 that joins gets the
    /// request (01M51533BJSD0RB7BFQR9CAFYW).
    #[tokio::test(start_paused = true)]
    async fn the_author_gets_no_request_for_its_verify_after_a_clear() {
        let fake = Fake::new(World {
            authors: [(12, ["w1".to_owned()].into())].into(),
            ..world(0)
        });
        let begin = tokio::time::Instant::now();
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(Duration::from_secs(15)).await;
        fake.with(|w| {
            // The release and the clear of w1: no claim names it now.
            w.authors.clear();
            w.work = 1;
            w.idle = 1;
            w.items = vec!["verify-issue-12".into()];
            w.idlers = vec![idler("w1")];
        });
        tokio::time::sleep(Duration::from_secs(10)).await;
        fake.with(|w| {
            // A worker joins.
            w.idle += 1;
            w.idlers.push(idler("w2"));
        });
        tokio::time::sleep(Duration::from_secs(10)).await;
        task.abort();
        assert_eq!(fake.asks(begin), [20], "the wave takes the verify");
        assert_eq!(
            offers(&fake, begin),
            [("w2".into(), "verify-issue-12".into(), 30)]
        );
    }

    /// A usage comment names the author with the first characters of its
    /// session ID: no request of its verify goes to that session, also
    /// from a rollout that never saw the claim (01M51533BJSD0RB7BFQR9CAFYW).
    #[test]
    fn the_session_of_a_usage_comment_gets_no_request_for_the_verify() {
        let view = View {
            running: true,
            work: 2,
            idle: 2,
            items: vec!["verify-issue-12".into(), "issue-7".into()],
            idlers: vec![idler("a6cf0123-4567"), idler("b7d01234-4567")],
            authors: [(12, ["a6cf0123".to_owned()].into())].into(),
            ..View::default()
        };
        let (requests, _) = Offers::default().plan(&view, Instant::now(), Duration::from_secs(60));
        assert_eq!(
            requests,
            [
                ("a6cf0123-4567".to_owned(), "issue-7".to_owned()),
                ("b7d01234-4567".to_owned(), "verify-issue-12".to_owned())
            ]
        );
    }

    #[test]
    fn the_bigger_machine_has_the_most_free_capacity() {
        // thelio scores 64 and pangolin 24.
        assert_eq!(pick(&[thelio(), pangolin()]), Some(0));
        assert_eq!(pick(&[pangolin(), thelio()]), Some(1));
    }

    #[test]
    fn the_smaller_machine_wins_when_it_has_the_most_room() {
        let full = Place {
            workers: 60,
            limit: 70,
            ..thelio()
        };
        // thelio has 64 - 60 = 4 free, pangolin 24.
        assert_eq!(pick(&[full, pangolin()]), Some(1));
    }

    /// A higher limit while the rollout is on: the lead gets a note with
    /// only the change. The rollout starts no worker in tmux, and the
    /// wave of the daemon takes the free work (01M3X30KHKB6W11C3NBAW7KCGW,
    /// 01M5515CCV7WNKVZVAMQ375J0M).
    #[tokio::test(start_paused = true)]
    async fn a_higher_limit_gives_a_note_and_starts_no_worker_while_the_rollout_is_on() {
        let mut w = world(3);
        w.wave = WaveState::Runs;
        w.places = vec![Place {
            limit: 1,
            workers: 1,
            ..pangolin()
        }];
        let fake = Fake::new(w);
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(Duration::from_secs(35)).await;
        assert!(fake.told().is_empty(), "no change, no message");

        fake.with(|w| w.places[0].limit = 2);
        tokio::time::sleep(Duration::from_secs(10)).await;
        task.abort();
        assert_eq!(fake.with(|w| w.places[0].workers), 1);
        assert_eq!(
            fake.told(),
            [("workers: limit 1 to 2 on pangolin.".to_owned(), false)]
        );
    }

    /// A person sets a higher limit in the middle of a look, while the
    /// rollout is off: after the look read the limits, before it reads
    /// the machines. That look uses the old limit, so it gives no
    /// message. The next look gives one message, with the room of the
    /// new limit (01M3XFHSYJEN9V6QEKWJGJWQ8Q).
    #[tokio::test(start_paused = true)]
    async fn a_limit_that_changes_in_a_look_gives_one_message_at_the_next_look() {
        let mut w = world(3);
        w.interval = Duration::ZERO;
        w.places = vec![Place {
            limit: 1,
            workers: 1,
            ..pangolin()
        }];
        let fake = Fake::new(w);
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(OFF_WAIT * 2).await;
        fake.with(|w| w.limit_in_look = Some(2));
        tokio::time::sleep(OFF_WAIT).await;
        assert_eq!(fake.with(|w| w.places[0].limit), 2);
        assert!(fake.told().is_empty(), "the look with the old limit");
        tokio::time::sleep(OFF_WAIT * 2).await;
        task.abort();
        assert_eq!(
            fake.told(),
            [(
                "workers: limit 1 to 2 on pangolin: free work waits, and the rollout is off. \
                 Start workers with: riff workers start 1 --host pangolin"
                    .to_owned(),
                true
            )]
        );
    }

    /// With the rollout off, a higher limit wakes the lead: work waits,
    /// and only a person can start the worker (01M3X30RA3X08JBJ2JBVCCNEH3).
    /// A rollout that is off asks for no wave.
    #[tokio::test(start_paused = true)]
    async fn a_higher_limit_wakes_the_lead_when_the_rollout_is_off() {
        let mut w = world(3);
        w.interval = Duration::ZERO;
        w.places = vec![Place {
            limit: 1,
            workers: 1,
            ..pangolin()
        }];
        let fake = Fake::new(w);
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(OFF_WAIT * 2).await;
        fake.with(|w| w.places[0].limit = 3);
        tokio::time::sleep(OFF_WAIT * 2).await;
        task.abort();
        assert!(
            fake.with(|w| w.asks.is_empty()),
            "a rollout that is off asks nothing"
        );
        assert_eq!(
            fake.told(),
            [(
                "workers: limit 1 to 3 on pangolin: free work waits, and the rollout is off. \
                 Start workers with: riff workers start 2 --host pangolin"
                    .to_owned(),
                true
            )]
        );
    }

    /// A lower limit stops no worker at once, and the lead gets a note
    /// that says how many end after their item
    /// (01M402VFGAJQM1QW8B42NKMJM4). A change with no free work gives a
    /// note with only the change.
    #[tokio::test(start_paused = true)]
    async fn a_lower_limit_gives_a_note_and_stops_no_worker() {
        let mut w = world(0);
        w.places[1].workers = 3;
        let fake = Fake::new(w);
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(Duration::from_secs(15)).await;
        fake.with(|w| {
            w.places[1].limit = 2;
            w.places[0].limit = 6;
        });
        tokio::time::sleep(Duration::from_secs(10)).await;
        task.abort();
        assert_eq!(fake.with(|w| w.places[1].workers), 3);
        assert_eq!(
            fake.told(),
            [
                (
                    "workers: limit 4 to 2 on pangolin: 3 workers run there. 1 worker ends after \
                     its item. riff stops no worker in the middle of an item."
                        .to_owned(),
                    false
                ),
                ("workers: limit 4 to 6 on thelio.".to_owned(), false),
            ]
        );
    }

    /// A session that is not the lead tells nothing, and a new lead
    /// starts from the settings of its first look.
    #[tokio::test(start_paused = true)]
    async fn only_the_lead_gets_the_changes() {
        let fake = Fake::new(World {
            lead: false,
            ..world(0)
        });
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(Duration::from_secs(15)).await;
        fake.with(|w| w.places[0].limit = 9);
        tokio::time::sleep(Duration::from_secs(10)).await;
        fake.with(|w| w.lead = true);
        tokio::time::sleep(Duration::from_secs(20)).await;
        task.abort();
        assert!(fake.told().is_empty());
    }

    #[test]
    fn a_busy_machine_gets_no_worker() {
        let mut busy = thelio();
        busy.machine = busy.machine.map(|m| Machine { load: 40.0, ..m });
        assert_eq!(pick(&[busy.clone(), pangolin()]), Some(1));
        assert_eq!(pick(&[busy]), None);
    }

    #[test]
    fn a_machine_under_its_floor_gets_no_worker() {
        let mut low = thelio();
        low.machine = low.machine.map(|m| Machine { avail_gb: 3, ..m });
        assert!(!low.room());
        assert_eq!(pick(&[low.clone(), pangolin()]), Some(1));
        assert_eq!(pick(std::slice::from_ref(&low)), None);
        // A floor of 0 turns the floor off.
        assert!(Place { floor: 0, ..low }.room());
    }

    /// 01M41A11DX1QRP48YPTDNT67W4: under 5% of free disk, a machine has
    /// no room for a worker. The other one has room.
    #[test]
    fn a_machine_with_a_low_disk_gets_no_worker() {
        let mut low = thelio();
        low.disk = Some(Disk {
            free_gb: 4,
            total_gb: 100,
        });
        let mut ok = pangolin();
        ok.disk = Some(Disk {
            free_gb: 5,
            total_gb: 100,
        });
        assert!(!low.room());
        assert_eq!(pick(&[low, ok]), Some(1));
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
            container: false,
            binary: None,
            stopping: false,
            claims_secs: 0,
            must_clear: false,
            fresh_secs: None,
            state: Some(riff_core::wire::SessionState::Idle),
            work: None,
            waits: None,
            blocked: None,
            step: None,
            machine: None,
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
        let me: SessionUri = "riff://mike@thelio/o/r?session=l1&lead=true"
            .parse()
            .unwrap();
        // bbbb and ffff of mike, cccc of brett.
        assert_eq!(idle(&sessions, &me, &panes), 3);
        // A worker that the server asks to stop is not idle.
        let mut stopping = sessions.clone();
        stopping[1].stopping = true;
        stopping[2].stopping = true;
        assert_eq!(idle(&stopping, &me, &panes), 1);
        assert_eq!(claims(&sessions).len(), 2);
    }

    /// An idle worker counts only when it can take the free work: a
    /// worker in the repository of the lead (01M3W27BJYFQCHY5MTZ2J4SKW4).
    #[test]
    fn idle_counts_only_workers_in_the_repository_of_the_lead() {
        let me: SessionUri = "riff://mike@pangolin/o/riff?session=l1&lead=true"
            .parse()
            .unwrap();
        let other = info("riff://brett@kadomony/o/strata?session=cccc3333-x", true);
        assert_eq!(idle(std::slice::from_ref(&other), &me, &[]), 0);
        // A repository with the same name of another owner is another
        // repository.
        let owner = info("riff://brett@kadomony/p/riff?session=cccc3333-x", true);
        assert_eq!(idle(&[owner], &me, &[]), 0);
        let same = info("riff://brett@kadomony/o/riff?session=dddd4444-x", true);
        assert_eq!(idle(&[other, same.clone()], &me, &[]), 1);
        // The worktree does not change the repository.
        let worktree = info(
            "riff://brett@kadomony/o/riff?session=eeee5555-x#issue-7",
            true,
        );
        assert_eq!(idle(&[same, worktree], &me, &[]), 2);
    }

    /// A worker pane of the user that joined in another repository does
    /// not count. A pane that did not join yet counts.
    #[test]
    fn idle_skips_a_pane_of_the_user_in_another_repository() {
        let me: SessionUri = "riff://mike@pangolin/o/riff?session=l1&lead=true"
            .parse()
            .unwrap();
        let sessions = [
            info("riff://mike@pangolin/o/dotfiles?session=aaaa1111-x", true),
            info("riff://mike@pangolin/o/riff?session=bbbb2222-x", true),
        ];
        let pane = |p: &str, s: &str| WorkerPane {
            pane: p.into(),
            session: s.into(),
        };
        let panes = [
            pane("%1", "aaaa1111"),
            pane("%2", "bbbb2222"),
            pane("%3", "cccc3333-not-joined"),
        ];
        assert_eq!(idle(&sessions, &me, &panes), 2);
    }

    /// A worker on a task of the lead with no work item holds a task
    /// claim: it gets no free work, and an idle worker gets it. When only
    /// the worker with the task runs, the rollout starts a new worker for
    /// the free item (01M518P1N8R893MG9Z3VAQZR6Z).
    #[test]
    fn a_worker_with_a_task_claim_gets_no_free_work() {
        let me: SessionUri = "riff://mike@thelio/o/riff?session=l1&lead=true"
            .parse()
            .unwrap();
        let task = info(
            "riff://mike@thelio/o/riff?session=aaaa1111-x&claim=task-iroh-listener",
            true,
        );
        let free = info("riff://mike@thelio/o/riff?session=bbbb2222-x", true);
        let pane = |p: &str, s: &str| WorkerPane {
            pane: p.into(),
            session: s.into(),
        };
        let base = View {
            running: true,
            work: 1,
            places: vec![thelio()],
            items: vec!["issue-7".into()],
            ..View::default()
        };
        // The idle workers of one look, as `Live::look` finds them from
        // the sessions and the panes.
        let look = |sessions: &[SessionInfo], panes: &[WorkerPane]| View {
            idle: idle(sessions, &me, panes),
            idlers: idlers(sessions, &me),
            ..base.clone()
        };
        let wait = Duration::from_secs(60);

        // The idle worker gets the item, and no worker starts.
        let panes = [pane("%1", "aaaa1111"), pane("%2", "bbbb2222")];
        let view = look(&[task.clone(), free], &panes);
        let (requests, seen) = Offers::default().plan(&view, Instant::now(), wait);
        assert_eq!(requests, [("bbbb2222-x".to_owned(), "issue-7".to_owned())]);
        assert_eq!(decide(&seen), None);

        // Only the worker with the task runs: it gets no request, it is
        // not idle, and the rollout starts a worker.
        let view = look(std::slice::from_ref(&task), &panes[..1]);
        let (requests, seen) = Offers::default().plan(&view, Instant::now(), wait);
        assert_eq!(requests, []);
        assert_eq!(seen.idle, 0);
        assert_eq!(decide(&seen), Some(0));
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
    /// rollout starts no worker for it and sends no request for it. A
    /// hold names one item. A free makes the item free work at the next
    /// look.
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

        // Each item held: an idle worker gets no request, and no worker
        // starts.
        let me: SessionUri = "riff://mike@thelio/o/riff?session=l1&lead=true"
            .parse()
            .unwrap();
        let sessions = [info("riff://mike@thelio/o/riff?session=aaaa1111-x", true)];
        let panes = [WorkerPane {
            pane: "%1".into(),
            session: "aaaa1111".into(),
        }];
        let look = |items: Vec<String>| View {
            running: true,
            work: items.len(),
            idle: idle(&sessions, &me, &panes),
            places: vec![thelio()],
            items,
            idlers: idlers(&sessions, &me),
            ..View::default()
        };
        let wait = Duration::from_secs(60);
        let mut offers = Offers::default();
        let (requests, seen) = offers.plan(
            &look(work(&["issue-12", "verify-issue-13"])),
            Instant::now(),
            wait,
        );
        assert_eq!(requests, []);
        assert_eq!(decide(&seen), None);
        // No idle worker: still no start.
        let none = View {
            idle: 0,
            idlers: vec![],
            ..look(work(&["issue-12", "verify-issue-13"]))
        };
        assert_eq!(decide(&none), None);

        // The lead frees issue-12: the next look gives it to the worker.
        let (requests, _) = offers.plan(&look(work(&["verify-issue-13"])), Instant::now(), wait);
        assert_eq!(requests, [("aaaa1111-x".to_owned(), "issue-12".to_owned())]);
        // With no idle worker, it starts one.
        let none = View {
            idle: 0,
            idlers: vec![],
            ..look(work(&["verify-issue-13"]))
        };
        assert_eq!(decide(&none), Some(0));
    }
}
