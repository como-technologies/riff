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
//! record, does not start the wait again from [`FIRST_WAIT`]
//! (01M51ZA08MGF20MPHATHE81B44). A daemon also takes a connection of a
//! higher key, for example at a join: its own dial loop to that key waits
//! while that link runs, and a new connection of a key takes the place of
//! its old link (01M51ZA05RTRE9TSMBP20AS20H).
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
//!   [`LATER_HEADS`]), and each [`HEADS_AGAIN`]
//!   (01M51Z9ZX4P0HEH53YFJ6XAWST).
//! - At heads of the other side, each record that the other side lacks
//!   ([`crate::peer::keep::Peers::lacks`]).
//! - Each new record that the daemon keeps, at once: its own, and each
//!   record of another daemon, but not to the daemon that sent it
//!   (01M51BFYQVCFEWCJZERBD4S5N5). And the proof of each new fork
//!   (01M51ZA001C8MZM4BTBYTXRJ8M).
//!
//! A writer task sends the frames, and a reader task takes the frames of
//! the other side. So two daemons that send many records at one time do
//! not wait for each other. The reader keeps each record on a thread for
//! blocking work ([`Hub::keep`]: a sync of a file). It closes the link
//! at a refused record ([`Close::Refused`], 01M519Z363W5GV4MV5RDB6ECG0)
//! and at a bad frame ([`Close::BadFrame`]), with a log line that names
//! the author, the sequence number and the check, with no body. A link
//! whose writer falls more than [`crate::peer::keep::FRESH`] records
//! behind ends ([`Close::Behind`], 01M51ZA08MGF20MPHATHE81B44): its next
//! connect sends what the other side lacks.
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
//! at a join and at a leave ([`Fresh::Switched`]), and at least each
//! [`PLAN_AGAIN`]. A link of a key that may link no more closes
//! ([`Close::Gone`]). [`Links::shown`] gives the state of each link for
//! `riff daemon status` (01M51ZA0BDZNXBB7R5SB8CEGA9).
//!
//! A daemon of the riff is online while a link to it runs
//! ([`Links::online`], 01M522MRYR89SY6WRV720806RA). Else it was last seen
//! at the later of the end of its last link ([`Links::seen`]) and the
//! time of its last record ([`State::last_ms`]). `riff view` and
//! `riff top` show the sessions of a daemon that is offline with that
//! time (01M519Z554SC3EAT69Q3M6JDAP), and `riff members` shows each key
//! with it.
//!
//! # A join and a leave
//!
//! The endpoint also takes the ALPN of a join ([`crate::peer::join`]). A
//! daemon that takes joins ([`Links::take_joins`]) answers a join, and on
//! a join the connection goes on as the peer link. The new daemon runs
//! its side of that link with [`Links::serve_joined`].
//!
//! At `riff leave`, the daemon writes its `left` record. Each link sends
//! it, ends its stream, and waits up to [`LEAVE_LIMIT`] for the other
//! daemon to close the connection (01M521J6ZRNWBSPK7WB23TVY59). A link
//! that starts during the leave sends the `left` record with the records
//! that the other daemon lacks, and then ends its stream in the same way.
//! The other daemon reads each frame in order. At the end of the stream
//! of a key that left, or when it keeps the `left` record, it closes the
//! link with [`Close::Gone`].
//!
//! A link sent the `left` record only when the other daemon closed it
//! with [`Close::Gone`]. A new connection can take the place of a link
//! during the leave ([`Close::Replaced`]): the other daemon then drops
//! the frames of the old link that it did not read, and the new link is
//! not in the links until its first step. So [`Links::wait_left`] waits
//! until a link sent the record and no link runs, or up to
//! [`LEAVE_LIMIT`]. A moment with no link, or a link that a replace
//! closed, does not count (01M52JF709YHYE7SG6V3SDHR74).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant M as the daemon that leaves
//!     participant O as the daemon of the owner
//!     Note over M,O: a new connection takes the place of the old link
//!     O->>M: close the old link (code 4): it does not count
//!     M->>M: write the left record; no link runs
//!     M->>O: the new link: heads, then the records that O lacks, with left
//!     M->>O: the end of the stream
//!     O->>M: close (code 5, Gone): the new link sent it
//!     M->>M: no link runs: the riff of its own
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
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
use crate::peer::invite::Invites;
use crate::peer::join;
use crate::peer::keep::{Fresh, Heads, Outcome, Peers};
use crate::peer::record::{Fault, Record, author_and_seq};
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

/// A link waits this long for the other daemon to read the `left`
/// record of this daemon (01M521J6ZRNWBSPK7WB23TVY59).
pub const LEAVE_LIMIT: Duration = Duration::from_secs(5);

/// The variable that picks the net of the daemon: `local` gives
/// [`Net::Local`], with no address. The tests of riff set it, so that no
/// test reaches n0 (01M51ZA0E79V2FPBMQ6F1MYZJ5).
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
            Close::Refused => "a record that fails a check",
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

/// True when the other daemon closed the connection with [`Close::Gone`]:
/// it kept the `left` record of this daemon (01M52JF709YHYE7SG6V3SDHR74).
/// Each other end gives false: a close of this daemon, a replace, a
/// fault, a drop or a lost connection. Then the other daemon can have
/// dropped frames that it did not read.
fn kept_left(reason: &ConnectionError) -> bool {
    matches!(
        reason,
        ConnectionError::ApplicationClosed(close)
            if Close::of(close.error_code.into_inner()) == Some(Close::Gone)
    )
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
///
/// The owner and one member on this machine: the member gets each
/// record of the owner.
///
/// ```
/// use std::sync::Arc;
/// use std::time::Duration;
///
/// use iroh::address_lookup::memory::MemoryLookup;
/// use riff::daemon_key::{Key, Public};
/// use riff::peer::body::{Body, Member, Riff};
/// use riff::peer::keep::Peers;
/// use riff::peer::sync::{Links, Net};
/// use riff::peer::talk::Hub;
///
/// /// The daemon of the key `byte` in the riff `riff`, on 127.0.0.1.
/// async fn daemon(byte: u8, riff: Public, lookup: &MemoryLookup)
///     -> anyhow::Result<(Arc<Hub>, Arc<Links>, tempfile::TempDir)> {
///     let dir = tempfile::tempdir()?;
///     let key = Key::from_bytes(&[byte; 32]);
///     let peers = Peers::open(dir.path(), riff, |_| {})?;
///     let endpoint = Links::bind(&key, &Net::Local(lookup.clone())).await?;
///     let hub = Arc::new(Hub::new(key, peers));
///     Ok((hub.clone(), Links::start(endpoint, hub, |_| {}), dir))
/// }
///
/// tokio::runtime::Runtime::new()?.block_on(async {
///     let riff = Key::from_bytes(&[1; 32]).public();
///     let (seen_by_owner, seen_by_member) = (MemoryLookup::new(), MemoryLookup::new());
///     let (owner, owner_links, _a) = daemon(1, riff, &seen_by_owner).await?;
///     owner.write(&Body::Riff(Riff { user: "mike".into(), host: None }))?;
///     let key = Key::from_bytes(&[2; 32]).public();
///     let host = "kadomony".into();
///     owner.write(&Body::Member(Member { key, user: "brett".into(), host }))?;
///
///     let (member, member_links, _b) = daemon(2, riff, &seen_by_member).await?;
///     seen_by_member.set_endpoint_info(owner_links.endpoint().addr());
///     seen_by_owner.set_endpoint_info(member_links.endpoint().addr());
///     tokio::time::timeout(Duration::from_secs(30), async {
///         while member.read(|state| state.user_of(&key).is_none()) {
///             tokio::time::sleep(Duration::from_millis(50)).await;
///         }
///     })
///     .await?;
///     assert_eq!(member.read(|state| state.owner().map(str::to_owned)), Some("mike".into()));
///     assert_eq!(member_links.linked().into_iter().collect::<Vec<_>>(), [riff]);
///     Ok::<(), anyhow::Error>(())
/// })?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub struct Links {
    endpoint: Endpoint,
    hub: Arc<Hub>,
    links: Mutex<BTreeMap<Public, Link>>,
    /// The end of the last link of each key, since the start of the
    /// daemon: [`Links::seen`].
    seen: Mutex<BTreeMap<Public, i64>>,
    dials: Mutex<BTreeMap<Public, JoinHandle<()>>>,
    tries: Mutex<BTreeMap<Public, Tries>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    /// A link ended.
    ended: Notify,
    /// The records that a link got and the daemon had already.
    duplicates: AtomicU64,
    /// The links that sent the `left` record of this daemon: [`Links::left_sent`].
    left_sent: AtomicU64,
    /// The invites of the daemon of the owner: it answers a join.
    joins: OnceLock<Arc<Invites>>,
    say: Box<dyn Fn(String) + Send + Sync>,
    /// A test holds each new link here: [`Links::hold_new_links`].
    #[cfg(test)]
    new_links: Arc<tokio::sync::Mutex<()>>,
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

/// True when the member `key` left or was removed.
fn out_of(state: &State, key: &Public) -> bool {
    state
        .members()
        .get(key)
        .is_some_and(|member| member.left || member.removed)
}

/// The user and the host of a key, when the records name them.
type Names = (Option<String>, Option<String>);

/// The user and the host of `key`, when the records name them: for the
/// owner, its `riff` record (01M521J6T2PWA9JREDXJK6DZ0V).
fn names_of(state: &State, key: &Public) -> Names {
    (
        state.user_of(key).map(str::to_owned),
        state.host_of(key).map(str::to_owned),
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
            .alpns(vec![ALPN.to_vec(), join::ALPN.to_vec()])
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
            seen: Mutex::new(BTreeMap::new()),
            dials: Mutex::new(BTreeMap::new()),
            tries: Mutex::new(BTreeMap::new()),
            tasks: Mutex::new(Vec::new()),
            ended: Notify::new(),
            duplicates: AtomicU64::new(0),
            left_sent: AtomicU64::new(0),
            joins: OnceLock::new(),
            say: Box::new(say),
            #[cfg(test)]
            new_links: Arc::default(),
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

    /// The records that a link got and the daemon had already: each
    /// drops with no fault (01M51BFYQVCFEWCJZERBD4S5N5).
    pub fn duplicates(&self) -> u64 {
        self.duplicates.load(Ordering::Relaxed)
    }

    /// The key of each daemon that has a link now.
    pub fn linked(&self) -> BTreeSet<Public> {
        lock(&self.links).keys().copied().collect()
    }

    /// True while a link to the daemon of `key` runs: the daemon is
    /// online (01M519Z554SC3EAT69Q3M6JDAP).
    pub fn online(&self, key: &Public) -> bool {
        lock(&self.links).contains_key(key)
    }

    /// The end of the last link to the daemon of `key`, in milliseconds
    /// since the epoch: the time when this daemon last saw it. `None`
    /// when no link to it ended since the start of this daemon. While a
    /// link runs, it gives the end of the link before it. A daemon that
    /// started again knows the time of the last record of the key too
    /// ([`State::last_ms`]): the later of the two counts.
    pub fn seen(&self, key: &Public) -> Option<i64> {
        lock(&self.seen).get(key).copied()
    }

    /// The daemon answers each join with the invites `invites`: the
    /// daemon of the owner ([`crate::peer::join`]). Without it, the
    /// daemon closes each connection of a join.
    pub fn take_joins(&self, invites: Arc<Invites>) {
        let _ = self.joins.set(invites);
    }

    /// Runs the peer link on the connection `conn` of a join that this
    /// daemon asked for ([`crate::peer::join::ask`]), after the daemon
    /// took the records of the new riff.
    pub fn serve_joined(self: &Arc<Self>, conn: Connection) {
        let sync = self.clone();
        let task = tokio::spawn(async move {
            sync.serve(conn, true).await;
        });
        lock(&self.tasks).push(task);
    }

    /// The count of the links that sent the `left` record of this daemon
    /// since its start: each link that ended its stream after that record,
    /// and that the other daemon then closed with [`Close::Gone`]. Read
    /// it before the daemon writes its `left` record, and give it to
    /// [`Links::wait_left`].
    pub fn left_sent(&self) -> u64 {
        self.left_sent.load(Ordering::SeqCst)
    }

    /// Waits at a leave, up to `limit`, until the count of
    /// [`Links::left_sent`] is more than `before`, and no link runs
    /// (01M52JF709YHYE7SG6V3SDHR74). So a moment with no link, for example
    /// between the end of a replaced link and the first step of the new
    /// one, does not end the wait. It gives true when a link sent the
    /// `left` record.
    pub async fn wait_left(&self, before: u64, limit: Duration) -> bool {
        let _ = tokio::time::timeout(limit, async {
            loop {
                let ended = self.ended.notified();
                tokio::pin!(ended);
                ended.as_mut().enable();
                if self.left_sent() > before && lock(&self.links).is_empty() {
                    return;
                }
                ended.await;
            }
        })
        .await;
        self.left_sent() > before
    }

    /// A test: each new link of this daemon waits before its first step
    /// while the test holds the guard.
    #[cfg(test)]
    pub(crate) async fn hold_new_links(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.new_links.clone().lock_owned().await
    }

    /// A test: this daemon makes one more connection to the daemon of
    /// `key`. It takes the place of the link to that key on both sides.
    #[cfg(test)]
    pub(crate) fn connect_again(self: &Arc<Self>, key: Public) {
        let sync = self.clone();
        let task = tokio::spawn(async move {
            let id = iroh::PublicKey::from_bytes(key.as_bytes()).expect("an endpoint ID");
            let conn = sync.endpoint.connect(id, ALPN).await.expect("a connect");
            sync.serve(conn, true).await;
        });
        lock(&self.tasks).push(task);
    }

    /// The state of the link to each other daemon of the riff.
    pub fn shown(&self) -> Vec<Shown> {
        let me = self.hub.public();
        let keys: Vec<(Public, Names)> = self.hub.read(|state| {
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

    /// Takes each connection of another daemon: a peer link, or a join.
    async fn accept_all(self: Arc<Self>) {
        while let Some(incoming) = self.endpoint.accept().await {
            let sync = self.clone();
            tokio::spawn(async move {
                let Ok(Ok(conn)) = tokio::time::timeout(CONNECT_LIMIT, incoming).await else {
                    return;
                };
                if conn.alpn() == join::ALPN {
                    sync.answer_join(conn).await;
                } else {
                    sync.serve(conn, false).await;
                }
            });
        }
    }

    /// Answers the join of the connection `conn`. On a join, the
    /// connection goes on as the peer link.
    async fn answer_join(self: &Arc<Self>, conn: Connection) {
        let key = public_of(&conn);
        let Some(invites) = self.joins.get().cloned() else {
            (self.say)(text::join_refused_line(
                &key.to_string(),
                text::JOIN_NOT_THE_OWNER,
            ));
            Close::NotMember.close(&conn);
            return;
        };
        match join::answer(&conn, &self.hub, &invites).await {
            Ok(join::Answer::Joined) => {
                (self.say)(text::join_admitted(&self.who(&key)));
                self.serve(conn, false).await;
            }
            Ok(join::Answer::Refused { why }) => {
                (self.say)(text::join_refused_line(&key.to_string(), &why));
                Close::NotMember.close(&conn);
            }
            Err(e) => {
                (self.say)(text::join_refused_line(&key.to_string(), &format!("{e:#}")));
                Close::NotMember.close(&conn);
            }
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
                        Ok(Fresh::Switched) => break,
                        Ok(Fresh::Kept { .. } | Fresh::Fork(_)) => {}
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
        #[cfg(test)]
        drop(self.new_links.lock().await);
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
        let sent = Arc::new(AtomicBool::new(false));
        let end = self.run(&conn, remote, dialed, sent.clone()).await;
        if sent.load(Ordering::SeqCst) && conn.close_reason().is_some_and(|why| kept_left(&why)) {
            self.left_sent.fetch_add(1, Ordering::SeqCst);
        }
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
        lock(&self.seen).insert(remote, now_ms());
        (self.say)(text::peer_link_down(&who, &end.to_string()));
        self.ended.notify_waiters();
        end
    }

    /// The frames of the link `conn` to `remote`, both ways. The writer
    /// sets `sent` when it ended its stream after the `left` record of
    /// this daemon.
    async fn run(
        &self,
        conn: &Connection,
        remote: Public,
        dialed: bool,
        sent: Arc<AtomicBool>,
    ) -> End {
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
            sent,
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
                // The other daemon ends its stream after its `left`
                // record. The close with `Gone` tells it that this daemon
                // kept that record (01M52JF709YHYE7SG6V3SDHR74).
                Ok(None) if self.hub.read(|state| out_of(state, &remote)) => {
                    return End::Closed(Close::Gone, text::PEER_OTHER_LEFT.into());
                }
                Ok(None) => return End::lost(conn, text::PEER_STREAM_ENDED.into()),
                Err(wire::Bad::Read(e)) => return End::lost(conn, e.to_string()),
                Err(bad) => {
                    (self.say)(text::peer_bad_frame(&self.who(&remote), &bad.to_string()));
                    return End::Closed(Close::BadFrame, Close::BadFrame.to_string());
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
                Ok(Outcome::Duplicate) => {
                    self.duplicates.fetch_add(1, Ordering::Relaxed);
                }
                Ok(Outcome::Kept(_) | Outcome::Forked) => {}
                Err(e) => {
                    (self.say)(text::peer_cannot_keep(&format!("{e:#}")));
                    return End::Closed(Close::Disk, Close::Disk.to_string());
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
        (self.say)(line);
        End::Closed(Close::Refused, Close::Refused.to_string())
    }
}

/// After the `left` record of this daemon went out on `send`: ends the
/// stream, sets `sent`, and waits up to [`LEAVE_LIMIT`] for the other
/// daemon to close the connection: it reads each frame in order, and
/// closes with [`Close::Gone`] when it kept the record
/// (01M521J6ZRNWBSPK7WB23TVY59, [`kept_left`]).
async fn left(send: &mut SendStream, conn: &Connection, sent: &AtomicBool) -> End {
    if send.finish().is_ok() {
        sent.store(true, Ordering::SeqCst);
    }
    match tokio::time::timeout(LEAVE_LIMIT, conn.closed()).await {
        Ok(why) if kept_left(&why) => End::Closed(Close::Gone, text::PEER_LEFT.into()),
        Ok(_) => End::lost(conn, text::PEER_STREAM_ENDED.into()),
        Err(_) => End::Closed(Close::Gone, text::peer_left_no_close(LEAVE_LIMIT)),
    }
}

/// True for the `left` record of the daemon of `me`.
fn own_left(record: &Record, me: &Public) -> bool {
    record.of() == Type::Left && record.author() == *me
}

/// The frame of the record `record`.
fn frame_of(record: &Record) -> Frame {
    Frame::Record(record.bytes().to_vec())
}

/// Sends the frames of the link to `remote` on `send`: what the reader
/// asks, each fresh record, and the heads each [`HEADS_AGAIN`]. After the
/// `left` record of this daemon, fresh or in what the other side lacks,
/// it ends the stream ([`left`]).
async fn write_all(
    mut send: SendStream,
    mut asks: mpsc::UnboundedReceiver<Out>,
    mut fresh: broadcast::Receiver<Fresh>,
    hub: Arc<Hub>,
    remote: Public,
    conn: Connection,
    sent: Arc<AtomicBool>,
) -> End {
    let me = hub.public();
    let mut again = tokio::time::interval_at(Instant::now() + HEADS_AGAIN, HEADS_AGAIN);
    loop {
        let (frames, last): (Vec<Frame>, bool) = tokio::select! {
            biased;
            ask = asks.recv() => match ask {
                Some(Out::Heads) => (vec![Frame::Heads(hub.with_peers(Peers::heads))], false),
                Some(Out::Lacks(theirs)) => {
                    let lacked = hub.with_peers(|peers| peers.lacks(&theirs));
                    let last = lacked.iter().any(|record| own_left(record, &me));
                    (lacked.iter().map(frame_of).collect(), last)
                }
                None => return End::lost(&conn, text::PEER_STREAM_ENDED.into()),
            },
            got = fresh.recv() => match got {
                Ok(Fresh::Kept { record, from: None }) if own_left(&record, &me) => {
                    (vec![frame_of(&record)], true)
                }
                Ok(Fresh::Kept { record, from }) if from != Some(remote) => {
                    (vec![frame_of(&record)], false)
                }
                Ok(Fresh::Kept { .. } | Fresh::Switched) => continue,
                Ok(Fresh::Fork(fork)) => (vec![frame_of(&fork.first), frame_of(&fork.second)], false),
                Err(RecvError::Lagged(_)) => {
                    return End::Closed(Close::Behind, Close::Behind.to_string());
                }
                Err(RecvError::Closed) => return End::lost(&conn, text::PEER_STREAM_ENDED.into()),
            },
            _ = again.tick() => (vec![Frame::Heads(hub.with_peers(Peers::heads))], false),
        };
        for frame in &frames {
            if let Err(e) = wire::write(&mut send, frame).await {
                return End::lost(&conn, e.to_string());
            }
        }
        if last {
            return left(&mut send, &conn, &sent).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use iroh::endpoint::{ApplicationClose, VarInt};

    use super::*;

    /// A close of the other daemon with the code `code`.
    fn closed(code: u32) -> ConnectionError {
        ConnectionError::ApplicationClosed(ApplicationClose {
            error_code: VarInt::from_u32(code),
            reason: Default::default(),
        })
    }

    /// 01M52JF709YHYE7SG6V3SDHR74: only a close of the other daemon with
    /// `Gone` says that it kept the `left` record. A replace, a drop with
    /// the code 0, a fault, a close of this daemon and a lost connection
    /// do not.
    #[test]
    fn only_a_close_with_gone_says_that_the_other_daemon_kept_the_left_record() {
        assert!(kept_left(&closed(Close::Gone as u32)));
        for close in [
            Close::NotMember,
            Close::Refused,
            Close::BadFrame,
            Close::Replaced,
            Close::Behind,
            Close::Disk,
        ] {
            assert!(!kept_left(&closed(close as u32)), "{close:?}");
        }
        assert!(!kept_left(&closed(0)));
        assert!(!kept_left(&ConnectionError::LocallyClosed));
        assert!(!kept_left(&ConnectionError::TimedOut));
    }

    /// The names of each key of the riff of the owner [1] mike, with its
    /// `riff` record of the host `host`, and the member brett on kadomony
    /// [2].
    fn names(host: Option<&str>) -> [Names; 2] {
        use crate::daemon_key::Key;
        use crate::peer::body::{Body, Member, Riff};
        use crate::peer::keep::Peers;

        let dir = tempfile::tempdir().unwrap();
        let owner = Key::from_bytes(&[1; 32]);
        let brett = Key::from_bytes(&[2; 32]).public();
        let mut peers = Peers::open(dir.path(), owner.public(), |_| {}).unwrap();
        let riff = Riff {
            user: "mike".into(),
            host: host.map(str::to_owned),
        };
        peers.write(&owner, &Body::Riff(riff), 1).unwrap();
        let member = Member {
            key: brett,
            user: "brett".into(),
            host: "kadomony".into(),
        };
        peers.write(&owner, &Body::Member(member), 2).unwrap();
        [owner.public(), brett].map(|key| names_of(peers.state(), &key))
    }

    /// 01M51ZA0BDZNXBB7R5SB8CEGA9, 01M521J6T2PWA9JREDXJK6DZ0V: a link to
    /// the owner names the host of its `riff` record, as a link to a
    /// member names the host of its `member` record. A `riff` record with
    /// no host gives no host.
    #[test]
    fn a_link_to_the_owner_names_the_host_of_its_riff_record() {
        let some = |text: &str| Some(text.to_owned());
        let brett = (some("brett"), some("kadomony"));
        assert_eq!(
            names(Some("pangolin")),
            [(some("mike"), some("pangolin")), brett.clone()]
        );
        assert_eq!(names(None), [(some("mike"), None), brett]);
    }
}
