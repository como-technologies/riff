//! The group "sessions": the commands [`Register`], [`Start`] and
//! [`End`], and the sessions that the log names.
//!
//! - Part of the riff: [`Sessions`]. Each session that a record names,
//!   with its URI and the time of the last record that names it. A
//!   replay makes a session in the presence for each.
//! - `apply`: `Sessions::named` for each record, and
//!   `Sessions::forgotten` for a `session_forgotten` record.
//! - Checkpoint: `Saved`, the field `sessions`.
//!
//! The session in memory (its place, its signs of life) is in
//! [`super::presence`].

use std::collections::BTreeMap;

use riff_core::name::{SessionUri, Who};
use riff_core::record::{Change, Member, Record};
use serde::{Deserialize, Serialize};

use super::command::{Command, Now};
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
}

/// The session that a change names, if any.
fn named(change: &Change) -> Option<&SessionUri> {
    match change {
        Change::Posted(posted) => Some(&posted.message.from),
        Change::JoinedThread(m) | Change::LeftThread(m) | Change::LeadSet(m) => Some(&m.session),
        Change::Claimed(c) | Change::Released(c) => Some(&c.session),
        Change::RiffStateSet(_) | Change::SettingChanged(_) | Change::SessionForgotten(_) => None,
    }
}

impl Sessions {
    /// Keeps the URI of the session that `record` names, and the time
    /// of the record. The server is not a session.
    pub(super) fn named(&mut self, record: &Record) {
        if let Some(uri) = named(&record.change)
            && uri.who() != crate::owner::server_uri().who()
        {
            let known = Known {
                uri: SessionUri::new(uri.who().clone(), uri.place().clone()),
                at_ms: record.written_at_ms,
            };
            self.known.insert(uri.who().clone(), known);
        }
    }

    /// Drops a forgotten session. False when the session is not known.
    pub(super) fn forgotten(&mut self, who: &Who) -> bool {
        self.known.remove(who).is_some()
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
                })
                .collect(),
        }
    }
}

/// The part of the checkpoint of this group: each session that the log
/// names.
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
                },
            );
        }
        (sessions, seen)
    }
}

/// A session registers, or arrives: it joins the thread of its
/// repository, and becomes the lead when it is the first.
#[derive(Clone, Copy, Debug)]
pub struct Register;

impl Command for Register {
    type Note = ();

    fn handle(
        &self,
        me: &SessionUri,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), String> {
        let who = me.who();
        let mut changes = Vec::new();
        if let Some(thread) = view.place(who).default_thread()
            && !view.riff.threads().member(who, &thread)
        {
            changes.push(Change::JoinedThread(Member {
                session: view.plain(who),
                thread,
            }));
        }
        changes.extend(view.lead_if_first(who, now.at));
        Ok((changes, ()))
    }
}

/// A new start of the session: each of its claims is free.
#[derive(Clone, Copy, Debug)]
pub struct Start;

impl Command for Start {
    type Note = ();

    fn handle(
        &self,
        me: &SessionUri,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, ()), String> {
        Ok((view.released_all(me.who()), ()))
    }
}

/// The session ended: each of its claims is free.
#[derive(Clone, Copy, Debug)]
pub struct End;

impl Command for End {
    type Note = ();

    fn handle(
        &self,
        me: &SessionUri,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, ()), String> {
        Ok((view.released_all(me.who()), ()))
    }
}
