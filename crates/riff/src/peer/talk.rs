//! The riff tools of a session of the daemon, on the riff of peers of
//! the daemon (01M519Z3FTAZ20DQKDTW09M6NS, 01M519Z3DB3MVP0FK3FBC9KSGD).
//!
//! # Design
//!
//! [`Hub`] is the riff of peers of one daemon: its key, its records
//! ([`Peers`]), the wakes that wait for each of its sessions, and the
//! read cursor of each of its sessions in each thread. [`Session`] is
//! the [`Talk`] of one session of the daemon: each tool that talks
//! acts on the hub, and not on riff-server.
//!
//! | Tool | Record | What else |
//! |---|---|---|
//! | `whoami`, `who`, `threads` | none | a read of the state |
//! | `read` | none | the read cursor of the session moves to the end of the page, but a page that skips an unread message keeps it (01M51TKZSYR7PH629S29JX00ZD) |
//! | `post`, `tell` | `message` | each session of this daemon that a selector matches wakes |
//! | `status` | `session` | the status of the session |
//! | `blocked` | `session` and `message` | the status `blocked: REASON`, and the message to the lead of the user |
//! | `lead` | `session` | a lead mark that names the mark of the old lead |
//! | `hold`, `free` | `plan` | a free ends each hold of the item that the daemon may end |
//! | `pause`, `resume` | `plan` and `message` | a resume ends each pause that the daemon may end; the news wakes the sessions of the repository |
//!
//! The daemon writes a `session` record at the start and at the end of
//! each of its sessions ([`Hub::start`], [`Hub::end`]). A worker claims
//! its item. The lead takes the lead at its start.
//!
//! In the riff of peers, the user of a session of the daemon is the user
//! of the key of the daemon (01M519Z317AT7KT7Q643NVYZD1,
//! 01M51T470TNWVFF97E474FV7BY), also when riff-server gave the session
//! another user. [`Hub::peer`] gives the URI of a session with that user.
//!
//! When the daemon keeps a message, also of another daemon, it wakes
//! each session of its own that the `to` selectors of the message match,
//! but the sender. A note wakes no session. `riff relay watch` in a
//! session waits for its next wake ([`Hub::wait`]). The hub keeps each
//! wake of a session until the session reads its message. So a watch
//! that starts after a wake wakes the session one time, for the newest
//! message that woke it and that it did not read, as riff-server does
//! (R49). Only `read` moves the read cursor: a post of the session does
//! not.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as session A
//!     participant D as the daemon: Hub
//!     participant B as session B
//!     A->>D: post 5, to B
//!     A->>D: post 6, to B
//!     D->>D: keep the wakes of 5 and 6 for B
//!     B->>D: riff relay watch
//!     D-->>B: the wake line of 6, at once
//!     B->>D: read
//!     D-->>B: 5 and 6, and the cursor of B moves to 6
//!     B->>D: riff relay watch
//!     D->>D: no wake after the cursor: wait
//! ```
//!
//! A write of a record syncs a file. So each call of a tool runs on a
//! thread for blocking work, not on a thread of the runtime.
//!
//! The read cursors live in the memory of the daemon. After a new start
//! of the daemon, each message is unread again.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use anyhow::{Context as _, Result, anyhow, bail};
use riff_core::name::{Place, SessionUri, ThreadName, Who};
use riff_core::record::By;
use riff_core::selector::Selector;
use riff_core::wire::{
    FreeReply, HoldReply, Kind, LeadReply, PauseInfo, Posted, RepositoryPause, RiffOwner,
    RiffReply, RiffState, SessionInfo, Status, StatusInfo, ThreadInfo, Wake, WhoReply,
};
use tokio::sync::Notify;

use crate::api::{Checked as Shown, Inbox, PauseScope, Told};
use crate::daemon_key::{Key, Public};
use crate::peer::body::{Body, Lead, Message, Plan, Session as SessionBody};
use crate::peer::keep::{Fork, Outcome, Peers};
use crate::peer::record::Id;
use crate::peer::state::{Applied, Kept, Mark, Run, State};
use crate::talk::Talk;
use crate::text;
use crate::wave::BoxFuture;

/// The most messages of one thread in one page of `read`, as
/// riff-server gives.
pub const PAGE: usize = 50;

/// The riff of peers of one daemon. See the design of the module. Each
/// call that writes a record syncs a file: call it on a thread for
/// blocking work.
///
/// Two sessions of the daemon: b posts to a, the watch of a wakes, and
/// a reads the message.
///
/// ```
/// use std::sync::Arc;
///
/// use riff::daemon_key::Key;
/// use riff::peer::body::{Body, Riff};
/// use riff::peer::keep::Peers;
/// use riff::peer::talk::{Hub, Session};
/// use riff::talk::Talk;
/// use riff_core::name::{SessionUri, ThreadName};
/// use riff_core::selector::Selector;
/// use riff_core::wire::Kind;
///
/// let dir = tempfile::tempdir()?;
/// let key = Key::from_bytes(&[1; 32]);
/// let mut peers = Peers::open(dir.path(), key.public(), |_| {})?;
/// peers.write(&key, &Body::Riff(Riff { user: "mike".into() }), 1)?;
/// let hub = Arc::new(Hub::new(key, peers));
/// let a: SessionUri = "riff://mike@h/o/r?session=a".parse()?;
/// let b: SessionUri = "riff://mike@h/o/r?session=b".parse()?;
/// hub.start(&a, true, false, vec!["issue-1".into()])?;
/// hub.start(&b, true, false, vec!["issue-2".into()])?;
///
/// let talk = Session::new(hub.clone());
/// let repo: ThreadName = "o/r".parse()?;
/// tokio::runtime::Runtime::new()?.block_on(async {
///     let to_a = [Selector::session("a")];
///     talk.post(&b, Some(&repo), &to_a, "hello a", Kind::Message).await?;
///     assert!(hub.wait("a").await.contains("wrote to you in o/r"));
///     let pages = talk.inbox_page(&a, None, false, None).await?;
///     assert_eq!(pages[0].messages[0].message.body, "hello a");
///     Ok::<(), anyhow::Error>(())
/// })?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug)]
pub struct Hub {
    key: Key,
    peers: Mutex<Peers>,
    /// The wakes of each session of the daemon that it did not read, by
    /// its session ID, in the order that the daemon kept their messages.
    wakes: Mutex<BTreeMap<String, Vec<Wake>>>,
    woke: Notify,
    /// The last number that each session read in each thread.
    reads: Mutex<BTreeMap<(String, ThreadName), u64>>,
}

/// Locks `mutex`, also when a thread that held it panicked.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The time now, in milliseconds since the epoch.
fn now_ms() -> u64 {
    u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or_default()
}

impl Hub {
    /// The hub of the daemon of `key`, with its records `peers`.
    pub fn new(key: Key, peers: Peers) -> Hub {
        Hub {
            key,
            peers: Mutex::new(peers),
            wakes: Mutex::new(BTreeMap::new()),
            woke: Notify::new(),
            reads: Mutex::new(BTreeMap::new()),
        }
    }

    /// The public key of the daemon.
    pub fn public(&self) -> Public {
        self.key.public()
    }

    /// Reads the state with `read`.
    pub fn read<T>(&self, read: impl FnOnce(&State) -> T) -> T {
        read(lock(&self.peers).state())
    }

    /// The URI of the session `me` of the daemon in the riff of peers:
    /// with the user of the key of the daemon, and with no lead mark and
    /// no claims.
    pub fn peer(&self, me: &SessionUri) -> SessionUri {
        let user = self.read(|state| state.user_of(&self.public()).map(str::to_owned));
        let plain = me.clone().with_lead(false).with_claims(Vec::new());
        match user.and_then(|user| Who::new(&user, me.who().session()).ok()) {
            Some(who) => SessionUri::new(who, plain.place().clone()),
            None => plain,
        }
    }

    /// Writes a record of the daemon with `body` ([`Peers::write`]), and
    /// wakes the sessions that a new message selects.
    pub fn write(&self, body: &Body) -> Result<(Id, Applied)> {
        let (id, applied) = lock(&self.peers).write(&self.key, body, now_ms())?;
        self.wake(&applied);
        Ok((id, applied))
    }

    /// Reads the records of the daemon with `read`: for example its heads,
    /// or the records that another daemon lacks ([`Peers::lacks`]).
    pub fn with_peers<T>(&self, read: impl FnOnce(&Peers) -> T) -> T {
        read(&lock(&self.peers))
    }

    /// Takes a record that the daemon `from` sent ([`Peers::keep`]), and
    /// wakes the sessions that a new message selects. On a new fork, it
    /// posts a note to the lead of the user of the daemon
    /// ([`Hub::tell_fork`]).
    pub fn keep(&self, bytes: Vec<u8>, from: Option<Public>) -> Result<Outcome> {
        let outcome = lock(&self.peers).keep(bytes, from)?;
        match &outcome {
            Outcome::Kept(applied) => self.wake(applied),
            Outcome::Fork(fork) => self.tell_fork(fork)?,
            Outcome::Duplicate | Outcome::Forked | Outcome::Later(_) | Outcome::Refused(_) => {}
        }
        Ok(outcome)
    }

    /// Posts a note on the fork `fork` to the lead of the user of the
    /// daemon (01M519Z38G3RAZPKGFA3R2CB3P): one note in each repository
    /// where that user has a lead. The note comes from the daemon: the
    /// user of its key, on this host, with no session.
    pub fn tell_fork(&self, fork: &Fork) -> Result<()> {
        let author = fork.first.author();
        let notes = self.read(|state| {
            let Some(user) = state.user_of(&self.public()) else {
                return Ok(Vec::new());
            };
            let from = SessionUri::new(
                Who::new(user, None)?,
                Place::host_only(&crate::identity::this_host())?,
            );
            let (of, host) = (
                state.user_of(&author),
                state.members().get(&author).map(|m| m.host.as_str()),
            );
            let body = text::peer_fork(&author.to_string(), of, host, fork.first.seq());
            let leads: BTreeSet<ThreadName> = state
                .runs()
                .filter_map(Run::thread)
                .filter(|thread| state.lead(user, thread).is_some())
                .collect();
            Ok::<_, anyhow::Error>(
                leads
                    .into_iter()
                    .map(|thread| {
                        Body::Message(Box::new(Message {
                            from: from.clone(),
                            to: vec![Selector {
                                user: Some(user.to_owned()),
                                repo: Some(thread.to_string()),
                                lead: Some(true),
                                ..Selector::default()
                            }],
                            thread,
                            kind: Kind::Note,
                            body: body.clone(),
                        }))
                    })
                    .collect::<Vec<Body>>(),
            )
        })?;
        for note in notes {
            self.write(&note)?;
        }
        Ok(())
    }

    /// Writes the `session` record of the start of the session `me` of
    /// the daemon, when the riff has no record of it that runs. A worker
    /// claims `claims`. With `lead`, the session takes the lead.
    pub fn start(
        &self,
        me: &SessionUri,
        worker: bool,
        lead: bool,
        claims: Vec<String>,
    ) -> Result<()> {
        let me = self.peer(me);
        let id = me.who().session().context("a session needs a session ID")?;
        let mut peers = lock(&self.peers);
        let state = peers.state();
        if state
            .run(&self.public(), id)
            .is_some_and(|run| !run.body.ended)
        {
            return Ok(());
        }
        let lead = lead.then(|| Lead {
            replaces: old_mark(state, &me),
        });
        let body = SessionBody {
            session: me.clone(),
            worker,
            lead,
            claims,
            status: None,
            ended: false,
        };
        peers.write(&self.key, &Body::Session(Box::new(body)), now_ms())?;
        Ok(())
    }

    /// Writes the `session` record of the end of the session `id` of the
    /// daemon, when it runs. Then the hub forgets the wakes of the
    /// session: after the record, no message wakes it.
    pub fn end(&self, id: &str) -> Result<()> {
        self.change(id, |body| body.ended = true)?;
        lock(&self.wakes).remove(id);
        Ok(())
    }

    /// Writes the next `session` record of the session `id` of the
    /// daemon: its last record, changed by `change`. It gives the new
    /// body, or `None` when the session does not run.
    fn change(
        &self,
        id: &str,
        change: impl FnOnce(&mut SessionBody),
    ) -> Result<Option<SessionBody>> {
        let mut peers = lock(&self.peers);
        let last = peers
            .state()
            .run(&self.public(), id)
            .filter(|run| !run.body.ended)
            .map(|run| run.body.clone());
        let Some(mut body) = last else {
            return Ok(None);
        };
        change(&mut body);
        peers.write(&self.key, &Body::Session(Box::new(body.clone())), now_ms())?;
        Ok(Some(body))
    }

    /// Waits for the next wake of the session `id` of the daemon, and
    /// gives its line ([`text::wake_line`]). When a message woke the
    /// session and the session did not read it, the line comes at once:
    /// the line of the newest such message (R49).
    pub async fn wait(&self, id: &str) -> String {
        loop {
            let woke = self.woke.notified();
            tokio::pin!(woke);
            woke.as_mut().enable();
            if let Some(wake) = self.missed(id) {
                return text::wake_line(&wake);
            }
            woke.await;
        }
    }

    /// The newest message that woke the session `id` and that it did not
    /// read. The hub forgets each wake of a message that the session
    /// read.
    fn missed(&self, id: &str) -> Option<Wake> {
        let mut wakes = lock(&self.wakes);
        let kept = wakes.get_mut(id)?;
        let reads = lock(&self.reads);
        kept.retain(|wake| {
            let read = reads.get(&(id.to_owned(), wake.thread.clone()));
            wake.seq > read.copied().unwrap_or_default()
        });
        kept.last().cloned()
    }

    /// Wakes each session of the daemon that the message of `applied`
    /// selects.
    fn wake(&self, applied: &Applied) {
        let (thread, number) = match applied {
            Applied::Message { thread, number } => (thread, number),
            Applied::Nothing | Applied::Changed => return,
        };
        let woken: Vec<(String, Wake)> = self.read(|state| {
            let Some(kept) = state.messages(thread).iter().find(|k| k.number == *number) else {
                return Vec::new();
            };
            let message = &kept.message;
            selected(state, &message.from, &message.to, message.kind)
                .into_iter()
                .filter(|run| run.author == self.public())
                .filter_map(|run| run.body.session.who().session().map(str::to_owned))
                .map(|id| (id, wake_of(kept)))
                .collect()
        });
        if woken.is_empty() {
            return;
        }
        let mut wakes = lock(&self.wakes);
        for (id, wake) in woken {
            wakes.entry(id).or_default().push(wake);
        }
        drop(wakes);
        self.woke.notify_waiters();
    }
}

/// The ID of the mark of the lead of the user of `me` in its repository
/// now, when it has one.
fn old_mark(state: &State, me: &SessionUri) -> Option<Id> {
    let thread = me.default_thread()?;
    state
        .lead(me.who().user(), &thread)
        .and_then(|run| run.mark)
}

/// The wake of the kept message `kept`.
fn wake_of(kept: &Kept) -> Wake {
    Wake {
        thread: kept.message.thread.clone(),
        seq: kept.number,
        from: kept.message.from.clone(),
        kind: kept.message.kind,
    }
}

/// The URI of `run` with its lead mark, as the riff sees it.
fn uri_of(state: &State, run: &Run) -> SessionUri {
    run.uri(state.is_lead(run))
}

/// Each session that runs, other than `from`, that one or more of the
/// selectors `to` match. A note wakes no session.
fn selected<'a>(state: &'a State, from: &SessionUri, to: &[Selector], kind: Kind) -> Vec<&'a Run> {
    match kind {
        Kind::Note => return Vec::new(),
        Kind::Message | Kind::Status | Kind::Other => {}
    }
    others(state, from)
        .filter(|run| {
            let uri = uri_of(state, run);
            to.iter().any(|selector| selector.matches(&uri))
        })
        .collect()
}

/// Each session that runs, other than `from`.
fn others<'a>(state: &'a State, from: &SessionUri) -> impl Iterator<Item = &'a Run> + use<'a> {
    let from = from.who().clone();
    state
        .runs()
        .filter(move |run| *run.body.session.who() != from)
}

/// Runs `call` on a thread for blocking work: a write of a record syncs
/// a file.
fn blocking<T: Send + 'static>(
    call: impl FnOnce() -> Result<T> + Send + 'static,
) -> BoxFuture<'static, Result<T>> {
    Box::pin(async move { tokio::task::spawn_blocking(call).await? })
}

/// The [`Talk`] of one session of the daemon. See the design of the
/// module.
///
/// A read of all after a later number skips the unread messages before
/// it, so it keeps the cursor. The next unread read shows each message
/// (01M51TKZSYR7PH629S29JX00ZD):
///
/// ```
/// use std::sync::Arc;
///
/// use riff::daemon_key::Key;
/// use riff::peer::body::{Body, Riff};
/// use riff::peer::keep::Peers;
/// use riff::peer::talk::{Hub, Session};
/// use riff::talk::Talk;
/// use riff_core::name::SessionUri;
/// use riff_core::selector::Selector;
/// use riff_core::wire::Kind;
///
/// let dir = tempfile::tempdir()?;
/// let key = Key::from_bytes(&[1; 32]);
/// let mut peers = Peers::open(dir.path(), key.public(), |_| {})?;
/// peers.write(&key, &Body::Riff(Riff { user: "mike".into() }), 1)?;
/// let hub = Arc::new(Hub::new(key, peers));
/// let a: SessionUri = "riff://mike@h/o/r?session=a".parse()?;
/// let b: SessionUri = "riff://mike@h/o/r?session=b".parse()?;
/// hub.start(&a, true, false, Vec::new())?;
/// hub.start(&b, true, false, Vec::new())?;
///
/// let talk = Session::new(hub);
/// tokio::runtime::Runtime::new()?.block_on(async {
///     let to_a = [Selector::session("a")];
///     talk.post(&b, None, &to_a, "one", Kind::Message).await?;
///     let two = talk.post(&b, None, &to_a, "two", Kind::Message).await?;
///     talk.post(&b, None, &to_a, "three", Kind::Message).await?;
///     let pages = talk.inbox_page(&a, None, true, Some(two.seq)).await?;
///     assert_eq!(pages[0].messages[0].message.body, "three");
///     let pages = talk.inbox_page(&a, None, false, None).await?;
///     assert_eq!(pages[0].messages.len(), 3);
///     Ok::<(), anyhow::Error>(())
/// })?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Clone, Debug)]
pub struct Session {
    hub: Arc<Hub>,
}

impl Session {
    /// The tools of a session of the daemon of `hub`.
    pub fn new(hub: Arc<Hub>) -> Session {
        Session { hub }
    }

    /// The run of the session `me` of this daemon.
    fn run_of(&self, state: &State, me: &SessionUri) -> Option<Run> {
        state.run(&self.hub.public(), me.who().session()?).cloned()
    }

    /// The URI of `me` with its lead mark and its claims, as the riff sees
    /// it.
    fn now(&self, me: &SessionUri) -> SessionUri {
        self.hub.read(|state| {
            self.run_of(state, me)
                .map_or_else(|| me.clone(), |run| uri_of(state, &run))
        })
    }

    /// The sessions of the riff: each one that runs, and with `all` each
    /// one that ended too.
    fn sessions(&self, all: bool) -> Vec<SessionInfo> {
        self.hub.read(|state| {
            let runs: Vec<&Run> = if all {
                state.runs_all().collect()
            } else {
                state.runs().collect()
            };
            runs.into_iter().map(|run| info(state, run)).collect()
        })
    }

    fn roster(&self, all: bool) -> WhoReply {
        let owner = self.hub.read(|state| match state.owner() {
            Some(user) => RiffOwner::Owner {
                user: user.to_owned(),
                email: String::new(),
            },
            None => RiffOwner::Nobody,
        });
        WhoReply {
            sessions: self.sessions(all),
            owner,
            people: Vec::new(),
        }
    }

    /// The threads that `me` may read: its repository thread, and each
    /// thread with a message of the riff but each direct thread of
    /// others.
    fn threads_of(&self, state: &State, me: &SessionUri) -> Vec<ThreadName> {
        let mut threads: BTreeSet<ThreadName> = state
            .threads()
            .filter(|thread| riff_state::state::may_read(me.who(), thread))
            .cloned()
            .collect();
        threads.extend(me.default_thread());
        threads.into_iter().collect()
    }

    fn threads(&self, me: &SessionUri) -> Vec<ThreadInfo> {
        self.hub.read(|state| {
            self.threads_of(state, me)
                .into_iter()
                .map(|thread| ThreadInfo {
                    unread: self.unread(state, me, &thread, false, None).len(),
                    members: members(state, &thread),
                    thread,
                })
                .collect()
        })
    }

    /// The read cursor of `me` in `thread`.
    fn cursor(&self, me: &SessionUri, thread: &ThreadName) -> u64 {
        let id = me.who().session().unwrap_or_default().to_owned();
        lock(&self.hub.reads)
            .get(&(id, thread.clone()))
            .copied()
            .unwrap_or_default()
    }

    /// Moves the read cursor of `me` in `thread` to `number`, not back.
    fn moved(&self, me: &SessionUri, thread: &ThreadName, number: u64) {
        let id = me.who().session().unwrap_or_default().to_owned();
        let mut reads = lock(&self.hub.reads);
        let cursor = reads.entry((id, thread.clone())).or_default();
        *cursor = (*cursor).max(number);
    }

    /// The messages of `thread` that `me` did not read, but its own, or
    /// with `all` each one after `after`.
    fn unread<'a>(
        &self,
        state: &'a State,
        me: &SessionUri,
        thread: &ThreadName,
        all: bool,
        after: Option<u64>,
    ) -> Vec<&'a Kept> {
        let from = if all {
            after.unwrap_or(0)
        } else {
            self.cursor(me, thread)
        };
        state
            .messages(thread)
            .iter()
            .filter(|kept| kept.number > from && (all || kept.message.from.who() != me.who()))
            .collect()
    }

    /// With `all`, whether a read after `after` skips a message of
    /// another session that `me` did not read (01M51TKZSYR7PH629S29JX00ZD).
    fn skips(
        &self,
        state: &State,
        me: &SessionUri,
        thread: &ThreadName,
        all: bool,
        after: Option<u64>,
    ) -> bool {
        let Some(from) = after.filter(|_| all) else {
            return false;
        };
        let cursor = self.cursor(me, thread);
        state.messages(thread).iter().any(|kept| {
            (cursor + 1..=from).contains(&kept.number) && kept.message.from.who() != me.who()
        })
    }

    /// One page of each thread of `me`, or of `thread`. The cursor of
    /// each thread moves to the end of its page. A page that skips an
    /// unread message keeps the cursor, so the next unread read shows
    /// the message ([`Session::skips`], 01M51TKZSYR7PH629S29JX00ZD).
    fn inbox(
        &self,
        me: &SessionUri,
        thread: Option<&ThreadName>,
        all: bool,
        after: Option<u64>,
    ) -> Result<Vec<Inbox>> {
        if let Some(thread) = thread
            && !riff_state::state::may_read(me.who(), thread)
        {
            bail!("no thread named {thread}");
        }
        let pages: Vec<(Inbox, bool)> = self.hub.read(|state| {
            let threads = match thread {
                Some(thread) => vec![thread.clone()],
                None => self.threads_of(state, me),
            };
            threads
                .into_iter()
                .filter_map(|thread| {
                    let shown = self.unread(state, me, &thread, all, after);
                    let next = (shown.len() > PAGE).then(|| shown[PAGE - 1].number);
                    let messages: Vec<Shown> = shown
                        .into_iter()
                        .take(PAGE)
                        .map(|kept| Shown {
                            message: wire_message(kept),
                            verified: true,
                        })
                        .collect();
                    if messages.is_empty() {
                        return None;
                    }
                    let skips = self.skips(state, me, &thread, all, after);
                    let page = Inbox {
                        members: members(state, &thread),
                        thread,
                        messages,
                        next,
                    };
                    Some((page, skips))
                })
                .collect()
        });
        for (page, skips) in &pages {
            if !skips && let Some(last) = page.messages.last() {
                self.moved(me, &page.thread, last.message.seq);
            }
        }
        Ok(pages.into_iter().map(|(page, _)| page).collect())
    }

    /// Posts a message of `me`: `thread`, or with `None` a direct message
    /// to the one session that `to` names.
    fn post(
        &self,
        me: &SessionUri,
        thread: Option<&ThreadName>,
        to: &[Selector],
        body: &str,
        kind: Kind,
    ) -> Result<Posted> {
        if to.iter().any(Selector::is_empty) {
            bail!("a selector needs one or more fields");
        }
        let from = self.now(me);
        let thread = match thread {
            Some(thread) if thread.is_direct() => {
                bail!("leave out the thread to send a direct message")
            }
            Some(thread) => thread.clone(),
            None => ThreadName::direct(me.who(), &self.direct_target(me, to)?),
        };
        let message = Message {
            from: from.clone(),
            thread: thread.clone(),
            to: to.to_vec(),
            kind,
            body: body.to_owned(),
        };
        let (_, applied) = self.hub.write(&Body::Message(Box::new(message)))?;
        let seq = match applied {
            Applied::Message { number, .. } => number,
            Applied::Nothing | Applied::Changed => 0,
        };
        let (woken, unmatched) = self.hub.read(|state| {
            let woken = selected(state, &from, to, kind)
                .into_iter()
                .map(|run| uri_of(state, run))
                .collect();
            let unmatched = to
                .iter()
                .filter(|selector| {
                    !others(state, &from).any(|run| selector.matches(&uri_of(state, run)))
                })
                .cloned()
                .collect();
            (woken, unmatched)
        });
        Ok(Posted {
            thread,
            seq,
            woken,
            unmatched,
        })
    }

    /// The one session that a direct message goes to: one selector with a
    /// session or `lead=true`, that matches one session that runs.
    fn direct_target(&self, me: &SessionUri, to: &[Selector]) -> Result<Who> {
        let [selector] = to else {
            bail!("a direct message needs exactly one selector");
        };
        let to_lead = selector.lead == Some(true);
        if selector.session.is_none() && !to_lead {
            bail!("a direct message needs a selector with a session or lead=true");
        }
        let matched: Vec<Who> = self.hub.read(|state| {
            others(state, me)
                .filter(|run| selector.matches(&uri_of(state, run)))
                .map(|run| run.body.session.who().clone())
                .collect()
        });
        match matched.as_slice() {
            [who] => Ok(who.clone()),
            [] if to_lead => {
                bail!("no other session is the lead for {selector}. Ask your own user.")
            }
            [] => bail!("no session matches {selector}. Use who to list the sessions."),
            more => bail!(
                "{} sessions match {selector}. Name one session.",
                more.len()
            ),
        }
    }

    /// The selector of the session `session`: `lead`, a session ID or the
    /// start of one, or a full URI.
    fn selector_of(&self, me: &SessionUri, session: &str) -> Result<Selector> {
        if session == crate::api::LEAD {
            return Ok(Selector::lead(me.who().user(), &me.place().repo_text()));
        }
        if let Ok(uri) = session.parse::<SessionUri>() {
            let id = uri.who().session().context("that URI has no session ID")?;
            return Ok(Selector::session(id));
        }
        let ids: Vec<String> = self.hub.read(|state| {
            state
                .runs()
                .filter_map(|run| run.body.session.who().session().map(str::to_owned))
                .filter(|id| id.starts_with(session))
                .collect()
        });
        if ids.iter().any(|id| id == session) {
            return Ok(Selector::session(session));
        }
        match ids.as_slice() {
            [] => Ok(Selector::session(session)),
            [one] => Ok(Selector::session(one)),
            more => bail!(
                "{session} is the start of more than one session ID: {}. Give more of it.",
                more.join(", ")
            ),
        }
    }

    fn status(&self, me: &SessionUri, status: &Status) -> Result<()> {
        status.check().map_err(anyhow::Error::msg)?;
        let id = me.who().session().context("a session needs a session ID")?;
        self.hub
            .change(id, |body| body.status = Some(status.step.clone()))?
            .ok_or_else(|| anyhow!("{}", text::PEER_NO_SESSION))?;
        Ok(())
    }

    fn blocked(&self, me: &SessionUri, reason: &str) -> Result<Told> {
        let step = format!("blocked: {reason}");
        self.status(me, &Status { step: step.clone() })?;
        if self.is_lead(me) {
            return Ok(Told::You);
        }
        let to = [Selector::lead(me.who().user(), &me.place().repo_text())];
        Ok(match self.post(me, None, &to, &step, Kind::Message) {
            Ok(_) => Told::Lead,
            Err(_) => Told::Nobody,
        })
    }

    /// True when `me` is the lead of its user in its repository.
    fn is_lead(&self, me: &SessionUri) -> bool {
        self.hub.read(|state| {
            self.run_of(state, me)
                .is_some_and(|run| state.is_lead(&run))
        })
    }

    /// The pauses that `me` sees.
    fn pauses(&self, me: &SessionUri) -> RiffReply {
        self.hub.read(|state| {
            let info = |mark: &Mark| PauseInfo {
                by: Some(By::Person(mark.user.clone())),
                at_ms: mark.time_ms,
            };
            let riff = state
                .pauses()
                .find(|pause| pause.thread.is_none())
                .map(info);
            let mut repositories: Vec<RepositoryPause> = Vec::new();
            for pause in state.pauses() {
                if let Some(thread) = &pause.thread
                    && !repositories.iter().any(|r| &r.repository == thread)
                {
                    repositories.push(RepositoryPause {
                        repository: thread.clone(),
                        pause: info(pause),
                    });
                }
            }
            let paused = riff.is_some()
                || me
                    .default_thread()
                    .is_some_and(|thread| repositories.iter().any(|r| r.repository == thread));
            RiffReply {
                state: if paused {
                    RiffState::Paused
                } else {
                    RiffState::Running
                },
                changed: false,
                riff,
                repositories,
            }
        })
    }

    /// The error of a step that only a lead does, when `me` is not the
    /// lead and the daemon is not the daemon of the owner.
    fn must_lead(&self, me: &SessionUri) -> Result<()> {
        let owner = self.hub.read(|state| state.riff() == self.hub.public());
        if owner || self.is_lead(me) {
            Ok(())
        } else {
            bail!("{}", text::PEER_NOT_LEAD)
        }
    }

    /// True when the daemon may end `mark`: the daemon of the owner, or a
    /// daemon of the user of the mark, but only the owner for a pause of
    /// the whole riff (01M519Z3J7Z0M3PNDY249Q57FH).
    fn may_end(&self, state: &State, mark: &Mark) -> bool {
        let owner = state.riff() == self.hub.public();
        let user = state.user_of(&self.hub.public()).unwrap_or_default();
        owner || (mark.thread.is_some() && mark.user == user)
    }

    fn hold(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
        item: &str,
        reason: &str,
    ) -> Result<HoldReply> {
        self.must_lead(me)?;
        let held = self
            .hub
            .read(|state| state.holds_of(thread, item).next().is_some());
        let plan = Plan::Hold {
            thread: thread.clone(),
            item: item.to_owned(),
            reason: reason.to_owned(),
        };
        self.hub.write(&Body::Plan(plan))?;
        Ok(HoldReply {
            changed: !held,
            holder: None,
        })
    }

    fn free(&self, me: &SessionUri, thread: &ThreadName, item: &str) -> Result<FreeReply> {
        self.must_lead(me)?;
        let ends: Vec<Id> = self.hub.read(|state| {
            state
                .holds_of(thread, item)
                .filter(|hold| self.may_end(state, hold))
                .map(|hold| hold.id)
                .collect()
        });
        if ends.is_empty() {
            return Ok(FreeReply { freed: false });
        }
        self.hub.write(&Body::Plan(Plan::Free { ends }))?;
        Ok(FreeReply { freed: true })
    }

    fn lead(&self, me: &SessionUri) -> Result<LeadReply> {
        let id = me.who().session().context("a session needs a session ID")?;
        let thread = me.default_thread().context(text::PEER_NO_REPOSITORY)?;
        let old = self.hub.read(|state| {
            state
                .lead(me.who().user(), &thread)
                .map(|run| (uri_of(state, run), run.mark))
        });
        if let Some((uri, _)) = &old
            && uri.who() == me.who()
        {
            return Ok(LeadReply {
                lead: uri.clone(),
                replaced: None,
            });
        }
        let replaces = old.as_ref().and_then(|(_, mark)| *mark);
        self.hub
            .change(id, |body| body.lead = Some(Lead { replaces }))?
            .ok_or_else(|| anyhow!("{}", text::PEER_NO_SESSION))?;
        Ok(LeadReply {
            lead: self.now(me),
            replaced: old.map(|(uri, _)| uri),
        })
    }

    fn set_pause(
        &self,
        me: &SessionUri,
        scope: &PauseScope,
        to: RiffState,
    ) -> Result<(RiffReply, Vec<Posted>)> {
        self.must_lead(me)?;
        let thread = match scope {
            PauseScope::Riff => None,
            PauseScope::Here => Some(me.default_thread().context(text::PEER_NO_REPOSITORY)?),
            PauseScope::Repository(thread) => Some(thread.clone()),
        };
        // As in riff-server, a change is a change of the pause of the
        // scope, not of the state of `me`.
        let held = |s: &Session| {
            s.hub
                .read(|state| state.pauses().any(|pause| pause.thread == thread))
        };
        let before = held(self);
        match to {
            RiffState::Paused => {
                let pause = Plan::Pause {
                    thread: thread.clone(),
                };
                self.hub.write(&Body::Plan(pause))?;
            }
            RiffState::Running => {
                let ends: Vec<Id> = self.hub.read(|state| {
                    state
                        .pauses()
                        .filter(|pause| pause.thread == thread && self.may_end(state, pause))
                        .map(|pause| pause.id)
                        .collect()
                });
                if !ends.is_empty() {
                    self.hub.write(&Body::Plan(Plan::Resume { ends }))?;
                }
            }
        }
        let mut reply = self.pauses(me);
        reply.changed = held(self) != before;
        if !reply.changed {
            return Ok((reply, Vec::new()));
        }
        // The news wakes each session that the change stops or starts, as
        // riff-server does (01M3JCG3YD7C2Y3V0QJPF082YH): the sessions of
        // the repository, or for the whole riff the sessions of each
        // repository that no pause of its own stops.
        let repos = match &thread {
            Some(_) if reply.riff.is_some() => Vec::new(),
            Some(thread) => vec![thread.clone()],
            None => self.hub.read(|state| {
                let repos: BTreeSet<ThreadName> = state
                    .runs()
                    .filter_map(|run| run.body.session.default_thread())
                    .filter(|repo| reply.repository(repo).is_none())
                    .collect();
                repos.into_iter().collect()
            }),
        };
        let news = text::riff_news(thread.as_ref(), to);
        let posted = repos
            .into_iter()
            .map(|repo| {
                let to = Selector {
                    repo: Some(repo.to_string()),
                    ..Selector::default()
                };
                self.post(me, Some(&repo), &[to], &news, Kind::Message)
            })
            .collect::<Result<Vec<Posted>>>()?;
        Ok((reply, posted))
    }
}

/// The info of `run` for `who`.
fn info(state: &State, run: &Run) -> SessionInfo {
    SessionInfo {
        uri: uri_of(state, run),
        live: !run.body.ended,
        idle_secs: 0,
        status: run.body.status.clone().map(|step| StatusInfo {
            status: Status { step },
            age_secs: 0,
            stale: false,
        }),
        worker: run.body.worker,
        container: true,
        binary: None,
        stopping: false,
        claims_secs: 0,
        must_clear: false,
        fresh_secs: None,
        state: None,
        work: None,
        waits: None,
        blocked: None,
        step: None,
        machine: None,
    }
}

/// The sessions that posted in `thread`.
fn members(state: &State, thread: &ThreadName) -> Vec<SessionUri> {
    let mut members: Vec<SessionUri> = Vec::new();
    for kept in state.messages(thread) {
        if !members.iter().any(|m| m.who() == kept.message.from.who()) {
            members.push(kept.message.from.clone());
        }
    }
    members
}

/// The wire form of the kept message `kept`, with its number as its
/// sequence number.
fn wire_message(kept: &Kept) -> riff_core::wire::Message {
    riff_core::wire::Message {
        seq: kept.number,
        from: kept.message.from.clone(),
        to: kept.message.to.clone(),
        body: kept.message.body.clone(),
        at_ms: kept.time_ms,
        kind: kept.message.kind,
        sig: None,
        payload: None,
    }
}

impl Session {
    /// Runs `call` with the session `me` in the riff of peers
    /// ([`Hub::peer`]), on a thread for blocking work.
    fn on<T: Send + 'static>(
        &self,
        me: &SessionUri,
        call: impl FnOnce(&Session, &SessionUri) -> Result<T> + Send + 'static,
    ) -> BoxFuture<'static, Result<T>> {
        let (s, me) = (self.clone(), me.clone());
        blocking(move || {
            let me = s.hub.peer(&me);
            call(&s, &me)
        })
    }
}

impl Talk for Session {
    fn who<'a>(&'a self, me: &'a SessionUri, all: bool) -> BoxFuture<'a, Result<Vec<SessionInfo>>> {
        self.on(me, move |s, _| Ok(s.sessions(all)))
    }

    fn roster<'a>(&'a self, me: &'a SessionUri, all: bool) -> BoxFuture<'a, Result<WhoReply>> {
        self.on(me, move |s, _| Ok(s.roster(all)))
    }

    fn pauses<'a>(&'a self, me: &'a SessionUri) -> BoxFuture<'a, Result<RiffReply>> {
        self.on(me, |s, me| Ok(s.pauses(me)))
    }

    fn threads<'a>(&'a self, me: &'a SessionUri) -> BoxFuture<'a, Result<Vec<ThreadInfo>>> {
        self.on(me, |s, me| Ok(s.threads(me)))
    }

    fn post<'a>(
        &'a self,
        me: &'a SessionUri,
        thread: Option<&'a ThreadName>,
        to: &'a [Selector],
        body: &'a str,
        kind: Kind,
    ) -> BoxFuture<'a, Result<Posted>> {
        let (thread, to, body) = (thread.cloned(), to.to_vec(), body.to_owned());
        self.on(me, move |s, me| {
            s.post(me, thread.as_ref(), &to, &body, kind)
        })
    }

    fn status<'a>(&'a self, me: &'a SessionUri, status: &'a Status) -> BoxFuture<'a, Result<()>> {
        let status = status.clone();
        self.on(me, move |s, me| s.status(me, &status))
    }

    fn blocked<'a>(&'a self, me: &'a SessionUri, reason: &'a str) -> BoxFuture<'a, Result<Told>> {
        let reason = reason.to_owned();
        self.on(me, move |s, me| s.blocked(me, &reason))
    }

    fn tell<'a>(
        &'a self,
        me: &'a SessionUri,
        session: &'a str,
        body: &'a str,
    ) -> BoxFuture<'a, Result<Posted>> {
        let (session, body) = (session.to_owned(), body.to_owned());
        self.on(me, move |s, me| {
            let to = [s.selector_of(me, &session)?];
            s.post(me, None, &to, &body, Kind::Message)
        })
    }

    fn inbox_page<'a>(
        &'a self,
        me: &'a SessionUri,
        thread: Option<&'a ThreadName>,
        all: bool,
        after: Option<u64>,
    ) -> BoxFuture<'a, Result<Vec<Inbox>>> {
        let thread = thread.cloned();
        self.on(me, move |s, me| s.inbox(me, thread.as_ref(), all, after))
    }

    fn hold<'a>(
        &'a self,
        me: &'a SessionUri,
        thread: &'a ThreadName,
        item: &'a str,
        reason: &'a str,
    ) -> BoxFuture<'a, Result<HoldReply>> {
        let (thread, item, reason) = (thread.clone(), item.to_owned(), reason.to_owned());
        self.on(me, move |s, me| s.hold(me, &thread, &item, &reason))
    }

    fn free<'a>(
        &'a self,
        me: &'a SessionUri,
        thread: &'a ThreadName,
        item: &'a str,
    ) -> BoxFuture<'a, Result<FreeReply>> {
        let (thread, item) = (thread.clone(), item.to_owned());
        self.on(me, move |s, me| s.free(me, &thread, &item))
    }

    fn lead<'a>(&'a self, me: &'a SessionUri) -> BoxFuture<'a, Result<LeadReply>> {
        self.on(me, |s, me| s.lead(me))
    }

    fn set_pause<'a>(
        &'a self,
        me: &'a SessionUri,
        scope: &'a PauseScope,
        state: RiffState,
    ) -> BoxFuture<'a, Result<(RiffReply, Vec<Posted>)>> {
        let scope = scope.clone();
        self.on(me, move |s, me| s.set_pause(me, &scope, state))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use super::*;
    use crate::peer::body::Riff;

    /// A session of mike on the host h in the repository `repo`.
    fn uri(id: &str, repo: &str) -> SessionUri {
        format!("riff://mike@h/{repo}?session={id}")
            .parse()
            .unwrap()
    }

    /// The hub of a riff of its own of mike, with the workers `sessions`
    /// of the daemon: each one a session ID and its repository.
    fn hub(dir: &Path, sessions: &[(&str, &str)]) -> Arc<Hub> {
        let key = Key::from_bytes(&[1; 32]);
        let mut peers = Peers::open(dir, key.public(), |_| {}).unwrap();
        let riff = Body::Riff(Riff {
            user: "mike".into(),
        });
        peers.write(&key, &riff, 1).unwrap();
        let hub = Arc::new(Hub::new(key, peers));
        for (id, repo) in sessions {
            hub.start(&uri(id, repo), true, false, Vec::new()).unwrap();
        }
        hub
    }

    /// The bodies of the messages of `pages`, sorted.
    fn bodies(pages: &[Inbox]) -> Vec<String> {
        let mut bodies: Vec<String> = pages
            .iter()
            .flat_map(|page| &page.messages)
            .map(|shown| shown.message.body.clone())
            .collect();
        bodies.sort();
        bodies
    }

    /// The wake line of the session `id`, or `None` when no wake comes
    /// soon.
    async fn wake(hub: &Hub, id: &str) -> Option<String> {
        tokio::time::timeout(Duration::from_millis(200), hub.wait(id))
            .await
            .ok()
    }

    /// R79: a post of a session does not move its read cursor. So a
    /// message of another session that came before the post, and that
    /// the session did not read, stays unread: in its thread, and in a
    /// direct thread.
    #[test]
    fn a_post_keeps_the_unread_messages_of_others_unread() {
        let dir = tempfile::tempdir().unwrap();
        let s = Session::new(hub(dir.path(), &[("a", "o/r"), ("b", "o/r")]));
        let (a, b) = (uri("a", "o/r"), uri("b", "o/r"));
        let repo: ThreadName = "o/r".parse().unwrap();
        let (to_a, to_b) = ([Selector::session("a")], [Selector::session("b")]);
        s.post(&b, Some(&repo), &to_a, "a note to a", Kind::Note)
            .unwrap();
        s.post(&a, Some(&repo), &[], "started", Kind::Note).unwrap();
        s.post(&b, None, &to_a, "a direct word", Kind::Message)
            .unwrap();
        s.post(&a, None, &to_b, "my word", Kind::Message).unwrap();

        let unread: usize = s.threads(&a).iter().map(|t| t.unread).sum();
        assert_eq!(unread, 2);
        let read = bodies(&s.inbox(&a, None, false, None).unwrap());
        assert_eq!(read, ["a direct word", "a note to a"]);
        assert!(bodies(&s.inbox(&a, None, false, None).unwrap()).is_empty());
    }

    /// 01M51TKZSYR7PH629S29JX00ZD: a tell to a before its first read,
    /// then a read of all after a later number. The read shows the
    /// second tell and skips the first, so it keeps the cursor: the first
    /// tell stays unread until a read shows it.
    #[test]
    fn a_read_of_all_after_a_later_number_keeps_a_tell_unread() {
        let dir = tempfile::tempdir().unwrap();
        let s = Session::new(hub(dir.path(), &[("a", "o/r"), ("b", "o/r")]));
        let (a, b) = (uri("a", "o/r"), uri("b", "o/r"));
        let repo: ThreadName = "o/r".parse().unwrap();
        let to_a = [Selector::session("a")];
        let request = "request: claim issue-12";
        let dm = s.post(&b, None, &to_a, request, Kind::Message).unwrap();
        let later = s.post(&b, Some(&repo), &[], "a note", Kind::Note).unwrap();
        s.post(&b, None, &to_a, "a second word", Kind::Message)
            .unwrap();
        let unread = |s: &Session| -> Vec<(String, usize)> {
            s.threads(&a)
                .into_iter()
                .map(|t| (t.thread.to_string(), t.unread))
                .collect()
        };
        let (dm, repo) = (dm.thread.to_string(), repo.to_string());

        let read = bodies(&s.inbox(&a, None, true, Some(later.seq)).unwrap());
        assert_eq!(read, ["a second word"]);
        assert_eq!(unread(&s), [(dm.clone(), 2), (repo.clone(), 1)]);

        let read = bodies(&s.inbox(&a, None, false, None).unwrap());
        assert_eq!(read, ["a note", "a second word", request]);
        assert_eq!(unread(&s), [(dm, 0), (repo, 0)]);
    }

    /// R49: a watch wakes a session one time, for the newest message that
    /// woke it and that it did not read. After a read, a watch waits for
    /// a new message. A note and an own post wake no session.
    #[tokio::test]
    async fn a_watch_wakes_once_for_the_newest_unread_message() {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub(dir.path(), &[("a", "o/r"), ("b", "o/r")]);
        let s = Session::new(hub.clone());
        let (a, b) = (uri("a", "o/r"), uri("b", "o/r"));
        let repo: ThreadName = "o/r".parse().unwrap();
        let to_a = [Selector::session("a")];
        s.post(&b, Some(&repo), &to_a, "one", Kind::Message)
            .unwrap();
        let two = s
            .post(&b, Some(&repo), &to_a, "two", Kind::Message)
            .unwrap();
        s.post(&b, Some(&repo), &to_a, "a note", Kind::Note)
            .unwrap();

        // Two wakes wait: the watch gives the newest, and gives it again
        // until a read.
        let line = format!("(message {})", two.seq);
        assert!(wake(&hub, "a").await.unwrap().contains(&line));
        assert!(wake(&hub, "a").await.unwrap().contains(&line));
        assert_eq!(bodies(&s.inbox(&a, None, false, None).unwrap()).len(), 3);
        assert_eq!(wake(&hub, "a").await, None);
        assert_eq!(wake(&hub, "b").await, None);

        // A watch that waits wakes on the next message.
        let waits = tokio::spawn({
            let hub = hub.clone();
            async move { hub.wait("a").await }
        });
        tokio::task::yield_now().await;
        let three = s
            .post(&b, Some(&repo), &to_a, "three", Kind::Message)
            .unwrap();
        let line = tokio::time::timeout(Duration::from_secs(5), waits)
            .await
            .unwrap()
            .unwrap();
        assert!(line.contains(&format!("(message {})", three.seq)), "{line}");

        // The end of a session drops its wakes.
        hub.end("a").unwrap();
        assert_eq!(wake(&hub, "a").await, None);
    }

    /// 01M3JCG3YD7C2Y3V0QJPF082YH: a pause of the whole riff wakes each
    /// session that it stops. It posts the news to each repository of a
    /// session that runs, but not to a repository that a pause of its
    /// own stops. A pause of a repository in a paused riff posts nothing.
    #[test]
    fn a_pause_of_the_riff_tells_each_repository_that_it_stops() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = [("a", "o/r"), ("b", "o/r"), ("c", "o/s"), ("d", "o/u")];
        let s = Session::new(hub(dir.path(), &sessions));
        let a = uri("a", "o/r");
        let threads = |posted: &[Posted]| -> Vec<String> {
            posted.iter().map(|p| p.thread.to_string()).collect()
        };
        let woken = |posted: &[Posted]| -> Vec<String> {
            posted
                .iter()
                .flat_map(|p| &p.woken)
                .filter_map(|uri| uri.who().session().map(str::to_owned))
                .collect()
        };

        let os = PauseScope::Repository("o/s".parse().unwrap());
        let (_, posted) = s.set_pause(&a, &os, RiffState::Paused).unwrap();
        assert_eq!(woken(&posted), ["c"]);

        let (reply, posted) = s
            .set_pause(&a, &PauseScope::Riff, RiffState::Paused)
            .unwrap();
        assert!(reply.changed);
        assert_eq!(threads(&posted), ["o/r", "o/u"]);
        assert_eq!(woken(&posted), ["b", "d"]);

        let (_, posted) = s
            .set_pause(&a, &PauseScope::Here, RiffState::Paused)
            .unwrap();
        assert!(posted.is_empty());

        let (_, posted) = s
            .set_pause(&a, &PauseScope::Riff, RiffState::Running)
            .unwrap();
        assert_eq!(threads(&posted), ["o/u"]);
    }
}
