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
//! | the riff | [`super::the_riff`] | [`MakeRiff`](super::MakeRiff), [`Pause`](riff_core::wire::Pause), [`Resume`](riff_core::wire::Resume), [`SetIdle`](riff_core::wire::SetIdle), [`Forget`](super::Forget), [`Import`](super::Import) |
//! | people | [`super::people`] | [`Admit`](super::Admit), [`Invite`](riff_core::wire::Invite), [`Remove`](riff_core::wire::Remove), [`SetAdmin`](riff_core::wire::SetAdmin), [`PassOwner`](riff_core::wire::PassOwner), [`TakeOwner`](riff_core::wire::TakeOwner), [`DenyOwner`](riff_core::wire::DenyOwner), [`GrantOwner`](super::GrantOwner), [`EndOwner`](super::EndOwner), [`NameOwner`](super::NameOwner), [`Revoke`](riff_core::wire::Revoke) |
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
    /// A verified email of the provider, before a token is there. Only
    /// the command `admit` takes it.
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
    /// The verified email of a sign-in.
    email: Option<String>,
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
            email: None,
        }
    }

    /// A sign-in: the verified email `email` of the provider, which
    /// gives the USER `user`, before a token is there. The engine makes
    /// it for the command `admit`.
    ///
    /// ```
    /// use riff_core::name::Who;
    /// use riff_core::record::By;
    /// use riff_server::state::{Caller, Class};
    ///
    /// let caller = Caller::sign_in(&Who::new("mike", None)?, "mike@comotechnologies.io");
    /// assert_eq!(caller.class(), Class::SignIn);
    /// assert_eq!(caller.by(), By::SignIn("mike@comotechnologies.io".into()));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn sign_in(user: &Who, email: &str) -> Caller {
        let place = crate::owner::server_uri().place().clone();
        Caller {
            class: Class::SignIn,
            me: SessionUri::new(user.clone(), place),
            worker: false,
            role: Role::Member,
            email: Some(email.to_owned()),
        }
    }

    /// The server itself, for the command of a timer.
    pub fn server() -> Caller {
        Caller {
            class: Class::Server,
            me: crate::owner::server_uri(),
            worker: false,
            role: Role::Member,
            email: None,
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
            Class::SignIn => {
                By::SignIn(self.email.clone().unwrap_or_else(|| who.user().to_owned()))
            }
            Class::Server => By::Server,
        }
    }

    /// The caller as a log line names it (01M3XA87CJHCGZX283ZQAFKARZ): as
    /// [`Caller::by`], but a sign-in has its USER in the place of its
    /// email. An email is in a record, and in no log line.
    ///
    /// ```
    /// use riff_core::name::Who;
    /// use riff_core::record::By;
    /// use riff_server::state::Caller;
    ///
    /// let caller = Caller::sign_in(&Who::new("mike", None)?, "mike@comotechnologies.io");
    /// assert_eq!(caller.traced(), By::SignIn("mike".into()));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn traced(&self) -> By {
        match self.class {
            Class::SignIn => By::SignIn(self.who().user().to_owned()),
            _ => self.by(),
        }
    }
}

/// The word of the writer to a call: its entry in the queue is done.
/// [`Command::reply`] gets it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Done {
    /// The records of the command, which are in the log now.
    pub made: Vec<Record>,
    /// The number of sign-ins that the writer ended for the records of
    /// the command: the effect of a `member_removed` or a
    /// `signins_ended` record (01M3XA87A9GGFA89RQXWSKY0V6).
    pub ended: usize,
}

impl Done {
    /// The word for `made`, with no sign-in that ended.
    pub fn of(made: Vec<Record>) -> Done {
        Done { made, ended: 0 }
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

/// Makes the enum [`CommandKind`], the list [`CommandKind::ALL`] and
/// [`CommandKind::as_str`] from one list of kinds. So a kind cannot be
/// missing from the list.
macro_rules! command_kinds {
    ($($variant:ident = $name:literal,)*) => {
        /// The kind of a command: its name in a record and in a log
        /// line. A kind is never renamed, and the name of a removed kind
        /// is never used again (01M3XM2C3MND6YB24SGZ565353).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum CommandKind {
            $($variant,)*
        }

        impl CommandKind {
            /// Each kind of this build.
            pub const ALL: [CommandKind; [$($name),*].len()] = [$(CommandKind::$variant),*];

            /// The name of the kind.
            pub fn as_str(self) -> &'static str {
                match self {
                    $(CommandKind::$variant => $name,)*
                }
            }
        }
    };
}

command_kinds! {
    Register = "register",
    Start = "start",
    End = "end",
    Join = "join",
    Leave = "leave",
    Post = "post",
    Announce = "announce",
    Claim = "claim",
    Release = "release",
    ReleaseFor = "release_for",
    Lead = "lead",
    MakeRiff = "make_riff",
    Pause = "pause",
    Resume = "resume",
    SetIdle = "set_idle",
    Forget = "forget",
    Import = "import",
    Admit = "admit",
    Invite = "invite",
    Remove = "remove",
    SetAdmin = "set_admin",
    PassOwner = "pass_owner",
    TakeOwner = "take_owner",
    DenyOwner = "deny_owner",
    GrantOwner = "grant_owner",
    EndOwner = "end_owner",
    NameOwner = "name_owner",
    Revoke = "revoke",
}

impl CommandKind {
    /// True for a command of the group "people". A riff with no sign-in
    /// refuses each of them (01M3WRD9G5GAF65EX8P6D5DMQM), and the log
    /// line of each has no reason: the reason can name an email
    /// (01M3XA87CJHCGZX283ZQAFKARZ).
    ///
    /// ```
    /// use riff_server::state::CommandKind;
    ///
    /// assert!(CommandKind::Invite.of_people() && CommandKind::Admit.of_people());
    /// assert!(!CommandKind::Claim.of_people() && !CommandKind::MakeRiff.of_people());
    /// assert_eq!(CommandKind::ALL.iter().filter(|kind| kind.of_people()).count(), 11);
    /// ```
    pub fn of_people(self) -> bool {
        matches!(
            self,
            CommandKind::Admit
                | CommandKind::Invite
                | CommandKind::Remove
                | CommandKind::SetAdmin
                | CommandKind::PassOwner
                | CommandKind::TakeOwner
                | CommandKind::DenyOwner
                | CommandKind::GrantOwner
                | CommandKind::EndOwner
                | CommandKind::NameOwner
                | CommandKind::Revoke
        )
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
/// | `no_sign_in` | A command of the people in a riff with no sign-in. | 403 |
/// | `not_member` | The command names a person who is not a member. | 403 |
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
/// // A change of the idle workers needs a person who is an admin.
/// let person: SessionUri = "riff://mike@pangolin".parse()?;
/// let person = Caller::of(&person);
/// assert!(permits(CommandKind::SetIdle, &person, Role::Admin).is_err());
/// let admin = person.with_role(Role::Admin);
/// assert!(permits(CommandKind::SetIdle, &admin, Role::Admin).is_ok());
/// assert!(permits(CommandKind::SetIdle, &session.with_role(Role::Admin), Role::Admin).is_err());
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn permits(kind: CommandKind, caller: &Caller, needs: Role) -> Result<(), Refused> {
    use Class::{Person, Server, Session, SignIn};
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
        // Only a person changes the settings and the people.
        CommandKind::SetIdle
        | CommandKind::Invite
        | CommandKind::Remove
        | CommandKind::SetAdmin
        | CommandKind::PassOwner
        | CommandKind::TakeOwner
        | CommandKind::DenyOwner
        | CommandKind::Revoke => &[Person],
        CommandKind::Admit => &[SignIn],
        CommandKind::MakeRiff
        | CommandKind::Announce
        | CommandKind::Forget
        | CommandKind::Import
        | CommandKind::GrantOwner
        | CommandKind::EndOwner
        | CommandKind::NameOwner => &[Server],
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
    if matches!(class, Person | Session) && caller.role() < needs {
        let user = caller.who().user();
        let admin = |what: &str| format!("{user} is not an admin; only an admin {what}");
        let owner = |what: &str| format!("{user} is not the owner; only the owner {what}");
        let reason = match (kind, needs) {
            (CommandKind::SetIdle, _) => admin("can change the settings of idle workers"),
            (CommandKind::Pause | CommandKind::Resume, _) => format!(
                "{user} is not an admin; only the owner or an admin can {kind} the whole riff \
                 or a repository by its name"
            ),
            (CommandKind::Invite, _) => admin("can invite a person"),
            (CommandKind::Remove, _) => admin("can remove a person"),
            (CommandKind::TakeOwner, _) => admin("can take the owner role"),
            (CommandKind::Revoke, _) => admin("revokes another person"),
            (CommandKind::SetAdmin, _) => owner("adds or removes an admin"),
            (CommandKind::PassOwner, _) => owner("passes the owner role"),
            (CommandKind::DenyOwner, _) => owner("denies the owner role"),
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

    /// The role that the command of `caller` needs. [`permits`]
    /// compares it with the role of the caller. It reads the caller
    /// only to know if the command names the caller: a `revoke` of the
    /// own sign-ins needs a member, and a `revoke` of another person
    /// needs an admin.
    fn needs(&self, _caller: &Caller) -> Role {
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
    /// of the records of the command. `done` is the word of the writer:
    /// the records, and what their effects did.
    fn reply(
        &self,
        caller: &Caller,
        view: &View<'_>,
        done: &Done,
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
        use Class::{Person, Server, Session, SignIn};
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
            CommandKind::SetIdle
            | CommandKind::Invite
            | CommandKind::Remove
            | CommandKind::SetAdmin
            | CommandKind::PassOwner
            | CommandKind::TakeOwner
            | CommandKind::DenyOwner
            | CommandKind::Revoke => (&[Person], true),
            CommandKind::Admit => (&[SignIn], true),
            CommandKind::MakeRiff
            | CommandKind::Announce
            | CommandKind::Forget
            | CommandKind::Import
            | CommandKind::GrantOwner
            | CommandKind::EndOwner
            | CommandKind::NameOwner => (&[Server], true),
        }
    }

    /// The role that each command of the people needs, in the table of
    /// the design.
    #[test]
    fn each_command_of_the_people_needs_the_role_of_the_table() {
        let ann = caller(Class::Person);
        let expected = |kind| match kind {
            CommandKind::Invite | CommandKind::Remove | CommandKind::TakeOwner => Role::Admin,
            CommandKind::SetAdmin | CommandKind::PassOwner | CommandKind::DenyOwner => Role::Owner,
            _ => Role::Member,
        };
        for kind in CommandKind::ALL.into_iter().filter(|kind| kind.of_people()) {
            if kind == CommandKind::Revoke {
                // The own sign-ins: a member. Another person: an admin.
                assert_eq!(needs(kind, &ann), [Role::Member, Role::Member, Role::Admin]);
            } else {
                assert_eq!(needs(kind, &ann), [expected(kind)], "{kind}");
            }
        }
    }

    /// The role that each case of the command of `kind` needs for
    /// `caller`: [`Command::needs`] of one command of its type. So the
    /// test reads the role from the command, as the engine does. Only
    /// `revoke` has more than one case: the own sign-ins, by no name and
    /// by the own name, and the sign-ins of another person.
    fn needs(kind: CommandKind, caller: &Caller) -> Vec<Role> {
        use riff_core::wire::{
            Claim, DenyOwner, End, Invite, Join, Kind, Lead, Leave, PassOwner, Pause, Post,
            Register, Release, ReleaseFor, Remove, Resume, Revoke, SetAdmin, SetIdle, Start,
            StartReason, TakeOwner,
        };

        use crate::state::{
            Admit, Announce, EndOwner, Forget, GrantOwner, Import, MakeRiff, NameOwner,
        };

        let of = Needs(caller);
        let email = "bob@acme.io".to_owned();
        let me: SessionUri = "riff://ann@heron/acme/app?session=a1".parse().unwrap();
        let thread = me.default_thread().unwrap();
        let item = "issue-7".to_owned();
        let role = match kind {
            CommandKind::Register => of.of(Register { me, worker: false }, kind),
            CommandKind::Start => {
                let start = Start {
                    me,
                    reason: StartReason::Process,
                    worker: false,
                };
                of.of(start, kind)
            }
            CommandKind::End => of.of(End { me }, kind),
            CommandKind::Join => of.of(Join { me, thread }, kind),
            CommandKind::Leave => of.of(Leave { me, thread }, kind),
            CommandKind::Post => of.of(Post::new(&me, Some(thread), vec![], "hi"), kind),
            CommandKind::Announce => {
                let announce = Announce {
                    thread: Some(thread),
                    to: Vec::new(),
                    body: "hi".into(),
                    kind: Kind::Note,
                    at_ms: 0,
                };
                of.of(announce, kind)
            }
            CommandKind::Claim => of.of(Claim { me, thread, item }, kind),
            CommandKind::Release => of.of(Release { me, thread, item }, kind),
            CommandKind::ReleaseFor => {
                let session = "a2".to_owned();
                let release = ReleaseFor {
                    me,
                    thread,
                    item,
                    session,
                };
                of.of(release, kind)
            }
            CommandKind::Lead => of.of(Lead { me }, kind),
            CommandKind::MakeRiff => {
                let riff_id = "r1".to_owned();
                of.of(MakeRiff { riff_id }, kind)
            }
            CommandKind::Pause => of.of(Pause::here(me), kind),
            CommandKind::Resume => of.of(Resume::here(me), kind),
            CommandKind::SetIdle => {
                let set = SetIdle {
                    me,
                    per_host: Some(1),
                    after_secs: None,
                };
                of.of(set, kind)
            }
            CommandKind::Forget => of.of(Forget, kind),
            CommandKind::Import => of.of(Import { changes: vec![] }, kind),
            CommandKind::Admit => {
                let admit = Admit {
                    email,
                    allowed_domain: false,
                };
                of.of(admit, kind)
            }
            CommandKind::Invite => of.of(Invite { email }, kind),
            CommandKind::Remove => of.of(Remove { email }, kind),
            CommandKind::SetAdmin => of.of(SetAdmin { email, admin: true }, kind),
            CommandKind::PassOwner => of.of(PassOwner { email }, kind),
            CommandKind::TakeOwner => of.of(TakeOwner {}, kind),
            CommandKind::DenyOwner => of.of(DenyOwner {}, kind),
            CommandKind::GrantOwner => of.of(GrantOwner, kind),
            CommandKind::EndOwner => of.of(EndOwner, kind),
            CommandKind::NameOwner => of.of(NameOwner { email }, kind),
            CommandKind::Revoke => {
                let own = caller.who().user().to_uppercase();
                return [None, Some(own), Some("bob".to_owned())]
                    .into_iter()
                    .map(|user| of.of(Revoke { user }, kind))
                    .collect();
            }
        };
        vec![role]
    }

    /// Gives the role that a command needs for one caller.
    struct Needs<'a>(&'a Caller);

    impl Needs<'_> {
        fn of<C: Command>(&self, command: C, kind: CommandKind) -> Role {
            assert_eq!(C::KIND, kind);
            command.needs(self.0)
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
                        let has_role = matches!(class, Class::Person | Class::Session);
                        for needs in needs(kind, &caller) {
                            let expected = classes.contains(&class)
                                && (workers || !worker)
                                && (!has_role || role >= needs);
                            let result = permits(kind, &caller, needs);
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
        }
        // Each kind has one case, and `revoke` has 3.
        assert_eq!(tried, (28 + 2) * 4 * 2 * 3);
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
        let member = caller(Class::Session);
        assert_eq!(pause(false, None).needs(&member), Role::Member);
        assert_eq!(pause(true, None).needs(&member), Role::Admin);
        assert_eq!(pause(false, Some("acme/lib")).needs(&member), Role::Admin);
        assert_eq!(Resume::whole(me.clone()).needs(&member), Role::Admin);
        assert_eq!(Resume::here(me.clone()).needs(&member), Role::Member);

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
