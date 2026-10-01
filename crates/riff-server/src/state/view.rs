//! The view: one copy of the riff with the presence, read only.
//!
//! [`Command::handle`](super::Command::handle) checks a command against
//! the view of the pending copy. Each read uses the view of the written
//! copy. A view cannot change the riff or the presence
//! (01M3WNQRCBP0PHSA0H3THDH5NJ).
//!
//! This file has the rules that each group uses: the place and the URI
//! of a session, and whether it holds or is gone. The rules of the
//! leads are in [`super::work`], and the rules of a post are in
//! [`super::threads`].

use std::time::Instant;

use riff_core::name::{Place, SessionUri, Who};

use super::presence::Presence;
use super::riff::Riff;

/// One copy of the state that the log gives, with the presence. Read
/// only.
pub struct View<'a> {
    pub(super) riff: &'a Riff,
    pub(super) presence: &'a Presence,
}

impl View<'_> {
    /// The place of a session. The server is not a session: it has the
    /// place of [`crate::owner::server_uri`].
    pub(super) fn place(&self, who: &Who) -> Place {
        self.presence.sessions.get(who).map_or_else(
            || crate::owner::server_uri().place().clone(),
            |s| s.place.clone(),
        )
    }

    /// The URI of a session now: its place, whether it is the lead, and
    /// the claims that it holds.
    pub(super) fn uri(&self, who: &Who, now: Instant) -> SessionUri {
        SessionUri::new(who.clone(), self.place(who))
            .with_lead(self.is_lead(who, now))
            .with_claims(self.riff.work().items_of(who))
    }

    /// The URI of a session in a record: its who and place only.
    pub(super) fn plain(&self, who: &Who) -> SessionUri {
        SessionUri::new(who.clone(), self.place(who))
    }

    /// True while the claims and the lead of `holder` hold.
    pub(super) fn holds(&self, holder: &Who, now: Instant) -> bool {
        self.presence
            .sessions
            .get(holder)
            .is_some_and(|s| s.holds(now, self.presence.loaded))
    }

    /// True when the presence does not know `who`, or it is gone.
    pub(super) fn gone(&self, who: &Who, now: Instant) -> bool {
        self.presence.sessions.get(who).is_none_or(|s| s.gone(now))
    }
}
