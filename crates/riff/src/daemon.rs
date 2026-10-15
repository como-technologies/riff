//! The riff daemon (#716, 01M4Z60B7FCREZDCTVR73WWRY1): it owns the
//! sessions of riff 3.0 on this machine. A systemd user unit runs it
//! ([`crate::service`]).
//!
//! # Design
//!
//! The daemon starts and stops each session through a [`Runtime`]: on
//! Linux, rootless podman. It gives each session a Unix socket of its
//! own (01M4Z60B7W6F3CC5XJE0VS7Y86). `riff` talks to the daemon on the
//! control socket. In each container, `riff relay` talks to the daemon
//! on the socket of its session ([`crate::session_relay`]).
//!
//! ```mermaid
//! flowchart LR
//!     R["riff run issue-12"] -- "control socket" --> D["riff daemon<br/>systemd user unit"]
//!     D -- "Runtime: podman" --> C1
//!     D -- "Runtime: podman" --> C2
//!     subgraph C1["container riff-issue-12-ID8"]
//!         L1["riff relay"] --> K1["claude -p"]
//!         S1["/run/riff/riff.sock"]
//!     end
//!     subgraph C2["container riff-issue-9-ID8"]
//!         L2["riff relay"] --> K2["claude -p"]
//!         S2["/run/riff/riff.sock"]
//!     end
//!     L1 -- "its own socket" --> D
//!     L2 -- "its own socket" --> D
//! ```
//!
//! # The folder of the daemon
//!
//! The daemon keeps its sockets in [`dir`]: `daemon` in the data folder
//! of riff ([`crate::run::data_dir`]), with mode 0700
//! (01M4ZDTVQEHD87WD5TSN93XFSS).
//!
//! | Path | Holds |
//! |---|---|
//! | `riff.sock` | The control socket: `riff run`, `riff daemon status`. |
//! | `sessions/NAME/` | The folder of the socket of the session `NAME`, with mode 0700. |
//! | `sessions/NAME/riff.sock` | The socket of that session. |
//! | `live/NAME.json` | The [`Record`] of the session `NAME`, with no secret. |
//! | `state/` | The log and the checkpoints of the state of the daemon ([`state_dir`]). |
//!
//! The daemon binds the folder of a session at [`IN_SESSION`] in its
//! container, with write, and sets [`SOCKET_VAR`]
//! (01M4ZDTVSP0T1YENKAWGKA114D). Its listener tells the daemon which
//! session asks: one listener for each session. No other session has
//! that bind. The daemon refuses a spec that binds its folder, a folder
//! above it, or a path in it ([`Daemon::check`]). So a session cannot
//! reach the socket of another session, or the control socket. The
//! folder of a session goes at the end of the session.
//!
//! A path of a socket can be longer than the address of a Unix socket
//! takes (108 bytes). On Linux, riff binds and connects through a
//! descriptor of the folder: `/proc/self/fd/N/riff.sock`.
//!
//! # The state of the daemon
//!
//! The daemon keeps the state of riff on its machine with the state
//! machine of riff-server, the crate `riff-state`
//! (01M4Z60B8AFV68ZSAFA540G73C, 01M4ZHW1Y1X21ZPV053D0CXWMC): a
//! [`Local`] in [`state_dir`] (01M4ZHW2083VFTQJ772ZJ27XAC).
//!
//! ```mermaid
//! flowchart LR
//!     O["Daemon::open"] --> L["Local::open: the newest checkpoint,<br/>then the log after it"]
//!     L -->|no riff| M["MakeRiff with a random ID"]
//!     L --> S["serve"]
//!     M --> S
//!     S -->|"a command (#719)"| R["Local::run: the records<br/>in the log, then the reply"]
//!     S -->|"the stop"| C["Local::checkpoint"]
//! ```
//!
//! - [`Daemon::open`] loads the state. At the first start, it makes the
//!   riff of the machine with a random ID.
//! - Each change goes through [`Daemon::state`]: one command at a time,
//!   with its records in the log before its reply
//!   (01M4ZHW22F8HE2E1WDSTNDJW4H). The riff tools of a session act on it
//!   from #719. Until then, they act on the riff-server of `riff run`
//!   ([`Daemon::tools_of`]).
//! - At the stop, [`Daemon::serve`] writes a checkpoint. A start after a
//!   crash replays the log after the last checkpoint.
//! - `riff daemon status` shows the ID of the riff and the position of
//!   the log ([`Status::riff`], [`Status::position`],
//!   01M4ZHW24JZZXM3JJXG3PRJS7N).
//!
//! # The relay
//!
//! The daemon binds its own binary at [`RELAY_BIN`] in each container,
//! with no write, and runs `RELAY_BIN relay -- COMMAND`: `riff relay` is
//! the entrypoint (01M4Z60B7KEV0H161RBZ1SDRY2, 01M4ZHSZZDH0C01A5EVD0KFY14).
//! The relay gives `claude` each input, and sends the daemon each event
//! ([`crate::session_relay`]).
//!
//! # The protocol
//!
//! JSON, one message on each line (01M4ZDTVVV56X964S28GS6K2Z2). The
//! first line of each connection on the control socket is a [`Hello`]:
//! the version [`PROTOCOL`] and one [`Ask`]. The daemon refuses another
//! version, and says to start it again with this riff. The daemon
//! answers with [`Say`] lines.
//!
//! | Ask | The daemon says |
//! |---|---|
//! | [`Ask::Status`] | [`Say::Status`] |
//! | [`Ask::Image`] | [`Say::Line`] for a pull, then [`Say::Done`] or [`Say::Failed`] |
//! | [`Ask::Start`] | [`Say::Started`], each [`Say::Event`] and [`Say::Stderr`], then [`Say::Ended`] or [`Say::Failed`] |
//! | [`Ask::Follow`] | each [`Say::Event`] after a number, then as [`Ask::Start`] |
//!
//! After [`Ask::Start`] or [`Ask::Follow`], `riff` can send
//! [`Then::Stop`]: the daemon stops the container. When `riff` closes the
//! connection, the session goes on: the daemon owns it.
//!
//! `riff run` asks [`Ask::Follow`] after a new start of the daemon, with
//! the number of the last event that it showed
//! (01M4ZKX0F25S284R0N7KZVTHNA). `riff daemon events NAME` asks it with
//! the number 0 ([`show_events`]).
//!
//! On the socket of a session, the first line is a [`SessionAsk`]. The
//! daemon takes a version of the protocol from [`PROTOCOL`] - 1 to
//! [`PROTOCOL`] + 1 there ([`compatible`], 01M4ZHSZPRYK42C20ETXM3HJZ2):
//!
//! | Ask | The daemon |
//! |---|---|
//! | [`SessionAsk::Hello`] | says [`Say::Session`]: the session ID, the item and the name of the container |
//! | [`SessionAsk::Relay`] | relays the session ([`crate::session_relay`]) |
//! | [`SessionAsk::Mcp`] | serves the riff tools of the session that only talk ([`Daemon::tools_of`]) |
//!
//! # A session
//!
//! The daemon takes each event of the relay in order, takes each secret
//! out of it ([`Scrub`]), writes it to `events.jsonl` in the folder of
//! the session, and sends it to each `riff` that follows the session.
//! The event with the sequence number N is line N of `events.jsonl`. At
//! the `result` of the turn, it closes the input: `claude` ends. When
//! the daemon cannot keep an event, it closes the connection of the
//! relay with no ack. The relay connects again and sends the event
//! again. The daemon acks the exit of `claude` only when it has each
//! event before it (01M4ZHSZMH4Y8CB41FYGE3C3R2).
//!
//! A session ends when the daemon has the exit of the relay, or when its
//! container ends (01M4ZHT03MAH0B5Q4S6GZC5JRP). A [`Then::Stop`] stops
//! the container: the relay gives `claude` the SIGTERM.
//!
//! # A stop of the daemon
//!
//! A stop or a restart of the daemon stops no container
//! (01M4ZDTW00B7N6T9P3MK066KA7). conmon holds each container in a scope
//! of its own ([`crate::podman`]). The relay keeps `claude` running and
//! keeps its events while the daemon is away (01M4Z60B7Z24WBD6HV6ER0WN9K).
//!
//! The daemon keeps a [`Record`] of each session in `live/NAME.json`.
//! At its start, a new daemon reads each record ([`Daemon::recover`]).
//! It takes up each session whose container runs, or when podman cannot
//! say: it binds the socket of the session again in the same folder, and
//! the relay connects again. It removes the record of each other one
//! (01M4ZHT01HH88KPA8628M7VBEY).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant R as riff run
//!     participant D1 as riff daemon
//!     participant L as riff relay
//!     participant D2 as new riff daemon
//!     R->>D1: start
//!     D1->>D1: live/NAME.json
//!     L->>D1: event 1, event 2
//!     D1-->>R: event 1, event 2
//!     Note over D1: SIGTERM: the daemon stops
//!     Note over L: keeps event 3
//!     D2->>D2: live/NAME.json: the container runs
//!     L->>D2: hello relay
//!     D2-->>L: resume: events 2
//!     L->>D2: event 3
//!     R->>D2: follow NAME after 2
//!     D2-->>R: event 3
//! ```

use std::collections::BTreeMap;
use std::future::Future;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use riff_core::build::Build;
use riff_core::name::SessionUri;
use riff_state::checkpoint;
use riff_state::local::Local;
use riff_state::state::{Caller, MakeRiff, State};
use riff_state::store::Dir;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Notify, mpsc};
use tokio::task::{JoinHandle, JoinSet};

use crate::run::{Scrub, user_line};
use crate::runtime::{Bind, End, Env, Runtime, SessionSpec, Source};
use crate::session_relay::Up;
use crate::text;

/// The version of the protocol between `riff` and the daemon, and
/// between the relay and the daemon.
pub const PROTOCOL: u32 = 2;

/// The name of each socket of the daemon.
pub const SOCKET: &str = "riff.sock";

/// The folder of its socket in a session.
pub const IN_SESSION: &str = "/run/riff";

/// The variable that names the socket in a session.
pub const SOCKET_VAR: &str = "RIFF_SOCKET";

/// The path of the binary of riff in a session (01M4ZHSZZDH0C01A5EVD0KFY14).
pub const RELAY_BIN: &str = "/opt/riff/riff";

/// The folder of the records of the live sessions, in the folder of the
/// daemon.
pub const LIVE: &str = "live";

/// The log of the events of a session, in its folder.
pub const EVENTS: &str = "events.jsonl";

/// After the end of a container, or of the connection of a relay, the
/// longest wait for the exit of the relay.
const GONE_WAIT: Duration = Duration::from_secs(1);

/// How often the daemon looks at the container of a session that it took
/// up while no relay is connected: a relay that gave up does not come.
const LOOK_EVERY: Duration = Duration::from_secs(30);

/// The folder of the daemon in the data folder `data` of riff.
///
/// ```
/// use std::path::Path;
///
/// let dir = riff::daemon::dir_in(Path::new("/h/.local/share/riff"));
/// assert_eq!(dir, Path::new("/h/.local/share/riff/daemon"));
/// assert_eq!(riff::daemon::socket(&dir), Path::new("/h/.local/share/riff/daemon/riff.sock"));
/// assert_eq!(
///     riff::daemon::session_dir(&dir, "riff-issue-12-1a2b3c4d"),
///     Path::new("/h/.local/share/riff/daemon/sessions/riff-issue-12-1a2b3c4d")
/// );
/// assert_eq!(
///     riff::daemon::record_path(&dir, "riff-issue-12-1a2b3c4d"),
///     Path::new("/h/.local/share/riff/daemon/live/riff-issue-12-1a2b3c4d.json")
/// );
/// ```
pub fn dir_in(data: &Path) -> PathBuf {
    data.join("daemon")
}

/// The folder of the daemon of this process: [`dir_in`] the data folder
/// of riff ([`crate::run::data_dir`]).
pub fn dir() -> Result<PathBuf> {
    Ok(dir_in(&crate::run::data_dir()?))
}

/// The control socket in the folder `dir` of the daemon.
pub fn socket(dir: &Path) -> PathBuf {
    dir.join(SOCKET)
}

/// The folder of the socket of the session `name`.
pub fn session_dir(dir: &Path, name: &str) -> PathBuf {
    dir.join("sessions").join(name)
}

/// The [`Record`] of the session `name`.
pub fn record_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(LIVE).join(format!("{name}.json"))
}

/// The folder of the state of the daemon in its folder `dir`: the log
/// and the checkpoints.
///
/// ```
/// use std::path::Path;
///
/// let dir = Path::new("/h/.local/share/riff/daemon");
/// assert_eq!(riff::daemon::state_dir(dir), Path::new("/h/.local/share/riff/daemon/state"));
/// ```
pub fn state_dir(dir: &Path) -> PathBuf {
    dir.join("state")
}

/// The path of the socket in a session: `/run/riff/riff.sock`.
pub fn in_session() -> PathBuf {
    Path::new(IN_SESSION).join(SOCKET)
}

/// True when the daemon takes a session of the protocol `theirs`: one
/// version before its own, its own, and one after
/// (01M4ZHSZPRYK42C20ETXM3HJZ2).
///
/// ```
/// use riff::daemon::{PROTOCOL, compatible};
///
/// assert!(compatible(PROTOCOL) && compatible(PROTOCOL - 1) && compatible(PROTOCOL + 1));
/// assert!(!compatible(PROTOCOL - 2) && !compatible(PROTOCOL + 2));
/// ```
pub fn compatible(theirs: u32) -> bool {
    theirs.abs_diff(PROTOCOL) <= 1
}

/// The binary of this process: the daemon binds it into each session.
/// After an update of riff, the path holds the new binary
/// (01M4Z60B800BH5BHX0H9JVDG6A).
pub fn own_binary() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("cannot find the path of riff")?;
    let text = exe.to_string_lossy();
    Ok(match text.strip_suffix(" (deleted)") {
        Some(path) => PathBuf::from(path),
        None => exe,
    })
}

/// Makes the folder `dir` with mode 0700.
fn private_dir(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("cannot set the mode of {}", dir.display()))
}

/// Runs `with` on a path of the socket `name` in the folder `dir` that
/// fits in the address of a Unix socket. On Linux, it is a path through
/// a descriptor of the folder: `/proc/self/fd/N/NAME`.
fn short_path<T>(dir: &Path, name: &str, with: impl FnOnce(&Path) -> T) -> Result<T> {
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        let folder =
            std::fs::File::open(dir).with_context(|| format!("cannot open {}", dir.display()))?;
        let path = PathBuf::from(format!("/proc/self/fd/{}/{name}", folder.as_raw_fd()));
        let out = with(&path);
        drop(folder);
        Ok(out)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(with(&dir.join(name)))
    }
}

/// Binds the socket `name` in the folder `dir`, with mode 0600.
fn bind_in(dir: &Path, name: &str) -> Result<UnixListener> {
    use std::os::unix::fs::PermissionsExt;
    let listener = short_path(dir, name, |path| UnixListener::bind(path))?
        .with_context(|| format!("cannot listen on {}", dir.join(name).display()))?;
    std::fs::set_permissions(dir.join(name), std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Connects to the socket `name` in the folder `dir`.
pub(crate) async fn connect_in(dir: &Path, name: &str) -> std::io::Result<UnixStream> {
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        let folder = std::fs::File::open(dir)?;
        let path = format!("/proc/self/fd/{}/{name}", folder.as_raw_fd());
        let stream = UnixStream::connect(path).await;
        drop(folder);
        stream
    }
    #[cfg(not(target_os = "linux"))]
    {
        UnixStream::connect(dir.join(name)).await
    }
}

/// The first line of each connection: the version of the protocol, and
/// the ask.
#[derive(Serialize, Deserialize)]
pub struct Hello {
    /// [`PROTOCOL`] of the side that asks.
    pub protocol: u32,
    /// What it asks.
    pub ask: Ask,
}

/// What `riff` asks the daemon.
#[derive(Serialize, Deserialize)]
#[serde(tag = "ask", rename_all = "snake_case")]
pub enum Ask {
    /// The state of the daemon and its sessions.
    Status,
    /// Have the image on this machine: pull it when it is not here.
    Image {
        /// The image.
        image: String,
    },
    /// Start a session, and send its events.
    Start(Box<Start>),
    /// Send the events of a live session after the number `after`, and
    /// each later one (01M4ZHT05QHQRQ2KGVJFB9F7HD).
    Follow {
        /// The name of its container.
        name: String,
        /// The sequence number of the last event that `riff` has.
        after: u64,
    },
}

/// A session to start. It holds the values of the host and the
/// secrets, so it has no `Debug`: no log shows it.
#[derive(Serialize, Deserialize)]
pub struct Start {
    /// The riff session ID.
    pub session: String,
    /// The work item, for example `issue-12`.
    pub item: String,
    /// The folder of the session: `events.jsonl` goes there.
    pub folder: PathBuf,
    /// The spec. The daemon adds the bind of the socket and of the
    /// relay.
    pub spec: SessionSpec,
    /// The start text: the first message to `claude`.
    pub start: String,
    /// The value of each [`Env::Host`] of the spec.
    pub values: Vec<(String, String)>,
    /// The secrets that no event shows ([`Scrub`]).
    pub secrets: Vec<String>,
    /// The riff-server of the session: its riff tools act there.
    pub server: String,
    /// The URI of the session.
    pub me: SessionUri,
}

/// A line of `riff` after [`Ask::Start`] or [`Ask::Follow`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "then", rename_all = "snake_case")]
pub enum Then {
    /// Stop the container.
    Stop,
}

/// What a session asks on its own socket: the first line of a
/// connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "ask", rename_all = "snake_case")]
pub enum SessionAsk {
    /// Who am I?
    Hello {
        /// [`PROTOCOL`] of the session.
        protocol: u32,
    },
    /// The relay of the session ([`crate::session_relay`]).
    Relay {
        /// [`PROTOCOL`] of the relay.
        protocol: u32,
        /// The number of inputs that the relay gave `claude`.
        inputs: u64,
        /// True when the relay closed the stdin of `claude`.
        closed: bool,
    },
    /// The riff tools of the session, over MCP
    /// (01M4ZHSZXAY7F7E17B3CMHG562).
    Mcp {
        /// [`PROTOCOL`] of the relay.
        protocol: u32,
        /// The initialize request of `claude`, after a new start of the
        /// daemon: the daemon skips the handshake.
        client: Option<Value>,
    },
}

/// A session that the daemon owns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Live {
    /// The name of its container.
    pub name: String,
    /// The riff session ID.
    pub session: String,
    /// The work item.
    pub item: String,
    /// The image.
    pub image: String,
    /// The start, in milliseconds since the epoch.
    pub since_ms: i64,
}

/// The state of the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// The process ID of the daemon.
    pub pid: u32,
    /// The build of the daemon.
    pub build: String,
    /// The start of the daemon, in milliseconds since the epoch.
    pub since_ms: i64,
    /// The folder of the daemon.
    pub dir: PathBuf,
    /// The ID of the riff of this machine, from the state of the daemon.
    #[serde(default)]
    pub riff: Option<String>,
    /// The position of the last record of the log of the daemon.
    #[serde(default)]
    pub position: u64,
    /// Its sessions, by name.
    pub sessions: Vec<Live>,
}

/// How a session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ended {
    /// The exit code of `claude`, or none after a signal.
    pub code: Option<i32>,
    /// True when a turn of `claude` ended with an error, a stop ended the
    /// session, or the container ended with no exit of the relay.
    pub failed: bool,
}

/// What the daemon says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "say", rename_all = "snake_case")]
pub enum Say {
    /// A line for the person.
    Line {
        /// The text.
        text: String,
    },
    /// The session started.
    Started {
        /// The name of its container.
        name: String,
        /// The folder of its socket on the host.
        socket: PathBuf,
    },
    /// One event of `claude`, with no secret.
    Event {
        /// Its sequence number: its line in `events.jsonl`.
        seq: u64,
        /// The line.
        line: String,
    },
    /// One line of the output of the container, with no secret: the
    /// notes of the relay and the stderr of `claude`.
    Stderr {
        /// The line.
        line: String,
    },
    /// The session ended.
    Ended(Ended),
    /// The state of the daemon.
    Status(Status),
    /// On the socket of a session: who that session is.
    Session {
        /// The riff session ID.
        session: String,
        /// The work item.
        item: String,
        /// The name of the container.
        name: String,
    },
    /// To the relay: the daemon has this many events.
    Resume {
        /// The number of events in `events.jsonl`.
        events: u64,
    },
    /// To the relay: one line for the stdin of `claude`.
    Input {
        /// The line.
        line: String,
    },
    /// To the relay: close the stdin of `claude`.
    Close,
    /// To the relay: the daemon has each line up to this number.
    Ack {
        /// The sequence number.
        seq: u64,
    },
    /// The ask is done.
    Done,
    /// The ask failed.
    Failed {
        /// Why.
        error: String,
    },
}

/// The lines that the daemon says on one connection. A send never
/// waits: a task writes the lines. When `riff` left, the lines go
/// nowhere, and the session goes on.
#[derive(Clone)]
pub struct Out(mpsc::UnboundedSender<Say>);

impl Out {
    /// Says `say`.
    pub fn say(&self, say: Say) {
        let _ = self.0.send(say);
    }

    /// An `Out` that keeps each line, for a test.
    pub fn keep() -> (Out, mpsc::UnboundedReceiver<Say>) {
        let (send, read) = mpsc::unbounded_channel();
        (Out(send), read)
    }

    /// True while its connection is open.
    fn open(&self) -> bool {
        !self.0.is_closed()
    }

    /// True when `other` says on the same connection.
    fn same(&self, other: &Out) -> bool {
        self.0.same_channel(&other.0)
    }
}

/// The [`Out`] of the write half `write`, and its task. The task ends
/// when each [`Out`] is gone and each line is written.
fn writer(mut write: OwnedWriteHalf) -> (Out, JoinHandle<()>) {
    let (out, mut read) = Out::keep();
    let task = tokio::spawn(async move {
        while let Some(say) = read.recv().await {
            let Ok(mut line) = serde_json::to_string(&say) else {
                continue;
            };
            line.push('\n');
            if write.write_all(line.as_bytes()).await.is_err() {
                return;
            }
        }
        let _ = write.shutdown().await;
    });
    (out, task)
}

/// The time now in milliseconds since the epoch.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// `path` with each link resolved, when it exists.
fn resolved(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

/// What the daemon keeps on disk of a live session, in `live/NAME.json`
/// of its folder. It holds no secret (01M4ZHT01HH88KPA8628M7VBEY).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The session.
    pub live: Live,
    /// The folder of the session: `events.jsonl` is there.
    pub folder: PathBuf,
    /// The riff-server of the session: its riff tools act there.
    pub server: String,
    /// The URI of the session.
    pub me: SessionUri,
    /// Each input for `claude`, in order: one stream-json line each.
    pub inputs: Vec<String>,
    /// True when the input of `claude` closes after the last input.
    pub closed: bool,
    /// True when a turn ended with an error.
    pub failed: bool,
}

/// What a link of a session tells the task that waits for its end.
enum Note {
    /// The relay sent the exit of `claude`, and the daemon took it.
    Exit(Option<i32>),
    /// The connection of the relay ended.
    Gone,
}

/// A session that the daemon owns, in memory.
struct Owned {
    /// The name of its container.
    name: String,
    /// The folder of its socket.
    socket: PathBuf,
    /// Its [`Record`] on disk.
    record_path: PathBuf,
    /// The secrets that no event shows. A daemon that takes up a session
    /// after a restart knows none: the relay took them out.
    scrub: Scrub,
    state: Mutex<SessionState>,
    /// The links tell the task of the end.
    notes: mpsc::UnboundedSender<Note>,
    /// A `riff` asked to stop the container.
    stop: Notify,
    /// The task of the socket of the session.
    door: Mutex<Option<JoinHandle<()>>>,
}

/// The state of an [`Owned`] session.
struct SessionState {
    record: Record,
    /// The number of events in `events.jsonl`.
    seq: u64,
    log: std::fs::File,
    /// The connections of `riff` that follow the session.
    followers: Vec<Out>,
    /// The connection of the relay.
    relay: Option<Out>,
}

impl Owned {
    fn lock(&self) -> MutexGuard<'_, SessionState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Writes the record `record` in place of the one on disk.
    fn save(&self, record: &Record) -> Result<()> {
        let tmp = self.record_path.with_extension("json.new");
        std::fs::write(&tmp, serde_json::to_vec_pretty(record)?)
            .with_context(|| format!("cannot write {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.record_path)
            .with_context(|| format!("cannot write {}", self.record_path.display()))
    }

    /// Says `say` to each `riff` that follows the session.
    fn tell(state: &mut SessionState, say: &Say) {
        state.followers.retain(Out::open);
        for out in &state.followers {
            out.say(say.clone());
        }
    }

    /// Takes the event `seq` of the relay, and gives the number of
    /// events that the daemon has. The daemon takes only the next event:
    /// it has each one before it.
    fn event(&self, seq: u64, line: &str) -> u64 {
        let mut state = self.lock();
        if seq != state.seq + 1 {
            return state.seq;
        }
        let line = self.scrub.clean(line).into_owned();
        if state.log.write_all(format!("{line}\n").as_bytes()).is_err() {
            return state.seq;
        }
        state.seq = seq;
        if let Ok(event) = serde_json::from_str::<Value>(&line)
            && event["type"] == "result"
        {
            state.record.failed |= event["is_error"] == true;
            // The end of the one turn: claude ends when its input
            // closes.
            if !state.record.closed {
                state.record.closed = true;
                if let Some(relay) = &state.relay {
                    relay.say(Say::Close);
                }
            }
            if let Err(e) = self.save(&state.record) {
                eprintln!("riff daemon: {e:#}");
            }
        }
        Self::tell(&mut state, &Say::Event { seq, line });
        seq
    }

    /// Follows the session on `out`: each event after `after`, then each
    /// new one.
    fn follow(&self, out: Out, after: u64) -> Result<()> {
        let mut state = self.lock();
        if after < state.seq {
            let events = state.record.folder.join(EVENTS);
            let text = std::fs::read_to_string(&events)
                .with_context(|| format!("cannot read {}", events.display()))?;
            for (seq, line) in (1..=state.seq).zip(text.lines()) {
                if seq > after {
                    out.say(Say::Event {
                        seq,
                        line: line.to_owned(),
                    });
                }
            }
        }
        state.followers.push(out);
        Ok(())
    }

    /// Relays the session on the connection of its relay: the hello said
    /// `inputs` and `closed`.
    async fn relay(
        &self,
        inputs: u64,
        closed: bool,
        mut lines: Lines<BufReader<OwnedReadHalf>>,
        write: OwnedWriteHalf,
    ) {
        let (out, written) = writer(write);
        {
            let mut state = self.lock();
            out.say(Say::Resume { events: state.seq });
            let given = usize::try_from(inputs).unwrap_or(usize::MAX);
            for line in state.record.inputs.iter().skip(given) {
                out.say(Say::Input { line: line.clone() });
            }
            if state.record.closed && !closed {
                out.say(Say::Close);
            }
            state.relay = Some(out.clone());
        }
        while let Ok(Some(line)) = lines.next_line().await {
            // A line of a newer relay that this daemon does not know:
            // skip it.
            match serde_json::from_str::<Up>(&line) {
                Ok(Up::Event { seq, line }) => {
                    let has = self.event(seq, &line);
                    if has < seq {
                        // The daemon could not keep the event: the relay
                        // connects again and sends it again.
                        break;
                    }
                    out.say(Say::Ack { seq: has });
                }
                Ok(Up::Exit { seq, code }) => {
                    // The exit counts only after each event before it.
                    if self.lock().seq + 1 < seq {
                        break;
                    }
                    out.say(Say::Ack { seq });
                    let _ = self.notes.send(Note::Exit(code));
                }
                Err(_) => {}
            }
        }
        {
            let mut state = self.lock();
            if state.relay.as_ref().is_some_and(|r| r.same(&out)) {
                state.relay = None;
            }
        }
        let _ = self.notes.send(Note::Gone);
        drop(out);
        let _ = written.await;
    }
}

/// The number of lines of the file `path`: 0 when it is not there.
fn lines_of(path: &Path) -> u64 {
    std::fs::read(path).map_or(0, |bytes| {
        bytes.iter().filter(|&&b| b == b'\n').count() as u64
    })
}

/// The daemon: the sessions of this machine, on the runtime `R`, and
/// the state of riff on this machine.
pub struct Daemon<R> {
    runtime: R,
    dir: PathBuf,
    since_ms: i64,
    /// The binary that the daemon binds into each session.
    relay_bin: PathBuf,
    live: Mutex<BTreeMap<String, Arc<Owned>>>,
    state: Local,
}

/// A random ID of 16 bytes as hex: the ID of a new riff.
fn random_id() -> String {
    let mut bytes = [0u8; 16];
    // The OS random source fails only when the OS is broken.
    getrandom::fill(&mut bytes).expect("the OS gives random bytes");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Binds the control socket in the folder `dir` of the daemon. It
/// refuses when a daemon answers there. It takes the place of a socket
/// that no daemon answers.
pub async fn listen(dir: &Path) -> Result<UnixListener> {
    private_dir(dir)?;
    let path = socket(dir);
    if std::fs::symlink_metadata(&path).is_ok() {
        if let Ok(status) = status(dir).await {
            bail!("{}", text::daemon_runs_already(status.pid, &path));
        }
        std::fs::remove_file(&path).with_context(|| format!("cannot remove {}", path.display()))?;
    }
    bind_in(dir, SOCKET)
}

impl<R: Runtime + 'static> Daemon<R> {
    /// A daemon with no session, on `runtime`, with its folder `dir`. It
    /// binds its own binary into each session ([`own_binary`]). It
    /// loads its state from [`state_dir`]: the newest checkpoint, and
    /// the log after it. At its first start, it makes the riff of this
    /// machine with a random ID. Only one daemon may open a folder: call
    /// [`listen`] first.
    pub async fn open(runtime: R, dir: PathBuf) -> Result<Self> {
        private_dir(&dir)?;
        let state_dir = state_dir(&dir);
        let store = Arc::new(Dir::new(&state_dir));
        let state = Local::open(store, checkpoint::Settings::default())
            .await
            .with_context(|| text::daemon_no_state(&state_dir))?;
        if state.read(State::riff_id).await.is_none() {
            let make = MakeRiff {
                riff_id: random_id(),
            };
            state
                .run(&Caller::server(), &make)
                .await
                .with_context(|| text::daemon_no_state(&state_dir))?;
        }
        Ok(Daemon {
            runtime,
            dir,
            since_ms: now_ms(),
            relay_bin: own_binary().unwrap_or_else(|_| PathBuf::from("riff")),
            live: Mutex::new(BTreeMap::new()),
            state,
        })
    }

    /// The state of riff on this machine. The riff tools of a session
    /// act on it from #719.
    pub fn state(&self) -> &Local {
        &self.state
    }

    /// The state of the daemon.
    pub async fn status(&self) -> Status {
        let (riff, position) = self
            .state
            .read(|state| (state.riff_id(), state.written_position()))
            .await;
        Status {
            pid: std::process::id(),
            build: Build::this().to_string(),
            since_ms: self.since_ms,
            dir: self.dir.clone(),
            riff,
            position,
            sessions: self
                .lock()
                .values()
                .map(|owned| owned.lock().record.live.clone())
                .collect(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, Arc<Owned>>> {
        self.live.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Answers each connection of `listener` until `stop` ends. The stop
    /// stops no session (01M4ZDTW00B7N6T9P3MK066KA7). At the stop, it
    /// writes a checkpoint of the state.
    pub async fn serve(
        self: Arc<Self>,
        listener: UnixListener,
        stop: impl Future<Output = ()>,
    ) -> Result<()> {
        tokio::pin!(stop);
        loop {
            tokio::select! {
                () = &mut stop => {
                    if let Err(e) = self.state.checkpoint().await {
                        eprintln!("riff daemon: {e}");
                    }
                    return Ok(());
                }
                accepted = listener.accept() => match accepted {
                    Ok((conn, _)) => {
                        let daemon = self.clone();
                        tokio::spawn(async move {
                            if let Err(e) = daemon.answer(conn).await {
                                eprintln!("riff daemon: {e:#}");
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("riff daemon: cannot accept a connection: {e}");
                        // For example no file descriptor is free: wait
                        // a moment, and do not spin.
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                },
            }
        }
    }

    /// Answers one connection.
    async fn answer(self: Arc<Self>, conn: UnixStream) -> Result<()> {
        let (read, write) = conn.into_split();
        let mut lines = BufReader::new(read).lines();
        let (out, written) = writer(write);
        if let Some(first) = lines.next_line().await? {
            match hello(&first) {
                Ok(Ask::Status) => out.say(Say::Status(self.status().await)),
                Ok(Ask::Image { image }) => self.image(&image, &out).await,
                Ok(Ask::Start(start)) => self.start(*start, lines, &out).await,
                Ok(Ask::Follow { name, after }) => self.follow(&name, after, lines, &out),
                Err(error) => out.say(Say::Failed { error }),
            }
        }
        drop(out);
        let _ = written.await;
        Ok(())
    }

    /// Has the image `image` on this machine: it pulls it when it is not
    /// here (01M4Z81B5A456HDKW7G0WFGCY6).
    async fn image(&self, image: &str, out: &Out) {
        let had = match self.runtime.has_image(image).await {
            Ok(had) => had,
            Err(e) => {
                out.say(Say::Failed {
                    error: format!("{e:#}"),
                });
                return;
            }
        };
        if !had {
            out.say(Say::Line {
                text: text::run_pulling(image),
            });
            if let Err(e) = self.runtime.pull(image).await {
                out.say(Say::Failed {
                    error: format!("{}: {e:#}", text::run_no_image(image)),
                });
                return;
            }
        }
        out.say(Say::Done);
    }

    /// Refuses a spec that would show the folder of the daemon to the
    /// session: a bind of the folder, of a folder above it, or of a path
    /// in it, or a bind at [`IN_SESSION`] or at [`RELAY_BIN`]
    /// (01M4ZDTVQEHD87WD5TSN93XFSS).
    pub fn check(&self, spec: &SessionSpec) -> Result<()> {
        let ours = resolved(&self.dir);
        for bind in &spec.binds {
            if bind.target.starts_with(IN_SESSION) || bind.target.starts_with(RELAY_BIN) {
                bail!("{}", text::daemon_bind_in_session(&bind.target));
            }
            if let Source::Path(source) = &bind.source {
                let source = resolved(source);
                if ours.starts_with(&source) || source.starts_with(&ours) {
                    bail!("{}", text::daemon_bind_shows(&source));
                }
            }
        }
        Ok(())
    }

    /// Owns the session of `record`: its log, its record on disk, and
    /// its socket. With `again`, the daemon takes up a session that runs:
    /// it goes on with the log, and binds the socket again in the folder
    /// that the container sees.
    fn own(
        self: &Arc<Self>,
        record: Record,
        scrub: Scrub,
        again: bool,
    ) -> Result<(Arc<Owned>, mpsc::UnboundedReceiver<Note>)> {
        let name = record.live.name.clone();
        let events = record.folder.join(EVENTS);
        // The log comes first: a session that cannot log starts no
        // container.
        let (log, seq) = if again {
            let log = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&events);
            (log, lines_of(&events))
        } else {
            (std::fs::File::create(&events), 0)
        };
        let log = log.with_context(|| format!("cannot write {}", events.display()))?;
        private_dir(&self.dir.join(LIVE))?;
        let (notes, noted) = mpsc::unbounded_channel();
        let owned = Arc::new(Owned {
            socket: session_dir(&self.dir, &name),
            record_path: record_path(&self.dir, &name),
            name: name.clone(),
            scrub,
            state: Mutex::new(SessionState {
                record,
                seq,
                log,
                followers: Vec::new(),
                relay: None,
            }),
            notes,
            stop: Notify::new(),
            door: Mutex::new(None),
        });
        {
            let mut live = self.lock();
            if live.contains_key(&name) {
                bail!("{}", text::daemon_runs_session(&name));
            }
            live.insert(name.clone(), owned.clone());
        }
        let opened = owned
            .save(&owned.lock().record.clone())
            .and_then(|()| self.door(&owned, again));
        match opened {
            Ok(door) => {
                *owned.door.lock().unwrap_or_else(PoisonError::into_inner) = Some(door);
                Ok((owned, noted))
            }
            Err(e) => {
                self.lock().remove(&name);
                let _ = std::fs::remove_file(&owned.record_path);
                Err(e)
            }
        }
    }

    /// Starts the session of `start`, says its events on `out`, and
    /// waits for its end. A [`Then::Stop`] on `from` stops it.
    async fn start(
        self: &Arc<Self>,
        start: Start,
        from: Lines<BufReader<OwnedReadHalf>>,
        out: &Out,
    ) {
        let Start {
            session,
            item,
            folder,
            mut spec,
            start,
            values,
            secrets,
            server,
            me,
        } = start;
        let name = spec.name.clone();
        if let Err(e) = self.check(&spec) {
            out.say(Say::Failed {
                error: format!("{e:#}"),
            });
            return;
        }
        let record = Record {
            live: Live {
                name: name.clone(),
                session: session.clone(),
                item: item.clone(),
                image: spec.image.clone(),
                since_ms: now_ms(),
            },
            folder,
            server,
            me,
            inputs: vec![user_line(&start)],
            closed: false,
            failed: false,
        };
        let scrub = Scrub::new(secrets);
        let (owned, notes) = match self.own(record, scrub.clone(), false) {
            Ok(owned) => owned,
            Err(e) => {
                out.say(Say::Failed {
                    error: format!("{e:#}"),
                });
                return;
            }
        };
        owned.lock().followers.push(out.clone());
        spec.binds.push(Bind {
            source: Source::Path(owned.socket.clone()),
            target: IN_SESSION.into(),
            write: true,
        });
        spec.binds.push(Bind {
            source: Source::Path(self.relay_bin.clone()),
            target: RELAY_BIN.into(),
            write: false,
        });
        spec.env.push(Env::Value(
            SOCKET_VAR.into(),
            in_session().to_string_lossy().into_owned(),
        ));
        let mut command: Vec<String> = [RELAY_BIN, "relay", "--"].map(String::from).into();
        command.append(&mut spec.command);
        spec.command = command;
        println!("{}", text::daemon_session_starts(&name, &item, &session));
        out.say(Say::Started {
            name: name.clone(),
            socket: owned.socket.clone(),
        });
        let running = match self.runtime.start(&spec, &values).await {
            Ok(running) => running,
            Err(e) => {
                self.close(&owned);
                println!("{}", text::daemon_session_failed(&name));
                out.say(Say::Failed {
                    error: format!("{e:#}"),
                });
                return;
            }
        };
        // The output of the container: the notes of the relay and the
        // stderr of claude.
        for output in [running.events, running.errors] {
            let (scrub, owned) = (scrub.clone(), owned.clone());
            tokio::spawn(async move {
                let mut lines = BufReader::new(output).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let line = scrub.clean(&line).into_owned();
                    Owned::tell(&mut owned.lock(), &Say::Stderr { line });
                }
            });
        }
        let reader = stop_on(from, owned.clone());
        // The input of the container stays open until the end: the relay
        // gives claude its input.
        let input = running.input;
        self.clone().watch(owned, notes, Some(running.end)).await;
        drop(input);
        reader.abort();
    }

    /// Follows the live session `name` on `out`: each event after
    /// `after`, then as [`Daemon::start`]. A [`Then::Stop`] on `from`
    /// stops it.
    fn follow(&self, name: &str, after: u64, from: Lines<BufReader<OwnedReadHalf>>, out: &Out) {
        let Some(owned) = self.lock().get(name).cloned() else {
            out.say(Say::Failed {
                error: text::daemon_no_session(name),
            });
            return;
        };
        if let Err(e) = owned.follow(out.clone(), after) {
            out.say(Say::Failed {
                error: format!("{e:#}"),
            });
            return;
        }
        // The reader ends with the connection.
        stop_on(from, owned);
    }

    /// Waits for the end of the session `owned`: the exit of its relay,
    /// the end of its container, or, with no `end` of this daemon, the
    /// end of the relay in a container that does not run.
    async fn watch(
        self: Arc<Self>,
        owned: Arc<Owned>,
        mut notes: mpsc::UnboundedReceiver<Note>,
        end: Option<End>,
    ) {
        let mut end = end.map(|end| Box::pin(end.wait()));
        let mut stopped = false;
        let (code, exited) = loop {
            tokio::select! {
                note = notes.recv() => match note {
                    Some(Note::Exit(code)) => break (code, true),
                    Some(Note::Gone) if end.is_none() => {
                        tokio::time::sleep(GONE_WAIT).await;
                        let back = owned.lock().relay.is_some();
                        if !back && !self.runtime.running(&owned.name).await.unwrap_or(true) {
                            break (None, false);
                        }
                    }
                    Some(Note::Gone) => {}
                    None => break (None, false),
                },
                // A session that this daemon took up has no end of its
                // own. Its relay can have given up before the daemon
                // came back: then no relay connects.
                () = tokio::time::sleep(LOOK_EVERY), if end.is_none() => {
                    let back = owned.lock().relay.is_some();
                    if !back && !self.runtime.running(&owned.name).await.unwrap_or(true) {
                        break (None, false);
                    }
                }
                exit = async { end.as_mut().expect("an end").await }, if end.is_some() => {
                    // The container ended. Its relay may have sent its
                    // exit just before: take it.
                    let code = exit.ok().and_then(|e| e.code);
                    let exit = tokio::time::timeout(GONE_WAIT, async {
                        loop {
                            match notes.recv().await {
                                Some(Note::Exit(code)) => return Some(code),
                                Some(Note::Gone) => {}
                                None => return None,
                            }
                        }
                    });
                    break match exit.await {
                        Ok(Some(code)) => (code, true),
                        _ => (code, false),
                    };
                }
                () = owned.stop.notified(), if !stopped => {
                    stopped = true;
                    println!("{}", text::daemon_stops(&owned.name));
                    let _ = self.runtime.stop(&owned.name).await;
                }
            }
        };
        let failed = owned.lock().record.failed || stopped || !exited;
        let ended = Ended { code, failed };
        println!("{}", text::daemon_session_ends(&owned.name, code));
        let followers = self.close(&owned);
        for out in followers {
            out.say(Say::Ended(ended));
        }
    }

    /// Ends the ownership of the session `owned`: its socket, its folder
    /// and its record go. It gives the connections that followed it.
    fn close(&self, owned: &Owned) -> Vec<Out> {
        if let Some(door) = owned
            .door
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            door.abort();
        }
        let _ = std::fs::remove_dir_all(&owned.socket);
        let _ = std::fs::remove_file(&owned.record_path);
        self.lock().remove(&owned.name);
        std::mem::take(&mut owned.lock().followers)
    }

    /// Makes the folder of the socket of the session `owned`, and
    /// answers each connection on its socket. With `again`, the folder
    /// stays: the container sees it. The task ends at its abort, with
    /// each of its connections.
    fn door(self: &Arc<Self>, owned: &Arc<Owned>, again: bool) -> Result<JoinHandle<()>> {
        let dir = &owned.socket;
        if again {
            let _ = std::fs::remove_file(dir.join(SOCKET));
            private_dir(dir)?;
        } else {
            let _ = std::fs::remove_dir_all(dir);
            private_dir(dir)?;
        }
        let listener = bind_in(dir, SOCKET)?;
        let (daemon, owned) = (self.clone(), owned.clone());
        Ok(tokio::spawn(async move {
            let mut connections = JoinSet::new();
            while let Ok((conn, _)) = listener.accept().await {
                while connections.try_join_next().is_some() {}
                connections.spawn(daemon.clone().answer_session(conn, owned.clone()));
            }
        }))
    }

    /// Answers one connection on the socket of the session `owned`.
    async fn answer_session(self: Arc<Self>, conn: UnixStream, owned: Arc<Owned>) {
        let (read, write) = conn.into_split();
        let mut lines = BufReader::new(read).lines();
        let Ok(Some(first)) = lines.next_line().await else {
            return;
        };
        match session_hello(&first) {
            Ok(SessionAsk::Relay { inputs, closed, .. }) => {
                owned.relay(inputs, closed, lines, write).await;
            }
            Ok(SessionAsk::Mcp { client, .. }) => self.mcp(&owned, client, lines, write).await,
            Ok(SessionAsk::Hello { .. }) => {
                let (out, written) = writer(write);
                let record = owned.lock().record.live.clone();
                let who = Say::Session {
                    session: record.session,
                    item: record.item,
                    name: record.name,
                };
                out.say(who.clone());
                while let Ok(Some(line)) = lines.next_line().await {
                    out.say(match session_hello(&line) {
                        Ok(SessionAsk::Hello { .. }) => who.clone(),
                        Ok(_) => Say::Failed {
                            error: text::DAEMON_NO_HELLO.into(),
                        },
                        Err(error) => Say::Failed { error },
                    });
                }
                drop(out);
                let _ = written.await;
            }
            Err(error) => {
                let (out, written) = writer(write);
                out.say(Say::Failed { error });
                drop(out);
                let _ = written.await;
            }
        }
    }

    /// The riff tools of the session of `record`: they act on its
    /// riff-server as that session, with the sign-in of the person on
    /// this machine (01M4ZHSZV46RF3XDXDTQXWGXRG). That session is the
    /// worker session of `riff run`, which holds the claim of the item.
    /// The session gets only the tools that act on riff-server, and on
    /// no folder of the host and no claim ([`crate::mcp::CONTAINER_TOOLS`]).
    /// From #719, the tools act on the state of the daemon
    /// (01M4ZKX0H9N82ZBC9PKSPTQV4N).
    pub fn tools_of(record: &Record) -> Result<crate::mcp::Tools> {
        let api = crate::api::Api::new(&record.server).signed_in(Some(&record.live.session))?;
        Ok(crate::mcp::Tools::new(api, record.me.clone())
            .as_worker(true)
            .in_container())
    }

    /// Serves the riff tools of the session `owned` on one connection.
    /// With `client`, the initialize request of `claude`, it skips the
    /// handshake.
    async fn mcp(
        &self,
        owned: &Owned,
        client: Option<Value>,
        lines: Lines<BufReader<OwnedReadHalf>>,
        write: OwnedWriteHalf,
    ) {
        use rmcp::ServiceExt;
        let record = owned.lock().record.clone();
        let tools = match Self::tools_of(&record) {
            Ok(tools) => tools,
            Err(e) => {
                eprintln!("riff daemon: {}: {e:#}", record.live.name);
                return;
            }
        };
        // The reader may hold the first lines of the client already.
        let stream = tokio::io::join(lines.into_inner(), write);
        let client = client.and_then(|c| serde_json::from_value(c).ok());
        let served = match client {
            Some(client) => Ok(rmcp::service::serve_directly(tools, stream, Some(client))),
            None => tools.serve(stream).await,
        };
        if let Ok(service) = served {
            let _ = service.waiting().await;
        }
    }

    /// Takes up each session of a record in the folder of the daemon
    /// whose container runs, or when podman cannot say, and removes each
    /// other record
    /// (01M4ZHT01HH88KPA8628M7VBEY). Call it after [`listen`], before
    /// [`Daemon::serve`].
    pub async fn recover(self: &Arc<Self>) {
        let Ok(entries) = std::fs::read_dir(self.dir.join(LIVE)) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        paths.sort();
        for path in paths {
            let record = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<Record>(&text).ok());
            let Some(record) = record else {
                let _ = std::fs::remove_file(&path);
                continue;
            };
            let name = record.live.name.clone();
            // When podman cannot say, the daemon takes up the session,
            // as its watch does.
            if !self.runtime.running(&name).await.unwrap_or(true) {
                let _ = std::fs::remove_file(&path);
                let _ = std::fs::remove_dir_all(session_dir(&self.dir, &name));
                println!("{}", text::daemon_session_gone(&name));
                continue;
            }
            match self.own(record, Scrub::default(), true) {
                Ok((owned, notes)) => {
                    println!("{}", text::daemon_takes_up(&name));
                    tokio::spawn(self.clone().watch(owned, notes, None));
                }
                Err(e) => eprintln!("riff daemon: {name}: {e:#}"),
            }
        }
    }
}

/// Reads each line of `from` until its end: a [`Then::Stop`] asks to
/// stop the session `owned`.
fn stop_on(mut from: Lines<BufReader<OwnedReadHalf>>, owned: Arc<Owned>) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Ok(Some(line)) = from.next_line().await {
            if matches!(serde_json::from_str::<Then>(&line), Ok(Then::Stop)) {
                owned.stop.notify_one();
            }
        }
    })
}

/// Reads the [`Hello`] of the line `first`: an error when it is of
/// another version, or not a hello. The error shows no part of the
/// line: it can hold a secret.
fn hello(first: &str) -> Result<Ask, String> {
    #[derive(Deserialize)]
    struct Version {
        protocol: u32,
    }
    match serde_json::from_str::<Version>(first) {
        Ok(Version { protocol }) if protocol != PROTOCOL => {
            return Err(text::daemon_protocol(protocol, PROTOCOL));
        }
        Ok(_) => {}
        Err(_) => return Err(text::DAEMON_NO_HELLO.into()),
    }
    serde_json::from_str::<Hello>(first)
        .map(|hello| hello.ask)
        .map_err(|_| text::DAEMON_NO_HELLO.into())
}

/// Reads the [`SessionAsk`] of the line `first` on the socket of a
/// session: an error when its version is not [`compatible`], or when it
/// is not an ask. The error shows no part of the line.
fn session_hello(first: &str) -> Result<SessionAsk, String> {
    #[derive(Deserialize)]
    struct Version {
        protocol: u32,
    }
    match serde_json::from_str::<Version>(first) {
        Ok(Version { protocol }) if !compatible(protocol) => {
            return Err(text::daemon_protocol(protocol, PROTOCOL));
        }
        Ok(_) => {}
        Err(_) => return Err(text::DAEMON_NO_HELLO.into()),
    }
    serde_json::from_str::<SessionAsk>(first).map_err(|_| text::DAEMON_NO_HELLO.into())
}

/// A connection of `riff` to the daemon.
pub struct Client {
    lines: Lines<BufReader<OwnedReadHalf>>,
    write: OwnedWriteHalf,
}

impl Client {
    /// Connects to the daemon in the folder `dir`. With no daemon, the
    /// error says how to start it.
    pub async fn connect(dir: &Path) -> Result<Client> {
        let conn = connect_in(dir, SOCKET)
            .await
            .map_err(|_| anyhow!("{}", text::daemon_none(&socket(dir))))?;
        let (read, write) = conn.into_split();
        Ok(Client {
            lines: BufReader::new(read).lines(),
            write,
        })
    }

    async fn line(&mut self, value: &impl Serialize) -> Result<()> {
        let mut line = serde_json::to_string(value)?;
        line.push('\n');
        self.write.write_all(line.as_bytes()).await?;
        Ok(())
    }

    /// Sends the ask `ask`: the first line of the connection.
    pub async fn ask(&mut self, ask: Ask) -> Result<()> {
        self.line(&Hello {
            protocol: PROTOCOL,
            ask,
        })
        .await
    }

    /// Asks the daemon to stop the session of this connection.
    pub async fn stop(&mut self) -> Result<()> {
        self.line(&Then::Stop).await
    }

    /// The next line of the daemon, or `None` when the daemon closed
    /// the connection.
    pub async fn next(&mut self) -> Result<Option<Say>> {
        match self.lines.next_line().await? {
            Some(line) => Ok(Some(
                serde_json::from_str(&line)
                    .context("the riff daemon said a line that riff cannot read")?,
            )),
            None => Ok(None),
        }
    }
}

/// The state of the daemon in the folder `dir`.
pub async fn status(dir: &Path) -> Result<Status> {
    let mut client = Client::connect(dir).await?;
    client.ask(Ask::Status).await?;
    match client.next().await? {
        Some(Say::Status(status)) => Ok(status),
        Some(Say::Failed { error }) => bail!("{error}"),
        _ => bail!("{}", text::DAEMON_GONE),
    }
}

/// Asks the daemon in the folder `dir` to have the image `image`. It
/// shows each line of the daemon with `show`.
pub async fn image(dir: &Path, image: &str, mut show: impl FnMut(&str)) -> Result<()> {
    let mut client = Client::connect(dir).await?;
    client
        .ask(Ask::Image {
            image: image.into(),
        })
        .await?;
    loop {
        match client.next().await? {
            Some(Say::Line { text }) => show(&text),
            Some(Say::Done) => return Ok(()),
            Some(Say::Failed { error }) => bail!("{error}"),
            _ => bail!("{}", text::DAEMON_GONE),
        }
    }
}

/// `riff daemon events NAME`: shows each event of the live session in
/// the container `name` from its start, as `riff run` does, and each new
/// one until the end of the session (01M4ZKX0KMS58NR2E9AF8229A8). A
/// Ctrl-C ends the view: the session goes on. It gives the exit code: 1
/// when the session failed.
pub async fn show_events(dir: &Path, name: &str) -> Result<i32> {
    let mut client = Client::connect(dir).await?;
    client
        .ask(Ask::Follow {
            name: name.into(),
            after: 0,
        })
        .await?;
    loop {
        match client.next().await? {
            Some(Say::Event { line, .. }) => crate::run::show(&line),
            Some(Say::Stderr { line }) => eprintln!("{line}"),
            Some(Say::Ended(ended)) => {
                println!("{}", text::daemon_session_ends(name, ended.code));
                return Ok(i32::from(ended.failed));
            }
            Some(Say::Failed { error }) => bail!("{error}"),
            Some(_) => {}
            None => bail!("{}", text::DAEMON_GONE),
        }
    }
}

/// `riff daemon`: runs the daemon of this machine until a SIGTERM or a
/// SIGINT. It takes up each session that runs ([`Daemon::recover`]).
/// The stop stops no session (01M4ZDTVH57VE77YT20G65A4BQ).
pub async fn serve_here() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let dir = dir()?;
        let listener = listen(&dir).await?;
        let mut term = signal(SignalKind::terminate())?;
        let mut int = signal(SignalKind::interrupt())?;
        let daemon = Arc::new(Daemon::open(crate::podman::Podman, dir.clone()).await?);
        let status = daemon.status().await;
        println!(
            "{}",
            text::daemon_listens(&socket(&dir), status.pid, &status.build)
        );
        daemon.recover().await;
        let stop = async move {
            tokio::select! {
                _ = term.recv() => {}
                _ = int.recv() => {}
            }
        };
        daemon.serve(listener, stop).await?;
        let _ = std::fs::remove_file(socket(&dir));
        println!("{}", text::DAEMON_STOPS);
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        bail!("{}", text::DAEMON_LINUX_ONLY)
    }
}

/// `riff daemon status`: shows the daemon in the folder `dir` and its
/// sessions, or, with no daemon, the state of its unit. It gives the
/// exit code: 1 when no daemon answers (01M4ZDTVNBKDM64043GGWQEV4S).
pub async fn show_status(dir: &Path, manager: &impl crate::service::ServiceManager) -> i32 {
    match status(dir).await {
        Ok(status) => {
            print!("{}", text::daemon_status(&status, now_ms()));
            0
        }
        Err(e) => {
            println!("{e:#}");
            match manager.state() {
                Ok(state) => println!("{}", text::daemon_unit_state(&state)),
                Err(e) => println!("{e:#}"),
            }
            1
        }
    }
}

/// `riff daemon install`: installs the unit of the daemon with
/// `manager`, starts it again, and waits up to `wait` for the daemon in
/// the folder `dir` to answer (01M4ZDTVK74596M0ZA46N51NKV).
pub async fn install(
    manager: &impl crate::service::ServiceManager,
    unit: &crate::service::Unit,
    dir: &Path,
    wait: Duration,
) -> Result<()> {
    if crate::home::dir().is_some() {
        bail!("{}", text::DAEMON_INSTALL_RIFF_HOME);
    }
    let path = manager.install(unit)?;
    println!("{}", text::daemon_installed(&path));
    let start = std::time::Instant::now();
    loop {
        if let Ok(status) = status(dir).await {
            println!("{}", text::daemon_runs(&status));
            return Ok(());
        }
        if start.elapsed() >= wait {
            let state = manager.state().unwrap_or_else(|e| format!("{e:#}"));
            bail!("{}", text::daemon_no_answer(&state, &socket(dir)));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::HIDDEN;
    use crate::runtime::{Exit, Fake};
    use tokio::sync::oneshot;

    const IMAGE: &str = "localhost/riff-session:latest";

    /// The events of one turn of a fake `claude`, with a token in a
    /// tool result.
    const TURN: [&str; 3] = [
        r#"{"type":"system","subtype":"init","model":"m","permissionMode":"auto"}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true,"content":"gho_secret1"}]}}"#,
        r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1}"#,
    ];

    fn me(name: &str) -> SessionUri {
        format!("riff://mike@thelio/como-technologies/riff?session={name}-session#issue-1")
            .parse()
            .unwrap()
    }

    fn spec(name: &str, clone: &Path) -> SessionSpec {
        SessionSpec {
            name: name.into(),
            image: IMAGE.into(),
            home: "/home/mike".into(),
            workdir: clone.to_owned(),
            binds: vec![Bind {
                source: Source::Path(clone.to_owned()),
                target: clone.to_owned(),
                write: true,
            }],
            env: vec![Env::Host("GH_TOKEN".into())],
            memory: "8g".into(),
            cpus: 4,
            command: vec!["claude".into()],
        }
    }

    /// A daemon on `fake` in a temp folder, with a session folder for
    /// each session.
    struct Here {
        root: tempfile::TempDir,
        dir: PathBuf,
        fake: Fake,
        daemon: Arc<Daemon<Fake>>,
        stop: Option<oneshot::Sender<()>>,
        served: JoinHandle<Result<()>>,
    }

    /// A daemon on `fake` in the folder `dir`, which serves until its
    /// stop.
    type Served = (
        Arc<Daemon<Fake>>,
        oneshot::Sender<()>,
        JoinHandle<Result<()>>,
    );

    async fn serve(fake: &Fake, dir: &Path) -> Served {
        let listener = listen(dir).await.unwrap();
        let daemon = Arc::new(Daemon::open(fake.clone(), dir.to_owned()).await.unwrap());
        let (stop, stopped) = oneshot::channel::<()>();
        let served = tokio::spawn(daemon.clone().serve(listener, async {
            let _ = stopped.await;
        }));
        (daemon, stop, served)
    }

    impl Here {
        async fn new(fake: Fake) -> Here {
            let root = tempfile::tempdir().unwrap();
            let dir = dir_in(root.path());
            let (daemon, stop, served) = serve(&fake, &dir).await;
            Here {
                root,
                dir,
                fake,
                daemon,
                stop: Some(stop),
                served,
            }
        }

        /// Stops the daemon, and starts a new one in the same folder.
        async fn restart(&mut self) {
            self.stop().await;
            let (daemon, stop, served) = serve(&self.fake, &self.dir).await;
            self.daemon = daemon;
            self.stop = Some(stop);
            self.served = served;
        }

        /// The [`Start`] of the session `name` of `item`.
        fn start(&self, name: &str, item: &str) -> Start {
            let folder = self.root.path().join("sessions").join(name);
            let clone = folder.join("clone");
            std::fs::create_dir_all(&clone).unwrap();
            Start {
                session: format!("{name}-session"),
                item: item.into(),
                folder,
                spec: spec(name, &clone),
                start: "Do it.".into(),
                values: vec![("GH_TOKEN".into(), "gho_secret1".into())],
                secrets: vec!["gho_secret1".into()],
                server: "http://127.0.0.1:1".into(),
                me: me(name),
            }
        }

        /// Asks the daemon to start `start`. Gives the client after the
        /// line `Started`.
        async fn started(&self, start: Start) -> (Client, String) {
            let mut client = Client::connect(&self.dir).await.unwrap();
            client.ask(Ask::Start(Box::new(start))).await.unwrap();
            match client.next().await.unwrap() {
                Some(Say::Started { name, socket }) => {
                    assert_eq!(socket, session_dir(&self.dir, &name));
                    (client, name)
                }
                other => panic!("{other:?}"),
            }
        }

        async fn stop(&mut self) {
            let _ = self.stop.take().unwrap().send(());
            (&mut self.served).await.unwrap().unwrap();
        }
    }

    /// The relay of a session, as the test plays it.
    struct Relay {
        lines: Lines<BufReader<OwnedReadHalf>>,
        write: OwnedWriteHalf,
    }

    impl Relay {
        /// Connects to the socket of the session in the folder `dir` as
        /// its relay, and gives the resume.
        async fn connect(dir: &Path, inputs: u64, closed: bool) -> (Relay, u64) {
            let conn = connect_in(dir, SOCKET).await.unwrap();
            let (read, write) = conn.into_split();
            let mut relay = Relay {
                lines: BufReader::new(read).lines(),
                write,
            };
            relay
                .send(&SessionAsk::Relay {
                    protocol: PROTOCOL,
                    inputs,
                    closed,
                })
                .await;
            match relay.next().await {
                Say::Resume { events } => (relay, events),
                other => panic!("{other:?}"),
            }
        }

        async fn send(&mut self, value: &impl Serialize) {
            let line = serde_json::to_string(value).unwrap();
            self.write
                .write_all(format!("{line}\n").as_bytes())
                .await
                .unwrap();
        }

        async fn next(&mut self) -> Say {
            let line = self.lines.next_line().await.unwrap().unwrap();
            serde_json::from_str(&line).unwrap()
        }

        /// Sends the event `seq`, and waits for its ack.
        async fn event(&mut self, seq: u64, line: &str) {
            self.send(&Up::Event {
                seq,
                line: line.into(),
            })
            .await;
            loop {
                match self.next().await {
                    Say::Ack { seq: acked } if acked >= seq => return,
                    Say::Ack { .. } | Say::Close => {}
                    other => panic!("{other:?}"),
                }
            }
        }

        /// Sends the exit, and waits for its ack.
        async fn exit(&mut self, seq: u64, code: i32) {
            self.send(&Up::Exit {
                seq,
                code: Some(code),
            })
            .await;
            loop {
                match self.next().await {
                    Say::Ack { seq: acked } if acked == seq => return,
                    Say::Close | Say::Ack { .. } => {}
                    other => panic!("{other:?}"),
                }
            }
        }
    }

    /// Each line of `client` up to the end of the session.
    async fn to_the_end(client: &mut Client) -> (Vec<Say>, Option<Say>) {
        let mut said = Vec::new();
        while let Some(say) = client.next().await.unwrap() {
            match say {
                Say::Ended(_) | Say::Failed { .. } => return (said, Some(say)),
                say => said.push(say),
            }
        }
        (said, None)
    }

    /// Asks the socket of a session in the folder `dir` who it is, with
    /// the version `protocol`.
    async fn who_in(dir: &Path, protocol: u32) -> Say {
        let conn = connect_in(dir, SOCKET).await.unwrap();
        let (read, mut write) = conn.into_split();
        let hello = serde_json::to_string(&SessionAsk::Hello { protocol }).unwrap();
        write
            .write_all(format!("{hello}\n").as_bytes())
            .await
            .unwrap();
        let line = BufReader::new(read)
            .lines()
            .next_line()
            .await
            .unwrap()
            .unwrap();
        serde_json::from_str(&line).unwrap()
    }

    /// The events of `said`.
    fn events(said: &[Say]) -> Vec<(u64, String)> {
        said.iter()
            .filter_map(|s| match s {
                Say::Event { seq, line } => Some((*seq, line.clone())),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn a_session_runs_through_its_relay_and_ends_at_the_exit_of_the_relay() {
        let here = Here::new(Fake::default().with_image(IMAGE).with_events(["a note"])).await;
        let start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        let log = start.folder.join(EVENTS);
        let (mut client, name) = here.started(start).await;
        let socket = session_dir(&here.dir, &name);

        // The relay gets the start text, and the close after the result.
        let (mut relay, had) = Relay::connect(&socket, 0, false).await;
        assert_eq!(had, 0);
        assert_eq!(
            relay.next().await,
            Say::Input {
                line: user_line("Do it.")
            }
        );
        for (seq, line) in (1..).zip(TURN) {
            relay.event(seq, line).await;
        }
        assert!(
            serde_json::from_str::<Record>(
                &std::fs::read_to_string(record_path(&here.dir, &name)).unwrap()
            )
            .unwrap()
            .closed
        );
        relay.exit(4, 0).await;

        let (said, end) = to_the_end(&mut client).await;
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: false
            }))
        );
        // Each event, with no secret and its number, to riff and to the
        // log. The output of the container is a line of stderr.
        let lines = events(&said);
        assert_eq!(lines.len(), 3, "{said:?}");
        assert_eq!(lines.iter().map(|l| l.0).collect::<Vec<_>>(), [1, 2, 3]);
        assert!(lines[1].1.contains(HIDDEN) && !lines[1].1.contains("gho_secret1"));
        assert!(said.contains(&Say::Stderr {
            line: "a note".into()
        }));
        let logged: Vec<String> = lines.into_iter().map(|l| l.1).collect();
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            logged.join("\n") + "\n"
        );
        // The relay is the entrypoint: the binary of the daemon, with no
        // write, and the command of the spec after it.
        let spec = here.fake.spec(&name).unwrap();
        assert_eq!(spec.command, ["/opt/riff/riff", "relay", "--", "claude"]);
        assert!(spec.binds.contains(&Bind {
            source: Source::Path(own_binary().unwrap()),
            target: RELAY_BIN.into(),
            write: false,
        }));
        // The token goes to the runtime beside the spec.
        assert_eq!(
            here.fake.values(&name),
            [("GH_TOKEN".to_owned(), "gho_secret1".to_owned())]
        );
        // The daemon owns the session no more: its socket and its record
        // are gone, and the input of the container closed.
        assert!(status(&here.dir).await.unwrap().sessions.is_empty());
        assert!(!socket.exists());
        assert!(!record_path(&here.dir, &name).exists());
        assert_eq!(here.fake.exit(&name), Some(Exit::SUCCESS));
    }

    #[tokio::test]
    async fn a_relay_that_connects_again_gets_no_input_twice_and_resends_what_the_daemon_lacks() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        let log = start.folder.join(EVENTS);
        let (mut client, name) = here.started(start).await;
        let socket = session_dir(&here.dir, &name);
        let (mut relay, _) = Relay::connect(&socket, 0, false).await;
        assert!(matches!(relay.next().await, Say::Input { .. }));
        relay.event(1, TURN[0]).await;
        drop(relay);

        // The relay took the input, and connects again: the daemon has
        // one event. An event that it has already gets no second line.
        let (mut relay, had) = Relay::connect(&socket, 1, false).await;
        assert_eq!(had, 1);
        relay.event(1, TURN[0]).await;
        relay.event(2, TURN[1]).await;
        relay.event(3, TURN[2]).await;
        drop(relay);
        // After the result, a relay that did not close gets the close.
        let (mut relay, had) = Relay::connect(&socket, 1, false).await;
        assert_eq!(had, 3);
        assert_eq!(relay.next().await, Say::Close);
        relay.exit(4, 0).await;

        let (said, _) = to_the_end(&mut client).await;
        assert_eq!(
            events(&said).iter().map(|l| l.0).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 3);
    }

    #[tokio::test]
    async fn a_follower_gets_each_event_after_its_number_and_the_end() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        drop(client);
        let socket = session_dir(&here.dir, &name);
        let (mut relay, _) = Relay::connect(&socket, 1, false).await;
        relay.event(1, TURN[0]).await;
        relay.event(2, TURN[1]).await;

        let mut follower = Client::connect(&here.dir).await.unwrap();
        follower
            .ask(Ask::Follow {
                name: name.clone(),
                after: 1,
            })
            .await
            .unwrap();
        match follower.next().await.unwrap() {
            Some(Say::Event { seq: 2, line }) => assert!(line.contains(HIDDEN)),
            other => panic!("{other:?}"),
        }
        relay.event(3, TURN[2]).await;
        relay.exit(4, 0).await;
        let (said, end) = to_the_end(&mut follower).await;
        assert_eq!(events(&said).iter().map(|l| l.0).collect::<Vec<_>>(), [3]);
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: false
            }))
        );

        // A session that the daemon does not own.
        let mut follower = Client::connect(&here.dir).await.unwrap();
        follower
            .ask(Ask::Follow {
                name: "riff-issue-9-00000000".into(),
                after: 0,
            })
            .await
            .unwrap();
        assert!(matches!(
            follower.next().await.unwrap(),
            Some(Say::Failed { .. })
        ));
    }

    #[tokio::test]
    async fn a_turn_that_ends_with_an_error_fails() {
        let error = r#"{"type":"result","subtype":"error_during_execution","is_error":true}"#;
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (mut client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let (mut relay, _) = Relay::connect(&session_dir(&here.dir, &name), 1, false).await;
        relay.event(1, error).await;
        relay.exit(2, 0).await;
        let (_, end) = to_the_end(&mut client).await;
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: true
            }))
        );
    }

    #[tokio::test]
    async fn a_session_with_no_image_on_the_machine_does_not_start() {
        let here = Here::new(Fake::default()).await;
        let (mut client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        match to_the_end(&mut client).await {
            (_, Some(Say::Failed { error })) => assert!(error.contains("no image"), "{error}"),
            other => panic!("{other:?}"),
        }
        // The daemon owns no session, and the socket folder is gone.
        assert!(status(&here.dir).await.unwrap().sessions.is_empty());
        assert!(!session_dir(&here.dir, &name).exists());
        assert!(!record_path(&here.dir, &name).exists());
    }

    #[tokio::test]
    async fn each_session_gets_its_own_socket_and_no_other() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (mut one, a) = here
            .started(here.start("riff-issue-1-aaaaaaaa", "issue-1"))
            .await;
        let (mut two, b) = here
            .started(here.start("riff-issue-2-bbbbbbbb", "issue-2"))
            .await;

        for (name, other, item) in [(&a, &b, "issue-1"), (&b, &a, "issue-2")] {
            let spec = here.fake.spec(name).unwrap();
            let own = session_dir(&here.dir, name);
            // Its own socket folder, at /run/riff, with write.
            assert!(spec.binds.contains(&Bind {
                source: Source::Path(own.clone()),
                target: IN_SESSION.into(),
                write: true,
            }));
            assert!(
                spec.env
                    .contains(&Env::Value(SOCKET_VAR.into(), "/run/riff/riff.sock".into()))
            );
            // No bind shows the folder of the daemon, other than its own
            // socket folder.
            for bind in &spec.binds {
                if let Source::Path(source) = &bind.source
                    && source != &own
                {
                    assert!(!source.starts_with(&here.dir) && !here.dir.starts_with(source));
                }
                if let Source::Path(source) = &bind.source {
                    assert!(!source.starts_with(session_dir(&here.dir, other)));
                }
            }
            // Its socket tells the daemon which session asks, also to a
            // session of the protocol before.
            let who = Say::Session {
                session: format!("{name}-session"),
                item: item.into(),
                name: name.clone(),
            };
            assert_eq!(who_in(&own, PROTOCOL).await, who);
            assert_eq!(who_in(&own, PROTOCOL - 1).await, who);
            assert!(matches!(
                who_in(&own, PROTOCOL + 2).await,
                Say::Failed { .. }
            ));
        }
        let mode = |p: &Path| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(p).unwrap().permissions().mode() & 0o777
        };
        assert_eq!(mode(&here.dir), 0o700);
        assert_eq!(mode(&here.dir.join(LIVE)), 0o700);
        assert_eq!(mode(&session_dir(&here.dir, &a)), 0o700);
        assert_eq!(mode(&session_dir(&here.dir, &a).join(SOCKET)), 0o600);
        let names: Vec<String> = status(&here.dir)
            .await
            .unwrap()
            .sessions
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, [a.clone(), b.clone()]);

        // A stop: the runtime stops the container, and its end comes
        // with no exit of the relay.
        for client in [&mut one, &mut two] {
            client.stop().await.unwrap();
            let (_, end) = to_the_end(client).await;
            assert_eq!(
                end,
                Some(Say::Ended(Ended {
                    code: None,
                    failed: true
                }))
            );
        }
        assert_eq!(
            here.fake.calls(),
            [
                format!("start {a}"),
                format!("start {b}"),
                format!("stop {a}"),
                format!("stop {b}")
            ]
        );
    }

    #[tokio::test]
    async fn the_daemon_starts_no_session_whose_bind_shows_its_folder() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let data = here.root.path().to_owned();
        let shows = [
            data.clone(),
            here.dir.clone(),
            here.dir.join("sessions"),
            session_dir(&here.dir, "riff-issue-1-aaaaaaaa"),
        ];
        for source in shows {
            let mut start = here.start("riff-issue-2-bbbbbbbb", "issue-2");
            start.spec.binds.push(Bind {
                source: Source::Path(source.clone()),
                target: "/data".into(),
                write: false,
            });
            let mut client = Client::connect(&here.dir).await.unwrap();
            client.ask(Ask::Start(Box::new(start))).await.unwrap();
            match client.next().await.unwrap() {
                Some(Say::Failed { error }) => {
                    assert!(error.contains("the folder of the riff daemon"), "{error}");
                }
                other => panic!("{source:?}: {other:?}"),
            }
        }
        // A bind at the place of the socket or of the relay in the
        // session.
        for target in ["/run/riff", RELAY_BIN] {
            let mut start = here.start("riff-issue-2-bbbbbbbb", "issue-2");
            start.spec.binds[0].target = target.into();
            let mut client = Client::connect(&here.dir).await.unwrap();
            client.ask(Ask::Start(Box::new(start))).await.unwrap();
            assert!(matches!(
                client.next().await.unwrap(),
                Some(Say::Failed { .. })
            ));
        }
        assert!(here.fake.calls().is_empty(), "{:?}", here.fake.calls());
    }

    #[tokio::test]
    async fn a_stop_of_the_daemon_stops_no_session() {
        let mut here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (_client, name) = here
            .started(here.start("riff-issue-1-aaaaaaaa", "issue-1"))
            .await;
        here.stop().await;
        // No daemon answers, and the daemon stopped no container. A
        // connect alone can still work: a child that another test forks
        // holds the listener until its exec.
        assert!(status(&here.dir).await.is_err());
        assert_eq!(here.fake.calls(), [format!("start {name}")]);
        assert_eq!(here.fake.exit(&name), None);
        // The record stays for the next daemon.
        assert!(record_path(&here.dir, &name).exists());
    }

    #[tokio::test]
    async fn a_new_daemon_takes_up_a_session_that_runs_and_drops_one_that_ended() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let fake = Fake::default().with_image(IMAGE);
        let run = |start: &Start| {
            let mut spec = start.spec.clone();
            spec.env.clear();
            spec
        };
        // Two records of an old daemon: one container runs, one ended.
        let folder = root.path().join("sessions/issue-1");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(EVENTS), format!("{}\n{}\n", TURN[0], TURN[1])).unwrap();
        private_dir(&dir.join(LIVE)).unwrap();
        for (name, item) in [
            ("riff-issue-1-aaaaaaaa", "issue-1"),
            ("riff-issue-2-bbbbbbbb", "issue-2"),
        ] {
            let start = Start {
                session: format!("{name}-session"),
                item: item.into(),
                folder: folder.clone(),
                spec: spec(name, &folder),
                start: "Do it.".into(),
                values: vec![],
                secrets: vec![],
                server: "http://127.0.0.1:1".into(),
                me: me(name),
            };
            let record = Record {
                live: Live {
                    name: name.into(),
                    session: start.session.clone(),
                    item: item.into(),
                    image: IMAGE.into(),
                    since_ms: 0,
                },
                folder: folder.clone(),
                server: start.server.clone(),
                me: start.me.clone(),
                inputs: vec![user_line("Do it.")],
                closed: false,
                failed: false,
            };
            std::fs::write(
                record_path(&dir, name),
                serde_json::to_string(&record).unwrap(),
            )
            .unwrap();
            private_dir(&session_dir(&dir, name)).unwrap();
            if name.contains("issue-1") {
                let _running = fake.start(&run(&start), &[]).await.unwrap();
                std::mem::forget(_running);
            }
        }

        let listener = listen(&dir).await.unwrap();
        let daemon = Arc::new(Daemon::open(fake.clone(), dir.clone()).await.unwrap());
        daemon.recover().await;
        tokio::spawn(daemon.serve(listener, std::future::pending()));
        let names: Vec<String> = status(&dir)
            .await
            .unwrap()
            .sessions
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, ["riff-issue-1-aaaaaaaa"]);
        assert!(!record_path(&dir, "riff-issue-2-bbbbbbbb").exists());
        assert!(!session_dir(&dir, "riff-issue-2-bbbbbbbb").exists());

        // The relay connects again: the daemon has two events, and gives
        // the input that the relay did not take.
        let socket = session_dir(&dir, "riff-issue-1-aaaaaaaa");
        let (mut relay, had) = Relay::connect(&socket, 0, false).await;
        assert_eq!(had, 2);
        assert!(matches!(relay.next().await, Say::Input { .. }));
        let mut follower = Client::connect(&dir).await.unwrap();
        follower
            .ask(Ask::Follow {
                name: "riff-issue-1-aaaaaaaa".into(),
                after: 0,
            })
            .await
            .unwrap();
        relay.event(3, TURN[2]).await;
        relay.exit(4, 0).await;
        let (said, end) = to_the_end(&mut follower).await;
        assert_eq!(
            events(&said).iter().map(|l| l.0).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: false
            }))
        );
        assert_eq!(
            std::fs::read_to_string(folder.join(EVENTS))
                .unwrap()
                .lines()
                .count(),
            3
        );
        assert!(!record_path(&dir, "riff-issue-1-aaaaaaaa").exists());
    }

    #[tokio::test]
    async fn a_taken_up_session_whose_relay_and_container_end_ends() {
        assert_eq!(
            take_up_and_end(true).await,
            Some(Say::Ended(Ended {
                code: None,
                failed: true
            }))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_taken_up_session_whose_relay_never_comes_ends_with_its_container() {
        assert_eq!(
            take_up_and_end(false).await,
            Some(Say::Ended(Ended {
                code: None,
                failed: true
            }))
        );
    }

    /// A new daemon takes up a session whose container runs. With
    /// `relay`, its relay connects. Then the container ends with no exit
    /// of the relay. It gives the last line that a follower got.
    async fn take_up_and_end(relay: bool) -> Option<Say> {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let fake = Fake::default().with_image(IMAGE);
        let folder = root.path().join("sessions/issue-1");
        std::fs::create_dir_all(&folder).unwrap();
        let name = "riff-issue-1-aaaaaaaa";
        let record = Record {
            live: Live {
                name: name.into(),
                session: "s".into(),
                item: "issue-1".into(),
                image: IMAGE.into(),
                since_ms: 0,
            },
            folder: folder.clone(),
            server: "http://127.0.0.1:1".into(),
            me: me(name),
            inputs: vec![],
            closed: false,
            failed: false,
        };
        private_dir(&dir.join(LIVE)).unwrap();
        std::fs::write(
            record_path(&dir, name),
            serde_json::to_string(&record).unwrap(),
        )
        .unwrap();
        let mut spec = spec(name, &folder);
        spec.env.clear();
        let running = fake.start(&spec, &[]).await.unwrap();

        let listener = listen(&dir).await.unwrap();
        let daemon = Arc::new(Daemon::open(fake.clone(), dir.clone()).await.unwrap());
        daemon.recover().await;
        tokio::spawn(daemon.serve(listener, std::future::pending()));
        let mut follower = Client::connect(&dir).await.unwrap();
        follower
            .ask(Ask::Follow {
                name: name.into(),
                after: 0,
            })
            .await
            .unwrap();
        let relay = match relay {
            true => Some(Relay::connect(&session_dir(&dir, name), 0, false).await),
            false => None,
        };
        fake.stop(name).await.unwrap();
        drop((relay, running));
        to_the_end(&mut follower).await.1
    }

    #[tokio::test]
    async fn the_daemon_pulls_only_an_image_that_the_machine_does_not_have() {
        let (local, remote, missing) = ("img-local", "img-remote", "img-missing");
        let here = Here::new(Fake::default().with_image(local).with_registry(remote)).await;
        let mut shown = Vec::new();
        image(&here.dir, local, |l| shown.push(l.to_owned()))
            .await
            .unwrap();
        assert!(shown.is_empty());
        image(&here.dir, remote, |l| shown.push(l.to_owned()))
            .await
            .unwrap();
        assert_eq!(shown, [text::run_pulling(remote)]);
        let error = image(&here.dir, missing, |_| {})
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains(&text::run_no_image(missing)), "{error}");
        assert_eq!(
            here.fake.calls(),
            [format!("pull {remote}"), format!("pull {missing}")]
        );
    }

    #[tokio::test]
    async fn the_daemon_refuses_another_protocol_and_a_second_daemon() {
        let here = Here::new(Fake::default()).await;
        let conn = connect_in(&here.dir, SOCKET).await.unwrap();
        let (read, mut write) = conn.into_split();
        write
            .write_all(b"{\"protocol\":99,\"ask\":{\"ask\":\"status\"}}\n")
            .await
            .unwrap();
        let line = BufReader::new(read)
            .lines()
            .next_line()
            .await
            .unwrap()
            .unwrap();
        let said: Say = serde_json::from_str(&line).unwrap();
        assert_eq!(
            said,
            Say::Failed {
                error: text::daemon_protocol(99, PROTOCOL)
            }
        );
        // A line that is not a hello: the error shows no part of it.
        let mut client = Client::connect(&here.dir).await.unwrap();
        client
            .line(&serde_json::json!({"gho_secret1": 1}))
            .await
            .unwrap();
        match client.next().await.unwrap() {
            Some(Say::Failed { error }) => assert!(!error.contains("gho_secret1"), "{error}"),
            other => panic!("{other:?}"),
        }

        let error = listen(&here.dir).await.unwrap_err().to_string();
        assert!(error.contains("A riff daemon runs already"), "{error}");
    }

    #[tokio::test]
    async fn a_new_daemon_takes_the_place_of_a_socket_that_no_daemon_answers() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        drop(listen(&dir).await.unwrap());
        // The socket file stays, and no daemon answers on it. A connect
        // alone can still work: a child that another test forks holds
        // the listener until its exec.
        assert!(socket(&dir).exists());
        assert!(status(&dir).await.is_err());
        listen(&dir).await.unwrap();
    }

    #[test]
    fn a_start_and_a_say_go_through_json() {
        let root = Path::new("/r");
        let start = Start {
            session: "s".into(),
            item: "issue-12".into(),
            folder: root.join("f"),
            spec: spec("n", &root.join("clone")),
            start: "Do it.".into(),
            values: vec![("GH_TOKEN".into(), "v".into())],
            secrets: vec!["v".into()],
            server: "http://127.0.0.1:1".into(),
            me: me("n"),
        };
        let line = serde_json::to_string(&Hello {
            protocol: PROTOCOL,
            ask: Ask::Start(Box::new(start)),
        })
        .unwrap();
        let Ok(Ask::Start(back)) = hello(&line) else {
            panic!("{line}")
        };
        assert_eq!(back.spec, spec("n", &root.join("clone")));
        assert_eq!(back.values, [("GH_TOKEN".to_owned(), "v".to_owned())]);
        assert_eq!(back.me, me("n"));
        let ended = Say::Ended(Ended {
            code: None,
            failed: true,
        });
        let line = serde_json::to_string(&ended).unwrap();
        assert_eq!(line, r#"{"say":"ended","code":null,"failed":true}"#);
        assert_eq!(serde_json::from_str::<Say>(&line).unwrap(), ended);
        assert_eq!(
            serde_json::to_string(&Then::Stop).unwrap(),
            r#"{"then":"stop"}"#
        );
        // The lines of the relay protocol.
        assert_eq!(
            serde_json::to_string(&Say::Ack { seq: 3 }).unwrap(),
            r#"{"say":"ack","seq":3}"#
        );
        assert_eq!(
            serde_json::to_string(&SessionAsk::Relay {
                protocol: 2,
                inputs: 1,
                closed: false
            })
            .unwrap(),
            r#"{"ask":"relay","protocol":2,"inputs":1,"closed":false}"#
        );
        assert_eq!(
            serde_json::to_string(&Up::Event {
                seq: 1,
                line: "{}".into()
            })
            .unwrap(),
            r#"{"relay":"event","seq":1,"line":"{}"}"#
        );
    }

    /// A session `mike` of this repository, and a person `mike`.
    fn mike() -> (riff_core::name::SessionUri, riff_core::name::SessionUri) {
        let session = "riff://mike@thelio/como-technologies/riff?session=a6cf"
            .parse()
            .unwrap();
        (session, "riff://mike@thelio".parse().unwrap())
    }

    /// Resumes the riff of `daemon` as the person `mike`, and claims
    /// `item` as the session `mike`.
    async fn claim(daemon: &Daemon<Fake>, item: &str) {
        use riff_core::wire::{Claim, Resume};
        use riff_state::state::Role;
        let (session, person) = mike();
        let admin = Caller::of(&person).with_role(Role::Admin);
        let state = daemon.state();
        state.run(&admin, &Resume::whole(person)).await.unwrap();
        let claim = Claim {
            thread: session.default_thread().unwrap(),
            me: session.clone(),
            item: item.into(),
        };
        state.run(&Caller::of(&session), &claim).await.unwrap();
    }

    /// The claims of the session `mike` in the state of `daemon`.
    async fn claims(daemon: &Daemon<Fake>) -> Vec<String> {
        let (session, _) = mike();
        daemon
            .state()
            .read(|state| {
                state
                    .uri(session.who(), std::time::Instant::now())
                    .claims()
                    .to_vec()
            })
            .await
    }

    /// 01M4ZHW2083VFTQJ772ZJ27XAC: the first start makes the riff; a
    /// stop writes a checkpoint; the next start loads it, with the riff
    /// and the claims.
    #[tokio::test]
    async fn the_daemon_keeps_its_state_across_a_restart() {
        let mut here = Here::new(Fake::default()).await;
        let first = status(&here.dir).await.unwrap();
        let riff = first.riff.clone().unwrap();
        assert_eq!(riff.len(), 32);
        assert!(riff.chars().all(|c| c.is_ascii_hexdigit()), "{riff}");
        claim(&here.daemon, "issue-12").await;
        let position = here.daemon.status().await.position;
        assert!(position > first.position);
        let store = Dir::new(state_dir(&here.dir));
        assert!(checkpoint::names(&store).await.unwrap().is_empty());

        here.restart().await;
        let names = checkpoint::names(&store).await.unwrap();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].0, position);
        let again = status(&here.dir).await.unwrap();
        assert_eq!(again.riff.as_deref(), Some(riff.as_str()));
        assert_eq!(again.position, position);
        assert_eq!(claims(&here.daemon).await, ["issue-12"]);
        here.stop().await;
    }

    /// 01M4ZHW2083VFTQJ772ZJ27XAC: a daemon that ends with no stop
    /// writes no checkpoint. The next start replays its log.
    #[tokio::test]
    async fn a_start_after_a_crash_replays_the_log() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let daemon = Daemon::open(Fake::default(), dir.clone()).await.unwrap();
        claim(&daemon, "issue-12").await;
        let before = daemon.status().await;
        drop(daemon);

        let store = Dir::new(state_dir(&dir));
        assert!(checkpoint::names(&store).await.unwrap().is_empty());
        let daemon = Daemon::open(Fake::default(), dir.clone()).await.unwrap();
        let after = daemon.status().await;
        assert_eq!(after.riff, before.riff);
        assert_eq!(after.position, before.position);
        assert_eq!(claims(&daemon).await, ["issue-12"]);
    }

    /// The state is in the folder of the daemon, which only the user
    /// reads (01M4ZDTVQEHD87WD5TSN93XFSS). The first start writes one
    /// chunk: the riff.
    #[tokio::test]
    async fn the_state_is_in_the_folder_of_the_daemon() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        Daemon::open(Fake::default(), dir.clone()).await.unwrap();
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        let chunks = std::fs::read_dir(state_dir(&dir).join("log")).unwrap();
        assert_eq!(chunks.count(), 1);
    }
}
