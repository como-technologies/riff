//! The in-memory state of `riff-server`.
//!
//! # Model
//!
//! | Data | Key | Notes |
//! |---|---|---|
//! | Sessions | who | The place, open watch streams, the last call, the last sign of life, whether it ended, and its last status. |
//! | Threads | thread name | Members, and messages with a sequence number that starts at 1. |
//! | Read cursors | who and thread | The last sequence number that the session read. |
//! | Claims | thread and item | The session that holds the item. |
//! | Leads | user and repository thread | The lead session of the user. |
//! | Riff state | none: one for the server | Paused or running. |
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
//! - A session keeps the user of its first call. [`State::check_user`]
//!   refuses its session ID under another user (R159).
//! - A session joins the thread of its repository when the server makes
//!   it, and each time it registers.
//! - A post joins its sender to the thread. It wakes each other session
//!   that one or more of its selectors match, when it is posted (R51,
//!   R60). Each woken session joins the thread. The delivery lists each
//!   selector that matched no session (R61).
//! - A post with no thread is a direct message (R62). It needs one
//!   selector with a session ID or `lead=true`, and that selector must
//!   match one session (R179). Other sessions cannot see its thread.
//! - `threads` lists only the threads that the session joined. `read`
//!   takes any thread by name, except a direct thread of others.
//! - `read` returns the messages after the cursor, then moves the cursor
//!   to the end.
//! - When a watch starts, [`State::missed`] gives one wake for the
//!   newest unread message that woke the session (R49).
//! - `who` lists each session with the time since its last call. A
//!   keep-alive is not a call (R163).
//! - A session is gone when it ended ([`State::end`]), or when it had no
//!   call, no keep-alive ([`State::alive`]) and no watch for [`GONE`]
//!   (R164, R206). `who` hides a gone session, unless the caller asks
//!   for all sessions. A gone session matches no selector, and a direct
//!   message to it fails (R206).
//! - An end frees the claims of the session at once. Its lead does not
//!   count while it is gone. A session that stops with no end holds its
//!   claims and its lead for [`CLAIM_GRACE`] after its last sign of
//!   life, also while it is gone (R9, R206).
//! - [`State::start`] is a new start of a session: a new agent process,
//!   a resume or a `/clear`. It frees the claims of the session at once,
//!   and keeps its lead (01M3JEE1QQCFS5TMZW5N2DAD2D).
//! - A call or a keep-alive from a gone session makes it live again, with
//!   the same ID, threads and cursors. After a stop with no end, it gets
//!   back each claim that no other session took. After an end, it has no
//!   claims. A lead is the lead again, unless another session became
//!   the lead (R207).
//! - [`State::set_status`] keeps the last status of a session, with the
//!   time that it was set. `who` shows the status and its age (R182,
//!   R184).
//! - A post has a kind. A post of kind [`Kind::Status`] is a status
//!   request. It wakes as each post does, and its wakes carry the kind
//!   (R185).
//! - A post of kind [`Kind::Note`] wakes no
//!   session. Each session that its selectors match still joins the
//!   thread, so it sees the note at its next `read`
//!   (01M3JPMQE6S7YM4HPEVGXWK7ET).
//! - In a thread, a selector with `lead=true` that matches no live
//!   session matches each live session with no claim that its other
//!   fields match. So a verify request to the lead of the author reaches
//!   a free session when the lead is gone (01M3JY1TBPQHH6WPPBTF42T64H).
//! - A claim is free, or held. A held claim goes back to free when its
//!   holder releases it or ends, or when the holder has no watch stream
//!   and its last sign of life is more than [`CLAIM_GRACE`] ago. A claim
//!   of a free item succeeds.
//! - Each user has at most one lead in each repository thread. Its URI
//!   has `lead=true` (R175).
//! - A session with a session ID becomes the lead when it arrives in a
//!   repository and no other session of its user there holds (R176).
//!   [`State::lead`] makes a session the lead and replaces the old lead
//!   (R177).
//! - A lead counts while it holds, as a claim does, and while it works
//!   in that repository. A lead that leaves the repository thread stops
//!   being the lead (R178).
//! - The riff is paused or running. A new state is paused.
//!   [`State::riff`] reads it, and sets it for a person or a lead
//!   (01M3JCFTWCR72HQB8CBTQKXJNF, 01M3JCG3T8AJZN31SZQQTP3FAF).
//! - While the riff is paused, a claim fails. A held claim stays, and a
//!   release works (01M3JCG3WBHDF0ZWM06XV94ZDC).
//!
//! The state does no I/O and reads no clock. The caller passes `now`.
//!
//! # Saved state
//!
//! The state is a set of objects (R124): one [`Object::Sessions`] with
//! the sessions, their places, statuses, read cursors, claims, leads
//! and the riff state, and one
//! [`Object::Thread`] for each thread, with its members and messages.
//! [`crate::store`] names the objects in a store.
//!
//! - Each change marks the objects that it changes. [`State::changes`]
//!   gives the marked objects as JSON, and clears the marks. The thread
//!   objects come before the sessions object. So when only a part of a
//!   save succeeds, a saved cursor is never after the end of its saved
//!   thread.
//! - The open watch streams are not saved. A saved session holds the
//!   last time that it called and its last sign of life, in milliseconds
//!   since the Unix epoch, and whether it ended.
//! - [`State::load`] makes a state from the objects. Each session counts
//!   as stopped at the time of the load, so its claims end after
//!   [`CLAIM_GRACE`] unless it comes back (R125). A session that was
//!   gone at the save stays gone. A session with no sign of life for
//!   [`SESSION_EXPIRY`] is dropped, with its memberships, cursors and
//!   claims (R126). A cursor of a thread with no object is
//!   dropped too, so a new thread with the same name starts unread. A
//!   cursor after the last message of its thread moves back to the last
//!   message.
//! - The sessions object holds the time of its save. A claim or a lead
//!   whose session was stopped for more than [`CLAIM_GRACE`] at that
//!   time ended before the load. The load drops it (R154).
//!
//! # Example
//!
//! ```
//! use std::time::Instant;
//! use riff_core::name::SessionUri;
//! use riff_core::wire::{Post, RiffState};
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
//! // A new riff is paused. Mike's session is the lead, so it resumes it.
//! state.riff(&mike, Some(RiffState::Running), now).unwrap();
//!
//! // A post without an address wakes nobody, whatever its text.
//! let quiet = Post::new(&mike, thread.clone(), vec![], "@brett ready");
//! assert!(state.post(quiet, now, 0).unwrap().wakes.is_empty());
//!
//! // A post to brett's user wakes brett.
//! let to = vec!["user=brett".parse()?];
//! let delivery = state.post(Post::new(&mike, thread.clone(), to, "ready"), now, 0).unwrap();
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

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use riff_core::name::{Place, SessionUri, ThreadName, Who, check};
use riff_core::selector::Selector;
use riff_core::wire::{
    ClaimReply, Freed, Keys, Kind, LeadReply, Message, Post, RiffReply, RiffState, SessionInfo,
    Status, StatusInfo, Tailed, ThreadInfo, Wake,
};
use serde::{Deserialize, Serialize};

use crate::store::{self, Unreadable};

/// A claim stays with a session this long after the session stops (R9).
pub const CLAIM_GRACE: Duration = Duration::from_secs(5 * 60);

/// A session with no call, no keep-alive and no watch for this long is
/// gone (R206). `riff mcp` sends a keep-alive each
/// [`riff_core::wire::ALIVE_EVERY`].
pub const GONE: Duration = Duration::from_secs(3 * 60);

/// A load drops each session with no sign of life for this long (R126).
pub const SESSION_EXPIRY: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// All state of one `riff-server`. See the module docs for the rules.
#[derive(Default)]
pub struct State {
    sessions: BTreeMap<Who, Session>,
    threads: BTreeMap<ThreadName, Thread>,
    /// The last sequence number that each session read in each thread.
    cursors: BTreeMap<(Who, ThreadName), u64>,
    claims: BTreeMap<(ThreadName, String), Who>,
    /// The lead of each user in each repository thread (R175).
    leads: BTreeMap<(String, ThreadName), Who>,
    /// The state of the riff. A new riff is paused.
    riff: RiffState,
    /// Each object that changed since the last [`State::changes`].
    changed: BTreeSet<Object>,
}

struct Session {
    place: Place,
    /// The number of open watch streams.
    watchers: usize,
    /// The last call.
    last_seen: Instant,
    /// The last call before the load, in milliseconds since the Unix
    /// epoch. `None` when the session called after the load.
    seen_before_load: Option<u64>,
    /// The last sign of life: a call, a keep-alive or the end of a
    /// watch. `None` when the session was gone at the load and has not
    /// come back.
    alive: Option<Instant>,
    /// The last sign of life before the load, in milliseconds since the
    /// Unix epoch. `None` when the session showed life after the load.
    alive_before_load: Option<u64>,
    /// True after an end call, until the session comes back.
    ended: bool,
    status: Option<SetStatus>,
}

/// A status with the time that the session set it.
#[derive(Clone, Serialize, Deserialize)]
struct SetStatus {
    #[serde(flatten)]
    status: Status,
    /// Milliseconds since the Unix epoch.
    set_ms: u64,
}

impl Session {
    /// A new session that calls `now` from `place`.
    fn new(place: Place, now: Instant) -> Self {
        Session {
            place,
            watchers: 0,
            last_seen: now,
            seen_before_load: None,
            alive: Some(now),
            alive_before_load: None,
            ended: false,
            status: None,
        }
    }

    /// Records a sign of life at `now`. A gone session comes back.
    fn live(&mut self, now: Instant) {
        self.alive = Some(now);
        self.alive_before_load = None;
        self.ended = false;
    }

    /// True when the session ended, or had no sign of life for [`GONE`].
    fn gone(&self, now: Instant) -> bool {
        self.ended
            || (self.watchers == 0
                && self
                    .alive
                    .is_none_or(|alive| now.saturating_duration_since(alive) >= GONE))
    }

    /// True while the claims and the lead of the session hold.
    fn holds(&self, now: Instant) -> bool {
        !self.ended
            && (self.watchers > 0
                || self
                    .alive
                    .is_some_and(|alive| now.saturating_duration_since(alive) < CLAIM_GRACE))
    }

    /// The last sign of life, in milliseconds since the Unix epoch. A
    /// load is not a sign of life.
    fn alive_ms(&self, now: Instant, now_ms: u64) -> u64 {
        if self.watchers > 0 {
            return now_ms;
        }
        match (self.alive_before_load, self.alive) {
            (Some(before), _) => before,
            (None, Some(alive)) => {
                let ago = now.saturating_duration_since(alive).as_millis();
                now_ms.saturating_sub(u64::try_from(ago).unwrap_or(u64::MAX))
            }
            (None, None) => 0,
        }
    }

    /// The last time that the session called, in milliseconds since the
    /// Unix epoch. A live session calls now.
    fn seen_ms(&self, now: Instant, now_ms: u64) -> u64 {
        if self.watchers > 0 {
            return now_ms;
        }
        self.seen_before_load.unwrap_or_else(|| {
            let ago = now.saturating_duration_since(self.last_seen).as_millis();
            now_ms.saturating_sub(u64::try_from(ago).unwrap_or(u64::MAX))
        })
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Thread {
    members: BTreeSet<Who>,
    messages: Vec<Stored>,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    message: Message,
    /// Each session that the message woke.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    woken: BTreeSet<Who>,
}

/// An object of the saved state (R124). See the module docs. Each
/// thread object sorts before the sessions object.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Object {
    /// One thread, with its members and messages.
    Thread(ThreadName),
    /// The sessions, with their places, read cursors and claims.
    Sessions,
}

impl Object {
    /// The name of the object in a store.
    ///
    /// ```
    /// use riff_server::state::Object;
    ///
    /// assert_eq!(Object::Sessions.name(), "sessions");
    /// let thread = Object::Thread("como-technologies/riff".parse()?);
    /// assert_eq!(thread.name(), "threads/como-technologies%2Friff");
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn name(&self) -> String {
        match self {
            Object::Sessions => store::SESSIONS.into(),
            Object::Thread(thread) => store::thread_object(thread),
        }
    }
}

/// The JSON of the sessions object.
#[derive(Default, Serialize, Deserialize)]
struct SavedSessions {
    /// The time of the save, in milliseconds since the Unix epoch.
    #[serde(default)]
    saved_ms: u64,
    sessions: Vec<SavedSession>,
    cursors: Vec<SavedCursor>,
    claims: Vec<SavedClaim>,
    #[serde(default)]
    leads: Vec<SavedLead>,
    /// A saved state from before the riff state loads as paused.
    #[serde(default)]
    riff: RiffState,
}

#[derive(Serialize, Deserialize)]
struct SavedSession {
    /// The who and the place. It holds no claims.
    uri: SessionUri,
    /// The last call, in milliseconds since the Unix epoch.
    seen_ms: u64,
    /// The last sign of life, in milliseconds since the Unix epoch.
    #[serde(default)]
    alive_ms: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    ended: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    status: Option<SetStatus>,
}

#[derive(Serialize, Deserialize)]
struct SavedCursor {
    who: Who,
    thread: ThreadName,
    seq: u64,
}

#[derive(Serialize, Deserialize)]
struct SavedClaim {
    thread: ThreadName,
    item: String,
    who: Who,
}

/// The lead of `who`'s user in `thread`.
#[derive(Serialize, Deserialize)]
struct SavedLead {
    thread: ThreadName,
    who: Who,
}

/// The JSON of a thread object: the thread and its name.
#[derive(Serialize, Deserialize)]
struct Named<N, T> {
    name: N,
    #[serde(flatten)]
    thread: T,
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
    /// Makes a state from its saved objects: the sessions object, if
    /// there is one, and each thread object with its name. `now_ms` is
    /// `now` in milliseconds since the Unix epoch. See the module docs for
    /// the rules (R125, R126).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_server::state::{Object, State};
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&mike, now);
    ///
    /// let mut sessions = None;
    /// let mut threads = Vec::new();
    /// for (object, bytes) in state.changes(now, 1_000) {
    ///     match object {
    ///         Object::Sessions => sessions = Some(bytes),
    ///         Object::Thread(_) => threads.push((object.name(), bytes)),
    ///     }
    /// }
    /// let threads = threads.iter().map(|(name, bytes)| (name.as_str(), bytes.as_slice()));
    /// let loaded = State::load(sessions.as_deref(), threads, now, 2_000).unwrap();
    /// assert_eq!(loaded.who(now, 2_000, false)[0].uri.who(), mike.who());
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn load<'a>(
        sessions: Option<&[u8]>,
        threads: impl IntoIterator<Item = (&'a str, &'a [u8])>,
        now: Instant,
        now_ms: u64,
    ) -> Result<State, Unreadable> {
        let unreadable = |name: &str, e: serde_json::Error| Unreadable {
            name: name.to_owned(),
            why: e.to_string(),
        };
        let saved: SavedSessions = match sessions {
            Some(bytes) => {
                serde_json::from_slice(bytes).map_err(|e| unreadable(store::SESSIONS, e))?
            }
            None => SavedSessions::default(),
        };
        let expiry = u64::try_from(SESSION_EXPIRY.as_millis()).unwrap_or(u64::MAX);
        let grace = u64::try_from(CLAIM_GRACE.as_millis()).unwrap_or(u64::MAX);
        let mut state = State {
            riff: saved.riff,
            ..State::default()
        };
        let mut lapsed = BTreeSet::new();
        let gone = u64::try_from(GONE.as_millis()).unwrap_or(u64::MAX);
        for s in saved.sessions {
            let alive_ms = s.alive_ms.max(s.seen_ms);
            if now_ms.saturating_sub(alive_ms) > expiry {
                continue;
            }
            if s.ended || saved.saved_ms.saturating_sub(alive_ms) > grace {
                lapsed.insert(s.uri.who().clone());
            }
            let was_gone = s.ended || saved.saved_ms.saturating_sub(alive_ms) >= gone;
            let session = Session {
                seen_before_load: Some(s.seen_ms),
                alive: (!was_gone).then_some(now),
                alive_before_load: Some(alive_ms),
                ended: s.ended,
                status: s.status,
                ..Session::new(s.uri.place().clone(), now)
            };
            state.sessions.insert(s.uri.who().clone(), session);
        }
        for (object, bytes) in threads {
            let Named::<ThreadName, Thread> { name, mut thread } =
                serde_json::from_slice(bytes).map_err(|e| unreadable(object, e))?;
            thread
                .members
                .retain(|who| state.sessions.contains_key(who));
            state.threads.insert(name, thread);
        }
        for c in saved.cursors {
            let last = state
                .threads
                .get(&c.thread)
                .map(|t| t.messages.last().map_or(0, |m| m.message.seq));
            if let Some(last) = last
                && state.sessions.contains_key(&c.who)
            {
                state.cursors.insert((c.who, c.thread), c.seq.min(last));
            }
        }
        for c in saved.claims {
            if state.sessions.contains_key(&c.who) && !lapsed.contains(&c.who) {
                state.claims.insert((c.thread, c.item), c.who);
            }
        }
        for l in saved.leads {
            if state.sessions.contains_key(&l.who) && !lapsed.contains(&l.who) {
                state
                    .leads
                    .insert((l.who.user().to_owned(), l.thread), l.who);
            }
        }
        Ok(state)
    }

    /// Each object that changed since the last call, as JSON. The state
    /// then counts them as saved. `now_ms` is `now` in milliseconds since
    /// the Unix epoch.
    pub fn changes(&mut self, now: Instant, now_ms: u64) -> Vec<(Object, Vec<u8>)> {
        std::mem::take(&mut self.changed)
            .into_iter()
            .filter_map(|object| {
                let bytes = match &object {
                    Object::Sessions => to_json(&self.saved_sessions(now, now_ms)),
                    Object::Thread(name) => to_json(&Named {
                        name,
                        thread: self.threads.get(name)?,
                    }),
                };
                Some((object, bytes))
            })
            .collect()
    }

    /// Marks an object as changed again, for example after a failed save.
    pub fn mark_changed(&mut self, object: Object) {
        self.changed.insert(object);
    }

    /// Refuses `me` when the server knows its session ID under another
    /// user (R159). The server keys a session by user and session ID, so
    /// a new user would make a second session with the same ID. A
    /// session that the server knows under this user passes, so two
    /// entries from before this rule keep working.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::state::State;
    ///
    /// let mut state = State::default();
    /// let mike = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse().unwrap();
    /// let other = "riff://sandman@pangolin/como-technologies/riff?session=a6cf".parse().unwrap();
    /// state.register(&mike, Instant::now());
    /// assert!(state.check_user(&mike).is_ok());
    /// let error = state.check_user(&other).unwrap_err();
    /// assert!(error.contains("known as user mike"), "{error}");
    /// ```
    pub fn check_user(&self, me: &SessionUri) -> Result<(), String> {
        let who = me.who();
        let Some(id) = who.session() else {
            return Ok(());
        };
        if self.sessions.contains_key(who) {
            return Ok(());
        }
        match self
            .sessions
            .keys()
            .find(|known| known.session() == Some(id))
        {
            Some(known) => Err(format!(
                "session {id} is known as user {}, not {}. riff found another user for \
                 this session. Set RIFF_USER={} for the session, or start a new session.",
                known.user(),
                who.user(),
                known.user()
            )),
            None => Ok(()),
        }
    }

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
        self.lead_if_first(me.who(), now);
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
            session.seen_before_load = None;
            if !session.ended {
                session.live(now);
            }
            self.changed.insert(Object::Sessions);
        }
    }

    /// Records a call of `me` that changes nothing else, for example
    /// `who`.
    pub fn called(&mut self, me: &SessionUri, now: Instant) {
        self.arrive(me, now);
    }

    /// Records a keep-alive of `me`: a sign of life that is not a call
    /// (R204). It does not change the idle time in `who`. A gone session
    /// comes back (R207). An unknown session arrives as with a call.
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use riff_core::name::SessionUri;
    /// use riff_server::state::{GONE, State};
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&mike, now);
    ///
    /// // With a keep-alive each minute, the session waits for its user.
    /// let hour = now + Duration::from_secs(3600);
    /// state.alive(&mike, hour - Duration::from_secs(60));
    /// let shown = state.who(hour, 3_600_000, false);
    /// assert_eq!(shown[0].idle_secs, 3600);
    ///
    /// // With no sign of life for 3 minutes, it is gone.
    /// assert!(state.who(hour + GONE, 3_780_000, false).is_empty());
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn alive(&mut self, me: &SessionUri, now: Instant) {
        let who = me.who();
        let Some(session) = self.sessions.get_mut(who) else {
            self.arrive(me, now);
            return;
        };
        session.live(now);
        self.changed.insert(Object::Sessions);
    }

    /// Records that `me` ended (R205, R206). The session is gone at once.
    /// Its claims are free at once. Its lead does not count while it is
    /// gone, and counts again when it comes back, unless another session
    /// became the lead (R207). An unknown session stays unknown. A later
    /// call or keep-alive brings it back.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::RiffState;
    /// use riff_server::state::State;
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let brett: SessionUri = "riff://brett@heron/como-technologies/riff?session=77e0".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&mike, now);
    /// state.riff(&mike, Some(RiffState::Running), now).unwrap();
    /// let thread = mike.default_thread().unwrap();
    /// state.claim(&mike, &thread, "issue-12", now).unwrap();
    ///
    /// state.end(&mike, now);
    /// assert!(state.who(now, 0, false).is_empty());
    /// assert!(state.claim(&brett, &thread, "issue-12", now).unwrap().granted);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn end(&mut self, me: &SessionUri, now: Instant) {
        let who = me.who();
        let Some(session) = self.sessions.get_mut(who) else {
            return;
        };
        session.ended = true;
        session.last_seen = now;
        session.seen_before_load = None;
        self.claims.retain(|_, holder| holder != who);
        self.changed.insert(Object::Sessions);
    }

    /// A new start of `me`: a new agent process, a resume or a `/clear`
    /// (01M3JEE1QQCFS5TMZW5N2DAD2D). The session is live, with its ID,
    /// threads, cursors and lead. Each of its claims is free at once. It
    /// gives the claims that it freed.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::RiffState;
    /// use riff_server::state::State;
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&mike, now);
    /// state.riff(&mike, Some(RiffState::Running), now).unwrap();
    /// let thread = mike.default_thread().unwrap();
    /// state.claim(&mike, &thread, "issue-12", now).unwrap();
    ///
    /// let freed = state.start(&mike, now);
    /// assert_eq!(freed[0].item, "issue-12");
    /// let me = state.uri(mike.who(), now);
    /// assert!(me.claims().is_empty());
    /// assert!(me.lead(), "the lead stays the lead");
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn start(&mut self, me: &SessionUri, now: Instant) -> Vec<Freed> {
        let who = self.arrive(me, now);
        let mut freed = Vec::new();
        self.claims.retain(|(thread, item), holder| {
            let mine = *holder == who;
            if mine {
                freed.push(Freed {
                    thread: thread.clone(),
                    item: item.clone(),
                });
            }
            !mine
        });
        freed
    }

    /// Each known session with its URI now, whether it is live, and the
    /// time since its last call. `now_ms` is `now` in milliseconds since
    /// the Unix epoch. Only `all` lists gone sessions (R164). See the
    /// module docs for when a session is gone.
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use riff_core::name::SessionUri;
    /// use riff_server::state::State;
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&mike, now);
    ///
    /// let later = now + Duration::from_secs(120);
    /// assert_eq!(state.who(later, 120_000, false)[0].idle_secs, 120);
    /// let gone = now + Duration::from_secs(180);
    /// assert!(state.who(gone, 180_000, false).is_empty());
    /// assert_eq!(state.who(gone, 180_000, true).len(), 1);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn who(&self, now: Instant, now_ms: u64, all: bool) -> Vec<SessionInfo> {
        self.sessions
            .iter()
            .filter(|(_, session)| all || !session.gone(now))
            .map(|(who, session)| SessionInfo {
                uri: self.uri(who, now),
                live: session.watchers > 0,
                idle_secs: now_ms.saturating_sub(session.seen_ms(now, now_ms)) / 1000,
                status: session.status.as_ref().map(|s| StatusInfo {
                    status: s.status.clone(),
                    age_secs: now_ms.saturating_sub(s.set_ms) / 1000,
                }),
            })
            .collect()
    }

    /// The URI of a session now: its place, whether it is the lead, and
    /// the claims that it holds.
    ///
    /// # Panics
    ///
    /// When the server does not know the session.
    pub fn uri(&self, who: &Who, now: Instant) -> SessionUri {
        let place = self.sessions[who].place.clone();
        let claims = self
            .claims
            .iter()
            .filter(|(_, holder)| *holder == who)
            .map(|((_, item), _)| item.clone())
            .collect();
        SessionUri::new(who.clone(), place)
            .with_lead(self.is_lead(who, now))
            .with_claims(claims)
    }

    /// Makes `me` the lead of its user in its repository. It replaces
    /// the old lead (R177).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_server::state::State;
    ///
    /// let first: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a1".parse()?;
    /// let second: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=b2#api".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&first, now);
    /// state.register(&second, now);
    /// assert!(state.uri(first.who(), now).lead());
    /// assert!(!state.uri(second.who(), now).lead());
    ///
    /// let reply = state.lead(&second, now).unwrap();
    /// assert!(reply.lead.lead());
    /// assert_eq!(reply.replaced.unwrap().who(), first.who());
    /// assert!(!state.uri(first.who(), now).lead());
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn lead(&mut self, me: &SessionUri, now: Instant) -> Result<LeadReply, String> {
        let who = self.arrive(me, now);
        if who.session().is_none() {
            return Err("only an agent session can be the lead".into());
        }
        let thread = self.sessions[&who]
            .place
            .default_thread()
            .ok_or("the lead needs a git repository. Run it in a repository.")?;
        let key = (who.user().to_owned(), thread);
        let old = self.lead_of(&key, now).filter(|old| **old != who).cloned();
        self.leads.insert(key, who.clone());
        Ok(LeadReply {
            lead: self.uri(&who, now),
            replaced: old.map(|old| self.uri(&old, now)),
        })
    }

    /// The state of the riff. With `set`, it sets the state first. Only
    /// a person (`me` with no session ID) or a lead can set it
    /// (01M3JCG3T8AJZN31SZQQTP3FAF).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::RiffState;
    /// use riff_server::state::State;
    ///
    /// let lead: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a1".parse()?;
    /// let other: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=b2#api".parse()?;
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&lead, now);
    /// state.register(&other, now);
    /// assert_eq!(state.riff(&other, None, now).unwrap().state, RiffState::Paused);
    ///
    /// assert!(state.riff(&other, Some(RiffState::Running), now).is_err());
    /// assert!(state.riff(&lead, Some(RiffState::Running), now).unwrap().changed);
    /// let again = state.riff(&mike, Some(RiffState::Running), now).unwrap();
    /// assert!(!again.changed);
    /// assert_eq!(again.state, RiffState::Running);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn riff(
        &mut self,
        me: &SessionUri,
        set: Option<RiffState>,
        now: Instant,
    ) -> Result<RiffReply, String> {
        let who = self.arrive(me, now);
        let Some(set) = set else {
            return Ok(RiffReply {
                state: self.riff,
                changed: false,
            });
        };
        if who.session().is_some() && !self.is_lead(&who, now) {
            return Err(format!(
                "only your user or the lead can make the riff {set}. Tell the lead."
            ));
        }
        let changed = self.riff != set;
        self.riff = set;
        Ok(RiffReply {
            state: set,
            changed,
        })
    }

    /// Sets the status of `me` at `now_ms`, in milliseconds since the
    /// Unix epoch. It replaces the old status (R182). See
    /// [`Status::check`] for the status that it refuses (R183).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::Status;
    /// use riff_server::state::State;
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// let step = Status { step: "write the tests".into(), blocked: None };
    /// state.set_status(&mike, step.clone(), now, 1_000).unwrap();
    ///
    /// let status = state.who(now, 61_000, false)[0].status.clone().unwrap();
    /// assert_eq!(status.status, step);
    /// assert_eq!(status.age_secs, 60);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn set_status(
        &mut self,
        me: &SessionUri,
        status: Status,
        now: Instant,
        now_ms: u64,
    ) -> Result<(), String> {
        status.check()?;
        let who = self.arrive(me, now);
        if let Some(session) = self.sessions.get_mut(&who) {
            session.status = Some(SetStatus {
                status,
                set_ms: now_ms,
            });
        }
        Ok(())
    }

    /// The threads that `me` joined, with its unread counts. A count
    /// leaves out the own posts of `me` (01M3JPK82PN4F706MCHDH771MW).
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
                    members: t.members.iter().map(|m| self.uri(m, now)).collect(),
                    unread: t
                        .messages
                        .iter()
                        .filter(|m| m.message.seq > read && m.message.from.who() != who)
                        .count(),
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
        if let Some(t) = self.threads.get_mut(thread)
            && t.members.remove(me.who())
        {
            self.changed.insert(Object::Thread(thread.clone()));
        }
        let key = (me.who().user().to_owned(), thread.clone());
        if self.leads.get(&key) == Some(me.who()) {
            self.leads.remove(&key);
        }
    }

    /// Adds a message to a thread and wakes each session that `to`
    /// selects (R51). With no thread, the post is a direct message (R62).
    /// `at_ms` is the time of the post, in milliseconds since the Unix
    /// epoch. The message keeps the signature of the post (R198). The
    /// caller checks the signature.
    ///
    /// The signature covers the lead mark of `me`. So the sender of a
    /// signed message has `lead=true` only when `me` has it, and a signed
    /// post with the lead mark from a session that is not the lead is
    /// refused (R198).
    ///
    /// A signed post with the signature of a message in the thread is a
    /// copy, and is refused. So a session gets each request of its lead
    /// once (01M3JEJVXXEPPNGT3FY4ZSFCWZ).
    pub fn post(&mut self, post: Post, now: Instant, at_ms: u64) -> Result<Delivery, String> {
        let Post {
            me,
            thread,
            to,
            body,
            kind,
            sig,
            ..
        } = post;
        let from = self.arrive(&me, now);
        let signed = sig.is_some();
        if signed && me.lead() && !self.is_lead(&from, now) {
            return Err(
                "the post has the lead mark, but this session is not the lead. Post again.".into(),
            );
        }
        if to.iter().any(Selector::is_empty) {
            return Err("a selector needs one or more fields".into());
        }
        let thread = match thread {
            Some(thread) if thread.is_direct() => {
                return Err("leave out the thread to send a direct message".into());
            }
            Some(thread) => thread,
            None => ThreadName::direct(&from, &self.direct_target(&from, &to, now)?),
        };
        if let Some(sig) = &sig
            && let Some(copy) = self.threads.get(&thread).and_then(|t| {
                t.messages
                    .iter()
                    .find(|m| m.message.sig.as_ref() == Some(sig))
            })
        {
            return Err(format!(
                "the post is a copy of message {}: each signed message comes once",
                copy.message.seq
            ));
        }
        self.member(&from, &thread);
        let mut woken = BTreeSet::new();
        let mut unmatched = Vec::new();
        for selector in &to {
            let live = |who: &&Who| **who != from && !self.sessions[*who].gone(now);
            let mut matched: Vec<Who> = self
                .sessions
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
                matched = self
                    .sessions
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
        for who in &woken {
            self.member(who, &thread);
        }
        if kind == Kind::Note {
            woken.clear();
        }
        let mut sender = self.uri(&from, now);
        if signed {
            sender = sender.with_lead(me.lead());
        }
        let t = self.threads.entry(thread.clone()).or_default();
        let message = Message {
            seq: t.messages.last().map_or(1, |m| m.message.seq + 1),
            from: sender,
            to,
            body,
            at_ms,
            kind,
            sig,
        };
        t.messages.push(Stored {
            message: message.clone(),
            woken: woken.clone(),
        });
        self.changed.insert(Object::Thread(thread.clone()));
        let wakes = woken
            .iter()
            .map(|who| (who.clone(), wake(&thread, &message)))
            .collect();
        Ok(Delivery {
            wakes,
            woken: woken.iter().map(|who| self.uri(who, now)).collect(),
            unmatched,
            tailed: Tailed {
                thread,
                message,
                keys: Keys::new(),
                trusted: false,
            },
        })
    }

    /// Returns unread messages (or all of them) and marks them as read.
    /// The unread messages leave out the own posts of `me`; `all` gives
    /// them (01M3JPK82PN4F706MCHDH771MW).
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
            .filter(|m| all || (m.message.seq > from && m.message.from.who() != &who))
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
        if self.riff == RiffState::Paused {
            return Err(format!(
                "the riff is paused, so nobody claims {item}. Wait until your user or \
                 the lead resumes it."
            ));
        }
        let key = (thread.clone(), item.to_owned());
        if let Some(holder) = self.claims.get(&key)
            && *holder != who
            && self.holds(holder, now)
        {
            return Ok(ClaimReply {
                granted: false,
                holder: self.uri(holder, now),
            });
        }
        self.claims.insert(key, who.clone());
        Ok(ClaimReply {
            granted: true,
            holder: self.uri(&who, now),
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
            Some(holder) => Err(format!(
                "{item} is held by {}",
                self.uri(holder, now).short()
            )),
            None => Err(format!("nobody holds {item}")),
        }
    }

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
            .sessions
            .keys()
            .filter(|who| *who != from && selector.matches(&self.uri(who, now)))
            .partition(|who| !self.sessions[*who].gone(now));
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

    /// The lead of a user in a repository thread, while it holds and
    /// works in that repository.
    fn lead_of(&self, key: &(String, ThreadName), now: Instant) -> Option<&Who> {
        self.leads.get(key).filter(|who| {
            self.holds(who, now)
                && self.sessions[*who].place.default_thread().as_ref() == Some(&key.1)
        })
    }

    fn is_lead(&self, who: &Who, now: Instant) -> bool {
        self.sessions[who]
            .place
            .default_thread()
            .and_then(|thread| self.lead_of(&(who.user().to_owned(), thread), now))
            == Some(who)
    }

    /// Makes `who` the lead when its user has no lead in its repository
    /// and no other session of the user there holds (R176).
    fn lead_if_first(&mut self, who: &Who, now: Instant) {
        if who.session().is_none() {
            return;
        }
        let Some(thread) = self.sessions[who].place.default_thread() else {
            return;
        };
        let key = (who.user().to_owned(), thread);
        if self.lead_of(&key, now).is_some() {
            return;
        }
        let others = self.sessions.iter().any(|(other, s)| {
            other != who
                && other.user() == who.user()
                && other.session().is_some()
                && s.place.default_thread().as_ref() == Some(&key.1)
                && self.holds(other, now)
        });
        if !others {
            self.leads.insert(key, who.clone());
            self.changed.insert(Object::Sessions);
        }
    }

    fn holds(&self, holder: &Who, now: Instant) -> bool {
        self.sessions.get(holder).is_some_and(|s| s.holds(now))
    }

    /// Records that a session called. A new session starts in the place
    /// from its URI and joins the thread of its repository.
    fn arrive(&mut self, me: &SessionUri, now: Instant) -> Who {
        let who = me.who().clone();
        self.changed.insert(Object::Sessions);
        if let Some(session) = self.sessions.get_mut(&who) {
            session.last_seen = now;
            session.seen_before_load = None;
            session.live(now);
            return who;
        }
        self.sessions
            .insert(who.clone(), Session::new(me.place().clone(), now));
        if let Some(thread) = me.default_thread() {
            self.member(&who, &thread);
        }
        self.lead_if_first(&who, now);
        who
    }

    fn member(&mut self, who: &Who, thread: &ThreadName) {
        let t = self.threads.entry(thread.clone()).or_default();
        if t.members.insert(who.clone()) {
            self.changed.insert(Object::Thread(thread.clone()));
        }
    }

    fn saved_sessions(&self, now: Instant, now_ms: u64) -> SavedSessions {
        let sessions = self
            .sessions
            .iter()
            .map(|(who, s)| SavedSession {
                uri: SessionUri::new(who.clone(), s.place.clone()),
                seen_ms: s.seen_ms(now, now_ms),
                alive_ms: s.alive_ms(now, now_ms),
                ended: s.ended,
                status: s.status.clone(),
            })
            .collect();
        let cursors = self
            .cursors
            .iter()
            .map(|((who, thread), seq)| SavedCursor {
                who: who.clone(),
                thread: thread.clone(),
                seq: *seq,
            })
            .collect();
        let claims = self
            .claims
            .iter()
            .map(|((thread, item), who)| SavedClaim {
                thread: thread.clone(),
                item: item.clone(),
                who: who.clone(),
            })
            .collect();
        let leads = self
            .leads
            .iter()
            .map(|((_, thread), who)| SavedLead {
                thread: thread.clone(),
                who: who.clone(),
            })
            .collect();
        SavedSessions {
            saved_ms: now_ms,
            sessions,
            cursors,
            claims,
            leads,
            riff: self.riff,
        }
    }

    fn cursor(&self, who: &Who, thread: &ThreadName) -> u64 {
        self.cursors
            .get(&(who.clone(), thread.clone()))
            .copied()
            .unwrap_or(0)
    }
}

fn to_json(value: &impl Serialize) -> Vec<u8> {
    // Each key is a string and each value is plain data, so this cannot fail.
    serde_json::to_vec(value).expect("the state is valid JSON")
}

fn wake(thread: &ThreadName, message: &Message) -> Wake {
    Wake {
        thread: thread.clone(),
        seq: message.seq,
        from: message.from.clone(),
        kind: message.kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riff_core::wire::Kind;

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

    /// The URI as the lead. In [`setup`], `api` and `tests` are the leads
    /// of their users: each is the first session of its user.
    fn lead(u: SessionUri) -> SessionUri {
        u.with_lead(true)
    }

    fn repo() -> ThreadName {
        thread("como-technologies/riff")
    }

    /// Three sessions in a running riff.
    fn setup(now: Instant) -> State {
        let mut state = State::default();
        for n in [api(), tests(), docs()] {
            state.register(&n, now);
        }
        state.riff(&api(), Some(RiffState::Running), now).unwrap();
        state
    }

    fn woken(delivery: &Delivery) -> Vec<Who> {
        delivery.wakes.iter().map(|(w, _)| w.clone()).collect()
    }

    fn post(state: &mut State, me: &SessionUri, t: &str, sel: &[&str], body: &str) -> Delivery {
        state
            .post(
                Post::new(me, Some(thread(t)), to(sel), body),
                Instant::now(),
                0,
            )
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
    fn a_known_session_id_under_a_new_user_is_refused() {
        let state = setup(Instant::now());
        let other_user = uri("riff://brett@pangolin/como-technologies/riff?session=a1#api");
        assert!(state.check_user(&other_user).is_err());
        assert!(state.check_user(&api()).is_ok());
        let person = uri("riff://brett@pangolin");
        assert!(state.check_user(&person).is_ok());
        let new_session = uri("riff://brett@pangolin/como-technologies/riff?session=b9");
        assert!(state.check_user(&new_session).is_ok());
    }

    #[test]
    fn two_entries_from_before_the_rule_keep_working() {
        let now = Instant::now();
        let mut state = State::default();
        let mike = uri("riff://mike@pangolin/como-technologies/riff?session=a1");
        let sandman = uri("riff://sandman@pangolin/como-technologies/riff?session=a1");
        state.register(&mike, now);
        state.register(&sandman, now);
        assert!(state.check_user(&mike).is_ok());
        assert!(state.check_user(&sandman).is_ok());
    }

    #[test]
    fn two_sessions_in_one_place_are_two_sessions() {
        let now = Instant::now();
        let mut state = State::default();
        let a = uri("riff://mike@pangolin/como-technologies/riff?session=a");
        let b = uri("riff://mike@pangolin/como-technologies/riff?session=b");
        state.register(&a, now);
        state.register(&b, now);
        assert_eq!(listed(&state).len(), 2);
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
        assert_eq!(
            state.uri(tests().who(), Instant::now()).claims(),
            ["issue-6"]
        );
    }

    #[test]
    fn a_selector_that_matches_nobody_is_reported() {
        let now = Instant::now();
        let mut state = setup(now);
        let d = post(&mut state, &api(), "x", &["user=ghost", "user=brett"], "hi");
        assert_eq!(d.unmatched, to(&["user=ghost"]));
        assert_eq!(d.woken, vec![lead(tests())]);
    }

    #[test]
    fn a_post_with_no_address_field_is_refused() {
        let now = Instant::now();
        let mut state = setup(now);
        let result = state.post(
            Post::new(&api(), None, vec![Selector::default()], "x"),
            now,
            0,
        );
        assert!(result.is_err());
    }

    #[test]
    fn a_move_changes_the_place_but_not_the_session() {
        let now = Instant::now();
        let mut state = setup(now);
        state.claim(&api(), &repo(), "issue-6", now).unwrap();
        let moved = uri("riff://mike@pangolin/como-technologies/riff?session=a1#issue-6");
        state.register(&moved, now);
        assert_eq!(listed(&state).len(), 3);
        let now_uri = state.uri(api().who(), Instant::now());
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
        assert_eq!(
            state.uri(api().who(), Instant::now()).place().worktree(),
            Some("api")
        );
    }

    #[test]
    fn a_direct_message_wakes_the_receiver_and_hides_the_thread() {
        let now = Instant::now();
        let mut state = setup(now);
        let d = state
            .post(Post::new(&api(), None, to(&["session=b2"]), "hi"), now, 0)
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
                    .post(Post::new(&api(), None, to(sel), "x"), now, 0)
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
                Post::new(&api(), Some(thread("design")), to(&["user=brett"]), "look"),
                now,
                2,
            )
            .unwrap();
        assert_eq!(state.missed(&b).unwrap().seq, 2);
        let dm = state
            .post(Post::new(&docs(), None, to(&["session=b2"]), "hi"), now, 3)
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
        let live: Vec<_> = listed(&state).into_iter().filter(|s| s.live).collect();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].uri, lead(tests()));
    }

    #[test]
    fn who_hides_gone_sessions_but_not_live_ones() {
        let now = Instant::now();
        let mut state = setup(now);
        state.watch_started(&tests(), now);
        let day2 = now + DAY * 2;
        state.register(&api(), day2 - Duration::from_secs(90));
        let shown = state.who(day2, T0, false);
        let uris: Vec<SessionUri> = shown.iter().map(|s| s.uri.clone()).collect();
        assert_eq!(uris, [lead(tests()), lead(api())]);
        assert_eq!(shown[0].idle_secs, 0);
        assert_eq!(shown[1].idle_secs, 90);
        assert_eq!(state.who(day2, T0, true).len(), 3);
        // A call brings a gone session back.
        state.called(&docs(), day2);
        assert_eq!(state.who(day2, T0, false).len(), 3);
    }

    #[test]
    fn a_loaded_session_keeps_its_idle_time() {
        let now = Instant::now();
        let mut state = setup(now);
        let mut saved = Saved::new();
        save(&mut state, &mut saved, now, T0);
        let later = now + DAY;
        let loaded = load(&saved, later, T0 + ms(DAY));
        let shown = loaded.who(later, T0 + ms(DAY), false);
        assert_eq!(shown[0].idle_secs, DAY.as_secs());
        // A session that does not come back is gone after GONE.
        assert!(loaded.who(later + GONE, T0 + ms(DAY), false).is_empty());
    }

    #[test]
    fn a_session_gone_at_the_save_stays_gone_after_the_load() {
        let now = Instant::now();
        let mut state = setup(now);
        state.end(&docs(), now);
        let minute = Duration::from_secs(60);
        state.alive(&api(), now + minute * 2);
        let at = now + minute * 4;
        let mut saved = Saved::new();
        save(&mut state, &mut saved, at, T0 + ms(minute * 4));
        let loaded = load(&saved, at, T0 + ms(minute * 4));
        let shown: Vec<SessionUri> = loaded
            .who(at, T0 + ms(minute * 4), false)
            .into_iter()
            .map(|s| s.uri)
            .collect();
        assert_eq!(shown, [lead(api())]);
    }

    const MINUTE: Duration = Duration::from_secs(60);

    fn shown(state: &State, now: Instant) -> Vec<Who> {
        state
            .who(now, T0, false)
            .into_iter()
            .map(|s| s.uri.who().clone())
            .collect()
    }

    #[test]
    fn an_ended_session_leaves_at_once_and_frees_its_claims() {
        let now = Instant::now();
        let mut state = setup(now);
        state.watch_started(&api(), now);
        state.claim(&api(), &repo(), "issue-12", now).unwrap();
        state.end(&api(), now);
        assert!(!shown(&state, now).contains(api().who()));
        assert_eq!(state.who(now, T0, true).len(), 3);
        assert!(state.uri(api().who(), now).claims().is_empty());
        assert!(!is_lead(&state, &api(), now));
        let taken = state.claim(&docs(), &repo(), "issue-12", now).unwrap();
        assert!(taken.granted);
        // A resume brings the session back as the lead, with no claims.
        state.start(&api(), now);
        assert!(is_lead(&state, &api(), now));
        assert!(state.uri(api().who(), now).claims().is_empty());
    }

    #[test]
    fn a_new_start_frees_the_claims_and_keeps_the_lead_and_the_threads() {
        let now = Instant::now();
        let mut state = setup(now);
        state.watch_started(&api(), now);
        state.claim(&api(), &repo(), "issue-12", now).unwrap();
        state
            .claim(&api(), &thread("api-v2"), "issue-7", now)
            .unwrap();
        post(&mut state, &tests(), "como-technologies/riff", &[], "one");

        let freed = state.start(&api(), now);
        let items: Vec<&str> = freed.iter().map(|f| f.item.as_str()).collect();
        assert_eq!(items, ["issue-7", "issue-12"]);
        assert!(state.uri(api().who(), now).claims().is_empty());
        assert!(is_lead(&state, &api(), now));
        assert_eq!(state.read(&api(), &repo(), false, now).unwrap().len(), 1);
        let taken = state.claim(&docs(), &repo(), "issue-12", now).unwrap();
        assert!(taken.granted, "the item is free at once");
        assert!(state.start(&api(), now).is_empty());
    }

    #[test]
    fn a_gone_session_matches_no_selector() {
        let now = Instant::now();
        let mut state = setup(now);
        state.end(&tests(), now);
        let d = post(&mut state, &api(), "x", &["user=brett"], "hi");
        assert!(d.wakes.is_empty());
        assert_eq!(d.unmatched, to(&["user=brett"]));
        let error = state
            .post(Post::new(&api(), None, to(&["session=b2"]), "hi"), now, 0)
            .err()
            .unwrap();
        assert!(error.contains("is gone"), "{error}");
    }

    #[test]
    fn a_killed_session_leaves_after_gone_and_its_claims_after_the_grace() {
        let now = Instant::now();
        let mut state = setup(now);
        state.claim(&api(), &repo(), "issue-12", now).unwrap();
        // Only docs keeps sending keep-alives.
        for m in 1..=4 {
            state.alive(&docs(), now + MINUTE * m);
        }
        let later = now + GONE;
        assert_eq!(shown(&state, later), [docs().who().clone()]);
        let soon = now + CLAIM_GRACE - Duration::from_secs(1);
        // The lead ends with the claims, not when the session is gone.
        assert!(is_lead(&state, &api(), soon));
        assert!(!is_lead(&state, &api(), now + CLAIM_GRACE));
        assert!(
            !state
                .claim(&docs(), &repo(), "issue-12", soon)
                .unwrap()
                .granted
        );
        let late = now + CLAIM_GRACE;
        assert!(
            state
                .claim(&docs(), &repo(), "issue-12", late)
                .unwrap()
                .granted
        );
    }

    #[test]
    fn a_session_that_waits_for_its_user_stays_with_its_claims() {
        let now = Instant::now();
        let mut state = setup(now);
        state.claim(&api(), &repo(), "issue-12", now).unwrap();
        for m in 1..=60 {
            state.alive(&api(), now + MINUTE * m);
        }
        let hour = now + MINUTE * 60;
        let info = state.who(hour, T0 + ms(MINUTE * 60), false);
        let mike = info.iter().find(|s| s.uri.who() == api().who()).unwrap();
        assert_eq!(mike.idle_secs, 3600);
        assert!(!mike.live);
        assert_eq!(mike.uri.claims(), ["issue-12"]);
        assert!(
            !state
                .claim(&docs(), &repo(), "issue-12", hour)
                .unwrap()
                .granted
        );
    }

    #[test]
    fn twenty_killed_sessions_leave_who() {
        let now = Instant::now();
        let mut state = setup(now);
        for n in 0..20 {
            let dead = uri(&format!(
                "riff://mike@pangolin/como-technologies/riff?session=dead{n}"
            ));
            state.register(&dead, now);
        }
        assert_eq!(shown(&state, now).len(), 23);
        for m in 1..=4 {
            for me in [api(), tests(), docs()] {
                state.alive(&me, now + MINUTE * m);
            }
        }
        assert_eq!(shown(&state, now + MINUTE * 4).len(), 3);
        assert_eq!(state.who(now + MINUTE * 4, T0, true).len(), 23);
    }

    #[test]
    fn a_session_comes_back_after_a_network_fault() {
        let now = Instant::now();
        let mut state = setup(now);
        post(&mut state, &api(), "design", &["user=brett"], "look");
        state.read(&tests(), &repo(), false, now).unwrap();
        let back = now + MINUTE * 5;
        assert!(!shown(&state, back).contains(tests().who()));
        state.alive(&tests(), back);
        assert!(shown(&state, back).contains(tests().who()));
        // The same threads and cursors.
        let threads = state.threads(&tests(), back);
        let design = threads
            .iter()
            .find(|t| t.thread == thread("design"))
            .unwrap();
        assert_eq!(design.unread, 1);
        assert!(
            state
                .read(&tests(), &repo(), false, back)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_call_brings_an_ended_session_back() {
        let now = Instant::now();
        let mut state = setup(now);
        state.end(&api(), now);
        state.called(&api(), now);
        assert!(shown(&state, now).contains(api().who()));
        // An end of an unknown session makes no session.
        let ghost = uri("riff://mike@pangolin/como-technologies/riff?session=zz");
        state.end(&ghost, now);
        assert_eq!(state.who(now, T0, true).len(), 3);
    }

    fn status(step: &str, blocked: Option<&str>) -> Status {
        Status {
            step: step.into(),
            blocked: blocked.map(Into::into),
        }
    }

    #[test]
    fn a_status_replaces_the_old_one_and_shows_its_age() {
        let now = Instant::now();
        let mut state = setup(now);
        assert!(listed(&state).iter().all(|s| s.status.is_none()));
        state
            .set_status(&tests(), status("write the tests", None), now, T0)
            .unwrap();
        let blocked = status("merge", Some("waits for a review"));
        state
            .set_status(&tests(), blocked.clone(), now, T0 + 1_000)
            .unwrap();
        let later = T0 + 121_000;
        let shown = state.who(now, later, false);
        let brett = shown.iter().find(|s| s.uri.who() == tests().who()).unwrap();
        let info = brett.status.clone().unwrap();
        assert_eq!(info.status, blocked);
        assert_eq!(info.age_secs, 120);
        let others = shown.iter().filter(|s| s.uri.who() != tests().who());
        assert!(others.into_iter().all(|s| s.status.is_none()));
    }

    #[test]
    fn a_status_that_does_not_fit_on_one_line_is_refused() {
        let now = Instant::now();
        let mut state = setup(now);
        for bad in [
            status("", None),
            status("merge", Some(" ")),
            status("line\nbreak", None),
            status(&"x".repeat(201), None),
        ] {
            assert!(state.set_status(&api(), bad, now, T0).is_err());
        }
        assert!(listed(&state).iter().all(|s| s.status.is_none()));
    }

    /// 01M3JPMQE6S7YM4HPEVGXWK7ET
    #[test]
    fn a_note_wakes_nobody_and_read_shows_it() {
        let now = Instant::now();
        let mut state = setup(now);
        let note = Post {
            kind: Kind::Note,
            ..Post::new(
                &api(),
                Some(repo()),
                to(&["repo=como-technologies/riff", "user=nobody"]),
                "board: wave 4",
            )
        };
        let delivery = state.post(note, now, 0).unwrap();
        assert!(delivery.wakes.is_empty());
        assert!(delivery.woken.is_empty());
        assert_eq!(delivery.unmatched, to(&["user=nobody"]));
        assert_eq!(state.missed(tests().who()), None);
        let messages = state.read(&docs(), &repo(), false, now).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].kind, Kind::Note);
        assert_eq!(messages[0].body, "board: wave 4");
    }

    /// A direct note wakes nobody, but the receiver sees it at its next
    /// read (01M3JPMQE6S7YM4HPEVGXWK7ET).
    #[test]
    fn a_direct_note_reaches_the_receiver() {
        let now = Instant::now();
        let mut state = setup(now);
        let note = Post {
            kind: Kind::Note,
            ..Post::new(&api(), None, to(&["session=b2"]), "done")
        };
        let delivery = state.post(note, now, 0).unwrap();
        assert!(delivery.wakes.is_empty());
        let dm = delivery.tailed.thread;
        assert!(
            state
                .threads(&tests(), now)
                .iter()
                .any(|t| t.thread == dm && t.unread == 1)
        );
        assert_eq!(state.read(&tests(), &dm, false, now).unwrap().len(), 1);
    }

    #[test]
    fn a_status_request_wakes_with_its_kind() {
        let now = Instant::now();
        let mut state = setup(now);
        let ask = Post {
            kind: Kind::Status,
            ..Post::new(
                &api(),
                Some(repo()),
                to(&["repo=como-technologies/riff"]),
                "",
            )
        };
        let delivery = state.post(ask, now, 0).unwrap();
        assert_eq!(delivery.wakes.len(), 2);
        assert!(delivery.wakes.iter().all(|(_, w)| w.kind == Kind::Status));
        assert_eq!(delivery.tailed.message.kind, Kind::Status);
        assert_eq!(state.missed(tests().who()).unwrap().kind, Kind::Status);
        let messages = state.read(&docs(), &repo(), false, now).unwrap();
        assert_eq!(messages[0].kind, Kind::Status);
        post(
            &mut state,
            &api(),
            "como-technologies/riff",
            &["user=brett"],
            "hi",
        );
        assert_eq!(state.missed(tests().who()).unwrap().kind, Kind::Message);
    }

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);
    /// A time in milliseconds since the Unix epoch.
    const T0: u64 = 1_800_000_000_000;

    fn ms(d: Duration) -> u64 {
        u64::try_from(d.as_millis()).unwrap()
    }

    type Saved = BTreeMap<Object, Vec<u8>>;

    /// Saves each changed object, as a store does.
    fn save(state: &mut State, saved: &mut Saved, now: Instant, now_ms: u64) {
        saved.extend(state.changes(now, now_ms));
    }

    fn load(saved: &Saved, now: Instant, now_ms: u64) -> State {
        let sessions = saved.get(&Object::Sessions).map(Vec::as_slice);
        let threads: Vec<(String, &[u8])> = saved
            .iter()
            .filter(|(object, _)| **object != Object::Sessions)
            .map(|(object, bytes)| (object.name(), bytes.as_slice()))
            .collect();
        let threads = threads.iter().map(|(name, bytes)| (name.as_str(), *bytes));
        State::load(sessions, threads, now, now_ms).unwrap()
    }

    /// Each session, gone or not.
    fn listed(state: &State) -> Vec<SessionInfo> {
        state.who(Instant::now(), T0, true)
    }

    fn json(value: &impl Serialize) -> serde_json::Value {
        serde_json::to_value(value).unwrap()
    }

    #[test]
    fn a_load_gives_back_the_saved_state() {
        let now = Instant::now();
        let mut state = setup(now);
        post(&mut state, &api(), "design", &["user=brett"], "look");
        let dm = state
            .post(Post::new(&api(), None, to(&["session=c3"]), "hi"), now, 7)
            .unwrap()
            .tailed
            .thread;
        post(&mut state, &api(), "como-technologies/riff", &[], "one");
        state.read(&tests(), &repo(), false, now).unwrap();
        state.leave(&docs(), &repo(), now);
        state.claim(&tests(), &repo(), "issue-6", now).unwrap();
        let blocked = status("merge", Some("waits for a review"));
        state
            .set_status(&tests(), blocked, now, T0 - 5_000)
            .unwrap();
        let mut saved = Saved::new();
        save(&mut state, &mut saved, now, T0);

        let later = now + Duration::from_secs(1);
        let mut loaded = load(&saved, later, T0 + 1000);
        let brett = listed(&loaded)
            .into_iter()
            .find(|s| s.uri.who() == tests().who());
        assert_eq!(brett.unwrap().status.unwrap().age_secs, 5);
        assert_eq!(json(&listed(&loaded)), json(&listed(&state)));
        for me in [api(), tests(), docs()] {
            let threads = loaded.threads(&me, later);
            assert_eq!(json(&threads), json(&state.threads(&me, later)));
        }
        for (me, t) in [(docs(), thread("design")), (docs(), dm), (api(), repo())] {
            let messages = loaded.read(&me, &t, true, later).unwrap();
            assert_eq!(messages, state.read(&me, &t, true, later).unwrap());
            assert!(!messages.is_empty());
        }
        // Brett read the repository thread, and holds a claim.
        assert!(
            loaded
                .read(&tests(), &repo(), false, later)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            loaded.uri(tests().who(), Instant::now()).claims(),
            ["issue-6"]
        );
        // Brett did not read the message that woke him.
        assert_eq!(
            loaded.missed(tests().who()).unwrap().thread,
            thread("design")
        );
    }

    #[test]
    fn a_claim_ends_after_the_grace_period_from_the_load() {
        let now = Instant::now();
        let mut state = setup(now);
        state.watch_started(&api(), now);
        state.claim(&api(), &repo(), "issue-12", now).unwrap();
        let mut saved = Saved::new();
        save(&mut state, &mut saved, now, T0);

        let start = now + Duration::from_secs(3600);
        let mut loaded = load(&saved, start, T0 + 3_600_000);
        assert!(listed(&loaded).iter().all(|s| !s.live));
        let soon = start + CLAIM_GRACE - Duration::from_secs(1);
        assert!(
            !loaded
                .claim(&tests(), &repo(), "issue-12", soon)
                .unwrap()
                .granted
        );
        let late = start + CLAIM_GRACE + Duration::from_secs(1);
        assert!(
            loaded
                .claim(&tests(), &repo(), "issue-12", late)
                .unwrap()
                .granted
        );
    }

    #[test]
    fn a_session_that_comes_back_keeps_its_claim() {
        let now = Instant::now();
        let mut state = setup(now);
        state.claim(&api(), &repo(), "issue-12", now).unwrap();
        let mut saved = Saved::new();
        save(&mut state, &mut saved, now, T0);

        let mut loaded = load(&saved, now, T0);
        loaded.watch_started(&api(), now + Duration::from_secs(60));
        let late = now + CLAIM_GRACE * 2;
        assert!(
            !loaded
                .claim(&tests(), &repo(), "issue-12", late)
                .unwrap()
                .granted
        );
    }

    #[test]
    fn a_load_drops_each_session_not_seen_for_30_days() {
        let now = Instant::now();
        let mut state = setup(now);
        state.claim(&tests(), &repo(), "issue-6", now).unwrap();
        post(&mut state, &api(), "como-technologies/riff", &[], "one");
        state.read(&tests(), &repo(), false, now).unwrap();
        let mut saved = Saved::new();
        save(&mut state, &mut saved, now, T0);
        // Only mike's api session calls again, 20 days later.
        let day20 = now + DAY * 20;
        state.register(&api(), day20);
        save(&mut state, &mut saved, day20, T0 + ms(DAY * 20));

        let day31 = now + DAY * 31;
        let mut loaded = load(&saved, day31, T0 + ms(DAY * 31));
        let who: Vec<SessionUri> = listed(&loaded).into_iter().map(|s| s.uri).collect();
        assert_eq!(who, [lead(api())]);
        assert_eq!(loaded.threads(&api(), day31)[0].members, [lead(api())]);
        assert!(
            loaded
                .claim(&api(), &repo(), "issue-6", day31)
                .unwrap()
                .granted
        );
        // Brett's cursor went with his session.
        assert_eq!(
            loaded.read(&tests(), &repo(), false, day31).unwrap().len(),
            1
        );
    }

    #[test]
    fn a_load_keeps_a_session_seen_30_days_ago() {
        let now = Instant::now();
        let mut state = setup(now);
        let mut saved = Saved::new();
        save(&mut state, &mut saved, now, T0);
        let loaded = load(&saved, now + DAY * 30, T0 + ms(DAY * 30));
        assert_eq!(listed(&loaded).len(), 3);
    }

    #[test]
    fn a_live_session_counts_as_seen_at_each_save() {
        let now = Instant::now();
        let mut state = setup(now);
        state.watch_started(&tests(), now);
        let mut saved = Saved::new();
        save(&mut state, &mut saved, now, T0);
        // Another session calls 31 days later; brett's watch is still open.
        let day31 = now + DAY * 31;
        state.register(&api(), day31);
        save(&mut state, &mut saved, day31, T0 + ms(DAY * 31));
        let loaded = load(&saved, day31, T0 + ms(DAY * 31));
        let who: Vec<SessionUri> = listed(&loaded).into_iter().map(|s| s.uri).collect();
        assert_eq!(who, [lead(tests()), lead(api())]);
    }

    #[test]
    fn a_session_keeps_its_last_call_across_loads() {
        let now = Instant::now();
        let mut state = setup(now);
        let mut saved = Saved::new();
        save(&mut state, &mut saved, now, T0);
        // A load 20 days later does not count as a call.
        let day20 = now + DAY * 20;
        let mut loaded = load(&saved, day20, T0 + ms(DAY * 20));
        loaded.register(&api(), day20);
        let mut saved = Saved::new();
        save(&mut loaded, &mut saved, day20, T0 + ms(DAY * 20));
        let again = load(&saved, day20 + DAY * 11, T0 + ms(DAY * 31));
        let who: Vec<SessionUri> = listed(&again).into_iter().map(|s| s.uri).collect();
        assert_eq!(who, [lead(api())]);
    }

    #[test]
    fn a_cursor_of_a_deleted_thread_object_is_dropped() {
        let now = Instant::now();
        let mut state = setup(now);
        post(&mut state, &api(), "design", &[], "a");
        post(&mut state, &api(), "design", &[], "b");
        state.read(&tests(), &thread("design"), false, now).unwrap();
        let mut saved = Saved::new();
        save(&mut state, &mut saved, now, T0);
        // The lifecycle rule deleted the thread object (R46).
        saved.remove(&Object::Thread(thread("design")));

        let mut loaded = load(&saved, now, T0);
        post(&mut loaded, &api(), "design", &[], "new");
        let unread = loaded
            .read(&tests(), &thread("design"), false, now)
            .unwrap();
        assert_eq!(unread.len(), 1);
    }

    #[test]
    fn a_cursor_after_the_end_of_its_saved_thread_moves_back() {
        let now = Instant::now();
        let mut state = setup(now);
        post(&mut state, &api(), "design", &[], "a");
        let mut saved = Saved::new();
        save(&mut state, &mut saved, now, T0);
        let design = Object::Thread(thread("design"));
        let old_thread = saved[&design].clone();
        post(&mut state, &api(), "design", &[], "b");
        state.read(&tests(), &thread("design"), false, now).unwrap();
        save(&mut state, &mut saved, now, T0);
        // Only the sessions object of the second save reached the store.
        saved.insert(design, old_thread);

        let mut loaded = load(&saved, now, T0);
        post(&mut loaded, &api(), "design", &[], "new");
        let unread = loaded
            .read(&tests(), &thread("design"), false, now)
            .unwrap();
        let bodies: Vec<&str> = unread.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, ["new"]);
    }

    #[test]
    fn a_load_drops_a_claim_that_ended_before_the_save() {
        let now = Instant::now();
        let mut state = setup(now);
        state.claim(&api(), &repo(), "issue-6", now).unwrap();
        state.claim(&docs(), &repo(), "issue-7", now).unwrap();
        // The save is 6 minutes later. The api claim has ended. The docs
        // session called 1 minute before the save.
        let minute = Duration::from_secs(60);
        state.register(&docs(), now + minute * 5);
        state.register(&tests(), now + minute * 6);
        let mut saved = Saved::new();
        save(
            &mut state,
            &mut saved,
            now + minute * 6,
            T0 + ms(minute * 6),
        );

        let start = now + minute * 60;
        let mut loaded = load(&saved, start, T0 + ms(minute * 60));
        assert!(loaded.uri(api().who(), Instant::now()).claims().is_empty());
        assert_eq!(
            loaded.uri(docs().who(), Instant::now()).claims(),
            ["issue-7"]
        );
        let taken = loaded.claim(&tests(), &repo(), "issue-7", start).unwrap();
        assert!(!taken.granted);
    }

    #[test]
    fn changes_gives_each_changed_object_once() {
        let now = Instant::now();
        let mut state = setup(now);
        let objects = |state: &mut State| -> Vec<Object> {
            state.changes(now, T0).into_iter().map(|(o, _)| o).collect()
        };
        assert_eq!(
            objects(&mut state),
            [Object::Thread(repo()), Object::Sessions]
        );
        assert!(objects(&mut state).is_empty());
        post(&mut state, &api(), "design", &[], "x");
        let design = Object::Thread(thread("design"));
        assert_eq!(objects(&mut state), [design.clone(), Object::Sessions]);
        // A call that only looks changes nothing.
        listed(&state);
        state.missed(tests().who());
        assert!(objects(&mut state).is_empty());
        // A second join changes only the sessions.
        state.join(&api(), &thread("design"), now);
        assert_eq!(objects(&mut state), [Object::Sessions]);
        state.leave(&api(), &thread("design"), now);
        assert_eq!(objects(&mut state), [design, Object::Sessions]);
    }

    #[test]
    fn a_load_refuses_an_object_that_is_not_valid() {
        let now = Instant::now();
        let error = State::load(Some(b"{".as_slice()), [], now, T0)
            .err()
            .unwrap();
        assert_eq!(error.name, "sessions");
        assert!(error.why.contains("EOF"), "{error:?}");
        let threads = [("threads/x", b"[]".as_slice())];
        let error = State::load(None, threads, now, T0).err().unwrap();
        assert_eq!(error.name, "threads/x");
    }

    fn is_lead(state: &State, u: &SessionUri, now: Instant) -> bool {
        state.uri(u.who(), now).lead()
    }

    fn mike_lead() -> Vec<Selector> {
        vec![Selector::lead("mike", "como-technologies/riff")]
    }

    #[test]
    fn the_first_session_of_a_user_in_a_repository_is_the_lead() {
        let now = Instant::now();
        let state = setup(now);
        assert!(is_lead(&state, &api(), now));
        assert!(is_lead(&state, &tests(), now));
        assert!(!is_lead(&state, &docs(), now));
        let leads: Vec<Who> = listed(&state)
            .into_iter()
            .filter(|s| s.uri.lead())
            .map(|s| s.uri.who().clone())
            .collect();
        assert_eq!(leads, [tests().who().clone(), api().who().clone()]);
    }

    #[test]
    fn a_signed_message_has_the_lead_mark_only_when_it_is_signed() {
        let now = Instant::now();
        let mut state = setup(now);
        let repo = Some(thread("como-technologies/riff"));
        let signed = |me: SessionUri, sig: &str| Post {
            sig: Some(sig.into()),
            ..Post::new(&me, repo.clone(), vec![], "merge now")
        };

        // docs is not the lead, so it cannot sign the lead mark.
        let error = state
            .post(signed(lead(docs()), "s1"), now, 0)
            .err()
            .unwrap();
        assert!(error.contains("not the lead"), "{error}");

        // api is the lead. Its message has the mark only when it signed it.
        let d = state.post(signed(lead(api()), "s2"), now, 0).unwrap();
        assert!(d.tailed.message.from.lead());
        let d = state.post(signed(api(), "s3"), now, 0).unwrap();
        assert!(!d.tailed.message.from.lead());
        // Without a signature, the mark comes from the server.
        let d = state.post(Post::new(&api(), repo.clone(), vec![], "x"), now, 0);
        assert!(d.unwrap().tailed.message.from.lead());
    }

    /// 01M3JEJVXXEPPNGT3FY4ZSFCWZ
    #[test]
    fn a_copy_of_a_signed_message_is_refused() {
        let now = Instant::now();
        let mut state = setup(now);
        let repo = Some(thread("como-technologies/riff"));
        let request = Post {
            sig: Some("the signature of the lead".into()),
            ..Post::new(
                &lead(api()),
                repo.clone(),
                vec![],
                "request: claim issue-12",
            )
        };
        let first = state.post(request.clone(), now, 0).unwrap();
        let error = state.post(request, now, 0).err().unwrap();
        assert!(
            error.contains(&format!("a copy of message {}", first.tailed.message.seq)),
            "{error}"
        );
        // A new message with its own signature goes through.
        let again = Post {
            sig: Some("a new signature".into()),
            ..Post::new(
                &lead(api()),
                repo.clone(),
                vec![],
                "request: claim issue-12",
            )
        };
        let second = state.post(again, now, 0).unwrap();
        assert_eq!(second.tailed.message.seq, first.tailed.message.seq + 1);
    }

    #[test]
    fn a_person_or_a_session_outside_git_is_never_the_lead() {
        let now = Instant::now();
        let mut state = State::default();
        let person = uri("riff://mike@pangolin");
        let notes = uri("riff://mike@pangolin/-?session=n1#notes");
        state.register(&person, now);
        state.register(&notes, now);
        assert!(!is_lead(&state, &person, now));
        assert!(!is_lead(&state, &notes, now));
        let error = state.lead(&person, now).unwrap_err();
        assert!(error.contains("only an agent session"), "{error}");
        let error = state.lead(&notes, now).unwrap_err();
        assert!(error.contains("git repository"), "{error}");
    }

    #[test]
    fn a_marked_lead_replaces_the_old_lead() {
        let now = Instant::now();
        let mut state = setup(now);
        let reply = state.lead(&docs(), now).unwrap();
        assert_eq!(reply.lead, lead(docs()));
        assert_eq!(reply.replaced, Some(api()));
        assert!(!is_lead(&state, &api(), now));
        assert!(
            is_lead(&state, &tests(), now),
            "another user keeps its lead"
        );
        let again = state.lead(&docs(), now).unwrap();
        assert_eq!(again.replaced, None);
    }

    #[test]
    fn a_lead_selector_wakes_only_the_lead() {
        let now = Instant::now();
        let mut state = setup(now);
        let d = post(&mut state, &tests(), "x", &["user=mike,lead=true"], "hi");
        assert_eq!(woken(&d), [api().who().clone()]);
        let d = post(&mut state, &tests(), "x", &["user=mike,lead=false"], "hi");
        assert_eq!(woken(&d), [docs().who().clone()]);
    }

    #[test]
    fn a_lead_selector_with_no_live_lead_wakes_the_free_sessions() {
        let now = Instant::now();
        let mut state = setup(now);
        let sel = ["user=mike,repo=como-technologies/riff,lead=true"];
        let request = |state: &mut State| post(state, &tests(), "x", &sel, "verify request");
        state.end(&api(), now);
        let d = request(&mut state);
        assert_eq!(woken(&d), [docs().who().clone()]);
        assert!(d.unmatched.is_empty());

        state.claim(&docs(), &repo(), "issue-12", now).unwrap();
        let d = request(&mut state);
        assert!(woken(&d).is_empty(), "a session with a claim is not free");
        assert_eq!(d.unmatched, to(&sel));

        state.register(&api(), now);
        state.lead(&api(), now).unwrap();
        state.release(&docs(), &repo(), "issue-12", now).unwrap();
        let d = request(&mut state);
        assert_eq!(woken(&d), [api().who().clone()], "a live lead wakes alone");
    }

    #[test]
    fn a_direct_message_can_go_to_the_lead() {
        let now = Instant::now();
        let mut state = setup(now);
        let d = state
            .post(Post::new(&docs(), None, mike_lead(), "may I?"), now, 0)
            .unwrap();
        assert_eq!(woken(&d), [api().who().clone()]);
        assert_eq!(
            d.tailed.thread,
            ThreadName::direct(docs().who(), api().who())
        );

        let error = state
            .post(Post::new(&api(), None, mike_lead(), "me?"), now, 0)
            .err()
            .unwrap();
        assert!(error.contains("Ask your own user"), "{error}");
        let error = state
            .post(Post::new(&api(), None, to(&["user=mike"]), "who?"), now, 0)
            .err()
            .unwrap();
        assert!(error.contains("session or lead=true"), "{error}");
        let error = state
            .post(
                Post::new(&docs(), None, to(&["lead=true"]), "which?"),
                now,
                0,
            )
            .err()
            .unwrap();
        assert!(error.contains("2 sessions match"), "{error}");
    }

    #[test]
    fn a_stopped_lead_is_no_lead_until_it_comes_back() {
        let now = Instant::now();
        let mut state = setup(now);
        state.watch_started(&docs(), now);
        let later = now + CLAIM_GRACE + Duration::from_secs(1);
        assert!(!is_lead(&state, &api(), later));
        let error = state
            .post(Post::new(&docs(), None, mike_lead(), "may I?"), later, 0)
            .err()
            .unwrap();
        assert!(error.contains("Ask your own user"), "{error}");

        // docs still works, so a new session is not the first one.
        let new = uri("riff://mike@pangolin/como-technologies/riff?session=e5");
        state.register(&new, later);
        assert!(!is_lead(&state, &new, later));
        assert!(!is_lead(&state, &docs(), later));

        // The old lead comes back before anyone takes its place.
        state.register(&api(), later);
        assert!(is_lead(&state, &api(), later));
    }

    #[test]
    fn a_new_first_session_takes_the_lead_of_a_stopped_one() {
        let now = Instant::now();
        let mut state = setup(now);
        let next_day = now + DAY;
        let new = uri("riff://mike@pangolin/como-technologies/riff?session=e5");
        state.register(&new, next_day);
        assert!(is_lead(&state, &new, next_day));
        assert!(!is_lead(&state, &api(), next_day));
    }

    #[test]
    fn a_lead_that_leaves_its_repository_is_no_lead_there() {
        let now = Instant::now();
        let mut state = setup(now);
        state.leave(&api(), &repo(), now);
        assert!(!is_lead(&state, &api(), now));
        state.register(&api(), now);
        assert!(
            !is_lead(&state, &api(), now),
            "docs holds, so api is not first"
        );

        let mut state = setup(now);
        let other = uri("riff://mike@pangolin/como-technologies/other?session=a1");
        state.register(&other, now);
        assert!(is_lead(&state, &other, now), "the first of mike in other");
        let d = state
            .post(Post::new(&tests(), None, mike_lead(), "hi"), now, 0)
            .err()
            .unwrap();
        assert!(d.contains("Ask your own user"), "{d}");
    }

    #[test]
    fn a_lead_stays_across_a_load_unless_it_had_stopped() {
        let now = Instant::now();
        let mut state = setup(now);
        state.lead(&docs(), now).unwrap();
        let mut saved = Saved::new();
        save(&mut state, &mut saved, now, T0);
        let loaded = load(&saved, now, T0);
        assert!(is_lead(&loaded, &docs(), now));
        assert!(!is_lead(&loaded, &api(), now));

        // brett stopped long before this save.
        let later = now + CLAIM_GRACE * 2;
        state.register(&docs(), later);
        let mut saved = Saved::new();
        save(&mut state, &mut saved, later, T0 + ms(CLAIM_GRACE * 2));
        let loaded = load(&saved, later, T0 + ms(CLAIM_GRACE * 2));
        assert!(is_lead(&loaded, &docs(), later));
        assert!(!is_lead(&loaded, &tests(), later));
    }
}
