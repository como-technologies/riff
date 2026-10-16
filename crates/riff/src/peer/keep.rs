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
        let state = replay(riff, kept, &mut say);
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

    /// Keeps `checked` in the book, then applies it. A `removed` record
    /// drops each record of its key after its last number.
    fn apply(&mut self, checked: &Checked) -> Result<Applied> {
        self.book.append(checked.record.bytes())?;
        let applied = self.state.keep(checked);
        if let Body::Removed(removed) = &checked.body {
            self.drop_after(removed.key, removed.last)?;
        }
        Ok(applied)
    }

    /// Drops each record of `key` after the sequence number `last` that
    /// the daemon kept, when there is one: the book holds them no more,
    /// and the state comes again from the other records
    /// (01M51RGNHBCY7GDSCKPE8REY6A). So a daemon that kept such a record
    /// before the `removed` record, and a daemon that refused it after,
    /// have the same state.
    fn drop_after(&mut self, key: Public, last: u64) -> Result<()> {
        if self
            .state
            .heads()
            .get(&key)
            .is_none_or(|(seq, _)| *seq <= last)
        {
            return Ok(());
        }
        let records: Vec<Vec<u8>> = self
            .book
            .records()?
            .into_iter()
            .filter(|bytes| {
                // Each record in the book passed its check, so it has a
                // whole header.
                let author = Public::from_bytes(bytes[34..66].try_into().expect("32 bytes"));
                let seq = u64::from_be_bytes(bytes[66..74].try_into().expect("8 bytes"));
                author != key || seq <= last
            })
            .collect();
        self.book.rewrite(&records)?;
        self.state = replay(self.state.riff(), records, &mut |_| {});
        Ok(())
    }
}

/// The state of the riff `riff` from the records `kept`, in this order.
/// It gives `say` one line for each record that fails a check: the book
/// of a daemon holds only records that passed.
fn replay(riff: Public, kept: Vec<Vec<u8>>, say: &mut impl FnMut(String)) -> State {
    let mut state = State::new(riff);
    for bytes in kept {
        match check(bytes, &state) {
            Ok(checked) => {
                state.keep(&checked);
            }
            Err(fault) => say(crate::text::daemon_bad_record(&fault.to_string())),
        }
    }
    state
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

    /// 01M51RGNHBCY7GDSCKPE8REY6A: a daemon that kept the records 2 and 3
    /// of a key before the `removed` record with the last number 1 drops
    /// them. A daemon that gets the `removed` record first refuses them.
    /// Both have the same state, also after a new open of the book.
    #[test]
    fn a_removed_key_loses_its_records_after_its_last_number_in_each_order() {
        let owner = Key::from_bytes(&[1; 32]);
        let member = Key::from_bytes(&[2; 32]);
        let riff = owner.public();
        let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let mut first = Peers::open(one.path(), riff, |_| {}).unwrap();
        let mut second = Peers::open(two.path(), riff, |_| {}).unwrap();
        let admit = Body::Member(Member {
            key: member.public(),
            user: "brett".into(),
            host: "kadomony".into(),
        });
        let riff_record = Record::sign(
            &owner,
            riff,
            1,
            Id::ZERO,
            1,
            &Body::Riff(Riff {
                user: "mike".into(),
            }),
        )
        .unwrap();
        let admit = Record::sign(&owner, riff, 2, riff_record.id(), 2, &admit).unwrap();
        let removed = Body::Removed(crate::peer::body::Removed {
            key: member.public(),
            last: 1,
        });
        let removed = Record::sign(&owner, riff, 3, admit.id(), 3, &removed).unwrap();
        let mut chain = Vec::new();
        let mut before = Id::ZERO;
        for (seq, text) in [(1, "one"), (2, "two"), (3, "three")] {
            let record =
                Record::sign(&member, riff, seq, before, 3 + seq, &message("brett", text)).unwrap();
            before = record.id();
            chain.push(record);
        }
        for record in [&riff_record, &admit] {
            first.keep(record.bytes().to_vec()).unwrap();
            second.keep(record.bytes().to_vec()).unwrap();
        }
        // The first daemon keeps the chain, then the removed record.
        for record in &chain {
            first.keep(record.bytes().to_vec()).unwrap();
        }
        first.keep(removed.bytes().to_vec()).unwrap();
        // The second daemon gets the removed record first.
        second.keep(removed.bytes().to_vec()).unwrap();
        let outcomes: Vec<Outcome> = chain
            .iter()
            .map(|record| second.keep(record.bytes().to_vec()).unwrap())
            .collect();
        assert!(matches!(outcomes[0], Outcome::Kept(_)));
        assert_eq!(outcomes[1], Outcome::Refused(Fault::Author(Author::Gone)));
        assert_eq!(first.state().view(), second.state().view());
        let thread = "o/r".parse().unwrap();
        assert_eq!(first.state().messages(&thread).len(), 1);
        drop(first);
        let first = Peers::open(one.path(), riff, |_| {}).unwrap();
        assert_eq!(first.state().view(), second.state().view());
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
