//! The group "the riff": the commands [`MakeRiff`], [`Pause`],
//! [`Resume`], [`SetIdle`] and [`Forget`], and what is one for the whole
//! riff. [`MakeRiff`] and [`Forget`] are commands of the server: no
//! client can send them.
//!
//! - Part of the riff: [`TheRiff`]. Paused or running, and the settings
//!   of idle workers.
//! - `apply`: `TheRiff::state_set` for a `riff_state_set` record, and
//!   `TheRiff::setting_changed` for a `setting_changed` record. The
//!   `session_forgotten` record of [`Forget`] changes each part: see
//!   [`super::riff`].
//! - Checkpoint: `Saved`, the fields `riff` and `idle`.

use riff_core::record::{Change, Forgotten, Record, RiffStateSet, SettingChanged};
use riff_core::wire::{Idle, Pause, Resume, RiffReply, RiffState, SetIdle};
use serde::{Deserialize, Serialize};

use super::command::{Caller, Code, Command, CommandKind, Now, Refused, Role};
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

/// The first start of a riff (01M3WRD99M99PNGP8ME50KC6WS). The server
/// sends it when the log has no record: it makes the record that pauses
/// the riff. In a riff with a record it changes nothing. E3 (#393) adds
/// the `riff_made` record, and E5 (#364) puts a `pause_set` record in
/// the place of this one.
#[derive(Clone, Copy, Debug)]
pub struct MakeRiff;

impl Command for MakeRiff {
    const KIND: CommandKind = CommandKind::MakeRiff;
    type Reply = ();
    type Note = ();

    fn handle(
        &self,
        _caller: &Caller,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let mut changes = Vec::new();
        if view.riff.position() == 0 {
            changes.push(Change::RiffStateSet(RiffStateSet {
                state: RiffState::Paused,
            }));
        }
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &[Record], (): (), _: Now) {}
}

/// The changes that make the riff `set`, for the caller `caller`. Only
/// a person (a caller with no session ID) or a lead can
/// (01M3JCG3T8AJZN31SZQQTP3FAF).
fn set_riff(
    set: RiffState,
    caller: &Caller,
    view: &View<'_>,
    now: Now,
) -> Result<(Vec<Change>, ()), Refused> {
    let who = caller.who();
    if who.session().is_some() && !view.is_lead(who, now.at) {
        return Err(Refused::new(
            Code::NotAllowed,
            format!("only your user or the lead can make the riff {set}. Tell the lead."),
        ));
    }
    let mut changes = Vec::new();
    if view.riff.the_riff().state != set {
        changes.push(Change::RiffStateSet(RiffStateSet { state: set }));
    }
    Ok((changes, ()))
}

/// The reply to a pause or a resume: the state of the written copy, and
/// whether the command changed it.
fn riff_reply(view: &View<'_>, made: &[Record]) -> RiffReply {
    RiffReply {
        state: view.riff.the_riff().state,
        changed: !made.is_empty(),
    }
}

/// Pauses the riff.
impl Command for Pause {
    const KIND: CommandKind = CommandKind::Pause;
    type Reply = RiffReply;
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        set_riff(RiffState::Paused, caller, view, now)
    }

    fn reply(&self, _: &Caller, view: &View<'_>, made: &[Record], (): (), _: Now) -> RiffReply {
        riff_reply(view, made)
    }
}

/// Resumes the riff.
impl Command for Resume {
    const KIND: CommandKind = CommandKind::Resume;
    type Reply = RiffReply;
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        set_riff(RiffState::Running, caller, view, now)
    }

    fn reply(&self, _: &Caller, view: &View<'_>, made: &[Record], (): (), _: Now) -> RiffReply {
        riff_reply(view, made)
    }
}

/// Sets the settings of idle workers: each value that is `Some`
/// (01M3Q5A0TF9K49V8Z1ZY9NDF74). It needs an admin. The reply has the
/// settings of the written copy.
impl Command for SetIdle {
    const KIND: CommandKind = CommandKind::SetIdle;
    type Reply = Idle;
    type Note = ();

    fn needs(&self) -> Role {
        Role::Admin
    }

    fn handle(
        &self,
        _caller: &Caller,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        if self.after_secs == Some(0) {
            return Err("the idle time is at least 1 second".into());
        }
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

    fn reply(&self, _: &Caller, view: &View<'_>, _: &[Record], (): (), _: Now) -> Idle {
        view.riff.the_riff().idle
    }
}

/// The number of sessions that the records of a `forget` forgot.
pub(super) fn forgotten(made: &[Record]) -> usize {
    made.iter()
        .filter(|record| matches!(record.change, Change::SessionForgotten(_)))
        .count()
}

/// The timer of the server forgets each session with no sign of life
/// for [`SESSION_EXPIRY`]. It gives one `released` record for each
/// claim of the session, then the `session_forgotten` record
/// (01M3X9XCSBR11ACD86FNXKF8JH). The reply is the number of sessions that it forgot.
#[derive(Clone, Copy, Debug)]
pub struct Forget;

impl Command for Forget {
    const KIND: CommandKind = CommandKind::Forget;
    type Reply = usize;
    type Note = ();

    fn handle(
        &self,
        _caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
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
                changes.extend(view.released_all(who));
                changes.push(Change::SessionForgotten(Forgotten {
                    session: known.uri.clone(),
                }));
            }
        }
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, made: &[Record], (): (), _: Now) -> usize {
        forgotten(made)
    }
}
