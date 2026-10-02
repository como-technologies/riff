//! The view: one copy of the riff with the presence, read only.
//!
//! [`Command::handle`](super::Command::handle) checks a command against
//! the view of the pending copy. Each read uses the view of the written
//! copy. A view cannot change the riff or the presence
//! (01M3WNQRCBP0PHSA0H3THDH5NJ).
//!
//! A view also holds the [`Settings`]: what `handle` and `reply` read
//! from the settings of the server. They are not in the log.
//!
//! This file has the rules that each group uses: the place and the URI
//! of a session, and whether it holds or is gone. The rules of the
//! leads are in [`super::work`], and the rules of a post are in
//! [`super::threads`].

use std::time::Instant;

use riff_core::name::{Place, SessionUri, Who};

use crate::owner::Timing;

use super::presence::Presence;
use super::riff::Riff;

/// One copy of the state that the log gives, with the presence. Read
/// only.
pub struct View<'a> {
    pub(super) riff: &'a Riff,
    pub(super) presence: &'a Presence,
    pub(super) settings: &'a Settings,
}

/// The settings of the server that `handle` and `reply` read
/// (01M3XA87F70CD3WH4STADSCW6S). They are not in the log: the state gets them at its
/// start, and each view holds them.
///
/// ```
/// use riff_server::owner::Timing;
/// use riff_server::state::Settings;
///
/// let settings = Settings::new(&[" Boss@X.io".into()], "https://riff.x.io", Timing::default());
/// assert!(settings.is_admin("boss@x.io"));
/// assert!(!Settings::default().is_admin("boss@x.io"));
/// assert_eq!(settings.address(), "https://riff.x.io");
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// The email of each admin of the settings, in lower case (R210).
    admins: Vec<String>,
    /// The public address of the riff: the reply to an invite names it.
    address: String,
    /// The times of the owner role.
    owner_role: Timing,
}

impl Settings {
    /// The settings with the admin emails `admins` (R210), the public
    /// address `address`, and the times of the owner role.
    pub fn new(admins: &[String], address: &str, owner_role: Timing) -> Settings {
        let mut admins: Vec<String> = admins.iter().map(|a| super::people::email(a)).collect();
        admins.sort();
        admins.dedup();
        Settings {
            admins,
            address: address.to_owned(),
            owner_role,
        }
    }

    /// The email of each admin of the settings, sorted.
    pub fn admins(&self) -> &[String] {
        &self.admins
    }

    /// True when `email`, in lower case, is an admin of the settings.
    pub fn is_admin(&self, email: &str) -> bool {
        self.admins.iter().any(|admin| admin == email)
    }

    /// The public address of the riff.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The times of the owner role.
    pub fn owner_role(&self) -> &Timing {
        &self.owner_role
    }
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
