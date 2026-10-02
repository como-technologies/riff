//! A command: a call that asks for a change of the riff.
//!
//! Each command is a type that implements [`Command`]
//! (01M3WNQRCBP0PHSA0H3THDH5NJ). The type is in the file of its group.
//! The wire type of a command that a client can send is its command
//! type (01M3WRD8TBDPA4JNEZY6J4N2EX):
//!
//! | Group | File | Commands |
//! |---|---|---|
//! | sessions | [`super::sessions`] | [`Register`](riff_core::wire::Register), [`Arrive`](super::Arrive), [`Start`](riff_core::wire::Start), [`End`](riff_core::wire::End) |
//! | threads | [`super::threads`] | [`Join`](riff_core::wire::Join), [`Leave`](riff_core::wire::Leave), [`Post`](riff_core::wire::Post), [`Announce`](super::Announce) |
//! | work | [`super::work`] | [`Claim`](riff_core::wire::Claim), [`Release`](riff_core::wire::Release), [`ReleaseFor`](riff_core::wire::ReleaseFor), [`Lead`](riff_core::wire::Lead) |
//! | the riff | [`super::the_riff`] | [`MakeRiff`](super::MakeRiff), [`Pause`](riff_core::wire::Pause), [`Resume`](riff_core::wire::Resume), [`SetIdle`](riff_core::wire::SetIdle), [`Forget`](super::Forget) |
//!
//! # Who can send a command
//!
//! [`permits`] holds the table (01M3WRD959DYNZHDKP5ZT9Q1C7). It reads
//! only the [`Caller`] and the role that the command needs. The state
//! runs it before [`Command::handle`].

use std::fmt;
use std::time::Instant;

use riff_core::name::{SessionUri, Who};
use riff_core::record::{By, Change, Record};

use super::presence::Signal;
use super::view::View;

/// The time of a call: an instant, and the same time in milliseconds
/// since the Unix epoch, from the clock of the state.
#[derive(Clone, Copy, Debug)]
pub struct Now {
    pub at: Instant,
    pub ms: u64,
}

/// The class of a caller: who sends a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// A person, from a person token. It has no session ID.
    Person,
    /// An agent session, from a session token.
    Session,
    /// A verified email of the provider, before a token is there. No
    /// command of today takes it: E3 (#393) adds `admit`.
    SignIn,
    /// A timer of `riff-server`.
    Server,
}

impl Class {
    /// Each class.
    pub const ALL: [Class; 4] = [Class::Person, Class::Session, Class::SignIn, Class::Server];

    fn text(self) -> &'static str {
        match self {
            Class::Person => "a person",
            Class::Session => "a session",
            Class::SignIn => "a sign-in",
            Class::Server => "the server",
        }
    }
}

/// The role of a caller. The owner has the role of an admin too, and an
/// admin the role of a member.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Member,
    Admin,
    Owner,
}

impl Role {
    /// Each role.
    pub const ALL: [Role; 3] = [Role::Member, Role::Admin, Role::Owner];
}

/// Who sends a call (01M3WRD959DYNZHDKP5ZT9Q1C7). The token layer gives
/// the class and the `me`. The engine adds the worker mark and the role
/// under the lock of the state.
///
/// ```
/// use riff_core::name::SessionUri;
/// use riff_server::state::{Caller, Class, Role};
///
/// let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
/// let caller = Caller::of(&me);
/// assert_eq!(caller.class(), Class::Session);
/// assert_eq!(caller.role(), Role::Member);
/// assert!(!caller.worker());
///
/// let person: SessionUri = "riff://mike@pangolin".parse()?;
/// assert_eq!(Caller::of(&person).class(), Class::Person);
/// assert_eq!(Caller::server().class(), Class::Server);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caller {
    class: Class,
    me: SessionUri,
    worker: bool,
    role: Role,
}

impl Caller {
    /// The caller that acts as `me`: a session when `me` has a session
    /// ID, else a person. It is a member, and not a worker.
    pub fn of(me: &SessionUri) -> Caller {
        let class = match me.who().session() {
            Some(_) => Class::Session,
            None => Class::Person,
        };
        Caller {
            class,
            me: me.clone(),
            worker: false,
            role: Role::Member,
        }
    }

    /// The server itself, for the command of a timer.
    pub fn server() -> Caller {
        Caller {
            class: Class::Server,
            me: crate::owner::server_uri(),
            worker: false,
            role: Role::Member,
        }
    }

    /// The same caller with another class.
    pub fn with_class(self, class: Class) -> Caller {
        Caller { class, ..self }
    }

    /// The same caller with this worker mark.
    pub fn with_worker(self, worker: bool) -> Caller {
        Caller { worker, ..self }
    }

    /// The same caller with this role.
    pub fn with_role(self, role: Role) -> Caller {
        Caller { role, ..self }
    }

    pub fn class(&self) -> Class {
        self.class
    }

    /// The URI that the caller acts as: its who, and the place of the
    /// call.
    pub fn me(&self) -> &SessionUri {
        &self.me
    }

    pub fn who(&self) -> &Who {
        self.me.who()
    }

    /// True when the session of the caller is a worker.
    pub fn worker(&self) -> bool {
        self.worker
    }

    pub fn role(&self) -> Role {
        self.role
    }

    /// The caller as a record and a log line name it: its class, with
    /// the user and the session ID (01M3X4Z60G1FXQTDC5XDJ05BAX).
    ///
    /// ```
    /// use riff_core::name::SessionUri;
    /// use riff_core::record::By;
    /// use riff_server::state::Caller;
    ///
    /// let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// assert_eq!(Caller::of(&me).by().json().to_string(), r#"{"session":"mike/a6cf"}"#);
    /// let person: SessionUri = "riff://mike@pangolin".parse()?;
    /// assert_eq!(Caller::of(&person).by(), By::Person("mike".into()));
    /// assert_eq!(Caller::server().by(), By::Server);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn by(&self) -> By {
        let who = self.who();
        match self.class {
            Class::Person => By::Person(who.user().to_owned()),
            Class::Session => By::Session(who.clone()),
            Class::SignIn => By::SignIn(who.user().to_owned()),
            Class::Server => By::Server,
        }
    }
}

/// The cause of a record: the caller and the kind of its command
/// (01M3X4Z60G1FXQTDC5XDJ05BAX). [`State::queue`](super::State::queue) writes it in the
/// envelope of each record of the command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cause {
    pub by: By,
    pub command: CommandKind,
}

impl Cause {
    /// The cause of each record of a command of `kind` from `caller`.
    pub fn of(caller: &Caller, kind: CommandKind) -> Cause {
        Cause {
            by: caller.by(),
            command: kind,
        }
    }
}

/// The kind of a command: its name in a record and in a log line. A
/// kind is never renamed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Register,
    Start,
    End,
    Join,
    Leave,
    Post,
    Announce,
    Claim,
    Release,
    ReleaseFor,
    Lead,
    MakeRiff,
    Pause,
    Resume,
    SetIdle,
    Forget,
}

impl CommandKind {
    /// Each kind of this build.
    pub const ALL: [CommandKind; 16] = [
        CommandKind::Register,
        CommandKind::Start,
        CommandKind::End,
        CommandKind::Join,
        CommandKind::Leave,
        CommandKind::Post,
        CommandKind::Announce,
        CommandKind::Claim,
        CommandKind::Release,
        CommandKind::ReleaseFor,
        CommandKind::Lead,
        CommandKind::MakeRiff,
        CommandKind::Pause,
        CommandKind::Resume,
        CommandKind::SetIdle,
        CommandKind::Forget,
    ];

    /// The name of the kind.
    pub fn as_str(self) -> &'static str {
        match self {
            CommandKind::Register => "register",
            CommandKind::Start => "start",
            CommandKind::End => "end",
            CommandKind::Join => "join",
            CommandKind::Leave => "leave",
            CommandKind::Post => "post",
            CommandKind::Announce => "announce",
            CommandKind::Claim => "claim",
            CommandKind::Release => "release",
            CommandKind::ReleaseFor => "release_for",
            CommandKind::Lead => "lead",
            CommandKind::MakeRiff => "make_riff",
            CommandKind::Pause => "pause",
            CommandKind::Resume => "resume",
            CommandKind::SetIdle => "set_idle",
            CommandKind::Forget => "forget",
        }
    }
}

impl fmt::Display for CommandKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The code of a refusal (01M3WRD9JBQMNN96TXJH8EAJ3W): the fixed set of
/// this release. A test of a refusal compares the code. The log line
/// of a refused command and the header `riff-refused` of its reply
/// have the name of the code. A release can add a code, and a reader
/// takes a code that it does not know as text.
///
/// | Code | When | HTTP status |
/// |---|---|---|
/// | `not_allowed` | The class or the role of the caller cannot send the command. | 403 |
/// | `no_sign_in` | A command of the people in a riff with no sign-in (E3, #393). | 403 |
/// | `not_member` | The command names a person who is not a member (E3, #393). | 403 |
/// | `held` | Another session holds the item. | 409 |
/// | `paused` | The riff or the repository is paused. | 409 |
/// | `must_clear` | A worker must clear its context first. | 409 |
/// | `not_holder` | The caller does not hold the item. | 409 |
/// | `other_user` | The session ID is known under another user. | 409 |
/// | `bad_request` | The fields of the call do not agree. | 400 |
///
/// [`Failed::status`](crate::engine::Failed::status) gives the status
/// (01M3X4Z69CFV23V4QZBE8RP1GJ).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    /// The class or the role of the caller cannot send the command.
    NotAllowed,
    /// A command of the people in a riff with no sign-in.
    NoSignIn,
    /// The command names a person who is not a member.
    NotMember,
    /// Another session holds the item.
    Held,
    /// The riff or the repository is paused.
    Paused,
    /// A worker must clear its context before it claims.
    MustClear,
    /// The caller does not hold the item.
    NotHolder,
    /// The session ID is known under another user.
    OtherUser,
    /// The fields of the call do not agree.
    BadRequest,
}

impl Code {
    /// Each code of this release.
    pub const ALL: [Code; 9] = [
        Code::NotAllowed,
        Code::NoSignIn,
        Code::NotMember,
        Code::Held,
        Code::Paused,
        Code::MustClear,
        Code::NotHolder,
        Code::OtherUser,
        Code::BadRequest,
    ];

    /// The name of the code.
    pub fn as_str(self) -> &'static str {
        match self {
            Code::NotAllowed => "not_allowed",
            Code::NoSignIn => "no_sign_in",
            Code::NotMember => "not_member",
            Code::Held => "held",
            Code::Paused => "paused",
            Code::MustClear => "must_clear",
            Code::NotHolder => "not_holder",
            Code::OtherUser => "other_user",
            Code::BadRequest => "bad_request",
        }
    }
}

/// Why a command is refused: a code, and a reason as text for the
/// caller.
///
/// ```
/// use riff_server::state::{Code, Refused};
///
/// let refused: Refused = "a selector needs one or more fields".into();
/// assert_eq!(refused.code, Code::BadRequest);
/// assert_eq!(Refused::new(Code::Held, "issue-7 is held").code.as_str(), "held");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub code: Code,
    pub reason: String,
}

impl Refused {
    pub fn new(code: Code, reason: impl Into<String>) -> Refused {
        Refused {
            code,
            reason: reason.into(),
        }
    }
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

impl From<String> for Refused {
    fn from(reason: String) -> Refused {
        Refused::new(Code::BadRequest, reason)
    }
}

impl From<&str> for Refused {
    fn from(reason: &str) -> Refused {
        Refused::new(Code::BadRequest, reason)
    }
}

// ANCHOR: permits
/// Says if `caller` can send a command of `kind` that needs the role
/// `needs` (01M3WRD959DYNZHDKP5ZT9Q1C7). It reads only the caller: its
/// class, its worker mark and its role. It does not read the state.
/// Each kind has a row: a kind with no row does not compile. The life
/// cycle of a session is a check of `handle`, not of this table.
///
/// ```
/// use riff_core::name::SessionUri;
/// use riff_server::state::{Caller, Code, CommandKind, Role, permits};
///
/// let session: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
/// let session = Caller::of(&session);
/// assert!(permits(CommandKind::Claim, &session, Role::Member).is_ok());
/// // Only the server forgets a session.
/// let refused = permits(CommandKind::Forget, &session, Role::Member).unwrap_err();
/// assert_eq!(refused.code, Code::NotAllowed);
/// // A worker is never the lead.
/// let worker = session.clone().with_worker(true);
/// assert!(permits(CommandKind::Lead, &worker, Role::Member).is_err());
/// // A change of the idle workers needs an admin.
/// assert!(permits(CommandKind::SetIdle, &session, Role::Admin).is_err());
/// let admin = session.with_role(Role::Admin);
/// assert!(permits(CommandKind::SetIdle, &admin, Role::Admin).is_ok());
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn permits(kind: CommandKind, caller: &Caller, needs: Role) -> Result<(), Refused> {
    use Class::{Person, Server, Session};
    let class = caller.class();
    let classes: &[Class] = match kind {
        // The engine registers the first call of a person too. A person
        // has no life cycle.
        CommandKind::Register => &[Person, Session],
        CommandKind::Start | CommandKind::End => &[Session],
        CommandKind::Join | CommandKind::Leave | CommandKind::Post => &[Person, Session],
        CommandKind::Claim | CommandKind::Release => &[Person, Session],
        // `handle` checks that the caller is the lead of the user of
        // the holder.
        CommandKind::ReleaseFor => &[Session],
        CommandKind::Lead => &[Session],
        // The command gives the role: an admin for the whole riff.
        // `handle` checks that a session is a lead.
        CommandKind::Pause | CommandKind::Resume => &[Person, Session],
        // Until E3 (#393): then only a person changes the settings.
        CommandKind::SetIdle => &[Person, Session],
        CommandKind::MakeRiff | CommandKind::Announce | CommandKind::Forget => &[Server],
    };
    if !classes.contains(&class) {
        let reason = match (kind, class) {
            (CommandKind::Lead, Person) => "only an agent session can be the lead".to_owned(),
            _ => format!("{} cannot send the command {kind}", class.text()),
        };
        return Err(Refused::new(Code::NotAllowed, reason));
    }
    if kind == CommandKind::Lead && caller.worker() {
        return Err(Refused::new(
            Code::NotAllowed,
            "a worker cannot be the lead. Make another session the lead.",
        ));
    }
    if class != Server && caller.role() < needs {
        let user = caller.who().user();
        let reason = match (kind, needs) {
            (CommandKind::SetIdle, _) => format!(
                "{user} is not an admin; only an admin can change the settings of idle workers"
            ),
            (CommandKind::Pause | CommandKind::Resume, _) => format!(
                "{user} is not an admin; only the owner or an admin can {kind} the whole riff \
                 or a repository by its name"
            ),
            (_, Role::Owner) => format!("{user} is not the owner; the command {kind} needs it"),
            _ => format!("{user} is not an admin; the command {kind} needs an admin"),
        };
        return Err(Refused::new(Code::NotAllowed, reason));
    }
    Ok(())
}
// ANCHOR_END: permits

// ANCHOR: command
/// A call that asks for a change. [`Command::handle`] holds its rule.
///
/// ```
/// use std::time::Instant;
/// use riff_core::name::SessionUri;
/// use riff_core::wire::Claim;
/// use riff_server::state::{Caller, Code, Command, CommandKind, State};
///
/// /// The kind of a command, and the code of its refusal now.
/// fn refusal<C: Command>(state: &mut State, me: &SessionUri, command: &C) -> (&'static str, Option<Code>) {
///     let refused = state.run(&Caller::of(me), command, Instant::now()).err();
///     (C::KIND.as_str(), refused.map(|r| r.code))
/// }
///
/// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
/// let mut state = State::default();
/// state.register(&mike, Instant::now());
/// let claim = Claim { me: mike.clone(), thread: mike.default_thread().unwrap(), item: "issue-12".into() };
/// // A new riff is paused, so a claim is refused.
/// assert_eq!(refusal(&mut state, &mike, &claim), ("claim", Some(Code::Paused)));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub trait Command: Send + 'static {
    /// The name in each record and in each log line. Never renamed.
    const KIND: CommandKind;
    /// The reply to the call.
    type Reply: Send;
    /// What `handle` keeps for the reply, for example the selectors of
    /// a post that matched no session.
    type Note: Send;

    /// The role that the command needs. [`permits`] compares it with
    /// the role of the caller.
    fn needs(&self) -> Role {
        Role::Member
    }

    /// Checks the command of `caller` against `view`: the pending copy.
    /// Gives the changes and the note, or why the command is refused.
    /// It does no I/O and changes nothing.
    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, Self::Note), Refused>;

    /// Makes the reply from `view`: the written copy, after the write
    /// of `made`, the records of the command.
    fn reply(
        &self,
        caller: &Caller,
        view: &View<'_>,
        made: &[Record],
        note: Self::Note,
        now: Now,
    ) -> Self::Reply;

    /// The signal of the call, for the presence. Most commands have
    /// none. The state sets it at the check, only when the command is
    /// not refused (01M3WRD97EZJK3AABXECXEY133).
    fn signal(&self, _caller: &Caller) -> Option<Signal> {
        None
    }
}
// ANCHOR_END: command

#[cfg(test)]
mod tests {
    use super::*;

    fn caller(class: Class) -> Caller {
        let me: SessionUri = match class {
            Class::Session => "riff://ann@heron/acme/app?session=a1",
            Class::Person | Class::SignIn => "riff://ann@heron",
            Class::Server => return Caller::server(),
        }
        .parse()
        .unwrap();
        Caller::of(&me).with_class(class)
    }

    /// The table "Who can send a command" of the design: the classes
    /// that can send the kind, and whether a worker can.
    fn row(kind: CommandKind) -> (&'static [Class], bool) {
        use Class::{Person, Server, Session};
        match kind {
            CommandKind::Register => (&[Person, Session], true),
            CommandKind::Start | CommandKind::End => (&[Session], true),
            CommandKind::Join | CommandKind::Leave | CommandKind::Post => {
                (&[Person, Session], true)
            }
            CommandKind::Claim | CommandKind::Release => (&[Person, Session], true),
            CommandKind::ReleaseFor => (&[Session], true),
            CommandKind::Lead => (&[Session], false),
            CommandKind::Pause | CommandKind::Resume => (&[Person, Session], true),
            CommandKind::SetIdle => (&[Person, Session], true),
            CommandKind::MakeRiff | CommandKind::Announce | CommandKind::Forget => {
                (&[Server], true)
            }
        }
    }

    /// The role that the command of `kind` needs: [`Command::needs`] of
    /// one command of its type. So the test reads the role from the
    /// command, as the engine does.
    fn needs(kind: CommandKind) -> Role {
        use riff_core::wire::{
            Claim, End, Join, Kind, Lead, Leave, Pause, Post, Register, Release, ReleaseFor,
            Resume, SetIdle, Start, StartReason,
        };

        use crate::state::{Announce, Forget, MakeRiff};

        fn of<C: Command>(command: C, kind: CommandKind) -> Role {
            assert_eq!(C::KIND, kind);
            command.needs()
        }
        let me: SessionUri = "riff://ann@heron/acme/app?session=a1".parse().unwrap();
        let thread = me.default_thread().unwrap();
        let item = "issue-7".to_owned();
        match kind {
            CommandKind::Register => of(Register { me, worker: false }, kind),
            CommandKind::Start => {
                let start = Start {
                    me,
                    reason: StartReason::Process,
                    worker: false,
                };
                of(start, kind)
            }
            CommandKind::End => of(End { me }, kind),
            CommandKind::Join => of(Join { me, thread }, kind),
            CommandKind::Leave => of(Leave { me, thread }, kind),
            CommandKind::Post => of(Post::new(&me, Some(thread), vec![], "hi"), kind),
            CommandKind::Announce => {
                let announce = Announce {
                    thread: Some(thread),
                    to: Vec::new(),
                    body: "hi".into(),
                    kind: Kind::Note,
                    at_ms: 0,
                };
                of(announce, kind)
            }
            CommandKind::Claim => of(Claim { me, thread, item }, kind),
            CommandKind::Release => of(Release { me, thread, item }, kind),
            CommandKind::ReleaseFor => {
                let session = "a2".to_owned();
                let release = ReleaseFor {
                    me,
                    thread,
                    item,
                    session,
                };
                of(release, kind)
            }
            CommandKind::Lead => of(Lead { me }, kind),
            CommandKind::MakeRiff => of(MakeRiff, kind),
            CommandKind::Pause => of(Pause::here(me), kind),
            CommandKind::Resume => of(Resume::here(me), kind),
            CommandKind::SetIdle => {
                let set = SetIdle {
                    me,
                    per_host: Some(1),
                    after_secs: None,
                };
                of(set, kind)
            }
            CommandKind::Forget => of(Forget, kind),
        }
    }

    #[test]
    fn permits_is_the_table_for_each_kind_class_mark_and_role() {
        let mut tried = 0;
        for kind in CommandKind::ALL {
            let (classes, workers) = row(kind);
            for class in Class::ALL {
                for worker in [false, true] {
                    for role in Role::ALL {
                        let caller = caller(class).with_worker(worker).with_role(role);
                        let expected = classes.contains(&class)
                            && (workers || !worker)
                            && (class == Class::Server || role >= needs(kind));
                        let result = permits(kind, &caller, needs(kind));
                        assert_eq!(
                            result.is_ok(),
                            expected,
                            "{kind} as {class:?}, worker {worker}, {role:?}: {result:?}"
                        );
                        if let Err(refused) = result {
                            assert_eq!(refused.code, Code::NotAllowed);
                        }
                        tried += 1;
                    }
                }
            }
        }
        assert_eq!(tried, 16 * 4 * 2 * 3);
    }

    #[test]
    fn the_owner_has_the_role_of_an_admin_and_an_admin_the_role_of_a_member() {
        assert!(Role::Owner > Role::Admin && Role::Admin > Role::Member);
        let owner = caller(Class::Person).with_role(Role::Owner);
        assert!(permits(CommandKind::SetIdle, &owner, Role::Admin).is_ok());
        let member = caller(Class::Person);
        let refused = permits(CommandKind::SetIdle, &member, Role::Admin).unwrap_err();
        assert!(refused.reason.contains("ann is not an admin"), "{refused}");
    }

    /// A pause of the whole riff, or of a repository by its name, needs
    /// an admin (01M3XAHZDSQR263QZVB41CK0MX).
    #[test]
    fn a_pause_of_the_whole_riff_or_of_a_named_repository_needs_an_admin() {
        use riff_core::wire::{Pause, Resume};

        let me: SessionUri = "riff://ann@heron/acme/app?session=a1".parse().unwrap();
        let pause = |riff, repository: Option<&str>| Pause {
            me: me.clone(),
            riff,
            repository: repository.map(|r| r.parse().unwrap()),
        };
        assert_eq!(pause(false, None).needs(), Role::Member);
        assert_eq!(pause(true, None).needs(), Role::Admin);
        assert_eq!(pause(false, Some("acme/lib")).needs(), Role::Admin);
        assert_eq!(Resume::whole(me.clone()).needs(), Role::Admin);
        assert_eq!(Resume::here(me.clone()).needs(), Role::Member);

        let member = caller(Class::Session);
        let refused = permits(CommandKind::Pause, &member, Role::Admin).unwrap_err();
        assert_eq!(refused.code, Code::NotAllowed);
        assert!(refused.reason.contains("ann is not an admin"), "{refused}");
        let admin = member.with_role(Role::Admin);
        assert!(permits(CommandKind::Pause, &admin, Role::Admin).is_ok());
    }

    #[test]
    fn each_code_has_a_name_of_its_own() {
        let mut names: Vec<&str> = Code::ALL.iter().map(|code| code.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Code::ALL.len());
    }

    #[test]
    fn each_kind_has_a_name_of_its_own() {
        let mut names: Vec<&str> = CommandKind::ALL.iter().map(|k| k.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CommandKind::ALL.len());
    }
}
