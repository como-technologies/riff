//! The state of a riff of peers: what the set of its records gives
//! (01M519Z3AXFKBHJSAW46KWFTXS).
//!
//! # Design
//!
//! The daemon applies each record to this state, and not to
//! `riff-state` (01M51QQGYDR9NZ702NFEQR512W): `riff-state` decides a post
//! by the live sessions and the clock of one server, so two daemons with
//! the same records could get two states.
//!
//! [`State::keep`] applies one checked record. The state of each type
//! comes from one writer, and no part of the state depends on the order
//! of two records of two authors. So two daemons that keep the same
//! records have the same state, but for the numbers of the messages:
//! each daemon numbers the messages in the order in which it kept them
//! (01M519Z28S7FB4T1H52642JXZ1). [`State::view`] gives the state with no
//! numbers, for a test of the order.
//!
//! | Part | From | Rule |
//! |---|---|---|
//! | The owner | the `riff` record | The riff ID is the key of the owner. The record gives its user. |
//! | The members | `member`, `removed` and `left` | A member key has a user and a host for each join. A `removed` record gives the last sequence number that the key may write. A `left` record is the last record of its key in its join. See "A join again". |
//! | The heads | each record | The last sequence number and the ID of each author: [`Context::next`]. The time of its author of the last record of each author: [`State::last_ms`]. |
//! | The sessions | `session` | The last record of each session of each author counts (01M51Q0JZR00GHDQSGGSAQR4HG). A session runs until its record says that it ended. |
//! | The leads | the lead marks of `session` | See "The lead". |
//! | The holds and the pauses | `plan` | See "Holds and pauses". |
//! | The messages | `message` | Each thread keeps each message, with a number of this daemon. |
//!
//! # The lead
//!
//! A session takes the lead with a lead mark (01M51BFYT2ZGN4W5E5R87GPVCM).
//! The record that gives the mark first is the mark of the session. A
//! mark names the mark of the old lead that it replaces, when its daemon
//! knew one. The lead of a user in a repository is the running session of
//! that user there whose mark no mark names. When two such sessions are
//! left, the session of the daemon with the higher key counts.
//!
//! ```mermaid
//! flowchart LR
//!     A["mark A"] --> B["mark B names A"]
//!     B --> L{"the lead"}
//!     C["mark C names no mark"] --> L
//!     L -->|"B and C are not named"| H["the higher key of the daemons of B and C"]
//! ```
//!
//! # A join again
//!
//! A key that left joins again with a new `member` record of the owner
//! (01M5328FPJT7Q5Q1509BC5F0J7). Its `after` is the sequence number of
//! the `left` record of the key. Each `member` record of a key starts a
//! [`Join`]. The join of a record of the key is the last join whose
//! `after` is lower than the number of the record: its user and its host
//! count for that record. The first record of a join again names the
//! `member` record of the join as the record before it
//! ([`Context::next`]). So a record that the key signed after its `left`
//! record, and before the new `member` record, stays refused: before that
//! record, its author left; after it, it names the wrong record before it.
//!
//! The `member` records come in the order of the chain of the owner, and
//! the writer check of a join again reads only the records of the owner
//! (01M5328FVZJGCY9VPQFERM7P81). So two daemons that get the first
//! `member` record, the `left` record and the new `member` record in two
//! orders have the same joins, the same user and the same state of the
//! key.
//!
//! ```mermaid
//! flowchart LR
//!     M1["member: brett, after none"] --> R1["records 1 to 6 of the key: brett"]
//!     R1 --> L["left: record 7"]
//!     L --> M2["member: ann, after 7"]
//!     M2 --> R2["record 8 names the member record of ann: ann"]
//! ```
//!
//! # Holds and pauses
//!
//! A hold and a pause are `plan` records (01M519Z3J7Z0M3PNDY249Q57FH). A
//! free names the ID of each hold that it ends, and a resume names the
//! ID of each pause that it ends. A free or a resume ends a record only
//! when its author is a daemon of the user of that record, or the daemon
//! of the owner. A pause of the whole riff ends only by a resume of the
//! owner. An item is held while one of its holds has no free that ends
//! it. So a free that comes before its hold ends the hold when the hold
//! comes.
//!
//! # Example
//!
//! ```
//! use riff::daemon_key::Key;
//! use riff::peer::body::{Body, Message, Riff};
//! use riff::peer::record::{Id, Record, check};
//! use riff::peer::state::State;
//!
//! let owner = Key::from_bytes(&[1; 32]);
//! let mut state = State::new(owner.public());
//! let riff = Body::Riff(Riff { user: "mike".into(), host: None });
//! let record = Record::sign(&owner, owner.public(), 1, Id::ZERO, 1, &riff)?;
//! state.keep(&check(record.bytes().to_vec(), &state)?);
//!
//! let message = Body::Message(Box::new(Message {
//!     from: "riff://mike@thelio/o/r?session=s1".parse().unwrap(),
//!     thread: "o/r".parse().unwrap(),
//!     to: vec![],
//!     kind: Default::default(),
//!     body: "hello".into(),
//! }));
//! let record = Record::sign(&owner, owner.public(), 2, record.id(), 2, &message)?;
//! state.keep(&check(record.bytes().to_vec(), &state)?);
//! let thread = "o/r".parse().unwrap();
//! assert_eq!(state.messages(&thread)[0].number, 1);
//! assert_eq!(state.messages(&thread)[0].message.body, "hello");
//! # Ok::<(), riff::peer::record::Fault>(())
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;

use riff_core::name::{SessionUri, ThreadName};

use crate::daemon_key::Public;
use crate::peer::body::{Body, Message, Plan, Session};
use crate::peer::record::{Admitted, Author, Checked, Context, Fault, Id, Standing};

/// A member key. See "A join again".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    /// The user of its last join.
    pub user: String,
    /// The host of its last join.
    pub host: String,
    /// The last sequence number that the key may write in its last join:
    /// the last of its `removed` record, or the number of its `left`
    /// record after its last join.
    pub until: Option<u64>,
    /// True after a `removed` record of the key.
    pub removed: bool,
    /// True after a `left` record of the key after its last join.
    pub left: bool,
    /// Each join of the key, in the order of the chain of the owner.
    pub joins: Vec<Join>,
    /// The sequence number of each `left` record of the key.
    pub lefts: BTreeSet<u64>,
    /// The last number of its `removed` record.
    pub last: Option<u64>,
}

/// One join of a member key: one `member` record of the owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Join {
    /// The number of the `left` record of the key before this join: 0
    /// for its first join.
    pub after: u64,
    pub user: String,
    pub host: String,
    /// The ID of the `member` record.
    pub id: Id,
}

impl Member {
    /// The member of its first join `join`.
    fn new(join: Join) -> Member {
        let mut member = Member {
            user: String::new(),
            host: String::new(),
            until: None,
            removed: false,
            left: false,
            joins: vec![join],
            lefts: BTreeSet::new(),
            last: None,
        };
        member.settle();
        member
    }

    /// The join of the record `seq` of the key: the last join whose
    /// `after` is lower than `seq`.
    pub fn join_of(&self, seq: u64) -> &Join {
        self.joins
            .iter()
            .rev()
            .find(|join| join.after < seq)
            .unwrap_or(&self.joins[0])
    }

    /// The last join of the key.
    pub fn last_join(&self) -> &Join {
        self.joins.last().unwrap_or(&self.joins[0])
    }

    /// The last sequence number that the key may write in the join of
    /// its record `seq`: the last of its `removed` record, or the number
    /// of its `left` record in that join.
    pub fn until_of(&self, seq: u64) -> Option<u64> {
        let left = self.left_after(self.join_of(seq).after);
        match (left, self.last) {
            (Some(left), Some(last)) => Some(left.min(last)),
            (left, last) => left.or(last),
        }
    }

    /// The number of the `left` record of the key after its last join.
    pub fn left_at(&self) -> Option<u64> {
        self.left_after(self.last_join().after)
    }

    /// The number of the first `left` record of the key after its record
    /// `after`.
    fn left_after(&self, after: u64) -> Option<u64> {
        self.lefts
            .range((Bound::Excluded(after), Bound::Unbounded))
            .next()
            .copied()
    }

    /// Sets the user, the host, `until`, `removed` and `left` from the
    /// joins, the `left` records and the `removed` record.
    fn settle(&mut self) {
        let join = self.last_join().clone();
        self.user = join.user;
        self.host = join.host;
        self.until = self.until_of(u64::MAX);
        self.removed = self.last.is_some();
        self.left = self.left_at().is_some();
    }
}

/// One session of one author: its last `session` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    /// The daemon of the session.
    pub author: Public,
    /// The last record of the session.
    pub body: Session,
    /// The ID of the record that gave the session its lead mark, while
    /// the session has a lead mark.
    pub mark: Option<Id>,
    /// The time of the author of the last record: the time of the status.
    pub time_ms: u64,
    /// The time of the author of the record that gave the session its
    /// claims now: the first record, or the last one that changed them.
    pub claims_ms: u64,
}

impl Run {
    /// The URI of the session, with its lead mark and its claims.
    pub fn uri(&self, lead: bool) -> SessionUri {
        self.body
            .session
            .clone()
            .with_lead(lead)
            .with_claims(self.body.claims.clone())
    }

    /// The repository thread of the session.
    pub fn thread(&self) -> Option<ThreadName> {
        self.body.session.default_thread()
    }
}

/// A `plan` record that holds an item or pauses: a hold or a pause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    /// The ID of the record.
    pub id: Id,
    /// Its author.
    pub author: Public,
    /// The user of its author.
    pub user: String,
    /// The repository: of the hold, or of the pause. `None` for a pause
    /// of the whole riff.
    pub thread: Option<ThreadName>,
    /// The item of a hold.
    pub item: Option<String>,
    /// The reason of a hold.
    pub reason: String,
    /// The time of its author.
    pub time_ms: u64,
}

/// A free or a resume: the author of a record that ends a mark.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct End {
    author: Public,
    user: String,
}

/// A message that the daemon kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    /// The number of this daemon: the order in which it kept the
    /// messages, from 1.
    pub number: u64,
    /// The ID of the record.
    pub id: Id,
    /// The daemon that wrote it.
    pub author: Public,
    /// The time of its author.
    pub time_ms: u64,
    pub message: Message,
}

/// What a kept record changed, for the daemon that keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Applied {
    /// A record that the daemon does not apply: of a type that it does
    /// not know, or a plan of a kind that it does not know.
    Nothing,
    /// The members, a session or the plan changed.
    Changed,
    /// A message, with its thread and its number.
    Message { thread: ThreadName, number: u64 },
}

/// The state of a riff of peers. See the design of the module.
#[derive(Clone, Debug)]
pub struct State {
    riff: Public,
    owner: Option<String>,
    members: BTreeMap<Public, Member>,
    heads: BTreeMap<Public, (u64, Id)>,
    /// The time of its author of the last record of each author.
    times: BTreeMap<Public, u64>,
    /// The host of the owner, from its `riff` record.
    owner_host: Option<String>,
    /// The ID of each record that the state kept.
    ids: BTreeSet<Id>,
    runs: BTreeMap<(Public, String), Run>,
    /// Each lead mark that another lead mark names.
    named: BTreeSet<Id>,
    holds: BTreeMap<Id, Mark>,
    pauses: BTreeMap<Id, Mark>,
    /// The frees and the resumes of each hold and each pause, by its ID.
    ends: BTreeMap<Id, BTreeSet<End>>,
    threads: BTreeMap<ThreadName, Vec<Kept>>,
    numbered: u64,
}

impl State {
    /// The state of the riff `riff` with no record.
    pub fn new(riff: Public) -> State {
        State {
            riff,
            owner: None,
            members: BTreeMap::new(),
            heads: BTreeMap::new(),
            times: BTreeMap::new(),
            owner_host: None,
            ids: BTreeSet::new(),
            runs: BTreeMap::new(),
            named: BTreeSet::new(),
            holds: BTreeMap::new(),
            pauses: BTreeMap::new(),
            ends: BTreeMap::new(),
            threads: BTreeMap::new(),
            numbered: 0,
        }
    }

    /// The riff ID: the key of the owner.
    pub fn riff(&self) -> Public {
        self.riff
    }

    /// The user of the owner, after the `riff` record.
    pub fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    /// The host of the owner, when its `riff` record names it
    /// (01M521J6T2PWA9JREDXJK6DZ0V).
    pub fn owner_host(&self) -> Option<&str> {
        self.owner_host.as_deref()
    }

    /// Each member key.
    pub fn members(&self) -> &BTreeMap<Public, Member> {
        &self.members
    }

    /// The last sequence number and the ID of each author.
    pub fn heads(&self) -> &BTreeMap<Public, (u64, Id)> {
        &self.heads
    }

    /// The time of its author of the last record of `key` that the state
    /// kept, in milliseconds since the epoch. `None` for a key with no
    /// record. A daemon that has no link to `key` shows it as last seen
    /// at the later of this time and [`crate::peer::sync::Links::seen`]
    /// (01M519Z554SC3EAT69Q3M6JDAP). riff shows the time, and never
    /// orders records by it (01M519Z28S7FB4T1H52642JXZ1).
    pub fn last_ms(&self, key: &Public) -> Option<u64> {
        self.times.get(key).copied()
    }

    /// True when the state kept the record `id`.
    pub fn has(&self, id: &Id) -> bool {
        self.ids.contains(id)
    }

    /// The user of the key `key`: the owner or a member.
    pub fn user_of(&self, key: &Public) -> Option<&str> {
        if *key == self.riff {
            return self.owner();
        }
        self.members.get(key).map(|member| member.user.as_str())
    }

    /// The host of the key `key`: of the owner from its `riff` record
    /// (01M521J6T2PWA9JREDXJK6DZ0V), of a member from the `member`
    /// record of its last join. `None` for an owner whose `riff` record
    /// names no host, and for a key that is not in the riff.
    ///
    /// ```
    /// use riff::daemon_key::Key;
    /// use riff::peer::body::{Body, Member, Riff};
    /// use riff::peer::keep::Peers;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let owner = Key::from_bytes(&[1; 32]);
    /// let brett = Key::from_bytes(&[2; 32]).public();
    /// let mut peers = Peers::open(dir.path(), owner.public(), |_| {})?;
    /// let riff = Riff { user: "mike".into(), host: Some("pangolin".into()) };
    /// peers.write(&owner, &Body::Riff(riff), 1)?;
    /// let host = "kadomony".into();
    /// let member = Member { key: brett, user: "brett".into(), host, after: None };
    /// peers.write(&owner, &Body::Member(member), 2)?;
    ///
    /// let state = peers.state();
    /// assert_eq!(state.host_of(&owner.public()), Some("pangolin"));
    /// assert_eq!(state.host_of(&brett), Some("kadomony"));
    /// assert_eq!(state.host_of(&Key::from_bytes(&[3; 32]).public()), None);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn host_of(&self, key: &Public) -> Option<&str> {
        if *key == self.riff {
            return self.owner_host();
        }
        self.members.get(key).map(|member| member.host.as_str())
    }

    /// The user of the record `seq` of the key `key`: the owner, or a
    /// member in the join of that record (01M5328FVZJGCY9VPQFERM7P81).
    pub fn user_at(&self, key: &Public, seq: u64) -> Option<&str> {
        if *key == self.riff {
            return self.owner();
        }
        self.members
            .get(key)
            .map(|member| member.join_of(seq).user.as_str())
    }

    /// Applies the checked record `checked`. The caller checked it with
    /// this state as the [`Context`].
    pub fn keep(&mut self, checked: &Checked) -> Applied {
        let record = &checked.record;
        let (author, seq, id) = (record.author(), record.seq(), record.id());
        self.heads.insert(author, (seq, id));
        self.times.insert(author, record.time_ms());
        self.ids.insert(id);
        let user = self.user_at(&author, seq).unwrap_or_default().to_owned();
        match &checked.body {
            Body::Riff(riff) => {
                self.owner = Some(riff.user.clone());
                self.owner_host.clone_from(&riff.host);
            }
            Body::Member(member) => {
                let join = Join {
                    after: member.after.unwrap_or_default(),
                    user: member.user.clone(),
                    host: member.host.clone(),
                    id,
                };
                match self.members.get_mut(&member.key) {
                    Some(known) => {
                        known.joins.push(join);
                        known.settle();
                    }
                    None => {
                        self.members.insert(member.key, Member::new(join));
                    }
                }
            }
            Body::Removed(removed) => {
                if let Some(member) = self.members.get_mut(&removed.key) {
                    member.last = Some(member.last.map_or(removed.last, |l| l.min(removed.last)));
                    member.settle();
                }
            }
            Body::Left(_) => {
                if let Some(member) = self.members.get_mut(&author) {
                    member.lefts.insert(seq);
                    member.settle();
                }
            }
            Body::Session(session) => self.session(author, id, record.time_ms(), session),
            Body::Message(message) => {
                self.numbered += 1;
                let thread = message.thread.clone();
                self.threads.entry(thread.clone()).or_default().push(Kept {
                    number: self.numbered,
                    id,
                    author,
                    time_ms: record.time_ms(),
                    message: (**message).clone(),
                });
                return Applied::Message {
                    thread,
                    number: self.numbered,
                };
            }
            Body::Plan(plan) => {
                let mark = |thread: Option<ThreadName>, item: Option<String>, reason: &str| Mark {
                    id,
                    author,
                    user: user.clone(),
                    thread,
                    item,
                    reason: reason.to_owned(),
                    time_ms: record.time_ms(),
                };
                match plan {
                    Plan::Hold {
                        thread,
                        item,
                        reason,
                    } => {
                        let hold = mark(Some(thread.clone()), Some(item.clone()), reason);
                        self.holds.insert(id, hold);
                    }
                    Plan::Pause { thread } => {
                        self.pauses.insert(id, mark(thread.clone(), None, ""));
                    }
                    Plan::Free { ends } | Plan::Resume { ends } => {
                        for end in ends {
                            self.ends.entry(*end).or_default().insert(End {
                                author,
                                user: user.clone(),
                            });
                        }
                    }
                    Plan::Other => return Applied::Nothing,
                }
            }
            Body::Other => return Applied::Nothing,
        }
        Applied::Changed
    }

    /// Applies the `session` record `id` of `author`, from the time
    /// `time_ms` of its author.
    fn session(&mut self, author: Public, id: Id, time_ms: u64, session: &Session) {
        let Some(name) = session.session.who().session() else {
            return;
        };
        if let Some(replaces) = session.lead.as_ref().and_then(|lead| lead.replaces) {
            self.named.insert(replaces);
        }
        let key = (author, name.to_owned());
        let old = self.runs.get(&key);
        let mark = session
            .lead
            .as_ref()
            .map(|_| old.and_then(|run| run.mark).unwrap_or(id));
        let claims_ms = old
            .filter(|run| run.body.claims == session.claims)
            .map_or(time_ms, |run| run.claims_ms);
        self.runs.insert(
            key,
            Run {
                author,
                body: session.clone(),
                mark,
                time_ms,
                claims_ms,
            },
        );
    }

    /// Each session that runs: its last record does not say that it
    /// ended.
    pub fn runs(&self) -> impl Iterator<Item = &Run> {
        self.runs.values().filter(|run| !run.body.ended)
    }

    /// Each session, also each one that ended.
    pub fn runs_all(&self) -> impl Iterator<Item = &Run> {
        self.runs.values()
    }

    /// The session `session` of `author`, also one that ended.
    pub fn run(&self, author: &Public, session: &str) -> Option<&Run> {
        self.runs.get(&(*author, session.to_owned()))
    }

    /// The lead of `user` in the repository `thread`, when it has one.
    pub fn lead(&self, user: &str, thread: &ThreadName) -> Option<&Run> {
        self.runs()
            .filter(|run| run.body.session.who().user() == user)
            .filter(|run| run.thread().as_ref() == Some(thread))
            .filter(|run| run.mark.is_some_and(|mark| !self.named.contains(&mark)))
            .max_by(|a, b| {
                (a.author, a.body.session.who().session())
                    .cmp(&(b.author, b.body.session.who().session()))
            })
    }

    /// True when `run` is the lead of its user in its repository.
    pub fn is_lead(&self, run: &Run) -> bool {
        let user = run.body.session.who().user();
        run.thread()
            .and_then(|thread| self.lead(user, &thread))
            .is_some_and(|lead| lead.author == run.author && lead.body.session == run.body.session)
    }

    /// True when `end` ends the mark `mark`: its author is a daemon of the
    /// user of the mark, or the daemon of the owner. Only the owner ends
    /// a pause of the whole riff.
    fn ends(&self, end: &End, mark: &Mark, whole_riff: bool) -> bool {
        end.author == self.riff || (!whole_riff && end.user == mark.user)
    }

    /// True when a free or a resume ends the mark `mark`.
    fn ended(&self, mark: &Mark, whole_riff: bool) -> bool {
        self.ends
            .get(&mark.id)
            .is_some_and(|ends| ends.iter().any(|end| self.ends(end, mark, whole_riff)))
    }

    /// Each hold that no free ends.
    pub fn holds(&self) -> impl Iterator<Item = &Mark> {
        self.holds.values().filter(|hold| !self.ended(hold, false))
    }

    /// Each hold of the item `item` of the repository `thread` that no free
    /// ends. The item is held while it has one.
    pub fn holds_of<'a>(
        &'a self,
        thread: &'a ThreadName,
        item: &'a str,
    ) -> impl Iterator<Item = &'a Mark> {
        self.holds().filter(move |hold| {
            hold.thread.as_ref() == Some(thread) && hold.item.as_deref() == Some(item)
        })
    }

    /// Each pause that no resume ends.
    pub fn pauses(&self) -> impl Iterator<Item = &Mark> {
        self.pauses
            .values()
            .filter(|pause| !self.ended(pause, pause.thread.is_none()))
    }

    /// Each pause that stops the work in `thread`: a pause of the whole
    /// riff, or of the repository.
    pub fn pauses_of<'a>(&'a self, thread: &'a ThreadName) -> impl Iterator<Item = &'a Mark> {
        self.pauses()
            .filter(move |pause| pause.thread.as_ref().is_none_or(|t| t == thread))
    }

    /// The messages of `thread`, in the order of their numbers.
    pub fn messages(&self, thread: &ThreadName) -> &[Kept] {
        self.threads.get(thread).map_or(&[], Vec::as_slice)
    }

    /// The name of each thread with a message.
    pub fn threads(&self) -> impl Iterator<Item = &ThreadName> {
        self.threads.keys()
    }

    /// The state with no numbers of messages: the part that does not
    /// depend on the order of the records (01M519Z3AXFKBHJSAW46KWFTXS).
    pub fn view(&self) -> View {
        let leads = self
            .runs()
            .filter(|run| self.is_lead(run))
            .map(|run| (run.author, run.body.session.to_string()))
            .collect();
        View {
            owner: self.owner.clone(),
            members: self.members.clone(),
            heads: self.heads.clone(),
            runs: self.runs.clone(),
            leads,
            holds: self.holds().map(|hold| hold.id).collect(),
            pauses: self.pauses().map(|pause| pause.id).collect(),
            messages: self
                .threads
                .iter()
                .map(|(thread, kept)| (thread.clone(), kept.iter().map(|k| k.id).collect()))
                .collect(),
        }
    }
}

/// True when a record that failed with `fault` can pass later, so the
/// daemon drops it with no fault (01M519Z33N2MW43XY62NYC6RRH): a
/// sequence number after the next one, or an author that no record of
/// the owner admits yet. The next heads give it again, in order. Each
/// other fault refuses the record (01M519Z363W5GV4MV5RDB6ECG0).
///
/// ```
/// use riff::peer::record::{Author, Fault};
/// use riff::peer::state::later;
///
/// assert!(later(&Fault::Sequence { got: 5, next: 3 }));
/// assert!(later(&Fault::Author(Author::Unknown)));
/// assert!(!later(&Fault::Sequence { got: 2, next: 3 }));
/// assert!(!later(&Fault::Author(Author::Gone)));
/// assert!(!later(&Fault::Signature));
/// ```
pub fn later(fault: &Fault) -> bool {
    match fault {
        Fault::Sequence { got, next } => got > next,
        Fault::Author(Author::Unknown) => true,
        Fault::Author(Author::Gone)
        | Fault::Size { .. }
        | Fault::Version(_)
        | Fault::Type
        | Fault::Riff
        | Fault::Before
        | Fault::Signature
        | Fault::Json(_)
        | Fault::Writer(_) => false,
    }
}

/// The part of a [`State`] that does not depend on the order of the
/// records: each part but the numbers of the messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub owner: Option<String>,
    pub members: BTreeMap<Public, Member>,
    pub heads: BTreeMap<Public, (u64, Id)>,
    pub runs: BTreeMap<(Public, String), Run>,
    /// The daemon and the URI of each lead.
    pub leads: BTreeSet<(Public, String)>,
    /// The ID of each hold that no free ends.
    pub holds: BTreeSet<Id>,
    /// The ID of each pause that no resume ends.
    pub pauses: BTreeSet<Id>,
    /// The ID of each message of each thread.
    pub messages: BTreeMap<ThreadName, BTreeSet<Id>>,
}

impl Context for State {
    fn riff(&self) -> Public {
        self.riff
    }

    fn standing(&self, key: &Public, seq: u64) -> Standing {
        if *key == self.riff {
            return Standing::Owner {
                user: self.owner.clone(),
            };
        }
        match self.members.get(key) {
            Some(member) => Standing::Member {
                user: member.join_of(seq).user.clone(),
                until: member.until_of(seq),
            },
            None => Standing::Unknown,
        }
    }

    fn admitted(&self, key: &Public) -> Option<Admitted> {
        self.members.get(key).map(|member| Admitted {
            after: member.last_join().after,
            removed: member.removed,
        })
    }

    /// The first record of a key in a join again names the `member`
    /// record of that join (01M5328FPJT7Q5Q1509BC5F0J7).
    fn next(&self, author: &Public) -> (u64, Id) {
        let Some((seq, id)) = self.heads.get(author) else {
            return (1, Id::ZERO);
        };
        let join = self.members.get(author).and_then(|member| {
            member
                .joins
                .iter()
                .find(|join| join.after > 0 && join.after == *seq)
        });
        (seq + 1, join.map_or(*id, |join| join.id))
    }

    /// The daemon `author` runs a session with a lead mark: in the
    /// repository `thread`, or with `None` in any repository. It reads
    /// only the records of `author`, so the writer check of a `plan`
    /// record does not depend on the order of the records of two
    /// authors.
    fn leads(&self, author: &Public, thread: Option<&ThreadName>) -> bool {
        self.runs().any(|run| {
            run.author == *author
                && run.mark.is_some()
                && thread.is_none_or(|t| run.thread().as_ref() == Some(t))
        })
    }
}

#[cfg(test)]
mod tests {
    use riff_core::wire;

    use super::*;
    use crate::daemon_key::Key;
    use crate::peer::body::{Lead, Member as MemberBody, Riff};
    use crate::peer::record::{Record, check};

    /// The daemons of a test: the owner `mike` on thelio [1], `mike` on
    /// pangolin [2], and `brett` on kadomony [3].
    struct Daemons {
        keys: Vec<Key>,
        heads: BTreeMap<Public, (u64, Id)>,
        time: u64,
    }

    impl Daemons {
        fn new() -> Daemons {
            Daemons {
                keys: vec![
                    Key::from_bytes(&[1; 32]),
                    Key::from_bytes(&[2; 32]),
                    Key::from_bytes(&[3; 32]),
                ],
                heads: BTreeMap::new(),
                time: 0,
            }
        }

        fn public(&self, daemon: usize) -> Public {
            self.keys[daemon].public()
        }

        /// The next record of `daemon` with `body`.
        fn write(&mut self, daemon: usize, body: Body) -> Record {
            let key = &self.keys[daemon];
            let (seq, before) = self
                .heads
                .get(&key.public())
                .map_or((1, Id::ZERO), |(seq, id)| (seq + 1, *id));
            self.time += 1;
            let record =
                Record::sign(key, self.keys[0].public(), seq, before, self.time, &body).unwrap();
            self.heads.insert(key.public(), (seq, record.id()));
            record
        }

        /// The first records of the owner: the riff, and the two members.
        fn start(&mut self) -> Vec<Record> {
            let member = |d: &Daemons, daemon: usize, user: &str, host: &str| {
                Body::Member(MemberBody {
                    key: d.public(daemon),
                    user: user.into(),
                    host: host.into(),
                    after: None,
                })
            };
            let b1 = member(self, 1, "mike", "pangolin");
            let b2 = member(self, 2, "brett", "kadomony");
            vec![
                self.write(
                    0,
                    Body::Riff(Riff {
                        user: "mike".into(),
                        host: None,
                    }),
                ),
                self.write(0, b1),
                self.write(0, b2),
            ]
        }
    }

    fn session(uri: &str, lead: Option<Option<Id>>) -> Body {
        Body::Session(Box::new(Session {
            session: uri.parse().unwrap(),
            worker: false,
            lead: lead.map(|replaces| Lead { replaces }),
            claims: vec![],
            status: None,
            ended: false,
        }))
    }

    fn message(uri: &str, body: &str) -> Body {
        Body::Message(Box::new(Message {
            from: uri.parse().unwrap(),
            thread: "o/r".parse().unwrap(),
            to: vec![],
            kind: wire::Kind::Message,
            body: body.into(),
        }))
    }

    fn hold(item: &str) -> Body {
        Body::Plan(Plan::Hold {
            thread: "o/r".parse().unwrap(),
            item: item.into(),
            reason: "wait".into(),
        })
    }

    fn free(ends: &[&Record]) -> Body {
        Body::Plan(Plan::Free {
            ends: ends.iter().map(|r| r.id()).collect(),
        })
    }

    /// A state of the riff `riff` that keeps `records` in this order.
    fn keep_all(riff: Public, records: &[Record]) -> State {
        let mut state = State::new(riff);
        for record in records {
            let checked = check(record.bytes().to_vec(), &state)
                .unwrap_or_else(|fault| panic!("{record:?}: {fault}"));
            state.keep(&checked);
        }
        state
    }

    const THELIO: &str = "riff://mike@thelio/o/r?session=t1";
    const PANGOLIN: &str = "riff://mike@pangolin/o/r?session=p1";
    const KADOMONY: &str = "riff://brett@kadomony/o/r?session=k1";

    /// 01M519Z3AXFKBHJSAW46KWFTXS: two states that keep the same records of
    /// three authors in two orders are the same, but for the numbers of
    /// the messages.
    #[test]
    fn two_orders_of_the_same_records_give_the_same_state() {
        let mut d = Daemons::new();
        let riff = d.public(0);
        let start = d.start();
        // Each chain: the records of one author, in its order.
        let thelio = [
            d.write(0, session(THELIO, Some(None))),
            d.write(0, message(THELIO, "from thelio")),
            d.write(0, hold("issue-12")),
        ];
        let pangolin = [
            d.write(1, session(PANGOLIN, None)),
            d.write(1, message(PANGOLIN, "from pangolin")),
        ];
        let kadomony = [
            d.write(2, session(KADOMONY, Some(None))),
            d.write(2, hold("issue-12")),
            d.write(2, message(KADOMONY, "from kadomony")),
        ];
        let kadomony_free = d.write(2, free(&[&kadomony[1]]));
        let kadomony_free_of_thelio = d.write(2, free(&[&thelio[2]]));

        let mut one = start.clone();
        one.extend(thelio.iter().cloned());
        one.extend(pangolin.iter().cloned());
        one.extend(kadomony.iter().cloned());
        one.push(kadomony_free.clone());
        one.push(kadomony_free_of_thelio.clone());

        // Another order: the chains interleave, with the frees of
        // kadomony before the hold of thelio.
        let mut two = start;
        let mut chains = [kadomony.iter(), pangolin.iter(), thelio.iter()];
        loop {
            let mut any = false;
            for chain in &mut chains {
                if let Some(record) = chain.next() {
                    two.push(record.clone());
                    any = true;
                }
            }
            if !any {
                break;
            }
        }
        let at = two.iter().position(|r| *r == thelio[2]).unwrap();
        two.insert(at, kadomony_free_of_thelio);
        two.insert(at, kadomony_free);
        assert_ne!(one, two);

        let (a, b) = (keep_all(riff, &one), keep_all(riff, &two));
        assert_eq!(a.view(), b.view());
        let thread: ThreadName = "o/r".parse().unwrap();
        let numbers =
            |s: &State| -> Vec<u64> { s.messages(&thread).iter().map(|k| k.number).collect() };
        assert_eq!(numbers(&a), [1, 2, 3]);
        assert_eq!(numbers(&b), [1, 2, 3]);
        let first = |s: &State| s.messages(&thread)[0].message.body.clone();
        assert_eq!(first(&a), "from thelio");
        assert_eq!(first(&b), "from pangolin");
        // brett frees only the hold of brett: the hold of mike stays.
        let view = a.view();
        assert_eq!(view.holds, BTreeSet::from([thelio[2].id()]));
        assert_eq!(view.leads.len(), 2);
    }

    /// 01M521J6MKSBAA2QP6ZFHYT9ZQ: a second `member` record of a key, with
    /// another user, is refused in each order with a session record of
    /// that key. So the key keeps its first user, and its session record
    /// passes in both orders.
    #[test]
    fn a_second_member_record_of_a_key_is_refused_in_each_order() {
        let mut d = Daemons::new();
        let riff = d.public(0);
        let start = d.start();
        let second = d.write(
            0,
            Body::Member(MemberBody {
                key: d.public(2),
                user: "ann".into(),
                host: "kadomony".into(),
                after: None,
            }),
        );
        let kadomony = d.write(2, session(KADOMONY, None));
        for order in [
            [second.clone(), kadomony.clone()],
            [kadomony.clone(), second.clone()],
        ] {
            let mut state = keep_all(riff, &start);
            for record in &order {
                match check(record.bytes().to_vec(), &state) {
                    Ok(checked) => {
                        assert_eq!(checked.record, kadomony);
                        state.keep(&checked);
                    }
                    Err(fault) => {
                        assert_eq!(record, &second);
                        assert_eq!(
                            fault,
                            Fault::Writer("the key is a member already, or was one")
                        );
                    }
                }
            }
            assert_eq!(state.user_of(&d.public(2)), Some("brett"));
            assert_eq!(state.runs().count(), 1);
        }
    }

    /// 01M519Z3J7Z0M3PNDY249Q57FH: an item that two leads hold stays held
    /// until both free it. The owner frees each hold.
    #[test]
    fn two_holds_of_an_item_need_two_frees_and_the_owner_frees_each() {
        let mut d = Daemons::new();
        let riff = d.public(0);
        let mut records = d.start();
        records.push(d.write(1, session(PANGOLIN, Some(None))));
        records.push(d.write(2, session(KADOMONY, Some(None))));
        let pangolin_hold = d.write(1, hold("issue-12"));
        let kadomony_hold = d.write(2, hold("issue-12"));
        records.push(pangolin_hold.clone());
        records.push(kadomony_hold.clone());
        let thread: ThreadName = "o/r".parse().unwrap();
        let held = |records: &[Record]| {
            keep_all(riff, records)
                .holds_of(&thread, "issue-12")
                .count()
        };
        assert_eq!(held(&records), 2);

        // brett frees each hold: only the hold of brett ends.
        records.push(d.write(2, free(&[&pangolin_hold, &kadomony_hold])));
        assert_eq!(held(&records), 1);
        let state = keep_all(riff, &records);
        let left: Vec<_> = state.holds_of(&thread, "issue-12").collect();
        assert_eq!(left[0].id, pangolin_hold.id());

        // The owner frees each hold.
        records.push(d.write(0, free(&[&pangolin_hold])));
        assert_eq!(held(&records), 0);
    }

    /// 01M519Z3J7Z0M3PNDY249Q57FH: a hold, a free of the owner, and a new
    /// hold give the same holds in each order. The free ends only the
    /// hold that it names.
    #[test]
    fn a_hold_a_free_and_a_new_hold_give_the_same_holds_in_each_order() {
        let mut d = Daemons::new();
        let riff = d.public(0);
        let start = d.start();
        let lead = d.write(2, session(KADOMONY, Some(None)));
        let first = d.write(2, hold("issue-12"));
        let free = d.write(0, free(&[&first]));
        let again = d.write(2, hold("issue-12"));
        let thread: ThreadName = "o/r".parse().unwrap();
        let orders = [
            vec![lead.clone(), first.clone(), free.clone(), again.clone()],
            vec![free.clone(), lead.clone(), first.clone(), again.clone()],
            vec![lead.clone(), first.clone(), again.clone(), free.clone()],
        ];
        for order in orders {
            let mut records = start.clone();
            records.extend(order);
            let state = keep_all(riff, &records);
            let holds: Vec<Id> = state.holds_of(&thread, "issue-12").map(|h| h.id).collect();
            assert_eq!(holds, [again.id()]);
        }
    }

    /// 01M51BFYT2ZGN4W5E5R87GPVCM: of two lead marks, the one that names
    /// the other counts, in each order. Else the higher key counts.
    #[test]
    fn the_lead_mark_that_names_the_other_counts_else_the_higher_key() {
        let mut d = Daemons::new();
        let riff = d.public(0);
        let start = d.start();
        let thread: ThreadName = "o/r".parse().unwrap();
        // mike on thelio [1] and on pangolin [2]: the session of the
        // daemon with the higher key counts.
        let higher = if d.public(0) > d.public(1) {
            THELIO
        } else {
            PANGOLIN
        };
        let thelio = d.write(0, session(THELIO, Some(None)));
        let pangolin = d.write(1, session(PANGOLIN, Some(None)));
        let lead_of = |records: &[Record]| {
            keep_all(riff, records)
                .lead("mike", &thread)
                .map(|run| run.body.session.to_string())
        };
        for order in [
            vec![thelio.clone(), pangolin.clone()],
            vec![pangolin.clone(), thelio.clone()],
        ] {
            let mut records = start.clone();
            records.extend(order);
            assert_eq!(lead_of(&records).as_deref(), Some(higher));
        }
        // thelio takes the lead again, and names the mark of pangolin.
        let back = d.write(
            0,
            session(
                "riff://mike@thelio/o/r?session=t2",
                Some(Some(pangolin.id())),
            ),
        );
        for order in [
            vec![thelio.clone(), pangolin.clone(), back.clone()],
            vec![thelio.clone(), back.clone(), pangolin.clone()],
        ] {
            let mut records = start.clone();
            records.extend(order);
            assert_eq!(
                lead_of(&records).as_deref(),
                Some("riff://mike@thelio/o/r?session=t2")
            );
        }
        // The session of brett is the lead of brett: another user.
        let mut records = start.clone();
        records.push(d.write(2, session(KADOMONY, Some(None))));
        let state = keep_all(riff, &records);
        assert!(state.lead("brett", &thread).is_some());
        assert!(state.lead("mike", &thread).is_none());
    }

    /// A pause of a repository ends by a resume of a daemon of its user, or
    /// of the owner. A pause of the whole riff ends only by a resume of the
    /// owner.
    #[test]
    fn a_resume_ends_a_pause_of_its_user_and_the_owner_ends_each() {
        let mut d = Daemons::new();
        let riff = d.public(0);
        let mut records = d.start();
        let thread: ThreadName = "o/r".parse().unwrap();
        records.push(d.write(2, session(KADOMONY, Some(None))));
        records.push(d.write(1, session(PANGOLIN, Some(None))));
        let brett = d.write(
            2,
            Body::Plan(Plan::Pause {
                thread: Some(thread.clone()),
            }),
        );
        let whole = d.write(0, Body::Plan(Plan::Pause { thread: None }));
        records.push(brett.clone());
        records.push(whole.clone());
        let resume = |ends: &[&Record]| {
            Body::Plan(Plan::Resume {
                ends: ends.iter().map(|r| r.id()).collect(),
            })
        };
        // mike on pangolin may not end the pause of brett, nor the pause of
        // the whole riff.
        records.push(d.write(1, resume(&[&brett, &whole])));
        assert_eq!(keep_all(riff, &records).pauses_of(&thread).count(), 2);
        // brett ends the pause of brett.
        records.push(d.write(2, resume(&[&brett])));
        let state = keep_all(riff, &records);
        let left: Vec<_> = state.pauses_of(&thread).map(|p| p.id).collect();
        assert_eq!(left, [whole.id()]);
        // The owner ends the pause of the whole riff.
        records.push(d.write(0, resume(&[&whole])));
        assert_eq!(keep_all(riff, &records).pauses_of(&thread).count(), 0);
    }

    #[test]
    fn a_session_runs_until_its_end_and_keeps_its_first_mark() {
        let mut d = Daemons::new();
        let riff = d.public(0);
        let mut records = d.start();
        let first = d.write(1, session(PANGOLIN, Some(None)));
        records.push(first.clone());
        let mut status = session(PANGOLIN, Some(None));
        if let Body::Session(s) = &mut status {
            s.status = Some("tests".into());
            s.claims = vec!["issue-12".into()];
        }
        records.push(d.write(1, status));
        let state = keep_all(riff, &records);
        let run = state.run(&d.public(1), "p1").unwrap();
        assert_eq!(run.mark, Some(first.id()));
        assert_eq!(run.body.status.as_deref(), Some("tests"));
        assert!(state.is_lead(run));
        assert_eq!(
            run.uri(true).to_string(),
            "riff://mike@pangolin/o/r?session=p1&lead=true&claim=issue-12"
        );
        let mut end = session(PANGOLIN, None);
        if let Body::Session(s) = &mut end {
            s.ended = true;
        }
        records.push(d.write(1, end));
        let state = keep_all(riff, &records);
        assert_eq!(state.runs().count(), 0);
        assert!(state.lead("mike", &"o/r".parse().unwrap()).is_none());
    }

    /// 01M519Z554SC3EAT69Q3M6JDAP: the state keeps the time of the author
    /// of the last record of each key, and of the last record of each
    /// session.
    #[test]
    fn the_state_keeps_the_time_of_the_last_record_of_each_key() {
        let mut d = Daemons::new();
        let riff = d.public(0);
        let mut records = d.start();
        records.push(d.write(1, session(PANGOLIN, None)));
        records.push(d.write(1, message(PANGOLIN, "later")));
        let state = keep_all(riff, &records);
        // The times of the test are 1, 2, 3 for the owner, then 4 and 5.
        assert_eq!(state.last_ms(&d.public(0)), Some(3));
        assert_eq!(state.last_ms(&d.public(1)), Some(5));
        assert_eq!(state.last_ms(&d.public(2)), None);
        let run = state.run(&d.public(1), "p1").unwrap();
        assert_eq!((run.time_ms, run.claims_ms), (4, 4));

        // A new status at 6 keeps the time of the claims. A claim at 7
        // moves it.
        let mut status = session(PANGOLIN, None);
        if let Body::Session(s) = &mut status {
            s.status = Some("tests".into());
        }
        records.push(d.write(1, status.clone()));
        let state = keep_all(riff, &records);
        let run = state.run(&d.public(1), "p1").unwrap();
        assert_eq!((run.time_ms, run.claims_ms), (6, 4));
        if let Body::Session(s) = &mut status {
            s.claims = vec!["issue-12".into()];
        }
        records.push(d.write(1, status));
        let state = keep_all(riff, &records);
        let run = state.run(&d.public(1), "p1").unwrap();
        assert_eq!((run.time_ms, run.claims_ms), (7, 7));
        assert_eq!(state.last_ms(&d.public(1)), Some(7));
    }

    #[test]
    fn the_members_come_from_the_records_of_the_owner() {
        let mut d = Daemons::new();
        let riff = d.public(0);
        let mut records = d.start();
        let state = keep_all(riff, &records);
        assert_eq!(state.owner(), Some("mike"));
        assert_eq!(state.user_of(&d.public(2)), Some("brett"));
        records.push(d.write(
            0,
            Body::Removed(crate::peer::body::Removed {
                key: d.public(2),
                last: 0,
            }),
        ));
        records.push(d.write(1, Body::Left(crate::peer::body::Left {})));
        let state = keep_all(riff, &records);
        let brett = &state.members()[&d.public(2)];
        assert!(brett.removed && brett.until == Some(0));
        let pangolin = &state.members()[&d.public(1)];
        assert!(pangolin.left && pangolin.until == Some(1));
        assert_eq!(
            state.standing(&d.public(1), 2),
            Standing::Member {
                user: "mike".into(),
                until: Some(1)
            }
        );
    }

    const LYNX: &str = "riff://ann@lynx/o/r?session=l1";

    /// The records of a join again of kadomony [3]: its session as brett,
    /// its `left` record 2, the new `member` record of the owner for ann
    /// on lynx, and its first record as ann. That record names the new
    /// `member` record as the record before it.
    struct Again {
        start: Vec<Record>,
        before: Record,
        left: Record,
        again: Record,
        after: Record,
    }

    fn again(d: &mut Daemons) -> Again {
        let start = d.start();
        let before = d.write(2, session(KADOMONY, None));
        let left = d.write(2, Body::Left(crate::peer::body::Left {}));
        let again = d.write(
            0,
            Body::Member(MemberBody {
                key: d.public(2),
                user: "ann".into(),
                host: "lynx".into(),
                after: Some(2),
            }),
        );
        // The first record of the join again names the member record.
        d.heads.insert(d.public(2), (2, again.id()));
        let after = d.write(2, session(LYNX, None));
        Again {
            start,
            before,
            left,
            again,
            after,
        }
    }

    /// 01M5328FVZJGCY9VPQFERM7P81: the first `member` record of a key, its
    /// `left` record and a new `member` record give the same joins, the
    /// same user and the same state of the key in each order. Each record
    /// of the key counts with the user of its join.
    #[test]
    fn a_join_again_gives_the_same_user_and_state_in_each_order() {
        let mut d = Daemons::new();
        let riff = d.public(0);
        let a = again(&mut d);
        let orders = [
            [&a.before, &a.left, &a.again, &a.after],
            [&a.before, &a.again, &a.left, &a.after],
            [&a.again, &a.before, &a.left, &a.after],
        ];
        let mut views = Vec::new();
        for order in orders {
            let mut records = a.start.clone();
            records.extend(order.into_iter().cloned());
            let state = keep_all(riff, &records);
            let member = &state.members()[&d.public(2)];
            assert_eq!(
                (member.user.as_str(), member.host.as_str()),
                ("ann", "lynx")
            );
            assert!(!member.left && !member.removed && member.until.is_none());
            assert_eq!(member.joins.len(), 2);
            assert_eq!(member.lefts, BTreeSet::from([2]));
            assert_eq!(state.user_at(&d.public(2), 1), Some("brett"));
            assert_eq!(state.user_at(&d.public(2), 3), Some("ann"));
            let users: BTreeSet<&str> = state
                .runs()
                .map(|run| run.body.session.who().user())
                .collect();
            assert_eq!(users, BTreeSet::from(["ann", "brett"]));
            views.push(state.view());
        }
        assert_eq!(views[0], views[1]);
        assert_eq!(views[0], views[2]);

        // A record of the key in the join again with the user of its old
        // join is refused.
        let mut records = a.start.clone();
        records.extend([a.before, a.left, a.again]);
        let state = keep_all(riff, &records);
        let old_user = Record::sign(
            &d.keys[2],
            riff,
            3,
            records.last().unwrap().id(),
            9,
            &session(KADOMONY, None),
        )
        .unwrap();
        assert_eq!(
            check(old_user.bytes().to_vec(), &state).unwrap_err(),
            Fault::Writer("a session record of another user than the user of its key")
        );
    }

    /// 01M5328FPJT7Q5Q1509BC5F0J7: a record that the key signed after its
    /// `left` record and before the new `member` record is refused before
    /// that record, and after it. The first record of the join again
    /// passes.
    #[test]
    fn a_record_between_the_leave_and_the_join_again_stays_refused() {
        let mut d = Daemons::new();
        let riff = d.public(0);
        let a = again(&mut d);
        let between = Record::sign(
            &d.keys[2],
            riff,
            3,
            a.left.id(),
            8,
            &session(KADOMONY, None),
        )
        .unwrap();
        let mut records = a.start.clone();
        records.extend([a.before.clone(), a.left.clone()]);
        let state = keep_all(riff, &records);
        assert_eq!(
            check(between.bytes().to_vec(), &state).unwrap_err(),
            Fault::Author(Author::Gone)
        );
        records.push(a.again.clone());
        let state = keep_all(riff, &records);
        assert_eq!(
            check(between.bytes().to_vec(), &state).unwrap_err(),
            Fault::Before
        );
        assert_eq!(state.next(&d.public(2)), (3, a.again.id()));
        check(a.after.bytes().to_vec(), &state).unwrap();
    }
}
