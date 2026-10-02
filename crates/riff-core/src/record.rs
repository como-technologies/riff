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
//! the command (RID_CAUSE). So the log alone shows who made each
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
//! // A record from before the cause reads. Its cause is not known.
//! let old = r#"{"position":7,"written_at_ms":1,"change":{"riff_state_set":{"state":"running"}}}"#;
//! let Line::Record(old) = Line::parse(old)? else { panic!("a known kind") };
//! assert_eq!((old.by, old.command), (None, None));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::name::{SessionUri, ThreadName, Who};
use crate::wire::{Idle, Message, RiffState};

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

/// What happened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    /// A message, with its seq in its thread and the sessions that it
    /// woke.
    Posted(Box<Posted>),
    /// A session joined a thread.
    JoinedThread(Member),
    /// A session left a thread. It is no longer the lead there.
    LeftThread(Member),
    /// A session took a claim. It replaces the old holder.
    Claimed(Claimed),
    /// The holder freed a claim.
    Released(Claimed),
    /// A session became the lead of its user in a repository thread.
    LeadSet(Member),
    /// The riff is paused or running.
    RiffStateSet(RiffStateSet),
    /// A setting of the riff changed.
    SettingChanged(SettingChanged),
    /// A session had no sign of life for `SESSION_EXPIRY`. The state
    /// drops it: its read cursors, its memberships, its claims, its lead,
    /// and each direct thread whose two sessions are gone.
    SessionForgotten(Forgotten),
}
// ANCHOR_END: record

/// Who caused a record: the caller of its command, with its class
/// (RID_CAUSE). It holds only the user and the session ID. The session
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
    /// The name of each kind of change, as the JSON of a record has it.
    pub const KINDS: &[&str] = &[
        "posted",
        "joined_thread",
        "left_thread",
        "claimed",
        "released",
        "lead_set",
        "riff_state_set",
        "setting_changed",
        "session_forgotten",
    ];
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiffStateSet {
    pub state: RiffState,
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
            Change::Released(claim),
            Change::LeadSet(member),
            Change::RiffStateSet(RiffStateSet {
                state: RiffState::Running,
            }),
            Change::SettingChanged(SettingChanged {
                idle: Idle::default(),
            }),
            Change::SessionForgotten(Forgotten { session: uri() }),
        ]
    }

    #[test]
    fn each_kind_reads_back_and_has_its_name() {
        let changes = one_of_each();
        assert_eq!(changes.len(), Change::KINDS.len());
        for (change, kind) in changes.into_iter().zip(Change::KINDS) {
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
        let line = r#"{"position":2,"written_at_ms":1,"later":true,"change":{"riff_state_set":{"state":"running","why":"x"}}}"#;
        let Line::Record(record) = Line::parse(line).unwrap() else {
            panic!("a known kind");
        };
        assert_eq!(
            record.change,
            Change::RiffStateSet(RiffStateSet {
                state: RiffState::Running
            })
        );
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
        let line = r#"{"position":2,"written_at_ms":1,"by":{"robot":"r2"},"command":"sweep","change":{"riff_state_set":{"state":"running"}}}"#;
        let Line::Record(record) = Line::parse(line).unwrap() else {
            panic!("a known kind");
        };
        assert_eq!(record.by, Some(By::Other));
        // The kind of a command is text: a kind of a later build reads.
        assert_eq!(record.command.as_deref(), Some("sweep"));
    }

    #[test]
    fn a_record_with_no_cause_reads_and_writes_no_cause() {
        let line = r#"{"position":2,"written_at_ms":1,"change":{"riff_state_set":{"state":"running"}}}"#;
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
