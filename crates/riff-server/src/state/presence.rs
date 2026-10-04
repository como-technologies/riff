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
use std::time::Instant;

use riff_core::name::{Place, ThreadName, Who};
use riff_core::record::{Change, Record, Scope};
use riff_core::wire::{Activity, AliveReply, ItemFact, Kind, Status};
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
/// Signal::Alive { activity: None }.set(&mut presence, mike.who(), Instant::now());
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
/// Signal::Alive { activity: None }.set(&mut riff, mike.who(), Instant::now());
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
    /// ends a block (01M41FZPT31ATXP75QW965P3JB).
    Alive { activity: Option<Activity> },
    /// The place of the session, from a `register`. It makes a session
    /// that the presence does not know.
    Place { place: Place },
    /// The status of the session, set at `at_ms`.
    Status { status: Status, at_ms: u64 },
    /// The session cannot go on with no decision, since `at_ms`
    /// (01M41FZPGEK4TNPSM2051W4VMS). A new block replaces the old one.
    Blocked { reason: String, at_ms: u64 },
    /// What a client saw of the items of `thread` on the forge
    /// (01M41FZP2C4Z4J6WKRXZ5B31EH). With `all`, the facts replace each
    /// fact of the thread.
    Facts {
        thread: ThreadName,
        items: Vec<ItemFact>,
        all: bool,
    },
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
            Signal::Alive { activity } => {
                session.live(now);
                if let Some(activity) = activity {
                    session.worked(activity, now);
                }
            }
            Signal::Place { place } => {
                session.place = place;
                session.called(now);
            }
            Signal::Status { status, at_ms } => {
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
            Signal::AskedToStop => session.stopping = true,
            Signal::Read { .. } | Signal::Facts { .. } => {}
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
    ///   (01M41FZQCHWY1YVGAZ60ZHJK21).
    /// - `claimed`, `released` and `session_started` end the block of
    ///   the session: a change of its claims is work, and a new start
    ///   is a new context.
    pub(super) fn applied(&mut self, record: &Record, riff: &Riff, at: Option<Instant>) {
        match &record.change {
            Change::Claimed(claimed) => self.claims_changed(claimed.session.who(), at),
            Change::Released(released) => self.claims_changed(released.session.who(), at),
            Change::Posted(posted) => {
                if let (Some(at), Kind::Message) = (at, posted.message.kind) {
                    for who in &posted.woken {
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
            | Change::SigninsEnded(_) => {}
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

    /// The read cursors, for a checkpoint.
    pub(super) fn saved(&self) -> Saved {
        Saved {
            cursors: self
                .cursors
                .iter()
                .map(|((who, thread), seq)| SavedCursor {
                    session: who.clone(),
                    thread: thread.clone(),
                    seq: *seq,
                })
                .collect(),
        }
    }
}

/// The part of the checkpoint that the presence gives: the read cursors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Saved {
    #[serde(default)]
    cursors: Vec<SavedCursor>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SavedCursor {
    session: Who,
    thread: ThreadName,
    seq: u64,
}

impl Saved {
    /// The read cursors of a checkpoint.
    pub(super) fn restore(self) -> BTreeMap<(Who, ThreadName), u64> {
        self.cursors
            .into_iter()
            .map(|c| ((c.session, c.thread), c.seq))
            .collect()
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
    /// True when the server asked this idle worker to stop, and it made
    /// no call since (01M3Q5A0NKY1FCS0YH6N6YD3GN).
    pub(super) stopping: bool,
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
            stopping: false,
            claims_changed: now,
        }
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
            position: 1,
            written_at_ms: 0,
            by: None,
            command: None,
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
}
