//! The group "threads": the commands [`Join`], [`Leave`],
//! [`Post`] and [`Announce`], and the threads with their messages. The
//! wire type of a command that a client can send is its command type.
//! [`Announce`] is a command of the server.
//!
//! - Part of the riff: [`Threads`]. The members and the last
//!   [`KEEP_MESSAGES`] messages of each thread, and the index of the
//!   signed messages for the copy check (01M3JEJVXXEPPNGT3FY4ZSFCWZ).
//! - `apply`: `Threads::posted`, `Threads::joined` and
//!   `Threads::left` for the records `posted`, `joined_thread` and
//!   `left_thread`. `Threads::forgotten` for a `session_forgotten`
//!   record: see [`super::riff`].
//! - Checkpoint: `Saved`, the field `threads`.
//! - The rules of a post for `handle`: which thread it goes to, and
//!   which sessions it wakes (`View::thread_of`, `View::selected`).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Instant;

use riff_core::name::{ThreadName, Who};
use riff_core::record::{Change, Member, Posted, Record};
use riff_core::selector::Selector;
use riff_core::signed::payload_hash;
use riff_core::wire::{self, Join, Kind, Leave, Message, Post, Wake};
use serde::{Deserialize, Serialize};

use super::KEEP_MESSAGES;
use super::command::{Caller, Command, CommandKind, Done, Now, Refused};
use super::sessions::Sessions;
use super::view::View;

/// The threads of the riff.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Threads {
    pub(super) by_name: BTreeMap<ThreadName, Thread>,
    /// The seq of each kept signed message, by thread and by the hash of
    /// its payload.
    pub(super) copies: BTreeMap<(ThreadName, String), u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Thread {
    pub(super) members: BTreeSet<Who>,
    /// The last [`KEEP_MESSAGES`] messages.
    pub(super) messages: VecDeque<Stored>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Stored {
    pub(super) message: Message,
    /// Each session that the message woke.
    pub(super) woken: BTreeSet<Who>,
}

impl Threads {
    /// The thread gets the message. It keeps its last [`KEEP_MESSAGES`]
    /// messages.
    pub(super) fn posted(&mut self, posted: &Posted) -> Result<(), &'static str> {
        let Posted {
            thread,
            message,
            woken,
        } = posted;
        if let Some(payload) = &message.payload {
            self.copies
                .insert((thread.clone(), payload_hash(payload)), message.seq);
        }
        let messages = &mut self.by_name.entry(thread.clone()).or_default().messages;
        messages.push_back(Stored {
            message: message.clone(),
            woken: woken.clone(),
        });
        while messages.len() > KEEP_MESSAGES {
            let Some(old) = messages.pop_front() else {
                break;
            };
            if let Some(payload) = &old.message.payload {
                self.copies.remove(&(thread.clone(), payload_hash(payload)));
            }
        }
        Ok(())
    }

    /// The session is a member of the thread. A new thread starts.
    pub(super) fn joined(&mut self, member: &Member) -> Result<(), &'static str> {
        self.by_name
            .entry(member.thread.clone())
            .or_default()
            .members
            .insert(member.session.who().clone());
        Ok(())
    }

    /// The session is no member of the thread. True when it was one.
    pub(super) fn left(&mut self, member: &Member) -> bool {
        self.by_name
            .get_mut(&member.thread)
            .is_some_and(|t| t.members.remove(member.session.who()))
    }

    /// Drops a forgotten session from each thread, and each direct
    /// thread whose other session is gone too. `sessions` has the
    /// sessions that are still known.
    pub(super) fn forgotten(&mut self, who: &Who, sessions: &Sessions) {
        for thread in self.by_name.values_mut() {
            thread.members.remove(who);
        }
        let gone: BTreeSet<ThreadName> = self
            .by_name
            .keys()
            .filter(|thread| {
                thread
                    .peer(who)
                    .is_some_and(|peer| peer == *who || !sessions.known.contains_key(&peer))
            })
            .cloned()
            .collect();
        self.by_name.retain(|thread, _| !gone.contains(thread));
        self.copies.retain(|(thread, _), _| !gone.contains(thread));
    }

    /// True when the riff has the thread.
    pub(super) fn has(&self, thread: &ThreadName) -> bool {
        self.by_name.contains_key(thread)
    }

    pub(super) fn member(&self, who: &Who, thread: &ThreadName) -> bool {
        self.by_name
            .get(thread)
            .is_some_and(|t| t.members.contains(who))
    }

    fn last_seq(&self, thread: &ThreadName) -> u64 {
        self.by_name
            .get(thread)
            .and_then(|t| t.messages.back())
            .map_or(0, |m| m.message.seq)
    }

    /// The threads, for a checkpoint.
    pub(super) fn saved(&self) -> Saved {
        Saved {
            threads: self
                .by_name
                .iter()
                .map(|(name, thread)| SavedThread {
                    thread: name.clone(),
                    members: thread.members.iter().cloned().collect(),
                    messages: thread
                        .messages
                        .iter()
                        .map(|m| SavedMessage {
                            message: m.message.clone(),
                            woken: m.woken.clone(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

/// The part of the checkpoint of this group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Saved {
    #[serde(default)]
    threads: Vec<SavedThread>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SavedThread {
    thread: ThreadName,
    #[serde(default)]
    members: Vec<Who>,
    #[serde(default)]
    messages: Vec<SavedMessage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SavedMessage {
    message: Message,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    woken: BTreeSet<Who>,
}

impl Saved {
    pub(super) fn restore(self) -> Threads {
        let mut threads = Threads::default();
        for t in self.threads {
            for m in &t.messages {
                if let Some(payload) = &m.message.payload {
                    threads
                        .copies
                        .insert((t.thread.clone(), payload_hash(payload)), m.message.seq);
                }
            }
            let thread = Thread {
                members: t.members.into_iter().collect(),
                messages: t
                    .messages
                    .into_iter()
                    .map(|m| Stored {
                        message: m.message,
                        woken: m.woken,
                    })
                    .collect(),
            };
            threads.by_name.insert(t.thread, thread);
        }
        threads
    }
}

/// True when `who` may read the messages of `thread`: each thread but a
/// direct thread of two other sessions. `read`, `tail` and `watch` use
/// this one rule (01M3T411J3TN00FER230V3YX17).
///
/// ```
/// use riff_core::name::{ThreadName, Who};
/// use riff_server::state::may_read;
///
/// let (a, b, c) = (Who::new("ann", Some("a"))?, Who::new("bob", Some("b"))?, Who::new("cy", Some("c"))?);
/// let direct = ThreadName::direct(&a, &b);
/// assert!(may_read(&a, &direct) && may_read(&b, &direct));
/// assert!(!may_read(&c, &direct));
/// assert!(may_read(&c, &"acme/app".parse()?));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn may_read(who: &Who, thread: &ThreadName) -> bool {
    !thread.is_direct() || thread.peer(who).is_some()
}

/// The wake that `message` in `thread` gives.
pub(super) fn wake(thread: &ThreadName, message: &Message) -> Wake {
    Wake {
        thread: thread.clone(),
        seq: message.seq,
        from: message.from.clone(),
        kind: message.kind,
    }
}

impl View<'_> {
    /// Finds the one session that a direct message goes to (R179).
    fn direct_target(&self, from: &Who, to: &[Selector], now: Instant) -> Result<Who, String> {
        let [selector] = to else {
            return Err("a direct message needs exactly one selector".into());
        };
        let to_lead = selector.lead == Some(true);
        if selector.session.is_none() && !to_lead {
            return Err("a direct message needs a selector with a session or lead=true".into());
        }
        let (matched, gone): (Vec<&Who>, Vec<&Who>) = self
            .presence
            .sessions
            .keys()
            .filter(|who| *who != from && selector.matches(&self.uri(who, now)))
            .partition(|who| !self.gone(who, now));
        match matched[..] {
            [who] => Ok(who.clone()),
            [] if !gone.is_empty() && !to_lead => Err(format!(
                "the session {selector} is gone: it ended, or it stopped. \
                 Use who to list the sessions."
            )),
            [] if to_lead => Err(format!(
                "no other session is the lead for {selector}. Ask your own user."
            )),
            [] => Err(format!(
                "no session matches {selector}. Use who to list the sessions."
            )),
            _ => Err(format!(
                "{} sessions match {selector}. Name one session.",
                matched.len()
            )),
        }
    }

    /// The thread of a post: `thread`, or the direct thread of `from` and
    /// the one session that `to` names.
    fn thread_of(
        &self,
        from: &Who,
        thread: Option<&ThreadName>,
        to: &[Selector],
        now: Instant,
    ) -> Result<ThreadName, String> {
        if to.iter().any(Selector::is_empty) {
            return Err("a selector needs one or more fields".into());
        }
        match thread {
            Some(thread) if thread.is_direct() => {
                Err("leave out the thread to send a direct message".into())
            }
            Some(thread) => Ok(thread.clone()),
            None => Ok(ThreadName::direct(
                from,
                &self.direct_target(from, to, now)?,
            )),
        }
    }

    /// Each live session other than `from` that `to` selects, and each
    /// selector that matched no session.
    fn selected(
        &self,
        from: &Who,
        to: &[Selector],
        now: Instant,
    ) -> (BTreeSet<Who>, Vec<Selector>) {
        let mut woken = BTreeSet::new();
        let mut unmatched = Vec::new();
        let sessions = &self.presence.sessions;
        let live = |who: &&Who| *who != from && !self.gone(who, now);
        for selector in to {
            let mut matched: Vec<Who> = sessions
                .keys()
                .filter(live)
                .filter(|who| selector.matches(&self.uri(who, now)))
                .cloned()
                .collect();
            if matched.is_empty() && selector.lead == Some(true) {
                let free = Selector {
                    lead: None,
                    ..selector.clone()
                };
                matched = sessions
                    .keys()
                    .filter(live)
                    .filter(|who| {
                        let uri = self.uri(who, now);
                        uri.claims().is_empty() && free.matches(&uri)
                    })
                    .cloned()
                    .collect();
            }
            if matched.is_empty() {
                unmatched.push(selector.clone());
            }
            woken.extend(matched);
        }
        (woken, unmatched)
    }

    /// The changes that put `message` in `thread` with the next sequence
    /// number, and wake each live session that its selectors match,
    /// except `from`. Each selected session joins the thread. It also
    /// gives each selector that matched no session.
    pub(super) fn put(
        &self,
        from: &Who,
        thread: ThreadName,
        message: Message,
        now: Instant,
    ) -> (Vec<Change>, Vec<Selector>) {
        let (mut woken, unmatched) = self.selected(from, &message.to, now);
        let threads = self.riff.threads();
        let mut changes: Vec<Change> = woken
            .iter()
            .filter(|who| !threads.member(who, &thread))
            .map(|who| {
                Change::JoinedThread(Member {
                    session: self.plain(who),
                    thread: thread.clone(),
                })
            })
            .collect();
        if message.kind == Kind::Note {
            woken.clear();
        }
        let message = Message {
            seq: threads.last_seq(&thread) + 1,
            ..message
        };
        changes.push(Change::Posted(Box::new(Posted {
            thread,
            message,
            woken,
        })));
        (changes, unmatched)
    }
}

/// The message of the records of a post or of an announce: the last
/// `posted` record.
fn message_of(made: &[Record]) -> Option<&Posted> {
    made.iter().rev().find_map(|record| match &record.change {
        Change::Posted(posted) => Some(&**posted),
        _ => None,
    })
}

/// Adds a session to a thread. It makes the thread if it is new.
impl Command for Join {
    const KIND: CommandKind = CommandKind::Join;
    type Reply = ();
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let who = caller.who();
        let thread = &self.thread;
        let mut changes = Vec::new();
        if !view.riff.threads().member(who, thread) {
            changes.push(Change::JoinedThread(Member {
                session: view.plain(who),
                thread: thread.clone(),
            }));
        }
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &Done, (): (), _: Now) {}
}

/// Removes a session from a thread. It is no longer the lead there.
impl Command for Leave {
    const KIND: CommandKind = CommandKind::Leave;
    type Reply = ();
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let who = caller.who();
        let thread = &self.thread;
        let key = (who.user().to_owned(), thread.clone());
        let mut changes = Vec::new();
        if view.riff.threads().member(who, thread) || view.riff.work().leads.get(&key) == Some(who)
        {
            changes.push(Change::LeftThread(Member {
                session: view.plain(who),
                thread: thread.clone(),
            }));
        }
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &Done, (): (), _: Now) {}
}

/// A post of a session. Its `at_ms` is the time of the message. The
/// note has each selector that matched no session (R61). The reply
/// names each session that the message woke.
///
/// The signature covers the lead mark of `me`. So the sender of a
/// signed message has `lead=true` only when `me` has it, and a signed
/// post with the lead mark from a session that is not the lead is
/// refused (R198). A signed post with the payload of a message in the
/// thread is a copy, and is refused (01M3JEJVXXEPPNGT3FY4ZSFCWZ).
impl Command for Post {
    const KIND: CommandKind = CommandKind::Post;
    type Reply = wire::Posted;
    type Note = Vec<Selector>;

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, Vec<Selector>), Refused> {
        let post = self;
        let me = &post.me;
        let now = now.at;
        let from = caller.who();
        // Only the read of the log takes a value of a later build
        // (01M3XSF90E9JYYTC13D9THY4WE).
        if !post.kind.is_post() {
            return Err("no such kind: use message, status or note".into());
        }
        if let Some(later) = post.to.iter().find(|selector| selector.is_other()) {
            return Err(format!(
                "the selector {later} has a field that this server does not know. \
                 Use user, session, host, repo, worktree, claim or lead."
            )
            .into());
        }
        let signed = post.sig.is_some();
        if signed && me.lead() && !view.is_lead(from, now) {
            return Err(
                "the post has the lead mark, but this session is not the lead. Post again.".into(),
            );
        }
        let thread = view.thread_of(from, post.thread.as_ref(), &post.to, now)?;
        if let Some(payload) = &post.payload
            && let Some(seq) = view
                .riff
                .threads()
                .copies
                .get(&(thread.clone(), payload_hash(payload)))
        {
            return Err(format!(
                "the post is a copy of message {seq}: each signed message comes once"
            )
            .into());
        }
        let mut sender = view.uri(from, now);
        if signed {
            sender = sender.with_lead(me.lead());
        }
        let mut changes = Vec::new();
        if !view.riff.threads().member(from, &thread) {
            changes.push(Change::JoinedThread(Member {
                session: view.plain(from),
                thread: thread.clone(),
            }));
        }
        let message = Message {
            seq: 0,
            from: sender,
            to: post.to.clone(),
            body: post.body.clone(),
            at_ms: post.at_ms.unwrap_or(0),
            kind: post.kind,
            sig: post.sig.clone(),
            payload: post.payload.clone(),
        };
        let (mut put, unmatched) = view.put(from, thread, message, now);
        changes.append(&mut put);
        Ok((changes, unmatched))
    }

    fn reply(
        &self,
        _: &Caller,
        view: &View<'_>,
        done: &Done,
        unmatched: Vec<Selector>,
        now: Now,
    ) -> wire::Posted {
        let posted = message_of(&done.made).expect("the records of a post end with the message");
        wire::Posted {
            thread: posted.thread.clone(),
            seq: posted.message.seq,
            woken: posted
                .woken
                .iter()
                .map(|who| view.uri(who, now.at))
                .collect(),
            unmatched,
        }
    }
}

/// A post of the riff server itself (01M3N7K4BC1RPZKQ1XNDTBRPGF). The
/// sender is the URI of the server. The post has no signature. The
/// server is not a session: it joins no thread, and does not show in
/// `who`. With no thread, the post is a direct message. The reply has
/// each session that the message woke.
#[derive(Clone, Debug)]
pub struct Announce {
    pub thread: Option<ThreadName>,
    pub to: Vec<Selector>,
    pub body: String,
    pub kind: Kind,
    /// The time of the message.
    pub at_ms: u64,
}

impl Command for Announce {
    const KIND: CommandKind = CommandKind::Announce;
    type Reply = Vec<Who>;
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let from = caller.who();
        let thread = view.thread_of(from, self.thread.as_ref(), &self.to, now.at)?;
        let message = Message {
            seq: 0,
            from: caller.me().clone(),
            to: self.to.clone(),
            body: self.body.clone(),
            at_ms: self.at_ms,
            kind: self.kind,
            sig: None,
            payload: None,
        };
        let (changes, _) = view.put(from, thread, message, now.at);
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, done: &Done, (): (), _: Now) -> Vec<Who> {
        message_of(&done.made)
            .map(|posted| posted.woken.iter().cloned().collect())
            .unwrap_or_default()
    }
}
