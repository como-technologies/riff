//! The group "sessions": the commands [`Register`], [`Start`] and
//! [`End`], and the sessions that the log names. The wire type of each
//! command is its command type. [`Arrive`] is the `register` that the
//! engine runs first for a caller that the state does not know.
//!
//! - Part of the riff: [`Sessions`]. Each session that a record names,
//!   with its URI, the time of the last record that names it, and its
//!   life cycle: the worker mark, the MustClear mark and the time of
//!   its last fresh start. A replay makes a session in the presence for
//!   each.
//! - `apply`: `Sessions::named` for each record, `Sessions::started`
//!   for a `session_started` record, `Sessions::released` for a
//!   `released` record, and `Sessions::forgotten` for a
//!   `session_forgotten` record.
//! - Checkpoint: `Saved`, the field `sessions`.
//!
//! The session in memory (its place, its signs of life) is in
//! [`super::presence`].
//!
//! # The life cycle of a session
//!
//! Records move the life cycle, so a replay gives the same state.
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> Ready: the first record that names the session
//!     Ready --> Working: claimed
//!     Working --> Working: claimed, or released with a claim left
//!     Working --> Ready: the last claim goes, and not by a release of a worker
//!     Working --> MustClear: a worker releases its last claim
//!     MustClear --> Ready: session_started (process, clear)
//!     Ready --> [*]: session_forgotten
//!     MustClear --> [*]: session_forgotten
//!     Working --> [*]: released for each claim, then session_forgotten
//! ```
//!
//! - `handle` decides, and the record holds the decision
//!   (01M3X9XAK1KPZZVM1AJR2H8DSS). The `released` record of the last
//!   claim of a worker, made by its own `release`, has `must_clear`.
//!   `apply` only stores it. A `session_started` record with the reason
//!   `process` or `clear` ends it.
//! - A `start` makes one `released` record for each claim of the
//!   session, then a `session_started` record with its reason and its
//!   worker mark (01M3X9X9M079WGFPJZHNXH9VEP). So the worker mark is in
//!   the log.
//! - A `register` makes a `session_started` record with the reason
//!   `join` when the riff does not know the session, or when the worker
//!   mark of the call is not the mark of the riff. [`Arrive`] keeps the
//!   mark of the riff: a session that the riff does not know is not a
//!   worker.
//! - Only a [`Register`] or a [`Start`] makes the first lead of a user
//!   in a repository, and never for a worker
//!   (01M3X9XA3H6YF0QCYSNB2P0CT2). [`Arrive`] makes no lead.
//! - A person has no session ID and no life cycle.

use std::collections::BTreeMap;

use riff_core::name::{SessionUri, Who};
use riff_core::record::{By, Change, Member, Record, Released, SessionStarted};
use riff_core::wire::{End, Freed, Register, Start, StartReason, Started};
use serde::{Deserialize, Serialize};

use super::command::{Caller, Command, CommandKind, Done, Now, Refused};
use super::presence::Signal;
use super::view::View;

/// Each session that a record names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sessions {
    pub(super) known: BTreeMap<Who, Known>,
}

/// A session that a record names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Known {
    /// The URI of the last record that names the session.
    pub(super) uri: SessionUri,
    /// The time of that record, in milliseconds since the Unix epoch.
    pub(super) at_ms: u64,
    /// True when the session is a worker: the mark of its last
    /// `session_started` record.
    pub(super) worker: bool,
    /// True when the session is a worker that released its last claim,
    /// and did not start with a fresh context after it.
    pub(super) must_clear: bool,
    /// The time of the last `session_started` record with a fresh
    /// context, in milliseconds since the Unix epoch.
    pub(super) fresh_ms: Option<u64>,
}

/// The session that a change names, if any.
fn named(change: &Change) -> Option<&SessionUri> {
    match change {
        Change::Posted(posted) => Some(&posted.message.from),
        Change::JoinedThread(m) | Change::LeftThread(m) | Change::LeadSet(m) => Some(&m.session),
        Change::Claimed(c) => Some(&c.session),
        Change::Released(r) => Some(&r.session),
        Change::SessionStarted(s) => Some(&s.session),
        Change::SettingChanged(_)
        | Change::SessionForgotten(_)
        | Change::PauseSet(_)
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
        | Change::ItemFreed(_) => None,
    }
}

/// The session that `record` names and the time of the record, when the
/// session made it by its own call: a sign that it lived then. A record
/// of the server, of a person or of another session only names it, for
/// example a record of the import of go-live, or a release by the lead
/// (01M4263ZXH4K23CSY6C5GJPVQH). A record with no caller counts: its
/// cause is not known.
pub(super) fn own_call(record: &Record) -> Option<(&Who, u64)> {
    let who = named(&record.change)?.who();
    let own = match &record.envelope.by {
        Some(By::Session(by)) => by == who,
        Some(_) => false,
        None => true,
    };
    own.then_some((who, record.envelope.written_at_ms))
}

impl Sessions {
    /// Keeps the URI of the session that `record` names, and the time
    /// of the record. The server is not a session.
    pub(super) fn named(&mut self, record: &Record) {
        if let Some(uri) = named(&record.change)
            && uri.who() != crate::owner::server_uri().who()
        {
            let uri = SessionUri::new(uri.who().clone(), uri.place().clone());
            let at_ms = record.envelope.written_at_ms;
            self.known
                .entry(uri.who().clone())
                .and_modify(|known| {
                    known.uri = uri.clone();
                    known.at_ms = at_ms;
                })
                .or_insert(Known {
                    uri,
                    at_ms,
                    worker: false,
                    must_clear: false,
                    fresh_ms: None,
                });
        }
    }

    /// Stores the worker mark of a `session_started` record at `at_ms`.
    /// A fresh start also stores its time, and ends MustClear. A record
    /// with a reason that this build does not know changes nothing.
    pub(super) fn started(
        &mut self,
        started: &SessionStarted,
        at_ms: u64,
    ) -> Result<(), &'static str> {
        if started.reason == StartReason::Other {
            return Err("this build does not know the reason of the start");
        }
        let known = self
            .known
            .get_mut(started.session.who())
            .ok_or("the session is not known")?;
        known.worker = started.worker;
        if started.reason.is_fresh() {
            known.fresh_ms = Some(at_ms);
            known.must_clear = false;
        }
        Ok(())
    }

    /// Stores the MustClear mark of a `released` record that has it.
    pub(super) fn released(&mut self, released: &Released) {
        if released.must_clear
            && let Some(known) = self.known.get_mut(released.session.who())
        {
            known.must_clear = true;
        }
    }

    /// Drops a forgotten session. False when the session is not known.
    pub(super) fn forgotten(&mut self, who: &Who) -> bool {
        self.known.remove(who).is_some()
    }

    /// True when a record names the session `who`.
    pub(super) fn knows(&self, who: &Who) -> bool {
        self.known.contains_key(who)
    }

    /// True when the session `who` is a worker.
    pub(super) fn worker(&self, who: &Who) -> bool {
        self.known.get(who).is_some_and(|known| known.worker)
    }

    /// True when the session `who` must clear its context before its
    /// next claim.
    pub(super) fn must_clear(&self, who: &Who) -> bool {
        self.known.get(who).is_some_and(|known| known.must_clear)
    }

    /// The time of the last fresh start of the session `who`.
    pub(super) fn fresh_ms(&self, who: &Who) -> Option<u64> {
        self.known.get(who).and_then(|known| known.fresh_ms)
    }

    /// The sessions, for a checkpoint. `seen` gives the last call of a
    /// session before the checkpoint, or 0.
    pub(super) fn saved(&self, seen: impl Fn(&Who) -> u64) -> Saved {
        Saved {
            sessions: self
                .known
                .values()
                .map(|known| SavedSession {
                    session: known.uri.clone(),
                    at_ms: known.at_ms,
                    seen_ms: seen(known.uri.who()),
                    worker: known.worker,
                    must_clear: known.must_clear,
                    fresh_ms: known.fresh_ms,
                })
                .collect(),
        }
    }
}

/// The part of the checkpoint of this group: each session that the log
/// names, with its life cycle (01M3X9XD8QWHS2CXTFSQK0PN1Y).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Saved {
    #[serde(default)]
    sessions: Vec<SavedSession>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SavedSession {
    /// The URI of the last record that names the session.
    session: SessionUri,
    /// The time of that record.
    at_ms: u64,
    /// The last call of the session before the checkpoint, or 0.
    #[serde(default)]
    seen_ms: u64,
    /// The worker mark.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    worker: bool,
    /// The MustClear mark.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    must_clear: bool,
    /// The time of the last fresh start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fresh_ms: Option<u64>,
}

impl Saved {
    /// The sessions of a checkpoint, and the last call of each.
    pub(super) fn restore(self) -> (Sessions, BTreeMap<Who, u64>) {
        let mut sessions = Sessions::default();
        let mut seen = BTreeMap::new();
        for s in self.sessions {
            let who = s.session.who().clone();
            seen.insert(who.clone(), s.seen_ms);
            sessions.known.insert(
                who,
                Known {
                    uri: s.session,
                    at_ms: s.at_ms,
                    worker: s.worker,
                    must_clear: s.must_clear,
                    fresh_ms: s.fresh_ms,
                },
            );
        }
        (sessions, seen)
    }
}

impl View<'_> {
    /// The changes of a session that comes to the place of `me` with no
    /// new start: the rule of [`Register`] and of [`Arrive`].
    ///
    /// - It joins the thread of its repository.
    /// - A session gets a `session_started` record with the reason
    ///   `join` when the riff does not know it, or when its worker mark
    ///   changes (01M3X9X9M079WGFPJZHNXH9VEP). `worker` is the mark of
    ///   the call. `None` keeps the mark of the riff, and makes no lead.
    /// - With a mark that is false, it becomes the lead when it is the
    ///   first session of its user in the repository
    ///   (01M3X9XA3H6YF0QCYSNB2P0CT2).
    fn comes(&self, me: &SessionUri, worker: Option<bool>, now: Now) -> Vec<Change> {
        let (who, place) = (me.who(), me.place());
        let sessions = self.riff.sessions();
        let old = sessions.worker(who);
        let mark = worker.unwrap_or(old);
        let plain = || SessionUri::new(who.clone(), place.clone());
        let thread = place.default_thread();
        let mut changes = Vec::new();
        if let Some(thread) = &thread
            && !self.riff.threads().member(who, thread)
        {
            changes.push(Change::JoinedThread(Member {
                session: plain(),
                thread: thread.clone(),
            }));
        }
        if who.session().is_some() && (!sessions.knows(who) || mark != old) {
            changes.push(Change::SessionStarted(SessionStarted {
                session: plain(),
                reason: StartReason::Join,
                worker: mark,
            }));
        }
        if worker == Some(false)
            && let Some(thread) = &thread
        {
            changes.extend(self.lead_if_first(who, place, thread, now.at));
        }
        changes
    }
}

/// A session registers: it says where it works, and if it is a worker.
/// It joins the thread of its repository, and becomes the lead when it
/// is the first and not a worker. Its signal sets the place
/// (01M3WRD97EZJK3AABXECXEY133). The worker mark goes to the log: see
/// the module docs.
impl Command for Register {
    const KIND: CommandKind = CommandKind::Register;
    type Reply = ();
    type Note = ();

    fn handle(
        &self,
        _caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        Ok((view.comes(&self.me, Some(self.worker), now), ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &Done, (): (), _: Now) {}

    fn signal(&self, _caller: &Caller) -> Option<Signal> {
        Some(Signal::Place {
            place: self.me.place().clone(),
        })
    }
}

/// The `register` that the engine runs first for a call of a caller
/// that the state does not know. Its kind is `register`. It keeps the
/// worker mark of the riff, and it makes no lead: only a [`Register`]
/// or a [`Start`] that the session sends makes the first lead
/// (01M3X9XA3H6YF0QCYSNB2P0CT2).
///
/// ```
/// use std::time::Instant;
/// use riff_core::name::SessionUri;
/// use riff_core::record::Change;
/// use riff_server::state::{Arrive, Caller, State};
///
/// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
/// let now = Instant::now();
/// let mut state = State::default();
/// let (made, ()) = state.run(&Caller::of(&mike), &Arrive { me: mike.clone() }, now).unwrap();
/// assert!(matches!(made[0].change, Change::JoinedThread(_)));
/// assert!(matches!(made[1].change, Change::SessionStarted(_)));
/// assert_eq!(made.len(), 2);
/// assert_eq!(made[1].command.as_deref(), Some("register"));
/// assert!(!state.uri(mike.who(), now).lead());
///
/// // A register of the session makes it the lead.
/// state.register(&mike, now);
/// assert!(state.uri(mike.who(), now).lead());
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug)]
pub struct Arrive {
    pub me: SessionUri,
}

impl Command for Arrive {
    const KIND: CommandKind = CommandKind::Register;
    type Reply = ();
    type Note = ();

    fn handle(
        &self,
        _caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        Ok((view.comes(&self.me, None, now), ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &Done, (): (), _: Now) {}

    fn signal(&self, _caller: &Caller) -> Option<Signal> {
        Some(Signal::Place {
            place: self.me.place().clone(),
        })
    }
}

/// The claims that the records of a start freed.
fn freed(made: &[Record]) -> Vec<Freed> {
    made.iter()
        .filter_map(|record| match &record.change {
            Change::Released(released) => Some(Freed {
                thread: released.thread.clone(),
                item: released.item.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// A new start of the session: each of its claims is free. The reply
/// names them. It makes one `released` record for each claim, then the
/// `session_started` record with the reason and the worker mark of the
/// call (01M3X9X9M079WGFPJZHNXH9VEP). A start with a fresh context ends
/// MustClear. The session becomes the lead when it is the first of its
/// user in its repository and not a worker
/// (01M3X9XA3H6YF0QCYSNB2P0CT2).
impl Command for Start {
    const KIND: CommandKind = CommandKind::Start;
    type Reply = Started;
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        if !self.reason.is_start() {
            return Err("a start needs the reason process, resume or clear".into());
        }
        let who = caller.who();
        let place = view.place(who);
        let mut changes = view.released_all(who);
        changes.push(Change::SessionStarted(SessionStarted {
            session: view.plain(who),
            reason: self.reason,
            worker: self.worker,
        }));
        if !self.worker
            && let Some(thread) = place.default_thread()
        {
            changes.extend(view.lead_if_first(who, &place, &thread, now.at));
        }
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, done: &Done, (): (), _: Now) -> Started {
        Started {
            freed: freed(&done.made),
        }
    }
}

/// The session ended: each of its claims is free. Its signal ends the
/// session in the presence. The log has no record for the end itself.
impl Command for End {
    const KIND: CommandKind = CommandKind::End;
    type Reply = ();
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        Ok((view.released_all(caller.who()), ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &Done, (): (), _: Now) {}

    fn signal(&self, _caller: &Caller) -> Option<Signal> {
        Some(Signal::Ended)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riff_core::record::Envelope;

    /// A record at `at_ms` by `by` that names `session`.
    fn joined(session: &SessionUri, by: Option<By>, at_ms: u64) -> Record {
        Record {
            envelope: Envelope {
                position: at_ms,
                written_at_ms: at_ms,
                by,
                command: None,
                call: None,
            },
            change: Change::JoinedThread(Member {
                session: session.clone(),
                thread: "design".parse().unwrap(),
            }),
        }
    }

    #[test]
    fn only_a_record_of_the_session_itself_is_its_own_call() {
        let ann: SessionUri = "riff://ann@heron/acme/app?session=a1".parse().unwrap();
        let lead: SessionUri = "riff://ann@heron/acme/app?session=l1".parse().unwrap();
        let by_ann = joined(&ann, Some(By::Session(ann.who().clone())), 10);
        assert_eq!(own_call(&by_ann), Some((ann.who(), 10)));
        let by_lead = joined(&ann, Some(By::Session(lead.who().clone())), 20);
        assert_eq!(own_call(&by_lead), None);
        assert_eq!(own_call(&joined(&ann, Some(By::Server), 30)), None);
        let by_person = joined(&ann, Some(By::Person("ann".into())), 40);
        assert_eq!(own_call(&by_person), None);
        // A record from before the caller field counts.
        assert_eq!(own_call(&joined(&ann, None, 50)), Some((ann.who(), 50)));
    }
}
