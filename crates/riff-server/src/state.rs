//! The in-memory state of `riff-server`.
//!
//! # Model
//!
//! | Data | Key | Notes |
//! |---|---|---|
//! | Sessions | who | The place, open watch streams, and the last time the session called. |
//! | Threads | thread name | Members, and messages with a sequence number that starts at 1. |
//! | Read cursors | who and thread | The last sequence number that the session read. |
//! | Claims | thread and item | The session that holds the item. |
//!
//! The server keys each session by its [`Who`]: the user and the session
//! ID. It builds the [`SessionUri`] of a session from the who, the place
//! and the claims that the session holds now.
//!
//! # Rules
//!
//! - Each call records the session as seen at `now`. A call from a
//!   session that the server does not know makes it, in the place from
//!   its URI. Only [`State::register`] changes the place of a known
//!   session (R55, R64).
//! - A session joins the thread of its repository when the server makes
//!   it, and each time it registers.
//! - A post joins its sender to the thread. It wakes each other session
//!   that one or more of its selectors match, when it is posted (R51,
//!   R60). Each woken session joins the thread. The delivery lists each
//!   selector that matched no session (R61).
//! - A post with no thread is a direct message (R62). It needs one
//!   selector with a session ID. Other sessions cannot see its thread.
//! - `threads` lists only the threads that the session joined. `read`
//!   takes any thread by name, except a direct thread of others.
//! - `read` returns the messages after the cursor, then moves the cursor
//!   to the end.
//! - When a watch starts, [`State::missed`] gives one wake for the
//!   newest unread message that woke the session (R49).
//! - A claim is free, or held. A held claim goes back to free when its
//!   holder releases it, or when the holder has no watch stream and was
//!   last seen more than [`CLAIM_GRACE`] ago. A claim of a free item
//!   succeeds.
//!
//! The state does no I/O and reads no clock. The caller passes `now`.
//!
//! # Example
//!
//! ```
//! use std::time::Instant;
//! use riff_core::name::SessionUri;
//! use riff_server::state::State;
//!
//! let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf#api".parse()?;
//! let brett: SessionUri = "riff://brett@heron/como-technologies/riff?session=77e0#tests".parse()?;
//! let now = Instant::now();
//! let mut state = State::default();
//! state.register(&mike, now);
//! state.register(&brett, now);
//! let thread = mike.default_thread();
//!
//! // A post without an address wakes nobody, whatever its text.
//! let quiet = state.post(&mike, thread.clone(), vec![], "@brett ready".into(), now, 0).unwrap();
//! assert!(quiet.wakes.is_empty());
//!
//! // A post to brett's user wakes brett.
//! let to = vec!["user=brett".parse()?];
//! let delivery = state.post(&mike, thread.clone(), to, "ready".into(), now, 0).unwrap();
//! assert_eq!(&delivery.wakes[0].0, brett.who());
//!
//! // Brett reads both messages once.
//! let thread = thread.unwrap();
//! assert_eq!(state.read(&brett, &thread, false, now).unwrap().len(), 2);
//! assert!(state.read(&brett, &thread, false, now).unwrap().is_empty());
//!
//! // The first claim wins.
//! assert!(state.claim(&mike, &thread, "issue-12", now).unwrap().granted);
//! assert!(!state.claim(&brett, &thread, "issue-12", now).unwrap().granted);
//! # Ok::<(), riff_core::name::NameError>(())
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant};

use riff_core::name::{Place, SessionUri, ThreadName, Who, check};
use riff_core::selector::Selector;
use riff_core::wire::{ClaimReply, Message, SessionInfo, Tailed, ThreadInfo, Wake};

/// A claim stays with a session this long after the session stops (R9).
pub const CLAIM_GRACE: Duration = Duration::from_secs(5 * 60);

/// All state of one `riff-server`. See the module docs for the rules.
#[derive(Default)]
pub struct State {
    sessions: BTreeMap<Who, Session>,
    threads: BTreeMap<ThreadName, Thread>,
    /// The last sequence number that each session read in each thread.
    cursors: HashMap<(Who, ThreadName), u64>,
    claims: BTreeMap<(ThreadName, String), Who>,
}

struct Session {
    place: Place,
    /// The number of open watch streams.
    watchers: usize,
    last_seen: Instant,
}

#[derive(Default)]
struct Thread {
    members: BTreeSet<Who>,
    messages: Vec<Stored>,
}

struct Stored {
    message: Message,
    /// Each session that the message woke.
    woken: BTreeSet<Who>,
}

/// What a new message causes: sessions to wake and a line for `tail`.
pub struct Delivery {
    /// Each session to wake, with its event.
    pub wakes: Vec<(Who, Wake)>,
    /// The URI of each woken session, for the sender.
    pub woken: Vec<SessionUri>,
    /// Each selector that matched no session.
    pub unmatched: Vec<Selector>,
    /// The event for the `tail` streams of the thread.
    pub tailed: Tailed,
}

impl State {
    /// Records a session and its place now, and joins it to the thread
    /// of its repository. A session registers when it starts and when it
    /// moves.
    pub fn register(&mut self, me: &SessionUri, now: Instant) {
        self.arrive(me, now);
        if let Some(session) = self.sessions.get_mut(me.who()) {
            session.place = me.place().clone();
        }
        if let Some(thread) = me.default_thread() {
            self.join(me, &thread, now);
        }
    }

    /// Records that a watch stream opened. The session is live.
    pub fn watch_started(&mut self, me: &SessionUri, now: Instant) {
        let who = self.arrive(me, now);
        if let Some(session) = self.sessions.get_mut(&who) {
            session.watchers += 1;
        }
    }

    /// Records that a watch stream closed. The session is idle when it
    /// has no open stream.
    pub fn watch_ended(&mut self, who: &Who, now: Instant) {
        if let Some(session) = self.sessions.get_mut(who) {
            session.watchers = session.watchers.saturating_sub(1);
            session.last_seen = now;
        }
    }

    /// Each known session with its URI now, and whether it is live.
    pub fn who(&self) -> Vec<SessionInfo> {
        self.sessions
            .iter()
            .map(|(who, session)| SessionInfo {
                uri: self.uri(who),
                live: session.watchers > 0,
            })
            .collect()
    }

    /// The URI of a session now: its place and the claims that it holds.
    ///
    /// # Panics
    ///
    /// When the server does not know the session.
    pub fn uri(&self, who: &Who) -> SessionUri {
        let place = self.sessions[who].place.clone();
        let claims = self
            .claims
            .iter()
            .filter(|(_, holder)| *holder == who)
            .map(|((_, item), _)| item.clone())
            .collect();
        SessionUri::new(who.clone(), place).with_claims(claims)
    }

    /// The threads that `me` joined, with its unread counts.
    pub fn threads(&mut self, me: &SessionUri, now: Instant) -> Vec<ThreadInfo> {
        let who = self.arrive(me, now);
        let who = &who;
        self.threads
            .iter()
            .filter(|(_, t)| t.members.contains(who))
            .map(|(thread, t)| {
                let read = self.cursor(who, thread);
                ThreadInfo {
                    thread: thread.clone(),
                    members: t.members.iter().map(|m| self.uri(m)).collect(),
                    unread: t.messages.iter().filter(|m| m.message.seq > read).count(),
                }
            })
            .collect()
    }

    /// The wake for the newest unread message that woke `who`, if there
    /// is one. A watch sends it when it starts.
    pub fn missed(&self, who: &Who) -> Option<Wake> {
        self.threads
            .iter()
            .filter_map(|(thread, t)| {
                let read = self.cursor(who, thread);
                t.messages
                    .iter()
                    .rev()
                    .take_while(|m| m.message.seq > read)
                    .find(|m| m.woken.contains(who))
                    .map(|m| (m.message.at_ms, wake(thread, &m.message)))
            })
            .max_by_key(|(at_ms, _)| *at_ms)
            .map(|(_, wake)| wake)
    }

    /// Adds a session to a thread. It makes the thread if it is new.
    pub fn join(&mut self, me: &SessionUri, thread: &ThreadName, now: Instant) {
        let who = self.arrive(me, now);
        self.member(&who, thread);
    }

    /// Removes a session from a thread.
    pub fn leave(&mut self, me: &SessionUri, thread: &ThreadName, now: Instant) {
        self.arrive(me, now);
        if let Some(t) = self.threads.get_mut(thread) {
            t.members.remove(me.who());
        }
    }

    /// Adds a message to a thread and wakes each session that `to`
    /// selects (R51). With no thread, the post is a direct message (R62).
    pub fn post(
        &mut self,
        me: &SessionUri,
        thread: Option<ThreadName>,
        to: Vec<Selector>,
        body: String,
        now: Instant,
        at_ms: u64,
    ) -> Result<Delivery, String> {
        let from = self.arrive(me, now);
        if to.iter().any(Selector::is_empty) {
            return Err("a selector needs one or more fields".into());
        }
        let thread = match thread {
            Some(thread) if thread.is_direct() => {
                return Err("leave out the thread to send a direct message".into());
            }
            Some(thread) => thread,
            None => ThreadName::direct(&from, &self.direct_target(&from, &to)?),
        };
        self.member(&from, &thread);
        let mut woken = BTreeSet::new();
        let mut unmatched = Vec::new();
        for selector in &to {
            let matched: Vec<Who> = self
                .sessions
                .keys()
                .filter(|who| **who != from && selector.matches(&self.uri(who)))
                .cloned()
                .collect();
            if matched.is_empty() {
                unmatched.push(selector.clone());
            }
            woken.extend(matched);
        }
        for who in &woken {
            self.member(who, &thread);
        }
        let sender = self.uri(&from);
        let t = self.threads.entry(thread.clone()).or_default();
        let message = Message {
            seq: t.messages.last().map_or(1, |m| m.message.seq + 1),
            from: sender,
            to,
            body,
            at_ms,
        };
        t.messages.push(Stored {
            message: message.clone(),
            woken: woken.clone(),
        });
        let wakes = woken
            .iter()
            .map(|who| (who.clone(), wake(&thread, &message)))
            .collect();
        Ok(Delivery {
            wakes,
            woken: woken.iter().map(|who| self.uri(who)).collect(),
            unmatched,
            tailed: Tailed { thread, message },
        })
    }

    /// Returns unread messages (or all of them) and marks them as read.
    pub fn read(
        &mut self,
        me: &SessionUri,
        thread: &ThreadName,
        all: bool,
        now: Instant,
    ) -> Result<Vec<Message>, String> {
        let who = self.arrive(me, now);
        let t = self
            .threads
            .get(thread)
            .ok_or_else(|| format!("no thread named {thread}"))?;
        if thread.is_direct() && !t.members.contains(&who) {
            return Err(format!("no thread named {thread}"));
        }
        let from = if all { 0 } else { self.cursor(&who, thread) };
        let messages: Vec<Message> = t
            .messages
            .iter()
            .filter(|m| m.message.seq > from)
            .map(|m| m.message.clone())
            .collect();
        if let Some(last) = t.messages.last() {
            self.cursors.insert((who, thread.clone()), last.message.seq);
        }
        Ok(messages)
    }

    /// Takes a claim if nobody holds it, or if its holder stopped more than
    /// [`CLAIM_GRACE`] ago.
    pub fn claim(
        &mut self,
        me: &SessionUri,
        thread: &ThreadName,
        item: &str,
        now: Instant,
    ) -> Result<ClaimReply, String> {
        check("claim", item).map_err(|e| e.to_string())?;
        let who = self.arrive(me, now);
        let key = (thread.clone(), item.to_owned());
        if let Some(holder) = self.claims.get(&key)
            && *holder != who
            && self.holds(holder, now)
        {
            return Ok(ClaimReply {
                granted: false,
                holder: self.uri(holder),
            });
        }
        self.claims.insert(key, who.clone());
        Ok(ClaimReply {
            granted: true,
            holder: self.uri(&who),
        })
    }

    /// Frees a claim. Only its holder can.
    pub fn release(
        &mut self,
        me: &SessionUri,
        thread: &ThreadName,
        item: &str,
        now: Instant,
    ) -> Result<(), String> {
        let who = self.arrive(me, now);
        let key = (thread.clone(), item.to_owned());
        match self.claims.get(&key) {
            Some(holder) if *holder == who => {
                self.claims.remove(&key);
                Ok(())
            }
            Some(holder) => Err(format!("{item} is held by {}", self.uri(holder).short())),
            None => Err(format!("nobody holds {item}")),
        }
    }

    /// Finds the one session that a direct message goes to.
    fn direct_target(&self, from: &Who, to: &[Selector]) -> Result<Who, String> {
        let [selector] = to else {
            return Err("a direct message needs exactly one selector".into());
        };
        if selector.session.is_none() {
            return Err("a direct message needs a selector with a session".into());
        }
        self.sessions
            .keys()
            .find(|who| *who != from && selector.matches(&self.uri(who)))
            .cloned()
            .ok_or_else(|| format!("no session matches {selector}. Use who to list the sessions."))
    }

    fn holds(&self, holder: &Who, now: Instant) -> bool {
        self.sessions.get(holder).is_some_and(|s| {
            s.watchers > 0 || now.saturating_duration_since(s.last_seen) < CLAIM_GRACE
        })
    }

    /// Records that a session called. A new session starts in the place
    /// from its URI and joins the thread of its repository.
    fn arrive(&mut self, me: &SessionUri, now: Instant) -> Who {
        let who = me.who().clone();
        if let Some(session) = self.sessions.get_mut(&who) {
            session.last_seen = now;
            return who;
        }
        self.sessions.insert(
            who.clone(),
            Session {
                place: me.place().clone(),
                watchers: 0,
                last_seen: now,
            },
        );
        if let Some(thread) = me.default_thread() {
            self.member(&who, &thread);
        }
        who
    }

    fn member(&mut self, who: &Who, thread: &ThreadName) {
        self.threads
            .entry(thread.clone())
            .or_default()
            .members
            .insert(who.clone());
    }

    fn cursor(&self, who: &Who, thread: &ThreadName) -> u64 {
        self.cursors
            .get(&(who.clone(), thread.clone()))
            .copied()
            .unwrap_or(0)
    }
}

fn wake(thread: &ThreadName, message: &Message) -> Wake {
    Wake {
        thread: thread.clone(),
        seq: message.seq,
        from: message.from.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(text: &str) -> SessionUri {
        text.parse().unwrap()
    }

    fn thread(text: &str) -> ThreadName {
        text.parse().unwrap()
    }

    fn to(selectors: &[&str]) -> Vec<Selector> {
        selectors.iter().map(|s| s.parse().unwrap()).collect()
    }

    fn api() -> SessionUri {
        uri("riff://mike@pangolin/como-technologies/riff?session=a1#api")
    }

    fn tests() -> SessionUri {
        uri("riff://brett@heron/como-technologies/riff?session=b2#tests")
    }

    fn docs() -> SessionUri {
        uri("riff://mike@pangolin/como-technologies/riff?session=c3#docs")
    }

    fn repo() -> ThreadName {
        thread("como-technologies/riff")
    }

    fn setup(now: Instant) -> State {
        let mut state = State::default();
        for n in [api(), tests(), docs()] {
            state.register(&n, now);
        }
        state
    }

    fn woken(delivery: &Delivery) -> Vec<Who> {
        delivery.wakes.iter().map(|(w, _)| w.clone()).collect()
    }

    fn post(state: &mut State, me: &SessionUri, t: &str, sel: &[&str], body: &str) -> Delivery {
        state
            .post(me, Some(thread(t)), to(sel), body.into(), Instant::now(), 0)
            .unwrap()
    }

    #[test]
    fn register_joins_the_repository_thread() {
        let now = Instant::now();
        let mut state = setup(now);
        let threads = state.threads(&api(), now);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].thread, repo());
        assert_eq!(threads[0].members.len(), 3);
    }

    #[test]
    fn two_sessions_in_one_place_are_two_sessions() {
        let now = Instant::now();
        let mut state = State::default();
        let a = uri("riff://mike@pangolin/como-technologies/riff?session=a");
        let b = uri("riff://mike@pangolin/como-technologies/riff?session=b");
        state.register(&a, now);
        state.register(&b, now);
        assert_eq!(state.who().len(), 2);
        let d = post(&mut state, &a, "x", &["session=b"], "hi");
        assert_eq!(woken(&d), vec![b.who().clone()]);
    }

    #[test]
    fn body_text_never_wakes() {
        let now = Instant::now();
        let mut state = setup(now);
        let d = post(&mut state, &api(), "x", &[], "@brett@heron:riff#tests look");
        assert!(d.wakes.is_empty());
    }

    #[test]
    fn a_selector_wakes_each_match_but_not_the_sender() {
        let now = Instant::now();
        let mut state = setup(now);
        let d = post(&mut state, &api(), "x", &["user=mike"], "hi");
        assert_eq!(woken(&d), vec![docs().who().clone()]);
        let d = post(
            &mut state,
            &api(),
            "x",
            &["repo=como-technologies/riff"],
            "all",
        );
        assert_eq!(d.wakes.len(), 2);
        let d = post(
            &mut state,
            &api(),
            "x",
            &["user=brett", "worktree=docs"],
            "two",
        );
        assert_eq!(d.wakes.len(), 2);
    }

    #[test]
    fn a_woken_session_joins_the_thread() {
        let now = Instant::now();
        let mut state = setup(now);
        post(&mut state, &api(), "design", &["user=brett"], "look");
        let threads = state.threads(&tests(), now);
        let design = threads
            .iter()
            .find(|t| t.thread == thread("design"))
            .unwrap();
        assert_eq!(design.unread, 1);
    }

    #[test]
    fn a_claim_selector_finds_the_holder() {
        let now = Instant::now();
        let mut state = setup(now);
        state.claim(&tests(), &repo(), "issue-6", now).unwrap();
        let d = post(&mut state, &api(), "x", &["claim=issue-6"], "status?");
        assert_eq!(woken(&d), vec![tests().who().clone()]);
        assert_eq!(state.uri(tests().who()).claims(), ["issue-6"]);
    }

    #[test]
    fn a_selector_that_matches_nobody_is_reported() {
        let now = Instant::now();
        let mut state = setup(now);
        let d = post(&mut state, &api(), "x", &["user=ghost", "user=brett"], "hi");
        assert_eq!(d.unmatched, to(&["user=ghost"]));
        assert_eq!(d.woken, vec![tests()]);
    }

    #[test]
    fn a_post_with_no_address_field_is_refused() {
        let now = Instant::now();
        let mut state = setup(now);
        let result = state.post(&api(), None, vec![Selector::default()], "x".into(), now, 0);
        assert!(result.is_err());
    }

    #[test]
    fn a_move_changes_the_place_but_not_the_session() {
        let now = Instant::now();
        let mut state = setup(now);
        state.claim(&api(), &repo(), "issue-6", now).unwrap();
        let moved = uri("riff://mike@pangolin/como-technologies/riff?session=a1#issue-6");
        state.register(&moved, now);
        assert_eq!(state.who().len(), 3);
        let now_uri = state.uri(api().who());
        assert_eq!(now_uri.place().worktree(), Some("issue-6"));
        assert_eq!(now_uri.claims(), ["issue-6"]);
        let d = post(&mut state, &tests(), "x", &["worktree=issue-6"], "hi");
        assert_eq!(woken(&d), vec![api().who().clone()]);
    }

    #[test]
    fn a_call_does_not_move_a_known_session() {
        let now = Instant::now();
        let mut state = setup(now);
        let stale = uri("riff://mike@pangolin/como-technologies/riff?session=a1#old");
        state.watch_started(&stale, now);
        assert_eq!(state.uri(api().who()).place().worktree(), Some("api"));
    }

    #[test]
    fn a_direct_message_wakes_the_receiver_and_hides_the_thread() {
        let now = Instant::now();
        let mut state = setup(now);
        let d = state
            .post(&api(), None, to(&["session=b2"]), "hi".into(), now, 0)
            .unwrap();
        assert_eq!(woken(&d), vec![tests().who().clone()]);
        let dm = d.tailed.thread;
        assert!(dm.is_direct());
        assert!(state.read(&docs(), &dm, true, now).is_err());
        assert_eq!(state.read(&tests(), &dm, false, now).unwrap().len(), 1);
        assert!(!state.threads(&docs(), now).iter().any(|t| t.thread == dm));
    }

    #[test]
    fn a_direct_message_needs_one_session() {
        let now = Instant::now();
        let mut state = setup(now);
        for sel in [
            &["user=brett"][..],
            &["session=b2", "session=c3"],
            &["session=zz"],
        ] {
            assert!(
                state
                    .post(&api(), None, to(sel), "x".into(), now, 0)
                    .is_err(),
                "{sel:?}"
            );
        }
    }

    #[test]
    fn read_returns_only_unread_messages() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = repo();
        post(&mut state, &api(), "como-technologies/riff", &[], "one");
        assert_eq!(state.read(&tests(), &t, false, now).unwrap().len(), 1);
        post(&mut state, &api(), "como-technologies/riff", &[], "two");
        let unread = state.read(&tests(), &t, false, now).unwrap();
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].body, "two");
        assert_eq!(state.read(&tests(), &t, true, now).unwrap().len(), 2);
        assert!(state.read(&tests(), &t, false, now).unwrap().is_empty());
    }

    #[test]
    fn threads_lists_only_joined_threads() {
        let now = Instant::now();
        let mut state = setup(now);
        post(&mut state, &api(), "design", &[], "a plan");
        let has = |state: &mut State, me: &SessionUri| {
            state
                .threads(me, now)
                .iter()
                .any(|t| t.thread == thread("design"))
        };
        assert!(has(&mut state, &api()));
        assert!(!has(&mut state, &tests()));
        // A session can still read any thread by name (R25).
        assert_eq!(
            state
                .read(&tests(), &thread("design"), false, now)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn missed_gives_the_newest_unread_message_that_woke_the_session() {
        let now = Instant::now();
        let mut state = setup(now);
        let b = tests().who().clone();
        assert!(state.missed(&b).is_none());
        post(&mut state, &api(), "design", &[], "no address");
        assert!(state.missed(&b).is_none());
        state
            .post(
                &api(),
                Some(thread("design")),
                to(&["user=brett"]),
                "look".into(),
                now,
                2,
            )
            .unwrap();
        assert_eq!(state.missed(&b).unwrap().seq, 2);
        let dm = state
            .post(&docs(), None, to(&["session=b2"]), "hi".into(), now, 3)
            .unwrap()
            .tailed
            .thread;
        assert_eq!(state.missed(&b).unwrap().thread, dm);
        // The sender does not miss its own message.
        assert!(state.missed(docs().who()).is_none());
        state.read(&tests(), &dm, false, now).unwrap();
        assert_eq!(state.missed(&b).unwrap().thread, thread("design"));
        state.read(&tests(), &thread("design"), false, now).unwrap();
        assert!(state.missed(&b).is_none());
    }

    #[test]
    fn a_claim_blocks_others_while_its_holder_is_live() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = repo();
        state.watch_started(&api(), now);
        assert!(state.claim(&api(), &t, "issue-12", now).unwrap().granted);
        let later = now + CLAIM_GRACE * 2;
        let reply = state.claim(&tests(), &t, "issue-12", later).unwrap();
        assert!(!reply.granted);
        assert_eq!(reply.holder.who(), api().who());
    }

    #[test]
    fn a_claim_survives_a_short_gap_and_ends_after_the_grace_period() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = repo();
        state.watch_started(&api(), now);
        assert!(state.claim(&api(), &t, "issue-12", now).unwrap().granted);
        state.watch_ended(api().who(), now);
        let soon = now + Duration::from_secs(60);
        assert!(!state.claim(&tests(), &t, "issue-12", soon).unwrap().granted);
        let late = now + CLAIM_GRACE + Duration::from_secs(1);
        assert!(state.claim(&tests(), &t, "issue-12", late).unwrap().granted);
    }

    #[test]
    fn only_the_holder_releases_a_claim() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = repo();
        state.claim(&api(), &t, "issue-12", now).unwrap();
        assert!(state.release(&tests(), &t, "issue-12", now).is_err());
        assert!(state.release(&api(), &t, "issue-12", now).is_ok());
        assert!(state.claim(&tests(), &t, "issue-12", now).unwrap().granted);
    }

    #[test]
    fn a_claim_item_must_fit_in_a_uri() {
        let now = Instant::now();
        let mut state = setup(now);
        assert!(state.claim(&api(), &repo(), "issue 12", now).is_err());
    }

    #[test]
    fn who_shows_live_sessions() {
        let now = Instant::now();
        let mut state = setup(now);
        state.watch_started(&tests(), now);
        let live: Vec<_> = state.who().into_iter().filter(|s| s.live).collect();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].uri, tests());
    }
}
