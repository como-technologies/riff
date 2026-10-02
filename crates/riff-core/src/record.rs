//! The records of the log of `riff-server`.
//!
//! # Design
//!
//! Each change that must not be lost is a [`Record`] in one log
//! (01M3T410XDD9W4EC0Y68FAA7XN). A record
//! has a position (1, 2, 3, and so on), the time of its write, its
//! cause, and one [`Change`]. The change names say what happened, in the
//! past tense. These Rust types are the schema of the log.
//!
//! The cause is in the envelope: the caller ([`By`]) and the kind of
//! the command (01M3X4Z60G1FXQTDC5XDJ05BAX). So the log alone shows who made each
//! change. A record from before this rule has no cause: it reads, and
//! its cause is not known.
//!
//! A record names a session by its URI with no lead mark and no claims:
//! the who and the place at the time of the change
//! (01M3T411QW1SQV12RJVATEJ8YD). So a replay knows
//! where each session was.
//!
//! # The rules for a change of a record (01M3T4111PFM0C6KPREWFS9EQQ)
//!
//! - A new field has a default, and the default means "as before". An
//!   old build skips a field that it does not know.
//! - Do not change the type or the meaning of a field. Do not use the
//!   name of a removed field again.
//! - A new kind of change gets a new name. A build that does not know a
//!   kind skips the record ([`Line::Unknown`]).
//! - A kind is never renamed, and the name of a removed kind is never
//!   used again (01M3XM2C3MND6YB24SGZ565353). The file
//!   `crates/riff-server/tests/fixtures/1.0.0/kinds.json` lists the
//!   kinds of the release, and a test fails when a name of the list is
//!   gone from the code.
//! - The enum [`Change`] and the list [`Change::KINDS`] come from one
//!   macro. So a variant cannot be missing from the list. The order of
//!   the list is not a part of the format.
//! - A field with a set of named values that can grow has the value
//!   `other`: the class in [`By`], the [`Scope`] of a pause, and the
//!   reason of a start ([`StartReason`]). A build reads a value that it
//!   does not know as `other`: a text, and each other form of JSON.
//!   `apply` stores nothing for such a value. Such a record counts as a
//!   skipped record ([`Record::other`]): the build writes no checkpoint
//!   past it (01M3XM2C18TT8VSKGD77YPZG53). The `state` of a `pause_set`
//!   has two values and no `other`.
//! - `command` is text. A reader takes a kind of command that it does
//!   not know as text.
//!
//! # Example
//!
//! ```
//! use riff_core::name::Who;
//! use riff_core::record::{By, Change, Claimed, Line, Record};
//!
//! let record = Record {
//!     position: 1234,
//!     written_at_ms: 1_790_000_000_000,
//!     by: Some(By::Session(Who::new("ann", Some("s1"))?)),
//!     command: Some("claim".into()),
//!     change: Change::Claimed(Claimed {
//!         session: "riff://ann@heron/acme/app?session=s1".parse()?,
//!         thread: "acme/app".parse()?,
//!         item: "issue-7".into(),
//!     }),
//! };
//! let line = serde_json::to_string(&record).unwrap();
//! assert_eq!(
//!     line,
//!     r#"{"position":1234,"written_at_ms":1790000000000,"by":{"session":"ann/s1"},"command":"claim","change":{"claimed":{"session":"riff://ann@heron/acme/app?session=s1","thread":"acme/app","item":"issue-7"}}}"#
//! );
//! assert_eq!(Line::parse(&line)?, Line::Record(Box::new(record)));
//!
//! // A record with no cause reads. Its cause is not known.
//! let old = r#"{"position":7,"written_at_ms":1,"change":{"member_invited":{"email":"ann@acme.io"}}}"#;
//! let Line::Record(old) = Line::parse(old)? else { panic!("a known kind") };
//! assert_eq!((old.by, old.command), (None, None));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::name::{SessionUri, ThreadName, Who};
use crate::wire::{Idle, Message, RiffState, StartReason};

/// Makes the enum [`Change`], the list [`Change::KINDS`] and
/// [`Change::kind`] from one list of variants. Each variant gives its
/// name in the JSON of a record. So a variant cannot be missing from the
/// list of the kinds.
macro_rules! changes {
    ($($(#[$doc:meta])* $variant:ident($body:ty) = $kind:literal,)*) => {
        /// What happened.
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub enum Change {
            $($(#[$doc])* #[serde(rename = $kind)] $variant($body),)*
        }

        impl Change {
            /// The name of each kind of change, as the JSON of a record
            /// has it. The order of the list is not a part of the
            /// format: a reader finds a kind by its name. So a new kind
            /// can go in at any place.
            pub const KINDS: &[&str] = &[$($kind),*];

            /// The name of the kind of this change.
            pub fn kind(&self) -> &'static str {
                match self {
                    $(Change::$variant(_) => $kind,)*
                }
            }
        }
    };
}

// ANCHOR: record
/// One line of the log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The place of the record in the one log of the riff: 1, 2, 3, and
    /// so on.
    pub position: u64,
    /// The time when the server made the record, in milliseconds since
    /// the Unix epoch.
    pub written_at_ms: u64,
    /// The caller of the command that made the record. `None` in a
    /// record from before this field: the cause is not known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<By>,
    /// The kind of the command that made the record, for example
    /// `claim`. It is text: a reader takes a kind that it does not know
    /// as text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub change: Change,
}

changes! {
    /// A message, with its seq in its thread and the sessions that it
    /// woke.
    Posted(Box<Posted>) = "posted",
    /// A session joined a thread.
    JoinedThread(Member) = "joined_thread",
    /// A session left a thread. It is no longer the lead there.
    LeftThread(Member) = "left_thread",
    /// A session took a claim. It replaces the old holder.
    Claimed(Claimed) = "claimed",
    /// A claim is free. The record of the last claim of a worker, made
    /// by its own release, says that the worker must clear its context.
    Released(Released) = "released",
    /// A session became the lead of its user in a repository thread.
    LeadSet(Member) = "lead_set",
    /// A setting of the riff changed.
    SettingChanged(SettingChanged) = "setting_changed",
    /// A session had no sign of life for `SESSION_EXPIRY`. The state
    /// drops it: its read cursors, its memberships, its claims, its lead,
    /// and each direct thread whose two sessions are gone.
    SessionForgotten(Forgotten) = "session_forgotten",
    /// A session started, or came with no new start. The record has the
    /// worker mark of the session (01M3X9X9M079WGFPJZHNXH9VEP).
    SessionStarted(SessionStarted) = "session_started",
    /// A pause is set or ended: the pause of the whole riff, or the
    /// pause of one repository (01M3XAHZG26ECNARX35JD73YXJ).
    PauseSet(PauseSet) = "pause_set",
    /// The riff has its ID. It is the first record of a new log.
    RiffMade(RiffMade) = "riff_made",
    /// An email signed in for the first time, and holds its USER.
    PersonJoined(PersonJoined) = "person_joined",
    /// An email is a member of the riff.
    MemberInvited(Email) = "member_invited",
    /// An email is no member of the riff. Each sign-in of its USER from
    /// before this record is ended.
    MemberRemoved(Email) = "member_removed",
    /// An email is an admin that the owner made, or it is not.
    AdminSet(AdminSet) = "admin_set",
    /// An email is the owner. With no email, the owner is gone, and the
    /// riff has no owner. The request for the owner role ends.
    OwnerSet(OwnerSet) = "owner_set",
    /// An admin asks for the owner role.
    OwnerAsked(OwnerAsked) = "owner_asked",
    /// The owner keeps the owner role that an admin asked for.
    OwnerDenied(Email) = "owner_denied",
    /// Each sign-in of a USER from before this record is ended.
    SigninsEnded(SigninsEnded) = "signins_ended",
}
// ANCHOR_END: record

/// Who caused a record: the caller of its command, with its class
/// (01M3X4Z60G1FXQTDC5XDJ05BAX). It holds only the user and the session ID. The session
/// in a change is a full URI, with the place.
///
/// | Caller | JSON |
/// |---|---|
/// | a person | `{"person":"mike"}` |
/// | a session | `{"session":"mike/a6cf"}` |
/// | a sign-in | `{"sign_in":"mike@comotechnologies.io"}` |
/// | the server | `"server"` |
///
/// The set of classes can grow. A build reads a class that it does not
/// know as [`By::Other`].
///
/// ```
/// use riff_core::name::Who;
/// use riff_core::record::By;
///
/// let session = By::Session(Who::new("mike", Some("a6cf"))?);
/// assert_eq!(serde_json::to_string(&session).unwrap(), r#"{"session":"mike/a6cf"}"#);
/// assert_eq!(serde_json::to_string(&By::Server).unwrap(), r#""server""#);
/// assert_eq!(session.to_string(), "the session mike/a6cf");
///
/// let read = |json: &str| serde_json::from_str::<By>(json).unwrap();
/// assert_eq!(read(r#"{"person":"mike"}"#), By::Person("mike".into()));
/// assert_eq!(read(r#"{"session":"mike/a6cf"}"#), session);
/// assert_eq!(read(r#""server""#), By::Server);
/// // A class of a later build.
/// assert_eq!(read(r#"{"robot":"r2"}"#), By::Other);
/// assert_eq!(read(r#""cron""#), By::Other);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum By {
    /// A person, by the user.
    Person(String),
    /// An agent session, by the user and the session ID.
    Session(Who),
    /// A verified email of the provider, before a token is there.
    SignIn(String),
    /// A timer of `riff-server`.
    Server,
    /// A class that this build does not know.
    Other,
}

impl By {
    const PERSON: &str = "person";
    const SESSION: &str = "session";
    const SIGN_IN: &str = "sign_in";
    const SERVER: &str = "server";
    const OTHER: &str = "other";

    /// The class and the name as a JSON value: the form in a record and
    /// in a log line.
    pub fn json(&self) -> serde_json::Value {
        let named = |class: &str, name: String| serde_json::json!({ class: name });
        match self {
            By::Person(user) => named(By::PERSON, user.clone()),
            By::Session(who) => named(By::SESSION, who.to_string()),
            By::SignIn(email) => named(By::SIGN_IN, email.clone()),
            By::Server => By::SERVER.into(),
            By::Other => By::OTHER.into(),
        }
    }

    fn read(value: &serde_json::Value) -> By {
        if let Some(text) = value.as_str() {
            return if text == By::SERVER {
                By::Server
            } else {
                By::Other
            };
        }
        let Some(map) = value.as_object() else {
            return By::Other;
        };
        let mut fields = map.iter();
        let (Some((class, name)), None) = (fields.next(), fields.next()) else {
            return By::Other;
        };
        let Some(name) = name.as_str() else {
            return By::Other;
        };
        match class.as_str() {
            By::PERSON => By::Person(name.to_owned()),
            By::SESSION => name
                .split_once('/')
                .and_then(|(user, session)| Who::new(user, Some(session)).ok())
                .map_or(By::Other, By::Session),
            By::SIGN_IN => By::SignIn(name.to_owned()),
            _ => By::Other,
        }
    }
}

/// The caller for people, for example `the session mike/a6cf`.
impl std::fmt::Display for By {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            By::Person(user) => write!(f, "the person {user}"),
            By::Session(who) => write!(f, "the session {who}"),
            By::SignIn(email) => write!(f, "the sign-in {email}"),
            By::Server => f.write_str("the server"),
            By::Other => f.write_str("a caller of a class that this build does not know"),
        }
    }
}

/// The schema takes each value: [`By`] reads a class that it does not
/// know as [`By::Other`].
impl schemars::JsonSchema for By {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "By".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({})
    }
}

impl Serialize for By {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.json().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for By {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<By, D::Error> {
        Ok(By::read(&serde_json::Value::deserialize(deserializer)?))
    }
}

impl Record {
    /// The field of this record whose value this build read as `other`:
    /// `by`, `scope` or `reason`. `None` when the build knows each
    /// value. A record with such a value counts as a skipped record:
    /// the build writes no checkpoint past it (01M3XM2C18TT8VSKGD77YPZG53).
    ///
    /// ```
    /// use riff_core::record::{Change, Line};
    ///
    /// let read = |line: &str| match Line::parse(line).unwrap() {
    ///     Line::Record(record) => record,
    ///     Line::Unknown { .. } => panic!("a known kind"),
    /// };
    /// let known = r#"{"position":2,"written_at_ms":1,"by":{"person":"ann"},"command":"pause","change":{"pause_set":{"scope":"riff","state":"paused"}}}"#;
    /// assert_eq!(read(known).other(), None);
    /// assert_eq!(read(known).change.kind(), "pause_set");
    /// assert!(Change::KINDS.contains(&"pause_set"));
    /// // The values of a later build.
    /// assert_eq!(read(&known.replace(r#""riff""#, r#"{"wave":"17"}"#)).other(), Some("scope"));
    /// assert_eq!(read(&known.replace(r#""riff""#, r#""host""#)).other(), Some("scope"));
    /// assert_eq!(read(&known.replace("person", "robot")).other(), Some("by"));
    /// // The state of a pause has no `other`: the line does not read.
    /// assert!(Line::parse(&known.replace(r#""paused""#, r#""slow""#)).is_err());
    /// ```
    pub fn other(&self) -> Option<&'static str> {
        if self.by == Some(By::Other) {
            return Some("by");
        }
        match &self.change {
            Change::PauseSet(set) if set.scope == Scope::Other => Some("scope"),
            Change::SessionStarted(started) if started.reason == StartReason::Other => {
                Some("reason")
            }
            _ => None,
        }
    }
}

/// A message in a thread.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Posted {
    pub thread: ThreadName,
    /// The message, with its seq. A signed message keeps its payload and
    /// its signature unchanged.
    pub message: Message,
    /// Each session that the message woke.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub woken: BTreeSet<Who>,
}

/// A session and a thread.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub session: SessionUri,
    pub thread: ThreadName,
}

/// A claim of one work item in one thread.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claimed {
    pub session: SessionUri,
    pub thread: ThreadName,
    pub item: String,
}

/// A claim that is free now.
///
/// ```
/// use riff_core::record::{Claimed, Released};
///
/// let claim = Claimed {
///     session: "riff://ann@heron/acme/app?session=s1".parse()?,
///     thread: "acme/app".parse()?,
///     item: "issue-7".into(),
/// };
/// let released = Released::of(claim.clone());
/// let json = serde_json::to_string(&released).unwrap();
/// // A record from before the mark has the same fields.
/// assert_eq!(json, serde_json::to_string(&claim).unwrap());
/// assert!(!serde_json::from_str::<Released>(&json).unwrap().must_clear);
///
/// let last = Released { must_clear: true, ..released };
/// assert!(serde_json::to_string(&last).unwrap().ends_with(r#""must_clear":true}"#));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Released {
    pub session: SessionUri,
    pub thread: ThreadName,
    pub item: String,
    /// True when the session is a worker that released its last claim
    /// itself: it must clear its context before its next claim
    /// (01M3X9XAK1KPZZVM1AJR2H8DSS).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub must_clear: bool,
}

impl Released {
    /// The release of `claim`, with no ask to clear.
    pub fn of(claim: Claimed) -> Released {
        Released {
            session: claim.session,
            thread: claim.thread,
            item: claim.item,
            must_clear: false,
        }
    }
}

/// A start of a session.
///
/// ```
/// use riff_core::record::{Change, Line};
/// use riff_core::wire::StartReason;
///
/// let line = r#"{"position":4,"written_at_ms":9,"change":{"session_started":{"session":"riff://ann@heron/acme/app?session=s1","reason":"clear","worker":true}}}"#;
/// let Line::Record(record) = Line::parse(line)? else { panic!("a known kind") };
/// let Change::SessionStarted(started) = &record.change else { panic!("a start") };
/// assert!(started.worker && started.reason.is_fresh());
/// assert_eq!(serde_json::to_string(&record).unwrap(), line);
///
/// // A reason of a later build reads as `other`. It is no fresh start.
/// let later = line.replace("clear", "wake");
/// let Line::Record(record) = Line::parse(&later)? else { panic!("a known kind") };
/// let Change::SessionStarted(started) = &record.change else { panic!("a start") };
/// assert_eq!(started.reason, StartReason::Other);
/// assert!(!started.reason.is_fresh());
/// # Ok::<(), String>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStarted {
    pub session: SessionUri,
    /// Why the record is there. `process` and `clear` are fresh starts.
    pub reason: StartReason,
    /// True when the session is a worker.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub worker: bool,
}

/// A pause that a command set or ended. The envelope of the record
/// names who did it, and when.
///
/// ```
/// use riff_core::record::{Change, PauseSet, Scope};
/// use riff_core::wire::RiffState;
///
/// let set = Change::PauseSet(PauseSet {
///     scope: Scope::Repository("como-technologies/strata".parse()?),
///     state: RiffState::Paused,
/// });
/// assert_eq!(
///     serde_json::to_string(&set).unwrap(),
///     r#"{"pause_set":{"scope":{"repository":"como-technologies/strata"},"state":"paused"}}"#
/// );
/// let riff = PauseSet { scope: Scope::Riff, state: RiffState::Running };
/// assert_eq!(serde_json::to_string(&riff).unwrap(), r#"{"scope":"riff","state":"running"}"#);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PauseSet {
    pub scope: Scope,
    /// `paused` sets the pause, and `running` ends it.
    pub state: RiffState,
}

/// What a pause stops: the whole riff, or one repository
/// (01M3XAHZG26ECNARX35JD73YXJ).
///
/// | Scope | JSON |
/// |---|---|
/// | the whole riff | `"riff"` |
/// | a repository | `{"repository":"como-technologies/strata"}` |
///
/// The set of scopes can grow. A build reads a scope that it does not
/// know as [`Scope::Other`], and such a record changes no pause.
///
/// ```
/// use riff_core::record::Scope;
///
/// let read = |json: &str| serde_json::from_str::<Scope>(json).unwrap();
/// assert_eq!(read(r#""riff""#), Scope::Riff);
/// assert_eq!(read(r#"{"repository":"acme/app"}"#), Scope::Repository("acme/app".parse()?));
/// // A scope of a later build.
/// assert_eq!(read(r#"{"wave":"17"}"#), Scope::Other);
/// assert_eq!(read(r#""host""#), Scope::Other);
/// assert_eq!(Scope::Riff.to_string(), "the riff");
/// assert_eq!(read(r#"{"repository":"acme/app"}"#).to_string(), "the repository acme/app");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The whole riff.
    Riff,
    /// One repository, by its thread.
    Repository(ThreadName),
    /// A scope that this build does not know.
    Other,
}

impl Scope {
    const RIFF: &str = "riff";
    const REPOSITORY: &str = "repository";
    const OTHER: &str = "other";

    fn read(value: &serde_json::Value) -> Scope {
        if let Some(text) = value.as_str() {
            return if text == Scope::RIFF {
                Scope::Riff
            } else {
                Scope::Other
            };
        }
        let Some(map) = value.as_object() else {
            return Scope::Other;
        };
        let mut fields = map.iter();
        let (Some((kind, name)), None) = (fields.next(), fields.next()) else {
            return Scope::Other;
        };
        match (kind.as_str(), name.as_str()) {
            (Scope::REPOSITORY, Some(name)) => {
                ThreadName::try_from(name.to_owned()).map_or(Scope::Other, Scope::Repository)
            }
            _ => Scope::Other,
        }
    }
}

/// The scope for people, for example `the repository acme/app`.
impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::Riff => f.write_str("the riff"),
            Scope::Repository(thread) => write!(f, "the repository {thread}"),
            Scope::Other => f.write_str("a scope that this build does not know"),
        }
    }
}

impl Serialize for Scope {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Scope::Riff => Scope::RIFF.serialize(serializer),
            Scope::Repository(thread) => {
                serde_json::json!({ Scope::REPOSITORY: thread }).serialize(serializer)
            }
            Scope::Other => Scope::OTHER.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Scope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Scope, D::Error> {
        Ok(Scope::read(&serde_json::Value::deserialize(deserializer)?))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingChanged {
    /// The settings of idle workers.
    pub idle: Idle,
}

/// A session that the state forgets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forgotten {
    pub session: SessionUri,
}

/// The ID of a riff (01M3JNVBPMZ1K9WX7Q7DP6Y0DH).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiffMade {
    pub riff_id: String,
}

/// The first sign-in of a person: the verified email holds the USER
/// (R209).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonJoined {
    pub user: String,
    /// The verified email, in lower case.
    pub email: String,
}

/// A person, by the verified email in lower case. A record of the
/// people names a person by the email: the state finds the USER.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Email {
    pub email: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminSet {
    pub email: String,
    /// True: the person is an admin. False: the person is a member
    /// again.
    pub admin: bool,
}

/// The owner of the riff.
///
/// ```
/// use riff_core::record::OwnerSet;
///
/// let gone = OwnerSet { email: None };
/// assert_eq!(serde_json::to_string(&gone).unwrap(), "{}");
/// assert_eq!(serde_json::from_str::<OwnerSet>("{}").unwrap(), gone);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerSet {
    /// The email of the owner. `None`: the owner is gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// A request for the owner role.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerAsked {
    /// The email of the admin that asks.
    pub email: String,
    /// With no answer before this time, the admin is the owner. In
    /// milliseconds since the Unix epoch.
    pub due_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SigninsEnded {
    pub user: String,
}

/// One line of the log, as this build reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Record(Box<Record>),
    /// A record of a kind that this build does not know. A replay skips
    /// it, and logs a warning with its position.
    Unknown {
        position: u64,
        kind: String,
    },
}

impl Line {
    /// Reads one line of the log. A record of a kind that this build does
    /// not know is [`Line::Unknown`]. A line that does not read is an
    /// error.
    ///
    /// ```
    /// use riff_core::record::Line;
    ///
    /// let later = r#"{"position":9,"written_at_ms":1,"change":{"reacted":{"emoji":"+1"}}}"#;
    /// assert_eq!(
    ///     Line::parse(later).unwrap(),
    ///     Line::Unknown { position: 9, kind: "reacted".into() }
    /// );
    /// assert!(Line::parse(r#"{"position":9}"#).is_err());
    /// assert!(Line::parse(r#"{"position":9,"written_at_ms":1,"change":{"claimed":{}}}"#).is_err());
    /// ```
    pub fn parse(line: &str) -> Result<Line, String> {
        #[derive(Deserialize)]
        struct Raw {
            position: u64,
            written_at_ms: u64,
            #[serde(default)]
            by: Option<By>,
            #[serde(default)]
            command: Option<String>,
            change: serde_json::Map<String, serde_json::Value>,
        }
        let raw: Raw = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let [kind] = raw.change.keys().collect::<Vec<_>>()[..] else {
            return Err(format!(
                "the change of the record at position {} needs one kind",
                raw.position
            ));
        };
        if !Change::KINDS.contains(&kind.as_str()) {
            return Ok(Line::Unknown {
                position: raw.position,
                kind: kind.clone(),
            });
        }
        let change = serde_json::from_value(serde_json::Value::Object(raw.change))
            .map_err(|e| format!("the record at position {}: {e}", raw.position))?;
        Ok(Line::Record(Box::new(Record {
            position: raw.position,
            written_at_ms: raw.written_at_ms,
            by: raw.by,
            command: raw.command,
            change,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::Kind;

    fn uri() -> SessionUri {
        "riff://ann@heron/acme/app?session=s1".parse().unwrap()
    }

    fn thread() -> ThreadName {
        "acme/app".parse().unwrap()
    }

    fn one_of_each() -> Vec<Change> {
        let member = Member {
            session: uri(),
            thread: thread(),
        };
        let claim = Claimed {
            session: uri(),
            thread: thread(),
            item: "issue-7".into(),
        };
        let email = Email {
            email: "ann@acme.io".into(),
        };
        vec![
            Change::Posted(Box::new(Posted {
                thread: thread(),
                message: Message {
                    seq: 1,
                    from: uri(),
                    to: vec![],
                    body: "hi".into(),
                    at_ms: 5,
                    kind: Kind::Message,
                    sig: Some("h..s".into()),
                    payload: Some("cA".into()),
                },
                woken: BTreeSet::from([uri().who().clone()]),
            })),
            Change::JoinedThread(member.clone()),
            Change::LeftThread(member.clone()),
            Change::Claimed(claim.clone()),
            Change::Released(Released {
                must_clear: true,
                ..Released::of(claim)
            }),
            Change::LeadSet(member),
            Change::SettingChanged(SettingChanged {
                idle: Idle::default(),
            }),
            Change::SessionForgotten(Forgotten { session: uri() }),
            Change::SessionStarted(SessionStarted {
                session: uri(),
                reason: StartReason::Process,
                worker: true,
            }),
            Change::PauseSet(PauseSet {
                scope: Scope::Repository(thread()),
                state: RiffState::Paused,
            }),
            Change::RiffMade(RiffMade {
                riff_id: "r1".into(),
            }),
            Change::PersonJoined(PersonJoined {
                user: "ann".into(),
                email: email.email.clone(),
            }),
            Change::MemberInvited(email.clone()),
            Change::MemberRemoved(email.clone()),
            Change::AdminSet(AdminSet {
                email: email.email.clone(),
                admin: true,
            }),
            Change::OwnerSet(OwnerSet { email: None }),
            Change::OwnerAsked(OwnerAsked {
                email: email.email.clone(),
                due_ms: 9,
            }),
            Change::OwnerDenied(email),
            Change::SigninsEnded(SigninsEnded { user: "ann".into() }),
        ]
    }

    #[test]
    fn each_kind_reads_back_and_has_its_name() {
        let changes = one_of_each();
        let kinds: BTreeSet<&str> = changes.iter().map(Change::kind).collect();
        assert_eq!(kinds, BTreeSet::from_iter(Change::KINDS.iter().copied()));
        assert_eq!(
            kinds.len(),
            Change::KINDS.len(),
            "a name is in the list two times"
        );
        for change in changes {
            let kind = change.kind();
            let record = Record {
                position: 3,
                written_at_ms: 4,
                by: Some(By::Server),
                command: Some("forget".into()),
                change,
            };
            let line = serde_json::to_string(&record).unwrap();
            assert!(line.contains(&format!(r#""change":{{"{kind}":"#)), "{line}");
            assert_eq!(Line::parse(&line).unwrap(), Line::Record(Box::new(record)));
        }
    }

    #[test]
    fn an_unknown_field_is_skipped() {
        let line = r#"{"position":2,"written_at_ms":1,"later":true,"change":{"pause_set":{"scope":"riff","state":"running","why":"x"}}}"#;
        let Line::Record(record) = Line::parse(line).unwrap() else {
            panic!("a known kind");
        };
        assert_eq!(
            record.change,
            Change::PauseSet(PauseSet {
                scope: Scope::Riff,
                state: RiffState::Running
            })
        );
        assert_eq!(record.other(), None);
    }

    #[test]
    fn each_class_of_a_caller_reads_back() {
        let who = uri().who().clone();
        for (by, json) in [
            (By::Person("ann".into()), r#"{"person":"ann"}"#),
            (By::Session(who), r#"{"session":"ann/s1"}"#),
            (
                By::SignIn("ann@acme.io".into()),
                r#"{"sign_in":"ann@acme.io"}"#,
            ),
            (By::Server, r#""server""#),
            (By::Other, r#""other""#),
        ] {
            assert_eq!(serde_json::to_string(&by).unwrap(), json);
            assert_eq!(serde_json::from_str::<By>(json).unwrap(), by);
        }
    }

    #[test]
    fn a_class_that_the_build_does_not_know_reads_as_other() {
        for json in [
            r#"{"robot":"r2"}"#,
            r#""cron""#,
            r#"{"person":"ann","more":"x"}"#,
            r#"{"person":7}"#,
            r#"{"session":"no-session-id"}"#,
            "7",
            "{}",
        ] {
            assert_eq!(
                serde_json::from_str::<By>(json).unwrap(),
                By::Other,
                "{json}"
            );
        }
        let line = r#"{"position":2,"written_at_ms":1,"by":{"robot":"r2"},"command":"sweep","change":{"pause_set":{"scope":"riff","state":"running"}}}"#;
        let Line::Record(record) = Line::parse(line).unwrap() else {
            panic!("a known kind");
        };
        assert_eq!(record.by, Some(By::Other));
        assert_eq!(record.other(), Some("by"));
        // The kind of a command is text: a kind of a later build reads.
        assert_eq!(record.command.as_deref(), Some("sweep"));
    }

    #[test]
    fn each_scope_of_a_pause_reads_back() {
        for (scope, json) in [
            (Scope::Riff, r#""riff""#),
            (Scope::Repository(thread()), r#"{"repository":"acme/app"}"#),
            (Scope::Other, r#""other""#),
        ] {
            assert_eq!(serde_json::to_string(&scope).unwrap(), json);
            assert_eq!(serde_json::from_str::<Scope>(json).unwrap(), scope);
        }
    }

    #[test]
    fn a_scope_that_the_build_does_not_know_reads_as_other() {
        for json in [
            r#"{"wave":"17"}"#,
            r#""host""#,
            r#"{"repository":"acme/app","more":"x"}"#,
            r#"{"repository":7}"#,
            r#"{"repository":"two words"}"#,
            "7",
            "{}",
        ] {
            assert_eq!(
                serde_json::from_str::<Scope>(json).unwrap(),
                Scope::Other,
                "{json}"
            );
        }
        let line = r#"{"position":2,"written_at_ms":1,"by":{"person":"ann"},"command":"pause","change":{"pause_set":{"scope":{"wave":"17"},"state":"paused"}}}"#;
        let Line::Record(record) = Line::parse(line).unwrap() else {
            panic!("a known kind");
        };
        assert_eq!(
            record.change,
            Change::PauseSet(PauseSet {
                scope: Scope::Other,
                state: RiffState::Paused
            })
        );
        assert_eq!(record.other(), Some("scope"));
    }

    #[test]
    fn a_reason_that_the_build_does_not_know_reads_as_other() {
        let line = r#"{"position":2,"written_at_ms":1,"change":{"session_started":{"session":"riff://ann@heron/acme/app?session=s1","reason":"wake"}}}"#;
        let Line::Record(record) = Line::parse(line).unwrap() else {
            panic!("a known kind");
        };
        assert_eq!(record.other(), Some("reason"));
        let known = line.replace("wake", "join");
        let Line::Record(record) = Line::parse(&known).unwrap() else {
            panic!("a known kind");
        };
        assert_eq!(record.other(), None);
        // A reason of a later build can have each form of JSON.
        for later in [r#"{"wake":"timer"}"#, "7", "null", r#"["join"]"#] {
            let line = line.replace(r#""wake""#, later);
            let Line::Record(record) = Line::parse(&line).unwrap() else {
                panic!("a known kind");
            };
            assert_eq!(record.other(), Some("reason"), "{later}");
        }
    }

    #[test]
    fn a_state_of_a_pause_that_the_build_does_not_know_does_not_read() {
        let line = r#"{"position":2,"written_at_ms":1,"change":{"pause_set":{"scope":"riff","state":"slow"}}}"#;
        assert!(Line::parse(line).is_err());
    }

    #[test]
    fn a_record_of_the_kind_riff_state_set_is_of_no_kind_of_this_build() {
        let line =
            r#"{"position":2,"written_at_ms":1,"change":{"riff_state_set":{"state":"running"}}}"#;
        assert_eq!(
            Line::parse(line).unwrap(),
            Line::Unknown {
                position: 2,
                kind: "riff_state_set".into()
            }
        );
    }

    #[test]
    fn a_record_with_no_cause_reads_and_writes_no_cause() {
        let line = r#"{"position":2,"written_at_ms":1,"change":{"pause_set":{"scope":"riff","state":"running"}}}"#;
        let Line::Record(record) = Line::parse(line).unwrap() else {
            panic!("a known kind");
        };
        assert_eq!((&record.by, &record.command), (&None, &None));
        assert_eq!(serde_json::to_string(&record).unwrap(), line);
    }

    #[test]
    fn a_change_with_two_kinds_does_not_read() {
        let line = r#"{"position":2,"written_at_ms":1,"change":{"claimed":{},"released":{}}}"#;
        assert!(Line::parse(line).is_err());
    }
}
