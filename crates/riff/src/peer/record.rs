//! A record: one change of the riff by one author, signed by its key
//! (01M519Z1WKH6THH971AQWHH8CN, 01M519Z1Z1FVZQQ612DEMSQ7PG,
//! 01M519Z21ECQY89RS60DZKZQVR).
//!
//! # The bytes
//!
//! | Field | Bytes | Offset |
//! |---|---|---|
//! | The format version: [`VERSION`] | 1 | 0 |
//! | The type ([`Type`]) | 1 | 1 |
//! | The riff ID: the key of the owner | 32 | 2 |
//! | The key of the author | 32 | 34 |
//! | The sequence number | 8 | 66 |
//! | The ID of the record before it | 32 | 74 |
//! | The time of the author, in milliseconds since the Unix epoch | 8 | 106 |
//! | The length of the body | 4 | 114 |
//! | The body: the JSON of its type ([`crate::peer::body`]) | the length | 118 |
//! | The ed25519 signature | 64 | 118 + the length |
//!
//! Each number is big-endian. The signature is by the author, over the
//! tag [`TAG`] and each byte before the signature. A reader checks it
//! with `verify_strict`. The ID of a record is the BLAKE3 hash of its
//! bytes ([`Record::id`]).
//!
//! A [`Record`] holds the bytes as the daemon got them. riff never
//! encodes a record again: [`Record::sign`] makes the bytes one time, and
//! [`check`] keeps the bytes that it got.
//!
//! # The chain
//!
//! The first record of an author has the sequence number 1 and the ID
//! [`Id::ZERO`] before it. Each next record has the next sequence number
//! and the ID of the record before it. So the records of one author make
//! one chain with no gap.
//!
//! # The checks
//!
//! [`check`] checks a record before the daemon keeps it, in this order
//! (01M519Z317AT7KT7Q643NVYZD1). The first check that fails gives its
//! [`Fault`]:
//!
//! ```mermaid
//! flowchart LR
//!     S["1 the size of its type"] --> V["2 the version and the type"]
//!     V --> R["3 the riff ID"] --> A["4 the author"]
//!     A --> N["5 the sequence number"] --> H["6 the ID before it"]
//!     H --> G["7 the signature"] --> J["8 the JSON of its type"]
//!     J --> W["9 the writer of its type"]
//! ```
//!
//! Checks 3 to 6 and 9 need the state of the riff of the reader: a
//! [`Context`]. A record of a type that the reader does not know passes
//! checks 1 to 7 only (01M519Z26AP5NZSFHWRQ2QCV9D): the daemon keeps and
//! passes it on, and does not apply it.
//!
//! # Example
//!
//! ```
//! use riff::daemon_key::{Key, Public};
//! use riff::peer::body::{Body, Riff, Type};
//! use riff::peer::record::{Admitted, Context, Fault, Id, Record, Standing, check};
//! use riff_core::name::ThreadName;
//!
//! /// A riff of one owner with no record yet.
//! struct New(Public);
//!
//! impl Context for New {
//!     fn riff(&self) -> Public { self.0 }
//!     fn standing(&self, key: &Public, _: u64) -> Standing {
//!         if *key == self.0 { Standing::Owner { user: None } } else { Standing::Unknown }
//!     }
//!     fn admitted(&self, _: &Public) -> Option<Admitted> { None }
//!     fn next(&self, _: &Public) -> (u64, Id) { (1, Id::ZERO) }
//!     fn leads(&self, _: &Public, _: Option<&ThreadName>) -> bool { false }
//! }
//!
//! let owner = Key::from_bytes(&[1; 32]);
//! let riff = owner.public();
//! let body = Body::Riff(Riff { user: "mike".into(), host: None });
//! let record = Record::sign(&owner, riff, 1, Id::ZERO, 1_790_000_000_000, &body)?;
//! assert_eq!((record.of(), record.seq(), record.author()), (Type::Riff, 1, riff));
//!
//! let checked = check(record.bytes().to_vec(), &New(riff))?;
//! assert_eq!(checked.body, body);
//! assert_eq!(checked.record.id(), record.id());
//!
//! // One changed byte breaks the signature.
//! let mut bytes = record.bytes().to_vec();
//! bytes[120] ^= 1;
//! assert_eq!(check(bytes, &New(riff)).unwrap_err(), Fault::Signature);
//! # Ok::<(), Fault>(())
//! ```

use std::fmt;

use riff_core::name::ThreadName;

use crate::daemon_key::{Key, Public};
use crate::peer::body::{Body, Plan, Type};

/// The format version of a record.
pub const VERSION: u8 = 1;

/// The tag before the bytes that the author signs.
pub const TAG: &[u8] = b"riff record 1";

/// The length of the bytes before the body.
pub const HEADER: usize = 118;

/// The length of the signature.
pub const SIGNATURE: usize = 64;

/// The ID of a record: the BLAKE3 hash of its bytes. Its text is 64 hex
/// characters in lower case.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(pub [u8; 32]);

impl Id {
    /// The ID before the first record of an author: 32 zero bytes.
    pub const ZERO: Id = Id([0; 32]);
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({self})")
    }
}

impl serde::Serialize for Id {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Id {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        crate::daemon_key::hex32(&text)
            .map(Id)
            .ok_or_else(|| serde::de::Error::custom("not a record ID: 64 hex characters"))
    }
}

/// The bytes of one record, as its author signed them.
#[derive(Clone, PartialEq, Eq)]
pub struct Record {
    bytes: Vec<u8>,
}

impl Record {
    /// A new record of `key` in the riff `riff`, with the sequence number
    /// `seq`, the ID `before` of the record before it, the time `time_ms`
    /// and the body `body`. A body of [`Body::Other`] has no JSON: it
    /// gives [`Fault::Type`].
    pub fn sign(
        key: &Key,
        riff: Public,
        seq: u64,
        before: Id,
        time_ms: u64,
        body: &Body,
    ) -> Result<Record, Fault> {
        let (Some(of), Some(json)) = (body.of(), body.json()) else {
            return Err(Fault::Type);
        };
        Record::sign_json(key, riff, of, seq, before, time_ms, &json)
    }

    /// [`Record::sign`] with the type `of` and the bytes `json` of the
    /// body as they are, for a test of a reader.
    pub fn sign_json(
        key: &Key,
        riff: Public,
        of: Type,
        seq: u64,
        before: Id,
        time_ms: u64,
        json: &[u8],
    ) -> Result<Record, Fault> {
        let size = HEADER + json.len() + SIGNATURE;
        if size > of.largest() {
            return Err(Fault::Size { size, of });
        }
        let length = u32::try_from(json.len()).map_err(|_| Fault::Size { size, of })?;
        let mut bytes = Vec::with_capacity(size);
        bytes.push(VERSION);
        bytes.push(of.byte());
        bytes.extend_from_slice(riff.as_bytes());
        bytes.extend_from_slice(key.public().as_bytes());
        bytes.extend_from_slice(&seq.to_be_bytes());
        bytes.extend_from_slice(&before.0);
        bytes.extend_from_slice(&time_ms.to_be_bytes());
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(json);
        let signature = key.sign(&signed(&bytes));
        bytes.extend_from_slice(&signature);
        Ok(Record { bytes })
    }

    /// The record of `bytes` that passed a check before, for example from
    /// the book of the daemon, with no check again. `None` when the bytes
    /// are shorter than a header and a signature.
    pub fn kept(bytes: Vec<u8>) -> Option<Record> {
        (bytes.len() >= HEADER + SIGNATURE).then_some(Record { bytes })
    }

    /// The bytes of the record.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The ID of the record: the BLAKE3 hash of its bytes.
    pub fn id(&self) -> Id {
        Id(*blake3::hash(&self.bytes).as_bytes())
    }

    /// The format version.
    pub fn version(&self) -> u8 {
        self.bytes[0]
    }

    /// The type.
    pub fn of(&self) -> Type {
        Type::of(self.bytes[1])
    }

    /// The riff ID: the key of the owner.
    pub fn riff(&self) -> Public {
        Public::from_bytes(self.array(2))
    }

    /// The key of the author.
    pub fn author(&self) -> Public {
        Public::from_bytes(self.array(34))
    }

    /// The sequence number.
    pub fn seq(&self) -> u64 {
        u64::from_be_bytes(self.array(66))
    }

    /// The ID of the record before it.
    pub fn before(&self) -> Id {
        Id(self.array(74))
    }

    /// The time of the author, in milliseconds since the Unix epoch
    /// (01M519Z28S7FB4T1H52642JXZ1).
    pub fn time_ms(&self) -> u64 {
        u64::from_be_bytes(self.array(106))
    }

    /// The JSON of the body.
    pub fn body(&self) -> &[u8] {
        &self.bytes[HEADER..self.bytes.len() - SIGNATURE]
    }

    /// The signature.
    pub fn signature(&self) -> [u8; SIGNATURE] {
        self.array(self.bytes.len() - SIGNATURE)
    }

    /// True when the signature checks with the key of the author: check 7.
    /// A fork needs only this check after checks 1 to 4
    /// (01M519Z38G3RAZPKGFA3R2CB3P).
    pub fn signed_by_author(&self) -> bool {
        let unsigned = &self.bytes[..self.bytes.len() - SIGNATURE];
        self.author().verifies(&signed(unsigned), &self.signature())
    }

    /// The `N` bytes at `at`.
    fn array<const N: usize>(&self, at: usize) -> [u8; N] {
        // Each record has HEADER and SIGNATURE bytes or more.
        self.bytes[at..at + N]
            .try_into()
            .expect("a field of a record")
    }
}

impl fmt::Debug for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Record")
            .field("of", &self.of())
            .field("author", &self.author())
            .field("seq", &self.seq())
            .field("id", &self.id())
            .finish_non_exhaustive()
    }
}

/// The key of the author and the sequence number in the bytes `bytes`
/// of a record, with no check. `None` when the bytes are shorter than a
/// header and a signature. A log line names them for a record that
/// fails a check (01M519Z363W5GV4MV5RDB6ECG0).
///
/// ```
/// use riff::daemon_key::Key;
/// use riff::peer::body::{Body, Riff};
/// use riff::peer::record::{Id, Record, author_and_seq};
///
/// let key = Key::from_bytes(&[1; 32]);
/// let body = Body::Riff(Riff { user: "mike".into(), host: None });
/// let record = Record::sign(&key, key.public(), 1, Id::ZERO, 1, &body)?;
/// assert_eq!(author_and_seq(record.bytes()), Some((key.public(), 1)));
/// assert_eq!(author_and_seq(&[1, 1]), None);
/// # Ok::<(), riff::peer::record::Fault>(())
/// ```
pub fn author_and_seq(bytes: &[u8]) -> Option<(Public, u64)> {
    if bytes.len() < HEADER + SIGNATURE {
        return None;
    }
    let author = Public::from_bytes(bytes[34..66].try_into().expect("32 bytes"));
    let seq = u64::from_be_bytes(bytes[66..74].try_into().expect("8 bytes"));
    Some((author, seq))
}

/// The message that the author signs: [`TAG`] and `unsigned`, each byte
/// of the record before the signature.
fn signed(unsigned: &[u8]) -> Vec<u8> {
    [TAG, unsigned].concat()
}

/// A record that passed each check, with its body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checked {
    pub record: Record,
    pub body: Body,
}

/// What the riff of a reader knows of one key, for one record of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Standing {
    /// The owner: the riff ID. Its user is the user of its `riff`
    /// record, `None` before that record.
    Owner { user: Option<String> },
    /// A member key that a record of the owner admits: the user of the
    /// join of the record (01M5328FVZJGCY9VPQFERM7P81). `until` is the
    /// last sequence number that it may write in that join: the last of
    /// its `removed` record, or the number of its `left` record.
    Member { user: String, until: Option<u64> },
    /// No record of the owner admits the key yet.
    Unknown,
}

/// What the records of the owner say of one member key: for the writer
/// check of a `member` record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Admitted {
    /// The `after` of its last `member` record: 0 for its first join.
    pub after: u64,
    /// True after a `removed` record of the key.
    pub removed: bool,
}

/// The state of the riff of a reader that a check needs.
pub trait Context {
    /// The riff ID: the key of the owner.
    fn riff(&self) -> Public;

    /// What the riff knows of the key `key`, for its record `seq`.
    fn standing(&self, key: &Public, seq: u64) -> Standing;

    /// What the records of the owner say of the member key `key`:
    /// `None` for a key with no `member` record.
    fn admitted(&self, key: &Public) -> Option<Admitted>;

    /// The next sequence number of the author `author`, and the ID of
    /// the record before it: `(1, Id::ZERO)` for an author with no
    /// record. That is the ID of its last record, but at the first
    /// record of a join again: the ID of the `member` record of that
    /// join (01M5328FPJT7Q5Q1509BC5F0J7).
    fn next(&self, author: &Public) -> (u64, Id);

    /// True when the daemon `author` runs a session with a lead mark: in
    /// the repository `thread`, or with `None` in any repository. A mark
    /// that another mark names counts too, so the answer reads only the
    /// records of `author` (01M51T4734E3B3Z4BZZJQYYR32).
    fn leads(&self, author: &Public, thread: Option<&ThreadName>) -> bool;
}

/// Why the author of a record may not write it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Author {
    /// No record of the owner admits the key yet. The record can come
    /// again later (01M519Z33N2MW43XY62NYC6RRH).
    Unknown,
    /// The key was removed, or it left, before this record.
    Gone,
}

/// The check that a record failed, in the order of the checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    /// 1: the record is shorter than a header and a signature, its
    /// length field does not agree with its size, or it is larger than
    /// its type takes.
    Size { size: usize, of: Type },
    /// 2: another format version.
    Version(u8),
    /// 2: the byte 0, no type.
    Type,
    /// 3: another riff.
    Riff,
    /// 4: the author may not write in the riff.
    Author(Author),
    /// 5: not the next sequence number of the author.
    Sequence { got: u64, next: u64 },
    /// 6: not the ID of the last record of the author.
    Before,
    /// 7: the signature does not check.
    Signature,
    /// 8: the body is not the JSON of its type.
    Json(String),
    /// 9: the author is not the writer of the type.
    Writer(&'static str),
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fault::Size { size, of } => write!(
                f,
                "the size: a {of} record of {size} bytes (the largest is {} bytes)",
                of.largest()
            ),
            Fault::Version(version) => write!(f, "the version: {version}, not {VERSION}"),
            Fault::Type => f.write_str("the type: 0 is no type"),
            Fault::Riff => f.write_str("the riff: the record is of another riff"),
            Fault::Author(Author::Unknown) => f.write_str("the author: no member record yet"),
            Fault::Author(Author::Gone) => f.write_str("the author: removed, or left"),
            Fault::Sequence { got, next } => {
                write!(f, "the sequence number: {got}, not the next one {next}")
            }
            Fault::Before => f.write_str("the ID before it: not the last record of the author"),
            Fault::Signature => f.write_str("the signature"),
            Fault::Json(error) => write!(f, "the JSON of its type: {error}"),
            Fault::Writer(why) => write!(f, "the writer of its type: {why}"),
        }
    }
}

impl std::error::Error for Fault {}

/// Checks the bytes `bytes` of a record in the riff of `context`, in the
/// order of the checks (01M519Z317AT7KT7Q643NVYZD1). It gives the record
/// with its body, or the first [`Fault`].
pub fn check(bytes: Vec<u8>, context: &impl Context) -> Result<Checked, Fault> {
    // 1: the size of its type.
    let size = bytes.len();
    let of = Type::of(bytes.get(1).copied().unwrap_or_default());
    let fault = Fault::Size { size, of };
    if size < HEADER + SIGNATURE || size > of.largest() {
        return Err(fault);
    }
    let length = u32::from_be_bytes(bytes[114..HEADER].try_into().expect("4 bytes"));
    if usize::try_from(length).ok() != Some(size - HEADER - SIGNATURE) {
        return Err(fault);
    }
    // 2: the version and the type.
    if bytes[0] != VERSION {
        return Err(Fault::Version(bytes[0]));
    }
    if bytes[1] == 0 {
        return Err(Fault::Type);
    }
    let record = Record { bytes };
    // 3: the riff ID.
    if record.riff() != context.riff() {
        return Err(Fault::Riff);
    }
    // 4: the author, in the join of the record.
    let author = record.author();
    let standing = context.standing(&author, record.seq());
    match &standing {
        Standing::Unknown => return Err(Fault::Author(Author::Unknown)),
        Standing::Member {
            until: Some(until), ..
        } if record.seq() > *until => return Err(Fault::Author(Author::Gone)),
        Standing::Owner { .. } | Standing::Member { .. } => {}
    }
    // 5 and 6: the chain.
    let (next, last) = context.next(&author);
    if record.seq() != next {
        return Err(Fault::Sequence {
            got: record.seq(),
            next,
        });
    }
    if record.before() != last {
        return Err(Fault::Before);
    }
    // 7: the signature.
    if !record.signed_by_author() {
        return Err(Fault::Signature);
    }
    // 8: the JSON of its type.
    let body = Body::read(of, record.body()).map_err(|e| Fault::Json(e.to_string()))?;
    // 9: the writer of its type.
    writer(&record, &body, &standing, context).map_err(Fault::Writer)?;
    Ok(Checked { record, body })
}

/// Checks that the author of `record`, of the standing `standing`, is
/// the writer of the type of `body` (01M519Z23WX0A142N74BKAKKYY).
fn writer(
    record: &Record,
    body: &Body,
    standing: &Standing,
    context: &impl Context,
) -> Result<(), &'static str> {
    let owner = matches!(standing, Standing::Owner { .. });
    let user = match standing {
        Standing::Owner { user } => user.as_deref(),
        Standing::Member { user, .. } => Some(user.as_str()),
        Standing::Unknown => None,
    };
    let refuse = |no: bool, why: &'static str| if no { Err(why) } else { Ok(()) };
    // Each type has its arm, with no wildcard: a new type needs its own
    // writer rule.
    match body {
        Body::Riff(_) => refuse(
            !owner || record.seq() != 1,
            "only the first record of the owner makes the riff",
        ),
        Body::Member(member) => {
            refuse(!owner, "only the owner admits a member")?;
            refuse(
                member.key == context.riff(),
                "the owner is no member of its riff",
            )?;
            // The check reads only the records of the owner. So the joins
            // of a key do not depend on the order of the records
            // (01M5328FVZJGCY9VPQFERM7P81).
            match (member.after, context.admitted(&member.key)) {
                (None, None) => Ok(()),
                (None, Some(_)) => Err("the key is a member already, or was one"),
                (Some(_), None) => Err("a join again of a key that was no member"),
                (Some(_), Some(admitted)) if admitted.removed => {
                    Err("a join again of a removed key")
                }
                (Some(after), Some(admitted)) if after <= admitted.after => {
                    Err("a join again names no left record after the last join of the key")
                }
                (Some(_), Some(_)) => Ok(()),
            }
        }
        Body::Removed(removed) => {
            refuse(!owner, "only the owner removes a member")?;
            refuse(
                removed.key == context.riff(),
                "the owner cannot remove itself",
            )
        }
        Body::Left(_) => refuse(owner, "the owner cannot leave"),
        Body::Session(session) => refuse(
            Some(session.session.who().user()) != user,
            "a session record of another user than the user of its key",
        ),
        Body::Message(message) => refuse(
            Some(message.from.who().user()) != user,
            "a message of another user than the user of its key",
        ),
        Body::Plan(plan) => {
            let whole_riff = match plan {
                Plan::Pause { thread } => thread.is_none(),
                Plan::Hold { .. } | Plan::Free { .. } | Plan::Resume { .. } | Plan::Other => false,
            };
            refuse(whole_riff && !owner, "only the owner pauses the whole riff")?;
            refuse(
                !owner && !context.leads(&record.author(), plan.thread()),
                "only a daemon of a lead, or the daemon of the owner, writes a plan",
            )
        }
        // A type that the reader does not know has no writer check
        // (01M519Z26AP5NZSFHWRQ2QCV9D).
        Body::Other => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use riff_core::wire;

    use super::*;
    use crate::peer::body::{Left, Member, Message, Removed, Riff, Session};

    /// A riff of the owner [1; 32] and the member [2; 32] (`brett`), with
    /// no record yet of each. The member [3; 32] left after its record
    /// 4. The member [4; 32] is a lead in `o/r`. The owner removed the
    /// member [7; 32] after its record 2.
    struct Here {
        riff: Public,
        members: BTreeMap<Public, Standing>,
        removed: BTreeSet<Public>,
        next: BTreeMap<Public, (u64, Id)>,
        leads: Option<(Public, ThreadName)>,
    }

    fn key(byte: u8) -> Key {
        Key::from_bytes(&[byte; 32])
    }

    impl Here {
        fn new() -> Here {
            let mut members = BTreeMap::new();
            let member = |user: &str, until| Standing::Member {
                user: user.into(),
                until,
            };
            members.insert(key(2).public(), member("brett", None));
            members.insert(key(3).public(), member("ann", Some(4)));
            members.insert(key(4).public(), member("mike", None));
            members.insert(key(7).public(), member("zed", Some(2)));
            Here {
                riff: key(1).public(),
                members,
                removed: BTreeSet::from([key(7).public()]),
                next: BTreeMap::new(),
                leads: Some((key(4).public(), "o/r".parse().unwrap())),
            }
        }
    }

    impl Context for Here {
        fn riff(&self) -> Public {
            self.riff
        }

        fn standing(&self, key: &Public, _: u64) -> Standing {
            if *key == self.riff {
                return Standing::Owner {
                    user: Some("mike".into()),
                };
            }
            self.members.get(key).cloned().unwrap_or(Standing::Unknown)
        }

        /// Each member joined one time.
        fn admitted(&self, key: &Public) -> Option<Admitted> {
            self.members.contains_key(key).then(|| Admitted {
                after: 0,
                removed: self.removed.contains(key),
            })
        }

        fn next(&self, author: &Public) -> (u64, Id) {
            self.next.get(author).copied().unwrap_or((1, Id::ZERO))
        }

        fn leads(&self, author: &Public, thread: Option<&ThreadName>) -> bool {
            self.leads
                .as_ref()
                .is_some_and(|(lead, at)| lead == author && thread.is_none_or(|t| t == at))
        }
    }

    fn riff_body() -> Body {
        Body::Riff(Riff {
            user: "mike".into(),
            host: None,
        })
    }

    fn message(user: &str) -> Body {
        Body::Message(Box::new(Message {
            from: format!("riff://{user}@kadomony/o/r?session=s1")
                .parse()
                .unwrap(),
            thread: "o/r".parse().unwrap(),
            to: vec![],
            kind: wire::Kind::Message,
            body: "hello".into(),
        }))
    }

    /// The bytes of a good first record of `author` with `body`.
    fn good(author: u8, body: &Body) -> Vec<u8> {
        Record::sign(&key(author), key(1).public(), 1, Id::ZERO, 7, body)
            .unwrap()
            .bytes()
            .to_vec()
    }

    #[test]
    fn a_good_record_passes_and_keeps_its_bytes() {
        let here = Here::new();
        let bytes = good(2, &message("brett"));
        let checked = check(bytes.clone(), &here).unwrap();
        assert_eq!(checked.record.bytes(), bytes.as_slice());
        assert_eq!(checked.body, message("brett"));
        let record = &checked.record;
        assert_eq!(record.version(), VERSION);
        assert_eq!(record.of(), Type::Message);
        assert_eq!(record.riff(), key(1).public());
        assert_eq!(record.author(), key(2).public());
        assert_eq!(
            (record.seq(), record.before(), record.time_ms()),
            (1, Id::ZERO, 7)
        );
        assert_eq!(record.id(), Id(*blake3::hash(&bytes).as_bytes()));
        assert_eq!(&bytes[118..122], b"{\"fr");
    }

    #[test]
    fn the_second_record_names_the_first() {
        let mut here = Here::new();
        let first = check(good(2, &message("brett")), &here).unwrap().record;
        here.next.insert(key(2).public(), (2, first.id()));
        let second = Record::sign(&key(2), here.riff, 2, first.id(), 8, &message("brett")).unwrap();
        check(second.bytes().to_vec(), &here).unwrap();
        let gap = Record::sign(&key(2), here.riff, 3, first.id(), 8, &message("brett")).unwrap();
        assert_eq!(
            check(gap.bytes().to_vec(), &here).unwrap_err(),
            Fault::Sequence { got: 3, next: 2 }
        );
        let wrong = Record::sign(&key(2), here.riff, 2, Id::ZERO, 8, &message("brett")).unwrap();
        assert_eq!(
            check(wrong.bytes().to_vec(), &here).unwrap_err(),
            Fault::Before
        );
    }

    /// Writes `value` big-endian at `at` in `bytes`.
    fn put(bytes: &mut [u8], at: usize, value: &[u8]) {
        bytes[at..at + value.len()].copy_from_slice(value);
    }

    /// A record that is good but for one fault.
    fn with_fault(fault: &str) -> (Vec<u8>, Fault) {
        let here = Here::new();
        let body = message("brett");
        match fault {
            "size" => {
                let mut bytes = good(2, &body);
                bytes.pop();
                let size = bytes.len();
                (
                    bytes,
                    Fault::Size {
                        size,
                        of: Type::Message,
                    },
                )
            }
            "version" => {
                let mut bytes = good(2, &body);
                bytes[0] = 2;
                (bytes, Fault::Version(2))
            }
            "type" => {
                let mut bytes = good(2, &body);
                bytes[1] = 0;
                (bytes, Fault::Type)
            }
            "riff" => {
                let other = Record::sign(&key(2), key(9).public(), 1, Id::ZERO, 7, &body).unwrap();
                (other.bytes().to_vec(), Fault::Riff)
            }
            "author" => (good(5, &message("eve")), Fault::Author(Author::Unknown)),
            "sequence" => {
                let late = Record::sign(&key(2), here.riff, 2, Id::ZERO, 7, &body).unwrap();
                (late.bytes().to_vec(), Fault::Sequence { got: 2, next: 1 })
            }
            "before" => {
                let wrong = Record::sign(&key(2), here.riff, 1, Id([1; 32]), 7, &body).unwrap();
                (wrong.bytes().to_vec(), Fault::Before)
            }
            "signature" => {
                let mut bytes = good(2, &body);
                let last = bytes.len() - 1;
                bytes[last] ^= 1;
                (bytes, Fault::Signature)
            }
            "json" => {
                let bad = Record::sign_json(
                    &key(2),
                    here.riff,
                    Type::Message,
                    1,
                    Id::ZERO,
                    7,
                    br#"{"body":"no from"}"#,
                )
                .unwrap();
                let error = serde_json::from_slice::<Message>(br#"{"body":"no from"}"#)
                    .unwrap_err()
                    .to_string();
                (bad.bytes().to_vec(), Fault::Json(error))
            }
            "writer" => (
                good(2, &message("mike")),
                Fault::Writer("a message of another user than the user of its key"),
            ),
            other => panic!("no fault {other}"),
        }
    }

    const ORDER: [&str; 10] = [
        "size",
        "version",
        "type",
        "riff",
        "author",
        "sequence",
        "before",
        "signature",
        "json",
        "writer",
    ];

    /// 01M519Z317AT7KT7Q643NVYZD1: each check fails on its own fault.
    #[test]
    fn each_check_fails_on_its_own_fault() {
        let here = Here::new();
        for name in ORDER {
            let (bytes, fault) = with_fault(name);
            assert_eq!(check(bytes, &here).unwrap_err(), fault, "{name}");
        }
    }

    /// 01M519Z317AT7KT7Q643NVYZD1: of two faults, the check that comes
    /// first gives its fault. A record with the fault of a later check
    /// gets a second fault of each earlier check.
    #[test]
    fn the_first_check_in_the_order_gives_the_fault() {
        let here = Here::new();
        for (at, later) in ORDER.iter().enumerate().skip(1) {
            let (bytes, _) = with_fault(later);
            for earlier in &ORDER[..at] {
                let mut bytes = bytes.clone();
                let expected = match *earlier {
                    "size" => {
                        bytes.truncate(HEADER);
                        Fault::Size {
                            size: HEADER,
                            of: Type::of(bytes[1]),
                        }
                    }
                    "version" => {
                        bytes[0] = 9;
                        Fault::Version(9)
                    }
                    "type" => {
                        bytes[1] = 0;
                        Fault::Type
                    }
                    "riff" => {
                        put(&mut bytes, 2, key(9).public().as_bytes());
                        Fault::Riff
                    }
                    "author" => {
                        put(&mut bytes, 34, key(5).public().as_bytes());
                        Fault::Author(Author::Unknown)
                    }
                    "sequence" => {
                        put(&mut bytes, 66, &5u64.to_be_bytes());
                        Fault::Sequence { got: 5, next: 1 }
                    }
                    "before" => {
                        put(&mut bytes, 74, &[7; 32]);
                        Fault::Before
                    }
                    "signature" => {
                        let last = bytes.len() - 1;
                        bytes[last] ^= 1;
                        Fault::Signature
                    }
                    "json" => continue,
                    other => panic!("no fault {other}"),
                };
                assert_eq!(
                    check(bytes, &here).unwrap_err(),
                    expected,
                    "{earlier} before {later}"
                );
            }
        }
    }

    /// 01M519Z26AP5NZSFHWRQ2QCV9D: a body with a field that the reader
    /// does not know passes.
    #[test]
    fn a_field_that_the_reader_does_not_know_passes() {
        let here = Here::new();
        let json = br#"{"from":"riff://brett@kadomony/o/r?session=s1","thread":"o/r","body":"hi","mood":"glad"}"#;
        let record =
            Record::sign_json(&key(2), here.riff, Type::Message, 1, Id::ZERO, 7, json).unwrap();
        let checked = check(record.bytes().to_vec(), &here).unwrap();
        let Body::Message(message) = checked.body else {
            panic!("not a message")
        };
        assert_eq!(message.body, "hi");
        assert_eq!(checked.record.body(), json);
    }

    /// 01M519Z26AP5NZSFHWRQ2QCV9D: a record of a type that the reader does
    /// not know passes the checks of the bytes, the chain and the
    /// signature, and gives no body to apply.
    #[test]
    fn a_type_that_the_reader_does_not_know_is_kept_and_not_applied() {
        let here = Here::new();
        let json = b"anything, also no JSON";
        let record =
            Record::sign_json(&key(2), here.riff, Type::Other(42), 1, Id::ZERO, 7, json).unwrap();
        let checked = check(record.bytes().to_vec(), &here).unwrap();
        assert_eq!(checked.body, Body::Other);
        assert_eq!(checked.record.of(), Type::Other(42));
        // Its largest size is 64 KiB.
        let big = vec![b'x'; 64 * 1024 - HEADER - SIGNATURE + 1];
        let fault = Record::sign_json(&key(2), here.riff, Type::Other(42), 1, Id::ZERO, 7, &big)
            .unwrap_err();
        assert!(matches!(fault, Fault::Size { size: 65537, .. }), "{fault}");
        // The chain and the signature still count.
        let mut bytes = record.bytes().to_vec();
        bytes[120] ^= 1;
        assert_eq!(check(bytes, &here).unwrap_err(), Fault::Signature);
    }

    #[test]
    fn a_record_larger_than_its_type_is_refused() {
        let here = Here::new();
        let user = "m".repeat(1024);
        let fault = Record::sign(
            &key(1),
            here.riff,
            1,
            Id::ZERO,
            7,
            &Body::Riff(Riff { user, host: None }),
        )
        .unwrap_err();
        assert!(
            matches!(fault, Fault::Size { of: Type::Riff, .. }),
            "{fault}"
        );
        // A record of a reader of another build, with the type riff and
        // 2 KiB, fails the size of its type.
        let json = format!(r#"{{"user":"{}"}}"#, "m".repeat(1900));
        let record = Record::sign_json(
            &key(1),
            here.riff,
            Type::Message,
            1,
            Id::ZERO,
            7,
            json.as_bytes(),
        )
        .unwrap();
        let mut bytes = record.bytes().to_vec();
        bytes[1] = Type::Riff.byte();
        assert!(matches!(
            check(bytes, &here).unwrap_err(),
            Fault::Size { of: Type::Riff, .. }
        ));
    }

    /// 01M519Z23WX0A142N74BKAKKYY: the writer of each type.
    #[test]
    fn each_type_has_its_writer() {
        let here = Here::new();
        let session = |user: &str| {
            Body::Session(Box::new(Session {
                session: format!("riff://{user}@h/o/r?session=s1").parse().unwrap(),
                worker: false,
                lead: None,
                claims: vec![],
                status: None,
                ended: false,
            }))
        };
        let again = |of: u8, user: &str, after: Option<u64>| {
            Body::Member(Member {
                key: key(of).public(),
                user: user.into(),
                host: "h".into(),
                after,
            })
        };
        let admit = |of: u8, user: &str| again(of, user, None);
        let member = admit(6, "eve");
        let removed = |of: u8| {
            Body::Removed(Removed {
                key: key(of).public(),
                last: 3,
            })
        };
        let hold = |thread: &str| {
            Body::Plan(Plan::Hold {
                thread: thread.parse().unwrap(),
                item: "issue-12".into(),
                reason: String::new(),
            })
        };
        let free = Body::Plan(Plan::Free { ends: vec![] });
        let riff_pause = Body::Plan(Plan::Pause { thread: None });
        let left = Body::Left(Left {});
        // (author, body, the fault or none)
        let cases: Vec<(u8, Body, Option<&str>)> = vec![
            (1, riff_body(), None),
            (
                2,
                riff_body(),
                Some("only the first record of the owner makes the riff"),
            ),
            (1, member.clone(), None),
            (2, member, Some("only the owner admits a member")),
            (
                1,
                admit(1, "mike"),
                Some("the owner is no member of its riff"),
            ),
            (
                1,
                admit(2, "eve"),
                Some("the key is a member already, or was one"),
            ),
            (
                1,
                admit(3, "ann"),
                Some("the key is a member already, or was one"),
            ),
            // 01M5328FPJT7Q5Q1509BC5F0J7: ann left at 4 and joins again.
            (1, again(3, "eve", Some(4)), None),
            (2, again(3, "ann", Some(4)), Some("only the owner admits a member")),
            (
                1,
                again(3, "ann", Some(0)),
                Some("a join again names no left record after the last join of the key"),
            ),
            (
                1,
                again(6, "eve", Some(4)),
                Some("a join again of a key that was no member"),
            ),
            (
                1,
                again(7, "zed", Some(9)),
                Some("a join again of a removed key"),
            ),
            (1, removed(2), None),
            (2, removed(4), Some("only the owner removes a member")),
            (1, removed(1), Some("the owner cannot remove itself")),
            (2, left.clone(), None),
            (1, left, Some("the owner cannot leave")),
            (1, session("mike"), None),
            (2, session("brett"), None),
            (
                2,
                session("mike"),
                Some("a session record of another user than the user of its key"),
            ),
            (2, message("brett"), None),
            (1, message("mike"), None),
            (1, hold("o/r"), None),
            (1, free.clone(), None),
            (1, riff_pause.clone(), None),
            (4, hold("o/r"), None),
            (4, free.clone(), None),
            (
                4,
                hold("o/other"),
                Some("only a daemon of a lead, or the daemon of the owner, writes a plan"),
            ),
            (
                2,
                free,
                Some("only a daemon of a lead, or the daemon of the owner, writes a plan"),
            ),
            (4, riff_pause, Some("only the owner pauses the whole riff")),
        ];
        for (author, body, fault) in cases {
            let bytes = good(author, &body);
            let got = check(bytes, &here).err();
            assert_eq!(got, fault.map(Fault::Writer), "{author} {body:?}");
        }
    }

    /// The owner writes a riff record as its second record: refused. A
    /// member that left writes no record after its left record.
    #[test]
    fn the_riff_comes_first_and_a_member_that_left_is_gone() {
        let mut here = Here::new();
        here.next.insert(key(1).public(), (2, Id([5; 32])));
        let again = Record::sign(&key(1), here.riff, 2, Id([5; 32]), 7, &riff_body()).unwrap();
        assert_eq!(
            check(again.bytes().to_vec(), &here).unwrap_err(),
            Fault::Writer("only the first record of the owner makes the riff")
        );
        here.next.insert(key(3).public(), (5, Id([6; 32])));
        let late = Record::sign(&key(3), here.riff, 5, Id([6; 32]), 7, &message("ann")).unwrap();
        assert_eq!(
            check(late.bytes().to_vec(), &here).unwrap_err(),
            Fault::Author(Author::Gone)
        );
    }
}
