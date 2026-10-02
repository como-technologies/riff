//! The command engine: the one path of each change of the state.
//!
//! # Design
//!
//! The [`Engine`] owns the [`State`], its lock and the queue of the
//! writer. They are private fields of this module, so the engine is the
//! only code that locks the state (01M3WRD8WJ2JF9077PRDX04T9A). The
//! files of the commands are next to this module, in [`crate::state`],
//! not below it. So no command can make a stage or lock the state.
//!
//! ```mermaid
//! flowchart TD
//!     C[call with a token] --> A["Engine::authenticate:<br/>Authenticated&lt;C&gt;"]
//!     A -->|denied| E0[403]
//!     A --> K{sort of call}
//!     K -->|query| Q["Engine::query:<br/>the written copy"] --> QR[reply]
//!     K -->|signal| P["Engine::signal:<br/>the presence"] --> QR
//!     K -->|command| H["Engine::check: permits, then handle<br/>under the lock: Checked"]
//!     H -->|"accepted, or refused"| U["Checked::queue: positions, the entry,<br/>the pending copy: Queued"]
//!     U --> W["the writer writes the chunk<br/>outside the lock"]
//!     W -->|failed for good| S[503, the instance stops]
//!     W --> AP["Engine::finish: the written copy,<br/>in order, then the effects"]
//!     AP -->|"Applied, accepted"| R[the call makes the reply<br/>from the written copy]
//!     AP -->|"Applied, refused"| E[error to the caller]
//! ```
//!
//! - [`Engine::dispatch`] is the one path of each command. A command is
//!   a type that implements [`Command`]. A command that a client can
//!   send also implements [`Routed`], and [`command`] is the one handler
//!   of each (01M3WRD8TBDPA4JNEZY6J4N2EX).
//! - [`Engine::signal`] is the one path of each signal: a change of the
//!   presence only (01M3WRD97EZJK3AABXECXEY133).
//! - [`Engine::query`], [`Engine::peek`] and [`Engine::read`] read the
//!   written copy. They get the state as `&State`, so they change
//!   nothing.
//! - The server is a caller too. The engine has one function for each
//!   command of the server: [`Engine::make_riff`], [`Engine::announce`]
//!   and [`Engine::forget`]. No HTTP call can send them.
//!
//! # The stages
//!
//! The stages of a command are types. Each one is made from the stage
//! before it, and only this module can make one
//! (01M3WRD8YQFSKR2PENZC6CX24B):
//!
//! | Stage | Made by | It proves |
//! |---|---|---|
//! | [`Authenticated<C>`] | [`Engine::authenticate`] with the proof of the token layer, or a function of the engine | The caller is the caller of the token, and may act as the `me` of the body. |
//! | [`Checked<'s, C>`](Checked) | `Engine::check`, in the call | `permits` and `handle` ran: the command is accepted or refused. It holds the lock of the state. |
//! | [`Queued<C>`] | `Checked::queue`, in the call | The records have positions. The entry of the command is in the queue, and its records are in the pending copy. The lock is free. |
//! | [`Applied<C>`] | `Queued::applied`, from the word of the writer | The chunk of each record is in the log, and the written copy has the records, in order. The effects of the command are done. |
//!
//! Only `Applied` gives the reply. So a handler cannot reply before the
//! write.
//!
//! # The writer finishes each command
//!
//! The writer is one task of the server (01M3WRD90WBBCWTDGVQCBR6MNT).
//! It takes each entry of the queue ([`Engine::take`]), writes their
//! records as one chunk outside the lock, and gives the chunk back
//! ([`Engine::finish`]). `finish` applies the records to the written
//! copy in the order of their positions, sends the wakes and the `tail`
//! events of each `posted` record, and then tells each call that its
//! entry is done. The call only waits, and makes the reply. A call that
//! the client drops loses only its reply.
//!
//! Each command has one entry, also a command that makes no record, and
//! a command that is refused (01M3WRD933ESXF33WDEDFCRFB8). The queue is
//! in order, so an entry is done only after each entry before it. So no
//! reply and no refusal tells of a change that is not in the log.
//!
//! # Example
//!
//! ```
//! # #[tokio::main(flavor = "current_thread")] async fn main() {
//! use std::time::Instant;
//! use riff_core::wire::{Join, Resume};
//! use riff_server::engine::{Engine, Open};
//! use riff_server::state::State;
//!
//! let engine = Engine::new(State::with_writer(Instant::now(), 0), Open);
//! // The writer: the server writes each chunk to the log here.
//! let writer = engine.clone();
//! tokio::spawn(async move {
//!     loop {
//!         match writer.take() {
//!             Some(chunk) => writer.finish(chunk),
//!             None => writer.queued().notified().await,
//!         }
//!     }
//! });
//!
//! let me: riff_core::name::SessionUri =
//!     "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse().unwrap();
//! let join = Join { me: me.clone(), thread: "design".parse().unwrap() };
//! let call = engine.authenticate(None, join).unwrap();
//! engine.dispatch(call).await.unwrap();
//!
//! // A session that is the first of its user is the lead: it resumes.
//! let call = engine.authenticate(None, Resume { me }).unwrap();
//! assert!(engine.dispatch(call).await.unwrap().changed);
//! # }
//! ```

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::{FromRef, FromRequest, Request, State as AxumState};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use riff_core::name::{SessionUri, Who};
use riff_core::record::{Change, Record};
use riff_core::wire::{
    AliveReply, Call, Claim, End, Join, Keys, Lead, Leave, Pause, Post, REFUSED_HEADER, Register,
    Release, ReleaseFor, Resume, SetIdle, Start, Tailed, Wake,
};
use tokio::sync::{Notify, broadcast, oneshot};

use crate::auth::SignedIn;
use crate::state::{
    Announce, Caller, Cause, Check, Code, Command, Delivery, Forget, MakeRiff, Refused, Role,
    Signal, State, Stopping,
};

/// Events that a slow stream may miss before it drops them.
const EVENT_BUFFER: usize = 1024;

/// What the engine asks about the people of the riff. Until E3 (#393),
/// the roles and the keys come from the token store.
pub trait People: Send + Sync + 'static {
    /// True when a call needs the proof of the token layer.
    fn needs_sign_in(&self) -> bool;

    /// True for a riff with no sign-in: each reader counts its messages
    /// as verified (R211).
    fn trusted(&self) -> bool;

    /// The role of `user`.
    fn role(&self, user: &str) -> Role;

    /// The thumbprints of the device keys of the live sign-ins of
    /// `user` (R199).
    fn keys(&self, user: &str) -> Vec<String>;
}

/// The people of a riff with no sign-in: it trusts its network. Each
/// caller has the role of an admin (01M3WRD9G5GAF65EX8P6D5DMQM).
#[derive(Clone, Copy, Debug, Default)]
pub struct Open;

impl People for Open {
    fn needs_sign_in(&self) -> bool {
        false
    }

    fn trusted(&self) -> bool {
        true
    }

    fn role(&self, _user: &str) -> Role {
        Role::Admin
    }

    fn keys(&self, _user: &str) -> Vec<String> {
        Vec::new()
    }
}

// ANCHOR: routed
/// A command that a client can send: a [`Call`] that is a [`Command`]
/// with the same reply (01M3WRD8TBDPA4JNEZY6J4N2EX). [`command`] is the
/// handler of each, and the router has one line for each:
/// `.route(C::PATH, post(command::<C>))`.
pub trait Routed: Call + Command<Reply = <Self as Call>::Reply> {
    /// The session that the body names, for the token layer.
    fn me(&self) -> Option<&SessionUri>;

    /// Lets the token layer check and fill the body before the engine
    /// gets the command. Only a post has such a check: its signature.
    fn prepare(&mut self, _proof: Option<&SignedIn>, _now_ms: u64) -> Result<(), String> {
        Ok(())
    }
}
// ANCHOR_END: routed

/// Gives each command with a `me` its [`Routed`].
macro_rules! routed {
    ($($command:ty),*) => {
        $(impl Routed for $command {
            fn me(&self) -> Option<&SessionUri> {
                Some(&self.me)
            }
        })*
    };
}

routed!(
    Register, Start, End, Join, Leave, Claim, Release, ReleaseFor, Lead, Pause, Resume, SetIdle
);

/// With sign-in, a post needs a valid signature from the key of its
/// token over its payload (R197), and the message keeps the payload and
/// the signature. Without sign-in, the message keeps no signature
/// (R201). The time of the message is the signed time, or the time of
/// the server.
impl Routed for Post {
    fn me(&self) -> Option<&SessionUri> {
        Some(&self.me)
    }

    fn prepare(&mut self, proof: Option<&SignedIn>, now_ms: u64) -> Result<(), String> {
        let at_ms = match proof {
            Some(proof) => proof.check_post(self, now_ms)?,
            None => {
                self.sig = None;
                self.payload = None;
                now_ms
            }
        };
        self.at_ms = Some(at_ms);
        Ok(())
    }
}

// ANCHOR: failed
/// Why a call gets no reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failed {
    /// The token layer refused the call: 403.
    Denied(String),
    /// `permits` or `handle` refused the command: 400, 403 or 409, by
    /// its code (01M3WRD9JBQMNN96TXJH8EAJ3W).
    Refused(Refused),
    /// The chunk was not written, and the instance stops: 503.
    Stopped,
}
// ANCHOR_END: failed

impl Failed {
    /// The HTTP status of the failure.
    ///
    /// ```
    /// use axum::http::StatusCode;
    /// use riff_server::engine::Failed;
    /// use riff_server::state::{Code, Refused};
    ///
    /// let held = Failed::Refused(Refused::new(Code::Held, "issue-7 is held"));
    /// assert_eq!(held.status(), StatusCode::CONFLICT);
    /// assert_eq!(Failed::Stopped.status(), StatusCode::SERVICE_UNAVAILABLE);
    /// ```
    pub fn status(&self) -> StatusCode {
        match self {
            Failed::Denied(_) => StatusCode::FORBIDDEN,
            Failed::Refused(refused) => match refused.code {
                Code::NotAllowed => StatusCode::FORBIDDEN,
                Code::Held | Code::Paused | Code::NotHolder | Code::OtherUser => {
                    StatusCode::CONFLICT
                }
                Code::BadRequest => StatusCode::BAD_REQUEST,
            },
            Failed::Stopped => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    /// The text for the caller.
    pub fn text(&self) -> String {
        match self {
            Failed::Denied(why) => why.clone(),
            Failed::Refused(refused) => refused.reason.clone(),
            Failed::Stopped => "the server stopped before it wrote the change. Try again.".into(),
        }
    }
}

impl From<Refused> for Failed {
    fn from(refused: Refused) -> Failed {
        Failed::Refused(refused)
    }
}

impl From<Failed> for (StatusCode, String) {
    fn from(failed: Failed) -> (StatusCode, String) {
        (failed.status(), failed.text())
    }
}

/// The reply to a failed call: its status, and its text. A refusal
/// also names its code, in the header [`REFUSED_HEADER`].
impl IntoResponse for Failed {
    fn into_response(self) -> Response {
        let reply = (self.status(), self.text());
        match &self {
            Failed::Refused(refused) => {
                ([(REFUSED_HEADER, refused.code.as_str())], reply).into_response()
            }
            Failed::Denied(_) | Failed::Stopped => reply.into_response(),
        }
    }
}

/// A caller that the token layer admitted: it is the caller of the
/// token, and it may act as its `me`. Only [`Engine::admit`] and the
/// functions of the engine make one. A signal and a query need it.
#[derive(Clone, Debug)]
pub struct Admitted {
    caller: Caller,
    /// True when a token proved the caller. If not, the riff took the
    /// call with no token: the caller has the role of an admin.
    proved: bool,
}

impl Admitted {
    /// The caller.
    pub fn caller(&self) -> &Caller {
        &self.caller
    }

    /// Who the caller is.
    pub fn who(&self) -> &Who {
        self.caller.who()
    }
}

/// The first stage: a command with its caller. See the module docs.
///
/// Code outside this module cannot make the next stage. This does not
/// compile, because `check` is private:
///
/// ```compile_fail,E0624
/// use riff_core::wire::Claim;
/// use riff_server::engine::{Authenticated, Engine};
///
/// fn send(engine: &Engine, call: Authenticated<Claim>) {
///     let _ = engine.check(call);
/// }
/// ```
///
/// Its twin differs in one line: it goes through the one path.
///
/// ```
/// use riff_core::wire::Claim;
/// use riff_server::engine::{Authenticated, Engine};
///
/// fn send(engine: &Engine, call: Authenticated<Claim>) {
///     let _ = engine.dispatch(call);
/// }
/// ```
pub struct Authenticated<C> {
    admitted: Admitted,
    command: C,
}

/// The second stage: `permits` and `handle` ran. It holds the lock of
/// the state, so no other call comes between the check and the queue.
pub struct Checked<'s, C: Command> {
    engine: &'s Engine,
    core: MutexGuard<'s, Core>,
    command: C,
    check: Check<C::Note>,
    now: Instant,
}

/// The third stage: the entry of the command is in the queue. The lock
/// is free.
pub struct Queued<C: Command> {
    engine: Engine,
    caller: Caller,
    command: C,
    outcome: Result<C::Note, Refused>,
    done: oneshot::Receiver<Done>,
    now: Instant,
}

/// The last stage: the writer is done with the entry of the command.
/// Only it gives the reply.
pub struct Applied<C: Command> {
    engine: Engine,
    caller: Caller,
    command: C,
    outcome: Result<C::Note, Refused>,
    made: Vec<Record>,
    now: Instant,
}

// ANCHOR: entry
/// One command in the queue of the writer: accepted (with its records,
/// or none) or refused (no record).
struct Entry {
    made: Vec<Record>,
    /// Tells the call that the entry is done. The `register` that the
    /// engine runs first for a caller has no call of its own.
    done: Option<oneshot::Sender<Done>>,
}

/// The word of the writer to the call: the entry is done.
struct Done {
    made: Vec<Record>,
}
// ANCHOR_END: entry

/// The entries that the writer took from the queue: the records of one
/// chunk of the log. The writer writes [`Chunk::records`], and gives the
/// chunk to [`Engine::finish`]. A chunk that is dropped tells each of
/// its calls that the server stopped.
pub struct Chunk {
    entries: Vec<Entry>,
}

impl Chunk {
    /// The records of the chunk, in the order of their positions.
    pub fn records(&self) -> Vec<Record> {
        self.entries
            .iter()
            .flat_map(|entry| entry.made.iter().cloned())
            .collect()
    }
}

/// The state and the queue, behind the one lock.
struct Core {
    state: State,
    /// Each entry that waits for the writer, in order.
    queue: Vec<Entry>,
    /// True once the server stopped for good: no entry is done.
    stopped: bool,
}

struct Shared {
    core: Mutex<Core>,
    /// Wakes the writer when an entry waits in the queue.
    queued: Arc<Notify>,
    wakes: broadcast::Sender<(Who, Wake)>,
    tail: broadcast::Sender<Tailed>,
    people: Box<dyn People>,
}

/// The command engine of one `riff-server`. Clones share the same
/// engine. See the module docs.
#[derive(Clone)]
pub struct Engine(Arc<Shared>);

impl Engine {
    /// An engine that owns `state`. `people` gives the roles and the
    /// keys.
    pub fn new(state: State, people: impl People) -> Engine {
        let (wakes, _) = broadcast::channel(EVENT_BUFFER);
        let (tail, _) = broadcast::channel(EVENT_BUFFER);
        Engine(Arc::new(Shared {
            core: Mutex::new(Core {
                state,
                queue: Vec::new(),
                stopped: false,
            }),
            queued: Arc::new(Notify::new()),
            wakes,
            tail,
            people: Box::new(people),
        }))
    }

    fn core(&self) -> MutexGuard<'_, Core> {
        // A panic while the lock is held leaves plain data behind; keep going.
        self.0
            .core
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Admits the caller of a call that acts as `me`. `proof` is the
    /// proof of the token layer: who the access token of the request
    /// acts as. It refuses a `me` that is another user or another
    /// session than the token (R104).
    ///
    /// A riff with no sign-in trusts its network, and takes a call with
    /// no proof: the caller is then the `me` of the body, with the role
    /// of an admin (01M3WRD9G5GAF65EX8P6D5DMQM). A riff with sign-in
    /// refuses such a call.
    pub fn admit(&self, proof: Option<&SignedIn>, me: &SessionUri) -> Result<Admitted, Failed> {
        match proof {
            Some(proof) => proof.may_act_as(me.who()).map_err(Failed::Denied)?,
            None if self.0.people.needs_sign_in() => {
                return Err(Failed::Denied(
                    "this riff needs a sign-in: the call has no token".into(),
                ));
            }
            None => {}
        }
        Ok(Admitted {
            caller: Caller::of(me),
            proved: proof.is_some(),
        })
    }

    /// The first stage of a routed command: admits its caller
    /// ([`Engine::admit`]), and lets the token layer check the body
    /// ([`Routed::prepare`]). A command whose body names no `me` needs
    /// the proof of a token: the caller is then the caller of the
    /// token (01M3WRD9G5GAF65EX8P6D5DMQM).
    pub fn authenticate<C: Routed>(
        &self,
        proof: Option<&SignedIn>,
        mut command: C,
    ) -> Result<Authenticated<C>, Failed> {
        let me = match (command.me(), proof) {
            (Some(me), _) => me.clone(),
            (None, Some(proof)) => {
                let place = crate::owner::server_uri().place().clone();
                SessionUri::new(proof.who.clone(), place)
            }
            (None, None) => {
                return Err(Failed::Denied(
                    "a call that names no sender needs a token".into(),
                ));
            }
        };
        let admitted = self.admit(proof, &me)?;
        command.prepare(proof, now_ms()).map_err(Failed::Denied)?;
        Ok(Authenticated { admitted, command })
    }

    /// The first stage of a command of the server itself.
    fn as_server<C: Command>(command: C) -> Authenticated<C> {
        Authenticated {
            admitted: Admitted {
                caller: Caller::server(),
                proved: false,
            },
            command,
        }
    }

    // ANCHOR: dispatch
    /// The one path of each command (01M3WRD8WJ2JF9077PRDX04T9A): the
    /// check under the lock, the entry in the queue, the wait for the
    /// writer, and then the reply or the refusal. A refused command
    /// waits as each command does (01M3WRD933ESXF33WDEDFCRFB8).
    ///
    /// The rule "no `await` while `Checked` lives" has no doc test.
    /// `Checked` holds the guard of a `std::sync::Mutex`, which is not
    /// `Send`. Only this function can hold it over an `await`, and then
    /// the future of [`command`] is not `Send`, so the one handler does
    /// not build. The error is at each line of the router. It does not
    /// name the lock. A test build with one `await` between the check
    /// and the queue gave it:
    ///
    /// ```text
    /// error[E0277]: the trait bound `fn(State<Engine>, ...) -> ... {command::<...>}: Handler<_, _>` is not satisfied
    ///     --> crates/riff-server/src/lib.rs:1465:41
    ///      |
    /// 1465 |             .route(Register::PATH, post(command::<Register>))
    ///      |                                    ---- ^^^^^^^^^^^^^^^^^^^ the trait `Handler<_, _>` is not implemented for fn item `fn(State<Engine>, Authenticated<Register>) -> ... {command::<...>}`
    /// ```
    pub async fn dispatch<C: Command>(&self, call: Authenticated<C>) -> Result<C::Reply, Failed> {
        let checked: Checked<'_, C> = self.check(call); // lock, permits, handle, signal
        let queued: Queued<C> = checked.queue(); // positions, the entry
        let applied: Applied<C> = queued.applied().await?; // the writer did the rest
        applied.reply() // the reply, or the refusal
    }
    // ANCHOR_END: dispatch

    /// Checks a command under the lock of the state
    /// ([`State::check`]). The caller carries its class from the token
    /// layer. The engine adds its role here, under the lock, and the
    /// state adds its worker mark, before `permits`
    /// (01M3WRD959DYNZHDKP5ZT9Q1C7).
    fn check<C: Command>(&self, call: Authenticated<C>) -> Checked<'_, C> {
        let now = Instant::now();
        let Authenticated { admitted, command } = call;
        let mut core = self.core();
        let caller = self.with_role(&admitted);
        let check = core.state.check(&caller, &command, now);
        Checked {
            engine: self,
            core,
            command,
            check,
            now,
        }
    }

    /// The caller of `admitted` with its role. Until E3 (#393), the
    /// role comes from the token store.
    fn with_role(&self, admitted: &Admitted) -> Caller {
        let role = if admitted.proved {
            self.0.people.role(admitted.who().user())
        } else {
            Role::Admin
        };
        admitted.caller.clone().with_role(role)
    }

    /// Sends a command, and does not wait for the writer. The writer
    /// finishes the command: a call that is gone loses only its reply.
    fn send<C: Command>(&self, call: Authenticated<C>) {
        drop(self.check(call).queue());
    }

    /// The first start of a riff: sends the command `make_riff` of the
    /// server (01M3WRD99M99PNGP8ME50KC6WS). It changes nothing in a riff
    /// that has a record. It does not wait for the write.
    pub fn make_riff(&self) {
        self.send(Engine::as_server(MakeRiff));
    }

    /// Posts a note or a message of the server itself: the command
    /// `announce`. Gives each session that it woke.
    pub async fn announce(&self, announce: Announce) -> Result<Vec<Who>, Failed> {
        self.dispatch(Engine::as_server(announce)).await
    }

    /// Forgets each session with no sign of life for
    /// [`crate::state::SESSION_EXPIRY`]: the command `forget` of the
    /// server. Gives the number of sessions that it forgot.
    pub async fn forget(&self) -> Result<usize, Failed> {
        self.dispatch(Engine::as_server(Forget)).await
    }

    /// Waits until the writer is done with each entry that is in the
    /// queue now.
    pub async fn settle(&self) -> Result<(), Failed> {
        let (tx, rx) = oneshot::channel();
        {
            let mut core = self.core();
            if core.stopped {
                return Err(Failed::Stopped);
            }
            core.queue.push(Entry {
                made: Vec::new(),
                done: Some(tx),
            });
        }
        self.0.queued.notify_one();
        rx.await.map(drop).map_err(|_| Failed::Stopped)
    }

    /// Makes the state know the caller, for a signal or a query. A
    /// caller that the state does not know first sends `register`
    /// through [`Engine::dispatch`], and waits for its write
    /// (01M3WRD97EZJK3AABXECXEY133). It refuses a session ID that the
    /// state knows under another user (R159).
    async fn known(&self, admitted: &Admitted) -> Result<(), Failed> {
        let me = admitted.caller.me();
        let known = {
            let core = self.core();
            core.state
                .check_user(me)
                .map_err(|reason| Refused::new(Code::OtherUser, reason))?;
            core.state.knows(me.who())
        };
        if known {
            return Ok(());
        }
        let register = Register {
            me: me.clone(),
            worker: false,
        };
        self.dispatch(Authenticated {
            admitted: admitted.clone(),
            command: register,
        })
        .await
    }

    /// Sets a signal in the presence, with no check of the caller.
    fn set(&self, who: &Who, signal: Signal) -> AliveReply {
        self.core().state.signal(who, signal, Instant::now())
    }

    /// The one path of each signal: a change of the presence only
    /// (01M3WRD97EZJK3AABXECXEY133). It makes no entry in the queue, and
    /// it does not wait for the writer. A signal of a session that the
    /// state does not know first registers the session. The reply says
    /// if the server asks the session to stop.
    pub async fn signal(&self, caller: &Admitted, signal: Signal) -> Result<AliveReply, Failed> {
        self.known(caller).await?;
        Ok(self.set(caller.who(), signal))
    }

    /// The signal of a watch stream that closed. The session of the
    /// stream is known, and the close is no call of it.
    pub fn watch_ended(&self, who: &Who) {
        self.set(who, Signal::WatchEnded);
    }

    /// Asks each idle worker past the limit to stop: a signal of the
    /// server for each ([`State::stop_idle_workers`]). Gives each.
    pub fn stop_idle_workers(&self) -> Vec<Stopping> {
        self.core().state.stop_idle_workers(Instant::now())
    }

    /// A query of a caller: it reads the written copy. The query is a
    /// call of the caller, so the engine sets that signal first. A
    /// caller that the state does not know registers first, and the
    /// query waits for that write.
    pub async fn query<T>(
        &self,
        caller: &Admitted,
        read: impl FnOnce(&State) -> T,
    ) -> Result<T, Failed> {
        let place = caller.caller.me().place().clone();
        self.signal(caller, Signal::Called { place }).await?;
        Ok(read(&self.core().state))
    }

    /// A read for a caller that is not a call of it: it sets no signal,
    /// and it registers nothing. It refuses a session ID that the state
    /// knows under another user (R159).
    pub fn peek<T>(&self, caller: &Admitted, read: impl FnOnce(&State) -> T) -> Result<T, Failed> {
        let core = self.core();
        core.state
            .check_user(caller.caller.me())
            .map_err(|reason| Refused::new(Code::OtherUser, reason))?;
        Ok(read(&core.state))
    }

    /// A read of the server itself, for example for a checkpoint or for
    /// the facts.
    pub fn read<T>(&self, read: impl FnOnce(&State) -> T) -> T {
        read(&self.core().state)
    }

    /// A stream of each wake, for the watch streams.
    pub fn wakes(&self) -> broadcast::Receiver<(Who, Wake)> {
        self.0.wakes.subscribe()
    }

    /// A stream of each new message, for the `tail` streams.
    pub fn tail(&self) -> broadcast::Receiver<Tailed> {
        self.0.tail.subscribe()
    }

    /// Wakes the writer when an entry waits in the queue. The writer
    /// waits on it when [`Engine::take`] gives nothing.
    pub fn queued(&self) -> Arc<Notify> {
        self.0.queued.clone()
    }

    /// Takes each entry of the queue, for the writer. `None` when the
    /// queue is empty.
    pub fn take(&self) -> Option<Chunk> {
        let entries = std::mem::take(&mut self.core().queue);
        (!entries.is_empty()).then_some(Chunk { entries })
    }

    // ANCHOR: finish
    /// Finishes each command of a chunk that the writer wrote
    /// (01M3WRD90WBBCWTDGVQCBR6MNT). It applies the records to the
    /// written copy in the order of their positions, under the lock.
    /// Then, for each entry in order, it sends the effects and tells the
    /// call that the entry is done. The call can be gone: the change is
    /// done.
    pub fn finish(&self, chunk: Chunk) {
        {
            let mut core = self.core();
            for entry in &chunk.entries {
                core.state.written(&entry.made);
            }
        }
        for entry in chunk.entries {
            self.effects(&entry.made);
            if let Some(done) = entry.done {
                let _ = done.send(Done { made: entry.made });
            }
        }
    }
    // ANCHOR_END: finish

    /// Sends the effects of the written records of one command: the
    /// wakes and the `tail` event of each `posted` record. With
    /// sign-in, the event holds the keys of the sender, so that a
    /// reader verifies the message (R199).
    fn effects(&self, made: &[Record]) {
        for record in made {
            let Change::Posted(posted) = &record.change else {
                continue;
            };
            let mut delivery = Delivery::of(posted);
            let from = posted.message.from.who().user();
            if posted.message.sig.is_some() {
                let keys = self.0.people.keys(from);
                delivery.tailed.keys = Keys::from([(from.to_owned(), keys)]);
            }
            delivery.tailed.trusted = self.0.people.trusted();
            // A send fails only when nobody listens. That is not an error.
            for wake in delivery.wakes {
                let _ = self.0.wakes.send(wake);
            }
            let _ = self.0.tail.send(delivery.tailed);
        }
    }

    /// Stops for good: no entry is done from now on. Each call that
    /// waits, and each later command, fails with [`Failed::Stopped`].
    pub fn stop(&self) {
        let waiting = {
            let mut core = self.core();
            core.stopped = true;
            std::mem::take(&mut core.queue)
        };
        drop(waiting);
    }
}

impl<'s, C: Command> Checked<'s, C> {
    /// Gives each change its position, and puts the entry of the
    /// command in the queue. The records are in the pending copy. A
    /// refused command has an entry with no record. The `register` that
    /// ran first has an entry of its own, before it.
    fn queue(self) -> Queued<C> {
        let Checked {
            engine,
            mut core,
            command,
            check,
            now,
        } = self;
        let Check {
            registered,
            caller,
            result,
        } = check;
        let (tx, done) = oneshot::channel();
        let (made, outcome) = match result {
            Ok((changes, note)) => {
                let cause = Cause::of(&caller, C::KIND);
                (core.state.queue(&cause, &changes, now), Ok(note))
            }
            Err(refused) => (Vec::new(), Err(refused)),
        };
        // A stopped engine drops the sender: the call fails as stopped.
        if !core.stopped {
            if let Some(made) = registered {
                core.queue.push(Entry { made, done: None });
            }
            core.queue.push(Entry {
                made,
                done: Some(tx),
            });
        }
        drop(core);
        engine.0.queued.notify_one();
        Queued {
            engine: engine.clone(),
            caller,
            command,
            outcome,
            done,
            now,
        }
    }
}

impl<C: Command> Queued<C> {
    /// Waits for the word of the writer: the entry is done. When the
    /// server stops first, the call fails.
    async fn applied(self) -> Result<Applied<C>, Failed> {
        let Done { made } = self.done.await.map_err(|_| Failed::Stopped)?;
        Ok(Applied {
            engine: self.engine,
            caller: self.caller,
            command: self.command,
            outcome: self.outcome,
            made,
            now: self.now,
        })
    }
}

impl<C: Command> Applied<C> {
    /// The reply, from the written copy, or the refusal.
    fn reply(self) -> Result<C::Reply, Failed> {
        let note = self.outcome?;
        let core = self.engine.core();
        Ok(core
            .state
            .reply(&self.caller, &self.command, &self.made, note, self.now))
    }
}

/// Makes [`Authenticated`] from a request: the proof that the token
/// layer put in the request, and the JSON body.
impl<S, C> FromRequest<S> for Authenticated<C>
where
    C: Routed,
    Engine: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> Result<Self, Response> {
        let engine = Engine::from_ref(state);
        let proof = request.extensions().get::<SignedIn>().cloned();
        let Json(command) = Json::<C>::from_request(request, state)
            .await
            .map_err(IntoResponse::into_response)?;
        engine
            .authenticate(proof.as_ref(), command)
            .map_err(IntoResponse::into_response)
    }
}

// ANCHOR: handler
/// The handler of each routed command. The router makes one route for
/// each: `.route(C::PATH, post(command::<C>))`.
pub async fn command<C: Routed>(
    AxumState(engine): AxumState<Engine>,
    call: Authenticated<C>,
) -> Result<Json<<C as Call>::Reply>, Failed> {
    engine.dispatch(call).await.map(Json)
}
// ANCHOR_END: handler

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}
