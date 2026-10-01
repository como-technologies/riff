//! The group "the riff": the commands [`SetRiff`], [`SetIdle`] and
//! [`Forget`], and what is one for the whole riff.
//!
//! - Part of the riff: [`TheRiff`]. Paused or running, and the settings
//!   of idle workers.
//! - `apply`: `TheRiff::state_set` for a `riff_state_set` record, and
//!   `TheRiff::setting_changed` for a `setting_changed` record. The
//!   `session_forgotten` record of [`Forget`] changes each part: see
//!   [`super::riff`].
//! - Checkpoint: `Saved`, the fields `riff` and `idle`.

use riff_core::name::SessionUri;
use riff_core::record::{Change, Forgotten, RiffStateSet, SettingChanged};
use riff_core::wire::{Idle, RiffState};
use serde::{Deserialize, Serialize};

use super::command::{Command, Now};
use super::view::View;
use super::{GONE, SESSION_EXPIRY};

/// What is one for the whole riff.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TheRiff {
    /// The state of the riff. A new riff is paused.
    pub(super) state: RiffState,
    /// The settings of idle workers (01M3Q5A0TF9K49V8Z1ZY9NDF74).
    pub(super) idle: Idle,
}

impl TheRiff {
    pub(super) fn state_set(&mut self, set: &RiffStateSet) -> Result<(), &'static str> {
        self.state = set.state;
        Ok(())
    }

    pub(super) fn setting_changed(&mut self, changed: &SettingChanged) -> Result<(), &'static str> {
        self.idle = changed.idle;
        Ok(())
    }

    /// The state and the settings, for a checkpoint.
    pub(super) fn saved(&self) -> Saved {
        Saved {
            riff: self.state,
            idle: self.idle,
        }
    }
}

/// The part of the checkpoint of this group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Saved {
    #[serde(default)]
    riff: RiffState,
    #[serde(default)]
    idle: Idle,
}

impl Saved {
    pub(super) fn restore(self) -> TheRiff {
        TheRiff {
            state: self.riff,
            idle: self.idle,
        }
    }
}

/// Pauses or resumes the riff. Only a person (`me` with no session ID)
/// or a lead can (01M3JCG3T8AJZN31SZQQTP3FAF).
#[derive(Clone, Copy, Debug)]
pub struct SetRiff(pub RiffState);

impl Command for SetRiff {
    type Note = ();

    fn handle(
        &self,
        me: &SessionUri,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), String> {
        let who = me.who();
        let set = self.0;
        if who.session().is_some() && !view.is_lead(who, now.at) {
            return Err(format!(
                "only your user or the lead can make the riff {set}. Tell the lead."
            ));
        }
        let mut changes = Vec::new();
        if view.riff.the_riff().state != set {
            changes.push(Change::RiffStateSet(RiffStateSet { state: set }));
        }
        Ok((changes, ()))
    }
}

/// Sets the settings of idle workers: each value that is `Some`
/// (01M3Q5A0TF9K49V8Z1ZY9NDF74).
#[derive(Clone, Copy, Debug)]
pub struct SetIdle {
    pub per_host: Option<u16>,
    pub after_secs: Option<u64>,
}

impl Command for SetIdle {
    type Note = ();

    fn handle(
        &self,
        _me: &SessionUri,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, ()), String> {
        let mut idle = view.riff.the_riff().idle;
        if let Some(per_host) = self.per_host {
            idle.per_host = per_host;
        }
        if let Some(after_secs) = self.after_secs {
            idle.after_secs = after_secs;
        }
        let mut changes = Vec::new();
        if idle != view.riff.the_riff().idle {
            changes.push(Change::SettingChanged(SettingChanged { idle }));
        }
        Ok((changes, ()))
    }
}

/// The timer of the server forgets each session with no sign of life
/// for [`SESSION_EXPIRY`]. `me` is the URI of the server.
#[derive(Clone, Copy, Debug)]
pub struct Forget;

impl Command for Forget {
    type Note = ();

    fn handle(
        &self,
        _me: &SessionUri,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), String> {
        let mut changes = Vec::new();
        // A replayed session calls again soon when it lives.
        if view
            .presence
            .loaded
            .is_some_and(|loaded| now.at.saturating_duration_since(loaded) < GONE)
        {
            return Ok((changes, ()));
        }
        let expiry = u64::try_from(SESSION_EXPIRY.as_millis()).unwrap_or(u64::MAX);
        for (who, known) in &view.riff.sessions().known {
            let expired = match view.presence.sessions.get(who) {
                Some(session) => {
                    session.gone(now.at)
                        && now.ms.saturating_sub(session.seen_ms(now.at, now.ms)) >= expiry
                }
                None => now.ms.saturating_sub(known.at_ms) >= expiry,
            };
            if expired {
                changes.push(Change::SessionForgotten(Forgotten {
                    session: known.uri.clone(),
                }));
            }
        }
        Ok((changes, ()))
    }
}
