//! The records of a daemon: its [`State`] and its [`Book`] together.
//!
//! # Design
//!
//! [`Peers`] is the one way to change the state of the riff of a daemon:
//!
//! - [`Peers::write`] makes a record of the daemon: the next sequence
//!   number of its key, the ID of its last record, the time of the
//!   daemon and the body. It checks the record as each reader does,
//!   keeps it in the book, then applies it.
//! - [`Peers::keep`] takes the bytes of a record of another daemon
//!   ([`Outcome`]): it drops a record that it has already, with no
//!   fault (01M51BFYQVCFEWCJZERBD4S5N5); it drops a record that can pass
//!   later (01M519Z33N2MW43XY62NYC6RRH); it refuses a record that fails
//!   a check (01M519Z363W5GV4MV5RDB6ECG0); else it keeps the record in
//!   the book, then applies it.
//! - [`Peers::open`] reads the book, and checks and applies each record
//!   in the order in which the daemon kept them.
//!
//! ```mermaid
//! flowchart LR
//!     B["the bytes"] --> D{"a record that it has?"}
//!     D -->|yes| Dup["Duplicate: no fault"]
//!     D -->|no| C{"check"}
//!     C -->|"a later sequence number, an author not admitted yet"| L["Later: no fault"]
//!     C -->|"another fault"| R["Refused"]
//!     C -->|"each check passes"| K["the book, then the state"]
//! ```

use std::path::Path;

use anyhow::Result;

use crate::daemon_key::{Key, Public};
use crate::peer::body::Body;
use crate::peer::book::Book;
use crate::peer::record::{Checked, Context, Fault, Id, Record, check};
use crate::peer::state::{Applied, State, later};

/// What [`Peers::keep`] did with a record of another daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It kept and applied the record.
    Kept(Applied),
    /// It has the record already: it dropped it, with no fault.
    Duplicate,
    /// The record can pass later: it dropped it, with no fault.
    Later(Fault),
    /// The record failed a check.
    Refused(Fault),
}

/// The riff of peers of a daemon: its state and its book.
#[derive(Debug)]
pub struct Peers {
    state: State,
    book: Book,
}

impl Peers {
    /// Opens the book in the folder `dir`, and applies each record in it
    /// to a state of the riff `riff`. It gives `say` one line for each
    /// record that fails a check: the book of a daemon holds only
    /// records that passed.
    pub fn open(dir: &Path, riff: Public, mut say: impl FnMut(String)) -> Result<Peers> {
        let (book, kept) = Book::open(dir)?;
        let mut state = State::new(riff);
        for bytes in kept {
            match check(bytes, &state) {
                Ok(checked) => {
                    state.keep(&checked);
                }
                Err(fault) => say(crate::text::daemon_bad_record(&fault.to_string())),
            }
        }
        Ok(Peers { state, book })
    }

    /// The state of the riff.
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Writes a record of `key` with the body `body` and the time
    /// `time_ms`, and applies it.
    pub fn write(&mut self, key: &Key, body: &Body, time_ms: u64) -> Result<(Id, Applied)> {
        let (seq, before) = self.state.next(&key.public());
        let record = Record::sign(key, self.state.riff(), seq, before, time_ms, body)?;
        let checked = check(record.bytes().to_vec(), &self.state)?;
        let id = checked.record.id();
        Ok((id, self.apply(&checked)?))
    }

    /// Takes the bytes of a record of another daemon. See [`Outcome`].
    pub fn keep(&mut self, bytes: Vec<u8>) -> Result<Outcome> {
        let id = Id(*blake3::hash(&bytes).as_bytes());
        if self.state.has(&id) {
            return Ok(Outcome::Duplicate);
        }
        match check(bytes, &self.state) {
            Ok(checked) => Ok(Outcome::Kept(self.apply(&checked)?)),
            Err(fault) if later(&fault) => Ok(Outcome::Later(fault)),
            Err(fault) => Ok(Outcome::Refused(fault)),
        }
    }

    /// Keeps `checked` in the book, then applies it.
    fn apply(&mut self, checked: &Checked) -> Result<Applied> {
        self.book.append(checked.record.bytes())?;
        Ok(self.state.keep(checked))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer::body::{Member, Message, Riff};
    use crate::peer::record::Author;

    fn message(user: &str, body: &str) -> Body {
        Body::Message(Box::new(Message {
            from: format!("riff://{user}@h/o/r?session=s1").parse().unwrap(),
            thread: "o/r".parse().unwrap(),
            to: vec![],
            kind: Default::default(),
            body: body.into(),
        }))
    }

    /// The owner writes its riff and a member. A record of the member is
    /// kept one time, a later one waits, and a bad one is refused. Each
    /// kept record is in the book after a new open.
    #[test]
    fn a_record_is_kept_once_a_later_one_waits_and_the_book_keeps_them() {
        let dir = tempfile::tempdir().unwrap();
        let owner = Key::from_bytes(&[1; 32]);
        let member = Key::from_bytes(&[2; 32]);
        let riff = owner.public();
        let mut peers = Peers::open(dir.path(), riff, |_| {}).unwrap();
        let body = Body::Riff(Riff {
            user: "mike".into(),
        });
        peers.write(&owner, &body, 1).unwrap();
        let admit = Body::Member(Member {
            key: member.public(),
            user: "brett".into(),
            host: "kadomony".into(),
        });
        peers.write(&owner, &admit, 2).unwrap();

        let first = Record::sign(&member, riff, 1, Id::ZERO, 3, &message("brett", "one")).unwrap();
        let third =
            Record::sign(&member, riff, 3, Id([9; 32]), 5, &message("brett", "three")).unwrap();
        assert!(matches!(
            peers.keep(first.bytes().to_vec()).unwrap(),
            Outcome::Kept(Applied::Message { number: 1, .. })
        ));
        assert_eq!(
            peers.keep(first.bytes().to_vec()).unwrap(),
            Outcome::Duplicate
        );
        assert_eq!(
            peers.keep(third.bytes().to_vec()).unwrap(),
            Outcome::Later(Fault::Sequence { got: 3, next: 2 })
        );
        let stranger = Key::from_bytes(&[3; 32]);
        let unknown = Record::sign(&stranger, riff, 1, Id::ZERO, 6, &message("eve", "hi")).unwrap();
        assert_eq!(
            peers.keep(unknown.bytes().to_vec()).unwrap(),
            Outcome::Later(Fault::Author(Author::Unknown))
        );
        let mut bad = Record::sign(&member, riff, 2, first.id(), 7, &message("brett", "two"))
            .unwrap()
            .bytes()
            .to_vec();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert_eq!(peers.keep(bad).unwrap(), Outcome::Refused(Fault::Signature));
        let view = peers.state().view();
        drop(peers);

        let mut lines = Vec::new();
        let peers = Peers::open(dir.path(), riff, |line| lines.push(line)).unwrap();
        assert!(lines.is_empty(), "{lines:?}");
        assert_eq!(peers.state().view(), view);
        assert_eq!(peers.state().heads().len(), 2);
    }

    /// A daemon writes no record that fails its own check: a member
    /// record of a key that is not the owner.
    #[test]
    fn the_daemon_checks_its_own_record() {
        let dir = tempfile::tempdir().unwrap();
        let owner = Key::from_bytes(&[1; 32]);
        let member = Key::from_bytes(&[2; 32]);
        let mut peers = Peers::open(dir.path(), owner.public(), |_| {}).unwrap();
        let error = peers
            .write(&member, &message("brett", "hi"), 1)
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("the author: no member record yet"),
            "{error:#}"
        );
        assert!(peers.state().heads().is_empty());
    }
}
