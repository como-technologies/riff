//! The presence: the part of the state that is in memory.
//!
//! The presence says if a session is there: its place, its open watch
//! streams, its last call, its last sign of life, whether it ended and
//! its status. It also holds the read cursors, which the checkpoint
//! keeps. A start of the server loses the rest. The worker mark is not
//! here: it is in the log (01M3X9X9M079WGFPJZHNXH9VEP).
//!
//! The presence also holds what the sessions show by facts, not by a
//! record (01M41FZRF5HEZCDS515CP7DYCV): the newest fact of the hooks of
//! each session, its block, and the facts of the items that clients saw
//! on the forge. A start of the server loses them, and the clients send
//! them again.
//!
//! No record is needed to change the presence: a keep-alive, a status
//! and a read change it. Each such change is a [`Signal`]
//! (01M3WRD97EZJK3AABXECXEY133). A record changes the presence only in
//! `Presence::applied` (01M3WNQRCBP0PHSA0H3THDH5NJ).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use riff_core::name::{Place, ThreadName, Who};
use riff_core::record::{Change, Record, Scope};
use riff_core::wire::{Activity, AliveReply, ItemFact, Kind, Status, StepChange};
use serde::{Deserialize, Serialize};

use super::riff::Riff;
use super::{CLAIM_GRACE, GONE};

/// The state in memory. See the module docs.
///
/// A status and a keep-alive change only the presence. They make no
/// record:
///
/// ```
/// use std::time::Instant;
/// use riff_core::name::SessionUri;
/// use riff_core::wire::Status;
/// use riff_server::state::State;
///
/// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
/// let now = Instant::now();
/// let mut state = State::default();
/// // The first call of a session makes records: it joins its thread.
/// state.register(&mike, now);
/// assert!(!state.take_queue().is_empty());
///
/// let status = Status { step: "the tests run".into() };
/// state.set_status(&mike, status, now, 0).unwrap();
/// state.alive(&mike, now);
/// assert!(state.take_queue().is_empty());
/// assert!(state.who(now, 0, false)[0].status.is_some());
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Default)]
pub struct Presence {
    pub(super) sessions: BTreeMap<Who, Session>,
    /// The last sequence number that each session read in each thread.
    pub(super) cursors: BTreeMap<(Who, ThreadName), u64>,
    /// The last pause or resume of the riff, or the replay. A status
    /// from before it is stale (01M3Q551YHYZBFV2NDS1QCYXCD).
    pub(super) riff_changed: Option<Instant>,
    /// The last pause or resume of each repository. A status of a
    /// session of that repository from before it is stale.
    pub(super) repository_changed: BTreeMap<ThreadName, Instant>,
    /// The time of the replay. A session that did not call since then
    /// holds its claims and its lead until [`CLAIM_GRACE`] after it.
    pub(super) loaded: Option<Instant>,
    /// The facts of the items of each repository thread, by item
    /// (01M41FZP2C4Z4J6WKRXZ5B31EH).
    pub(super) items: BTreeMap<ThreadName, BTreeMap<String, ItemFact>>,
    /// The time of the last look at the plan of each repository thread
    /// (01M4A4YTYVHFGK0CJVACJQ8DQ3).
    pub(super) looks: BTreeMap<ThreadName, Instant>,
}

// ANCHOR: signal
/// A change of the presence only: it makes no record, and it cannot
/// change the riff (01M3WRD97EZJK3AABXECXEY133).
///
/// A signal takes the presence:
///
/// ```
/// use std::time::Instant;
/// use riff_core::name::SessionUri;
/// use riff_server::state::{Presence, Riff, Signal};
///
/// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
/// let (mut presence, mut riff) = (Presence::default(), Riff::default());
/// let place = Signal::Place { place: mike.place().clone() };
/// place.set(&mut presence, mike.who(), Instant::now());
/// Signal::Alive { activity: None, prompt_secs: None }.set(&mut presence, mike.who(), Instant::now());
/// # let _ = &mut riff;
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
///
/// It does not take the riff. This differs only in the value that the
/// last signal gets, and it does not compile:
///
/// ```compile_fail,E0308
/// use std::time::Instant;
/// use riff_core::name::SessionUri;
/// use riff_server::state::{Presence, Riff, Signal};
///
/// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
/// let (mut presence, mut riff) = (Presence::default(), Riff::default());
/// let place = Signal::Place { place: mike.place().clone() };
/// place.set(&mut presence, mike.who(), Instant::now());
/// Signal::Alive { activity: None, prompt_secs: None }.set(&mut riff, mike.who(), Instant::now());
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signal {
    /// A call of the session from `place`: a sign of life. It takes
    /// back an ask to stop. A person takes the place of each call
    /// (01M3MWW8KYJ3ZV91X22RBSAF33).
    Called { place: Place },
    /// A keep-alive: a sign of life that is not a call (R204). It
    /// carries the newest fact of the hooks of the session
    /// (01M41FZNTPXQNCZ1S99HE42PYQ). A fact of work after an answer
    /// ends a block (01M41FZPT31ATXP75QW965P3JB). A prompt of the
    /// person after the block, `prompt_secs` ago, ends it too
    /// (01M48VDWPDYRPEAXHR1MYDN1M7).
    Alive {
        activity: Option<Activity>,
        prompt_secs: Option<u64>,
    },
    /// The place of the session, from a `register`. It makes a session
    /// that the presence does not know.
    Place { place: Place },
    /// The status of the session, set at `at_ms`.
    Status { status: Status, at_ms: u64 },
    /// The session cannot go on with no decision, since `at_ms`
    /// (01M41FZPGEK4TNPSM2051W4VMS). A new block replaces the old one.
    Blocked { reason: String, at_ms: u64 },
    /// A change of the long step of the session at `at_ms`
    /// (01M48VDGTD40P8RBZMS0XB5M9N). It is a sign of life.
    Step { change: StepChange, at_ms: u64 },
    /// What a client saw of the items of `thread` on the forge
    /// (01M41FZP2C4Z4J6WKRXZ5B31EH). With `all`, the facts replace each
    /// fact of the thread.
    Facts {
        thread: ThreadName,
        items: Vec<ItemFact>,
        all: bool,
    },
    /// A look saw the plan of `thread` (01M4A4YTYVHFGK0CJVACJQ8DQ3): a
    /// `plan` that the server took, or a `plan_seen` of the position of
    /// the plan of the server. A plan is stale
    /// [`PLAN_TTL`](super::plan::PLAN_TTL) after the last look.
    PlanSeen { thread: ThreadName },
    /// A watch stream opened.
    WatchStarted,
    /// A watch stream closed. It is no sign of life
    /// (01M3WG240PNMQYZ7TX6Z7ZF6M9).
    WatchEnded,
    /// The session ended (R205).
    Ended,
    /// The session read `thread` up to `seq`. A read of all the
    /// messages (`all`) never moves the cursor back.
    Read {
        thread: ThreadName,
        seq: u64,
        all: bool,
    },
    /// The server asks this idle worker to stop
    /// (01M3Q5A0NKY1FCS0YH6N6YD3GN).
    AskedToStop,
}
// ANCHOR_END: signal

impl Signal {
    /// Sets the signal of the session `who` in `presence`, at `now`.
    /// Only [`Signal::Place`] makes a session that the presence does not
    /// know: each other signal of such a session changes nothing. The
    /// reply says if the server asks the session to stop. The state adds
    /// the ask to clear: it is in the riff
    /// ([`State::signal`](super::State::signal)).
    pub fn set(self, presence: &mut Presence, who: &Who, now: Instant) -> AliveReply {
        if let Signal::Read { thread, seq, all } = self {
            let cursor = presence.cursors.entry((who.clone(), thread)).or_insert(0);
            *cursor = if all { (*cursor).max(seq) } else { seq };
            return AliveReply::default();
        }
        if let Signal::Facts { thread, items, all } = self {
            let known = presence.items.entry(thread).or_default();
            if all {
                known.clear();
            }
            for fact in items {
                known.insert(fact.item.clone(), fact);
            }
            return AliveReply::default();
        }
        if let Signal::PlanSeen { thread } = self {
            presence.looks.insert(thread, now);
            return AliveReply::default();
        }
        if let Signal::Place { place, .. } = &self
            && !presence.sessions.contains_key(who)
        {
            let session = Session::new(place.clone(), now);
            presence.sessions.insert(who.clone(), session);
        }
        let Some(session) = presence.sessions.get_mut(who) else {
            return AliveReply::default();
        };
        match self {
            Signal::Called { place } => {
                if who.session().is_none() {
                    session.place = place;
                }
                session.called(now);
            }
            Signal::Alive {
                activity,
                prompt_secs,
            } => {
                session.live(now);
                if let Some(secs) = prompt_secs {
                    session.prompted(secs, now);
                }
                if let Some(activity) = activity {
                    session.worked(activity, now);
                }
            }
            Signal::Place { place } => {
                session.place = place;
                session.called(now);
            }
            Signal::Status { status, at_ms } => {
                session.live(now);
                session.status = Some(SetStatus {
                    status,
                    set_ms: at_ms,
                    set: now,
                });
            }
            Signal::Blocked { reason, at_ms } => {
                session.blocked = Some(Block {
                    reason,
                    set_ms: at_ms,
                    set: now,
                    answered: None,
                    woken_again: None,
                    unanswered: false,
                });
            }
            Signal::Step { change, at_ms } => {
                session.live(now);
                session.step = match change {
                    StepChange::Start { name } => Some(LongStep {
                        name,
                        set_ms: at_ms,
                        failed: None,
                    }),
                    StepChange::Done => None,
                    StepChange::Fail { reason } => Some(LongStep {
                        name: session
                            .step
                            .take()
                            .map_or_else(|| "a step".into(), |s| s.name),
                        set_ms: at_ms,
                        failed: Some(reason),
                    }),
                };
            }
            Signal::WatchStarted => session.watchers += 1,
            Signal::WatchEnded => {
                session.watchers = session.watchers.saturating_sub(1);
                session.stopping = false;
                if !session.gone(now) {
                    session.last_seen = now;
                    session.seen_before_load = None;
                }
            }
            Signal::Ended => {
                session.ended = true;
                session.last_seen = now;
                session.seen_before_load = None;
                session.claims_changed = now;
            }
            Signal::AskedToStop => {
                if session.asked().is_none() {
                    session.asked = Some(now);
                    session.told_stuck = false;
                }
                session.stopping = true;
            }
            Signal::Read { .. } | Signal::Facts { .. } | Signal::PlanSeen { .. } => {}
        }
        AliveReply {
            stop: session.stopping,
            clear: false,
        }
    }
}

/// What the import of go-live gives to the presence
/// (01M3Z8MRDZEKTXSKZTDTDSCZ3W): the memory of each session of the old
/// server, and the read cursors. The log holds none of them. See
/// [`State::imported`](super::State::imported).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Imported {
    pub sessions: Vec<ImportedSession>,
    /// The last seq that a session read in a thread.
    pub cursors: Vec<(Who, ThreadName, u64)>,
}

/// The memory of one session of the old server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedSession {
    pub who: Who,
    /// The last call, in milliseconds since the Unix epoch.
    pub seen_ms: u64,
    /// True when the session ended.
    pub ended: bool,
    /// The last status, with the time of its set in milliseconds since
    /// the Unix epoch.
    pub status: Option<(Status, u64)>,
}

impl Presence {
    /// Makes the session `imported` in `place`, as it is after a replay
    /// at `loaded`: gone until it calls. Its status is from before
    /// `loaded`, so it is stale. A session that the presence knows
    /// stays as it is: it called after the import.
    pub(super) fn imported(&mut self, imported: ImportedSession, place: Place, loaded: Instant) {
        let ImportedSession {
            who,
            seen_ms,
            ended,
            status,
        } = imported;
        self.sessions.entry(who).or_insert_with(|| Session {
            seen_before_load: Some(seen_ms),
            alive: None,
            ended,
            status: status.map(|(status, set_ms)| SetStatus {
                status,
                set_ms,
                set: loaded,
            }),
            ..Session::new(place, loaded)
        });
    }

    /// True when the presence knows the session `who`.
    pub fn knows(&self, who: &Who) -> bool {
        self.sessions.contains_key(who)
    }

    /// What a record changes in memory. The caller applies the record to
    /// `riff` first. `at` is the time of the call that made the record.
    /// A replay gives `None`, and sets no time: it makes each session
    /// after its last record.
    ///
    /// - `claimed` and `released`: the time of the last change of the
    ///   claims of the session (01M3Q551WCMPQRCNJ8FXQEBFY4). A session
    ///   whose item another session takes has a `released` record of
    ///   its own, before the `claimed` record (01M3X4Z6BKM251H7CS2CEGR205).
    /// - `pause_set`: the time for a
    ///   stale status (01M3Q551YHYZBFV2NDS1QCYXCD), for the riff or for
    ///   the repository of the record.
    /// - `session_forgotten`: the session leaves memory, with its read
    ///   cursors, and each cursor of a thread that is gone.
    /// - `posted`: a message, not a note and not a status request, that
    ///   wakes a blocked session answers its block
    ///   (01M41FZPT31ATXP75QW965P3JB). The block is then not unanswered
    ///   (01M41FZQCHWY1YVGAZ60ZHJK21). A lead waits for its person: a
    ///   message does not answer it (01M48VDSB4CHQS9P6XVDJ6FMKS).
    /// - `claimed`, `released` and `session_started` end the block of
    ///   the session: a change of its claims is work, and a new start
    ///   is a new context.
    pub(super) fn applied(&mut self, record: &Record, riff: &Riff, at: Option<Instant>) {
        match &record.change {
            Change::Claimed(claimed) => self.claims_changed(claimed.session.who(), at),
            Change::Released(released) => self.claims_changed(released.session.who(), at),
            Change::Posted(posted) => {
                if let (Some(at), Kind::Message) = (at, posted.message.kind) {
                    let leads = &riff.work().leads;
                    for who in &posted.woken {
                        if leads.values().any(|lead| lead == who) {
                            continue;
                        }
                        let session = self.sessions.get_mut(who);
                        if let Some(block) = session.and_then(|s| s.blocked.as_mut()) {
                            block.answered.get_or_insert(at);
                            block.unanswered = false;
                        }
                    }
                }
            }
            Change::SessionStarted(started) => {
                if let Some(session) = self.sessions.get_mut(started.session.who()) {
                    session.blocked = None;
                }
            }
            Change::PauseSet(set) => match (&set.scope, at) {
                (Scope::Riff, _) => self.riff_changed = at.or(self.riff_changed),
                (Scope::Repository(thread), Some(at)) => {
                    self.repository_changed.insert(thread.clone(), at);
                }
                (Scope::Repository(_), None) | (Scope::Other, _) => {}
            },
            Change::SessionForgotten(forgotten) => {
                let who = forgotten.session.who();
                self.sessions.remove(who);
                self.cursors
                    .retain(|(reader, thread), _| reader != who && riff.threads().has(thread));
            }
            Change::JoinedThread(_)
            | Change::LeftThread(_)
            | Change::LeadSet(_)
            | Change::SettingChanged(_)
            | Change::RiffMade(_)
            | Change::PersonJoined(_)
            | Change::MemberInvited(_)
            | Change::MemberRemoved(_)
            | Change::AdminSet(_)
            | Change::OwnerSet(_)
            | Change::OwnerAsked(_)
            | Change::OwnerDenied(_)
            | Change::SigninsEnded(_)
            | Change::ItemHeld(_)
            | Change::ItemFreed(_)
            | Change::PlanSet(_)
            | Change::PlanEnded(_) => {}
        }
    }

    fn claims_changed(&mut self, who: &Who, at: Option<Instant>) {
        if let (Some(session), Some(at)) = (self.sessions.get_mut(who), at) {
            session.claims_changed = at;
            session.blocked = None;
        }
    }

    /// The fact of `item` in `thread`, when a client sent one.
    pub(super) fn item(&self, thread: &ThreadName, item: &str) -> Option<&ItemFact> {
        self.items.get(thread)?.get(item)
    }

    /// The look of the lead `lead` at the blocks of the sessions of its
    /// user in `thread` (01M41FZQ545HQ9Q75CSKX8HF8H,
    /// 01M41FZQCHWY1YVGAZ60ZHJK21). A block with no answer for `after`
    /// gets a second wake of the lead: it is in the first list. A block
    /// with no answer for `after` after the second wake is unanswered:
    /// it is in the second list, one time. A session that is gone gets
    /// no look.
    pub(super) fn look_blocks(
        &mut self,
        lead: &Who,
        thread: &ThreadName,
        after: std::time::Duration,
        now: Instant,
    ) -> (Vec<Found>, Vec<Found>) {
        let (mut again, mut unanswered) = (Vec::new(), Vec::new());
        for (who, session) in &mut self.sessions {
            if who.user() != lead.user()
                || who == lead
                || session.place.default_thread().as_ref() != Some(thread)
                || session.gone(now)
            {
                continue;
            }
            let Some(block) = session.blocked.as_mut().filter(|b| b.answered.is_none()) else {
                continue;
            };
            let since = |t: Instant| now.saturating_duration_since(t) >= after;
            match block.woken_again {
                None if since(block.set) => {
                    block.woken_again = Some(now);
                    again.push((who.clone(), block.reason.clone()));
                }
                Some(woken) if !block.unanswered && since(woken) => {
                    block.unanswered = true;
                    unanswered.push((who.clone(), block.reason.clone()));
                }
                _ => {}
            }
        }
        (again, unanswered)
    }

    /// The last sequence number that `who` read in `thread`.
    pub(super) fn cursor(&self, who: &Who, thread: &ThreadName) -> u64 {
        self.cursors
            .get(&(who.clone(), thread.clone()))
            .copied()
            .unwrap_or(0)
    }

    /// The read cursors, and the status and the long step of each
    /// session, for a checkpoint.
    pub(super) fn saved(&self) -> Saved {
        // The rest is not saved: a load starts it again.
        let Presence {
            sessions,
            cursors,
            riff_changed: _,
            repository_changed: _,
            loaded: _,
            items: _,
            looks: _,
        } = self;
        Saved {
            cursors: cursors
                .iter()
                .map(|((session, thread), seq)| SavedCursor {
                    session: session.clone(),
                    thread: thread.clone(),
                    seq: *seq,
                })
                .collect(),
            statuses: sessions
                .iter()
                .filter_map(|(who, session)| {
                    let signals = Signals::of(session.status.as_ref(), session.step.as_ref())?;
                    Some(SavedSession {
                        session: who.clone(),
                        signals,
                    })
                })
                .collect(),
        }
    }
}

/// The part of the checkpoint that the presence gives: the read cursors,
/// and the status and the long step of each session
/// (01M4263ZZVY8QJ2METTEVR1W26, 01M49NP8F3A9CTJWZ74MCNZG0M).
///
/// One entry of `statuses` keeps both signals of a session. A checkpoint
/// of 1.1.0 has no `step`, and loads:
///
/// ```json
/// {"statuses": [{"session": {"user": "ann", "session": "a1"},
///                "status": {"step": "tests"}, "set_ms": 1000,
///                "step": {"name": "live window", "set_ms": 900}}]}
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub(super) struct Saved {
    #[serde(default)]
    cursors: Vec<SavedCursor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    statuses: Vec<SavedSession>,
}

/// What the checkpoint keeps of the signals of one session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
struct SavedSession {
    session: Who,
    #[serde(flatten)]
    signals: Signals,
}

/// The status of a session, its long step, or both. An entry with
/// neither cannot be (01M49W18QQKF4KYDRYP3ZK9F1Q): it does not load.
/// Each variant has the JSON of 1.1.0: the fields of the status in the
/// entry, and the step in `step`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
enum Signals {
    Both {
        #[serde(flatten)]
        status: SavedStatus,
        step: LongStep,
    },
    Status(SavedStatus),
    Step {
        step: LongStep,
    },
}

impl Signals {
    /// The signals of a session to save. `None` when it has neither.
    fn of(status: Option<&SetStatus>, step: Option<&LongStep>) -> Option<Signals> {
        let status = status.map(|set| {
            let SetStatus {
                status,
                set_ms,
                set: _,
            } = set;
            SavedStatus {
                status: status.clone(),
                set_ms: *set_ms,
            }
        });
        match (status, step.cloned()) {
            (Some(status), Some(step)) => Some(Signals::Both { status, step }),
            (Some(status), None) => Some(Signals::Status(status)),
            (None, Some(step)) => Some(Signals::Step { step }),
            (None, None) => None,
        }
    }

    /// The status and the step.
    fn into_parts(self) -> (Option<SavedStatus>, Option<LongStep>) {
        match self {
            Signals::Both { status, step } => (Some(status), Some(step)),
            Signals::Status(status) => (Some(status), None),
            Signals::Step { step } => (None, Some(step)),
        }
    }
}

/// A status with the time of its set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
struct SavedStatus {
    status: Status,
    /// The time of the set, in milliseconds since the Unix epoch.
    set_ms: u64,
}

/// A read cursor: the map entry of [`Presence::cursors`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
struct SavedCursor {
    session: Who,
    thread: ThreadName,
    seq: u64,
}

impl Saved {
    /// Puts the read cursors, the statuses and the long steps of a
    /// checkpoint in `presence`, at the load `now`. A status and a step
    /// go only to a session that the presence knows. A status keeps its
    /// age, at least 1 ms: it is from before the load, so it is stale. A
    /// step keeps the time of its start or its failure, so its age goes
    /// on from the first start.
    pub(super) fn restore(self, presence: &mut Presence, now: Instant, now_ms: u64) {
        let Saved { cursors, statuses } = self;
        presence.cursors = cursors
            .into_iter()
            .map(
                |SavedCursor {
                     session,
                     thread,
                     seq,
                 }| ((session, thread), seq),
            )
            .collect();
        for SavedSession {
            session: who,
            signals,
        } in statuses
        {
            let Some(session) = presence.sessions.get_mut(&who) else {
                continue;
            };
            let (status, step) = signals.into_parts();
            if let Some(SavedStatus { status, set_ms }) = status {
                let age = Duration::from_millis(now_ms.saturating_sub(set_ms).max(1));
                session.status = Some(SetStatus {
                    status,
                    set_ms,
                    set: now.checked_sub(age).unwrap_or(now),
                });
            }
            session.step = step;
        }
    }
}

pub(super) struct Session {
    pub(super) place: Place,
    /// The number of open watch streams.
    pub(super) watchers: usize,
    /// The last call.
    pub(super) last_seen: Instant,
    /// The time of the last record that named the session before the
    /// replay, in milliseconds since the Unix epoch. `None` when the
    /// session called after the replay.
    pub(super) seen_before_load: Option<u64>,
    /// The last sign of life: a call or a keep-alive. The start of a
    /// watch is a call. `None` when the session did not show life since
    /// the replay.
    pub(super) alive: Option<Instant>,
    /// True after an end call, until the session comes back.
    pub(super) ended: bool,
    pub(super) status: Option<SetStatus>,
    /// The newest fact of the hooks, and its time
    /// (01M41FZNTPXQNCZ1S99HE42PYQ).
    pub(super) work: Option<(Activity, Instant)>,
    /// The block of the session, while it holds
    /// (01M41FZPGEK4TNPSM2051W4VMS).
    pub(super) blocked: Option<Block>,
    /// The long step of the session, until it is done
    /// (01M48VDGTD40P8RBZMS0XB5M9N).
    pub(super) step: Option<LongStep>,
    /// True when the server asked this idle worker to stop, and it made
    /// no call since (01M3Q5A0NKY1FCS0YH6N6YD3GN).
    pub(super) stopping: bool,
    /// The first ask to stop. A change of the claims after it ends it
    /// ([`Session::asked`], 01M4385Z039RCFSKWFPWZAETTX).
    pub(super) asked: Option<Instant>,
    /// True when the server told the lead that this worker still runs
    /// after the ask ([`Session::asked`]).
    pub(super) told_stuck: bool,
    /// The last change of the claims of the session, or its arrival
    /// (01M3Q551WCMPQRCNJ8FXQEBFY4).
    pub(super) claims_changed: Instant,
}

/// A status with the time that the session set it.
#[derive(Clone)]
pub(super) struct SetStatus {
    pub(super) status: Status,
    /// Milliseconds since the Unix epoch.
    pub(super) set_ms: u64,
    /// The time of the set.
    pub(super) set: Instant,
}

/// A long step of a session (01M48VDGTD40P8RBZMS0XB5M9N). The checkpoint
/// keeps it as it is (01M49NP8F3A9CTJWZ74MCNZG0M).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub(super) struct LongStep {
    pub(super) name: String,
    /// The start of the step, or its failure, in milliseconds since the
    /// Unix epoch.
    pub(super) set_ms: u64,
    /// The reason, when the step failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) failed: Option<String>,
}

/// A blocked session that a look found, with its reason.
pub(super) type Found = (Who, String);

/// A block: the session cannot go on with no decision
/// (01M41FZPGEK4TNPSM2051W4VMS).
#[derive(Clone)]
pub(super) struct Block {
    pub(super) reason: String,
    /// Milliseconds since the Unix epoch.
    pub(super) set_ms: u64,
    pub(super) set: Instant,
    /// The first message that woke the session after the block.
    pub(super) answered: Option<Instant>,
    /// The second wake of the lead (01M41FZQ545HQ9Q75CSKX8HF8H).
    pub(super) woken_again: Option<Instant>,
    /// True when the lead gave no answer after the second wake
    /// (01M41FZQCHWY1YVGAZ60ZHJK21).
    pub(super) unanswered: bool,
}

impl SetStatus {
    /// True when the session set the status before `changed`.
    pub(super) fn before(&self, changed: Option<Instant>) -> bool {
        changed.is_some_and(|changed| self.set < changed)
    }
}

impl Session {
    /// A new session that calls `now` from `place`.
    pub(super) fn new(place: Place, now: Instant) -> Self {
        Session {
            place,
            watchers: 0,
            last_seen: now,
            seen_before_load: None,
            alive: Some(now),
            ended: false,
            status: None,
            work: None,
            blocked: None,
            step: None,
            stopping: false,
            asked: None,
            told_stuck: false,
            claims_changed: now,
        }
    }

    /// The first ask to stop since the last change of the claims. A
    /// wake takes the ask back, but not this time: the server tells the
    /// lead one time for each idle time of a worker
    /// (01M4385Z039RCFSKWFPWZAETTX).
    pub(super) fn asked(&self) -> Option<Instant> {
        self.asked.filter(|asked| *asked >= self.claims_changed)
    }

    /// Records a sign of life at `now`. A gone session comes back.
    pub(super) fn live(&mut self, now: Instant) {
        self.alive = Some(now);
        self.ended = false;
    }

    /// Keeps the newest fact of the hooks, which a keep-alive at `now`
    /// carries. A fact of work after an answer ends the block
    /// (01M41FZPT31ATXP75QW965P3JB).
    fn worked(&mut self, activity: Activity, now: Instant) {
        let at = now
            .checked_sub(std::time::Duration::from_secs(activity.secs))
            .unwrap_or(now);
        let answered = self.blocked.as_ref().and_then(|b| b.answered);
        if activity.works() && answered.is_some_and(|answered| at >= answered) {
            self.blocked = None;
        }
        self.work = Some((activity, at));
    }

    /// Ends the block when the person gave a prompt at or after it,
    /// `secs` before `now` (01M48VDWPDYRPEAXHR1MYDN1M7).
    fn prompted(&mut self, secs: u64, now: Instant) {
        let at = now
            .checked_sub(std::time::Duration::from_secs(secs))
            .unwrap_or(now);
        if self.blocked.as_ref().is_some_and(|b| at >= b.set) {
            self.blocked = None;
        }
    }

    /// Records a call at `now`: a sign of life that also takes back an
    /// ask to stop.
    fn called(&mut self, now: Instant) {
        self.last_seen = now;
        self.seen_before_load = None;
        self.stopping = false;
        self.live(now);
    }

    /// True when the session ended, or had no sign of life for [`GONE`].
    /// An open watch stream is no sign of life: a front end can hold the
    /// stream of a dead client open (01M3WG240PNMQYZ7TX6Z7ZF6M9).
    pub(super) fn gone(&self, now: Instant) -> bool {
        self.ended
            || self
                .alive
                .is_none_or(|alive| now.saturating_duration_since(alive) >= GONE)
    }

    /// True while the session has an open watch stream and is not gone.
    pub(super) fn watching(&self, now: Instant) -> bool {
        self.watchers > 0 && !self.gone(now)
    }

    /// True while the claims and the lead of the session hold. A session
    /// with no sign of life since the replay at `loaded` holds until
    /// [`CLAIM_GRACE`] after it.
    pub(super) fn holds(&self, now: Instant, loaded: Option<Instant>) -> bool {
        let within = |since: Instant| now.saturating_duration_since(since) < CLAIM_GRACE;
        !self.ended
            && match self.alive {
                Some(alive) => within(alive),
                None => loaded.is_some_and(within),
            }
    }

    /// The last time that the session called, in milliseconds since the
    /// Unix epoch. A live session calls now.
    pub(super) fn seen_ms(&self, now: Instant, now_ms: u64) -> u64 {
        if self.watching(now) {
            return now_ms;
        }
        self.seen_before_load.unwrap_or_else(|| {
            let ago = now.saturating_duration_since(self.last_seen).as_millis();
            now_ms.saturating_sub(u64::try_from(ago).unwrap_or(u64::MAX))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use riff_core::name::SessionUri;
    use riff_core::record::{Claimed, Forgotten, Member, PauseSet, Released};
    use riff_core::wire::RiffState;

    use super::super::riff::apply;
    use super::*;
    use riff_core::record::Envelope;

    fn ann() -> SessionUri {
        "riff://ann@heron/acme/app?session=a1".parse().unwrap()
    }

    fn bob() -> SessionUri {
        "riff://bob@kite/acme/app?session=b1".parse().unwrap()
    }

    fn repo() -> ThreadName {
        "acme/app".parse().unwrap()
    }

    fn design() -> ThreadName {
        "design".parse().unwrap()
    }

    fn claim_of(me: &SessionUri) -> Claimed {
        Claimed {
            session: me.clone(),
            thread: repo(),
            item: "issue-7".into(),
        }
    }

    fn joined(me: &SessionUri, thread: &ThreadName) -> Change {
        Change::JoinedThread(Member {
            session: me.clone(),
            thread: thread.clone(),
        })
    }

    fn record(change: Change) -> Record {
        Record {
            envelope: Envelope {
                position: 1,
                written_at_ms: 0,
                by: None,
                command: None,
                call: None,
            },
            change,
        }
    }

    /// Ann and bob in memory since `since`.
    fn presence(since: Instant) -> Presence {
        let mut presence = Presence::default();
        for me in [ann(), bob()] {
            let session = Session::new(me.place().clone(), since);
            presence.sessions.insert(me.who().clone(), session);
        }
        presence
    }

    fn claims_changed(presence: &Presence, me: &SessionUri) -> Instant {
        presence.sessions[me.who()].claims_changed
    }

    #[test]
    fn a_claimed_record_sets_only_the_time_of_its_session() {
        let since = Instant::now();
        let at = since + Duration::from_secs(9);
        let mut presence = presence(since);
        let record = record(Change::Claimed(claim_of(&bob())));
        presence.applied(&record, &Riff::default(), Some(at));
        assert_eq!(claims_changed(&presence, &bob()), at);
        assert_eq!(claims_changed(&presence, &ann()), since);
    }

    #[test]
    fn a_released_record_sets_the_time_of_its_session() {
        let since = Instant::now();
        let at = since + Duration::from_secs(9);
        let mut presence = presence(since);
        let record = record(Change::Released(Released::of(claim_of(&ann()))));
        presence.applied(&record, &Riff::default(), Some(at));
        assert_eq!(claims_changed(&presence, &ann()), at);
        assert_eq!(claims_changed(&presence, &bob()), since);
    }

    #[test]
    fn a_pause_set_record_sets_the_time_for_its_scope() {
        let since = Instant::now();
        let at = since + Duration::from_secs(9);
        let repo: ThreadName = "acme/app".parse().unwrap();
        let set = |scope| {
            record(Change::PauseSet(PauseSet {
                scope,
                state: RiffState::Paused,
            }))
        };
        let mut presence = presence(since);
        let of_repo = set(Scope::Repository(repo.clone()));
        presence.applied(&of_repo, &Riff::default(), Some(at));
        assert_eq!(presence.repository_changed.get(&repo), Some(&at));
        assert_eq!(presence.riff_changed, None);
        presence.applied(&set(Scope::Other), &Riff::default(), Some(at));
        assert_eq!(presence.riff_changed, None);
        presence.applied(&set(Scope::Riff), &Riff::default(), Some(at));
        assert_eq!(presence.riff_changed, Some(at));
        // A replay sets no time.
        let other: ThreadName = "acme/lib".parse().unwrap();
        let replayed = set(Scope::Repository(other.clone()));
        presence.applied(&replayed, &Riff::default(), None);
        assert_eq!(presence.repository_changed.get(&other), None);
    }

    #[test]
    fn a_record_of_a_replay_sets_no_time() {
        let since = Instant::now();
        let mut presence = presence(since);
        presence.riff_changed = Some(since);
        let changes = [
            Change::Claimed(claim_of(&bob())),
            Change::Released(Released::of(claim_of(&bob()))),
            Change::PauseSet(PauseSet {
                scope: Scope::Riff,
                state: RiffState::Running,
            }),
        ];
        for change in changes {
            presence.applied(&record(change), &Riff::default(), None);
        }
        assert_eq!(claims_changed(&presence, &ann()), since);
        assert_eq!(claims_changed(&presence, &bob()), since);
        assert_eq!(presence.riff_changed, Some(since));
    }

    #[test]
    fn a_session_forgotten_record_drops_the_session_its_cursors_and_the_cursors_of_a_thread_that_is_gone()
     {
        let now = Instant::now();
        let mut presence = presence(now);
        for (me, thread) in [(ann(), repo()), (bob(), repo()), (bob(), design())] {
            presence.cursors.insert((me.who().clone(), thread), 3);
        }
        // The riff after the record has the repository thread, and no
        // thread `design`.
        let mut riff = Riff::default();
        apply(&mut riff, &record(joined(&bob(), &repo())));
        let forgotten = record(Change::SessionForgotten(Forgotten { session: ann() }));
        presence.applied(&forgotten, &riff, Some(now));
        assert!(!presence.sessions.contains_key(ann().who()));
        assert!(presence.sessions.contains_key(bob().who()));
        let cursors: Vec<_> = presence.cursors.keys().cloned().collect();
        assert_eq!(cursors, [(bob().who().clone(), repo())]);
        assert_eq!(presence.cursor(bob().who(), &repo()), 3);
        assert_eq!(presence.cursor(ann().who(), &repo()), 0);
    }

    #[test]
    fn a_record_of_a_thread_changes_nothing_in_memory() {
        let since = Instant::now();
        let at = since + Duration::from_secs(9);
        let mut presence = presence(since);
        presence.cursors.insert((ann().who().clone(), repo()), 3);
        presence.applied(
            &record(joined(&ann(), &design())),
            &Riff::default(),
            Some(at),
        );
        assert_eq!(presence.sessions.len(), 2);
        assert_eq!(claims_changed(&presence, &ann()), since);
        assert_eq!(presence.riff_changed, None);
        assert_eq!(presence.cursor(ann().who(), &repo()), 3);
    }

    fn step(name: &str, set_ms: u64, failed: Option<&str>) -> LongStep {
        LongStep {
            name: name.into(),
            set_ms,
            failed: failed.map(Into::into),
        }
    }

    /// A load of a checkpoint gives each session its status and its long
    /// step again, a failed step with its reason
    /// (01M49NP8F3A9CTJWZ74MCNZG0M).
    #[test]
    fn a_checkpoint_keeps_the_status_and_the_step_of_each_session() {
        let since = Instant::now();
        let mut before = presence(since);
        let ann_session = before.sessions.get_mut(ann().who()).unwrap();
        ann_session.status = Some(SetStatus {
            status: Status {
                step: "tests".into(),
            },
            set_ms: 1_000,
            set: since,
        });
        ann_session.step = Some(step("live window", 900, None));
        let bob_session = before.sessions.get_mut(bob().who()).unwrap();
        bob_session.step = Some(step("deploy", 2_000, Some("the stage gave 502")));

        let json = serde_json::to_string(&before.saved()).unwrap();
        let saved: Saved = serde_json::from_str(&json).unwrap();
        assert_eq!(saved, before.saved());
        let mut after = presence(since);
        saved.restore(&mut after, since + Duration::from_secs(60), 61_000);

        let ann_after = &after.sessions[ann().who()];
        assert_eq!(ann_after.step, Some(step("live window", 900, None)));
        let status = ann_after.status.as_ref().unwrap();
        assert_eq!(
            (status.status.step.as_str(), status.set_ms),
            ("tests", 1_000)
        );
        let bob_after = &after.sessions[bob().who()];
        let failed = step("deploy", 2_000, Some("the stage gave 502"));
        assert_eq!(bob_after.step, Some(failed));
        assert!(bob_after.status.is_none());
    }

    /// A checkpoint of 1.1.0 has statuses with no step, and loads.
    #[test]
    fn a_checkpoint_of_1_1_0_with_no_step_loads() {
        let json = r#"{"cursors":[],"statuses":[{"session":{"user":"ann","session":"a1"},"status":{"step":"tests"},"set_ms":1000}]}"#;
        let saved: Saved = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_string(&saved).unwrap(), json);
        let since = Instant::now();
        let mut presence = presence(since);
        saved.restore(&mut presence, since, 2_000);
        let ann_after = &presence.sessions[ann().who()];
        assert_eq!(ann_after.status.as_ref().unwrap().status.step, "tests");
        assert!(ann_after.step.is_none());
    }

    /// Each variant of the signals of a saved session reads back with
    /// its JSON. An entry with neither a status nor a step cannot be
    /// (01M49W18QQKF4KYDRYP3ZK9F1Q): it does not load.
    #[test]
    fn each_variant_of_a_saved_session_reads_back_and_an_empty_one_does_not_load() {
        let session = r#""session":{"user":"ann","session":"a1""#;
        let status = r#""status":{"step":"tests"},"set_ms":1000"#;
        let step = r#""step":{"name":"deploy","set_ms":900,"failed":"502"}"#;
        for (json, variant) in [
            (format!("{{{session}}},{status},{step}}}"), "both"),
            (format!("{{{session}}},{status}}}"), "status"),
            (format!("{{{session}}},{step}}}"), "step"),
        ] {
            let saved: SavedSession = serde_json::from_str(&json).unwrap();
            let read = match &saved.signals {
                Signals::Both { .. } => "both",
                Signals::Status(_) => "status",
                Signals::Step { .. } => "step",
            };
            assert_eq!(read, variant, "{json}");
            assert_eq!(serde_json::to_string(&saved).unwrap(), json);
        }
        let empty = format!("{{{session}}}}}");
        assert!(serde_json::from_str::<SavedSession>(&empty).is_err());
        assert_eq!(Signals::of(None, None), None);
    }
}
