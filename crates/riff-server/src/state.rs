//! The in-memory state of `riff-server`.
//!
//! # Model
//!
//! | Data | Key | Notes |
//! |---|---|---|
//! | Presence | session | Open watch streams and the last time the session called. |
//! | Threads | thread name | Members, and messages with a sequence number that starts at 1. |
//! | Read cursors | session and thread | The last sequence number that the session read. |
//! | Claims | thread and item | The session that holds the item. |
//!
//! # Rules
//!
//! - Each call records the session as seen at `now`.
//! - A post joins its sender to the thread. It wakes each known session
//!   that it mentions, except the sender.
//! - A direct message joins both sessions to their direct thread and
//!   always wakes the receiver. Other sessions cannot see that thread.
//! - `threads` lists only the threads that the session joined. `read`
//!   takes any thread by name, except a direct thread of others.
//! - `read` returns the messages after the cursor, then moves the cursor
//!   to the end.
//! - When a watch starts, [`State::missed`] gives one wake for the
//!   newest unread direct message or mention, so that the session
//!   learns about messages that came while it had no watch.
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
//! use riff_core::name::SessionName;
//! use riff_core::wire::WakeReason;
//! use riff_server::state::State;
//!
//! let mike: SessionName = "riff://mike@pangolin/como-technologies/riff#api".parse()?;
//! let brett: SessionName = "riff://brett@heron/como-technologies/riff#tests".parse()?;
//! let now = Instant::now();
//! let mut state = State::default();
//! state.register(&mike, now);
//! state.register(&brett, now);
//! let thread = mike.default_thread().unwrap();
//!
//! // A post without a mention wakes nobody.
//! assert!(state.post(&mike, &thread, "working".into(), now, 0).wakes.is_empty());
//!
//! // A mention wakes the named session.
//! let delivery = state.post(&mike, &thread, "@brett@heron:riff#tests ready".into(), now, 0);
//! assert_eq!(delivery.wakes[0].0, brett);
//! assert_eq!(delivery.wakes[0].1.reason, WakeReason::Mention);
//!
//! // Brett reads both messages once.
//! assert_eq!(state.read(&brett, &thread, false, now).unwrap().len(), 2);
//! assert!(state.read(&brett, &thread, false, now).unwrap().is_empty());
//!
//! // The first claim wins.
//! assert!(state.claim(&mike, &thread, "issue-12", now).granted);
//! assert!(!state.claim(&brett, &thread, "issue-12", now).granted);
//! # Ok::<(), riff_core::name::NameError>(())
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant};

use riff_core::name::{SessionName, ThreadName};
use riff_core::wire::{ClaimReply, Message, SessionInfo, Tailed, ThreadInfo, Wake, WakeReason};

/// A claim stays with a session this long after the session stops (R9).
pub const CLAIM_GRACE: Duration = Duration::from_secs(5 * 60);

/// All state of one `riff-server`. See the module docs for the rules.
#[derive(Default)]
pub struct State {
    sessions: BTreeMap<SessionName, Presence>,
    threads: BTreeMap<ThreadName, Thread>,
    /// The last sequence number that each session read in each thread.
    cursors: HashMap<(SessionName, ThreadName), u64>,
    claims: HashMap<(ThreadName, String), SessionName>,
}

struct Presence {
    /// The number of open watch streams.
    watchers: usize,
    last_seen: Instant,
}

#[derive(Default)]
struct Thread {
    members: BTreeSet<SessionName>,
    messages: Vec<Message>,
}

/// What a new message causes: sessions to wake and a line for `tail`.
pub struct Delivery {
    /// Each session to wake, with its event.
    pub wakes: Vec<(SessionName, Wake)>,
    /// The event for the `tail` streams of the thread.
    pub tailed: Tailed,
}

impl State {
    /// Records that a session exists and joins it to its default thread.
    pub fn register(&mut self, name: &SessionName, now: Instant) {
        self.touch(name, now);
        if let Some(thread) = name.default_thread() {
            self.join(name, &thread, now);
        }
    }

    /// Records that a watch stream opened. The session is live.
    pub fn watch_started(&mut self, name: &SessionName, now: Instant) {
        self.touch(name, now).watchers += 1;
    }

    /// Records that a watch stream closed. The session is idle when it
    /// has no open stream.
    pub fn watch_ended(&mut self, name: &SessionName, now: Instant) {
        let presence = self.touch(name, now);
        presence.watchers = presence.watchers.saturating_sub(1);
    }

    /// Each known session, and whether it is live.
    pub fn who(&self) -> Vec<SessionInfo> {
        self.sessions
            .iter()
            .map(|(name, presence)| SessionInfo {
                name: name.clone(),
                live: presence.watchers > 0,
            })
            .collect()
    }

    /// The threads that `name` joined, with its unread counts.
    pub fn threads(&self, name: &SessionName) -> Vec<ThreadInfo> {
        self.threads
            .iter()
            .filter(|(_, t)| t.members.contains(name))
            .map(|(thread, t)| {
                let read = self.cursor(name, thread);
                ThreadInfo {
                    thread: thread.clone(),
                    members: t.members.iter().cloned().collect(),
                    unread: t.messages.iter().filter(|m| m.seq > read).count(),
                }
            })
            .collect()
    }

    /// The wake for the newest unread direct message or mention of
    /// `name`, if there is one. A watch sends it when it starts.
    pub fn missed(&self, name: &SessionName) -> Option<Wake> {
        self.threads
            .iter()
            .filter_map(|(thread, t)| {
                let read = self.cursor(name, thread);
                t.messages
                    .iter()
                    .rev()
                    .take_while(|m| m.seq > read)
                    .filter(|m| &m.from != name)
                    .find_map(|m| {
                        let reason = if thread.is_direct() {
                            t.members.contains(name).then_some(WakeReason::Direct)
                        } else {
                            self.mentioned(&m.body)
                                .contains(name)
                                .then_some(WakeReason::Mention)
                        };
                        reason.map(|r| (m.at_ms, wake(thread, m, r)))
                    })
            })
            .max_by_key(|(at_ms, _)| *at_ms)
            .map(|(_, wake)| wake)
    }

    /// Adds a session to a thread. It makes the thread if it is new.
    pub fn join(&mut self, name: &SessionName, thread: &ThreadName, now: Instant) {
        self.touch(name, now);
        self.threads
            .entry(thread.clone())
            .or_default()
            .members
            .insert(name.clone());
    }

    /// Removes a session from a thread.
    pub fn leave(&mut self, name: &SessionName, thread: &ThreadName, now: Instant) {
        self.touch(name, now);
        if let Some(t) = self.threads.get_mut(thread) {
            t.members.remove(name);
        }
    }

    /// Adds a message to a thread. The sender joins the thread. A mention
    /// wakes the named session (R26).
    pub fn post(
        &mut self,
        from: &SessionName,
        thread: &ThreadName,
        body: String,
        now: Instant,
        at_ms: u64,
    ) -> Delivery {
        self.join(from, thread, now);
        let message = self.append(from, thread, body, at_ms);
        let wakes = self
            .mentioned(&message.body)
            .into_iter()
            .filter(|name| name != from)
            .map(|name| {
                let wake = wake(thread, &message, WakeReason::Mention);
                (name, wake)
            })
            .collect();
        Delivery {
            wakes,
            tailed: tailed(thread, message),
        }
    }

    /// Sends a direct message. It always wakes the receiver (R26).
    pub fn tell(
        &mut self,
        from: &SessionName,
        to: &SessionName,
        body: String,
        now: Instant,
        at_ms: u64,
    ) -> Delivery {
        let thread = ThreadName::direct(from, to);
        self.join(from, &thread, now);
        self.join(to, &thread, now);
        let message = self.append(from, &thread, body, at_ms);
        let wakes = vec![(to.clone(), wake(&thread, &message, WakeReason::Direct))];
        Delivery {
            wakes,
            tailed: tailed(&thread, message),
        }
    }

    /// Returns unread messages (or all of them) and marks them as read.
    pub fn read(
        &mut self,
        name: &SessionName,
        thread: &ThreadName,
        all: bool,
        now: Instant,
    ) -> Result<Vec<Message>, String> {
        self.touch(name, now);
        let t = self
            .threads
            .get(thread)
            .ok_or_else(|| format!("no thread named {thread}"))?;
        if thread.is_direct() && !t.members.contains(name) {
            return Err(format!("no thread named {thread}"));
        }
        let from = if all { 0 } else { self.cursor(name, thread) };
        let messages: Vec<Message> = t
            .messages
            .iter()
            .filter(|m| m.seq > from)
            .cloned()
            .collect();
        if let Some(last) = t.messages.last() {
            self.cursors
                .insert((name.clone(), thread.clone()), last.seq);
        }
        Ok(messages)
    }

    /// Takes a claim if nobody holds it, or if its holder stopped more than
    /// [`CLAIM_GRACE`] ago.
    pub fn claim(
        &mut self,
        name: &SessionName,
        thread: &ThreadName,
        item: &str,
        now: Instant,
    ) -> ClaimReply {
        self.touch(name, now);
        let key = (thread.clone(), item.to_owned());
        if let Some(holder) = self.claims.get(&key)
            && holder != name
            && self.holds(holder, now)
        {
            return ClaimReply {
                granted: false,
                holder: holder.clone(),
            };
        }
        self.claims.insert(key, name.clone());
        ClaimReply {
            granted: true,
            holder: name.clone(),
        }
    }

    /// Frees a claim. Only its holder can.
    pub fn release(
        &mut self,
        name: &SessionName,
        thread: &ThreadName,
        item: &str,
        now: Instant,
    ) -> Result<(), String> {
        self.touch(name, now);
        let key = (thread.clone(), item.to_owned());
        match self.claims.get(&key) {
            Some(holder) if holder == name => {
                self.claims.remove(&key);
                Ok(())
            }
            Some(holder) => Err(format!("{item} is held by {}", holder.short())),
            None => Err(format!("nobody holds {item}")),
        }
    }

    fn holds(&self, holder: &SessionName, now: Instant) -> bool {
        self.sessions.get(holder).is_some_and(|p| {
            p.watchers > 0 || now.saturating_duration_since(p.last_seen) < CLAIM_GRACE
        })
    }

    fn touch(&mut self, name: &SessionName, now: Instant) -> &mut Presence {
        let presence = self.sessions.entry(name.clone()).or_insert(Presence {
            watchers: 0,
            last_seen: now,
        });
        presence.last_seen = now;
        presence
    }

    fn cursor(&self, name: &SessionName, thread: &ThreadName) -> u64 {
        self.cursors
            .get(&(name.clone(), thread.clone()))
            .copied()
            .unwrap_or(0)
    }

    fn append(
        &mut self,
        from: &SessionName,
        thread: &ThreadName,
        body: String,
        at_ms: u64,
    ) -> Message {
        let t = self.threads.entry(thread.clone()).or_default();
        let message = Message {
            seq: t.messages.last().map_or(1, |m| m.seq + 1),
            from: from.clone(),
            body,
            at_ms,
        };
        t.messages.push(message.clone());
        message
    }

    /// The known sessions that a body mentions, in short form or in full
    /// (`@mike@pangolin:riff#api`).
    fn mentioned(&self, body: &str) -> BTreeSet<SessionName> {
        body.split_whitespace()
            .filter_map(|word| word.strip_prefix('@'))
            .map(|word| word.trim_end_matches(['.', ',', ';', ':', '!', '?', ')']))
            .filter_map(|word| {
                self.sessions
                    .keys()
                    .find(|name| name.short() == word || name.to_string() == word)
                    .cloned()
            })
            .collect()
    }
}

fn wake(thread: &ThreadName, message: &Message, reason: WakeReason) -> Wake {
    Wake {
        thread: thread.clone(),
        seq: message.seq,
        from: message.from.clone(),
        reason,
    }
}

fn tailed(thread: &ThreadName, message: Message) -> Tailed {
    Tailed {
        thread: thread.clone(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> SessionName {
        text.parse().unwrap()
    }

    fn thread(text: &str) -> ThreadName {
        text.parse().unwrap()
    }

    fn api() -> SessionName {
        name("riff://mike@pangolin/como-technologies/riff#api")
    }

    fn tests() -> SessionName {
        name("riff://brett@heron/como-technologies/riff#tests")
    }

    fn docs() -> SessionName {
        name("riff://mike@pangolin/como-technologies/riff#docs")
    }

    fn setup(now: Instant) -> State {
        let mut state = State::default();
        for n in [api(), tests(), docs()] {
            state.register(&n, now);
        }
        state
    }

    #[test]
    fn register_joins_the_repository_thread() {
        let now = Instant::now();
        let state = setup(now);
        let threads = state.threads(&api());
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].thread, thread("como-technologies/riff"));
        assert_eq!(threads[0].members.len(), 3);
    }

    #[test]
    fn a_mention_wakes_only_the_named_session() {
        let now = Instant::now();
        let mut state = setup(now);
        let delivery = state.post(
            &api(),
            &thread("como-technologies/riff"),
            "@brett@heron:riff#tests, the API is ready".into(),
            now,
            0,
        );
        let woken: Vec<_> = delivery.wakes.iter().map(|(n, _)| n.clone()).collect();
        assert_eq!(woken, vec![tests()]);
        assert_eq!(delivery.wakes[0].1.reason, WakeReason::Mention);
    }

    #[test]
    fn a_session_does_not_wake_itself() {
        let now = Instant::now();
        let mut state = setup(now);
        let delivery = state.post(
            &api(),
            &thread("x"),
            "@mike@pangolin:riff#api".into(),
            now,
            0,
        );
        assert!(delivery.wakes.is_empty());
    }

    #[test]
    fn tell_wakes_the_receiver_and_hides_the_thread_from_others() {
        let now = Instant::now();
        let mut state = setup(now);
        let delivery = state.tell(&api(), &tests(), "hi".into(), now, 0);
        assert_eq!(delivery.wakes[0].0, tests());
        assert_eq!(delivery.wakes[0].1.reason, WakeReason::Direct);
        let dm = delivery.tailed.thread;
        assert!(state.read(&docs(), &dm, true, now).is_err());
        assert_eq!(state.read(&tests(), &dm, false, now).unwrap().len(), 1);
        assert!(!state.threads(&docs()).iter().any(|t| t.thread == dm));
    }

    #[test]
    fn read_returns_only_unread_messages() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = thread("como-technologies/riff");
        state.post(&api(), &t, "one".into(), now, 0);
        assert_eq!(state.read(&tests(), &t, false, now).unwrap().len(), 1);
        state.post(&api(), &t, "two".into(), now, 0);
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
        state.post(&api(), &thread("design"), "a plan".into(), now, 0);
        assert!(
            state
                .threads(&api())
                .iter()
                .any(|t| t.thread == thread("design"))
        );
        assert!(
            !state
                .threads(&tests())
                .iter()
                .any(|t| t.thread == thread("design"))
        );
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
    fn missed_gives_the_newest_unread_mention_or_direct_message() {
        let now = Instant::now();
        let mut state = setup(now);
        assert!(state.missed(&tests()).is_none());
        state.post(&api(), &thread("design"), "no mention".into(), now, 1);
        assert!(state.missed(&tests()).is_none());
        state.post(
            &api(),
            &thread("design"),
            "@brett@heron:riff#tests look".into(),
            now,
            2,
        );
        let wake = state.missed(&tests()).unwrap();
        assert_eq!((wake.reason, wake.seq), (WakeReason::Mention, 2));
        let dm = state
            .tell(&docs(), &tests(), "hi".into(), now, 3)
            .tailed
            .thread;
        let wake = state.missed(&tests()).unwrap();
        assert_eq!((wake.reason, wake.thread), (WakeReason::Direct, dm.clone()));
        // The sender does not miss its own message.
        assert!(state.missed(&docs()).is_none());
        state.read(&tests(), &dm, false, now).unwrap();
        assert_eq!(state.missed(&tests()).unwrap().reason, WakeReason::Mention);
        state.read(&tests(), &thread("design"), false, now).unwrap();
        assert!(state.missed(&tests()).is_none());
    }

    #[test]
    fn a_claim_blocks_others_while_its_holder_is_live() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = thread("como-technologies/riff");
        state.watch_started(&api(), now);
        assert!(state.claim(&api(), &t, "issue-12", now).granted);
        let later = now + CLAIM_GRACE * 2;
        let reply = state.claim(&tests(), &t, "issue-12", later);
        assert!(!reply.granted);
        assert_eq!(reply.holder, api());
    }

    #[test]
    fn a_claim_survives_a_short_gap_and_ends_after_the_grace_period() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = thread("como-technologies/riff");
        state.watch_started(&api(), now);
        assert!(state.claim(&api(), &t, "issue-12", now).granted);
        state.watch_ended(&api(), now);
        let soon = now + Duration::from_secs(60);
        assert!(!state.claim(&tests(), &t, "issue-12", soon).granted);
        let late = now + CLAIM_GRACE + Duration::from_secs(1);
        assert!(state.claim(&tests(), &t, "issue-12", late).granted);
    }

    #[test]
    fn only_the_holder_releases_a_claim() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = thread("como-technologies/riff");
        state.claim(&api(), &t, "issue-12", now);
        assert!(state.release(&tests(), &t, "issue-12", now).is_err());
        assert!(state.release(&api(), &t, "issue-12", now).is_ok());
        assert!(state.claim(&tests(), &t, "issue-12", now).granted);
    }

    #[test]
    fn who_shows_live_sessions() {
        let now = Instant::now();
        let mut state = setup(now);
        state.watch_started(&tests(), now);
        let live: Vec<_> = state.who().into_iter().filter(|s| s.live).collect();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].name, tests());
    }
}
