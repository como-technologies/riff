//! The sync of the records of a riff of peers over iroh (#842).
//!
//! # Design
//!
//! Each daemon binds one iroh endpoint at its start ([`Links::bind`],
//! 01M519Z2QE6QYPJX8HVY17GR6Z): the key of the daemon is the key of the
//! endpoint, and the ALPN is [`ALPN`]. [`Net::N0`] uses the DNS and the
//! public relays of n0. [`Net::Local`] uses no relay and no DNS: only the
//! addresses of a lookup on this machine, for a test, as the test of the
//! spike of #697.
//!
//! A daemon keeps one peer link to each other daemon of its riff that
//! is online (01M519Z2SV9XSQA4N59N10JAQC): the owner, and each member
//! that is not removed and did not leave. Of two daemons, the one with
//! the lower key connects ([`Links::plan`]). When a link ends, its dial
//! loop connects again. Each try has a limit of [`CONNECT_LIMIT`]. The
//! wait between two tries starts at [`FIRST_WAIT`] and doubles up to
//! [`LAST_WAIT`]. A link that ends with a fault, for example a refused
//! record, does not start the wait again from [`FIRST_WAIT`].
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as daemon A (the lower key)
//!     participant B as daemon B
//!     A->>B: connect: iroh checks the key of each side
//!     Note over A,B: each side: is the other key the owner or a member?
//!     A->>B: heads
//!     B->>A: heads
//!     A->>B: the records that B lacks
//!     B->>A: the records that A lacks
//!     Note over A,B: each new record at once, but to the daemon that sent it
//!     B->>A: heads, again each minute
//! ```
//!
//! # A link
//!
//! At each connect, each side checks that the key of the other side is
//! the owner or a member of its riff that is not removed and did not
//! leave (01M519Z2WAGQEX1H4EGZXWBZT6). Else it closes the connection with
//! [`Close::NotMember`]. The side that connected opens one stream. Each
//! side sends [`crate::peer::wire`] frames on it:
//!
//! - Its heads ([`Frame::Heads`]): at the connect, at once when it drops
//!   a record that it cannot check yet (at most one time in
//!   [`LATER_HEADS`]), and each [`HEADS_AGAIN`].
//! - At heads of the other side, each record that the other side lacks
//!   ([`crate::peer::keep::Peers::lacks`]).
//! - Each new record that the daemon keeps, at once: its own, and each
//!   record of another daemon, but not to the daemon that sent it
//!   (01M51BFYQVCFEWCJZERBD4S5N5). And the proof of each new fork.
//!
//! A writer task sends the frames, and a reader task takes the frames of
//! the other side. So two daemons that send many records at one time do
//! not wait for each other. The reader keeps each record on a thread for
//! blocking work ([`Hub::keep`]: a sync of a file). It closes the link
//! at a refused record ([`Close::Refused`], 01M519Z363W5GV4MV5RDB6ECG0)
//! and at a bad frame ([`Close::BadFrame`]), with a log line that names
//! the author, the sequence number and the check, with no body. A link
//! whose writer falls more than [`crate::peer::keep::FRESH`] records
//! behind ends ([`Close::Behind`]): its next connect sends what the
//! other side lacks.
//!
//! ```mermaid
//! flowchart LR
//!     F["a frame of the other side"] --> H{"kind"}
//!     H -->|heads| W["the writer: each record that the other side lacks"]
//!     H -->|record| K["Hub::keep"]
//!     K -->|"kept"| N["each other link, and the sessions of the daemon"]
//!     K -->|"a duplicate, of a forked author"| D["drop, no fault"]
//!     K -->|"later"| L["send the heads again"]
//!     K -->|"a fork"| P["the proof to each link; a note to the lead"]
//!     K -->|"refused"| C["log the author, the number, the check; close"]
//! ```
//!
//! The members come from the records of the owner. So the plan of the
//! links changes with each `riff`, `member`, `removed` and `left` record,
//! and at least each [`PLAN_AGAIN`]. A link of a key that may link no
//! more closes ([`Close::Gone`]). [`Links::shown`] gives the state of
//! each link for `riff daemon status`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context as _, Result};
use iroh::address_lookup::memory::MemoryLookup;
use iroh::endpoint::{Connection, ConnectionError, RecvStream, SendStream, presets};
use iroh::{Endpoint, SecretKey};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{Notify, broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use zeroize::Zeroize;

use crate::daemon_key::{Key, Public};
use crate::peer::body::Type;
use crate::peer::keep::{Fresh, Heads, Outcome, Peers};
use crate::peer::record::{Fault, author_and_seq};
use crate::peer::state::State;
use crate::peer::talk::Hub;
use crate::peer::wire::{self, Frame};
use crate::text;

/// The ALPN of a peer link: `riff/` and the major version of riff. A
/// daemon of another major version does not connect
/// (01M519Z2QE6QYPJX8HVY17GR6Z).
pub const ALPN: &[u8] = b"riff/3";

/// The limit of one try to connect.
pub const CONNECT_LIMIT: Duration = Duration::from_secs(5);

/// The first wait between two tries to connect.
pub const FIRST_WAIT: Duration = Duration::from_secs(1);

/// The longest wait between two tries to connect.
pub const LAST_WAIT: Duration = Duration::from_secs(60);

/// A link sends its heads again after this time.
pub const HEADS_AGAIN: Duration = Duration::from_secs(60);

/// A link sends its heads for a record that it cannot check yet at most
/// one time in this time.
pub const LATER_HEADS: Duration = Duration::from_secs(1);

/// The plan of the links looks at the members again after this time.
pub const PLAN_AGAIN: Duration = Duration::from_secs(30);

/// The variable that picks the net of the daemon: `local` gives
/// [`Net::Local`], with no address. The tests of riff set it, so that no
/// test reaches n0.
pub const NET_VAR: &str = "RIFF_PEER_NET";

/// How a daemon finds the other daemons.
#[derive(Clone, Debug)]
pub enum Net {
    /// The defaults of n0: its DNS and its public relays
    /// (01M4Z60B8HN1C1XAG1CA1YS36H).
    N0,
    /// No relay and no DNS: the endpoint binds on 127.0.0.1, and finds a
    /// daemon only by the addresses of this lookup.
    Local(MemoryLookup),
}

impl Net {
    /// The net of [`NET_VAR`]: [`Net::Local`] with an empty lookup for
    /// `local`, else [`Net::N0`].
    pub fn from_env() -> Net {
        match std::env::var(NET_VAR).as_deref() {
            Ok("local") => Net::Local(MemoryLookup::new()),
            _ => Net::N0,
        }
    }
}

/// Why a daemon closes a peer link: the application code of the close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Close {
    /// The key of the other side is not the owner or a member.
    NotMember = 1,
    /// The other side sent a record that fails a check.
    Refused = 2,
    /// The other side sent a frame that the daemon cannot read.
    BadFrame = 3,
    /// A new connection of the same key takes the place of the link.
    Replaced = 4,
    /// The key of the other side was removed, or it left.
    Gone = 5,
    /// The writer of the link fell too far behind the new records.
    Behind = 6,
    /// The daemon cannot keep a record.
    Disk = 7,
}

impl Close {
    /// The close of the code `code`, when riff knows it.
    fn of(code: u64) -> Option<Close> {
        [
            Close::NotMember,
            Close::Refused,
            Close::BadFrame,
            Close::Replaced,
            Close::Gone,
            Close::Behind,
            Close::Disk,
        ]
        .into_iter()
        .find(|close| *close as u64 == code)
    }

    /// True for a close for a fault of a side: the wait to connect again
    /// does not start again.
    fn fault(self) -> bool {
        matches!(
            self,
            Close::NotMember | Close::Refused | Close::BadFrame | Close::Disk
        )
    }

    /// Closes `conn` with this code.
    fn close(self, conn: &Connection) {
        conn.close((self as u32).into(), self.to_string().as_bytes());
    }
}

impl fmt::Display for Close {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Close::NotMember => "not a member",
            Close::Refused => "a refused record",
            Close::BadFrame => "a bad frame",
            Close::Replaced => "a new connection",
            Close::Gone => "a key that was removed or left",
            Close::Behind => "too far behind the new records",
            Close::Disk => "a record that the daemon cannot keep",
        })
    }
}

/// How a link ended.
#[derive(Clone, Debug)]
enum End {
    /// This daemon closed it.
    Closed(Close, String),
    /// The other daemon closed it, or the connection was lost.
    Lost(Option<Close>, String),
}

impl End {
    /// True when a fault of a side ended the link.
    fn fault(&self) -> bool {
        match self {
            End::Closed(close, _) | End::Lost(Some(close), _) => close.fault(),
            End::Lost(None, _) => false,
        }
    }

    /// How the connection `conn` ended, or `why` when it did not end.
    fn lost(conn: &Connection, why: String) -> End {
        match conn.close_reason() {
            Some(ConnectionError::ApplicationClosed(close)) => {
                let code = close.error_code.into_inner();
                let shown = String::from_utf8_lossy(&close.reason).into_owned();
                End::Lost(
                    Close::of(code),
                    text::peer_closed_by_the_other(code, &shown),
                )
            }
            Some(other) => End::Lost(None, other.to_string()),
            None => End::Lost(None, why),
        }
    }
}

impl fmt::Display for End {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            End::Closed(_, why) | End::Lost(_, why) => f.write_str(why),
        }
    }
}

/// What the reader asks the writer to send.
enum Out {
    /// The heads of the daemon.
    Heads,
    /// Each record that a daemon with these heads lacks.
    Lacks(Heads),
}

/// A link that runs.
struct Link {
    conn: Connection,
    since_ms: i64,
}

/// The tries of a dial loop since its last connect.
#[derive(Clone, Debug, Default)]
struct Tries {
    count: u32,
    next_ms: i64,
    why: String,
}

/// The state of the link to one daemon, for `riff daemon status`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shown {
    /// The key of the other daemon.
    pub key: String,
    /// Its user, when the records name it.
    pub user: Option<String>,
    /// Its host, when the records name it.
    pub host: Option<String>,
    /// The state of the link.
    pub link: LinkState,
}

/// The state of one link.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LinkState {
    /// The link runs, from this time, on a direct path or on a relay.
    Up { since_ms: i64, direct: bool },
    /// This daemon connects: the tries that failed, the time of the next
    /// try, and why the last try failed. No try yet: `tries` is 0.
    Connects {
        tries: u32,
        next_ms: i64,
        why: String,
    },
    /// The other daemon has the lower key: this daemon waits for it to
    /// connect.
    Waits,
}

/// The peer links of a daemon. See the design of the module.
pub struct Links {
    endpoint: Endpoint,
    hub: Arc<Hub>,
    links: Mutex<BTreeMap<Public, Link>>,
    dials: Mutex<BTreeMap<Public, JoinHandle<()>>>,
    tries: Mutex<BTreeMap<Public, Tries>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    /// A link ended.
    ended: Notify,
    say: Box<dyn Fn(String) + Send + Sync>,
}

impl fmt::Debug for Links {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Links")
            .field("endpoint", &self.endpoint.id())
            .finish_non_exhaustive()
    }
}

/// Locks `mutex`, also when a thread that held it panicked.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The time now, in milliseconds since the epoch.
fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// The key of the other side of `conn`.
fn public_of(conn: &Connection) -> Public {
    Public::from_bytes(*conn.remote_id().as_bytes())
}

/// The keys that the daemon `me` links to: the owner, and each member
/// that is not removed and did not leave, but `me`.
fn keys_of(state: &State, me: &Public) -> BTreeSet<Public> {
    let mut keys: BTreeSet<Public> = state
        .members()
        .iter()
        .filter(|(_, member)| !member.removed && !member.left)
        .map(|(key, _)| *key)
        .collect();
    keys.insert(state.riff());
    keys.remove(me);
    keys
}

/// The user and the host of `key`, when the records name them.
fn names_of(state: &State, key: &Public) -> (Option<String>, Option<String>) {
    (
        state.user_of(key).map(str::to_owned),
        state.members().get(key).map(|member| member.host.clone()),
    )
}

impl Links {
    /// Binds the endpoint of the daemon of `key` on the net `net`.
    pub async fn bind(key: &Key, net: &Net) -> Result<Endpoint> {
        let mut bytes = key.to_bytes();
        let secret = SecretKey::from_bytes(&bytes);
        bytes.zeroize();
        let builder = match net {
            Net::N0 => Endpoint::builder(presets::N0),
            Net::Local(lookup) => Endpoint::builder(presets::Minimal)
                .clear_ip_transports()
                .bind_addr("127.0.0.1:0")?
                .address_lookup(lookup.clone()),
        };
        builder
            .secret_key(secret)
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await
            .context(text::PEER_NO_ENDPOINT)
    }

    /// Runs the peer links of the daemon of `hub` on `endpoint`, with the
    /// log lines `say`.
    pub fn start(
        endpoint: Endpoint,
        hub: Arc<Hub>,
        say: impl Fn(String) + Send + Sync + 'static,
    ) -> Arc<Links> {
        let sync = Arc::new(Links {
            endpoint,
            hub,
            links: Mutex::new(BTreeMap::new()),
            dials: Mutex::new(BTreeMap::new()),
            tries: Mutex::new(BTreeMap::new()),
            tasks: Mutex::new(Vec::new()),
            ended: Notify::new(),
            say: Box::new(say),
        });
        let accept = tokio::spawn(sync.clone().accept_all());
        let plan = tokio::spawn(sync.clone().plan_all());
        lock(&sync.tasks).extend([accept, plan]);
        sync
    }

    /// The endpoint of the daemon.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// The key of each daemon that has a link now.
    pub fn linked(&self) -> BTreeSet<Public> {
        lock(&self.links).keys().copied().collect()
    }

    /// The state of the link to each other daemon of the riff.
    pub fn shown(&self) -> Vec<Shown> {
        let me = self.hub.public();
        let keys: Vec<(Public, (Option<String>, Option<String>))> = self.hub.read(|state| {
            keys_of(state, &me)
                .into_iter()
                .map(|key| (key, names_of(state, &key)))
                .collect()
        });
        let links = lock(&self.links);
        let tries = lock(&self.tries);
        keys.into_iter()
            .map(|(key, (user, host))| {
                let link = match links.get(&key) {
                    Some(link) => LinkState::Up {
                        since_ms: link.since_ms,
                        direct: link
                            .conn
                            .paths()
                            .iter()
                            .find(|path| path.is_selected())
                            .is_some_and(|path| path.is_ip()),
                    },
                    None if me < key => {
                        let tries = tries.get(&key).cloned().unwrap_or_default();
                        LinkState::Connects {
                            tries: tries.count,
                            next_ms: tries.next_ms,
                            why: tries.why,
                        }
                    }
                    None => LinkState::Waits,
                };
                Shown {
                    key: key.to_string(),
                    user,
                    host,
                    link,
                }
            })
            .collect()
    }

    /// Stops each link, and closes the endpoint.
    pub async fn close(&self) {
        for task in lock(&self.tasks).drain(..) {
            task.abort();
        }
        for (_, dial) in std::mem::take(&mut *lock(&self.dials)) {
            dial.abort();
        }
        lock(&self.links).clear();
        self.endpoint.close().await;
    }

    /// Takes each connection of another daemon.
    async fn accept_all(self: Arc<Self>) {
        while let Some(incoming) = self.endpoint.accept().await {
            let sync = self.clone();
            tokio::spawn(async move {
                if let Ok(Ok(conn)) = tokio::time::timeout(CONNECT_LIMIT, incoming).await {
                    sync.serve(conn, false).await;
                }
            });
        }
    }

    /// Plans the links at each change of the members, and at least each
    /// [`PLAN_AGAIN`].
    async fn plan_all(self: Arc<Self>) {
        let mut fresh = self.hub.with_peers(Peers::subscribe);
        loop {
            self.plan();
            let again = tokio::time::sleep(PLAN_AGAIN);
            tokio::pin!(again);
            loop {
                tokio::select! {
                    got = fresh.recv() => match got {
                        Ok(Fresh::Kept { record, .. }) if matches!(
                            record.of(),
                            Type::Riff | Type::Member | Type::Removed | Type::Left
                        ) => break,
                        Ok(_) => {}
                        Err(RecvError::Lagged(_)) => break,
                        Err(RecvError::Closed) => return,
                    },
                    () = &mut again => break,
                }
            }
        }
    }

    /// Starts a dial loop for each key that this daemon connects to: each
    /// key of the riff that is higher than its own key. It stops the dial
    /// loop of each other key, and closes the link of each key that may
    /// link no more.
    pub fn plan(self: &Arc<Self>) {
        let me = self.hub.public();
        let keys = self.hub.read(|state| keys_of(state, &me));
        let dials_to = |key: &Public| keys.contains(key) && me < *key;
        let mut dials = lock(&self.dials);
        dials.retain(|key, dial| {
            let keep = dials_to(key);
            if !keep {
                dial.abort();
            }
            keep
        });
        for key in keys.iter().filter(|key| dials_to(key)) {
            dials
                .entry(*key)
                .or_insert_with(|| tokio::spawn(self.clone().dial(*key)));
        }
        drop(dials);
        lock(&self.tries).retain(|key, _| dials_to(key));
        lock(&self.links).retain(|key, link| {
            let keep = keys.contains(key);
            if !keep {
                Close::Gone.close(&link.conn);
            }
            keep
        });
    }

    /// Connects to the daemon of `key`, and again after each end.
    async fn dial(self: Arc<Self>, key: Public) {
        let Ok(id) = iroh::PublicKey::from_bytes(key.as_bytes()) else {
            self.tried(key, text::PEER_NO_ENDPOINT_ID.into(), LAST_WAIT);
            return;
        };
        let mut wait = FIRST_WAIT;
        loop {
            self.no_link(key).await;
            let why =
                match tokio::time::timeout(CONNECT_LIMIT, self.endpoint.connect(id, ALPN)).await {
                    Ok(Ok(conn)) => {
                        lock(&self.tries).remove(&key);
                        let end = self.serve(conn, true).await;
                        if !end.fault() {
                            wait = FIRST_WAIT;
                        }
                        end.to_string()
                    }
                    Ok(Err(e)) => e.to_string(),
                    Err(_) => text::peer_no_connect_in(CONNECT_LIMIT),
                };
            self.tried(key, why, wait);
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(LAST_WAIT);
        }
    }

    /// Waits until the daemon has no link to `key`: a link that the other
    /// daemon started holds.
    async fn no_link(&self, key: Public) {
        loop {
            let ended = self.ended.notified();
            tokio::pin!(ended);
            ended.as_mut().enable();
            if !lock(&self.links).contains_key(&key) {
                return;
            }
            ended.await;
        }
    }

    /// Keeps a try to connect to `key` that failed with `why`, and the
    /// next try after `wait`.
    fn tried(&self, key: Public, why: String, wait: Duration) {
        let mut tries = lock(&self.tries);
        let tries = tries.entry(key).or_default();
        tries.count += 1;
        tries.next_ms = now_ms() + i64::try_from(wait.as_millis()).unwrap_or(i64::MAX);
        tries.why = why;
    }

    /// The user and the host of `key`, for a log line.
    fn who(&self, key: &Public) -> String {
        let (user, host) = self.hub.read(|state| names_of(state, key));
        text::peer_who(&key.to_string(), user.as_deref(), host.as_deref())
    }

    /// Runs the link of the connection `conn` until it ends. `dialed` is
    /// true when this daemon connected.
    async fn serve(self: &Arc<Self>, conn: Connection, dialed: bool) -> End {
        let remote = public_of(&conn);
        let me = self.hub.public();
        if remote == me || !self.hub.read(|state| keys_of(state, &me).contains(&remote)) {
            (self.say)(text::peer_not_member(&remote.to_string()));
            Close::NotMember.close(&conn);
            return End::Closed(Close::NotMember, Close::NotMember.to_string());
        }
        let link = Link {
            conn: conn.clone(),
            since_ms: now_ms(),
        };
        if let Some(old) = lock(&self.links).insert(remote, link) {
            Close::Replaced.close(&old.conn);
        }
        let who = self.who(&remote);
        (self.say)(text::peer_link_up(&who));
        let end = self.run(&conn, remote, dialed).await;
        if let End::Closed(close, _) = &end {
            close.close(&conn);
        }
        {
            let mut links = lock(&self.links);
            if links
                .get(&remote)
                .is_some_and(|link| link.conn.stable_id() == conn.stable_id())
            {
                links.remove(&remote);
            }
        }
        (self.say)(text::peer_link_down(&who, &end.to_string()));
        self.ended.notify_waiters();
        end
    }

    /// The frames of the link `conn` to `remote`, both ways.
    async fn run(&self, conn: &Connection, remote: Public, dialed: bool) -> End {
        // The fresh records from before the heads: so the other side
        // gets each record, also one that the daemon keeps while it sends
        // what the other side lacks.
        let fresh = self.hub.with_peers(Peers::subscribe);
        let streams = if dialed {
            conn.open_bi().await.map_err(|e| e.to_string())
        } else {
            match tokio::time::timeout(CONNECT_LIMIT, conn.accept_bi()).await {
                Ok(streams) => streams.map_err(|e| e.to_string()),
                Err(_) => Err(text::peer_no_stream_in(CONNECT_LIMIT)),
            }
        };
        let (send, recv) = match streams {
            Ok(streams) => streams,
            Err(why) => return End::lost(conn, why),
        };
        let (out, asks) = mpsc::unbounded_channel();
        let _ = out.send(Out::Heads);
        let mut writer = tokio::spawn(write_all(
            send,
            asks,
            fresh,
            self.hub.clone(),
            remote,
            conn.clone(),
        ));
        let end = tokio::select! {
            end = self.read_all(recv, out, remote, conn) => end,
            end = &mut writer => end.unwrap_or_else(|e| End::lost(conn, e.to_string())),
        };
        writer.abort();
        end
    }

    /// Takes each frame of `remote` on `recv`, until the link ends.
    async fn read_all(
        &self,
        mut recv: RecvStream,
        out: mpsc::UnboundedSender<Out>,
        remote: Public,
        conn: &Connection,
    ) -> End {
        let mut asked: Option<Instant> = None;
        loop {
            let frame = match wire::read(&mut recv).await {
                Ok(Some(frame)) => frame,
                Ok(None) => return End::lost(conn, text::PEER_STREAM_ENDED.into()),
                Err(wire::Bad::Read(e)) => return End::lost(conn, e.to_string()),
                Err(bad) => {
                    let line = text::peer_bad_frame(&self.who(&remote), &bad.to_string());
                    (self.say)(line.clone());
                    return End::Closed(Close::BadFrame, line);
                }
            };
            let bytes = match frame {
                Frame::Heads(theirs) => {
                    let _ = out.send(Out::Lacks(theirs));
                    continue;
                }
                Frame::Record(bytes) => bytes,
            };
            let named = author_and_seq(&bytes);
            let hub = self.hub.clone();
            let kept = tokio::task::spawn_blocking(move || hub.keep(bytes, Some(remote))).await;
            match kept.map_err(anyhow::Error::from).and_then(|kept| kept) {
                Ok(Outcome::Later(_)) => {
                    if asked.is_none_or(|at| at.elapsed() >= LATER_HEADS) {
                        let _ = out.send(Out::Heads);
                        asked = Some(Instant::now());
                    }
                }
                Ok(Outcome::Refused(fault)) => return self.refused(remote, named, &fault),
                Ok(Outcome::Fork(fork)) => {
                    let author = fork.first.author();
                    let (user, host) = self.hub.read(|state| names_of(state, &author));
                    (self.say)(text::peer_fork(
                        &author.to_string(),
                        user.as_deref(),
                        host.as_deref(),
                        fork.first.seq(),
                    ));
                }
                Ok(Outcome::Kept(_) | Outcome::Duplicate | Outcome::Forked) => {}
                Err(e) => {
                    let line = text::peer_cannot_keep(&format!("{e:#}"));
                    (self.say)(line.clone());
                    return End::Closed(Close::Disk, line);
                }
            }
        }
    }

    /// Logs the refused record of `remote`: its author, its sequence
    /// number and the check `fault`, with no body
    /// (01M519Z363W5GV4MV5RDB6ECG0).
    fn refused(&self, remote: Public, named: Option<(Public, u64)>, fault: &Fault) -> End {
        let line = text::peer_refused(
            &self.who(&remote),
            named.map(|(author, seq)| (author.to_string(), seq)),
            &fault.to_string(),
        );
        (self.say)(line.clone());
        End::Closed(Close::Refused, line)
    }
}

/// Sends the frames of the link to `remote` on `send`: what the reader
/// asks, each fresh record, and the heads each [`HEADS_AGAIN`].
async fn write_all(
    mut send: SendStream,
    mut asks: mpsc::UnboundedReceiver<Out>,
    mut fresh: broadcast::Receiver<Fresh>,
    hub: Arc<Hub>,
    remote: Public,
    conn: Connection,
) -> End {
    let mut again = tokio::time::interval_at(Instant::now() + HEADS_AGAIN, HEADS_AGAIN);
    loop {
        let frames: Vec<Frame> = tokio::select! {
            biased;
            ask = asks.recv() => match ask {
                Some(Out::Heads) => vec![Frame::Heads(hub.with_peers(Peers::heads))],
                Some(Out::Lacks(theirs)) => hub
                    .with_peers(|peers| peers.lacks(&theirs))
                    .into_iter()
                    .map(|record| Frame::Record(record.bytes().to_vec()))
                    .collect(),
                None => return End::lost(&conn, text::PEER_STREAM_ENDED.into()),
            },
            got = fresh.recv() => match got {
                Ok(Fresh::Kept { record, from }) if from != Some(remote) => {
                    vec![Frame::Record(record.bytes().to_vec())]
                }
                Ok(Fresh::Kept { .. }) => continue,
                Ok(Fresh::Fork(fork)) => vec![
                    Frame::Record(fork.first.bytes().to_vec()),
                    Frame::Record(fork.second.bytes().to_vec()),
                ],
                Err(RecvError::Lagged(_)) => {
                    return End::Closed(Close::Behind, Close::Behind.to_string());
                }
                Err(RecvError::Closed) => return End::lost(&conn, text::PEER_STREAM_ENDED.into()),
            },
            _ = again.tick() => vec![Frame::Heads(hub.with_peers(Peers::heads))],
        };
        for frame in &frames {
            if let Err(e) = wire::write(&mut send, frame).await {
                return End::lost(&conn, e.to_string());
            }
        }
    }
}
