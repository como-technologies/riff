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
//! The fields next to the change are the [`Envelope`]: one type that
//! [`Record`] and the tolerant reader [`Line::parse`] share. So a new
//! field of the envelope gets to the writer, the reader and each start.
//!
//! The cause is in the envelope: the caller ([`By`]) and the kind of
//! the command (01M3X4Z60G1FXQTDC5XDJ05BAX). So the log alone shows who made each
//! change. A record from before this rule has no cause: it reads, and
//! its cause is not known.
//!
//! The envelope also has the call ID of the command: the field `call`
//! (01M48VFFY5CK9MRXJESV2NHY5F). It came after release 1.0.0. A record
//! of a command with no call ID, and each record of 1.0.0, has none. A
//! build of 1.0.0 skips the field. `riff-server` uses it to give a
//! repeated call the reply of the first try, also after a start.
//!
//! # One layout for each type
//!
//! Each type of the log, the wire and the checkpoint has one struct
//! (01M49W17GGV1K5FZEKJRPV0GNA). A tolerant reader holds the struct
//! itself, as [`Line::parse`] holds the [`Envelope`] with
//! `#[serde(flatten)]`: it never keeps a second list of the fields.
//! Each conversion between two such types names each field of its
//! source in a destructure with no `..` (01M49W17M2JVNYSHDQJWHZ8A7X):
//! a field that the target does not need is named with `_`. So a new
//! field fails the build at each place that must carry it, and no
//! review must find it:
//!
//! ```compile_fail
//! use riff_core::record::Envelope;
//!
//! // A conversion that does not name the field `call` does not build.
//! fn written(envelope: Envelope) -> (u64, u64) {
//!     let Envelope { position, written_at_ms, by: _, command: _ } = envelope;
//!     (position, written_at_ms)
//! }
//! ```
//!
//! Each type has a round-trip test: each variant with each optional
//! field set, written and read back (01M49W18ETF4KZJN848M91VY35). The
//! records go through the real path of the store in
//! `riff-server/tests/calls.rs`.
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
//!   used again (01M3XM2C3MND6YB24SGZ565353). The file `kinds.json`
//!   in the directory of each release under
//!   `crates/riff-server/tests/fixtures/` lists the new kinds of the
//!   release (01M43GSRSDJMGAH8SR1GD4Z3XF). The file of 1.0.0 also
//!   lists each field of the envelope. A test fails when a name of a
//!   list is gone from the code.
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
//! - A `posted` record has two such parts too
//!   (01M3XSF90E9JYYTC13D9THY4WE): the kind of the message
//!   ([`Kind`](crate::wire::Kind)), and each selector of its `to`
//!   ([`Selector`]). A selector that the build does not know is a
//!   selector `other`: an object with a field or a value that the build
//!   does not know, and each other form of JSON. It keeps its JSON as
//!   it came, and it matches no session. The message stays in its
//!   thread, and a reader shows it as a message.
//! - A session URI keeps each query part that the build does not know,
//!   as it came ([`SessionUri::other`], 01M3XYYSY536AEJVERBPTQFQYX). The session is the same
//!   session, and the part gives no mark: no lead and no claim. Such a
//!   record counts as a skipped record too.
//! - `command` is text. A reader takes a kind of command that it does
//!   not know as text.
//! - Only the read of JSON takes a value of a later build. A call with
//!   such a kind, such a selector or such a URI of its caller is
//!   refused.
//!
//! # Each type with named values in a record
//!
//! The list has each enum that a record holds, and each struct that
//! refused a field that it does not know. A struct that is not in the
//! list skips such a field. A new type of these two sorts in a record
//! needs a line here.
//!
//! | Type | Where | A value that the build does not know |
//! |---|---|---|
//! | [`Change`] | `change` | The record is skipped ([`Line::Unknown`]). |
//! | [`By`] | `by` | `other` |
//! | [`Scope`] | `pause_set` | `other` |
//! | [`StartReason`] | `session_started` | `other` |
//! | [`Kind`](crate::wire::Kind) | the message of `posted` | `other` |
//! | [`Selector`] | the `to` of the message of `posted` | `other`: a new field, a new value of a field, and each other form of JSON |
//! | [`RiffState`] | `pause_set` | The line does not read. The set never grows: a pause is set or ended. A new sort of pause is a new [`Scope`]. |
//!
//! # Each text with a grammar in a record
//!
//! The list has each text that the read of a record checks or takes
//! apart, and each number. A text that is not in the list is free text:
//! the read takes each value. A new text with a grammar in a record
//! needs a line here.
//!
//! | Text | Where | A value that the build does not know |
//! |---|---|---|
//! | The query of a session URI ([`SessionUri`]) | `session` of a change, `from` of a message | The URI keeps the part, and the record counts as a skipped record: a part with a new name, `lead` with a value other than `true`, a second `session`, and a `claim` with a character that is not allowed. |
//! | The other parts of a session URI | the same | The line does not read. They never grow: `riff://USER@HOST`, then `/OWNER/REPO` or `/-`, then `#WORKTREE`, and each part holds only ASCII letters, digits, `-`, `_`, `.` and `~`. A new fact of a session is a new query part. |
//! | The session ID of a session URI | the `session` part of the query | The line does not read when the ID is empty or has a character that is not allowed. The grammar never grows: the ID is a name with only ASCII letters, digits, `-`, `_`, `.` and `~`, and a new fact of a session is a new query part. |
//! | A thread name ([`ThreadName`]) | `thread` | The line does not read. The grammar never grows: a text that is not empty and has no white space. Each new sort of thread name fits it, as `dm:` did. A name that starts with `dm:` reads with each character, also white space. The read does not take a name apart. |
//! | The caller in `by` | `{"session":"USER/ID"}` | `other` ([`By`]) |
//! | A session in `woken` ([`Who`]) | `posted` | The read checks no character. A field that the build does not know is skipped. |
//! | The signature and the payload of a message | `sig`, `payload` | The read keeps the text, and checks nothing. A reader checks them: a text that does not check gives a message that is not verified. |
//! | An item, an email, a user, the ID of a riff, a body, a command, the reason of a hold | `item`, `email`, `user`, `riff_id`, `body`, `command`, `reason` | Free text. |
//! | A time, a position, a seq, a count | `written_at_ms`, `at_ms`, `due_ms`, `position`, `seq`, `after_secs`, `per_host` | The line does not read. A number is a whole number that is not negative, and its type never changes (the rule of a field). `per_host` is at most 65535. |
//! | A mark | `worker`, `must_clear`, `admin` | The line does not read. A mark is `true` or `false`, and its type never changes. |
//!
//! The header of a chunk has the number of the format. A header of a
//! later format stops the load (01M3T411F3K6FD28R3Q3ZE4VCN): this is
//! the one sign that a build must not read the log.
//!
//! # Example
//!
//! ```
//! use riff_core::name::Who;
//! use riff_core::record::{By, Change, Claimed, Envelope, Line, Record};
//!
//! let record = Record {
//!     envelope: Envelope {
//!         position: 1234,
//!         written_at_ms: 1_790_000_000_000,
//!         by: Some(By::Session(Who::new("ann", Some("s1"))?)),
//!         command: Some("claim".into()),
//!         call: None,
//!     },
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
//! assert_eq!((old.envelope.by, old.envelope.command), (None, None));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::name::{SessionUri, ThreadName, Who};
use crate::selector::Selector;
use crate::wire::{Idle, Message, RiffState, StartReason};

/// Makes the enum [`Change`], the list [`Change::KINDS`] and
/// [`Change::kind`] from one list of variants. Each variant gives its
/// name in the JSON of a record. So a variant cannot be missing from the
/// list of the kinds.
macro_rules! changes {
    ($($(#[$doc:meta])* $variant:ident($body:ty) = $kind:literal,)*) => {
        /// What happened.
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
/// One line of the log: its envelope and its change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Record {
    #[serde(flatten)]
    pub envelope: Envelope,
    pub change: Change,
}

/// The fields of a record next to its change. See the design of the
/// module.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Envelope {
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
    /// The call ID of the command that made the record: the header
    /// `riff-call` of the call (01M48VFFY5CK9MRXJESV2NHY5F). `None` in a
    /// record of a command with no call ID, and in a record from before
    /// this field. A build that does not know the field skips it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<String>,
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
    /// A lead holds an item, with a reason: no worker can claim it. A
    /// second record for the same item replaces the reason. New in
    /// 1.1.0.
    ItemHeld(ItemHeld) = "item_held",
    /// The hold of an item ends. New in 1.1.0.
    ItemFreed(ItemFreed) = "item_freed",
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

impl Change {
    /// The session URI that this change holds: the `session` of the
    /// change, or the `from` of the message of a `posted` change. `None`
    /// for a change with no session URI.
    pub fn session(&self) -> Option<&SessionUri> {
        match self {
            Change::Posted(posted) => Some(&posted.message.from),
            Change::JoinedThread(m) | Change::LeftThread(m) | Change::LeadSet(m) => {
                Some(&m.session)
            }
            Change::Claimed(c) => Some(&c.session),
            Change::Released(r) => Some(&r.session),
            Change::SessionForgotten(f) => Some(&f.session),
            Change::SessionStarted(s) => Some(&s.session),
            Change::SettingChanged(_)
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
}

impl Record {
    /// The field of this record whose value this build read as `other`:
    /// `by`, `scope`, `reason`, the `kind` or the `to` of a message, or
    /// a session URI (`session`, or the `from` of a message). `None`
    /// when the build knows each value. A record with such a
    /// value counts as a skipped record: the build writes no checkpoint
    /// past it (01M3XM2C18TT8VSKGD77YPZG53).
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
    ///
    /// // A message of a later build: a kind, and a field of a selector.
    /// let post = r#"{"position":3,"written_at_ms":1,"change":{"posted":{"thread":"acme/app","message":{"seq":1,"from":"riff://ann@heron/acme/app?session=s1","to":[{"user":"bob"}],"body":"hi","at_ms":1,"kind":"note"}}}}"#;
    /// assert_eq!(read(post).other(), None);
    /// assert_eq!(read(&post.replace("note", "poll")).other(), Some("kind"));
    /// assert_eq!(read(&post.replace(r#"{"user":"bob"}"#, r#"{"user":"bob","wave":"17"}"#)).other(), Some("to"));
    /// assert_eq!(read(&post.replace(r#"{"user":"bob"}"#, r#""all""#)).other(), Some("to"));
    /// // A session URI with a query part of a later build.
    /// assert_eq!(read(&post.replace("session=s1", "session=s1&wave=17")).other(), Some("from"));
    /// ```
    pub fn other(&self) -> Option<&'static str> {
        if self.envelope.by == Some(By::Other) {
            return Some("by");
        }
        if self.change.session().is_some_and(SessionUri::is_other) {
            return Some(match self.change {
                Change::Posted(_) => "from",
                _ => "session",
            });
        }
        match &self.change {
            Change::PauseSet(set) if set.scope == Scope::Other => Some("scope"),
            Change::SessionStarted(started) if started.reason == StartReason::Other => {
                Some("reason")
            }
            Change::Posted(posted) if !posted.message.kind.is_post() => Some("kind"),
            Change::Posted(posted) if posted.message.to.iter().any(Selector::is_other) => {
                Some("to")
            }
            _ => None,
        }
    }
}

impl Record {
    /// True when this record is a record of the repository thread `repo`
    /// for an audit (01M3ZWRC11R5M9V1KTF05P240W): a post, a
    /// membership, a lead, a claim or a release in that thread; a direct
    /// message from a session in that repository; the start or the end
    /// of a session in that repository; and a pause of the riff or of
    /// that repository. Each other record is not.
    ///
    /// ```
    /// use riff_core::record::{Change, Claimed, Envelope, Line, PauseSet, Record, Scope};
    /// use riff_core::wire::RiffState;
    ///
    /// let repo = "acme/app".parse()?;
    /// let envelope = Envelope { position: 1, written_at_ms: 1, by: None, command: None, call: None };
    /// let record = |change| Record { envelope: envelope.clone(), change };
    /// let claim = |thread: &str| record(Change::Claimed(Claimed {
    ///     session: "riff://ann@heron/acme/app?session=s1".parse().unwrap(),
    ///     thread: thread.parse().unwrap(),
    ///     item: "issue-7".into(),
    /// }));
    /// assert!(claim("acme/app").of_repository(&repo));
    /// assert!(!claim("acme/web").of_repository(&repo));
    ///
    /// let pause = |scope| record(Change::PauseSet(PauseSet { scope, state: RiffState::Paused }));
    /// assert!(pause(Scope::Riff).of_repository(&repo));
    /// assert!(pause(Scope::Repository(repo.clone())).of_repository(&repo));
    /// assert!(!pause(Scope::Repository("acme/web".parse()?)).of_repository(&repo));
    ///
    /// // A record of the people is not a record of a repository.
    /// let invited = r#"{"position":7,"written_at_ms":1,"change":{"member_invited":{"email":"ann@acme.io"}}}"#;
    /// let Line::Record(invited) = Line::parse(invited)? else { panic!("a known kind") };
    /// assert!(!invited.of_repository(&repo));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn of_repository(&self, repo: &ThreadName) -> bool {
        let here = |session: &SessionUri| session.default_thread().as_ref() == Some(repo);
        match &self.change {
            Change::Posted(posted) if posted.thread.is_direct() => here(&posted.message.from),
            Change::Posted(posted) => &posted.thread == repo,
            Change::JoinedThread(m) | Change::LeftThread(m) | Change::LeadSet(m) => {
                &m.thread == repo
            }
            Change::Claimed(c) => &c.thread == repo,
            Change::Released(r) => &r.thread == repo,
            Change::SessionStarted(s) => here(&s.session),
            Change::SessionForgotten(f) => here(&f.session),
            Change::PauseSet(set) => match &set.scope {
                Scope::Riff => true,
                Scope::Repository(thread) => thread == repo,
                Scope::Other => false,
            },
            Change::ItemHeld(held) => &held.thread == repo,
            Change::ItemFreed(freed) => &freed.thread == repo,
            Change::SettingChanged(_)
            | Change::RiffMade(_)
            | Change::PersonJoined(_)
            | Change::MemberInvited(_)
            | Change::MemberRemoved(_)
            | Change::AdminSet(_)
            | Change::OwnerSet(_)
            | Change::OwnerAsked(_)
            | Change::OwnerDenied(_)
            | Change::SigninsEnded(_) => false,
        }
    }

    /// This record for an audit: a post has only its mark
    /// ([`Posted::for_audit`]). Each other record stays as it is.
    pub fn for_audit(self) -> Record {
        let Record { envelope, change } = self;
        let change = match change {
            Change::Posted(posted) => Change::Posted(Box::new(posted.for_audit())),
            change => change,
        };
        Record { envelope, change }
    }
}

/// A message in a thread.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Posted {
    pub thread: ThreadName,
    /// The message, with its seq. A signed message keeps its payload and
    /// its signature unchanged.
    pub message: Message,
    /// Each session that the message woke.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub woken: BTreeSet<Who>,
}

/// The marks of a post for an audit, longest first: the start of a
/// body that names the step of the flow (01M3ZWRC3XBFN8FJDGE8XWZ5EA).
pub const MARKS: [&str; 3] = ["verify request", "verify result", "request"];

impl Posted {
    /// The mark of `body`: the first of [`MARKS`] that starts it, before
    /// a colon. An empty text when no mark starts it.
    ///
    /// ```
    /// use riff_core::record::Posted;
    ///
    /// assert_eq!(Posted::mark("request: claim issue-12"), "request");
    /// assert_eq!(Posted::mark("verify request: issue-12, PR #40"), "verify request");
    /// assert_eq!(Posted::mark("verify result: PASS for issue-12"), "verify result");
    /// assert_eq!(Posted::mark("Lead: request: claim issue-12"), "");
    /// assert_eq!(Posted::mark("requests are free"), "");
    /// ```
    pub fn mark(body: &str) -> &'static str {
        MARKS
            .into_iter()
            .find(|mark| {
                body.strip_prefix(mark)
                    .is_some_and(|rest| rest.starts_with(':'))
            })
            .unwrap_or_default()
    }

    /// The post for an audit (01M3ZWRC3XBFN8FJDGE8XWZ5EA): its body is
    /// only its [mark](Posted::mark), and it has no signature and no
    /// payload. The kind, the sender, the `to` and the time stay.
    ///
    /// ```
    /// use riff_core::record::Posted;
    /// use riff_core::wire::Message;
    ///
    /// let posted = Posted {
    ///     thread: "acme/app".parse()?,
    ///     message: Message {
    ///         seq: 4,
    ///         from: "riff://ann@heron/acme/app?session=s1".parse()?,
    ///         to: Vec::new(),
    ///         body: "request: claim issue-12. The token is t0p.".into(),
    ///         at_ms: 9,
    ///         kind: Default::default(),
    ///         sig: Some("sig".into()),
    ///         payload: Some("payload".into()),
    ///     },
    ///     woken: Default::default(),
    /// };
    /// let audit = posted.clone().for_audit();
    /// assert_eq!(audit.message.body, "request");
    /// assert_eq!((audit.message.sig, audit.message.payload), (None, None));
    /// assert_eq!((audit.message.seq, audit.message.at_ms), (4, 9));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn for_audit(mut self) -> Posted {
        self.message.body = Posted::mark(&self.message.body).to_owned();
        self.message.sig = None;
        self.message.payload = None;
        self
    }
}

/// A session and a thread.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Member {
    pub session: SessionUri,
    pub thread: ThreadName,
}

/// A claim of one work item in one thread.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Claimed {
    pub session: SessionUri,
    pub thread: ThreadName,
    pub item: String,
}

/// The hold of one item in one repository thread, with its reason
/// (01M43GSGB9ZFHSG0Q83Y50FEGW). The envelope of the record names who held it, and
/// when. A hold is not a claim.
///
/// ```
/// use riff_core::record::{Change, ItemFreed, ItemHeld};
///
/// let held = Change::ItemHeld(ItemHeld {
///     thread: "como-technologies/riff".parse()?,
///     item: "issue-366".into(),
///     reason: "waits for the word of Mike".into(),
/// });
/// assert_eq!(
///     serde_json::to_string(&held).unwrap(),
///     r#"{"item_held":{"thread":"como-technologies/riff","item":"issue-366","reason":"waits for the word of Mike"}}"#
/// );
/// let freed = Change::ItemFreed(ItemFreed {
///     thread: "como-technologies/riff".parse()?,
///     item: "issue-366".into(),
/// });
/// assert_eq!(
///     serde_json::to_string(&freed).unwrap(),
///     r#"{"item_freed":{"thread":"como-technologies/riff","item":"issue-366"}}"#
/// );
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ItemHeld {
    pub thread: ThreadName,
    pub item: String,
    pub reason: String,
}

/// The end of the hold of one item in one repository thread.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ItemFreed {
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
        let Claimed {
            session,
            thread,
            item,
        } = claim;
        Released {
            session,
            thread,
            item,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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

/// The schema takes each value: [`Scope`] reads a scope that it does
/// not know as [`Scope::Other`].
impl schemars::JsonSchema for Scope {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Scope".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({})
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SettingChanged {
    /// The settings of idle workers.
    pub idle: Idle,
}

/// A session that the state forgets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Forgotten {
    pub session: SessionUri,
}

/// The ID of a riff (01M3JNVBPMZ1K9WX7Q7DP6Y0DH).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RiffMade {
    pub riff_id: String,
}

/// The first sign-in of a person: the verified email holds the USER
/// (R209).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PersonJoined {
    pub user: String,
    /// The verified email, in lower case.
    pub email: String,
}

/// A person, by the verified email in lower case. A record of the
/// people names a person by the email: the state finds the USER.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Email {
    pub email: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OwnerSet {
    /// The email of the owner. `None`: the owner is gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// A request for the owner role.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OwnerAsked {
    /// The email of the admin that asks.
    pub email: String,
    /// With no answer before this time, the admin is the owner. In
    /// milliseconds since the Unix epoch.
    pub due_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
    /// error. The record keeps its call ID, so a start from the log keeps
    /// each call.
    ///
    /// ```
    /// use riff_core::record::Line;
    ///
    /// let called = r#"{"position":3,"written_at_ms":1,"call":"c1","change":{"member_invited":{"email":"a@acme.io"}}}"#;
    /// let Line::Record(record) = Line::parse(called).unwrap() else { panic!("a known kind") };
    /// assert_eq!(record.envelope.call.as_deref(), Some("c1"));
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
            #[serde(flatten)]
            envelope: Envelope,
            change: serde_json::Map<String, serde_json::Value>,
        }
        let Raw { envelope, change } = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let position = envelope.position;
        let [kind] = change.keys().collect::<Vec<_>>()[..] else {
            return Err(format!(
                "the change of the record at position {position} needs one kind"
            ));
        };
        if !Change::KINDS.contains(&kind.as_str()) {
            return Ok(Line::Unknown {
                position,
                kind: kind.clone(),
            });
        }
        let change = serde_json::from_value(serde_json::Value::Object(change))
            .map_err(|e| format!("the record at position {position}: {e}"))?;
        Ok(Line::Record(Box::new(Record { envelope, change })))
    }
}

/// One change of each kind, for the tests of each crate: a new kind
/// goes here, and each round trip covers it.
#[cfg(any(test, feature = "test-support"))]
pub fn one_of_each() -> Vec<Change> {
    let uri = || -> SessionUri { "riff://ann@heron/acme/app?session=s1".parse().unwrap() };
    let thread = || -> ThreadName { "acme/app".parse().unwrap() };
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
                to: vec!["user=ann,session=s1,host=heron,repo=acme/app,worktree=issue-7,claim=issue-7,lead=true".parse().unwrap()],
                body: "hi".into(),
                at_ms: 5,
                kind: crate::wire::Kind::Status,
                sig: Some("h..s".into()),
                payload: Some("cA".into()),
            },
            woken: BTreeSet::from([uri().who().clone()]),
        })),
        Change::JoinedThread(member.clone()),
        Change::LeftThread(member.clone()),
        Change::Claimed(claim.clone()),
        Change::Released(Released {
            session: claim.session,
            thread: claim.thread,
            item: claim.item,
            must_clear: true,
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
        Change::OwnerSet(OwnerSet {
            email: Some(email.email.clone()),
        }),
        Change::OwnerAsked(OwnerAsked {
            email: email.email.clone(),
            due_ms: 9,
        }),
        Change::OwnerDenied(email),
        Change::SigninsEnded(SigninsEnded { user: "ann".into() }),
        Change::ItemHeld(ItemHeld {
            thread: thread(),
            item: "issue-7".into(),
            reason: "waits for ann".into(),
        }),
        Change::ItemFreed(ItemFreed {
            thread: thread(),
            item: "issue-7".into(),
        }),
    ]
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
                envelope: Envelope {
                    position: 3,
                    written_at_ms: 4,
                    by: Some(By::Server),
                    command: Some("forget".into()),
                    call: None,
                },
                change,
            };
            let line = serde_json::to_string(&record).unwrap();
            assert!(line.contains(&format!(r#""change":{{"{kind}":"#)), "{line}");
            assert_eq!(Line::parse(&line).unwrap(), Line::Record(Box::new(record)));
        }
    }

    /// Each field of the envelope comes through the tolerant reader
    /// (01M49W17GGV1K5FZEKJRPV0GNA). The destructure names each field:
    /// a new field of the envelope does not build until it is set here
    /// too, and then the reader must keep it.
    #[test]
    fn each_field_of_the_envelope_comes_through_the_reader() {
        let envelope = Envelope {
            position: 7,
            written_at_ms: 8,
            by: Some(By::Session(uri().who().clone())),
            command: Some("claim".into()),
            call: Some("call-7".into()),
        };
        let Envelope {
            position,
            written_at_ms,
            by,
            command,
            call,
        } = &envelope;
        assert!(*position != 0 && *written_at_ms != 0);
        assert!(by.is_some() && command.is_some() && call.is_some());
        let record = Record {
            envelope: envelope.clone(),
            change: Change::RiffMade(RiffMade {
                riff_id: "r1".into(),
            }),
        };
        let line = serde_json::to_string(&record).unwrap();
        let Line::Record(read) = Line::parse(&line).unwrap() else {
            panic!("a known kind");
        };
        assert_eq!(read.envelope, envelope);
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
        assert_eq!(record.envelope.by, Some(By::Other));
        assert_eq!(record.other(), Some("by"));
        // The kind of a command is text: a kind of a later build reads.
        assert_eq!(record.envelope.command.as_deref(), Some("sweep"));
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

    /// A `posted` record with the message `message`, as JSON.
    fn posted(message: &str) -> Record {
        let line = format!(
            r#"{{"position":2,"written_at_ms":1,"change":{{"posted":{{"thread":"acme/app","message":{message},"woken":[{{"user":"bob","session":"b1"}}]}}}}}}"#
        );
        match Line::parse(&line).unwrap() {
            Line::Record(record) => *record,
            Line::Unknown { .. } => panic!("a known kind"),
        }
    }

    const MESSAGE: &str = r#"{"seq":1,"from":"riff://ann@heron/acme/app?session=s1","to":[{"user":"bob"}],"body":"the text","at_ms":5,"kind":"status"}"#;

    #[test]
    fn a_kind_of_a_message_that_the_build_does_not_know_reads_as_other() {
        assert_eq!(posted(MESSAGE).other(), None);
        // A message with no kind is a message.
        let plain = posted(&MESSAGE.replace(r#","kind":"status""#, ""));
        assert_eq!(plain.other(), None);
        // A kind of a later build can have each form of JSON.
        for later in [
            r#""poll""#,
            r#"{"poll":"wave"}"#,
            "7",
            "null",
            r#"["note"]"#,
        ] {
            let record = posted(&MESSAGE.replace(r#""status""#, later));
            assert_eq!(record.other(), Some("kind"), "{later}");
            let Change::Posted(posted) = &record.change else {
                panic!("a post");
            };
            assert_eq!(posted.message.kind, Kind::Other);
            // The message keeps its text, its address and its wakes.
            assert_eq!(posted.message.body, "the text");
            assert_eq!(posted.message.to, ["user=bob".parse().unwrap()]);
            assert_eq!(posted.woken.len(), 1);
        }
    }

    #[test]
    fn a_selector_with_a_field_that_the_build_does_not_know_reads_as_other() {
        for later in [
            r#"[{"user":"bob","wave":"17"}]"#,
            r#"[{"user":"bob"},{"wave":17}]"#,
            r#"[{"wave":{"n":17},"lead":true}]"#,
            // A value that the build does not know, and each other form
            // of JSON.
            r#"[{"user":null,"wave":"17"}]"#,
            r#"[{"lead":"maybe","user":"bob"}]"#,
            r#"["all"]"#,
            r#"[["user","bob"]]"#,
            "[17]",
            "[null]",
        ] {
            let record = posted(&MESSAGE.replace(r#"[{"user":"bob"}]"#, later));
            assert_eq!(record.other(), Some("to"), "{later}");
            let Change::Posted(posted) = &record.change else {
                panic!("a post");
            };
            assert_eq!(posted.message.kind, Kind::Status);
            assert_eq!(posted.message.body, "the text");
            // The record writes each field of the selector again.
            let written = serde_json::to_value(&posted.message.to).unwrap();
            let read: serde_json::Value = serde_json::from_str(later).unwrap();
            assert_eq!(written, read);
        }
    }

    /// The line of a change with the session URI `uri`, for each kind
    /// that holds one.
    fn lines_with(uri: &str) -> Vec<String> {
        let member = format!(r#"{{"session":"{uri}","thread":"acme/app"}}"#);
        let claim = format!(r#"{{"session":"{uri}","thread":"acme/app","item":"issue-7"}}"#);
        let message = MESSAGE.replace("riff://ann@heron/acme/app?session=s1", uri);
        [
            (
                "posted",
                format!(r#"{{"thread":"acme/app","message":{message}}}"#),
            ),
            ("joined_thread", member.clone()),
            ("left_thread", member.clone()),
            ("lead_set", member),
            ("claimed", claim.clone()),
            ("released", claim),
            ("session_forgotten", format!(r#"{{"session":"{uri}"}}"#)),
            (
                "session_started",
                format!(r#"{{"session":"{uri}","reason":"process"}}"#),
            ),
        ]
        .into_iter()
        .map(|(kind, body)| {
            format!(r#"{{"position":2,"written_at_ms":1,"change":{{"{kind}":{body}}}}}"#)
        })
        .collect()
    }

    #[test]
    fn a_session_uri_with_a_query_part_of_a_later_build_reads_in_each_kind() {
        let known = "riff://ann@heron/acme/app?session=s1";
        for line in lines_with(known) {
            let Line::Record(record) = Line::parse(&line).unwrap() else {
                panic!("a known kind");
            };
            assert_eq!(record.other(), None, "{line}");
            assert_eq!(record.change.session(), Some(&uri()), "{line}");
        }
        // Each kind with no session URI has none.
        let with_uri = lines_with(known).len();
        let without = one_of_each()
            .iter()
            .filter(|change| change.session().is_none())
            .count();
        assert_eq!(with_uri + without, Change::KINDS.len());

        for later in [
            "riff://ann@heron/acme/app?session=s1&wave=17",
            "riff://ann@heron/acme/app?session=s1&lead=maybe",
            "riff://ann@heron/acme/app?session=s1&lead=true&mode",
            "riff://ann@heron/acme/app?session=s1&session=s2",
            "riff://ann@heron/acme/app?session=s1&claim=two%20words",
            "riff://ann@heron/acme/app?session=s1&claim=issue-7&wave=17#api",
        ] {
            for line in lines_with(later) {
                let Line::Record(record) = Line::parse(&line).unwrap() else {
                    panic!("a known kind");
                };
                let field = match record.change {
                    Change::Posted(_) => "from",
                    _ => "session",
                };
                assert_eq!(record.other(), Some(field), "{line}");
                // The same session, and the URI as it came.
                let session = record.change.session().unwrap();
                assert_eq!(session.who(), uri().who(), "{line}");
                assert_eq!(session.place().host(), "heron", "{line}");
                assert_eq!(session.to_string(), later, "{line}");
                assert!(serde_json::to_string(&record).unwrap().contains(later));
            }
        }
        // The part gives no mark.
        let read = |text: &str| SessionUri::try_from(text.to_owned()).unwrap();
        let maybe = read("riff://ann@heron/acme/app?session=s1&lead=maybe");
        assert!(!maybe.lead() && maybe.claims().is_empty());
        let claim = read("riff://ann@heron/acme/app?session=s1&claim=two%20words");
        assert!(claim.claims().is_empty());
    }

    /// The list "Each text with a grammar in a record" of the module
    /// docs: what reads, and what does not.
    #[test]
    fn each_text_with_a_grammar_reads_as_the_list_says() {
        let reads = |line: &str| Line::parse(line).is_ok();
        let claimed = |session: &str, thread: &str, item: &str| {
            format!(
                r#"{{"position":2,"written_at_ms":1,"change":{{"claimed":{{"session":"{session}","thread":"{thread}","item":"{item}"}}}}}}"#
            )
        };
        let session = "riff://ann@heron/acme/app?session=s1";
        assert!(reads(&claimed(session, "acme/app", "issue-7")));
        // The parts of a URI before its query never grow.
        for other in [
            "riff://ann@heron/acme/app/more?session=s1",
            "riff://ann@two words/acme/app?session=s1",
            "rifs://ann@heron/acme/app?session=s1",
            "riff://heron/acme/app?session=s1",
        ] {
            assert!(!reads(&claimed(other, "acme/app", "issue-7")), "{other}");
        }
        // A thread name is a text with no white space. The read does
        // not take it apart.
        for thread in [
            "wave:17",
            "dm:ann/s1|bob/b1",
            "dm:a later form",
            "dm:x",
            "a/b/c",
        ] {
            let reads_it = !thread.contains(' ') || thread.starts_with("dm:");
            assert_eq!(
                reads(&claimed(session, thread, "issue-7")),
                reads_it,
                "{thread}"
            );
        }
        assert!(!reads(&claimed(session, "", "issue-7")));
        // An item is free text.
        assert!(reads(&claimed(
            session,
            "acme/app",
            "two words, and more: 17"
        )));
        assert!(reads(&claimed(session, "acme/app", "")));
        // A session in `woken`: no check of a character, and a field of
        // a later build is skipped.
        let woken =
            r#"[{"user":"two words","session":"b 1","wave":17},{"user":"bob","session":null}]"#;
        let record = posted(MESSAGE);
        let line = serde_json::to_string(&record)
            .unwrap()
            .replace(r#"[{"user":"bob","session":"b1"}]"#, woken);
        assert!(line.contains("two words"));
        let Line::Record(record) = Line::parse(&line).unwrap() else {
            panic!("a known kind");
        };
        assert_eq!(record.other(), None);
        // A signature and a payload that do not check still read.
        let signed = MESSAGE.replace(
            r#""kind""#,
            r#""sig":"not a signature","payload":"%%","kind""#,
        );
        assert_eq!(posted(&signed).other(), None);
        // A number is a whole number that is not negative.
        let numbered = |position: &str, at: &str| {
            format!(
                r#"{{"position":{position},"written_at_ms":{at},"change":{{"riff_made":{{"riff_id":"r1"}}}}}}"#
            )
        };
        assert!(reads(&numbered("2", "18446744073709551615")));
        for (position, at) in [("-2", "1"), ("2", "1.5"), ("2", r#""1""#), ("2", "null")] {
            assert!(!reads(&numbered(position, at)), "{position} {at}");
        }
        let idle = |per_host: &str| {
            format!(
                r#"{{"position":2,"written_at_ms":1,"change":{{"setting_changed":{{"idle":{{"per_host":{per_host},"after_secs":60}}}}}}}}"#
            )
        };
        assert!(reads(&idle("65535")) && !reads(&idle("65536")));
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
        assert_eq!(
            (&record.envelope.by, &record.envelope.command),
            (&None, &None)
        );
        assert_eq!(serde_json::to_string(&record).unwrap(), line);
    }

    #[test]
    fn a_change_with_two_kinds_does_not_read() {
        let line = r#"{"position":2,"written_at_ms":1,"change":{"claimed":{},"released":{}}}"#;
        assert!(Line::parse(line).is_err());
    }
}
