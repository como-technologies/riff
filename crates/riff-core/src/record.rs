//! The records of the log of `riff-server`.
//!
//! # Design
//!
//! Each change that must not be lost is a [`Record`] in one log
//! (01M3T410XDD9W4EC0Y68FAA7XN). A record
//! has a position (1, 2, 3, and so on), the time of its write, and one
//! [`Change`]. The change names say what happened, in the past tense.
//! These Rust types are the schema of the log.
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
//! use riff_core::record::{Change, Claimed, Line, Record};
//!
//! let record = Record {
//!     position: 1234,
//!     written_at_ms: 1_790_000_000_000,
//!     change: Change::Claimed(Claimed {
//!         session: "riff://ann@heron/acme/app?session=s1".parse()?,
//!         thread: "acme/app".parse()?,
//!         item: "issue-7".into(),
//!     }),
//! };
//! let line = serde_json::to_string(&record).unwrap();
//! assert_eq!(
//!     line,
//!     r#"{"position":1234,"written_at_ms":1790000000000,"change":{"claimed":{"session":"riff://ann@heron/acme/app?session=s1","thread":"acme/app","item":"issue-7"}}}"#
//! );
//! assert_eq!(Line::parse(&line)?, Line::Record(Box::new(record)));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

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
    fn a_change_with_two_kinds_does_not_read() {
        let line = r#"{"position":2,"written_at_ms":1,"change":{"claimed":{},"released":{}}}"#;
        assert!(Line::parse(line).is_err());
    }
}
