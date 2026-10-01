//! The presence: the part of the state that is in memory.
//!
//! The presence says if a session is there: its place, its open watch
//! streams, its last call, its last sign of life, whether it ended, its
//! status and its worker mark. It also holds the read cursors, which the
//! checkpoint keeps. A start of the server loses the rest.
//!
//! No record is needed to change the presence: a keep-alive, a status
//! and a read change it. A record changes it only in
//! [`Presence::applied`] (01M3WNQRCBP0PHSA0H3THDH5NJ).

use std::collections::BTreeMap;
use std::time::Instant;

use riff_core::name::{Place, ThreadName, Who};
use riff_core::record::{Change, Record};
use riff_core::wire::Status;
use serde::{Deserialize, Serialize};

use super::riff::Riff;
use super::{CLAIM_GRACE, GONE};

/// The state in memory. See the module docs.
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
