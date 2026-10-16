//! The type of a record, and the JSON body of each type
//! (01M519Z23WX0A142N74BKAKKYY, 01M519Z26AP5NZSFHWRQ2QCV9D).
//!
//! # Design
//!
//! Each type has one writer and a largest size of the whole record. The
//! byte of the type is in the bytes of the record
//! ([`crate::peer::record`]).
//!
//! | Type | Byte | Writer | Largest record | Body |
//! |---|---|---|---|---|
//! | `riff` | 1 | the owner, in its first record | 1 KiB | [`Riff`]: the user and the host of the owner |
//! | `member` | 2 | the owner | 1 KiB | [`Member`]: a member key, its user and its host |
//! | `removed` | 3 | the owner, for a key that is not its own | 1 KiB | [`Removed`]: a member key, and its last sequence number |
//! | `left` | 4 | a member, for its own key | 1 KiB | [`Left`]: nothing more |
//! | `session` | 5 | the daemon of the session, of the user of its key | 4 KiB | [`Session`]: the session, its lead mark, its claims, its status and its end |
//! | `message` | 6 | the daemon of the session that posts, of the user of its key | 64 KiB | [`Message`]: a message, a note or a status request |
//! | `plan` | 7 | a daemon of a lead, or the daemon of the owner | 4 KiB | [`Plan`]: a hold, a free, a pause or a resume |
//! | another byte | 8 to 255 | not checked | 64 KiB | not read: the daemon keeps it, and does not apply it |
//!
//! The byte 0 is no type (01M51Q0JX87PPFGPABFPVWWW0E). A reader skips
//! each field of a body that it does not know: no body has
//! `deny_unknown_fields`. A [`Plan`] of a kind that the reader does not
//! know reads as [`Plan::Other`].
//!
//! A `session` record holds the whole state of its session at that
//! time: its place, its lead mark, its claims, its status and its end.
//! Its author writes a new record at each change. The chain of the
//! author orders them, so the last one counts
//! (01M51Q0JZR00GHDQSGGSAQR4HG).
//!
//! The writer of `plan` is "a daemon of a lead, or the daemon of the
//! owner": the owner frees, pauses and resumes also with no lead
//! (01M519Z23WX0A142N74BKAKKYY). Only the owner pauses the whole riff.
//!
//! # Example
//!
//! ```
//! use riff::peer::body::{Body, Plan, Type};
//!
//! let body = br#"{"plan":"hold","thread":"o/r","item":"issue-12","reason":"wait","new":1}"#;
//! let Body::Plan(Plan::Hold { item, .. }) = Body::read(Type::Plan, body)? else { panic!() };
//! assert_eq!(item, "issue-12");
//! assert_eq!(Type::of(7), Type::Plan);
//! assert_eq!(Type::of(99).largest(), 64 * 1024);
//! # Ok::<(), serde_json::Error>(())
//! ```

use std::fmt;
use std::ops::Not;

use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire;
use serde::{Deserialize, Serialize};

use crate::daemon_key::Public;
use crate::peer::record::Id;

/// One KiB.
const KIB: usize = 1024;

/// The largest size of a record of each type: [`Type::largest`] of a
/// `message`. A peer link reads no larger record ([`crate::peer::wire`]).
pub const LARGEST: usize = 64 * KIB;

/// The type of a record: its byte in the bytes of the record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Type {
    Riff,
    Member,
    Removed,
    Left,
    Session,
    Message,
    Plan,
    /// A byte that this build does not know, or 0: no type.
    Other(u8),
}

impl Type {
    /// The type of the byte `byte`.
    pub fn of(byte: u8) -> Type {
        match byte {
            1 => Type::Riff,
            2 => Type::Member,
            3 => Type::Removed,
            4 => Type::Left,
            5 => Type::Session,
            6 => Type::Message,
            7 => Type::Plan,
            other => Type::Other(other),
        }
    }

    /// The byte of the type.
    pub fn byte(self) -> u8 {
        match self {
            Type::Riff => 1,
            Type::Member => 2,
            Type::Removed => 3,
            Type::Left => 4,
            Type::Session => 5,
            Type::Message => 6,
            Type::Plan => 7,
            Type::Other(byte) => byte,
        }
    }

    /// The largest size of a whole record of the type, in bytes.
    pub fn largest(self) -> usize {
        match self {
            Type::Riff | Type::Member | Type::Removed | Type::Left => KIB,
            Type::Session | Type::Plan => 4 * KIB,
            Type::Message | Type::Other(_) => LARGEST,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Riff => f.write_str("riff"),
            Type::Member => f.write_str("member"),
            Type::Removed => f.write_str("removed"),
            Type::Left => f.write_str("left"),
            Type::Session => f.write_str("session"),
            Type::Message => f.write_str("message"),
            Type::Plan => f.write_str("plan"),
            Type::Other(byte) => write!(f, "type {byte}"),
        }
    }
}

/// The body of a record, read from its JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    Riff(Riff),
    Member(Member),
    Removed(Removed),
    Left(Left),
    Session(Box<Session>),
    Message(Box<Message>),
    Plan(Plan),
    /// A type that this build does not know: the body is not read.
    Other,
}

impl Body {
    /// Reads the JSON `json` of a body of the type `of`. A type that this
    /// build does not know gives [`Body::Other`] with no read.
    pub fn read(of: Type, json: &[u8]) -> Result<Body, serde_json::Error> {
        Ok(match of {
            Type::Riff => Body::Riff(serde_json::from_slice(json)?),
            Type::Member => Body::Member(serde_json::from_slice(json)?),
            Type::Removed => Body::Removed(serde_json::from_slice(json)?),
            Type::Left => Body::Left(serde_json::from_slice(json)?),
            Type::Session => Body::Session(serde_json::from_slice(json)?),
            Type::Message => Body::Message(serde_json::from_slice(json)?),
            Type::Plan => Body::Plan(serde_json::from_slice(json)?),
            Type::Other(_) => Body::Other,
        })
    }

    /// The type of the body.
    pub fn of(&self) -> Option<Type> {
        Some(match self {
            Body::Riff(_) => Type::Riff,
            Body::Member(_) => Type::Member,
            Body::Removed(_) => Type::Removed,
            Body::Left(_) => Type::Left,
            Body::Session(_) => Type::Session,
            Body::Message(_) => Type::Message,
            Body::Plan(_) => Type::Plan,
            Body::Other => return None,
        })
    }

    /// The JSON of the body. [`Body::Other`] has none.
    pub fn json(&self) -> Option<Vec<u8>> {
        let json = match self {
            Body::Riff(body) => serde_json::to_vec(body),
            Body::Member(body) => serde_json::to_vec(body),
            Body::Removed(body) => serde_json::to_vec(body),
            Body::Left(body) => serde_json::to_vec(body),
            Body::Session(body) => serde_json::to_vec(body),
            Body::Message(body) => serde_json::to_vec(body),
            Body::Plan(body) => serde_json::to_vec(body),
            Body::Other => return None,
        };
        // A body of this module always gives JSON.
        Some(json.expect("the JSON of a body"))
    }
}

/// The body of `riff`: the first record of the owner. Its author is the
/// owner, and the riff ID is its key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Riff {
    /// The user of the owner.
    pub user: String,
    /// The host of the daemon of the owner, for `riff members`
    /// (01M521J6T2PWA9JREDXJK6DZ0V). A riff of an older build has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

/// The body of `member`: the owner admits one key. A key that left joins
/// again with a second `member` record (01M5328FPJT7Q5Q1509BC5F0J7).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// The key of the daemon of the member.
    pub key: Public,
    /// The user of the member.
    pub user: String,
    /// The host of the daemon.
    pub host: String,
    /// At a join again: the sequence number of the `left` record of the
    /// key. The records of the key after it count with this user and
    /// host. `None` at the first join of the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<u64>,
}

/// The body of `removed`: the owner removes one key
/// (01M519Z45J9RC734KBY1G8D49V).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Removed {
    /// The key that the owner removes.
    pub key: Public,
    /// The highest sequence number of that key that the daemon of the
    /// owner had. A record of the key after it is refused.
    pub last: u64,
}

/// The body of `left`: its author leaves the riff.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Left {}

/// The body of `session`: the whole state of one session of the
/// author.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// The session: its user, host, repository, worktree and ID.
    pub session: SessionUri,
    /// True for a worker.
    #[serde(default, skip_serializing_if = "Not::not")]
    pub worker: bool,
    /// The lead mark, when the session took the lead of its user in its
    /// repository (01M51BFYT2ZGN4W5E5R87GPVCM).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<Lead>,
    /// The items that the session claims.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<String>,
    /// The words of the status of the session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// True at the end of the session.
    #[serde(default, skip_serializing_if = "Not::not")]
    pub ended: bool,
}

/// A lead mark (01M51BFYT2ZGN4W5E5R87GPVCM).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lead {
    /// The ID of the record that gave the old lead of the user in the
    /// repository its lead mark, when the daemon knew one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<Id>,
}

/// The body of `message`: a message, a note or a status request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// The session that posts.
    pub from: SessionUri,
    /// The thread.
    pub thread: ThreadName,
    /// The selectors of the sessions to wake.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub to: Vec<Selector>,
    /// A message, a note or a status request.
    #[serde(default)]
    pub kind: wire::Kind,
    /// The text.
    #[serde(default)]
    pub body: String,
}

/// The body of `plan` (01M519Z3J7Z0M3PNDY249Q57FH,
/// 01M51BFYT2ZGN4W5E5R87GPVCM).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "plan", rename_all = "lowercase")]
pub enum Plan {
    /// A lead holds an item of a repository.
    Hold {
        thread: ThreadName,
        item: String,
        #[serde(default)]
        reason: String,
    },
    /// A free: it ends each hold record that it names.
    Free { ends: Vec<Id> },
    /// A pause of a repository, or with no thread of the whole riff.
    Pause {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread: Option<ThreadName>,
    },
    /// A resume: it ends each pause record that it names.
    Resume { ends: Vec<Id> },
    /// A kind that this build does not know: not applied.
    #[serde(other)]
    Other,
}

impl Plan {
    /// The repository of a hold or of a pause of one repository.
    pub fn thread(&self) -> Option<&ThreadName> {
        match self {
            Plan::Hold { thread, .. } => Some(thread),
            Plan::Pause { thread } => thread.as_ref(),
            Plan::Free { .. } | Plan::Resume { .. } | Plan::Other => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_type_has_its_byte_and_its_largest_size() {
        let sizes = [
            (Type::Riff, 1, 1024),
            (Type::Member, 2, 1024),
            (Type::Removed, 3, 1024),
            (Type::Left, 4, 1024),
            (Type::Session, 5, 4096),
            (Type::Message, 6, 65536),
            (Type::Plan, 7, 4096),
            (Type::Other(8), 8, 65536),
            (Type::Other(0), 0, 65536),
        ];
        for (of, byte, largest) in sizes {
            assert_eq!(of.byte(), byte);
            assert_eq!(Type::of(byte), of);
            assert_eq!(of.largest(), largest, "{of}");
        }
    }

    #[test]
    fn a_body_gives_its_json_and_reads_it_again() {
        let public = crate::daemon_key::Key::from_bytes(&[2; 32]).public();
        let bodies = [
            Body::Riff(Riff {
                user: "mike".into(),
                host: None,
            }),
            Body::Member(Member {
                key: public,
                user: "brett".into(),
                host: "kadomony".into(),
                after: None,
            }),
            Body::Member(Member {
                key: public,
                user: "brett".into(),
                host: "kadomony".into(),
                after: Some(9),
            }),
            Body::Removed(Removed {
                key: public,
                last: 7,
            }),
            Body::Left(Left {}),
            Body::Session(Box::new(Session {
                session: "riff://mike@thelio/o/r?session=s1#issue-12"
                    .parse()
                    .unwrap(),
                worker: true,
                lead: Some(Lead {
                    replaces: Some(Id([3; 32])),
                }),
                claims: vec!["issue-12".into()],
                status: Some("tests".into()),
                ended: false,
            })),
            Body::Message(Box::new(Message {
                from: "riff://mike@thelio/o/r?session=s1".parse().unwrap(),
                thread: "o/r".parse().unwrap(),
                to: vec![Selector {
                    user: Some("mike".into()),
                    ..Selector::default()
                }],
                kind: wire::Kind::Note,
                body: "started".into(),
            })),
            Body::Plan(Plan::Free {
                ends: vec![Id([4; 32])],
            }),
        ];
        for body in bodies {
            let of = body.of().unwrap();
            let json = body.json().unwrap();
            assert_eq!(Body::read(of, &json).unwrap(), body, "{of}");
        }
        assert_eq!(Body::Other.json(), None);
        assert_eq!(
            Body::read(Type::Other(9), b"not json").unwrap(),
            Body::Other
        );
    }

    /// The table of the types in "The records" of the book page "riff
    /// 3.0: peers" agrees with the code: the name, the byte and the
    /// largest size of each type.
    #[test]
    fn the_book_has_the_byte_and_the_largest_size_of_each_type() {
        let book =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/riff-3-peers.md");
        let book = std::fs::read_to_string(book).unwrap();
        let part = &book[book.find("## The records").unwrap()..];
        let part = &part[..part.find("## How the sync works").unwrap()];
        let mut rows = 0;
        for line in part.lines().filter(|l| l.starts_with("| `")) {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let byte: u8 = cells[2].parse().unwrap();
            let of = Type::of(byte);
            assert_eq!(cells[1], format!("`{of}`"), "{line}");
            assert_eq!(cells[5], format!("{} KiB", of.largest() / 1024), "{line}");
            rows += 1;
        }
        assert_eq!(rows, 7);
    }

    /// 01M519Z26AP5NZSFHWRQ2QCV9D: a reader skips a field that it does
    /// not know, also a kind of plan that it does not know.
    #[test]
    fn a_reader_skips_what_it_does_not_know() {
        let json = br#"{"user":"mike","color":"red"}"#;
        assert_eq!(
            Body::read(Type::Riff, json).unwrap(),
            Body::Riff(Riff {
                user: "mike".into(),
                host: None,
            })
        );
        let json = br#"{"plan":"vote","item":"issue-12"}"#;
        assert_eq!(
            Body::read(Type::Plan, json).unwrap(),
            Body::Plan(Plan::Other)
        );
        assert!(Body::read(Type::Riff, b"{}").is_err());
        assert!(Body::read(Type::Member, b"[1]").is_err());
    }
}
