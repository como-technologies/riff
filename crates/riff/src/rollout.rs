//! riff starts workers by itself when the wave has free work.
//!
//! # Design
//!
//! `riff mcp` of the lead runs the rollout (01M3Q5QE01DB0FJQJWFKR450KQ).
//! So the start of work does not wait for an agent that remembers a
//! step. Once each interval ([`crate::settings::workers_interval`],
//! 10 seconds by default, 01M3Q5QE9H42FQKEDC5G9GKCWD), it looks at the
//! riff, gives free work to the idle workers, and starts at most one
//! worker:
//!
//! ```mermaid
//! flowchart TD
//!     T["each interval"] --> L{"this session is the lead?"}
//!     L -- no --> T
//!     L -- yes --> R{"the riff runs?"}
//!     R -- "no: paused" --> T
//!     R -- yes --> P{"a machine with room,<br/>or an idle worker that joined?"}
//!     P -- no --> T
//!     P -- yes --> W["free work (gh): free items of the current wave,<br/>pull requests that wait for a verify"]
//!     W --> O["each idle worker with no open request:<br/>tell it request: claim ITEM"]
//!     O --> I{"free work, and no idle worker?<br/>(a worker that refused does not count)"}
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
//!   another user with no claim. A worker that the server asked to stop
//!   is not idle. A worker in another repository is not idle: it cannot
//!   take the free work (01M3W27BJYFQCHY5MTZ2J4SKW4). riff starts a worker only when no
//!   worker is idle (01M3Q5QEJNP1JGQM7VXXEBJ9J9). So a new worker must
//!   claim before the next one starts. When no worker takes the counted work, one
//!   worker waits idle, the server keeps it (#259), and riff starts no
//!   more: no loop of starts and stops.
//! - **Requests** ([`Offers`], 01M49ZK19GQP79Z8HH14PK85QQ to
//!   01M49ZK1EG52YAKG974XH201RK). An idle worker that joined sleeps on
//!   its watch: only a wake makes it read. So the rollout gives it free
//!   work with a request of the lead, as the lead does by hand. A
//!   worker that does not claim in [`offer_wait`] refused the item: it
//!   does not block a start. An item that two workers refused starts no
//!   more workers.
//! - **Machines** ([`Place`], [`pick`], 01M3Q5QE76BZ27SZ14FFE8HM1G).
//!   The machine of the lead, when the lead runs in tmux, and each live
//!   workers host of the user. A machine has room when its workers are
//!   fewer than its limit, it is not busy
//!   ([`crate::machine::Machine::busy`]), and its available memory is
//!   not less than its floor ([`crate::machine::Machine::low`],
//!   01M3WFZ01PTAYYKG3T5CFA2W4D). riff picks the machine with the
//!   most free capacity. On a tie, the machine of the lead wins.
//! - **Pause** (01M3Q5QEBTNM90SPYXNVTT7RJA). A pause stops the rollout
//!   within one look. A look that started before the pause can start
//!   one more worker. After it, riff starts no worker while the riff is
//!   paused.
//! - **Notes** (01M3Q5QEE4MQNCRKVJK3D54G9Z). On the machine of the lead,
//!   riff posts a note to the lead with the host, the pane and the
//!   session. A workers host posts the same note when it starts a
//!   worker.
//!
//! - **Where riff is off** (01M3XY2T542DCHBN95H9PX4AGQ). A worker
//!   starts in the main clone. When riff is off there, the machine of
//!   the lead is no place for a worker: riff starts none there, and the
//!   lead gets one note with the reason and `riff enable`
//!   (01M3YCGKKRDNFC338K1JSK30JK). A workers host can still take the
//!   work.
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
//!     E -- "a higher limit gives room, and the rollout is on" --> N1["note: the rollout starts 1 worker"]
//!     E -- "a higher limit gives room, and the rollout is off" --> W["message that wakes the lead:<br/>riff workers start N"]
//!     E -- "more workers run than the new limit" --> N2["note: the workers over the limit end after their item"]
//!     E -- "nothing" --> N3["note: the change"]
//! ```
//!
//! [`effect`] finds what the change does (01M3X30R4PSBP3RQWM02BJ6GK3). The message is a note
//! when the lead has nothing to do. It wakes the lead only when work
//! waits that the change lets start and the rollout is off (01M3X30RA3X08JBJ2JBVCCNEH3).
//! One look uses each limit one time: the view of the look takes the
//! limits that the look read for the changes ([`View::with_limits`]).
//! So a limit that changes in the middle of a look starts no worker
//! before the next look, and the note comes before the start
//! (01M3XFHSYJEN9V6QEKWJGJWQ8Q). A
//! workers host sets its status at once when its limit changes, and
//! posts the note for a change of its own MCP servers
//! ([`crate::host`], 01M3X30RJS8YE5TXJBQDC2FT0C).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use riff_core::name::SessionUri;
use riff_core::selector::Selector;
use riff_core::wire::{Idle, Kind, RiffState, SessionInfo};
use serde::Deserialize;

use crate::api::Api;
use crate::disk::Disk;
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

/// The machine for one new worker, or `None` when riff starts none:
/// the riff is paused, there is no free work, a worker is idle
/// (01M3Q5QEJNP1JGQM7VXXEBJ9J9), or no machine has room.
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
/// a time. A worker in the worktree `issue-N` gets no request for
/// `verify-issue-N`: it can be the author. A request is open until its
/// worker claims or goes, or its item is no longer free. A worker that
/// does not claim in [`offer_wait`] refused the item. It never gets that
/// request again. It does not block the start of a new worker, when it
/// has no other item to take. An item that two workers refused is no
/// work for a start: so no loop of starts.
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
/// // count as idle, so the rollout can start a worker.
/// let (requests, seen) = offers.plan(&view, t0 + wait, wait);
/// assert_eq!((requests, seen.idle, seen.work), (vec![], 0, 1));
/// ```
#[derive(Debug, Default)]
pub struct Offers {
    /// Each open request: the worker, the item and the time of the
    /// request.
    open: Vec<(String, String, Instant)>,
    /// The workers that refused each item.
    refused: BTreeMap<String, BTreeSet<String>>,
}

impl Offers {
    /// The new requests of this look `view` at `now`, as pairs of
    /// worker and item, and the view for [`decide`]: an idle worker
    /// with no request and no item to take is not idle, and an item
    /// that two workers refused is no work.
    pub fn plan(
        &mut self,
        view: &View,
        now: Instant,
        wait: Duration,
    ) -> (Vec<(String, String)>, View) {
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
                    && item
                        .strip_prefix("verify-")
                        .is_none_or(|own| idler.worktree.as_deref() != Some(own))
            });
            match item {
                Some(item) => {
                    self.open.push((idler.session.clone(), item.clone(), now));
                    requests.push((idler.session.clone(), item.clone()));
                }
                None => spent += 1,
            }
        }
        let dead = self.refused.values().filter(|r| r.len() >= 2).count();
        let view = View {
            idle: view.idle.saturating_sub(spent),
            work: view.work.saturating_sub(dead),
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
    /// The rollout starts one worker at this look because of the change.
    Starts,
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
/// A higher limit starts a worker when the rollout starts none with the
/// old limit and one with the new limit. A lower limit stops no worker.
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
/// assert_eq!(effect(&raise, &view, true), Effect::Starts);
/// assert_eq!(effect(&raise, &view, false), Effect::Waits { count: 1, remote: true });
/// // No free work: the change starts nothing.
/// assert_eq!(effect(&raise, &View { work: 0, ..view.clone() }, true), Effect::Nothing);
/// // The machine had room before the change.
/// let more = Change::Limit { host: "pangolin".into(), old: 4, new: 5 };
/// let mut wide = view.clone();
/// wide.places[0].limit = 5;
/// assert_eq!(effect(&more, &wide, true), Effect::Nothing);
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
    if decide(&before).is_some() || decide(view).is_none() {
        return Effect::Nothing;
    }
    if on {
        return Effect::Starts;
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
    /// Starts one worker on `place`.
    fn start(&self, place: &Place) -> impl Future<Output = Result<()>> + Send;
    /// Gives `body` to the lead: a message that wakes it when `wake` is
    /// true, else a note.
    fn tell(&self, body: &str, wake: bool) -> impl Future<Output = Result<()>> + Send;
    /// Gives `item` to the idle worker `worker` with a request of the
    /// lead (01M49ZK19GQP79Z8HH14PK85QQ).
    fn offer(&self, worker: &str, item: &str) -> impl Future<Output = Result<()>> + Send;
}

/// Runs the rollout until the task ends. It waits one interval, looks,
/// and starts at most one worker, again and again. So riff starts at
/// most one worker each interval. It prints an error once, not again
/// until the error changes.
///
/// At each look it tells the lead each change of a worker setting
/// (01M3X30KHKB6W11C3NBAW7KCGW). A rollout that is off looks at the settings each
/// [`OFF_WAIT`], and starts no worker.
///
/// One look reads each limit one time, tells the changes, and then
/// starts the worker ([`View::with_limits`], 01M3XFHSYJEN9V6QEKWJGJWQ8Q).
/// So the note of a higher limit comes before the start that it names.
///
/// At each look of a running riff, it gives free work to the idle
/// workers first ([`Offers`]), and then decides on a start.
pub async fn run(env: impl Env) {
    let mut last_error = None;
    let mut known: Option<Seen> = None;
    let mut offers = Offers::default();
    loop {
        let every = env.interval();
        tokio::time::sleep(if every.is_zero() { OFF_WAIT } else { every }).await;
        let step = async {
            let Some(seen) = env.seen().await? else {
                known = None;
                return anyhow::Ok(());
            };
            let on = seen.interval > 0;
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
            if let Some(i) = decide(&view) {
                env.start(&view.places[i]).await?;
                eprintln!(
                    "riff: the rollout started a worker on {}",
                    view.places[i].host
                );
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
/// repository, the `claims` of all sessions and the open `pulls`: an
/// open issue of the wave that no session claims, with no comment
/// `Merged in #`, and with each issue of its `Needs:` line closed. An
/// open need blocks the item, also a need outside the wave, and also a
/// need that is merged but not closed.
///
/// An item whose pull request waits for a verify or for the merge is
/// not free: it is work for a verify, not for a build
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
/// ```
pub fn free_items(
    open: &[Issue],
    wave: &str,
    claims: &HashSet<String>,
    pulls: &[Pull],
) -> Vec<u64> {
    let numbers: HashSet<u64> = open.iter().map(|i| i.number).collect();
    let in_verify = in_verify(pulls);
    open.iter()
        .filter(|i| i.milestone.as_ref().is_some_and(|m| m.title == wave))
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
/// ];
/// let mut issues: Vec<u64> = in_verify(&pulls).into_iter().collect();
/// issues.sort_unstable();
/// assert_eq!(issues, [12, 13]);
/// ```
pub fn in_verify(pulls: &[Pull]) -> HashSet<u64> {
    pulls
        .iter()
        .filter(|p| matches!(p.verify(), Some(Verify::Asked | Verify::Passed)))
        .filter_map(|p| branch_issue(&p.branch))
        .collect()
}

/// The fields of `gh pr list --json` that [`Pull`] reads.
pub const PULL_FIELDS: &str =
    "number,headRefName,headRefOid,isDraft,statusCheckRollup,mergeable,autoMergeRequest";

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
}

/// True for a value that is not `null`.
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Option::<serde::de::IgnoredAny>::deserialize(d)?.is_some())
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
    /// The state of the verify of this pull request. `None` for a
    /// draft, and for a branch that names no issue: a pull request
    /// that a person opened by hand is no work.
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
        branch_issue(&self.branch)?;
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
/// ([`Verify::Asked`]): the branch names an issue, not a draft, no status `riff/verify` on the
/// head, and no session claims `verify-issue-N` for the issue of the
/// branch. A pull request that a person opened by hand names no issue,
/// so it is no work.
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
/// ];
/// let claims: HashSet<String> = ["verify-issue-14".to_owned()].into();
/// assert_eq!(waiting_verifies(&pulls, &claims), [12]);
/// ```
pub fn waiting_verifies(pulls: &[Pull], claims: &HashSet<String>) -> Vec<u64> {
    pulls
        .iter()
        .filter(|p| p.verify() == Some(Verify::Asked))
        .filter_map(|p| branch_issue(&p.branch))
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
        .filter(|p| branch_issue(&p.branch) == Some(issue))
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
///     stopping: false,
///     claims_secs: 0,
///     must_clear: false,
///     fresh_secs: None,
///     state: None,
///     work: None,
///     waits: None,
///     blocked: None,
///     step: None,
/// };
/// let me = "riff://mike@pangolin/o/riff?session=l1&lead=true".parse().unwrap();
/// let strata = worker("riff://brett@kadomony/o/strata?session=w1");
/// assert_eq!(idle(&[strata], &me, &[]), 0);
/// let riff = worker("riff://brett@kadomony/o/riff?session=w2");
/// assert_eq!(idle(&[riff], &me, &[]), 1);
/// ```
pub fn idle(sessions: &[SessionInfo], me: &SessionUri, panes: &[WorkerPane]) -> usize {
    let (user, repo) = (me.who().user(), me.place().repo());
    // A worker that holds a claim is busy. A worker that the server
    // asked to stop goes away. A worker in another repository cannot
    // take the free work.
    let away =
        |s: &SessionInfo| !s.uri.claims().is_empty() || s.stopping || s.uri.place().repo() != repo;
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
/// no clear of the context to wait for.
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
///     stopping: false,
///     claims_secs: 0,
///     must_clear: false,
///     fresh_secs: None,
///     state: None,
///     work: None,
///     waits: None,
///     blocked: None,
///     step: None,
/// };
/// let me = "riff://mike@pangolin/o/riff?session=l1&lead=true".parse().unwrap();
/// let sessions = [
///     worker("riff://mike@pangolin/o/riff?session=w1#issue-12"),
///     worker("riff://mike@pangolin/o/riff?session=w2&claim=issue-7"),
///     worker("riff://mike@pangolin/o/strata?session=w3"),
///     worker("riff://brett@kadomony/o/riff?session=w4"),
///     SessionInfo { must_clear: true, ..worker("riff://mike@pangolin/o/riff?session=w5") },
/// ];
/// assert_eq!(idlers(&sessions, &me), [Idler { session: "w1".into(), worktree: Some("issue-12".into()) }]);
/// ```
pub fn idlers(sessions: &[SessionInfo], me: &SessionUri) -> Vec<Idler> {
    sessions
        .iter()
        .filter(|s| s.worker && s.live && !s.stopping && !s.must_clear)
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

/// The free work of the repository `repo` (`OWNER/REPO`), with `gh`,
/// by name: the pull requests that wait for a verify
/// (`verify-issue-N`), then the free items of the current wave
/// (`issue-N`). An item counts one time: as a build or as a verify
/// (01M3Z9N5HHHS1E17NFGMVBKZ0K).
pub fn free_work(gh: &Gh, repo: &str, claims: &HashSet<String>) -> Result<Vec<String>> {
    let open: Vec<Milestone> = gh.json(&[
        "api",
        &format!("repos/{repo}/milestones?state=open&per_page=100"),
    ])?;
    let pulls = pulls(gh, repo)?;
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
            free_items(&issues, wave, claims, &pulls)
        }
        None => Vec::new(),
    };
    let verifies = waiting_verifies(&pulls, claims);
    Ok(verifies
        .iter()
        .map(|n| format!("verify-issue-{n}"))
        .chain(items.iter().map(|n| format!("issue-{n}")))
        .collect())
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
    /// True when the lead has the note that riff is off in the main
    /// clone of its machine (01M3YCGKKRDNFC338K1JSK30JK).
    pub off_told: std::sync::atomic::AtomicBool,
}

impl<M: Fn() -> SessionUri + Send + Sync> Live<M> {
    /// True when riff is on where a worker of this machine starts: the
    /// main clone of the lead. When it is off, a worker there is a plain
    /// session, so the machine of the lead is no place for a worker
    /// (01M3XY2T542DCHBN95H9PX4AGQ), and the lead gets one note with
    /// the reason (01M3YCGKKRDNFC338K1JSK30JK). The next note comes
    /// only after riff was on again.
    async fn on_here(&self, me: &SessionUri) -> Result<bool> {
        use std::sync::atomic::Ordering;
        let Some(why) = worker::off(&identity::working_dir()?) else {
            self.off_told.store(false, Ordering::Relaxed);
            return Ok(true);
        };
        if !self.off_told.swap(true, Ordering::Relaxed) {
            let note = text::rollout_off(me.place().host(), &why);
            if let Err(e) = note_lead(self.api.base(), me, &note).await {
                self.off_told.store(false, Ordering::Relaxed);
                return Err(e);
            }
        }
        Ok(false)
    }
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
        if let Some(tmux) = &self.tmux
            && limit > 0
            && self.on_here(&me).await?
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
                disk: identity::main_worktree(&identity::working_dir()?)
                    .and_then(|main| Disk::here(&main)),
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
        // With no room and no idle worker to give work to, the free
        // work changes nothing: no call of gh.
        if !places.iter().any(Place::room) && idlers.is_empty() {
            return Ok(Some(View {
                running: true,
                idle,
                places,
                ..View::default()
            }));
        }
        let claims = claims(&sessions);
        let (gh, repo) = (self.gh.clone(), me.place().repo_text());
        let items = tokio::task::spawn_blocking(move || free_work(&gh, &repo, &claims)).await??;
        Ok(Some(View {
            running: true,
            work: items.len(),
            idle,
            places,
            items,
            idlers,
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
        /// Each message to the lead, with true when it wakes the lead.
        told: Vec<(String, bool)>,
        /// A person sets this limit on the first machine in the middle
        /// of the next look: after its `seen`, before its `look`.
        limit_in_look: Option<u16>,
        /// What the rollout did, in order: `told` and `start`.
        order: Vec<&'static str>,
        /// The free work by name, for the requests.
        items: Vec<String>,
        /// The idle workers that joined. Each one is also in `idle`.
        idlers: Vec<Idler>,
        /// Each request: the worker and the item, with its time.
        offers: Vec<(String, String, tokio::time::Instant)>,
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
            self.with(|w| {
                w.told.push((body.to_owned(), wake));
                w.order.push("told");
            });
            Ok(())
        }

        async fn look(&self) -> Result<Option<View>> {
            Ok(self.with(|w| {
                w.advance();
                w.lead.then(|| View {
                    running: w.running,
                    work: w.work,
                    idle: w.idle + w.new.len(),
                    places: w.places.clone(),
                    items: w.items.clone(),
                    idlers: w.idlers.clone(),
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

        async fn start(&self, place: &Place) -> Result<()> {
            self.with(|w| {
                let p = w.places.iter_mut().find(|p| p.host == place.host).unwrap();
                p.workers += 1;
                let now = tokio::time::Instant::now();
                w.new.push((place.host.clone(), now));
                w.starts.push((place.host.clone(), now));
                w.order.push("start");
            });
            Ok(())
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
            claim_after: Some(Duration::from_secs(5)),
            keep_idle: None,
            new: Vec::new(),
            starts: Vec::new(),
            stops: 0,
            told: Vec::new(),
            limit_in_look: None,
            order: Vec::new(),
            items: Vec::new(),
            idlers: Vec::new(),
            offers: Vec::new(),
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
            claim_after: None,
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

    #[tokio::test(start_paused = true)]
    async fn an_idle_worker_gets_a_request_for_a_waiting_verify() {
        let fake = Fake::new(one_idle("verify-issue-12"));
        let begin = tokio::time::Instant::now();
        run_for(&fake, 15).await;
        assert_eq!(
            offers(&fake, begin),
            [("w1".into(), "verify-issue-12".into(), 10)]
        );
        assert!(fake.hosts().is_empty(), "the idle worker has the work");
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

    /// The worker does not claim in 6 intervals: it does not block the
    /// start of a new worker.
    #[tokio::test(start_paused = true)]
    async fn a_worker_that_does_not_claim_does_not_block_a_start() {
        let fake = Fake::new(one_idle("issue-7"));
        let begin = tokio::time::Instant::now();
        run_for(&fake, 75).await;
        let secs: Vec<u64> = fake.with(|w| {
            w.starts
                .iter()
                .map(|(_, at)| (*at - begin).as_secs())
                .collect()
        });
        assert_eq!(secs, [70]);
    }

    /// The new worker joins, gets the request and also does not claim.
    /// Two workers refused the item: no third worker starts.
    #[tokio::test(start_paused = true)]
    async fn an_item_that_two_workers_refuse_starts_no_loop() {
        let fake = Fake::new(one_idle("issue-7"));
        let begin = tokio::time::Instant::now();
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(Duration::from_secs(75)).await;
        fake.with(|w| {
            // The new worker joins.
            w.new.clear();
            w.idle += 1;
            w.idlers.push(idler("w2"));
        });
        tokio::time::sleep(Duration::from_secs(600)).await;
        task.abort();
        assert_eq!(fake.hosts().len(), 1);
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

    /// A higher limit on a machine that was full, with free work: the
    /// lead gets a note that the rollout starts a worker, and the
    /// rollout starts it (01M3X30KHKB6W11C3NBAW7KCGW, 01M3X30R4PSBP3RQWM02BJ6GK3).
    #[tokio::test(start_paused = true)]
    async fn a_higher_limit_tells_the_lead_that_the_rollout_starts_a_worker() {
        let mut w = world(3);
        w.places = vec![Place {
            limit: 1,
            ..pangolin()
        }];
        let fake = Fake::new(w);
        let task = tokio::spawn(run(fake.clone()));
        tokio::time::sleep(Duration::from_secs(35)).await;
        assert_eq!(fake.hosts(), ["pangolin"]);
        assert!(fake.with(|w| w.told.is_empty()), "no change, no message");

        fake.with(|w| w.places[0].limit = 2);
        tokio::time::sleep(Duration::from_secs(10)).await;
        task.abort();
        assert_eq!(fake.hosts(), ["pangolin", "pangolin"]);
        assert_eq!(
            fake.with(|w| w.told.clone()),
            [(
                "workers: limit 1 to 2 on pangolin: the rollout starts 1 worker.".to_owned(),
                false
            )]
        );
    }

    /// A person sets a higher limit in the middle of a look: after the
    /// look read the limits, before it reads the machines. That look
    /// starts no worker with the new limit. The next look tells the
    /// lead that the rollout starts a worker, and then starts it
    /// (01M3XFHSYJEN9V6QEKWJGJWQ8Q).
    #[tokio::test(start_paused = true)]
    async fn a_limit_that_changes_in_a_look_gives_the_note_before_the_start() {
        let mut w = world(3);
        w.places = vec![Place {
            limit: 1,
            ..pangolin()
        }];
        let fake = Fake::new(w);
        let task = tokio::spawn(run(fake.clone()));
        // The looks at 10, 20 and 30 s: one worker, and it claimed.
        tokio::time::sleep(Duration::from_secs(35)).await;
        assert_eq!(fake.with(|w| w.order.clone()), ["start"]);

        // The look at 40 s reads the limit 1. Then the limit is 2.
        fake.with(|w| w.limit_in_look = Some(2));
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(fake.with(|w| w.places[0].limit), 2);
        assert_eq!(
            fake.with(|w| w.order.clone()),
            ["start"],
            "the look with the old limit starts no worker"
        );

        // The look at 50 s.
        tokio::time::sleep(Duration::from_secs(10)).await;
        task.abort();
        assert_eq!(fake.with(|w| w.order.clone()), ["start", "told", "start"]);
        assert_eq!(
            fake.with(|w| w.told.clone()),
            [(
                "workers: limit 1 to 2 on pangolin: the rollout starts 1 worker.".to_owned(),
                false
            )]
        );
    }

    /// With the rollout off, the same change wakes the lead: work waits,
    /// and only the lead can start the worker (01M3X30RA3X08JBJ2JBVCCNEH3).
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
            fake.hosts().is_empty(),
            "a rollout that is off starts nothing"
        );
        assert_eq!(
            fake.with(|w| w.told.clone()),
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
            fake.with(|w| w.told.clone()),
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
        assert!(fake.with(|w| w.told.is_empty()));
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

    /// 01M41A11DX1QRP48YPTDNT67W4: under 5% of free disk, the rollout
    /// starts no worker on the machine. It picks the other one.
    #[tokio::test(start_paused = true)]
    async fn the_rollout_starts_no_worker_on_a_machine_with_a_low_disk() {
        let mut w = world(1);
        w.places[0].disk = Some(Disk {
            free_gb: 4,
            total_gb: 100,
        });
        w.places[1].disk = Some(Disk {
            free_gb: 5,
            total_gb: 100,
        });
        let fake = Fake::new(w);
        run_for(&fake, 30).await;
        assert_eq!(fake.hosts(), ["pangolin"]);
        assert!(!fake.with(|w| w.places[0].room()));
    }

    #[tokio::test(start_paused = true)]
    async fn the_rollout_starts_no_worker_on_a_machine_under_its_floor() {
        let mut w = world(4);
        w.places[0].machine = w.places[0].machine.map(|m| Machine { avail_gb: 1, ..m });
        w.places[1].machine = w.places[1].machine.map(|m| Machine { avail_gb: 2, ..m });
        let fake = Fake::new(w);
        run_for(&fake, 60).await;
        assert!(fake.hosts().is_empty(), "{:?}", fake.hosts());
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
            must_clear: false,
            fresh_secs: None,
            state: Some(riff_core::wire::SessionState::Idle),
            work: None,
            waits: None,
            blocked: None,
            step: None,
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
    #[test]
    fn an_item_with_an_open_pull_request_and_no_claim_counts_one_time() {
        let issues: Vec<Issue> = serde_json::from_str(
            r#"[{"number":12,"body":"","comments":[],"milestone":{"title":"Wave 3"}}]"#,
        )
        .unwrap();
        let pulls = |status: &str| -> Vec<Pull> {
            serde_json::from_str(&format!(
                r#"[{{"number":40,"headRefName":"worktree-issue-12","headRefOid":"1a2b3c4d","isDraft":false,
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
        assert_eq!(work(&asked, &[]), (vec![], vec![12]));
        // A verifier holds it: no work.
        assert_eq!(work(&asked, &["verify-issue-12"]), (vec![], vec![]));
        // Passed: the merge waits, no work.
        assert_eq!(work(&pulls(&status("SUCCESS")), &[]), (vec![], vec![]));
        // Failed: the item is free for a build, and no verify waits.
        let failed = pulls(&status("FAILURE"));
        assert_eq!(work(&failed, &[]), (vec![12], vec![]));
        assert_eq!(
            failed[0].verify_status().unwrap().url.as_deref(),
            Some("https://c")
        );
        // The next session holds it: no work.
        assert_eq!(work(&failed, &["issue-12"]), (vec![], vec![]));
    }
}
