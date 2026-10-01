//! The presence: the part of the state that is in memory.
//!
//! The presence says if a session is there: its place, its open watch
//! streams, its last call, its last sign of life, whether it ended, its
//! status and its worker mark. It also holds the read cursors, which the
//! checkpoint keeps. A start of the server loses the rest.
//!
//! No record is needed to change the presence: a keep-alive, a status
//! and a read change it. A record changes it only in
//! `Presence::applied` (01M3WNQRCBP0PHSA0H3THDH5NJ).

use std::collections::BTreeMap;
use std::time::Instant;

use riff_core::name::{Place, ThreadName, Who};
use riff_core::record::{Change, Record};
use riff_core::wire::Status;
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
/// let status = Status { step: "the tests run".into(), blocked: None };
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
    /// The time of the replay. A session that did not call since then
    /// holds its claims and its lead until [`CLAIM_GRACE`] after it.
    pub(super) loaded: Option<Instant>,
}

impl Presence {
    /// What a record changes in memory. The caller applies the record to
    /// `riff` first. `at` is the time of the call that made the record.
    /// A replay gives `None`, and sets no time: it makes each session
    /// after its last record.
    ///
    /// - `claimed` and `released`: the time of the last change of the
    ///   claims of the session (01M3Q551WCMPQRCNJ8FXQEBFY4). `lost` is
    ///   the session that held the item of a `claimed` record before
    ///   it: its claims change too.
    /// - `riff_state_set`: the time for a stale status
    ///   (01M3Q551YHYZBFV2NDS1QCYXCD).
    /// - `session_forgotten`: the session leaves memory, with its read
    ///   cursors, and each cursor of a thread that is gone.
    pub(super) fn applied(
        &mut self,
        record: &Record,
        riff: &Riff,
        lost: Option<&Who>,
        at: Option<Instant>,
    ) {
        match &record.change {
            Change::Claimed(claimed) => {
                for holder in lost.into_iter().chain([claimed.session.who()]) {
                    self.claims_changed(holder, at);
                }
            }
            Change::Released(claimed) => self.claims_changed(claimed.session.who(), at),
            Change::RiffStateSet(_) => self.riff_changed = at.or(self.riff_changed),
            Change::SessionForgotten(forgotten) => {
                let who = forgotten.session.who();
                self.sessions.remove(who);
                self.cursors
                    .retain(|(reader, thread), _| reader != who && riff.threads().has(thread));
            }
            Change::Posted(_)
            | Change::JoinedThread(_)
            | Change::LeftThread(_)
            | Change::LeadSet(_)
            | Change::SettingChanged(_) => {}
        }
    }

    fn claims_changed(&mut self, who: &Who, at: Option<Instant>) {
        if let (Some(session), Some(at)) = (self.sessions.get_mut(who), at) {
            session.claims_changed = at;
        }
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
    /// True when the session registered as a worker
    /// (01M3NT4M159EHN5W8JRTQ417N4).
    pub(super) worker: bool,
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
            worker: false,
            stopping: false,
            claims_changed: now,
        }
    }

    /// Records a sign of life at `now`. A gone session comes back.
    pub(super) fn live(&mut self, now: Instant) {
        self.alive = Some(now);
        self.ended = false;
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
    use riff_core::record::{Claimed, Forgotten, Member, RiffStateSet};
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
    fn a_claimed_record_sets_the_time_of_the_new_holder_and_of_the_old_one() {
        let since = Instant::now();
        let at = since + Duration::from_secs(9);
        let mut presence = presence(since);
        let record = record(Change::Claimed(claim_of(&bob())));
        presence.applied(&record, &Riff::default(), Some(ann().who()), Some(at));
        assert_eq!(claims_changed(&presence, &bob()), at);
        assert_eq!(claims_changed(&presence, &ann()), at);
    }

    #[test]
    fn a_claimed_record_with_no_old_holder_sets_only_the_time_of_its_session() {
        let since = Instant::now();
        let at = since + Duration::from_secs(9);
        let mut presence = presence(since);
        let record = record(Change::Claimed(claim_of(&bob())));
        presence.applied(&record, &Riff::default(), None, Some(at));
        assert_eq!(claims_changed(&presence, &bob()), at);
        assert_eq!(claims_changed(&presence, &ann()), since);
    }

    #[test]
    fn a_released_record_sets_the_time_of_its_session() {
        let since = Instant::now();
        let at = since + Duration::from_secs(9);
        let mut presence = presence(since);
        let record = record(Change::Released(claim_of(&ann())));
        presence.applied(&record, &Riff::default(), None, Some(at));
        assert_eq!(claims_changed(&presence, &ann()), at);
        assert_eq!(claims_changed(&presence, &bob()), since);
    }

    #[test]
    fn a_riff_state_set_record_sets_the_time_for_a_stale_status() {
        let since = Instant::now();
        let at = since + Duration::from_secs(9);
        let mut presence = presence(since);
        let record = record(Change::RiffStateSet(RiffStateSet {
            state: RiffState::Running,
        }));
        presence.applied(&record, &Riff::default(), None, Some(at));
        assert_eq!(presence.riff_changed, Some(at));
    }

    #[test]
    fn a_record_of_a_replay_sets_no_time() {
        let since = Instant::now();
        let mut presence = presence(since);
        presence.riff_changed = Some(since);
        let changes = [
            Change::Claimed(claim_of(&bob())),
            Change::Released(claim_of(&bob())),
            Change::RiffStateSet(RiffStateSet {
                state: RiffState::Running,
            }),
        ];
        for change in changes {
            presence.applied(&record(change), &Riff::default(), Some(ann().who()), None);
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
        presence.applied(&forgotten, &riff, None, Some(now));
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
            None,
            Some(at),
        );
        assert_eq!(presence.sessions.len(), 2);
        assert_eq!(claims_changed(&presence, &ann()), since);
        assert_eq!(presence.riff_changed, None);
        assert_eq!(presence.cursor(ann().who(), &repo()), 3);
    }
}
