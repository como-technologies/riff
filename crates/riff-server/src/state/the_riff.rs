//! The group "the riff": the commands [`MakeRiff`], [`Pause`],
//! [`Resume`], [`SetIdle`] and [`Forget`], and what is one for the whole
//! riff. [`MakeRiff`] and [`Forget`] are commands of the server: no
//! client can send them.
//!
//! - Part of the riff: [`TheRiff`]. The pauses ([`Pauses`]), and the
//!   settings of idle workers.
//! - `apply`: `TheRiff::pause_set` for a `pause_set` record and for a
//!   `riff_state_set` record of an old log, and
//!   `TheRiff::setting_changed` for a `setting_changed` record. The
//!   `session_forgotten` record of [`Forget`] changes each part: see
//!   [`super::riff`].
//! - Checkpoint: `Saved`, the fields `riff`, `idle`, `riff_pause` and
//!   `pauses`.
//!
//! # The two pauses (01M3XAHZBGSSJB3YX23K88W01K)
//!
//! The riff has a pause of the whole riff, and a pause for each
//! repository. A thread is paused when the riff is paused, or when it
//! is the thread of a repository that is paused.
//!
//! | Pause | Who can set and end it (01M3XAHZDSQR263QZVB41CK0MX) |
//! |---|---|
//! | the repository of the call | a person; the lead of that repository |
//! | a repository that the call names | an admin: as a person, or as a lead |
//! | the whole riff | an admin: as a person, or as a lead |
//!
//! [`permits`](super::permits) checks the role: [`Command::needs`] is
//! an admin for the whole riff and for a named repository. `handle`
//! checks the lead, because it reads the state.

use std::collections::BTreeMap;

use riff_core::name::ThreadName;
use riff_core::record::{Change, Forgotten, PauseSet, Record, Scope, SettingChanged};
use riff_core::wire::{
    Idle, Pause, PauseInfo, RepositoryPause, Resume, RiffReply, RiffState, SetIdle,
};
use serde::{Deserialize, Serialize};

use super::command::{Caller, Class, Code, Command, CommandKind, Now, Refused, Role};
use super::view::View;
use super::{GONE, SESSION_EXPIRY};

/// The pauses of the riff (01M3XAHZBGSSJB3YX23K88W01K): the pause of
/// the whole riff, and the pause of each repository. Each pause names
/// who set it and when: the `by` and the time of its record.
///
/// ```
/// use std::time::Instant;
/// use riff_core::name::{SessionUri, ThreadName};
/// use riff_core::record::Scope;
/// use riff_core::wire::RiffState;
/// use riff_server::state::State;
///
/// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a1".parse()?;
/// let riff: ThreadName = "como-technologies/riff".parse()?;
/// let strata: ThreadName = "como-technologies/strata".parse()?;
/// let now = Instant::now();
/// let mut state = State::default();
/// // A new riff is paused.
/// assert_eq!(state.pauses().check(&riff).unwrap().0, Scope::Riff);
/// state.riff(&mike, Some(RiffState::Running), now).unwrap();
/// assert!(state.pauses().check(&riff).is_none());
///
/// // The lead pauses its repository. The other repository goes on.
/// state.pause_repository(&mike, RiffState::Paused, now).unwrap();
/// let (scope, pause) = state.pauses().check(&riff).unwrap();
/// assert_eq!(scope, Scope::Repository(riff));
/// assert_eq!(pause.by.as_ref().unwrap().to_string(), "the session mike/a1");
/// assert!(state.pauses().check(&strata).is_none());
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pauses {
    /// The pause of the whole riff. A new riff is paused.
    riff: Option<PauseInfo>,
    /// The pause of each repository thread. A new repository runs.
    repositories: BTreeMap<ThreadName, PauseInfo>,
}

impl Default for Pauses {
    /// A new riff is paused. No caller set that pause.
    fn default() -> Self {
        Pauses {
            riff: Some(PauseInfo::default()),
            repositories: BTreeMap::new(),
        }
    }
}

impl Pauses {
    /// The pause of the whole riff, when it is paused.
    pub fn riff(&self) -> Option<&PauseInfo> {
        self.riff.as_ref()
    }

    /// The pause of the repository `thread`, when it is paused.
    pub fn repository(&self, thread: &ThreadName) -> Option<&PauseInfo> {
        self.repositories.get(thread)
    }

    /// The pause that stops `thread`, with its scope: the pause of the
    /// riff, else the pause of the repository `thread`. `None` when the
    /// thread runs. A thread that is not a repository thread sees only
    /// the pause of the riff.
    pub fn check(&self, thread: &ThreadName) -> Option<(Scope, &PauseInfo)> {
        self.at(Some(thread))
    }

    /// The pause that stops a session in the repository `thread`, as
    /// [`Pauses::check`]. A place with no repository sees only the
    /// pause of the riff.
    pub fn at(&self, thread: Option<&ThreadName>) -> Option<(Scope, &PauseInfo)> {
        if let Some(pause) = &self.riff {
            return Some((Scope::Riff, pause));
        }
        let thread = thread?;
        let pause = self.repositories.get(thread)?;
        Some((Scope::Repository(thread.clone()), pause))
    }

    /// True when the pause of `scope` is set.
    fn holds(&self, scope: &Scope) -> bool {
        match scope {
            Scope::Riff => self.riff.is_some(),
            Scope::Repository(thread) => self.repositories.contains_key(thread),
            Scope::Other => false,
        }
    }

    /// The pauses as a caller in the repository `thread` sees them.
    pub fn reply(&self, thread: Option<&ThreadName>, changed: bool) -> RiffReply {
        let state = match self.at(thread) {
            Some(_) => RiffState::Paused,
            None => RiffState::Running,
        };
        let repositories = self
            .repositories
            .iter()
            .map(|(repository, pause)| RepositoryPause {
                repository: repository.clone(),
                pause: pause.clone(),
            })
            .collect();
        RiffReply {
            state,
            changed,
            riff: self.riff.clone(),
            repositories,
        }
    }
}

/// What is one for the whole riff.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TheRiff {
    /// The pause of the riff and of each repository.
    pub(super) pauses: Pauses,
    /// The settings of idle workers (01M3Q5A0TF9K49V8Z1ZY9NDF74).
    pub(super) idle: Idle,
}

impl TheRiff {
    /// Sets or ends the pause of `scope`, as `record` says: its `by`
    /// and its time are who set the pause, and when. A scope that this
    /// build does not know changes no pause
    /// (01M3XAHZG26ECNARX35JD73YXJ).
    pub(super) fn pause_set(
        &mut self,
        scope: &Scope,
        state: RiffState,
        record: &Record,
    ) -> Result<(), &'static str> {
        let pause = match state {
            RiffState::Paused => Some(PauseInfo {
                by: record.by.clone(),
                at_ms: record.written_at_ms,
            }),
            RiffState::Running => None,
        };
        match (scope, pause) {
            (Scope::Riff, pause) => self.pauses.riff = pause,
            (Scope::Repository(thread), Some(pause)) => {
                self.pauses.repositories.insert(thread.clone(), pause);
            }
            (Scope::Repository(thread), None) => {
                self.pauses.repositories.remove(thread);
            }
            (Scope::Other, _) => return Err("this build does not know the scope of the pause"),
        }
        Ok(())
    }

    pub(super) fn setting_changed(&mut self, changed: &SettingChanged) -> Result<(), &'static str> {
        self.idle = changed.idle;
        Ok(())
    }

    /// The pauses and the settings, for a checkpoint.
    pub(super) fn saved(&self) -> Saved {
        let riff = match self.pauses.riff {
            Some(_) => RiffState::Paused,
            None => RiffState::Running,
        };
        Saved {
            riff,
            idle: self.idle,
            riff_pause: self
                .pauses
                .riff
                .clone()
                .filter(|pause| *pause != PauseInfo::default()),
            pauses: self.pauses.repositories.clone(),
        }
    }
}

/// The part of the checkpoint of this group. It holds each pause, with
/// who set it and when (01M3XAHZQ92GGFHBC50FQ7FQ0K). A checkpoint from
/// before the pause of a repository has only `riff` and `idle`: it
/// reads, and nobody is known to have set its pause.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Saved {
    #[serde(default)]
    riff: RiffState,
    #[serde(default)]
    idle: Idle,
    /// Who paused the whole riff, and when. Absent when the riff runs,
    /// and when no caller set the pause.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    riff_pause: Option<PauseInfo>,
    /// Each repository that is paused.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pauses: BTreeMap<ThreadName, PauseInfo>,
}

impl Saved {
    pub(super) fn restore(self) -> TheRiff {
        let riff = match self.riff {
            RiffState::Paused => Some(self.riff_pause.unwrap_or_default()),
            RiffState::Running => None,
        };
        TheRiff {
            pauses: Pauses {
                riff,
                repositories: self.pauses,
            },
            idle: self.idle,
        }
    }
}

/// The first start of a riff (01M3WRD99M99PNGP8ME50KC6WS). The server
/// sends it when the log has no record: it makes the `pause_set` record
/// that pauses the whole riff. In a riff with a record it changes
/// nothing. E3 (#393) adds the `riff_made` record.
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
            changes.push(Change::PauseSet(PauseSet {
                scope: Scope::Riff,
                state: RiffState::Paused,
            }));
        }
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &[Record], (): (), _: Now) {}
}

/// The repository of a call: for a session, the repository of its
/// place in the state; for a person, the repository of the `me` of the
/// call.
fn repository_of(caller: &Caller, view: &View<'_>) -> Option<ThreadName> {
    match caller.class() {
        Class::Session => view.place(caller.who()).default_thread(),
        _ => caller.me().default_thread(),
    }
}

/// The role that a pause or a resume needs: an admin for the whole
/// riff and for a repository that the call names
/// (01M3XAHZDSQR263QZVB41CK0MX).
fn role_for(whole: bool, named: Option<&ThreadName>) -> Role {
    if whole || named.is_some() {
        Role::Admin
    } else {
        Role::Member
    }
}

/// The changes that make the pause of the call `set`
/// (01M3XAHZDSQR263QZVB41CK0MX). `whole` names the whole riff, and
/// `named` a repository. With none of the two, the scope is the
/// repository of the call. A session must be a lead. [`permits`]
/// checked the role before.
///
/// [`permits`]: super::permits
fn set_pause(
    set: RiffState,
    whole: bool,
    named: Option<&ThreadName>,
    caller: &Caller,
    view: &View<'_>,
    now: Now,
) -> Result<(Vec<Change>, ()), Refused> {
    let verb = match set {
        RiffState::Paused => "pause",
        RiffState::Running => "resume",
    };
    let who = caller.who();
    let lead = caller.class() != Class::Session || view.is_lead(who, now.at);
    let scope = match (whole, named) {
        (true, Some(_)) => {
            return Err("name the whole riff or one repository, not the two".into());
        }
        (true, None) => Scope::Riff,
        (false, Some(thread)) => Scope::Repository(thread.clone()),
        (false, None) => match repository_of(caller, view) {
            Some(thread) => Scope::Repository(thread),
            None => {
                return Err(format!(
                    "this place has no repository. Run riff {verb} in a repository, or name \
                     the whole riff with --riff."
                )
                .into());
            }
        },
    };
    if !lead {
        return Err(Refused::new(
            Code::NotAllowed,
            format!("only your user or the lead can {verb} {scope}. Tell the lead."),
        ));
    }
    let mut changes = Vec::new();
    if view.riff.the_riff().pauses.holds(&scope) != (set == RiffState::Paused) {
        changes.push(Change::PauseSet(PauseSet { scope, state: set }));
    }
    Ok((changes, ()))
}

/// The reply to a pause or a resume: the pauses of the written copy as
/// the caller sees them, and whether the command changed one.
fn riff_reply(caller: &Caller, view: &View<'_>, made: &[Record]) -> RiffReply {
    let thread = repository_of(caller, view);
    view.riff
        .the_riff()
        .pauses
        .reply(thread.as_ref(), !made.is_empty())
}

/// Pauses the repository of the call, a named repository, or the whole
/// riff (01M3XAHZBGSSJB3YX23K88W01K).
impl Command for Pause {
    const KIND: CommandKind = CommandKind::Pause;
    type Reply = RiffReply;
    type Note = ();

    fn needs(&self) -> Role {
        role_for(self.riff, self.repository.as_ref())
    }

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let named = self.repository.as_ref();
        set_pause(RiffState::Paused, self.riff, named, caller, view, now)
    }

    fn reply(
        &self,
        caller: &Caller,
        view: &View<'_>,
        made: &[Record],
        (): (),
        _: Now,
    ) -> RiffReply {
        riff_reply(caller, view, made)
    }
}

/// Resumes the repository of the call, a named repository, or the
/// whole riff (01M3XAHZBGSSJB3YX23K88W01K). A resume of a repository
/// while the riff is paused changes only the pause of the repository.
impl Command for Resume {
    const KIND: CommandKind = CommandKind::Resume;
    type Reply = RiffReply;
    type Note = ();

    fn needs(&self) -> Role {
        role_for(self.riff, self.repository.as_ref())
    }

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let named = self.repository.as_ref();
        set_pause(RiffState::Running, self.riff, named, caller, view, now)
    }

    fn reply(
        &self,
        caller: &Caller,
        view: &View<'_>,
        made: &[Record],
        (): (),
        _: Now,
    ) -> RiffReply {
        riff_reply(caller, view, made)
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
