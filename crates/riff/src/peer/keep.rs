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
//!   fault (01M51BFYQVCFEWCJZERBD4S5N5); it drops a record of an author
//!   that forked; it drops a record that can pass later
//!   (01M519Z33N2MW43XY62NYC6RRH); it keeps the proof of a fork
//!   (01M519Z38G3RAZPKGFA3R2CB3P); it refuses a record that fails a
//!   check (01M519Z363W5GV4MV5RDB6ECG0); else it keeps the record in the
//!   book, then applies it.
//! - [`Peers::open`] reads the book, and checks and applies each record
//!   in the order in which the daemon kept them.
//!
//! ```mermaid
//! flowchart LR
//!     B["the bytes"] --> D{"a record that it has?"}
//!     D -->|yes| Dup["Duplicate: no fault"]
//!     D -->|no| F{"an author that forked?"}
//!     F -->|yes| Fd["Forked: no fault"]
//!     F -->|no| C{"check"}
//!     C -->|"a later sequence number, an author not admitted yet"| L["Later: no fault"]
//!     C -->|"an old sequence number, another ID, a good signature"| P["Fork: keep the proof"]
//!     C -->|"another fault"| R["Refused"]
//!     C -->|"each check passes"| K["the book, then the state"]
//! ```
//!
//! # The records for the peer links
//!
//! The peer links of the daemon ([`crate::peer::sync`]) send each record
//! to the daemons that lack it. So [`Peers`] also keeps each record of
//! each author in memory, in sequence order: its chain.
//!
//! - [`Peers::heads`] gives, for each author, the last sequence number
//!   and the ID of that record (01M519Z2YSXADT45A1R0N70WRQ,
//!   01M51Z9ZT69AQ0V2PX8T439NA5).
//! - [`Peers::lacks`] gives the records that a daemon with other heads
//!   lacks, in this order: the proof of each fork
//!   (01M51ZA001C8MZM4BTBYTXRJ8M); the record of this daemon at each head
//!   of the other daemon that has another ID, a fork that the other daemon
//!   does not see yet (01M51Z9ZT69AQ0V2PX8T439NA5); the records of the
//!   owner; the records of each other author. The records of one author
//!   come in sequence order.
//! - [`Peers::subscribe`] gives each [`Fresh`] record: each record that
//!   the daemon keeps, with the daemon that sent it, in the order in
//!   which it keeps them, and the proof of each new fork.
//!
//! # A fork
//!
//! A fork is two records of one author with the same sequence number,
//! each with a good signature (01M519Z38G3RAZPKGFA3R2CB3P). [`Peers`]
//! keeps the first one in its book, and both in the file [`FORKS`]: the
//! proof. It keeps no next record of that author. A new open reads the
//! proofs again.
//!
//! ```
//! use riff::daemon_key::Key;
//! use riff::peer::body::{Body, Riff};
//! use riff::peer::keep::{Outcome, Peers};
//! use riff::peer::record::{Id, Record};
//!
//! let dir = tempfile::tempdir()?;
//! let owner = Key::from_bytes(&[1; 32]);
//! let riff = owner.public();
//! let mut peers = Peers::open(dir.path(), riff, |_| {})?;
//! peers.write(&owner, &Body::Riff(Riff { user: "mike".into(), host: None }), 1)?;
//! // The owner signs another first record: a fork.
//! let other = Body::Riff(Riff { user: "ann".into(), host: None });
//! let second = Record::sign(&owner, riff, 1, Id::ZERO, 2, &other)?;
//! let Outcome::Fork(fork) = peers.keep(second.bytes().to_vec(), None)? else {
//!     panic!("no fork");
//! };
//! assert_eq!(fork.second, second);
//! assert_eq!(peers.state().owner(), Some("mike"));
//! drop(peers);
//! let peers = Peers::open(dir.path(), riff, |_| {})?;
//! assert_eq!(peers.forks()[&riff].second, second);
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Result, bail};
use tokio::sync::broadcast;

use crate::daemon_key::{Key, Public};
use crate::peer::body::Body;
use crate::peer::book::Book;
use crate::peer::record::{Checked, Context, Fault, Id, Record, author_and_seq, check};
use crate::peer::state::{Applied, State, later};

/// The name of the file of the proofs of the forks in the folder
/// `state`: the two records of each fork, one after the other.
pub const FORKS: &str = "forks";

/// The most fresh records that wait for one slow peer link. A link that
/// falls more behind ends, and its next connect sends what it lacks
/// ([`crate::peer::sync`]).
pub const FRESH: usize = 4096;

/// The heads of a daemon: for each author, the last sequence number and
/// the ID of that record (01M519Z2YSXADT45A1R0N70WRQ).
pub type Heads = BTreeMap<Public, (u64, Id)>;

/// What [`Peers::keep`] did with a record of another daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It kept and applied the record.
    Kept(Applied),
    /// It has the record already: it dropped it, with no fault.
    Duplicate,
    /// The author forked before: the daemon keeps no next record of it.
    /// It dropped the record, with no fault.
    Forked,
    /// The record can pass later: it dropped it, with no fault.
    Later(Fault),
    /// The record and the record of the daemon with the same author and
    /// sequence number are a fork. The daemon keeps the proof.
    Fork(Fork),
    /// The record failed a check.
    Refused(Fault),
}

/// The proof of a fork (01M519Z38G3RAZPKGFA3R2CB3P): two records of one
/// author with the same sequence number, each with a good signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fork {
    /// The record that the daemon kept: it is in its book.
    pub first: Record,
    /// The other record.
    pub second: Record,
}

/// A record for the peer links ([`Peers::subscribe`]).
#[derive(Clone, Debug)]
pub enum Fresh {
    /// A record that the daemon kept. `from` is the daemon that sent it,
    /// `None` for a record of this daemon.
    Kept {
        record: Record,
        from: Option<Public>,
    },
    /// The proof of a new fork.
    Fork(Fork),
    /// The daemon is in another riff now: after a join or a leave
    /// ([`Peers::reopen`], 01M521J6WZT2KR5ZQE9DZ9A5AC).
    Switched,
}

/// The riff of peers of a daemon: its state and its book.
#[derive(Debug)]
pub struct Peers {
    state: State,
    book: Book,
    /// Each record that the daemon keeps, by author, in sequence order:
    /// the record N of an author is at N - 1.
    chains: BTreeMap<Public, Vec<Record>>,
    /// The file of the proofs of the forks.
    proofs: Book,
    /// The proof of each author that forked.
    forks: BTreeMap<Public, Fork>,
    fresh: broadcast::Sender<Fresh>,
}

impl Peers {
    /// Opens the book in the folder `dir`, and applies each record in it
    /// to a state of the riff `riff`. It gives `say` one line for each
    /// record that fails a check: the book of a daemon holds only
    /// records that passed. It reads the proofs of the forks in [`FORKS`]
    /// too.
    pub fn open(dir: &Path, riff: Public, say: impl FnMut(String)) -> Result<Peers> {
        Peers::open_with(dir, riff, broadcast::channel(FRESH).0, say)
    }

    /// The records of the riff `riff` in the folder `dir`, as
    /// [`Peers::open`], in place of these records: after a join or a
    /// leave (01M521J6WZT2KR5ZQE9DZ9A5AC). The records of this riff stay
    /// in their folder. Each subscriber gets [`Fresh::Switched`], then
    /// the fresh records of the new riff.
    pub fn reopen(&mut self, dir: &Path, riff: Public, say: impl FnMut(String)) -> Result<()> {
        *self = Peers::open_with(dir, riff, self.fresh.clone(), say)?;
        let _ = self.fresh.send(Fresh::Switched);
        Ok(())
    }

    /// [`Peers::open`] with the sender `fresh` of the fresh records.
    fn open_with(
        dir: &Path,
        riff: Public,
        fresh: broadcast::Sender<Fresh>,
        mut say: impl FnMut(String),
    ) -> Result<Peers> {
        let (book, kept) = Book::open(dir)?;
        let (proofs, pairs) = Book::open_file(dir, FORKS)?;
        let (state, chains) = replay(riff, kept, &mut say);
        let mut forks = BTreeMap::new();
        // A crash between the two records of a proof leaves one: it
        // proves nothing.
        for pair in pairs.chunks_exact(2) {
            let (Some(first), Some(second)) =
                (Record::kept(pair[0].clone()), Record::kept(pair[1].clone()))
            else {
                continue;
            };
            forks
                .entry(first.author())
                .or_insert(Fork { first, second });
        }
        let mut peers = Peers {
            state,
            book,
            chains,
            proofs,
            forks,
            fresh,
        };
        // A crash after the append of a `removed` record and before the
        // drop leaves the later records of its key in the book.
        let removed: Vec<(Public, u64)> = peers
            .state
            .members()
            .iter()
            .filter(|(_, member)| member.removed)
            .filter_map(|(key, member)| Some((*key, member.until?)))
            .collect();
        for (key, last) in removed {
            peers.drop_after(key, last)?;
        }
        Ok(peers)
    }

    /// The state of the riff.
    pub fn state(&self) -> &State {
        &self.state
    }

    /// The proof of each author that forked.
    pub fn forks(&self) -> &BTreeMap<Public, Fork> {
        &self.forks
    }

    /// Each [`Fresh`] record from now on, in the order in which the
    /// daemon keeps them.
    pub fn subscribe(&self) -> broadcast::Receiver<Fresh> {
        self.fresh.subscribe()
    }

    /// The heads of the daemon.
    pub fn heads(&self) -> Heads {
        self.state.heads().clone()
    }

    /// The records that a daemon with the heads `theirs` lacks, in the
    /// order to send them. See "The records for the peer links".
    pub fn lacks(&self, theirs: &Heads) -> Vec<Record> {
        let mut first = Vec::new();
        for fork in self.forks.values() {
            first.extend([fork.first.clone(), fork.second.clone()]);
        }
        let (mut owner, mut rest) = (Vec::new(), Vec::new());
        for (author, chain) in &self.chains {
            let (seq, id) = theirs.get(author).copied().unwrap_or((0, Id::ZERO));
            let had = usize::try_from(seq).unwrap_or(usize::MAX);
            if let Some(mine) = had.checked_sub(1).and_then(|at| chain.get(at))
                && mine.id() != id
            {
                // A fork that the other daemon does not see yet. It keeps
                // no next record of the author after this one.
                first.push(mine.clone());
                continue;
            }
            let lacked = chain.iter().skip(had).cloned();
            if *author == self.state.riff() {
                owner.extend(lacked);
            } else {
                rest.extend(lacked);
            }
        }
        first.extend(owner);
        first.extend(rest);
        first
    }

    /// Writes a record of `key` with the body `body` and the time
    /// `time_ms`, and applies it. A daemon that joined again writes no
    /// record before it has its own records up to its `left` record: the
    /// record would fork its chain (01M5328FPJT7Q5Q1509BC5F0J7).
    pub fn write(&mut self, key: &Key, body: &Body, time_ms: u64) -> Result<(Id, Applied)> {
        let (seq, before) = self.state.next(&key.public());
        if let Some(member) = self.state.members().get(&key.public()) {
            let after = member.last_join().after;
            if seq <= after {
                bail!("{}", crate::text::peer_lacks_own_records(seq - 1, after));
            }
        }
        let record = Record::sign(key, self.state.riff(), seq, before, time_ms, body)?;
        let checked = check(record.bytes().to_vec(), &self.state)?;
        let id = checked.record.id();
        Ok((id, self.apply(&checked, None)?))
    }

    /// Takes the bytes of a record that the daemon `from` sent, or `None`.
    /// See [`Outcome`].
    pub fn keep(&mut self, bytes: Vec<u8>, from: Option<Public>) -> Result<Outcome> {
        let id = Id(*blake3::hash(&bytes).as_bytes());
        if self.state.has(&id) {
            return Ok(Outcome::Duplicate);
        }
        let author = author_and_seq(&bytes).map(|(author, _)| author);
        if author.is_some_and(|author| self.forks.contains_key(&author)) {
            return Ok(Outcome::Forked);
        }
        match check(bytes.clone(), &self.state) {
            Ok(checked) => Ok(Outcome::Kept(self.apply(&checked, from)?)),
            Err(fault @ Fault::Sequence { got, next }) if got < next => self.fork(bytes, fault),
            Err(fault) if later(&fault) => Ok(Outcome::Later(fault)),
            Err(fault) => Ok(Outcome::Refused(fault)),
        }
    }

    /// The record `bytes` passed checks 1 to 4. It has an old sequence
    /// number and another ID than the record of the daemon there. With a
    /// good signature, the two are a fork: keep the proof. Else refuse
    /// the record with `fault`.
    fn fork(&mut self, bytes: Vec<u8>, fault: Fault) -> Result<Outcome> {
        let Some(second) = Record::kept(bytes).filter(Record::signed_by_author) else {
            return Ok(Outcome::Refused(fault));
        };
        let author = second.author();
        let at = usize::try_from(second.seq()).unwrap_or(usize::MAX);
        let Some(first) = at
            .checked_sub(1)
            .and_then(|at| self.chains.get(&author)?.get(at))
            .cloned()
        else {
            return Ok(Outcome::Refused(fault));
        };
        self.proofs.append(first.bytes())?;
        self.proofs.append(second.bytes())?;
        let fork = Fork { first, second };
        self.forks.insert(author, fork.clone());
        let _ = self.fresh.send(Fresh::Fork(fork.clone()));
        Ok(Outcome::Fork(fork))
    }

    /// Keeps `checked` in the book, then applies it. A `removed` record
    /// drops each record of its key after its last number. The peer
    /// links get the record, with the daemon `from` that sent it.
    fn apply(&mut self, checked: &Checked, from: Option<Public>) -> Result<Applied> {
        let record = &checked.record;
        self.book.append(record.bytes())?;
        let applied = self.state.keep(checked);
        self.chains
            .entry(record.author())
            .or_default()
            .push(record.clone());
        if let Body::Removed(removed) = &checked.body {
            self.drop_after(removed.key, removed.last)?;
        }
        let _ = self.fresh.send(Fresh::Kept {
            record: record.clone(),
            from,
        });
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
                Record::kept(bytes.clone())
                    .is_none_or(|record| record.author() != key || record.seq() <= last)
            })
            .collect();
        self.book.rewrite(&records)?;
        (self.state, self.chains) = replay(self.state.riff(), records, &mut |_| {});
        Ok(())
    }
}

/// The state of the riff `riff` from the records `kept`, in this order,
/// and the chain of each author. It gives `say` one line for each record
/// that fails a check: the book of a daemon holds only records that
/// passed.
fn replay(
    riff: Public,
    kept: Vec<Vec<u8>>,
    say: &mut impl FnMut(String),
) -> (State, BTreeMap<Public, Vec<Record>>) {
    let mut state = State::new(riff);
    let mut chains: BTreeMap<Public, Vec<Record>> = BTreeMap::new();
    for bytes in kept {
        match check(bytes, &state) {
            Ok(checked) => {
                state.keep(&checked);
                let record = checked.record;
                chains.entry(record.author()).or_default().push(record);
            }
            Err(fault) => say(crate::text::daemon_bad_record(&fault.to_string())),
        }
    }
    (state, chains)
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

    /// The `member` body of `key` with the user `user`.
    fn admit(key: &Key, user: &str) -> Body {
        Body::Member(Member {
            key: key.public(),
            user: user.into(),
            host: "h".into(),
            after: None,
        })
    }

    /// 01M519Z2YSXADT45A1R0N70WRQ: the records that another daemon lacks
    /// come in this order: the proof of each fork, the record at a head
    /// of the other daemon that has another ID, the records of the owner,
    /// then the records of each other author. The records of one author
    /// come in sequence order. A fresh record names the daemon that sent
    /// it, and a new fork is fresh too.
    #[test]
    fn lacks_gives_the_proofs_then_the_owner_then_each_author_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, brett, ann) = (
            Key::from_bytes(&[1; 32]),
            Key::from_bytes(&[2; 32]),
            Key::from_bytes(&[3; 32]),
        );
        let riff = owner.public();
        let mut peers = Peers::open(dir.path(), riff, |_| {}).unwrap();
        let mut fresh = peers.subscribe();
        let riff_body = Body::Riff(Riff {
            user: "mike".into(),
            host: None,
        });
        for body in [riff_body, admit(&brett, "brett"), admit(&ann, "ann")] {
            peers.write(&owner, &body, 1).unwrap();
        }
        let b1 = Record::sign(&brett, riff, 1, Id::ZERO, 2, &message("brett", "b1")).unwrap();
        let b2 = Record::sign(&brett, riff, 2, b1.id(), 3, &message("brett", "b2")).unwrap();
        let a1 = Record::sign(&ann, riff, 1, Id::ZERO, 4, &message("ann", "a1")).unwrap();
        for record in [&b1, &b2, &a1] {
            let from = Some(record.author());
            let outcome = peers.keep(record.bytes().to_vec(), from).unwrap();
            assert!(matches!(outcome, Outcome::Kept(_)), "{outcome:?}");
        }
        let mut got = Vec::new();
        while let Ok(Fresh::Kept { record, from }) = fresh.try_recv() {
            got.push((record.seq(), record.author(), from));
        }
        assert_eq!(got.len(), 6);
        assert_eq!(got[0], (1, riff, None));
        assert_eq!(got[3], (1, brett.public(), Some(brett.public())));

        let owner_chain: Vec<u64> = (1..=3).collect();
        let lacks = |theirs: &Heads| -> Vec<(Public, u64)> {
            peers
                .lacks(theirs)
                .iter()
                .map(|record| (record.author(), record.seq()))
                .collect()
        };
        let all = lacks(&Heads::new());
        assert_eq!(all.len(), 6);
        assert!(all[..3].iter().map(|(_, seq)| *seq).eq(owner_chain));
        assert!(all[..3].iter().all(|(author, _)| *author == riff));
        let (first, second) = if brett.public() < ann.public() {
            (brett.public(), ann.public())
        } else {
            (ann.public(), brett.public())
        };
        assert_eq!(all[3].0, first);
        assert_eq!(all[5].0, second);
        if first == brett.public() {
            assert_eq!(all[3..5], [(first, 1), (first, 2)]);
        }

        // A daemon with each record of the owner and of brett lacks a1.
        let mut theirs = peers.heads();
        theirs.remove(&ann.public());
        assert_eq!(lacks(&theirs), [(ann.public(), 1)]);
        // A daemon whose record 1 of brett has another ID gets the record
        // 1 of this daemon, and no next record of brett.
        theirs.insert(brett.public(), (1, Id([9; 32])));
        assert_eq!(lacks(&theirs), [(brett.public(), 1), (ann.public(), 1)]);

        // A fork of ann: its proof comes first, and it is fresh.
        let other = Record::sign(&ann, riff, 1, Id::ZERO, 5, &message("ann", "x")).unwrap();
        let outcome = peers.keep(other.bytes().to_vec(), None).unwrap();
        assert!(matches!(outcome, Outcome::Fork(_)), "{outcome:?}");
        assert!(matches!(fresh.try_recv(), Ok(Fresh::Fork(fork)) if fork.second == other));
        let proof: Vec<Record> = peers.lacks(&peers.heads()).into_iter().collect();
        assert_eq!(proof, [a1.clone(), other.clone()]);
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
            host: None,
        });
        peers.write(&owner, &body, 1).unwrap();
        let admit = Body::Member(Member {
            key: member.public(),
            user: "brett".into(),
            host: "kadomony".into(),
            after: None,
        });
        peers.write(&owner, &admit, 2).unwrap();

        let first = Record::sign(&member, riff, 1, Id::ZERO, 3, &message("brett", "one")).unwrap();
        let third =
            Record::sign(&member, riff, 3, Id([9; 32]), 5, &message("brett", "three")).unwrap();
        assert!(matches!(
            peers.keep(first.bytes().to_vec(), None).unwrap(),
            Outcome::Kept(Applied::Message { number: 1, .. })
        ));
        assert_eq!(
            peers.keep(first.bytes().to_vec(), None).unwrap(),
            Outcome::Duplicate
        );
        assert_eq!(
            peers.keep(third.bytes().to_vec(), None).unwrap(),
            Outcome::Later(Fault::Sequence { got: 3, next: 2 })
        );
        let stranger = Key::from_bytes(&[3; 32]);
        let unknown = Record::sign(&stranger, riff, 1, Id::ZERO, 6, &message("eve", "hi")).unwrap();
        assert_eq!(
            peers.keep(unknown.bytes().to_vec(), None).unwrap(),
            Outcome::Later(Fault::Author(Author::Unknown))
        );
        let mut bad = Record::sign(&member, riff, 2, first.id(), 7, &message("brett", "two"))
            .unwrap()
            .bytes()
            .to_vec();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert_eq!(
            peers.keep(bad, None).unwrap(),
            Outcome::Refused(Fault::Signature)
        );
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
            after: None,
        });
        let riff_record = Record::sign(
            &owner,
            riff,
            1,
            Id::ZERO,
            1,
            &Body::Riff(Riff {
                user: "mike".into(),
                host: None,
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
            first.keep(record.bytes().to_vec(), None).unwrap();
            second.keep(record.bytes().to_vec(), None).unwrap();
        }
        // The first daemon keeps the chain, then the removed record.
        for record in &chain {
            first.keep(record.bytes().to_vec(), None).unwrap();
        }
        first.keep(removed.bytes().to_vec(), None).unwrap();
        // The second daemon gets the removed record first.
        second.keep(removed.bytes().to_vec(), None).unwrap();
        let outcomes: Vec<Outcome> = chain
            .iter()
            .map(|record| second.keep(record.bytes().to_vec(), None).unwrap())
            .collect();
        assert!(matches!(outcomes[0], Outcome::Kept(_)));
        assert_eq!(outcomes[1], Outcome::Refused(Fault::Author(Author::Gone)));
        assert_eq!(first.state().view(), second.state().view());
        let thread = "o/r".parse().unwrap();
        assert_eq!(first.state().messages(&thread).len(), 1);
        drop(first);
        let first = Peers::open(one.path(), riff, |_| {}).unwrap();
        assert_eq!(first.state().view(), second.state().view());

        // A crash after the append of the removed record, and before the
        // drop: the book still holds the later records of the key. The
        // next open drops them.
        drop(first);
        let three = tempfile::tempdir().unwrap();
        let (mut book, _) = crate::peer::book::Book::open(three.path()).unwrap();
        for record in [
            &riff_record,
            &admit,
            &chain[0],
            &chain[1],
            &chain[2],
            &removed,
        ] {
            book.append(record.bytes()).unwrap();
        }
        drop(book);
        let third = Peers::open(three.path(), riff, |_| {}).unwrap();
        assert_eq!(third.state().view(), second.state().view());
        let (_, kept) = crate::peer::book::Book::open(three.path()).unwrap();
        assert_eq!(kept.len(), 4);
    }

    /// 01M5328FPJT7Q5Q1509BC5F0J7: a daemon that joined again, and lacks
    /// its own records up to its `left` record, writes no record: it would
    /// fork its chain. With its records, its next record names the new
    /// `member` record, and each other daemon keeps it.
    #[test]
    fn a_daemon_that_joined_again_writes_only_after_its_own_records() {
        use crate::peer::body::Left;
        let owner = Key::from_bytes(&[1; 32]);
        let member = Key::from_bytes(&[2; 32]);
        let riff = owner.public();
        let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let mut of_owner = Peers::open(one.path(), riff, |_| {}).unwrap();
        let body = Body::Riff(Riff {
            user: "mike".into(),
            host: None,
        });
        of_owner.write(&owner, &body, 1).unwrap();
        of_owner.write(&owner, &admit(&member, "brett"), 2).unwrap();
        let hello = Record::sign(&member, riff, 1, Id::ZERO, 3, &message("brett", "hi")).unwrap();
        let left = Record::sign(&member, riff, 2, hello.id(), 4, &Body::Left(Left {})).unwrap();
        for record in [&hello, &left] {
            of_owner.keep(record.bytes().to_vec(), None).unwrap();
        }
        let again = Body::Member(Member {
            key: member.public(),
            user: "brett".into(),
            host: "h".into(),
            after: Some(2),
        });
        of_owner.write(&owner, &again, 5).unwrap();

        // The member gets the records of the owner first: it lacks its
        // own two records.
        let mut of_member = Peers::open(two.path(), riff, |_| {}).unwrap();
        for record in of_owner.lacks(&Heads::new()) {
            if record.author() == riff {
                of_member.keep(record.bytes().to_vec(), None).unwrap();
            }
        }
        let error = of_member
            .write(&member, &message("brett", "back"), 6)
            .unwrap_err();
        assert_eq!(error.to_string(), crate::text::peer_lacks_own_records(0, 2));
        assert!(!of_member.state().heads().contains_key(&member.public()));

        for record in [&hello, &left] {
            of_member.keep(record.bytes().to_vec(), None).unwrap();
        }
        let (id, _) = of_member
            .write(&member, &message("brett", "back"), 6)
            .unwrap();
        let back = of_member.lacks(&of_owner.heads());
        assert_eq!(back.len(), 1);
        assert_eq!((back[0].id(), back[0].seq()), (id, 3));
        let outcome = of_owner.keep(back[0].bytes().to_vec(), None).unwrap();
        assert!(matches!(outcome, Outcome::Kept(_)), "{outcome:?}");
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
