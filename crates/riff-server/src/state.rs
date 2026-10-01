//! The in-memory state of `riff-server`.
//!
//! # Model
//!
//! The state has two types (01M3WNQRCBP0PHSA0H3THDH5NJ): the [`Riff`]
//! is the state that the log gives, and the [`Presence`] is memory.
//!
//! | Data | Key | Type | Where it comes from |
//! |---|---|---|---|
//! | Threads | thread name | [`Riff`] | The log. Members, and the last [`KEEP_MESSAGES`] messages with a sequence number that starts at 1. |
//! | Claims | thread and item | [`Riff`] | The log. The session that holds the item. |
//! | Leads | user and repository thread | [`Riff`] | The log. The lead session of the user. |
//! | Riff state | none: one for the server | [`Riff`] | The log. Paused or running, and the settings of idle workers. |
//! | Known sessions | who | [`Riff`] | The log. The URI and the time of the last record that names the session. |
//! | Sessions | who | [`Presence`] | Memory. The place, open watch streams, the last call, the last sign of life, whether it ended, and its last status. |
//! | Read cursors | who and thread | [`Presence`] | Memory and the checkpoint. The last sequence number that the session read. |
//!
//! The server keys each session by its [`Who`]: the user and the session
//! ID. It builds the [`SessionUri`] of a session from the who, the place
//! and the claims that the session holds now.
//!
//! # Where each part lives
//!
//! | File | What it holds |
//! |---|---|
//! | `state.rs` | [`State`]: the two copies of the [`Riff`], the queue, the [`Presence`] and the clock. Its methods are the calls of the server. Each one that changes the riff runs one command type. The signals and the queries are here too: a keep-alive, a status, a watch, `who`, `read`. |
//! | [`riff`] | [`Riff`] and [`apply`]: the one function that changes a riff, with one arm for each kind of record. |
//! | [`presence`] | [`Presence`], the session in memory, and `Presence::applied`: what a record changes in memory. |
//! | [`view`] | [`View`]: one copy of the riff with the presence, read only. `handle` and each query read it. |
//! | [`command`] | The trait [`Command`] with `handle`, and [`Now`]. |
//! | [`snapshot`] | [`Snapshot`]: the parts of each group in one checkpoint. |
//! | `state/rules.rs` | The given/when/then tests of `handle` and `apply`. |
//!
//! Each group of commands has one file with its part of the riff, its
//! `apply` arms, its part of the checkpoint and its command types:
//!
//! | Group | File | Part of the riff | Commands |
//! |---|---|---|---|
//! | sessions | [`sessions`] | [`Sessions`](sessions::Sessions) | [`Register`], [`Start`], [`End`] |
//! | threads | [`threads`] | [`Threads`](threads::Threads) | [`Join`], [`Leave`], [`Post`], [`Announce`] |
//! | work | [`work`] | [`Work`](work::Work) | [`Claim`], [`Release`], [`ReleaseFor`], [`Lead`] |
//! | the riff | [`the_riff`] | [`TheRiff`](the_riff::TheRiff) | [`SetRiff`], [`SetIdle`], [`Forget`] |
//!
//! A new command is a type in the file of its group. A new kind of
//! record is one arm in [`apply`] and one method of the part that it
//! changes.
//!
//! # Event sourcing
//!
//! Each change that must not be lost is a [`Record`] in one log (see
//! [`riff_core::record`] and [`crate::log`]):
//!
//! - [`State::handle`] checks a [`Command`] against the state, and gives
//!   the changes, or the reason for a refusal. It changes nothing and
//!   does no I/O.
//! - [`apply`] changes a [`Riff`] for one record. It does no I/O, reads
//!   no clock, and does not fail. A record that the state cannot take
//!   changes nothing, and the server logs a warning. The live path and
//!   the replay use the same `apply`.
//! - Each call of the state runs `handle`, gives each change its
//!   position and time, puts the records in the queue, and applies them
//!   to the pending copy.
//! - The state keeps two copies of the [`Riff`] (01M3T4115BF1F0JFHYMK0WRKCX).
//!   `handle` checks against
//!   the pending copy, which has each record in the queue. Each view
//!   (`who`, `threads`, `read`, the wakes) uses the written copy, which
//!   has only the records whose chunk is written. [`State::written`]
//!   applies the records of a chunk after its write. So nobody sees a
//!   record that is not in the log.
//! - A [`State::default`] has no writer: each record counts as written
//!   at once. Tests and examples use it. The server makes its state with
//!   [`State::with_writer`] or [`State::replay`].
//!
//! # Rules
//!
//! - Each call records the session as seen at `now`. A call from a
//!   session that the server does not know makes it, in the place from
//!   its URI. Only [`State::register`] changes the place of a known
//!   session (R55, R64). A person has no session ID, so each call of a
//!   person gives it the place of that call (01M3MWW8KYJ3ZV91X22RBSAF33).
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
//!   takes any thread by name, except a direct thread of others
//!   ([`may_read`]).
//! - `read` returns one page of the messages after the cursor, then moves
//!   the cursor to the end of the page ([`State::read_page`]).
//! - A thread keeps its last [`KEEP_MESSAGES`] messages
//!   (01M3TBZBT7MME9BG1RWX5SZAZ6).
//! - [`State::forget_expired`] forgets each session with no sign of life
//!   for [`SESSION_EXPIRY`] (01M3TBZBZVH907QD359AB8TBSX): a
//!   [`Change::SessionForgotten`] record drops the session, its read
//!   cursors, its memberships, its claims, its lead, and each direct
//!   thread whose two sessions are gone.
//! - When a watch starts, [`State::missed`] gives one wake for the
//!   newest unread message that woke the session (R49).
//! - `who` lists each session with the time since its last call. A
//!   keep-alive is not a call (R163).
//! - A session is gone when it ended ([`State::end`]), or when it had no
//!   call and no keep-alive ([`State::alive`]) for [`GONE`] (R164,
//!   R206). An open watch stream is no sign of life: a front end can
//!   hold the stream of a dead client open, and the server does not see
//!   the death (01M3WG240PNMQYZ7TX6Z7ZF6M9). `riff watch` sends a
//!   keep-alive while it runs. `who` hides a gone session, unless the
//!   caller asks for all sessions. A gone session matches no selector,
//!   and a direct message to it fails (R206).
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
//! - [`State::announce`] posts a note or a message of the riff server
//!   itself. The server is not a session: it does not show in `who`
//!   (01M3N7K4BC1RPZKQ1XNDTBRPGF).
//! - A post of kind [`Kind::Note`] wakes no
//!   session. Each session that its selectors match still joins the
//!   thread, so it sees the note at its next `read`
//!   (01M3JPMQE6S7YM4HPEVGXWK7ET).
//! - In a thread, a selector with `lead=true` that matches no live
//!   session matches each live session with no claim that its other
//!   fields match. So a verify request to the lead of the author reaches
//!   a free session when the lead is gone (01M3JY1TBPQHH6WPPBTF42T64H).
//! - A claim is free, or held. A held claim goes back to free when its
//!   holder releases it or ends, when the lead of its user releases it
//!   ([`State::release_for`], 01M3WG243BW7P6E1ME0DFNQF8C), or when the
//!   last sign of life of the holder is more than [`CLAIM_GRACE`] ago.
//!   A claim of a free item succeeds.
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
//! - [`State::stop_idle_workers`] asks each idle worker past the limit
//!   to stop. The reply to its keep-alive carries the ask. A call of the
//!   worker takes it back (01M3Q5A0NKY1FCS0YH6N6YD3GN).
//! - While the riff is paused, a claim fails. A held claim stays, and a
//!   release works (01M3JCG3WBHDF0ZWM06XV94ZDC).
//! - A signed post with the payload of a message in the thread is a
//!   copy, and is refused. So a session gets each request of its lead
//!   once (01M3JEJVXXEPPNGT3FY4ZSFCWZ).
//!
//! The state does no I/O and reads no clock. The caller passes `now`.
//!
//! # After a replay
//!
//! [`State::replay`] makes a state from the records of the log, and
//! [`State::load`] from a checkpoint and the records after it. The
//! sessions and their statuses are in memory, so a replay has none of
//! them. The read cursors come from the checkpoint. It makes each
//! session that a record names, in the place of the last record that
//! names it. Each such session is
//! gone until it calls. Its claims and its lead hold for [`CLAIM_GRACE`]
//! from the replay, unless it comes back (R125).
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
//! assert!(state.claim(&mike, &thread, "issue-12", now).unwrap().0);
//! assert!(!state.claim(&brett, &thread, "issue-12", now).unwrap().0);
//!
//! // Each change is a record. A replay of the records gives the same
//! // state.
//! let records = state.take_queue();
//! let replayed = State::replay(records, now, 0);
//! assert!(replayed.same_log_state(&state));
//! # Ok::<(), riff_core::name::NameError>(())
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use riff_core::name::{SessionUri, ThreadName, Who};
use riff_core::record::{Change, Claimed, Posted, Record};
use riff_core::selector::Selector;
use riff_core::wire::{
    AliveReply, ClaimReply, Freed, Idle, Keys, Kind, LeadReply, Message, Post, RiffReply,
    RiffState, SessionInfo, SessionState, Status, StatusInfo, Tailed, ThreadInfo, Wake,
};

pub mod command;
pub mod presence;
pub mod riff;
pub mod sessions;
pub mod snapshot;
pub mod the_riff;
pub mod threads;
pub mod view;
pub mod work;

pub use command::{Command, Now};
pub use presence::Presence;
pub use riff::{Riff, apply};
pub use sessions::{End, Register, Start};
pub use snapshot::Snapshot;
pub use the_riff::{Forget, SetIdle, SetRiff};
pub use threads::{Announce, Join, Leave, may_read};
pub use view::View;
pub use work::{Claim, Lead, Release, ReleaseFor, released_for};

use presence::{Session, SetStatus};
use threads::wake;

/// A claim stays with a session this long after the session stops (R9).
pub const CLAIM_GRACE: Duration = Duration::from_secs(5 * 60);

/// A session with no call and no keep-alive for this long is gone
/// (R206). `riff mcp` and `riff watch` send a keep-alive each
/// [`riff_core::wire::ALIVE_EVERY`].
pub const GONE: Duration = Duration::from_secs(3 * 60);

/// A session with no sign of life for this long is forgotten
/// ([`Change::SessionForgotten`], 01M3TBZBZVH907QD359AB8TBSX).
pub const SESSION_EXPIRY: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Memory and the checkpoint keep the last this many messages of each
/// thread. Nobody reads an older message (01M3TBZBT7MME9BG1RWX5SZAZ6).
///
/// The number is a constant of the format: [`apply`] uses it, so a
/// change of it changes the state that a replay gives. Such a change
/// follows the rules for a change of a record
/// (01M3T4111PFM0C6KPREWFS9EQQ, 01M3WNQQWA7XGK4Y9ET8HJZ8NN).
pub const KEEP_MESSAGES: usize = 200;

/// `read` gives at most this many messages, and a cursor for the next
/// page (01M3TBZBX140GJWCV5GZ73Q5Z5).
pub const PAGE: usize = 50;

/// All state of one `riff-server`. See the module docs for the rules.
pub struct State {
    /// The state of the records whose chunk is written.
    written: Riff,
    /// The written state and each record in the queue.
    pending: Riff,
    /// Each record that waits for its chunk.
    queue: Vec<Record>,
    /// The time of the call that made each record in the queue, by the
    /// position of the record.
    made: BTreeMap<u64, Instant>,
    /// False: each record counts as written at once.
    writer: bool,
    /// An instant and the same time in milliseconds since the Unix
    /// epoch. A record gets its time from it.
    clock: Option<(Instant, u64)>,
    /// The state in memory.
    presence: Presence,
    /// The last position of the log, when the last records of the log
    /// are of a kind that this build skipped ([`State::continue_after`]).
    skipped_to: u64,
}

impl Default for State {
    /// An empty state with no writer: each record counts as written at
    /// once.
    fn default() -> Self {
        State {
            written: Riff::default(),
            pending: Riff::default(),
            queue: Vec::new(),
            made: BTreeMap::new(),
            writer: false,
            clock: None,
            presence: Presence::default(),
            skipped_to: 0,
        }
    }
}

/// An idle worker that the server asks to stop
/// ([`State::stop_idle_workers`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stopping {
    /// The worker.
    pub worker: SessionUri,
    /// Its host.
    pub host: String,
    /// The time since its last call.
    pub idle: Duration,
}

/// What a new message causes: sessions to wake and a line for `tail`.
/// The server sends it after the chunk of the message is written.
#[derive(Clone, Debug)]
pub struct Delivery {
    /// Each session to wake, with its event.
    pub wakes: Vec<(Who, Wake)>,
    /// Each woken session, for the sender.
    pub woken: Vec<Who>,
    /// Each selector that matched no session.
    pub unmatched: Vec<Selector>,
    /// The event for the `tail` streams of the thread.
    pub tailed: Tailed,
}

/// The sizes of the state, from [`State::counts`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub sessions: u64,
    /// The number of read cursors.
    pub cursors: u64,
    pub threads: u64,
}

/// One page of messages from [`State::read_page`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub messages: Vec<Message>,
    /// The seq of the last message of the page, when more messages
    /// follow.
    pub next: Option<u64>,
}

impl State {
    /// An empty state whose records wait in the queue until
    /// [`State::written`]. `now` and `now_ms` are the same time: an
    /// instant, and milliseconds since the Unix epoch. A record gets its
    /// time from them.
    pub fn with_writer(now: Instant, now_ms: u64) -> State {
        State {
            writer: true,
            clock: Some((now, now_ms)),
            ..State::default()
        }
    }

    /// Makes a state from the records of the log, with a writer. See
    /// "After a replay" in the module docs.
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
    /// let replayed = State::replay(state.take_queue(), now, 1_000);
    /// // The session is gone until it calls, and it still holds its claim.
    /// assert!(replayed.who(now, 1_000, false).is_empty());
    /// assert_eq!(replayed.uri(mike.who(), now).claims(), ["issue-12"]);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn replay(records: impl IntoIterator<Item = Record>, now: Instant, now_ms: u64) -> State {
        State::load(None, records, now, now_ms)
    }

    /// Makes a state from a checkpoint and the records of the log after
    /// it, with a writer. With no checkpoint, it is [`State::replay`].
    /// The read cursors come from the checkpoint. Each session that the
    /// log names counts as seen at the later of its last record and its
    /// last call before the checkpoint.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_server::state::State;
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&mike, now);
    /// let first = state.take_queue();
    /// let snapshot = State::replay(first.clone(), now, 0).snapshot(now, 0);
    /// state.join(&mike, &"design".parse()?, now);
    /// let rest = state.take_queue();
    ///
    /// // A start from the checkpoint and the records after it gives the
    /// // state of a full replay.
    /// let loaded = State::load(Some(snapshot), rest.clone(), now, 0);
    /// let full = State::replay(first.into_iter().chain(rest), now, 0);
    /// assert!(loaded.same_log_state(&full));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn load(
        snapshot: Option<Snapshot>,
        records: impl IntoIterator<Item = Record>,
        now: Instant,
        now_ms: u64,
    ) -> State {
        let mut state = State::with_writer(now, now_ms);
        let mut seen = BTreeMap::new();
        if let Some(snapshot) = snapshot {
            let (riff, cursors, seen_ms) = snapshot.into_parts();
            state.pending = riff.clone();
            state.written = riff;
            state.presence.cursors = cursors;
            seen = seen_ms;
        }
        for record in records {
            apply(&mut state.pending, &record);
            state.apply_written(&record, None);
        }
        state.presence.riff_changed = Some(now);
        state.presence.loaded = Some(now);
        state.sessions_of_the_log(&seen, now);
        state
    }

    /// Applies the records that came after the load: the old instance
    /// wrote them between the load and the lease of this instance
    /// (01M3THEE08ZKV8WGHDSVWV69ZE). Call it before the first call of a
    /// session. The claim timer of each session still starts at the
    /// load.
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::RiffState;
    /// use riff_server::state::State;
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let now = Instant::now();
    /// let mut old = State::default();
    /// old.register(&mike, now);
    /// old.riff(&mike, Some(RiffState::Running), now).unwrap();
    /// let mut new = State::replay(old.take_queue(), now, 1_000);
    ///
    /// // The old instance takes a claim after the load of the new one.
    /// let thread = mike.default_thread().unwrap();
    /// old.claim(&mike, &thread, "issue-12", now).unwrap();
    /// new.catch_up(old.take_queue());
    /// let later = now + Duration::from_secs(60);
    /// assert_eq!(new.uri(mike.who(), later).claims(), ["issue-12"]);
    /// assert!(new.same_log_state(&old));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn catch_up(&mut self, records: impl IntoIterator<Item = Record>) {
        for record in records {
            apply(&mut self.pending, &record);
            self.apply_written(&record, None);
        }
        let Some(loaded) = self.presence.loaded else {
            return;
        };
        let seen = self
            .presence
            .sessions
            .iter()
            .filter_map(|(who, session)| Some((who.clone(), session.seen_before_load?)))
            .collect();
        self.sessions_of_the_log(&seen, loaded);
    }

    /// Makes each session that the log names, as it is after a replay at
    /// `loaded`: gone until it calls, in the place of the last record that
    /// names it. `seen` has the last call of each session that is known
    /// from before.
    fn sessions_of_the_log(&mut self, seen: &BTreeMap<Who, u64>, loaded: Instant) {
        for (who, known) in &self.written.sessions().known {
            let at_ms = seen.get(who).copied().unwrap_or(0).max(known.at_ms);
            let session = Session {
                seen_before_load: Some(at_ms),
                alive: None,
                ..Session::new(known.uri.place().clone(), loaded)
            };
            self.presence.sessions.insert(who.clone(), session);
        }
    }

    /// The written state, the read cursors and the last call of each
    /// session, for a checkpoint. The caller encodes it outside the lock.
    pub fn snapshot(&self, now: Instant, now_ms: u64) -> Snapshot {
        let sessions = &self.presence.sessions;
        let seen = |who: &Who| sessions.get(who).map_or(0, |s| s.seen_ms(now, now_ms));
        Snapshot::new(self.written_position(), &self.written, &self.presence, seen)
    }

    /// Applies a record to the written copy, and then to the presence
    /// ([`Presence::applied`]). `at` is the time of the call that made
    /// the record, or `None` in a replay.
    fn apply_written(&mut self, record: &Record, at: Option<Instant>) {
        let lost = match &record.change {
            Change::Claimed(claimed) => self
                .written
                .work()
                .holder(&claimed.thread, &claimed.item)
                .cloned(),
            _ => None,
        };
        apply(&mut self.written, record);
        self.presence
            .applied(record, &self.written, lost.as_ref(), at);
    }

    /// True when the state that the log gives is the same in both
    /// states: the written copies.
    pub fn same_log_state(&self, other: &State) -> bool {
        self.written == other.written && self.written_position() == other.written_position()
    }

    /// Puts the next record after the position `last` of the log, also
    /// when the last records of the log are of a kind that this build
    /// skipped (01M3T4111PFM0C6KPREWFS9EQQ). So a new record never takes
    /// the position of a skipped one.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::state::State;
    ///
    /// let mut state = State::replay([], Instant::now(), 0);
    /// state.continue_after(7);
    /// assert_eq!(state.position(), 7);
    /// ```
    pub fn continue_after(&mut self, last: u64) {
        self.skipped_to = self.skipped_to.max(last);
    }

    /// The position of the last record: in the queue, or written.
    pub fn position(&self) -> u64 {
        self.pending.position().max(self.skipped_to)
    }

    /// The position of the last written record.
    pub fn written_position(&self) -> u64 {
        self.written.position().max(self.skipped_to)
    }

    /// The numbers of sessions, read cursors and threads of the written
    /// state, for the facts of `riff server`
    /// (01M3TJWJ12WEDCXW3W0529KRP2).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_server::state::State;
    ///
    /// let a: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a".parse()?;
    /// let mut state = State::default();
    /// state.register(&a, Instant::now());
    /// let counts = state.counts();
    /// assert_eq!((counts.sessions, counts.threads), (1, 1));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn counts(&self) -> Counts {
        Counts {
            sessions: self.presence.sessions.len() as u64,
            cursors: self.presence.cursors.len() as u64,
            threads: self.written.threads().by_name.len() as u64,
        }
    }

    /// Takes each record in the queue, for the writer.
    pub fn take_queue(&mut self) -> Vec<Record> {
        std::mem::take(&mut self.queue)
    }

    /// Applies the records of a written chunk to the written copy. A
    /// state with no writer counts each record as written at once, so it
    /// skips them.
    pub fn written(&mut self, records: &[Record]) {
        if self.writer {
            for record in records {
                let at = self.made.remove(&record.position);
                self.apply_written(record, at);
            }
        }
    }

    /// Checks a command of `me` against the pending copy
    /// ([`Command::handle`]). Gives the changes, or why it is refused. It
    /// changes nothing.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::record::Change;
    /// use riff_server::state::{Claim, State};
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&mike, now);
    /// let thread = mike.default_thread().unwrap();
    /// let claim = Claim { thread, item: "issue-12".into() };
    ///
    /// // A new riff is paused, so a claim is refused.
    /// assert!(state.handle(&mike, &claim, now).unwrap_err().contains("paused"));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn handle<C: Command>(
        &self,
        me: &SessionUri,
        command: &C,
        now: Instant,
    ) -> Result<Vec<Change>, String> {
        let now = self.now(now);
        let (changes, _) = command.handle(me, &self.pending_view(), now)?;
        Ok(changes)
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
        let sessions = &self.presence.sessions;
        if sessions.contains_key(who) {
            return Ok(());
        }
        match sessions.keys().find(|known| known.session() == Some(id)) {
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
        if let Some(session) = self.presence.sessions.get_mut(me.who()) {
            session.place = me.place().clone();
        }
        self.run(me, &Register, now)
            .expect("a register is never refused");
    }

    /// Records that a watch stream opened. The session is live.
    pub fn watch_started(&mut self, me: &SessionUri, now: Instant) {
        self.arrive(me, now);
        if let Some(session) = self.presence.sessions.get_mut(me.who()) {
            session.watchers += 1;
        }
    }

    /// Records that a watch stream closed. The session is idle when it
    /// has no open stream. A wake ends the watch of a worker, so the end
    /// takes back an ask to stop, as a call does
    /// (01M3Q5A0NKY1FCS0YH6N6YD3GN). The close is no sign of life: the
    /// stream of a dead client can close a long time after its death
    /// (01M3WG240PNMQYZ7TX6Z7ZF6M9).
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use riff_core::name::SessionUri;
    /// use riff_server::state::State;
    ///
    /// let lead: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=lead".parse()?;
    /// let w1: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w1".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&lead, now);
    /// state.set_idle(Some(0), None, now);
    /// state.watch_started(&w1, now);
    /// state.worker(w1.who(), true);
    ///
    /// let later = now + Duration::from_secs(80);
    /// assert_eq!(state.stop_idle_workers(later).len(), 1);
    /// state.watch_ended(w1.who(), later);
    /// assert!(!state.alive(&w1, later).stop);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn watch_ended(&mut self, who: &Who, now: Instant) {
        if let Some(session) = self.presence.sessions.get_mut(who) {
            session.watchers = session.watchers.saturating_sub(1);
            session.stopping = false;
            if !session.gone(now) {
                session.last_seen = now;
                session.seen_before_load = None;
            }
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
    pub fn alive(&mut self, me: &SessionUri, now: Instant) -> AliveReply {
        let who = me.who();
        let Some(session) = self.presence.sessions.get_mut(who) else {
            self.arrive(me, now);
            return AliveReply::default();
        };
        session.live(now);
        AliveReply {
            stop: session.stopping,
        }
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
    /// assert!(state.claim(&brett, &thread, "issue-12", now).unwrap().0);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn end(&mut self, me: &SessionUri, now: Instant) {
        let who = me.who();
        if !self.presence.sessions.contains_key(who) {
            return;
        }
        self.run(me, &End, now).expect("an end is never refused");
        if let Some(session) = self.presence.sessions.get_mut(who) {
            session.ended = true;
            session.last_seen = now;
            session.seen_before_load = None;
            session.claims_changed = now;
        }
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
        self.arrive(me, now);
        let (changes, ()) = self.run(me, &Start, now).expect("a start is never refused");
        changes
            .into_iter()
            .filter_map(|change| match change {
                Change::Released(Claimed { thread, item, .. }) => Some(Freed { thread, item }),
                _ => None,
            })
            .collect()
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
        self.presence
            .sessions
            .iter()
            .filter(|(_, session)| all || !session.gone(now))
            .map(|(who, session)| self.info(who, session, now, now_ms))
            .collect()
    }

    /// Only the session `who`, as [`State::who`] shows it with `all`,
    /// or None when the state does not know it. It changes nothing: it
    /// is not a call of `who` (01M3T5GFVS8NMA992KHZN4VE17).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_server::state::State;
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let ada: SessionUri = "riff://ada@thelio/como-technologies/riff?session=b7d0".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&mike, now);
    /// state.register(&ada, now);
    ///
    /// let me = state.me(mike.who(), now, 0).unwrap();
    /// assert_eq!(me.uri.who(), mike.who());
    /// assert!(me.uri.lead());
    /// let bob: SessionUri = "riff://bob@thelio/como-technologies/riff?session=c8e1".parse()?;
    /// assert!(state.me(bob.who(), now, 0).is_none());
    /// assert_eq!(state.who(now, 0, true).len(), 2);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn me(&self, who: &Who, now: Instant, now_ms: u64) -> Option<SessionInfo> {
        let session = self.presence.sessions.get(who)?;
        Some(self.info(who, session, now, now_ms))
    }

    /// The session `who` as `who` and `me` show it.
    fn info(&self, who: &Who, session: &Session, now: Instant, now_ms: u64) -> SessionInfo {
        let uri = self.uri(who, now);
        let live = session.watching(now);
        let status = session.status.as_ref().map(|s| StatusInfo {
            status: s.status.clone(),
            age_secs: now_ms.saturating_sub(s.set_ms) / 1000,
            stale: s.before(Some(session.claims_changed)) || s.before(self.presence.riff_changed),
        });
        let blocked = status
            .as_ref()
            .is_some_and(|s| s.status.blocked.is_some() && !s.stale);
        let state = SessionState::of(
            live,
            self.written.the_riff().state == RiffState::Paused,
            blocked,
            !uri.claims().is_empty(),
        );
        SessionInfo {
            uri,
            live,
            idle_secs: now_ms.saturating_sub(session.seen_ms(now, now_ms)) / 1000,
            status,
            worker: session.worker,
            stopping: session.stopping,
            claims_secs: now
                .saturating_duration_since(session.claims_changed)
                .as_secs(),
            state: Some(state),
        }
    }

    /// Records whether the session `who` is a worker. A register call
    /// sets it (01M3NT4M159EHN5W8JRTQ417N4).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_server::state::State;
    ///
    /// let w1: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w1".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&w1, now);
    /// state.worker(w1.who(), true);
    /// assert!(state.who(now, 0, false)[0].worker);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn worker(&mut self, who: &Who, worker: bool) {
        if let Some(session) = self.presence.sessions.get_mut(who) {
            session.worker = worker;
        }
    }

    /// The settings of idle workers (01M3Q5A0TF9K49V8Z1ZY9NDF74).
    pub fn idle(&self) -> Idle {
        self.written.the_riff().idle
    }

    /// Sets the settings of idle workers: each value that is `Some`
    /// (01M3Q5A0TF9K49V8Z1ZY9NDF74). The caller checks who may set them.
    /// It gives the settings with the change.
    pub fn set_idle(
        &mut self,
        per_host: Option<u16>,
        after_secs: Option<u64>,
        now: Instant,
    ) -> Idle {
        let command = SetIdle {
            per_host,
            after_secs,
        };
        let server = crate::owner::server_uri();
        self.run(&server, &command, now)
            .expect("a change of the settings is never refused");
        self.pending.the_riff().idle
    }

    /// Asks each idle worker past the limit to stop, and gives each
    /// (01M3Q5A0NKY1FCS0YH6N6YD3GN). An idle worker is a live worker
    /// that is not a lead, holds no claim, was not asked before, and made
    /// no call for a time. On each host of each user, the server keeps
    /// the [`Idle::per_host`] idle workers with the shortest idle time. It
    /// asks each other one that is idle for [`Idle::after_secs`] or more.
    /// A later call of the worker takes the ask back: a worker that
    /// claims work goes on.
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use riff_core::name::SessionUri;
    /// use riff_server::state::State;
    ///
    /// let worker = |id: &str| -> SessionUri {
    ///     format!("riff://mike@pangolin/como-technologies/riff?session={id}").parse().unwrap()
    /// };
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&worker("lead"), now);
    /// for (id, at) in [("w1", 10), ("w2", 5), ("w3", 0)] {
    ///     let w = worker(id);
    ///     state.watch_started(&w, now + Duration::from_secs(at));
    ///     state.worker(w.who(), true);
    /// }
    /// let later = now + Duration::from_secs(80);
    /// let stopped: Vec<_> = state.stop_idle_workers(later).iter().map(|s| s.worker.to_string()).collect();
    /// assert_eq!(stopped, [worker("w2").to_string(), worker("w3").to_string()]);
    /// assert!(state.stop_idle_workers(later).is_empty(), "asks once");
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn stop_idle_workers(&mut self, now: Instant) -> Vec<Stopping> {
        let idle = self.written.the_riff().idle;
        let after = Duration::from_secs(idle.after_secs);
        let mut workers: BTreeMap<(String, String), Vec<(Duration, Who)>> = BTreeMap::new();
        {
            let view = self.written_view();
            for (who, session) in &self.presence.sessions {
                let free = session.worker
                    && session.watching(now)
                    && !session.stopping
                    && !view.holds_claim(who)
                    && !view.is_lead(who, now);
                if free {
                    let key = (who.user().to_owned(), session.place.host().to_owned());
                    let time = now.saturating_duration_since(session.last_seen);
                    workers.entry(key).or_default().push((time, who.clone()));
                }
            }
        }
        let mut stopping = Vec::new();
        for ((_, host), mut list) in workers {
            list.sort();
            for (time, who) in list.into_iter().skip(usize::from(idle.per_host)) {
                if time < after {
                    continue;
                }
                if let Some(session) = self.presence.sessions.get_mut(&who) {
                    session.stopping = true;
                }
                stopping.push(Stopping {
                    worker: self.uri(&who, now),
                    host: host.clone(),
                    idle: time,
                });
            }
        }
        stopping
    }

    /// Whether a session of `user` is live, and the seconds since the
    /// last call of a session of `user`, gone sessions too. `None` when
    /// the server knows no session of `user` (01M3NT4M3A4E3K5S2NM7MS6PQD).
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
    /// let hour = now + Duration::from_secs(3600);
    /// assert_eq!(state.seen("mike", hour, 3_600_000), Some((false, 3600)));
    /// assert_eq!(state.seen("ann", hour, 3_600_000), None);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn seen(&self, user: &str, now: Instant, now_ms: u64) -> Option<(bool, u64)> {
        self.presence
            .sessions
            .iter()
            .filter(|(who, _)| who.user() == user)
            .map(|(_, s)| {
                (
                    s.watching(now),
                    now_ms.saturating_sub(s.seen_ms(now, now_ms)) / 1000,
                )
            })
            .reduce(|(a_live, a_idle), (b_live, b_idle)| (a_live || b_live, a_idle.min(b_idle)))
    }

    /// The URI of a session now, in the written copy: its place, whether
    /// it is the lead, and the claims that it holds.
    pub fn uri(&self, who: &Who, now: Instant) -> SessionUri {
        self.written_view().uri(who, now)
    }

    /// Makes `me` the lead of its user in its repository. It replaces
    /// the old lead (R177). It gives `me` and the old lead, if another
    /// session was the lead. [`State::lead_reply`] makes the reply after
    /// the write.
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
    /// let (me, old) = state.lead(&second, now).unwrap();
    /// let reply = state.lead_reply(&me, old.as_ref(), now);
    /// assert!(reply.lead.lead());
    /// assert_eq!(reply.replaced.unwrap().who(), first.who());
    /// assert!(!state.uri(first.who(), now).lead());
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn lead(&mut self, me: &SessionUri, now: Instant) -> Result<(Who, Option<Who>), String> {
        let who = self.arrive(me, now);
        let (_, old) = self.run(me, &Lead, now)?;
        Ok((who, old))
    }

    /// The reply to [`State::lead`], from the written copy.
    pub fn lead_reply(&self, me: &Who, old: Option<&Who>, now: Instant) -> LeadReply {
        LeadReply {
            lead: self.uri(me, now),
            replaced: old.map(|old| self.uri(old, now)),
        }
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
        self.arrive(me, now);
        let Some(set) = set else {
            return Ok(RiffReply {
                state: self.written.the_riff().state,
                changed: false,
            });
        };
        let changed = !self.run(me, &SetRiff(set), now)?.0.is_empty();
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
        if let Some(session) = self.presence.sessions.get_mut(&who) {
            session.status = Some(SetStatus {
                status,
                set_ms: now_ms,
                set: now,
            });
        }
        Ok(())
    }

    /// The threads that `me` joined, with its unread counts. A count
    /// leaves out the own posts of `me` (01M3JPK82PN4F706MCHDH771MW).
    pub fn threads(&mut self, me: &SessionUri, now: Instant) -> Vec<ThreadInfo> {
        let who = self.arrive(me, now);
        let who = &who;
        let view = self.written_view();
        view.riff
            .threads()
            .by_name
            .iter()
            .filter(|(_, t)| t.members.contains(who))
            .map(|(thread, t)| {
                let read = self.presence.cursor(who, thread);
                ThreadInfo {
                    thread: thread.clone(),
                    members: t.members.iter().map(|m| view.uri(m, now)).collect(),
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
        self.written
            .threads()
            .by_name
            .iter()
            .filter(|(thread, _)| may_read(who, thread))
            .filter_map(|(thread, t)| {
                let read = self.presence.cursor(who, thread);
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

    /// Forgets each session with no sign of life for [`SESSION_EXPIRY`]
    /// ([`Forget`]). A timer of the server calls it. Gives the
    /// number of forgotten sessions.
    pub fn forget_expired(&mut self, now: Instant) -> usize {
        let server = crate::owner::server_uri();
        self.run(&server, &Forget, now)
            .map_or(0, |(changes, ())| changes.len())
    }

    /// Adds a session to a thread. It makes the thread if it is new.
    pub fn join(&mut self, me: &SessionUri, thread: &ThreadName, now: Instant) {
        self.arrive(me, now);
        self.run(me, &Join(thread.clone()), now)
            .expect("a join is never refused");
    }

    /// Removes a session from a thread. It is no longer the lead there.
    pub fn leave(&mut self, me: &SessionUri, thread: &ThreadName, now: Instant) {
        self.arrive(me, now);
        self.run(me, &Leave(thread.clone()), now)
            .expect("a leave is never refused");
    }

    /// Adds a message to a thread and wakes each session that `to`
    /// selects (R51). With no thread, the post is a direct message (R62).
    /// `at_ms` is the time of the post, in milliseconds since the Unix
    /// epoch. The message keeps the payload and the signature of the post
    /// unchanged (R198). The caller checks the signature.
    ///
    /// The signature covers the lead mark of `me`. So the sender of a
    /// signed message has `lead=true` only when `me` has it, and a signed
    /// post with the lead mark from a session that is not the lead is
    /// refused (R198).
    ///
    /// A signed post with the payload of a message in the thread is a
    /// copy, and is refused. So a session gets each request of its lead
    /// once (01M3JEJVXXEPPNGT3FY4ZSFCWZ).
    pub fn post(&mut self, post: Post, now: Instant, at_ms: u64) -> Result<Delivery, String> {
        let me = post.me.clone();
        self.arrive(&me, now);
        let post = Post {
            at_ms: Some(at_ms),
            ..post
        };
        let (changes, unmatched) = self.run(&me, &post, now)?;
        Ok(State::delivery(changes, unmatched))
    }

    /// Posts a note or a message of the riff server itself
    /// (01M3N7K4BC1RPZKQ1XNDTBRPGF). `me` is the URI of the server, for
    /// example [`crate::owner::server_uri`]. It is not a session: it does
    /// not arrive, and does not show in `who`. The post has no signature.
    /// A post with no thread is a direct message, as with
    /// [`State::post`].
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::selector::Selector;
    /// use riff_core::wire::Kind;
    /// use riff_server::owner::server_uri;
    /// use riff_server::state::State;
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&mike, now);
    ///
    /// let to = vec![Selector::session("a6cf")];
    /// let delivery = state.announce(&server_uri(), None, to, "hello", Kind::Message, now, 0).unwrap();
    /// assert_eq!(&delivery.wakes[0].0, mike.who());
    /// assert_eq!(delivery.tailed.message.from, server_uri());
    /// assert_eq!(state.who(now, 0, true).len(), 1, "the server is not a session");
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub fn announce(
        &mut self,
        me: &SessionUri,
        thread: Option<ThreadName>,
        to: Vec<Selector>,
        body: &str,
        kind: Kind,
        now: Instant,
        at_ms: u64,
    ) -> Result<Delivery, String> {
        let announce = Announce {
            thread,
            to,
            body: body.to_owned(),
            kind,
            at_ms,
        };
        let (changes, unmatched) = self.run(me, &announce, now)?;
        Ok(State::delivery(changes, unmatched))
    }

    /// The live lead of `user` in each repository, sorted: a lead that
    /// holds, and that is not gone.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_server::state::{GONE, State};
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&mike, now);
    /// assert_eq!(state.live_leads("mike", now), [mike.who().clone()]);
    /// assert!(state.live_leads("brett", now).is_empty());
    /// assert!(state.live_leads("mike", now + GONE).is_empty());
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn live_leads(&self, user: &str, now: Instant) -> Vec<Who> {
        let view = self.written_view();
        let leads: BTreeSet<Who> = view
            .riff
            .work()
            .leads
            .keys()
            .filter(|key| key.0 == user)
            .filter_map(|key| view.lead_of(key, now))
            .filter(|who| !view.gone(who, now))
            .cloned()
            .collect();
        leads.into_iter().collect()
    }

    /// True when `user` shows a sign of life at `now`: a session of the
    /// user that is not gone, or a call of the user at `since` or later,
    /// also a call as a person (01M3Q546335NBTKG5BHQ27QC93). A person
    /// entry is not a session, so only its calls count.
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use riff_core::name::SessionUri;
    /// use riff_server::state::{GONE, State};
    ///
    /// let lead: SessionUri = "riff://ada@thelio/como-technologies/riff?session=a6cf".parse()?;
    /// let other: SessionUri = "riff://ada@pangolin/como-technologies/riff?session=b7d0".parse()?;
    /// let person: SessionUri = "riff://ada@pangolin".parse()?;
    /// let start = Instant::now();
    /// let mut state = State::default();
    /// state.register(&lead, start);
    /// state.register(&other, start);
    /// state.end(&lead, start);
    /// assert!(state.present("ada", start, start), "another live session");
    /// assert!(!state.present("bob", start, start));
    ///
    /// let later = start + GONE;
    /// assert!(!state.present("ada", later, later), "each session is gone");
    /// state.register(&person, later);
    /// assert!(state.present("ada", later, later), "a call as a person");
    /// let after = later + Duration::from_secs(1);
    /// assert!(!state.present("ada", after, after), "no call since the last check");
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn present(&self, user: &str, since: Instant, now: Instant) -> bool {
        self.presence
            .sessions
            .iter()
            .filter(|(who, _)| who.user() == user)
            .any(|(who, session)| {
                (who.session().is_some() && !session.gone(now))
                    || session.alive.is_some_and(|alive| alive >= since)
                    || session.last_seen >= since
            })
    }

    /// The thread of each repository of the riff, sorted: the repository
    /// of each known session, also a gone one (01M3MN14ZCTRVD3T455P6TFK1B).
    pub fn repositories(&self) -> Vec<ThreadName> {
        let threads: BTreeSet<ThreadName> = self
            .presence
            .sessions
            .values()
            .filter_map(|s| s.place.default_thread())
            .collect();
        threads.into_iter().collect()
    }

    /// Returns unread messages (or all of them) and marks them as read,
    /// with no limit. See [`State::read_page`].
    pub fn read(
        &mut self,
        me: &SessionUri,
        thread: &ThreadName,
        all: bool,
        now: Instant,
    ) -> Result<Vec<Message>, String> {
        self.read_page(me, thread, all, None, usize::MAX, now)
            .map(|page| page.messages)
    }

    /// Returns at most `limit` unread messages (or of all the kept
    /// messages after the seq `after`), and marks them as read. The
    /// unread messages leave out the own posts of `me`; `all` gives them
    /// (01M3JPK82PN4F706MCHDH771MW). When more messages follow, the page
    /// has the seq of its last message in [`Page::next`]: a new unread
    /// read gives the next page, and a read of all gives it with `after`
    /// set to that seq. A direct thread of two other sessions is not
    /// found ([`may_read`]).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::Post;
    /// use riff_server::state::State;
    ///
    /// let ann: SessionUri = "riff://ann@heron/acme/app?session=a1".parse()?;
    /// let bob: SessionUri = "riff://bob@kite/acme/app?session=b1".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// let thread = ann.default_thread().unwrap();
    /// for body in ["one", "two", "three"] {
    ///     state.post(Post::new(&ann, Some(thread.clone()), vec![], body), now, 0).unwrap();
    /// }
    /// let page = state.read_page(&bob, &thread, false, None, 2, now).unwrap();
    /// assert_eq!((page.messages.len(), page.next), (2, Some(2)));
    /// let page = state.read_page(&bob, &thread, false, None, 2, now).unwrap();
    /// assert_eq!((page.messages[0].body.as_str(), page.next), ("three", None));
    /// let page = state.read_page(&bob, &thread, true, Some(1), 2, now).unwrap();
    /// assert_eq!(page.messages[0].seq, 2);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn read_page(
        &mut self,
        me: &SessionUri,
        thread: &ThreadName,
        all: bool,
        after: Option<u64>,
        limit: usize,
        now: Instant,
    ) -> Result<Page, String> {
        let who = self.arrive(me, now);
        let not_found = || format!("no thread named {thread}");
        if !may_read(&who, thread) {
            return Err(not_found());
        }
        let threads = &self.written.threads().by_name;
        let t = threads.get(thread).ok_or_else(not_found)?;
        let from = if all {
            after.unwrap_or(0)
        } else {
            self.presence.cursor(&who, thread)
        };
        let mut shown = t
            .messages
            .iter()
            .filter(|m| m.message.seq > from && (all || m.message.from.who() != &who))
            .map(|m| m.message.clone());
        let messages: Vec<Message> = shown.by_ref().take(limit.max(1)).collect();
        let next = if shown.next().is_some() {
            messages.last().map(|m| m.seq)
        } else {
            None
        };
        if let Some(last) = t.messages.back() {
            let read = next.unwrap_or(last.message.seq);
            let cursors = &mut self.presence.cursors;
            let cursor = cursors.entry((who, thread.clone())).or_insert(0);
            *cursor = if all { (*cursor).max(read) } else { read };
        }
        Ok(Page { messages, next })
    }

    /// Takes a claim if nobody holds it, or if its holder stopped more than
    /// [`CLAIM_GRACE`] ago. Gives whether the claim is granted, and the
    /// holder. [`State::claim_reply`] makes the reply after the write.
    pub fn claim(
        &mut self,
        me: &SessionUri,
        thread: &ThreadName,
        item: &str,
        now: Instant,
    ) -> Result<(bool, Who), String> {
        let who = self.arrive(me, now);
        let command = Claim {
            thread: thread.clone(),
            item: item.to_owned(),
        };
        self.run(me, &command, now)?;
        let holder = self
            .pending
            .work()
            .holder(thread, item)
            .expect("an item has a holder after a claim")
            .clone();
        Ok((holder == who, holder))
    }

    /// The reply to [`State::claim`], from the written copy.
    pub fn claim_reply(&self, granted: bool, holder: &Who, now: Instant) -> ClaimReply {
        ClaimReply {
            granted,
            holder: self.uri(holder, now),
        }
    }

    /// Frees a claim. Only its holder can.
    pub fn release(
        &mut self,
        me: &SessionUri,
        thread: &ThreadName,
        item: &str,
        now: Instant,
    ) -> Result<(), String> {
        self.arrive(me, now);
        let command = Release {
            thread: thread.clone(),
            item: item.to_owned(),
        };
        self.run(me, &command, now).map(drop)
    }

    /// Frees the claim of `holder` for it: the session with this session
    /// ID, or this start of it. Only the lead of the user of the holder
    /// in the repository of the holder can
    /// (01M3WG243BW7P6E1ME0DFNQF8C). The holder can be live, gone or
    /// ended. It gives the holder.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::RiffState;
    /// use riff_server::state::State;
    ///
    /// let lead: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=1ead".parse()?;
    /// let w1: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=068a2cc2".parse()?;
    /// let w2: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=fb118b5d".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&lead, now);
    /// state.riff(&lead, Some(RiffState::Running), now).unwrap();
    /// let thread = lead.default_thread().unwrap();
    /// state.claim(&w1, &thread, "issue-347", now).unwrap();
    ///
    /// // A session that is not the lead is refused.
    /// assert!(state.release_for(&w2, &thread, "issue-347", "068a", now).is_err());
    /// // The lead frees the claim, and the next session takes it.
    /// let holder = state.release_for(&lead, &thread, "issue-347", "068a", now).unwrap();
    /// assert_eq!(&holder, w1.who());
    /// assert!(state.claim(&w2, &thread, "issue-347", now).unwrap().0);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn release_for(
        &mut self,
        me: &SessionUri,
        thread: &ThreadName,
        item: &str,
        holder: &str,
        now: Instant,
    ) -> Result<Who, String> {
        self.arrive(me, now);
        let command = ReleaseFor {
            thread: thread.clone(),
            item: item.to_owned(),
            holder: holder.to_owned(),
        };
        let (changes, ()) = self.run(me, &command, now)?;
        match changes.first() {
            Some(Change::Released(freed)) => Ok(freed.session.who().clone()),
            _ => Err(format!("nobody holds {item}")),
        }
    }

    fn written_view(&self) -> View<'_> {
        View {
            riff: &self.written,
            presence: &self.presence,
        }
    }

    fn pending_view(&self) -> View<'_> {
        View {
            riff: &self.pending,
            presence: &self.presence,
        }
    }

    /// Runs the `handle` of a command, then commits its changes. Gives
    /// the changes and the note.
    fn run<C: Command>(
        &mut self,
        me: &SessionUri,
        command: &C,
        now: Instant,
    ) -> Result<(Vec<Change>, C::Note), String> {
        let (changes, note) = command.handle(me, &self.pending_view(), self.now(now))?;
        self.commit(&changes, now);
        Ok((changes, note))
    }

    /// Gives each change its position and time, puts the records in the
    /// queue, and applies them to the pending copy. A state with no
    /// writer applies them to the written copy too.
    fn commit(&mut self, changes: &[Change], now: Instant) {
        for change in changes {
            let record = Record {
                position: self.position() + 1,
                written_at_ms: self.ms(now),
                change: change.clone(),
            };
            apply(&mut self.pending, &record);
            if self.writer {
                self.made.insert(record.position, now);
            } else {
                self.apply_written(&record, Some(now));
            }
            self.queue.push(record);
        }
    }

    /// `now` in milliseconds since the Unix epoch, from the clock of the
    /// state. A state with no clock gives 0.
    fn ms(&self, now: Instant) -> u64 {
        self.clock.map_or(0, |(at, at_ms)| {
            let since = now.saturating_duration_since(at).as_millis();
            at_ms.saturating_add(u64::try_from(since).unwrap_or(u64::MAX))
        })
    }

    /// The time of a call at `now`, for [`Command::handle`].
    fn now(&self, now: Instant) -> Now {
        Now {
            at: now,
            ms: self.ms(now),
        }
    }

    /// What the changes of a post cause, with each selector that matched
    /// no session.
    fn delivery(changes: Vec<Change>, unmatched: Vec<Selector>) -> Delivery {
        let Some(Change::Posted(posted)) = changes.into_iter().last() else {
            unreachable!("the changes of a post end with the message");
        };
        let Posted {
            thread,
            message,
            woken,
        } = *posted;
        let wakes = woken
            .iter()
            .map(|who| (who.clone(), wake(&thread, &message)))
            .collect();
        Delivery {
            wakes,
            woken: woken.into_iter().collect(),
            unmatched,
            tailed: Tailed {
                thread,
                message,
                keys: Keys::new(),
                trusted: false,
            },
        }
    }

    /// Records that a session called. A new session starts in the place
    /// from its URI, joins the thread of its repository, and becomes the
    /// lead when it is the first ([`Register`]). A person has
    /// one entry for all its hosts, so it takes the place of each call
    /// (01M3MWW8KYJ3ZV91X22RBSAF33).
    fn arrive(&mut self, me: &SessionUri, now: Instant) -> Who {
        let who = me.who().clone();
        if let Some(session) = self.presence.sessions.get_mut(&who) {
            if who.session().is_none() {
                session.place = me.place().clone();
            }
            session.last_seen = now;
            session.seen_before_load = None;
            session.stopping = false;
            session.live(now);
            return who;
        }
        self.presence
            .sessions
            .insert(who.clone(), Session::new(me.place().clone(), now));
        self.run(me, &Register, now)
            .expect("a register is never refused");
        who
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riff_core::name::Place;
    use riff_core::record::Forgotten;
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
        assert_eq!(d.woken, vec![tests().who().clone()]);
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
        assert!(state.claim(&api(), &t, "issue-12", now).unwrap().0);
        for m in 1..=10 {
            state.alive(&api(), now + MINUTE * m);
        }
        let later = now + CLAIM_GRACE * 2;
        let reply = state.claim(&tests(), &t, "issue-12", later).unwrap();
        assert!(!reply.0);
        assert_eq!(&reply.1, api().who());
    }

    /// A front end can hold the watch stream of a killed client open:
    /// the server sees no close. The session is gone after [`GONE`], and
    /// its claims are free after [`CLAIM_GRACE`]
    /// (01M3WG240PNMQYZ7TX6Z7ZF6M9).
    #[test]
    fn a_killed_session_with_an_open_watch_is_gone_and_its_claim_is_free() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = repo();
        state.watch_started(&api(), now);
        state.worker(api().who(), true);
        assert!(state.claim(&api(), &t, "issue-12", now).unwrap().0);
        // The kill: no end call, no close of the stream, no keep-alive.
        let soon = now + GONE - Duration::from_secs(1);
        assert!(shown(&state, soon).contains(api().who()));
        let gone = now + GONE;
        assert!(!shown(&state, gone).contains(api().who()));
        let all = state.who(gone, T0 + ms(GONE), true);
        let dead = all.iter().find(|s| s.uri.who() == api().who()).unwrap();
        assert!(!dead.live, "an open stream of a gone session is not live");
        assert_eq!(dead.idle_secs, GONE.as_secs());
        // A post does not wake it.
        let to = vec![Selector::session("a1")];
        let post = Post::new(&tests(), None, to, "are you there?");
        assert!(state.post(post, gone, 0).is_err());
        let held = now + CLAIM_GRACE - Duration::from_secs(1);
        assert!(!state.claim(&tests(), &t, "issue-12", held).unwrap().0);
        let free = now + CLAIM_GRACE;
        assert!(state.claim(&tests(), &t, "issue-12", free).unwrap().0);
    }

    /// The stream of a dead client closes late, for example at the time
    /// limit of a request. The close does not bring the session back.
    #[test]
    fn the_late_close_of_a_watch_brings_no_session_back() {
        let now = Instant::now();
        let mut state = setup(now);
        state.watch_started(&api(), now);
        state.claim(&api(), &repo(), "issue-12", now).unwrap();
        let hour = now + MINUTE * 60;
        state.watch_ended(api().who(), hour);
        assert!(!shown(&state, hour).contains(api().who()));
        let all = state.who(hour, T0 + ms(MINUTE * 60), true);
        let dead = all.iter().find(|s| s.uri.who() == api().who()).unwrap();
        assert_eq!(dead.idle_secs, 3600, "the close is not a call");
        assert!(state.claim(&tests(), &repo(), "issue-12", hour).unwrap().0);
    }

    /// The keep-alive of `riff watch` keeps a session with an open watch
    /// live, with its claims.
    #[test]
    fn a_watch_with_keep_alives_stays_live() {
        let now = Instant::now();
        let mut state = setup(now);
        state.watch_started(&api(), now);
        state.claim(&api(), &repo(), "issue-12", now).unwrap();
        for m in 1..=60 {
            state.alive(&api(), now + MINUTE * m);
        }
        let hour = now + MINUTE * 60;
        let info = state.who(hour, T0 + ms(MINUTE * 60), false);
        let mike = info.iter().find(|s| s.uri.who() == api().who()).unwrap();
        assert!(mike.live);
        assert_eq!(mike.uri.claims(), ["issue-12"]);
    }

    #[test]
    fn a_claim_survives_a_short_gap_and_ends_after_the_grace_period() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = repo();
        state.watch_started(&api(), now);
        assert!(state.claim(&api(), &t, "issue-12", now).unwrap().0);
        state.watch_ended(api().who(), now);
        let soon = now + Duration::from_secs(60);
        assert!(!state.claim(&tests(), &t, "issue-12", soon).unwrap().0);
        let late = now + CLAIM_GRACE + Duration::from_secs(1);
        assert!(state.claim(&tests(), &t, "issue-12", late).unwrap().0);
    }

    #[test]
    fn only_the_holder_releases_a_claim() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = repo();
        state.claim(&api(), &t, "issue-12", now).unwrap();
        assert!(state.release(&tests(), &t, "issue-12", now).is_err());
        assert!(state.release(&api(), &t, "issue-12", now).is_ok());
        assert!(state.claim(&tests(), &t, "issue-12", now).unwrap().0);
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
        state.alive(&tests(), day2 - Duration::from_secs(60));
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
    fn a_replayed_session_is_idle_since_its_last_record() {
        let now = Instant::now();
        let mut state = State::with_writer(now, T0);
        state.register(&api(), now);
        let later = now + DAY;
        let loaded = State::replay(state.take_queue(), later, T0 + ms(DAY));
        let all = loaded.who(later, T0 + ms(DAY), true);
        assert_eq!(all[0].idle_secs, DAY.as_secs());
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
        assert!(taken.0);
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
        assert!(taken.0, "the item is free at once");
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
        assert!(!state.claim(&docs(), &repo(), "issue-12", soon).unwrap().0);
        let late = now + CLAIM_GRACE;
        assert!(state.claim(&docs(), &repo(), "issue-12", late).unwrap().0);
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
        assert!(!state.claim(&docs(), &repo(), "issue-12", hour).unwrap().0);
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

    /// A time in milliseconds since the Unix epoch.
    const T0: u64 = 1_800_000_000_000;

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn ms(d: Duration) -> u64 {
        u64::try_from(d.as_millis()).unwrap()
    }

    /// Each session, gone or not.
    fn listed(state: &State) -> Vec<SessionInfo> {
        state.who(Instant::now(), T0, true)
    }

    /// A replay of each record of `state` at `now`.
    fn replayed(state: &mut State, now: Instant) -> State {
        State::replay(state.take_queue(), now, T0)
    }

    #[test]
    fn a_replay_gives_the_state_of_the_live_path() {
        let now = Instant::now();
        let mut state = setup(now);
        post(&mut state, &api(), "design", &["user=brett"], "look");
        let dm = state
            .post(Post::new(&api(), None, to(&["session=c3"]), "hi"), now, 7)
            .unwrap()
            .tailed
            .thread;
        post(&mut state, &api(), "como-technologies/riff", &[], "one");
        let server = crate::owner::server_uri();
        let to = to(&["user=brett"]);
        let note = state.announce(
            &server,
            Some(thread("design")),
            to,
            "news",
            Kind::Note,
            now,
            9,
        );
        assert!(note.unwrap().wakes.is_empty());
        state.leave(&docs(), &repo(), now);
        state.claim(&tests(), &repo(), "issue-6", now).unwrap();
        state.claim(&api(), &repo(), "issue-7", now).unwrap();
        state.release(&api(), &repo(), "issue-7", now).unwrap();
        state.lead(&docs(), now).unwrap();
        state.set_idle(Some(3), None, now);
        let log: Vec<Record> = state.take_queue();
        assert!(log.len() > 10, "{log:?}");

        let mut loaded = State::replay(log, now, T0);
        assert!(loaded.same_log_state(&state));
        for me in [api(), tests(), docs()] {
            let threads = loaded.threads(&me, now);
            assert_eq!(threads.len(), state.threads(&me, now).len());
        }
        for (me, t) in [(docs(), thread("design")), (docs(), dm), (api(), repo())] {
            let messages = loaded.read(&me, &t, true, now).unwrap();
            assert_eq!(messages, state.read(&me, &t, true, now).unwrap());
            assert!(!messages.is_empty());
        }
        assert_eq!(loaded.uri(tests().who(), now).claims(), ["issue-6"]);
        assert_eq!(loaded.idle().per_host, 3);
        // The read cursors are in memory: Brett did not read the message
        // that woke him.
        assert_eq!(
            loaded.missed(tests().who()).unwrap().thread,
            thread("design")
        );
    }

    /// A JSON round trip of the snapshot of `state`, as a checkpoint
    /// does.
    fn through_json(state: &State, now: Instant) -> Snapshot {
        let bytes = serde_json::to_vec(&state.snapshot(now, T0)).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn a_start_from_a_checkpoint_and_the_records_after_it_gives_the_state_of_a_full_replay() {
        let now = Instant::now();
        let mut state = setup(now);
        // More messages than a thread keeps.
        for n in 0..KEEP_MESSAGES + 20 {
            post(
                &mut state,
                &api(),
                "design",
                &["user=brett"],
                &format!("m{n}"),
            );
        }
        state
            .post(Post::new(&api(), None, to(&["session=c3"]), "hi"), now, 7)
            .unwrap();
        state.claim(&tests(), &repo(), "issue-6", now).unwrap();
        state.lead(&docs(), now).unwrap();
        state.set_idle(Some(3), None, now);
        state.commit(
            &[Change::SessionForgotten(Forgotten { session: docs() })],
            now,
        );
        post(&mut state, &tests(), "design", &[], "after");
        let log: Vec<Record> = state.take_queue();
        let full = State::replay(log.clone(), now, T0);
        assert_eq!(
            full.written.threads().by_name[&thread("design")]
                .messages
                .len(),
            KEEP_MESSAGES
        );

        for at in [0, 1, 10, log.len() / 2, log.len() - 2, log.len()] {
            let head = State::replay(log[..at].to_vec(), now, T0);
            let loaded = State::load(Some(through_json(&head, now)), log[at..].to_vec(), now, T0);
            assert!(loaded.same_log_state(&full), "a checkpoint at {at}");
            let uris = |state: &State| -> Vec<String> {
                listed(state).iter().map(|s| s.uri.to_string()).collect()
            };
            assert_eq!(uris(&loaded), uris(&full), "a checkpoint at {at}");
        }
    }

    #[test]
    fn a_checkpoint_keeps_the_read_cursors() {
        let now = Instant::now();
        let mut state = setup(now);
        post(&mut state, &api(), "design", &["user=brett"], "one");
        let design = thread("design");
        assert_eq!(state.read(&tests(), &design, false, now).unwrap().len(), 1);
        post(&mut state, &api(), "design", &[], "two");
        let loaded = State::load(Some(through_json(&state, now)), [], now, T0);
        let mut loaded = loaded;
        let unread = loaded.read(&tests(), &design, false, now).unwrap();
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].body, "two");
    }

    #[test]
    fn read_pages_with_a_cursor() {
        let now = Instant::now();
        let mut state = setup(now);
        let design = thread("design");
        for n in 1..=7 {
            post(&mut state, &api(), "design", &[], &format!("m{n}"));
        }
        let bodies =
            |page: &Page| -> Vec<String> { page.messages.iter().map(|m| m.body.clone()).collect() };
        // The unread pages move the cursor of the reader.
        let page = state
            .read_page(&tests(), &design, false, None, 3, now)
            .unwrap();
        assert_eq!(
            (bodies(&page), page.next),
            (vec!["m1".into(), "m2".into(), "m3".into()], Some(3))
        );
        let page = state
            .read_page(&tests(), &design, false, None, 3, now)
            .unwrap();
        assert_eq!(page.next, Some(6));
        let page = state
            .read_page(&tests(), &design, false, None, 3, now)
            .unwrap();
        assert_eq!((bodies(&page), page.next), (vec!["m7".into()], None));
        assert!(
            state
                .read(&tests(), &design, false, now)
                .unwrap()
                .is_empty()
        );
        // A read of all follows the cursor in `after`.
        let page = state
            .read_page(&docs(), &design, true, Some(3), 3, now)
            .unwrap();
        assert_eq!(
            (bodies(&page), page.next),
            (vec!["m4".into(), "m5".into(), "m6".into()], Some(6))
        );
        let page = state
            .read_page(&docs(), &design, true, Some(6), 3, now)
            .unwrap();
        assert_eq!(page.next, None);
    }

    #[test]
    fn a_reader_cannot_read_a_message_older_than_the_kept_ones() {
        let now = Instant::now();
        let mut state = setup(now);
        for n in 0..KEEP_MESSAGES + 5 {
            post(&mut state, &api(), "design", &[], &format!("m{n}"));
        }
        let all = state.read(&tests(), &thread("design"), true, now).unwrap();
        assert_eq!(all.len(), KEEP_MESSAGES);
        assert_eq!(all[0].seq, 6);
    }

    #[test]
    fn a_replayed_session_is_gone_until_it_calls_and_keeps_its_place() {
        let now = Instant::now();
        let mut state = setup(now);
        let moved = uri("riff://mike@pangolin/como-technologies/riff?session=c3#other");
        state.register(&moved, now);
        state.claim(&moved, &repo(), "issue-9", now).unwrap();
        let later = now + Duration::from_secs(60);
        let mut loaded = replayed(&mut state, later);
        assert!(loaded.who(later, T0, false).is_empty());
        let all = listed(&loaded);
        assert_eq!(all.len(), 3);
        assert!(all.iter().all(|s| !s.live && s.status.is_none()));
        assert_eq!(
            loaded.uri(docs().who(), later).place().worktree(),
            Some("other")
        );
        loaded.register(&api(), later);
        assert_eq!(loaded.who(later, T0, false).len(), 1);
    }

    #[test]
    fn a_claim_ends_after_the_grace_period_from_the_replay() {
        let now = Instant::now();
        let mut state = setup(now);
        state.watch_started(&api(), now);
        state.claim(&api(), &repo(), "issue-12", now).unwrap();

        let start = now + Duration::from_secs(3600);
        let mut loaded = replayed(&mut state, start);
        let soon = start + CLAIM_GRACE - Duration::from_secs(1);
        assert!(!loaded.claim(&tests(), &repo(), "issue-12", soon).unwrap().0);
        let late = start + CLAIM_GRACE + Duration::from_secs(1);
        assert!(loaded.claim(&tests(), &repo(), "issue-12", late).unwrap().0);
    }

    #[test]
    fn the_records_since_the_load_keep_the_claim_timer_of_the_load() {
        let now = Instant::now();
        let mut state = setup(now);
        let start = now + Duration::from_secs(3600);
        let mut loaded = replayed(&mut state, start);

        // The old instance takes a claim and meets a new session after
        // the load.
        state.claim(&api(), &repo(), "issue-12", now).unwrap();
        let late_one = uri("riff://mike@pangolin/como-technologies/riff?session=d4#late");
        state.register(&late_one, now);
        loaded.catch_up(state.take_queue());
        assert!(loaded.same_log_state(&state));
        // Each session is gone until it calls, also the new one.
        assert!(loaded.who(start, T0, false).is_empty());
        let all = listed(&loaded);
        assert!(all.iter().any(|s| s.uri.who() == late_one.who()));

        let soon = start + CLAIM_GRACE - Duration::from_secs(1);
        assert!(!loaded.claim(&tests(), &repo(), "issue-12", soon).unwrap().0);
        let late = start + CLAIM_GRACE + Duration::from_secs(1);
        assert!(loaded.claim(&tests(), &repo(), "issue-12", late).unwrap().0);
    }

    #[test]
    fn a_session_that_comes_back_after_a_replay_keeps_its_claim() {
        let now = Instant::now();
        let mut state = setup(now);
        state.claim(&api(), &repo(), "issue-12", now).unwrap();

        let mut loaded = replayed(&mut state, now);
        loaded.watch_started(&api(), now + Duration::from_secs(60));
        let late = now + CLAIM_GRACE * 2;
        loaded.alive(&api(), late - Duration::from_secs(60));
        assert!(!loaded.claim(&tests(), &repo(), "issue-12", late).unwrap().0);
    }

    #[test]
    fn a_lead_holds_after_a_replay_while_its_claims_hold() {
        let now = Instant::now();
        let mut state = setup(now);
        state.lead(&docs(), now).unwrap();
        let loaded = replayed(&mut state, now);
        assert!(is_lead(&loaded, &docs(), now));
        assert!(!is_lead(&loaded, &api(), now));
        let late = now + CLAIM_GRACE;
        assert!(!is_lead(&loaded, &docs(), late), "it did not come back");
    }

    #[test]
    fn a_state_with_a_writer_shows_a_record_only_after_its_write() {
        let now = Instant::now();
        let mut state = State::with_writer(now, T0);
        state.register(&api(), now);
        state.riff(&api(), Some(RiffState::Running), now).unwrap();
        let first = state.take_queue();
        state.written(&first);
        assert_eq!(state.written_position(), state.position());

        let (granted, _) = state.claim(&api(), &repo(), "issue-6", now).unwrap();
        assert!(granted);
        assert!(state.written_position() < state.position());
        // The claim waits for its chunk: views do not show it yet, and a
        // second claim sees it.
        assert!(state.uri(api().who(), now).claims().is_empty());
        assert!(!state.claim(&tests(), &repo(), "issue-6", now).unwrap().0);
        let chunk = state.take_queue();
        assert_eq!(chunk.len(), 3, "the claim, and the arrival of brett");
        assert!(chunk.iter().all(|r| r.written_at_ms == T0));
        state.written(&chunk);
        assert_eq!(state.uri(api().who(), now).claims(), ["issue-6"]);
        assert_eq!(state.written_position(), state.position());
    }

    #[test]
    fn a_post_of_a_state_with_a_writer_is_read_only_after_its_write() {
        let now = Instant::now();
        let mut state = State::with_writer(now, T0);
        state.register(&api(), now);
        state.register(&tests(), now);
        let written = state.take_queue();
        state.written(&written);
        post(
            &mut state,
            &api(),
            "como-technologies/riff",
            &["user=brett"],
            "hi",
        );
        assert!(
            state
                .read(&tests(), &repo(), false, now)
                .unwrap()
                .is_empty()
        );
        assert!(state.missed(tests().who()).is_none());
        let chunk = state.take_queue();
        state.written(&chunk);
        assert_eq!(state.read(&tests(), &repo(), false, now).unwrap().len(), 1);
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
            payload: Some("the payload of the lead".into()),
            ..Post::new(
                &lead(api()),
                repo.clone(),
                vec![],
                "request: claim issue-12",
            )
        };
        let first = state.post(request.clone(), now, 0).unwrap();
        let error = state.post(request.clone(), now, 0).err().unwrap();
        assert!(
            error.contains(&format!("a copy of message {}", first.tailed.message.seq)),
            "{error}"
        );
        // ECDSA gives a second valid signature for the same payload: it is
        // still a copy.
        let resigned = Post {
            sig: Some("a second signature".into()),
            ..request.clone()
        };
        assert!(state.post(resigned, now, 0).is_err());
        // A new message has its own payload, and goes through.
        let again = Post {
            payload: Some("a new payload".into()),
            ..request
        };
        let second = state.post(again, now, 0).unwrap();
        assert_eq!(second.tailed.message.seq, first.tailed.message.seq + 1);
        assert_eq!(
            second.tailed.message.payload.as_deref(),
            Some("a new payload"),
            "the message keeps the payload"
        );
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
    fn a_person_takes_the_place_of_each_call_and_a_session_does_not() {
        let now = Instant::now();
        let mut state = setup(now);
        state.register(&uri("riff://mike@b"), now);
        let person_on_a = uri("riff://mike@a/como-technologies/riff");
        post(&mut state, &person_on_a, "x", &["user=brett"], "hi");
        let host =
            |state: &State, me: &SessionUri| state.uri(me.who(), now).place().host().to_owned();
        assert_eq!(host(&state, &person_on_a), "a");

        let elsewhere = api().moved(Place::host_only("b").unwrap());
        post(&mut state, &elsewhere, "x", &["user=brett"], "hi");
        assert_eq!(host(&state, &api()), api().place().host());
    }

    #[test]
    fn a_marked_lead_replaces_the_old_lead() {
        let now = Instant::now();
        let mut state = setup(now);
        let (me, old) = state.lead(&docs(), now).unwrap();
        let reply = state.lead_reply(&me, old.as_ref(), now);
        assert_eq!(reply.lead, lead(docs()));
        assert_eq!(reply.replaced, Some(api()));
        assert!(!is_lead(&state, &api(), now));
        assert!(
            is_lead(&state, &tests(), now),
            "another user keeps its lead"
        );
        let (_, old) = state.lead(&docs(), now).unwrap();
        assert_eq!(old, None);
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

    /// A live worker `id` of mike on `host`, with its last call at `at`.
    fn idle_worker(state: &mut State, host: &str, id: &str, at: Instant) -> SessionUri {
        let w = uri(&format!(
            "riff://mike@{host}/como-technologies/riff?session={id}"
        ));
        state.watch_started(&w, at);
        state.worker(w.who(), true);
        w
    }

    fn stopped(state: &mut State, now: Instant) -> Vec<String> {
        state
            .stop_idle_workers(now)
            .iter()
            .map(|s| s.worker.who().session().unwrap().to_owned())
            .collect()
    }

    fn running(state: &mut State) -> Result<(), String> {
        let person = uri("riff://mike@pangolin/como-technologies/riff");
        state.riff(&person, Some(RiffState::Running), Instant::now())?;
        Ok(())
    }

    /// 01M3Q5A0NKY1FCS0YH6N6YD3GN: three idle workers on one host, past
    /// the idle time: two stop, one stays. A worker with a claim and the
    /// lead stay.
    #[test]
    fn three_idle_workers_on_a_host_leave_one() {
        let now = Instant::now();
        let mut state = State::default();
        running(&mut state).unwrap();
        let lead = uri("riff://mike@pangolin/como-technologies/riff?session=lead");
        state.watch_started(&lead, now);
        state.worker(lead.who(), true);
        let busy = idle_worker(&mut state, "pangolin", "busy", now);
        state.claim(&busy, &repo(), "issue-12", now).unwrap();
        idle_worker(&mut state, "pangolin", "w1", now);
        idle_worker(&mut state, "pangolin", "w2", now + Duration::from_secs(1));
        idle_worker(&mut state, "pangolin", "w3", now + Duration::from_secs(2));

        let later = now + Duration::from_secs(90);
        assert_eq!(stopped(&mut state, later), ["w2", "w1"]);
        let shown = state.who(later, 90_000, false);
        let stopping: Vec<_> = shown
            .iter()
            .filter(|s| s.stopping)
            .map(|s| s.uri.who().session().unwrap())
            .collect();
        assert_eq!(stopping, ["w1", "w2"]);
        assert!(state.uri(lead.who(), later).lead());
        assert!(
            stopped(&mut state, later).is_empty(),
            "the server asks once"
        );
    }

    /// 01M3Q5A0NKY1FCS0YH6N6YD3GN: an idle worker on each of two hosts:
    /// both stay. A worker that is idle for less than the idle time stays.
    #[test]
    fn one_idle_worker_on_each_host_stays() {
        let now = Instant::now();
        let mut state = State::default();
        state.register(&lead(api()), now);
        idle_worker(&mut state, "pangolin", "w1", now);
        let w2 = idle_worker(&mut state, "thelio", "w2", now);
        let ten = now + Duration::from_secs(600);
        assert!(stopped(&mut state, ten).is_empty());

        state.called(&w2, ten);
        idle_worker(&mut state, "thelio", "w3", ten);
        let soon = ten + Duration::from_secs(30);
        assert!(
            stopped(&mut state, soon).is_empty(),
            "w2 is idle for 30 s only"
        );
    }

    /// 01M3Q5A0NKY1FCS0YH6N6YD3GN: a worker that claims work just before
    /// the stop is not stopped. A claim after the ask takes it back, so
    /// the next keep-alive does not stop it.
    #[test]
    fn a_worker_that_claims_is_not_stopped() {
        let now = Instant::now();
        let mut state = State::default();
        running(&mut state).unwrap();
        state.register(&lead(api()), now);
        idle_worker(&mut state, "pangolin", "w1", now + Duration::from_secs(5));
        let w2 = idle_worker(&mut state, "pangolin", "w2", now);
        let w3 = idle_worker(&mut state, "pangolin", "w3", now);

        let later = now + Duration::from_secs(80);
        state.claim(&w2, &repo(), "issue-12", later).unwrap();
        assert_eq!(stopped(&mut state, later), ["w3"]);

        assert!(state.alive(&w3, later).stop);
        state.claim(&w3, &repo(), "issue-13", later).unwrap();
        assert!(!state.alive(&w3, later).stop, "the claim wins");
        assert!(!state.who(later, 80_000, false).iter().any(|s| s.stopping));
    }

    /// 01M3Q5A0NKY1FCS0YH6N6YD3GN: a wake of the lead ends the watch of
    /// an idle worker after the ask. The worker is not stopped before it
    /// reads the request.
    #[test]
    fn a_woken_worker_is_not_stopped() {
        let now = Instant::now();
        let mut state = State::default();
        state.register(&lead(api()), now);
        state.set_idle(Some(0), None, now);
        let w1 = idle_worker(&mut state, "pangolin", "w1", now);
        let later = now + Duration::from_secs(80);
        assert_eq!(stopped(&mut state, later), ["w1"]);

        state.watch_ended(w1.who(), later + Duration::from_secs(1));
        assert!(!state.alive(&w1, later + Duration::from_secs(5)).stop);
    }

    /// 01M3Q5A0TF9K49V8Z1ZY9NDF74: the settings change the numbers, and
    /// the log keeps them.
    #[test]
    fn the_settings_change_the_numbers() {
        let now = Instant::now();
        let mut state = State::default();
        state.register(&lead(api()), now);
        for id in ["w1", "w2", "w3"] {
            idle_worker(&mut state, "pangolin", id, now);
        }
        assert_eq!(state.set_idle(None, Some(120), now).after_secs, 120);
        assert!(stopped(&mut state, now + Duration::from_secs(90)).is_empty());

        let idle = state.set_idle(Some(2), None, now);
        assert_eq!(
            idle,
            Idle {
                per_host: 2,
                after_secs: 120
            }
        );
        assert_eq!(stopped(&mut state, now + Duration::from_secs(130)).len(), 1);

        let loaded = State::replay(state.take_queue(), now, 0);
        assert_eq!(loaded.idle(), idle);
        assert_eq!(State::default().idle(), Idle::default());
    }

    /// A worker with no watch, and a session that is no worker, are not
    /// idle workers.
    #[test]
    fn only_a_live_worker_is_an_idle_worker() {
        let now = Instant::now();
        let mut state = State::default();
        state.register(&lead(api()), now);
        state.set_idle(Some(0), None, now);
        let w1 = idle_worker(&mut state, "pangolin", "w1", now);
        state.watch_ended(w1.who(), now);
        let agent = uri("riff://mike@pangolin/como-technologies/riff?session=a1");
        state.watch_started(&agent, now);
        assert!(stopped(&mut state, now + Duration::from_secs(90)).is_empty());
    }

    fn info(state: &State, me: &SessionUri, now: Instant) -> SessionInfo {
        state
            .who(now, T0, false)
            .into_iter()
            .find(|s| s.uri.who() == me.who())
            .unwrap()
    }

    fn set_step(state: &mut State, me: &SessionUri, step: &str, now: Instant) {
        let status = Status {
            step: step.into(),
            blocked: None,
        };
        state.set_status(me, status, now, T0).unwrap();
    }

    fn stale(state: &State, me: &SessionUri, now: Instant) -> bool {
        info(state, me, now).status.unwrap().stale
    }

    #[test]
    fn a_status_from_before_a_claim_or_a_release_is_stale() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = |secs| now + Duration::from_secs(secs);
        set_step(&mut state, &docs(), "look for work", t(1));
        assert!(!stale(&state, &docs(), t(1)));

        state.claim(&docs(), &repo(), "issue-12", t(2)).unwrap();
        assert!(stale(&state, &docs(), t(2)), "the claim is newer");
        set_step(&mut state, &docs(), "tests", t(3));
        assert!(!stale(&state, &docs(), t(3)));

        // A claim that the session holds already changes nothing.
        state.claim(&docs(), &repo(), "issue-12", t(4)).unwrap();
        assert!(!stale(&state, &docs(), t(4)));

        state.release(&docs(), &repo(), "issue-12", t(5)).unwrap();
        assert!(stale(&state, &docs(), t(5)), "the release is newer");
    }

    /// The server derives the state of each session in `who`
    /// (01M3QB6CJ1XCQG5B1BVR8AF3B4): the first that matches of offline,
    /// paused, blocked, busy and idle.
    #[test]
    fn who_derives_the_state_of_each_session() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = |secs| now + Duration::from_secs(secs);
        let of = |state: &State, secs| info(state, &docs(), t(secs)).state.unwrap();
        assert_eq!(of(&state, 0), SessionState::Offline, "no watch");
        state.watch_started(&docs(), t(1));
        assert_eq!(of(&state, 1), SessionState::Idle);
        state.claim(&docs(), &repo(), "issue-12", t(2)).unwrap();
        assert_eq!(of(&state, 2), SessionState::Busy);
        let blocked = Status {
            step: "merge".into(),
            blocked: Some("waits for a review".into()),
        };
        state.set_status(&docs(), blocked, t(3), T0).unwrap();
        assert_eq!(of(&state, 3), SessionState::Blocked);
        state.riff(&api(), Some(RiffState::Paused), t(4)).unwrap();
        assert_eq!(of(&state, 4), SessionState::Paused);
        state.riff(&api(), Some(RiffState::Running), t(5)).unwrap();
        assert_eq!(of(&state, 5), SessionState::Busy, "the block is stale");
        state.watch_ended(docs().who(), t(6));
        assert_eq!(of(&state, 6), SessionState::Offline);
    }

    #[test]
    fn a_status_from_before_a_pause_or_a_resume_is_stale() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = |secs| now + Duration::from_secs(secs);
        set_step(&mut state, &docs(), "tests", t(1));
        state.riff(&api(), Some(RiffState::Paused), t(2)).unwrap();
        assert!(stale(&state, &docs(), t(2)));
        set_step(&mut state, &docs(), "paused at: tests", t(3));
        assert!(!stale(&state, &docs(), t(3)));

        // A set to the same state is no change.
        state.riff(&api(), Some(RiffState::Paused), t(4)).unwrap();
        assert!(!stale(&state, &docs(), t(4)));
        state.riff(&api(), Some(RiffState::Running), t(5)).unwrap();
        assert!(stale(&state, &docs(), t(5)));
    }

    #[test]
    fn a_status_is_in_memory_and_a_replay_has_none() {
        let now = Instant::now();
        let mut state = setup(now);
        set_step(&mut state, &docs(), "tests", now);
        let mut loaded = State::replay(state.take_queue(), now, T0);
        loaded.register(&docs(), now);
        assert!(info(&loaded, &docs(), now).status.is_none());
    }

    #[test]
    fn the_claims_time_counts_from_the_last_change_of_the_claims() {
        let now = Instant::now();
        let mut state = setup(now);
        let t = |secs| now + Duration::from_secs(secs);
        assert_eq!(info(&state, &docs(), t(30)).claims_secs, 30);
        state.claim(&docs(), &repo(), "issue-12", t(40)).unwrap();
        state.release(&docs(), &repo(), "issue-12", t(100)).unwrap();
        assert_eq!(info(&state, &docs(), t(160)).claims_secs, 60);

        // A new start frees the claims: a change.
        state.claim(&docs(), &repo(), "issue-12", t(170)).unwrap();
        state.start(&docs(), t(200));
        assert_eq!(info(&state, &docs(), t(230)).claims_secs, 30);
    }
}

#[cfg(test)]
mod rules;
