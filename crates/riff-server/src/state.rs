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
//! | Riff state | none: one for the server | [`Riff`] | The log. The pause of the riff and of each repository, and the settings of idle workers. |
//! | People | email, or USER | [`Riff`] | The log. The riff ID, the email of each USER, the members, the admins, the owner, and the request for the owner role (01M3XA875QZ584JBGA37853PWX). |
//! | Known sessions | who | [`Riff`] | The log. The URI and the time of the last record that names the session, and its life cycle: the worker mark, the MustClear mark and the time of its last fresh start (see [`sessions`]). |
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
//! | `state.rs` | [`State`]: the two copies of the [`Riff`], the [`Presence`] and the clock. The steps of a command are here: [`State::check`], [`State::queue`], [`State::written`] and [`State::reply`]. The queries are here too: `who`, `read`, `threads`. The sync forms ([`State::run`], [`State::claim`] and the others) run the same steps for a state with no writer. |
//! | [`riff`] | [`Riff`] and [`apply`]: the one function that changes a riff, with one arm for each kind of record. |
//! | [`presence`] | [`Presence`], the session in memory, [`Signal`]: a change of the presence only, and `Presence::applied`: what a record changes in memory. |
//! | [`view`] | [`View`]: one copy of the riff with the presence, read only. `handle` and each query read it. |
//! | [`command`] | The trait [`Command`] with `handle` and `reply`, the [`Caller`], [`permits`], and the [`Refused`] of a refusal. |
//! | [`snapshot`] | [`Snapshot`]: the parts of each group in one checkpoint. |
//! | `state/rules.rs` | The given/when/then tests of `handle` and `apply`. |
//!
//! Each group of commands has one file with its part of the riff, its
//! `apply` arms, its part of the checkpoint and its command types:
//!
//! | Group | File | Part of the riff | Commands |
//! |---|---|---|---|
//! | sessions | [`sessions`] | [`Sessions`](sessions::Sessions) | [`Register`], [`Arrive`], [`Start`], [`End`] |
//! | threads | [`threads`] | [`Threads`](threads::Threads) | [`Join`], [`Leave`], [`Post`], [`Announce`] |
//! | work | [`work`] | [`Work`](work::Work) | [`Claim`], [`Release`], [`ReleaseFor`], [`Lead`] |
//! | the riff | [`the_riff`] | [`TheRiff`](the_riff::TheRiff) | [`MakeRiff`], [`Pause`], [`Resume`], [`SetIdle`], [`Forget`], [`Import`] |
//! | people | [`people`] | [`People`] | [`Admit`], `Invite`, `Remove`, `SetAdmin`, `PassOwner`, `TakeOwner`, `DenyOwner`, [`GrantOwner`], [`EndOwner`], [`NameOwner`], `Revoke` |
//!
//! The wire type of a command that a client can send is its command
//! type (01M3WRD8TBDPA4JNEZY6J4N2EX). A new command is a type with
//! [`Command`], a kind in [`CommandKind`] and a row in [`permits`]. A
//! new kind of record is one arm in [`apply`] and one method of the
//! part that it changes.
//!
//! The server runs each command through [`crate::engine`]: the engine
//! owns the state and its lock, and the writer finishes each command.
//!
//! # Event sourcing
//!
//! Each change that must not be lost is a [`Record`] in one log (see
//! [`riff_core::record`] and [`crate::log`]):
//!
//! - [`State::check`] checks a [`Command`] against the state
//!   ([`permits`], then [`Command::handle`]), and gives the changes, or
//!   the reason for a refusal. `handle` changes nothing and does no I/O.
//! - [`apply`] changes a [`Riff`] for one record. It does no I/O, reads
//!   no clock, and does not fail. A record that the state cannot take
//!   changes nothing, and the server logs a warning. The live path and
//!   the replay use the same `apply`.
//! - [`State::queue`] gives each change its position and time, and
//!   applies the records to the pending copy.
//! - The state keeps two copies of the [`Riff`] (01M3T4115BF1F0JFHYMK0WRKCX).
//!   `handle` checks against
//!   the pending copy, which has each record in the queue. Each view
//!   (`who`, `threads`, `read`, the wakes) uses the written copy, which
//!   has only the records whose chunk is written. [`State::written`]
//!   applies the records of a chunk after its write. So nobody sees a
//!   record that is not in the log.
//! - [`State::reply`] makes the reply to a command from the written
//!   copy.
//! - A [`State::default`] has no writer: each record counts as written
//!   at once. Tests and examples use it, with the sync forms
//!   ([`State::run`]). The server makes its state with
//!   [`State::with_writer`] or [`State::replay`], and gives it to its
//!   engine.
//!
//! # Rules
//!
//! - Each call records the session as seen at `now`. A call from a
//!   session that the server does not know registers it first, in the
//!   place from its URI ([`State::check`], [`Arrive`]). That register
//!   makes no lead (01M3X9XA3H6YF0QCYSNB2P0CT2). Only a `register` changes
//!   the place of a known session (R55, R64). A person has no session ID, so each call of a
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
//! - A worker that releases its last claim must clear its context before
//!   its next claim (01M3X9XAK1KPZZVM1AJR2H8DSS). The reply to that release and to each
//!   keep-alive carries the ask (01M3X9XB37TQCXWPNFZRMRGJB4). The server sends it no wake
//!   until a start with a fresh context. That start gives it the wake
//!   that it missed (01M3X9XBMB3R718Z81BYXTHMZ0).
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
//! - A session with a session ID becomes the lead when it registers or
//!   starts in a repository, it is not a worker, and no other session
//!   of its user there holds (R176, 01M3X9XA3H6YF0QCYSNB2P0CT2).
//!   [`State::lead`] makes a session the lead and replaces the old lead
//!   (R177).
//! - A lead counts while it holds, as a claim does, and while it works
//!   in that repository. A lead that leaves the repository thread stops
//!   being the lead (R178).
//! - The riff has a pause of the whole riff and a pause for each
//!   repository ([`Pauses`]). A new state is paused.
//!   [`State::riff`] reads it, and sets it for a person or a lead
//!   (01M3JCFTWCR72HQB8CBTQKXJNF, 01M3JCG3T8AJZN31SZQQTP3FAF).
//! - [`State::stop_idle_workers`] asks each idle worker past the limit
//!   to stop. The reply to its keep-alive carries the ask. A call of the
//!   worker takes it back (01M3Q5A0NKY1FCS0YH6N6YD3GN).
//! - While the riff or the repository of a thread is paused, a claim
//!   there fails. A held claim stays, and a
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

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use riff_core::name::{SessionUri, ThreadName, Who};
use riff_core::record::{Change, Posted, Record};
use riff_core::selector::Selector;
use riff_core::wire::{
    Activity, AliveReply, BlockedInfo, Claim, End, Freed, Idle, ItemFact, Join, Keys, Kind, Lead,
    LeadReply, Leave, Message, Pause, Post, Register, Release, ReleaseFor, ReleaseReply, Resume,
    RiffReply, RiffState, SessionInfo, SessionState, SetBlocked, SetIdle, Start, StartReason,
    Status, StatusInfo, Tailed, ThreadInfo, Wake, Waits,
};

pub mod command;
pub mod people;
pub mod presence;
pub mod riff;
pub mod sessions;
pub mod snapshot;
pub mod the_riff;
pub mod threads;
pub mod view;
pub mod work;

pub use command::{
    Caller, Cause, Class, Code, Command, CommandKind, Done, Now, Refused, Role, permits,
};
pub use people::{Admit, Admitted, EndOwner, GrantOwner, NameOwner, OwnerChange, People};
pub use presence::{Imported, ImportedSession, Presence, Signal};
pub use riff::{Riff, apply};
pub use sessions::Arrive;
pub use snapshot::Snapshot;
pub use the_riff::{Forget, Import, MakeRiff, Pauses};
pub use threads::{Announce, may_read};
pub use view::{Settings, View};
pub use work::{MUST_CLEAR, released_for};

use presence::Session;
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
    /// The settings of the server that `handle` and `reply` read
    /// (01M3XA87F70CD3WH4STADSCW6S). They are not in the log.
    settings: Settings,
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
            settings: Settings::default(),
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
    /// Each session to wake, with its event. A session that must clear
    /// its context gets none (01M3X9XBMB3R718Z81BYXTHMZ0): see [`State::keep_wakes`].
    pub wakes: Vec<(Who, Wake)>,
    /// Each woken session, for the sender.
    pub woken: Vec<Who>,
    /// Each selector that matched no session.
    pub unmatched: Vec<Selector>,
    /// The event for the `tail` streams of the thread.
    pub tailed: Tailed,
}

impl Delivery {
    /// What the `posted` record of a message causes: one wake for each
    /// session that the message woke, and the event for `tail`. The
    /// writer sends it after the write of the record
    /// (01M3WRD90WBBCWTDGVQCBR6MNT).
    pub fn of(posted: &Posted) -> Delivery {
        let Posted {
            thread,
            message,
            woken,
        } = posted;
        Delivery {
            wakes: woken
                .iter()
                .map(|who| (who.clone(), wake(thread, message)))
                .collect(),
            woken: woken.iter().cloned().collect(),
            unmatched: Vec::new(),
            tailed: Tailed {
                thread: thread.clone(),
                message: message.clone(),
                keys: Keys::new(),
                trusted: false,
            },
        }
    }
}

/// What [`State::check`] gives.
pub struct Check<N> {
    /// The records of the `register` that the state ran first, for a
    /// caller that it did not know. They are a command of their own.
    pub registered: Option<Vec<Record>>,
    /// The caller, with its worker mark.
    pub caller: Caller,
    /// The changes and the note of the command, or why it is refused.
    pub result: Result<(Vec<Change>, N), Refused>,
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

    /// The same state with these settings of the server
    /// (01M3XA87F70CD3WH4STADSCW6S): the admins of the settings, the
    /// public address, and the times of the owner role. Each view holds
    /// them.
    ///
    /// ```
    /// use riff_server::owner::Timing;
    /// use riff_server::state::{Role, Settings, State};
    ///
    /// let settings = Settings::new(&["boss@x.io".into()], "https://riff.x.io", Timing::default());
    /// let state = State::default().with_settings(settings);
    /// // A USER with no email is a member, also with the name of an admin.
    /// assert_eq!(state.role("boss"), Role::Member);
    /// ```
    pub fn with_settings(self, settings: Settings) -> State {
        State { settings, ..self }
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

    /// Takes the memory of the old server after the write of the
    /// import of go-live (01M3Z8MRDZEKTXSKZTDTDSCZ3W): the last call,
    /// the end and the status of each session, and the read cursors.
    /// The log holds none of them.
    ///
    /// - It makes each session that the written copy knows, as a replay
    ///   does: gone until it calls. So its claims and its lead hold for
    ///   [`CLAIM_GRACE`], and `who --all` shows it.
    /// - A session that called after the import stays as it is.
    /// - A cursor counts only for a session and a thread that the
    ///   written copy has. A cursor after the last message of its
    ///   thread moves back to that message.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::record::{Change, SessionStarted};
    /// use riff_core::wire::StartReason;
    /// use riff_server::state::{Caller, Import, Imported, ImportedSession, State};
    ///
    /// let ann: SessionUri = "riff://ann@heron/acme/app?session=a1".parse()?;
    /// let started = Change::SessionStarted(SessionStarted {
    ///     session: ann.clone(),
    ///     reason: StartReason::Join,
    ///     worker: false,
    /// });
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.run(&Caller::server(), &Import { changes: vec![started] }, now).unwrap();
    /// // The log names the session, and the presence does not know it.
    /// assert!(state.who(now, 9_000, true).is_empty());
    ///
    /// let session = ImportedSession { who: ann.who().clone(), seen_ms: 5_000, ended: false, status: None };
    /// state.imported(Imported { sessions: vec![session], cursors: vec![] }, now);
    /// let who = state.who(now, 9_000, true);
    /// assert_eq!(who[0].uri.who(), ann.who());
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn imported(&mut self, imported: Imported, now: Instant) {
        let loaded = self.presence.loaded.unwrap_or(now);
        self.presence.loaded = Some(loaded);
        for session in imported.sessions {
            if let Some(known) = self.written.sessions().known.get(&session.who) {
                let place = known.uri.place().clone();
                self.presence.imported(session, place, loaded);
            }
        }
        for (who, thread, seq) in imported.cursors {
            let threads = self.written.threads();
            if self.presence.knows(&who) && threads.has(&thread) {
                let seq = seq.min(threads.last_seq(&thread));
                self.presence.cursors.entry((who, thread)).or_insert(seq);
            }
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
    /// the record, or `None` in a replay. When the record takes its
    /// session out of MustClear, it gives the wake that the session
    /// missed.
    fn apply_written(&mut self, record: &Record, at: Option<Instant>) -> Option<(Who, Wake)> {
        let cleared = match &record.change {
            Change::SessionStarted(started) if self.must_clear(started.session.who()) => {
                Some(started.session.who().clone())
            }
            _ => None,
        };
        apply(&mut self.written, record);
        self.presence.applied(record, &self.written, at);
        let who = cleared.filter(|who| !self.must_clear(who))?;
        let wake = self.missed(&who)?;
        Some((who, wake))
    }

    /// The written copy: the riff that the written records give.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::state::{Riff, State};
    ///
    /// let state = State::replay([], Instant::now(), 0);
    /// assert_eq!(*state.written_riff(), Riff::default());
    /// ```
    pub fn written_riff(&self) -> &Riff {
        &self.written
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
    /// skips them. It gives the wake of each session that a record takes
    /// out of MustClear: the wake that it missed (01M3X9XBMB3R718Z81BYXTHMZ0).
    pub fn written(&mut self, records: &[Record]) -> Vec<(Who, Wake)> {
        let mut wakes = Vec::new();
        if self.writer {
            for record in records {
                let at = self.made.remove(&record.position);
                wakes.extend(self.apply_written(record, at));
            }
        }
        wakes
    }

    /// True when the session `who` must clear its context before its
    /// next claim, in the written copy (01M3X9XAK1KPZZVM1AJR2H8DSS).
    pub fn must_clear(&self, who: &Who) -> bool {
        self.written.sessions().must_clear(who)
    }

    /// Removes the wake of each session that must clear its context
    /// (01M3X9XBMB3R718Z81BYXTHMZ0). The message is in its thread, and its `posted` record
    /// names the session.
    pub fn keep_wakes(&self, wakes: &mut Vec<(Who, Wake)>) {
        wakes.retain(|(who, _)| !self.must_clear(who));
    }

    /// Checks a command of `caller` against the pending copy. It is the
    /// first step of each command, in the engine and in the sync form
    /// ([`State::run`]):
    ///
    /// 1. A session ID that the state knows under another user is
    ///    refused ([`State::check_user`]).
    ///    A command of the people skips the steps 1 and 2: its caller
    ///    is the person of the token, with no place, and it changes no
    ///    presence.
    /// 2. A caller that the state does not know registers first: the
    ///    state runs [`Arrive`] for it, and queues its records
    ///    ([`Check::registered`]). A `register` and an `end` do not
    ///    register first. A known caller is seen now.
    /// 3. The caller gets its worker mark from the pending copy. A
    ///    session that the riff does not know is not a worker. [`permits`] and
    ///    [`Command::handle`] run. They change nothing.
    /// 4. A command that is not refused sets its signal
    ///    ([`Command::signal`], 01M3WRD97EZJK3AABXECXEY133).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::Claim;
    /// use riff_server::state::{Caller, Code, State};
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// let thread = mike.default_thread().unwrap();
    /// let claim = Claim { me: mike.clone(), thread, item: "issue-12".into() };
    ///
    /// // The first call of a session registers it. A new riff is
    /// // paused, so the claim is refused.
    /// let check = state.check(&Caller::of(&mike), &claim, now);
    /// assert_eq!(check.registered.unwrap().len(), 2);
    /// assert_eq!(check.result.unwrap_err().code, Code::Paused);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn check<C: Command>(
        &mut self,
        caller: &Caller,
        command: &C,
        now: Instant,
    ) -> Check<C::Note> {
        let mut caller = caller.clone();
        let mut registered = None;
        // A command of the people names no `me`: its caller is the
        // person of the token, with no place. So it changes no presence.
        let has_place = !C::KIND.of_people();
        if has_place && matches!(caller.class(), Class::Person | Class::Session) {
            let me = caller.me().clone();
            if let Err(reason) = self.check_user(&me) {
                return Check {
                    registered,
                    caller,
                    result: Err(Refused::new(Code::OtherUser, reason)),
                };
            }
            let who = me.who();
            let place = me.place().clone();
            if self.presence.knows(who) {
                Signal::Called { place }.set(&mut self.presence, who, now);
            } else if !matches!(C::KIND, CommandKind::Register | CommandKind::End) {
                let arrive = Arrive { me: me.clone() };
                let (changes, ()) = arrive
                    .handle(&caller, &self.pending_view(), self.now(now))
                    .expect("a register is never refused");
                Signal::Place { place }.set(&mut self.presence, who, now);
                let cause = Cause::of(&caller, CommandKind::Register);
                registered = Some(self.queue(&cause, &changes, now));
            }
            caller = caller.with_worker(self.pending.sessions().worker(who));
        }
        let needs = command.needs(&caller);
        let result = permits(C::KIND, &caller, needs)
            .map_err(|refused| self.no_owner(needs, refused))
            .and_then(|()| command.handle(&caller, &self.pending_view(), self.now(now)));
        if result.is_ok()
            && let Some(signal) = command.signal(&caller)
        {
            signal.set(&mut self.presence, caller.who(), now);
        }
        Check {
            registered,
            caller,
            result,
        }
    }

    /// The refusal of `permits` for a command that needs the owner, in
    /// a riff with no owner: its text names `riff owner --take`
    /// (01M3Q63NNC6SC03BFCG80M7B4D). `permits` reads only the caller, so
    /// it cannot know that the riff has no owner.
    fn no_owner(&self, needs: Role, refused: Refused) -> Refused {
        if needs == Role::Owner && self.pending.people().owner().is_none() {
            Refused::new(refused.code, people::NO_OWNER)
        } else {
            refused
        }
    }

    /// The role of `user` for the check of a command: from the pending
    /// copy, with the admins of the settings
    /// (01M3XA87F70CD3WH4STADSCW6S). The queue is in order, so a command
    /// that comes after a change of a role gets the new role.
    pub fn role(&self, user: &str) -> Role {
        self.pending_view().role_of(user)
    }

    /// The people of the written copy, with the settings: for each
    /// query. See [`View::roles`], [`View::persons`] and
    /// [`View::riff_owner`].
    pub fn people(&self) -> View<'_> {
        self.written_view()
    }

    /// The ID of the riff in the written copy, when the riff has one
    /// (01M3JNVBPMZ1K9WX7Q7DP6Y0DH).
    pub fn riff_id(&self) -> Option<String> {
        self.written.people().riff_id().map(str::to_owned)
    }

    /// True when the riff has an owner, or had one, in the written
    /// copy (01M3JN3AQMHZHT6JP3P6GM9PWZ).
    pub fn owned(&self) -> bool {
        self.written.people().owned()
    }

    /// The email of the admin whose request for the owner role waits,
    /// in the written copy.
    pub fn asks(&self) -> Option<String> {
        self.written.people().asks().map(str::to_owned)
    }

    /// True when a request for the owner role waits, and its time ended
    /// at `now`, on the clock of the state: the clock that gave the
    /// request its time.
    pub fn owner_due(&self, now: Instant) -> bool {
        self.written.people().is_due(self.ms(now))
    }

    /// The position of the last end of the sign-ins of each USER, in
    /// the written copy (01M3XA87A9GGFA89RQXWSKY0V6).
    pub fn signins_ended(&self) -> BTreeMap<String, u64> {
        self.written.people().ended().clone()
    }

    /// True when the people of the written copy know `user`: an email
    /// signed in with it (R209).
    pub fn knows_person(&self, user: &str) -> bool {
        self.written.people().email_of(user).is_some()
    }

    /// The refusal of a command of `user` from a sign-in that started
    /// at the position `started`, when the pending copy has a later end
    /// of the sign-ins of `user`: a removal or a revoke
    /// (01M3XGP03RDF6S15JYS718WWFC). The record of that end can still
    /// wait in the queue. A person who is no member then gets the code
    /// `not_member`. `None` when the sign-in is good.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::record::{Change, Email, PersonJoined};
    /// use riff_server::state::{Caller, Cause, Code, CommandKind, State};
    ///
    /// let bob = "bob@gmail.com".to_owned();
    /// let now = Instant::now();
    /// let mut state = State::with_writer(now, 0);
    /// let cause = Cause::of(&Caller::server(), CommandKind::Forget);
    /// let joined = PersonJoined { user: "bob".into(), email: bob.clone() };
    /// state.queue(&cause, &[Change::PersonJoined(joined)], now);
    /// assert!(state.ended_since("bob", 1).is_none());
    /// // The removal waits in the queue, at the position 2.
    /// state.queue(&cause, &[Change::MemberRemoved(Email { email: bob })], now);
    /// assert_eq!(state.ended_since("bob", 1).unwrap().code, Code::NotMember);
    /// // A sign-in from after the removal is good.
    /// assert!(state.ended_since("bob", 2).is_none());
    /// ```
    pub fn ended_since(&self, user: &str, started: u64) -> Option<Refused> {
        let ended = *self.pending.people().ended().get(user)?;
        if started >= ended {
            return None;
        }
        Some(if self.pending_view().may_join(user) {
            Refused::new(
                Code::NotAllowed,
                format!("each sign-in of {user} ended. Sign in again: riff login"),
            )
        } else {
            Refused::new(
                Code::NotMember,
                format!("{user} is not a member of this riff, and each sign-in of {user} ended"),
            )
        })
    }

    /// Each USER that holds `email`, in the written copy: the people
    /// whose sign-ins a `member_removed` record ends.
    pub fn users_of(&self, email: &str) -> Vec<String> {
        self.written.people().users_of(email)
    }

    /// Gives each change its position, its time and its cause, and
    /// applies the records to the pending copy. It is the second step
    /// of a command, after [`State::check`]. The records of one command
    /// have positions one after another, and each one names the caller
    /// and the kind of the command (01M3X4Z60G1FXQTDC5XDJ05BAX). A state with no writer
    /// applies them to the written copy too.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::record::By;
    /// use riff_core::wire::Join;
    /// use riff_server::state::{Caller, State};
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let mut state = State::default();
    /// let join = Join { me: mike.clone(), thread: "design".parse()? };
    /// let (made, ()) = state.run(&Caller::of(&mike), &join, Instant::now()).unwrap();
    /// assert_eq!(made[0].by, Some(By::Session(mike.who().clone())));
    /// assert_eq!(made[0].command.as_deref(), Some("join"));
    /// // The register that the state ran first is a command of its own.
    /// let log = state.take_queue();
    /// assert_eq!(log[0].command.as_deref(), Some("register"));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn queue(&mut self, cause: &Cause, changes: &[Change], now: Instant) -> Vec<Record> {
        let mut records = Vec::with_capacity(changes.len());
        for change in changes {
            let record = Record {
                position: self.position() + 1,
                written_at_ms: self.ms(now),
                by: Some(cause.by.clone()),
                command: Some(cause.command.as_str().to_owned()),
                change: change.clone(),
            };
            apply(&mut self.pending, &record);
            if self.writer {
                self.made.insert(record.position, now);
            } else {
                // The sync form sends no wake.
                let _ = self.apply_written(&record, Some(now));
            }
            records.push(record);
        }
        records
    }

    /// The reply to a command, from the written copy
    /// ([`Command::reply`]). It is the last step of a command, after
    /// the write of `made`.
    pub fn reply<C: Command>(
        &self,
        caller: &Caller,
        command: &C,
        done: &Done,
        note: C::Note,
        now: Instant,
    ) -> C::Reply {
        command.reply(caller, &self.written_view(), done, note, self.now(now))
    }

    /// Runs a command in the sync form: [`State::check`], then
    /// [`State::queue`]. The records go to the queue of the state
    /// ([`State::take_queue`]). A state with no writer counts them as
    /// written at once. The tests, the examples and the tools use it.
    /// The server does not: its engine runs the same steps, and its
    /// writer applies the records after the write (see [`crate::engine`]).
    /// Gives the records of the command and its note.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::Join;
    /// use riff_server::state::{Caller, State};
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let mut state = State::default();
    /// let join = Join { me: mike.clone(), thread: "design".parse()? };
    /// let (made, ()) = state.run(&Caller::of(&mike), &join, Instant::now()).unwrap();
    /// assert_eq!(made.len(), 1);
    /// // The queue also has the records of the register of the session.
    /// assert_eq!(state.take_queue().len(), 3);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn run<C: Command>(
        &mut self,
        caller: &Caller,
        command: &C,
        now: Instant,
    ) -> Result<(Vec<Record>, C::Note), Refused> {
        self.run_as(caller, command, now)
            .map(|(_, made, note)| (made, note))
    }

    /// [`State::run`], which also gives the caller with its worker
    /// mark.
    fn run_as<C: Command>(
        &mut self,
        caller: &Caller,
        command: &C,
        now: Instant,
    ) -> Result<(Caller, Vec<Record>, C::Note), Refused> {
        let Check {
            registered,
            caller,
            result,
        } = self.check(caller, command, now);
        self.queue.extend(registered.into_iter().flatten());
        let (changes, note) = result?;
        let made = self.queue(&Cause::of(&caller, C::KIND), &changes, now);
        self.queue.extend(made.iter().cloned());
        Ok((caller, made, note))
    }

    /// Runs a command in the sync form, and gives its reply.
    fn ask<C: Command>(
        &mut self,
        me: &SessionUri,
        command: &C,
        now: Instant,
    ) -> Result<C::Reply, Refused> {
        let (caller, made, note) = self.run_as(&trusted(me), command, now)?;
        Ok(self.reply(&caller, command, &Done::of(made), note, now))
    }

    /// Sets a signal of the session `who` in the presence
    /// (01M3WRD97EZJK3AABXECXEY133). It makes no record, and it cannot
    /// change the riff. See [`Signal::set`]. The reply also says if the
    /// session must clear its context (01M3X9XB37TQCXWPNFZRMRGJB4): the written copy has
    /// that mark.
    pub fn signal(&mut self, who: &Who, signal: Signal, now: Instant) -> AliveReply {
        let reply = signal.set(&mut self.presence, who, now);
        AliveReply {
            clear: self.must_clear(who),
            ..reply
        }
    }

    /// True when the state knows the session `who` in memory.
    pub fn knows(&self, who: &Who) -> bool {
        self.presence.knows(who)
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
    /// moves. It keeps its worker mark.
    pub fn register(&mut self, me: &SessionUri, now: Instant) {
        let worker = self.pending.sessions().worker(me.who());
        let register = Register {
            me: me.clone(),
            worker,
        };
        self.ask(me, &register, now)
            .expect("a register is never refused");
    }

    /// Records that a watch stream opened. The session is live.
    pub fn watch_started(&mut self, me: &SessionUri, now: Instant) {
        let who = self.arrive(me, now);
        self.signal(&who, Signal::WatchStarted, now);
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
    /// // Only a person changes the settings.
    /// let mike: SessionUri = "riff://mike@pangolin".parse()?;
    /// state.set_idle(&mike, Some(0), None, now).unwrap();
    /// state.worker(&w1, true, now);
    /// state.watch_started(&w1, now);
    ///
    /// let later = now + Duration::from_secs(80);
    /// assert_eq!(state.stop_idle_workers(later).len(), 1);
    /// state.watch_ended(w1.who(), later);
    /// assert!(!state.alive(&w1, later).stop);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn watch_ended(&mut self, who: &Who, now: Instant) {
        self.signal(who, Signal::WatchEnded, now);
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
        if !self.knows(me.who()) {
            self.arrive(me, now);
            return AliveReply::default();
        }
        self.signal(me.who(), Signal::Alive { activity: None }, now)
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
        // Only a session ends: a person has no end.
        let _ = self.ask(me, &End { me: me.clone() }, now);
    }

    /// A new start of `me`: a new agent process, a resume or a `/clear`
    /// (01M3JEE1QQCFS5TMZW5N2DAD2D). The session is live, with its ID,
    /// threads, cursors and lead. Each of its claims is free at once. It
    /// gives the claims that it freed. The session keeps its worker
    /// mark. A start with a fresh context (`process`, `clear`) ends
    /// MustClear.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::{RiffState, StartReason};
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
    /// let freed = state.start(&mike, StartReason::Resume, now);
    /// assert_eq!(freed[0].item, "issue-12");
    /// let me = state.uri(mike.who(), now);
    /// assert!(me.claims().is_empty());
    /// assert!(me.lead(), "the lead stays the lead");
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn start(&mut self, me: &SessionUri, reason: StartReason, now: Instant) -> Vec<Freed> {
        let start = Start {
            me: me.clone(),
            reason,
            worker: self.pending.sessions().worker(me.who()),
        };
        self.ask(me, &start, now)
            .map(|started| started.freed)
            .unwrap_or_default()
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
        let repository = session.place.default_thread();
        let status = session.status.as_ref().map(|s| StatusInfo {
            status: s.status.clone(),
            age_secs: now_ms.saturating_sub(s.set_ms) / 1000,
            stale: s.before(Some(session.claims_changed))
                || s.before(self.presence.riff_changed)
                || s.before(
                    repository
                        .as_ref()
                        .and_then(|r| self.presence.repository_changed.get(r))
                        .copied(),
                ),
        });
        let blocked = session.blocked.as_ref().map(|b| BlockedInfo {
            reason: b.reason.clone(),
            secs: now_ms.saturating_sub(b.set_ms) / 1000,
            answered: b.answered.is_some(),
            woken_again: b.woken_again.is_some(),
            unanswered: b.unanswered,
        });
        let waits = repository
            .as_ref()
            .and_then(|thread| self.waits(thread, uri.claims()));
        let work = session.work.as_ref().map(|(activity, at)| Activity {
            secs: now.saturating_duration_since(*at).as_secs(),
            ..activity.clone()
        });
        let sessions = self.written.sessions();
        let must_clear = sessions.must_clear(who);
        let state = SessionState::of(
            live,
            self.pauses().at(repository.as_ref()).is_some(),
            blocked.is_some(),
            must_clear,
            waits.is_some(),
            !uri.claims().is_empty(),
        );
        SessionInfo {
            uri,
            live,
            idle_secs: now_ms.saturating_sub(session.seen_ms(now, now_ms)) / 1000,
            status,
            worker: sessions.worker(who),
            stopping: session.stopping,
            claims_secs: now
                .saturating_duration_since(session.claims_changed)
                .as_secs(),
            must_clear,
            fresh_secs: sessions
                .fresh_ms(who)
                .map(|fresh_ms| now_ms.saturating_sub(fresh_ms) / 1000),
            state: Some(state),
            work,
            waits,
            blocked,
        }
    }

    /// What `claims` in `thread` wait for, when each claim waits: the
    /// wait of the first claim (01M41FZP9A50CH4A2VX344DW49). A claim
    /// `verify-ITEM` has the fact of `ITEM`.
    fn waits(&self, thread: &ThreadName, claims: &[String]) -> Option<Waits> {
        let mut each = claims.iter().map(|claim| {
            let item = claim.strip_prefix("verify-").unwrap_or(claim);
            self.presence.item(thread, item)?.waits(claim)
        });
        let first = each.next()??;
        each.all(|w| w.is_some()).then_some(first)
    }

    /// Sets the block of `me` at `now_ms` (01M41FZPGEK4TNPSM2051W4VMS).
    /// See [`SetBlocked::check`] for the reason that it refuses.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::SessionState;
    /// use riff_server::state::State;
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&mike, now);
    /// state.set_blocked(&mike, "which design?".into(), now, 1_000).unwrap();
    /// let info = &state.who(now, 61_000, false)[0];
    /// assert_eq!(info.blocked.as_ref().unwrap().reason, "which design?");
    /// assert_eq!(info.blocked.as_ref().unwrap().secs, 60);
    /// assert!(state.set_blocked(&mike, " ".into(), now, 1_000).is_err());
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn set_blocked(
        &mut self,
        me: &SessionUri,
        reason: String,
        now: Instant,
        at_ms: u64,
    ) -> Result<(), String> {
        let set = SetBlocked {
            me: me.clone(),
            reason,
        };
        set.check()?;
        let who = self.arrive(me, now);
        let blocked = Signal::Blocked {
            reason: set.reason,
            at_ms,
        };
        self.signal(&who, blocked, now);
        Ok(())
    }

    /// The look of the lead `lead` at the blocks of the sessions of its
    /// user in its repository (01M41FZQ545HQ9Q75CSKX8HF8H,
    /// 01M41FZQCHWY1YVGAZ60ZHJK21). It gives each block that gets a
    /// second wake of the lead now, and each block that is unanswered
    /// now, each with the session URI and the reason. It refuses a
    /// caller that is not the lead.
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use riff_core::name::SessionUri;
    /// use riff_server::state::State;
    ///
    /// let lead: SessionUri = "riff://mike@pangolin/o/r?session=l1".parse()?;
    /// let w1: SessionUri = "riff://mike@pangolin/o/r?session=w1".parse()?;
    /// let t0 = Instant::now();
    /// let mut state = State::default();
    /// state.register(&lead, t0);
    /// state.register(&w1, t0);
    /// state.set_blocked(&w1, "which design?".into(), t0, 0).unwrap();
    /// let after = Duration::from_secs(600);
    /// let at = |secs| t0 + Duration::from_secs(secs);
    ///
    /// let look = |state: &mut State, secs| {
    ///     state.alive(&w1, at(secs));
    ///     let (again, unanswered) = state.look_blocks(&lead, after, at(secs)).unwrap();
    ///     (again.len(), unanswered.len())
    /// };
    /// assert_eq!(look(&mut state, 599), (0, 0));
    /// assert_eq!(look(&mut state, 600), (1, 0), "the second wake of the lead");
    /// assert_eq!(look(&mut state, 900), (0, 0));
    /// assert_eq!(look(&mut state, 1200), (0, 1), "no answer: unanswered");
    /// assert_eq!(look(&mut state, 1300), (0, 0), "one time");
    /// assert!(state.look_blocks(&w1, after, at(1300)).is_err(), "only the lead");
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    #[allow(clippy::type_complexity)]
    pub fn look_blocks(
        &mut self,
        lead: &SessionUri,
        after: Duration,
        now: Instant,
    ) -> Result<(Vec<(SessionUri, String)>, Vec<(SessionUri, String)>), String> {
        let who = lead.who();
        let uri = self.uri(who, now);
        let Some(thread) = uri.default_thread().filter(|_| uri.lead()) else {
            return Err("only the lead looks at the blocks of its sessions".into());
        };
        let (again, unanswered) = self.presence.look_blocks(who, &thread, after, now);
        let uris = |list: Vec<(Who, String)>| {
            list.into_iter()
                .map(|(who, reason)| (self.uri(&who, now), reason))
                .collect()
        };
        Ok((uris(again), uris(unanswered)))
    }

    /// Keeps the facts of the items of `thread` that a client saw on the
    /// forge (01M41FZP2C4Z4J6WKRXZ5B31EH). With `all`, they replace each
    /// fact of the thread.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::{ItemFact, PullFact, PullState, SessionState, Waits};
    /// use riff_server::state::State;
    ///
    /// let mike: SessionUri = "riff://mike@pangolin/o/r?session=a6cf".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&mike, now);
    /// state.riff(&mike, Some(riff_core::wire::RiffState::Running), now).unwrap();
    /// let thread = mike.default_thread().unwrap();
    /// state.claim(&mike, &thread, "issue-12", now).unwrap();
    /// let pull = Some(PullFact { number: 40, state: PullState::Asked });
    /// let fact = ItemFact { item: "issue-12".into(), pull, needs: vec![] };
    /// state.set_facts(&mike, thread.clone(), vec![fact], false, now);
    /// let info = &state.who(now, 0, false)[0];
    /// assert_eq!(info.waits, Some(Waits::Verify { pull: 40 }));
    /// state.set_facts(&mike, thread, vec![], true, now);
    /// assert_eq!(state.who(now, 0, false)[0].waits, None);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn set_facts(
        &mut self,
        me: &SessionUri,
        thread: ThreadName,
        items: Vec<ItemFact>,
        all: bool,
        now: Instant,
    ) {
        let who = self.arrive(me, now);
        self.signal(&who, Signal::Facts { thread, items, all }, now);
    }

    /// Registers the session `me` with this worker mark
    /// (01M3NT4M159EHN5W8JRTQ417N4). The mark goes to the log: a
    /// `session_started` record with the reason `join` has it
    /// (01M3X9X9M079WGFPJZHNXH9VEP).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::record::Change;
    /// use riff_server::state::State;
    ///
    /// let w1: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w1".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.worker(&w1, true, now);
    /// assert!(state.who(now, 0, false)[0].worker);
    /// // A worker is never the first lead.
    /// assert!(!state.uri(w1.who(), now).lead());
    ///
    /// // The mark is in the log, so a replay has it.
    /// let log = state.take_queue();
    /// assert!(matches!(&log[1].change, Change::SessionStarted(s) if s.worker));
    /// let replayed = State::replay(log, now, 0);
    /// assert!(replayed.who(now, 0, true)[0].worker);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn worker(&mut self, me: &SessionUri, worker: bool, now: Instant) {
        let register = Register {
            me: me.clone(),
            worker,
        };
        self.ask(me, &register, now)
            .expect("a register is never refused");
    }

    /// The settings of idle workers (01M3Q5A0TF9K49V8Z1ZY9NDF74).
    pub fn idle(&self) -> Idle {
        self.written.the_riff().idle
    }

    /// Sets the settings of idle workers as `me`: each value that is
    /// `Some` (01M3Q5A0TF9K49V8Z1ZY9NDF74). It gives the settings with
    /// the change.
    pub fn set_idle(
        &mut self,
        me: &SessionUri,
        per_host: Option<u16>,
        after_secs: Option<u64>,
        now: Instant,
    ) -> Result<Idle, String> {
        let command = SetIdle {
            me: me.clone(),
            per_host,
            after_secs,
        };
        self.ask(me, &command, now).map_err(|r| r.reason)?;
        Ok(self.pending.the_riff().idle)
    }

    /// Each idle worker past the limit (01M3Q5A0NKY1FCS0YH6N6YD3GN). An
    /// idle worker is a live worker that is not a lead, holds no claim,
    /// was not asked before, and made no call for a time. On each host
    /// of each user, the server keeps the [`Idle::per_host`] idle
    /// workers with the shortest idle time. It asks each other one that
    /// is idle for [`Idle::after_secs`] or more. It changes nothing: the
    /// signal [`Signal::AskedToStop`] asks a worker.
    pub fn idle_workers(&self, now: Instant) -> Vec<Stopping> {
        let idle = self.written.the_riff().idle;
        let after = Duration::from_secs(idle.after_secs);
        // One limit for each user, host and repository
        // (01M3XAHZMN8P0PRD0Q7881TEF9).
        type Key = (String, String, Option<ThreadName>);
        let mut workers: BTreeMap<Key, Vec<(Duration, Who)>> = BTreeMap::new();
        let view = self.written_view();
        for (who, session) in &self.presence.sessions {
            let free = self.written.sessions().worker(who)
                && session.watching(now)
                && !session.stopping
                && !view.holds_claim(who)
                && !view.is_lead(who, now);
            if free {
                let key = (
                    who.user().to_owned(),
                    session.place.host().to_owned(),
                    session.place.default_thread(),
                );
                let time = now.saturating_duration_since(session.last_seen);
                workers.entry(key).or_default().push((time, who.clone()));
            }
        }
        let mut stopping = Vec::new();
        for ((_, host, _), mut list) in workers {
            list.sort();
            for (time, who) in list.into_iter().skip(usize::from(idle.per_host)) {
                if time < after {
                    continue;
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

    /// Asks each idle worker past the limit to stop, and gives each
    /// ([`State::idle_workers`]). A later call of the worker takes the
    /// ask back: a worker that claims work goes on.
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
    ///     state.worker(&w, true, now);
    ///     state.watch_started(&w, now + Duration::from_secs(at));
    /// }
    /// let later = now + Duration::from_secs(80);
    /// let stopped: Vec<_> = state.stop_idle_workers(later).iter().map(|s| s.worker.to_string()).collect();
    /// assert_eq!(stopped, [worker("w2").to_string(), worker("w3").to_string()]);
    /// assert!(state.stop_idle_workers(later).is_empty(), "asks once");
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn stop_idle_workers(&mut self, now: Instant) -> Vec<Stopping> {
        let stopping = self.idle_workers(now);
        for stop in &stopping {
            self.signal(stop.worker.who(), Signal::AskedToStop, now);
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
    /// the old lead (R177). The reply names `me` and the old lead, if
    /// another session was the lead.
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
        self.ask(me, &Lead { me: me.clone() }, now)
            .map_err(|r| r.reason)
    }

    /// The pauses of the written copy: the pause of the whole riff, and
    /// the pause of each repository (01M3XAHZBGSSJB3YX23K88W01K).
    pub fn pauses(&self) -> &Pauses {
        &self.written.the_riff().pauses
    }

    /// The pauses as a caller at the place of `me` sees them
    /// (01M3XAHZJAF6YVDJ7WX74X8RBX): for a session that the state
    /// knows, the place in the state; for each other caller, the place
    /// of `me`.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::RiffState;
    /// use riff_server::state::State;
    ///
    /// let lead: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a1".parse()?;
    /// let brett: SessionUri = "riff://brett@kadomony/como-technologies/strata?session=b1".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// // The first session of its user that registers is the lead.
    /// state.register(&lead, now);
    /// state.register(&brett, now);
    /// state.riff(&lead, Some(RiffState::Running), now).unwrap();
    /// state.pause_repository(&brett, RiffState::Paused, now).unwrap();
    ///
    /// // The session of the other repository goes on.
    /// assert_eq!(state.pauses_at(&lead).state, RiffState::Running);
    /// let seen = state.pauses_at(&brett);
    /// assert_eq!(seen.state, RiffState::Paused);
    /// assert!(seen.riff.is_none());
    /// assert_eq!(seen.repositories[0].repository.to_string(), "como-technologies/strata");
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn pauses_at(&self, me: &SessionUri) -> RiffReply {
        let place = match self.presence.sessions.get(me.who()) {
            Some(session) if me.who().session().is_some() => &session.place,
            _ => me.place(),
        };
        self.pauses().reply(place.default_thread().as_ref(), false)
    }

    /// Pauses or resumes the repository of `me`
    /// (01M3XAHZBGSSJB3YX23K88W01K). A person (`me` with no session ID)
    /// or the lead of that repository can
    /// (01M3XAHZDSQR263QZVB41CK0MX).
    pub fn pause_repository(
        &mut self,
        me: &SessionUri,
        set: RiffState,
        now: Instant,
    ) -> Result<RiffReply, String> {
        let reply = match set {
            RiffState::Paused => self.ask(me, &Pause::here(me.clone()), now),
            RiffState::Running => self.ask(me, &Resume::here(me.clone()), now),
        };
        reply.map_err(|r| r.reason)
    }

    /// The pauses as `me` sees them. With `set`, it pauses or resumes
    /// the whole riff first. Only an admin can set it: as a person
    /// (`me` with no session ID), or as a lead
    /// (01M3XAHZDSQR263QZVB41CK0MX). This sync form has the role of an
    /// admin, as the caller in a riff with no sign-in.
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
        let reply = match set {
            None => {
                self.arrive(me, now);
                return Ok(self.pauses_at(me));
            }
            Some(RiffState::Paused) => self.ask(me, &Pause::whole(me.clone()), now),
            Some(RiffState::Running) => self.ask(me, &Resume::whole(me.clone()), now),
        };
        reply.map_err(|r| r.reason)
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
    /// let step = Status { step: "write the tests".into() };
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
        let at_ms = now_ms;
        self.signal(&who, Signal::Status { status, at_ms }, now);
        Ok(())
    }

    /// The threads that `me` joined, with its unread counts. A count
    /// leaves out the own posts of `me` (01M3JPK82PN4F706MCHDH771MW).
    pub fn threads(&mut self, me: &SessionUri, now: Instant) -> Vec<ThreadInfo> {
        let who = self.arrive(me, now);
        self.threads_of(&who, now)
    }

    /// The threads that the session `who` joined, with its unread
    /// counts. It changes nothing.
    pub fn threads_of(&self, who: &Who, now: Instant) -> Vec<ThreadInfo> {
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
    /// is one. A watch sends it when it starts. A session that must
    /// clear its context gets none: the wake waits until its start with
    /// a fresh context (01M3X9XBMB3R718Z81BYXTHMZ0).
    pub fn missed(&self, who: &Who) -> Option<Wake> {
        if self.must_clear(who) {
            return None;
        }
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
        self.run(&Caller::server(), &Forget, now)
            .map_or(0, |(made, ())| the_riff::forgotten(&made))
    }

    /// Adds a session to a thread. It makes the thread if it is new.
    pub fn join(&mut self, me: &SessionUri, thread: &ThreadName, now: Instant) {
        let join = Join {
            me: me.clone(),
            thread: thread.clone(),
        };
        self.ask(me, &join, now).expect("a join is never refused");
    }

    /// Removes a session from a thread. It is no longer the lead there.
    pub fn leave(&mut self, me: &SessionUri, thread: &ThreadName, now: Instant) {
        let leave = Leave {
            me: me.clone(),
            thread: thread.clone(),
        };
        self.ask(me, &leave, now).expect("a leave is never refused");
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
        let post = Post {
            at_ms: Some(at_ms),
            ..post
        };
        let (made, unmatched) = self.run(&trusted(&me), &post, now).map_err(|r| r.reason)?;
        Ok(self.delivery(&made, unmatched))
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
        let caller = Caller::of(me).with_class(Class::Server);
        let (made, ()) = self.run(&caller, &announce, now).map_err(|r| r.reason)?;
        Ok(self.delivery(&made, Vec::new()))
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
        self.written_view().live_leads(user, now)
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
        self.written_view().repositories()
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
        let (page, read) = self.page(&who, thread, all, after, limit)?;
        if let Some(read) = read {
            self.signal(&who, read, now);
        }
        Ok(page)
    }

    /// The page of [`State::read_page`] for the session `who`, and the
    /// signal that moves its read cursor to the end of the page. It
    /// changes nothing.
    pub fn page(
        &self,
        who: &Who,
        thread: &ThreadName,
        all: bool,
        after: Option<u64>,
        limit: usize,
    ) -> Result<(Page, Option<Signal>), String> {
        let not_found = || format!("no thread named {thread}");
        if !may_read(who, thread) {
            return Err(not_found());
        }
        let threads = &self.written.threads().by_name;
        let t = threads.get(thread).ok_or_else(not_found)?;
        let from = if all {
            after.unwrap_or(0)
        } else {
            self.presence.cursor(who, thread)
        };
        let mut shown = t
            .messages
            .iter()
            .filter(|m| m.message.seq > from && (all || m.message.from.who() != who))
            .map(|m| m.message.clone());
        let messages: Vec<Message> = shown.by_ref().take(limit.max(1)).collect();
        let next = if shown.next().is_some() {
            messages.last().map(|m| m.seq)
        } else {
            None
        };
        let read = t.messages.back().map(|last| Signal::Read {
            thread: thread.clone(),
            seq: next.unwrap_or(last.message.seq),
            all,
        });
        Ok((Page { messages, next }, read))
    }

    /// Takes a claim if nobody holds it, or if its holder stopped more than
    /// [`CLAIM_GRACE`] ago. Gives whether the claim is granted, and the
    /// holder. In this sync form, a claim of a held item is not an
    /// error: it is not granted. The command refuses it with the code
    /// `held` (01M3WRD9JBQMNN96TXJH8EAJ3W).
    pub fn claim(
        &mut self,
        me: &SessionUri,
        thread: &ThreadName,
        item: &str,
        now: Instant,
    ) -> Result<(bool, Who), String> {
        let command = Claim {
            me: me.clone(),
            thread: thread.clone(),
            item: item.to_owned(),
        };
        match self.ask(me, &command, now) {
            Ok(_) => Ok((true, me.who().clone())),
            Err(refused) if refused.code == Code::Held => {
                let holder = self.pending.work().holder(thread, item);
                Ok((false, holder.expect("a held item has a holder").clone()))
            }
            Err(refused) => Err(refused.reason),
        }
    }

    /// Frees a claim. Only its holder can. The reply says if the session
    /// must clear its context now: a worker that released its last claim
    /// (01M3X9XAK1KPZZVM1AJR2H8DSS).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::{RiffState, StartReason};
    /// use riff_server::state::{MUST_CLEAR, State};
    ///
    /// let lead: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=1ead".parse()?;
    /// let w1: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w1".parse()?;
    /// let now = Instant::now();
    /// let mut state = State::default();
    /// state.register(&lead, now);
    /// state.riff(&lead, Some(RiffState::Running), now).unwrap();
    /// state.worker(&w1, true, now);
    /// let thread = lead.default_thread().unwrap();
    /// state.claim(&w1, &thread, "issue-12", now).unwrap();
    ///
    /// // The worker releases its last claim: it must clear its context.
    /// assert!(state.release(&w1, &thread, "issue-12", now).unwrap().must_clear);
    /// assert_eq!(state.claim(&w1, &thread, "issue-13", now).unwrap_err(), MUST_CLEAR);
    /// // A start with a fresh context ends it.
    /// state.start(&w1, StartReason::Clear, now);
    /// assert!(state.claim(&w1, &thread, "issue-13", now).unwrap().0);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn release(
        &mut self,
        me: &SessionUri,
        thread: &ThreadName,
        item: &str,
        now: Instant,
    ) -> Result<ReleaseReply, String> {
        let command = Release {
            me: me.clone(),
            thread: thread.clone(),
            item: item.to_owned(),
        };
        self.ask(me, &command, now).map_err(|r| r.reason)
    }

    /// Frees the claim of `holder` for it: the session with this session
    /// ID, or this start of it. Only the lead of the user of the holder
    /// in the repository of the holder can
    /// (01M3WG243BW7P6E1ME0DFNQF8C). The holder can be live, gone or
    /// ended. The server posts a note to the thread of the claim. It
    /// gives the holder.
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
        let command = ReleaseFor {
            me: me.clone(),
            thread: thread.clone(),
            item: item.to_owned(),
            session: holder.to_owned(),
        };
        let (made, ()) = self
            .run(&trusted(me), &command, now)
            .map_err(|r| r.reason)?;
        match made.first().map(|record| &record.change) {
            Some(Change::Released(freed)) => Ok(freed.session.who().clone()),
            _ => Err(format!("nobody holds {item}")),
        }
    }

    fn written_view(&self) -> View<'_> {
        View {
            riff: &self.written,
            presence: &self.presence,
            settings: &self.settings,
        }
    }

    fn pending_view(&self) -> View<'_> {
        View {
            riff: &self.pending,
            presence: &self.presence,
            settings: &self.settings,
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

    /// What the records of a post cause, with each selector that matched
    /// no session. A session that must clear its context gets no wake.
    fn delivery(&self, made: &[Record], unmatched: Vec<Selector>) -> Delivery {
        let Some(Change::Posted(posted)) = made.last().map(|record| &record.change) else {
            unreachable!("the records of a post end with the message");
        };
        let mut delivery = Delivery {
            unmatched,
            ..Delivery::of(posted)
        };
        self.keep_wakes(&mut delivery.wakes);
        delivery
    }

    /// Records that a session called, in the sync form. A session that
    /// the state does not know registers: it starts in the place from
    /// its URI, and joins the thread of its repository ([`Arrive`]). It
    /// does not become the lead. A person has one entry for all its
    /// hosts, so it takes the place of each call
    /// (01M3MWW8KYJ3ZV91X22RBSAF33).
    fn arrive(&mut self, me: &SessionUri, now: Instant) -> Who {
        let who = me.who().clone();
        if self.knows(&who) {
            let place = me.place().clone();
            self.signal(&who, Signal::Called { place }, now);
        } else {
            self.ask(me, &Arrive { me: me.clone() }, now)
                .expect("a register is never refused");
        }
        who
    }
}

/// The caller of the sync form of a command: it acts as `me`, with the
/// role of an admin, as the caller in a riff with no sign-in
/// (01M3WRD9G5GAF65EX8P6D5DMQM).
fn trusted(me: &SessionUri) -> Caller {
    Caller::of(me).with_role(Role::Admin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use riff_core::name::Place;
    use riff_core::record::Forgotten;
    use riff_core::wire::Kind;
    use std::collections::BTreeSet;

    fn uri(text: &str) -> SessionUri {
        text.parse().unwrap()
    }

    fn thread(text: &str) -> ThreadName {
        text.parse().unwrap()
    }

    /// The person of `me` on the command line: no session. Only a
    /// person changes the settings.
    fn person_of(me: &SessionUri) -> SessionUri {
        uri(&format!("riff://{}@pangolin", me.who().user()))
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
        // The second entry comes from before the rule: no command made it.
        let place = Signal::Place {
            place: sandman.place().clone(),
        };
        state.signal(sandman.who(), place, now);
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
        state.worker(&api(), true, now);
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
        state.start(&api(), StartReason::Resume, now);
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

        let freed = state.start(&api(), StartReason::Clear, now);
        let items: Vec<&str> = freed.iter().map(|f| f.item.as_str()).collect();
        assert_eq!(items, ["issue-7", "issue-12"]);
        assert!(state.uri(api().who(), now).claims().is_empty());
        assert!(is_lead(&state, &api(), now));
        assert_eq!(state.read(&api(), &repo(), false, now).unwrap().len(), 1);
        let taken = state.claim(&docs(), &repo(), "issue-12", now).unwrap();
        assert!(taken.0, "the item is free at once");
        assert!(state.start(&api(), StartReason::Clear, now).is_empty());
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

    /// A selector of a later build, from its JSON.
    fn later(json: &str) -> Selector {
        serde_json::from_str(json).unwrap()
    }

    /// Only the read of the log takes a value of a later build
    /// (01M3XSF90E9JYYTC13D9THY4WE).
    #[test]
    fn a_post_with_a_kind_or_a_selector_field_of_a_later_build_is_refused() {
        let now = Instant::now();
        let mut state = setup(now);
        let position = state.position();
        let kind = Post {
            kind: Kind::Other,
            ..Post::new(&api(), Some(repo()), to(&["user=brett"]), "hi")
        };
        let why = state.post(kind, now, 0).unwrap_err();
        assert_eq!(why, "no such kind: use message, status or note");
        let field = vec![later(r#"{"user":"brett","wave":"17"}"#)];
        let why = state
            .post(Post::new(&api(), Some(repo()), field.clone(), "hi"), now, 0)
            .unwrap_err();
        let text = r#"this server does not know the selector {"user":"brett","wave":"17"}."#;
        assert!(why.starts_with(text), "{why}");
        // A selector in another form of JSON.
        let form = vec![later(r#""all""#)];
        let why = state
            .post(Post::new(&api(), Some(repo()), form, "hi"), now, 0)
            .unwrap_err();
        assert!(
            why.starts_with(r#"this server does not know the selector "all"."#),
            "{why}"
        );
        // A direct message too.
        let why = state
            .post(Post::new(&api(), None, field, "hi"), now, 0)
            .unwrap_err();
        assert!(why.starts_with(text), "{why}");
        assert_eq!(state.position(), position, "a refused post makes no record");
        let refused = state
            .run(
                &trusted(&api()),
                &Post {
                    kind: Kind::Other,
                    ..Post::new(&api(), Some(repo()), Vec::new(), "hi")
                },
                now,
            )
            .unwrap_err();
        assert_eq!(refused.code, Code::BadRequest);
    }

    /// A field that the build does not know never makes a selector
    /// wider (01M3XSF90E9JYYTC13D9THY4WE). The server posts with such a
    /// selector here, because a `post` call with it is refused.
    #[test]
    fn a_selector_of_a_later_build_wakes_nobody() {
        let now = Instant::now();
        let server = crate::owner::server_uri();
        for (json, known) in [
            (r#"{"user":"brett","wave":"17"}"#, "user=brett"),
            // A value that the build does not know, and each other form
            // of JSON.
            (r#"{"lead":"maybe","user":"brett"}"#, "user=brett"),
            (r#""all""#, "repo=como-technologies/riff"),
            (r#"["user","brett"]"#, "user=brett"),
            ("17", "user=brett"),
            ("null", "user=brett"),
            (
                r#"{"repo":"como-technologies/riff","wave":17}"#,
                "repo=como-technologies/riff",
            ),
            // A selector for a lead falls back to the free sessions of
            // the user when the user has no lead.
            (
                r#"{"user":"mike","repo":"como-technologies/riff","lead":true,"wave":"17"}"#,
                "user=mike,repo=como-technologies/riff,lead=true",
            ),
        ] {
            let mut state = setup(now);
            let design = thread("design");
            let selector = later(json);
            let delivery = state
                .announce(
                    &server,
                    Some(design.clone()),
                    vec![selector.clone()],
                    "for the wave",
                    Kind::Message,
                    now,
                    0,
                )
                .unwrap();
            assert!(
                delivery.wakes.is_empty() && delivery.woken.is_empty(),
                "{json}"
            );
            for session in [api(), tests(), docs()] {
                assert_eq!(state.missed(session.who()), None, "{json}");
                // The message makes no session a member of the thread.
                let threads = state.threads(&session, now);
                assert!(threads.iter().all(|t| t.thread != design), "{json}");
            }
            // The message keeps the selector as it came.
            assert_eq!(delivery.tailed.message.to, [selector]);

            // The same selector with only the fields that the build
            // knows wakes its sessions.
            let delivery = state
                .announce(
                    &server,
                    Some(design),
                    to(&[known]),
                    "for all",
                    Kind::Message,
                    now,
                    0,
                )
                .unwrap();
            assert!(!delivery.wakes.is_empty(), "{json}");
        }
    }

    /// A message of a later build stays in its thread: a reader gets it
    /// as a message, with its text (01M3XSF90E9JYYTC13D9THY4WE).
    #[test]
    fn a_replay_keeps_a_message_with_a_kind_or_a_selector_field_of_a_later_build() {
        let now = Instant::now();
        let line = |position: u64, kind: &str, to: &str| {
            let line = format!(
                r#"{{"position":{position},"written_at_ms":1,"change":{{"posted":{{"thread":"como-technologies/riff","message":{{"seq":{position},"from":"{}","to":[{to}],"body":"text {position}","at_ms":1,"kind":{kind}}}}}}}}}"#,
                api()
            );
            match riff_core::record::Line::parse(&line).unwrap() {
                riff_core::record::Line::Record(record) => *record,
                riff_core::record::Line::Unknown { .. } => panic!("a known kind"),
            }
        };
        let records = vec![
            line(1, r#""poll""#, r#"{"user":"brett"}"#),
            line(2, r#""note""#, r#"{"user":"brett","wave":"17"}"#),
        ];
        assert_eq!(records[0].other(), Some("kind"));
        assert_eq!(records[1].other(), Some("to"));
        let mut state = State::replay(records, now, 0);
        state.register(&tests(), now);
        // The records name no woken session, so no session has a wake.
        assert_eq!(state.missed(tests().who()), None);
        let messages = state.read(&tests(), &repo(), true, now).unwrap();
        let shown: Vec<(Kind, &str)> = messages.iter().map(|m| (m.kind, m.body.as_str())).collect();
        assert_eq!(shown, [(Kind::Other, "text 1"), (Kind::Note, "text 2")]);
        assert!(messages[1].to[0].is_other());
        assert!(!messages[1].to[0].matches(&tests()));
    }

    /// A session URI with a query part of a later build is the URI of
    /// the same session, and the part gives no mark: no lead and no
    /// claim (01M3XYYSY536AEJVERBPTQFQYX).
    #[test]
    fn a_replay_takes_a_session_uri_of_a_later_build_as_the_same_session() {
        let now = Instant::now();
        let later = "riff://mike@pangolin/como-technologies/riff?session=a1&lead=maybe&wave=17#api";
        let lines = [
            format!(
                r#"{{"position":1,"written_at_ms":1,"change":{{"session_started":{{"session":"{later}","reason":"process"}}}}}}"#
            ),
            format!(
                r#"{{"position":2,"written_at_ms":2,"change":{{"claimed":{{"session":"{later}","thread":"como-technologies/riff","item":"issue-7"}}}}}}"#
            ),
            format!(
                r#"{{"position":3,"written_at_ms":3,"change":{{"posted":{{"thread":"como-technologies/riff","message":{{"seq":1,"from":"{later}","to":[],"body":"the text","at_ms":3}}}}}}}}"#
            ),
        ];
        let records: Vec<Record> = lines
            .iter()
            .map(|line| match riff_core::record::Line::parse(line).unwrap() {
                riff_core::record::Line::Record(record) => *record,
                riff_core::record::Line::Unknown { .. } => panic!("a known kind"),
            })
            .collect();
        let fields: Vec<_> = records.iter().map(Record::other).collect();
        assert_eq!(fields, [Some("session"), Some("session"), Some("from")]);

        let mut state = State::replay(records, now, 0);
        // The same session, in its place, with its claim.
        let uri = state.uri(api().who(), now);
        assert_eq!((uri.who(), uri.place()), (api().who(), api().place()));
        assert_eq!(uri.claims(), ["issue-7"]);
        // The part `lead=maybe` gives no lead.
        assert!(!uri.lead() && !uri.is_other());
        // The message keeps the URI of its sender as it came.
        state.register(&tests(), now);
        let messages = state.read(&tests(), &repo(), true, now).unwrap();
        assert_eq!(messages[0].body, "the text");
        assert_eq!(messages[0].from.to_string(), later);
        assert_eq!(messages[0].from.who(), api().who());
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
        state
            .set_idle(&person_of(&docs()), Some(3), None, now)
            .unwrap();
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
        state
            .set_idle(&person_of(&docs()), Some(3), None, now)
            .unwrap();
        let forgotten = state.queue(
            &Cause::of(&Caller::server(), CommandKind::Forget),
            &[Change::SessionForgotten(Forgotten { session: docs() })],
            now,
        );
        state.queue.extend(forgotten);
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
        let reply = state.lead(&docs(), now).unwrap();
        assert_eq!(reply.lead, lead(docs()));
        assert_eq!(reply.replaced, Some(api()));
        assert!(!is_lead(&state, &api(), now));
        assert!(
            is_lead(&state, &tests(), now),
            "another user keeps its lead"
        );
        assert_eq!(state.lead(&docs(), now).unwrap().replaced, None);
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
        state.worker(&w, true, at);
        state.watch_started(&w, at);
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
        // The lead became the lead before it got the worker mark.
        state.register(&lead, now);
        state.watch_started(&lead, now);
        state.worker(&lead, true, now);
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
        state
            .set_idle(&person_of(&api()), Some(0), None, now)
            .unwrap();
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
        let set = state.set_idle(&person_of(&api()), None, Some(120), now);
        assert_eq!(set.unwrap().after_secs, 120);
        assert!(stopped(&mut state, now + Duration::from_secs(90)).is_empty());

        let idle = state
            .set_idle(&person_of(&api()), Some(2), None, now)
            .unwrap();
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
        state
            .set_idle(&person_of(&api()), Some(0), None, now)
            .unwrap();
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
        state.start(&docs(), StartReason::Process, t(200));
        assert_eq!(info(&state, &docs(), t(230)).claims_secs, 30);
    }

    /// A state with a clock, so that each record has a time: three
    /// sessions in a running riff, and the worker `w1` of mike.
    fn with_worker(now: Instant) -> (State, SessionUri) {
        let mut state = State {
            clock: Some((now, T0)),
            ..State::default()
        };
        for n in [api(), tests(), docs()] {
            state.register(&n, now);
        }
        state.riff(&api(), Some(RiffState::Running), now).unwrap();
        let w1 = uri("riff://mike@pangolin/como-technologies/riff?session=w1#issue-12");
        state.worker(&w1, true, now);
        state.watch_started(&w1, now);
        (state, w1)
    }

    /// 01M3X9XC99KY4RQY36A7CYWY11: `who` shows MustClear, and the time
    /// since the last fresh start.
    #[test]
    fn who_shows_must_clear_and_the_time_since_the_last_fresh_start() {
        let now = Instant::now();
        let (mut state, w1) = with_worker(now);
        let t = |secs| now + Duration::from_secs(secs);
        let shown = |state: &State, secs: u64| {
            let all = state.who(t(secs), T0 + secs * 1000, false);
            let found = all.into_iter().find(|s| s.uri.who() == w1.who());
            found.unwrap()
        };
        // The log has no fresh start of the worker yet: a register only.
        let first = shown(&state, 5);
        assert!(first.worker && !first.must_clear);
        assert_eq!(first.fresh_secs, None);
        assert_eq!(first.state, Some(SessionState::Idle));

        state.start(&w1, StartReason::Process, t(10));
        state.claim(&w1, &repo(), "issue-12", t(20)).unwrap();
        let busy = shown(&state, 40);
        assert_eq!(busy.fresh_secs, Some(30));
        assert_eq!(busy.state, Some(SessionState::Busy));

        state.release(&w1, &repo(), "issue-12", t(50)).unwrap();
        let waits = shown(&state, 60);
        assert!(waits.must_clear);
        assert_eq!(waits.state, Some(SessionState::MustClear));
        assert_eq!(waits.fresh_secs, Some(50));

        // A resume is no fresh start. A clear is one.
        state.start(&w1, StartReason::Resume, t(70));
        assert!(shown(&state, 80).must_clear);
        assert_eq!(shown(&state, 80).fresh_secs, Some(70));
        state.start(&w1, StartReason::Clear, t(90));
        let ready = shown(&state, 100);
        assert!(!ready.must_clear);
        assert_eq!(ready.fresh_secs, Some(10));
        assert_eq!(ready.state, Some(SessionState::Idle));
    }

    /// 01M3X9XB37TQCXWPNFZRMRGJB4: the reply to a keep-alive of a worker
    /// in MustClear carries the ask to clear.
    #[test]
    fn the_reply_to_a_keep_alive_of_a_worker_in_must_clear_asks_it_to_clear() {
        let now = Instant::now();
        let (mut state, w1) = with_worker(now);
        assert!(!state.alive(&w1, now).clear);
        state.claim(&w1, &repo(), "issue-12", now).unwrap();
        assert!(!state.alive(&w1, now).clear);
        assert!(
            state
                .release(&w1, &repo(), "issue-12", now)
                .unwrap()
                .must_clear
        );
        assert!(state.alive(&w1, now).clear);
        assert!(state.alive(&w1, now).clear, "each keep-alive says it");
        assert!(!state.alive(&api(), now).clear);
        state.start(&w1, StartReason::Clear, now);
        assert!(!state.alive(&w1, now).clear);
    }

    /// 01M3X9XBMB3R718Z81BYXTHMZ0: no wake goes to a session in
    /// MustClear. The message stays unread, and its record names the
    /// session.
    #[test]
    fn a_post_to_a_worker_in_must_clear_gives_no_wake_until_its_fresh_start() {
        let now = Instant::now();
        let (mut state, w1) = with_worker(now);
        state.claim(&w1, &repo(), "issue-12", now).unwrap();
        state.release(&w1, &repo(), "issue-12", now).unwrap();
        let request = Post::new(&api(), None, to(&["session=w1"]), "request: claim issue-7");
        let delivery = state.post(request, now, 0).unwrap();
        assert!(delivery.wakes.is_empty(), "no wake");
        assert_eq!(delivery.woken, [w1.who().clone()], "the record names it");
        assert!(state.missed(w1.who()).is_none());

        state.start(&w1, StartReason::Clear, now);
        let wake = state.missed(w1.who()).unwrap();
        assert_eq!(wake.from.who(), api().who());
    }

    /// 01M3X9XD8QWHS2CXTFSQK0PN1Y: the checkpoint holds the worker mark,
    /// the MustClear mark and the time of the last fresh start of each
    /// session.
    #[test]
    fn a_start_from_a_checkpoint_gives_the_life_cycle_of_a_full_replay() {
        let now = Instant::now();
        let (mut state, w1) = with_worker(now);
        let t = |secs| now + Duration::from_secs(secs);
        state.start(&w1, StartReason::Process, t(10));
        state.claim(&w1, &repo(), "issue-12", t(20)).unwrap();
        state.release(&w1, &repo(), "issue-12", t(30)).unwrap();
        state.start(&w1, StartReason::Resume, t(40));
        state.start(&w1, StartReason::Clear, t(50));
        state.claim(&w1, &repo(), "issue-13", t(60)).unwrap();
        state.release(&w1, &repo(), "issue-13", t(70)).unwrap();
        let log: Vec<Record> = state.take_queue();
        let full = State::replay(log.clone(), now, T0);
        let sessions = full.written.sessions();
        assert!(sessions.worker(w1.who()) && sessions.must_clear(w1.who()));
        assert_eq!(sessions.fresh_ms(w1.who()), Some(T0 + 50_000));

        // At each position, also in MustClear and after a fresh start.
        let life = |state: &State| {
            let sessions = state.written.sessions();
            let who = w1.who();
            (
                sessions.worker(who),
                sessions.must_clear(who),
                sessions.fresh_ms(who),
            )
        };
        let mut seen = BTreeSet::new();
        for at in 0..=log.len() {
            let head = State::replay(log[..at].to_vec(), now, T0);
            seen.insert(life(&head));
            let loaded = State::load(Some(through_json(&head, now)), log[at..].to_vec(), now, T0);
            assert!(loaded.same_log_state(&full), "a checkpoint at {at}");
            assert_eq!(life(&loaded), life(&full), "a checkpoint at {at}");
            // The checkpoint alone has the life cycle of its position.
            let alone = State::load(Some(through_json(&head, now)), [], now, T0);
            assert_eq!(life(&alone), life(&head), "a checkpoint at {at}");
            let shown = |state: &State| {
                let all = state.who(now, T0, true);
                let found = all.into_iter().find(|s| s.uri.who() == w1.who());
                found.map(|s| (s.worker, s.must_clear, s.fresh_secs))
            };
            assert_eq!(shown(&alone), shown(&head), "a checkpoint at {at}");
        }
        assert!(seen.contains(&(true, true, Some(T0 + 10_000))));
        assert!(seen.contains(&(true, false, Some(T0 + 50_000))));
    }

    /// A session of the repository `como-technologies/strata`.
    fn strata(id: &str) -> SessionUri {
        uri(&format!(
            "riff://brett@kadomony/como-technologies/strata?session={id}"
        ))
    }

    /// 01M3XAHZBGSSJB3YX23K88W01K: two repositories. A pause in one, and
    /// a claim in the other that works.
    #[test]
    fn a_pause_of_one_repository_lets_the_other_repository_go_on() {
        let now = Instant::now();
        let mut state = setup(now);
        let brett = strata("b1");
        state.register(&brett, now);
        state.watch_started(&brett, now);
        state.watch_started(&docs(), now);
        let other = thread("como-technologies/strata");

        let reply = state
            .pause_repository(&brett, RiffState::Paused, now)
            .unwrap();
        assert!(reply.changed);
        assert_eq!(reply.state, RiffState::Paused);
        assert!(reply.riff.is_none());
        assert_eq!(reply.repositories[0].repository, other);

        // The paused repository: no claim, and its session is paused.
        let refused = state.claim(&brett, &other, "issue-3", now).unwrap_err();
        assert!(
            refused.contains(
                "the repository como-technologies/strata is paused by the session brett/b1"
            ),
            "{refused}"
        );
        assert_eq!(info(&state, &brett, now).state, Some(SessionState::Paused));
        // The other repository goes on.
        state.claim(&docs(), &repo(), "issue-12", now).unwrap();
        assert_eq!(info(&state, &docs(), now).state, Some(SessionState::Busy));
        assert_eq!(state.pauses_at(&docs()).state, RiffState::Running);

        // A second pause changes nothing. A resume ends the pause.
        let again = state
            .pause_repository(&brett, RiffState::Paused, now)
            .unwrap();
        assert!(!again.changed);
        let resumed = state
            .pause_repository(&brett, RiffState::Running, now)
            .unwrap();
        assert!(resumed.changed && resumed.repositories.is_empty());
        state.claim(&brett, &other, "issue-3", now).unwrap();
    }

    /// 01M3XAHZBGSSJB3YX23K88W01K: a resume of a repository while the
    /// riff is paused changes only the pause of the repository, and the
    /// reply has the pause of the riff.
    #[test]
    fn a_resume_of_a_repository_in_a_paused_riff_leaves_the_riff_paused() {
        let now = Instant::now();
        let mut state = setup(now);
        state
            .pause_repository(&api(), RiffState::Paused, now)
            .unwrap();
        state.riff(&api(), Some(RiffState::Paused), now).unwrap();
        let reply = state
            .pause_repository(&api(), RiffState::Running, now)
            .unwrap();
        assert!(reply.changed);
        assert_eq!(reply.state, RiffState::Paused);
        assert_eq!(
            reply.riff.unwrap().by.unwrap().to_string(),
            "the session mike/a1"
        );
        assert!(reply.repositories.is_empty());
        let refused = state.claim(&docs(), &repo(), "issue-12", now).unwrap_err();
        assert!(refused.contains("the riff is paused by"), "{refused}");
    }

    /// 01M3XAHZQ92GGFHBC50FQ7FQ0K: the checkpoint holds each pause, with
    /// who set it and when. A start from a checkpoint gives the state of
    /// a full replay.
    #[test]
    fn a_start_from_a_checkpoint_gives_the_pauses_of_a_full_replay() {
        let now = Instant::now();
        let mut state = State::with_writer(now, T0);
        let make_riff = MakeRiff {
            riff_id: "r1".into(),
        };
        state.run(&Caller::server(), &make_riff, now).unwrap();
        let brett = strata("b1");
        for me in [api(), brett.clone()] {
            state.register(&me, now);
        }
        let later = now + Duration::from_secs(5);
        state
            .pause_repository(&brett, RiffState::Paused, later)
            .unwrap();
        state.riff(&api(), Some(RiffState::Running), later).unwrap();
        state
            .pause_repository(&api(), RiffState::Paused, later)
            .unwrap();
        state
            .pause_repository(&brett, RiffState::Running, later)
            .unwrap();
        state.riff(&api(), Some(RiffState::Paused), later).unwrap();
        let log: Vec<Record> = state.take_queue();
        let full = State::replay(log.clone(), now, T0);
        let pauses = full.pauses().clone();
        let by = |pause: &riff_core::wire::PauseInfo| pause.by.as_ref().unwrap().to_string();
        assert_eq!(by(pauses.riff().unwrap()), "the session mike/a1");
        assert_eq!(pauses.riff().unwrap().at_ms, T0 + 5000);
        assert_eq!(
            by(pauses.repository(&repo()).unwrap()),
            "the session mike/a1"
        );
        assert!(
            pauses
                .repository(&thread("como-technologies/strata"))
                .is_none()
        );

        for at in 0..=log.len() {
            let head = State::replay(log[..at].to_vec(), now, T0);
            let loaded = State::load(Some(through_json(&head, now)), log[at..].to_vec(), now, T0);
            assert!(loaded.same_log_state(&full), "a checkpoint at {at}");
            assert_eq!(loaded.pauses(), &pauses, "a checkpoint at {at}");
            // The pauses of the checkpoint alone.
            let alone = State::load(Some(through_json(&head, now)), [], now, T0);
            assert_eq!(alone.pauses(), head.pauses(), "a checkpoint at {at}");
        }
    }

    /// 01M3XAHZQ92GGFHBC50FQ7FQ0K: a checkpoint from before the pause of
    /// a repository reads. Nobody is known to have set its pause.
    #[test]
    fn a_checkpoint_from_before_the_pause_of_a_repository_reads() {
        let now = Instant::now();
        for (riff, paused) in [("paused", true), ("running", false)] {
            let json = format!(r#"{{"position":3,"riff":"{riff}"}}"#);
            let old: Snapshot = serde_json::from_str(&json).unwrap();
            let state = State::load(Some(old), [], now, T0);
            assert_eq!(state.pauses().riff().is_some(), paused);
            assert!(state.pauses().riff().is_none_or(|p| p.by.is_none()));
            // This build writes the same fields for it.
            let written = serde_json::to_string(&state.snapshot(now, T0)).unwrap();
            assert!(
                !written.contains("riff_pause") && !written.contains("pauses"),
                "{written}"
            );
        }
    }

    /// 01M3XAHZMN8P0PRD0Q7881TEF9: the idle rule is for each user, host
    /// and repository. An idle worker in one repository does not stop
    /// the idle worker of the same user and host in another repository.
    #[test]
    fn an_idle_worker_in_one_repository_does_not_stop_one_in_another() {
        let now = Instant::now();
        let mut state = State::default();
        running(&mut state).unwrap();
        let in_repo = |repo: &str, id: &str| {
            uri(&format!(
                "riff://mike@pangolin/como-technologies/{repo}?session={id}"
            ))
        };
        for (repo, id, at) in [("riff", "r1", 0), ("dotfiles", "d1", 1)] {
            let lead = in_repo(repo, &format!("lead-{repo}"));
            state.register(&lead, now);
            let worker = in_repo(repo, id);
            let at = now + Duration::from_secs(at);
            state.watch_started(&worker, at);
            state.worker(&worker, true, at);
        }
        let later = now + Duration::from_secs(90);
        assert!(stopped(&mut state, later).is_empty());

        // A second idle worker in one of the two repositories stops.
        let second = in_repo("riff", "r2");
        let at = now + Duration::from_secs(2);
        state.watch_started(&second, at);
        state.worker(&second, true, at);
        assert_eq!(stopped(&mut state, later), ["r1"]);
    }

    /// A status of a session from before a pause of its repository is
    /// stale. A pause of another repository does not change it.
    #[test]
    fn a_status_from_before_a_pause_of_its_repository_is_stale() {
        let now = Instant::now();
        let mut state = setup(now);
        let brett = strata("b1");
        state.register(&brett, now);
        let t = |secs| now + Duration::from_secs(secs);
        set_step(&mut state, &docs(), "tests", t(1));
        set_step(&mut state, &brett, "docs", t(1));
        state
            .pause_repository(&brett, RiffState::Paused, t(2))
            .unwrap();
        assert!(stale(&state, &brett, t(2)));
        assert!(!stale(&state, &docs(), t(2)));
    }
}

#[cfg(test)]
mod rules;
