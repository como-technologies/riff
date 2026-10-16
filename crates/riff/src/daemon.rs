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
//! | `state/` | The log and the checkpoints of the state of the daemon ([`state_dir`]), and its riff (`riff.json`, [`RIFF`]). |
//! | `state.lost-MS/` | The old state of a lost key, from the time `MS` ([`Lost`]). riff never reads it. |
//! | `limit.json` | The limit of the host ([`crate::room`]), when `riff daemon limit` set it. |
//! | `waves.json` | The waves that the daemon runs ([`crate::wave`]). |
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
//! [`Local`] in [`state_dir`] (01M519Z2JMAKN0T6JZFBPSTKC8).
//!
//! - [`Daemon::open`] reads the key of the daemon, then loads the state:
//!   the newest checkpoint, and the log after it. See "The key of the
//!   daemon".
//! - Each change goes through [`Daemon::state`]: one command at a time,
//!   with its records in the log before its reply
//!   (01M4ZHW22F8HE2E1WDSTNDJW4H). The riff tools of a session act on it
//!   with peer to peer. Until then, they act on the riff-server of the
//!   session ([`Daemon::tools_of`]).
//! - At the stop, [`Daemon::serve`] writes a checkpoint. A start after a
//!   crash replays the log after the last checkpoint.
//! - `riff daemon status` shows the public key of the daemon, the ID of
//!   its riff, the user of the owner and the position of the log
//!   ([`Status::key`], [`Status::riff`], [`Status::owner`],
//!   [`Status::position`], 01M519Z2N2PMZJ0CYAPV0A47BZ,
//!   01M4ZHW24JZZXM3JJXG3PRJS7N).
//!
//! # The key of the daemon
//!
//! Each daemon has one ed25519 key in the OS keyring
//! ([`crate::daemon_key`], 01M519Z2BB0YJY5CRMB54JVYFA). At its first
//! start, the daemon makes the key and a riff of its own: the riff ID is
//! the public key, and the owner is the user of the daemon (`USER`). The
//! object [`RIFF`] of the state keeps the riff ID and the user of the
//! owner, until the record `riff` of Wave B keeps them.
//!
//! The key and the state go together (01M519Z2G77CT94Y6XPW9BCRK9). In a
//! riff of its own, the riff ID names the key of the state. So [`step`]
//! compares the key of the keyring with the riff of the state:
//!
//! ```mermaid
//! flowchart TD
//!     O["Daemon::open"] --> W["daemon_key::wait: the key of the keyring<br/>(it waits while the keyring refuses)"]
//!     W --> L["Local::open: the newest checkpoint,<br/>then the log after it"]
//!     L --> S{"step: the key and the riff of the state"}
//!     S -->|"the riff is the public key"| G["go on with the key and the riff"]
//!     S -->|"no key and no riff"| F["the first start: a new key and a riff of its own"]
//!     S -->|"a key and no riff, a riff and no key,<br/>a value that is no key, another key"| N["a line that says why;<br/>the old state goes to state.lost-MS;<br/>a new key and a riff of its own"]
//! ```
//!
//! A new key goes to the keyring before the riff goes to the log. So a
//! crash between the two gives a key and no riff: the next start makes
//! a new key again, and says why.
//!
//! While the daemon waits for its keyring, it makes no key and no riff
//! (01M51BFYNMYHSQ62BBJES3CCWT). `riff daemon` already listens: it
//! answers [`Ask::Status`] with [`Status::opening`], which says what the
//! daemon waits for, and refuses each other ask with the same line
//! (01M51JW233MXB72MHHRAX4E4NF).
//!
//! # The relay
//!
//! The daemon binds its own binary at [`RELAY_BIN`] in each container,
//! with no write, and runs `RELAY_BIN relay -- COMMAND`: `riff relay` is
//! the entrypoint (01M4Z60B7KEV0H161RBZ1SDRY2, 01M4ZHSZZDH0C01A5EVD0KFY14).
//! The relay gives `claude` each input, and sends the daemon each event
//! ([`crate::session_relay`]).
//!
//! A session image can have an older libc than the host. So before each
//! start, the daemon reads the ELF of that binary ([`Daemon::check_relay`]).
//! When it is not static, for example a riff of `cargo install`, the
//! daemon starts no container, and the error names `riff update`
//! (01M510NW8YEVMQKD6RWJPW38N3). It reads the file at each start: after
//! `riff update`, the path holds the static riff, and the next start
//! works with no restart of the daemon.
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
//! | [`Ask::Wave`] | [`Say::Done`] when the daemon runs the wave, or [`Say::Failed`] |
//! | [`Ask::WaveStop`] | [`Say::Done`] when the daemon ran the wave, or [`Say::Failed`] |
//! | [`Ask::Takeover`] | [`Say::Line`] while the turn ends, [`Say::Held`], then [`Say::Done`] after the go on, or [`Say::Failed`] |
//!
//! After [`Ask::Start`] or [`Ask::Follow`], `riff` can send
//! [`Then::Stop`]: the daemon stops the container. When `riff` closes the
//! connection, the session goes on: the daemon owns it. After
//! [`Say::Held`], `riff takeover` sends [`Then::GoOn`].
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
//! | [`SessionAsk::Watch`] | says [`Say::Line`] at the next wake of the session ([`World::wake`]) |
//!
//! # A session
//!
//! Each start waits for room in the limit of the host ([`crate::room`]).
//! While a start of `riff run` waits, the daemon says why with
//! [`Say::Line`]. A [`Then::Stop`], or the end of the connection, before
//! the start starts nothing (01M4ZV3B6VS0RAR0P7MRHKE8CQ). The waves of
//! the daemon start their sessions in the same line ([`crate::wave`]).
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
//! `claude -p` does not end while one of its background tasks runs. The
//! end of each task starts a new turn. So a session can stay up after
//! the close. When no event comes for the quiet limit after the close
//! ([`QUIET_LIMIT`]), the daemon stops the container, and tells each
//! `riff` that follows the session why (01M4ZSZX77AJDGWNQMGBPV2QXG). Each
//! event moves the end of the quiet.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant D as riff daemon
//!     participant C as riff relay and claude -p
//!     C->>D: result
//!     D->>C: close the input
//!     C->>D: background_tasks_changed: one task runs
//!     Note over C: the task ends: a new turn
//!     C->>D: init, ..., result
//!     Note over D,C: no event for the quiet limit
//!     D->>C: stop the container
//! ```
//!
//! # The lead
//!
//! The lead is a session with a terminal (01M4Z60B81RTZT71PN5HV0GZHX,
//! 01M501PA2C087SEN4M4QTK5J1E, [`crate::lead`]). [`Ask::Start`] with [`Start::terminal`] starts it
//! with [`Runtime::start_terminal`]: conmon holds its terminal, and
//! `riff` attaches to it. Its command is `riff relay --terminal --
//! claude ...`: the relay adds the riff tools. The lead does not wait
//! for room, and does not count in the limit of the host: it runs no
//! build of an item. While it runs, the daemon keeps its session on
//! riff-server alive, and at its end it ends that session
//! ([`World::resume`], [`World::leave`]).
//!
//! # A takeover
//!
//! A person takes over a worker with [`Ask::Takeover`]
//! (01M4Z60B83ZH10HYKW7X1C0SSS, 01M501PABB010P4S8YF7R29XXS). The daemon keeps the takeover in the
//! [`Record`], and tells the relay [`Say::Hold`]: `claude` ends after
//! its turn, and the relay waits ([`crate::session_relay`]). At the
//! [`Up::Held`] of the relay, the daemon says [`Say::Held`] to `riff
//! takeover`, which runs `claude --resume ID` in the container with the
//! terminal of the person. At [`Then::GoOn`], or at the end of the
//! connection of `riff takeover`, the daemon gives the session back: it
//! tells the relay [`Say::GoOn`], and gives `claude` one new input
//! ([`text::TAKEOVER_GO_ON`]). The quiet limit does not count during a
//! takeover. A restart of the daemon keeps the takeover: a new `riff
//! takeover NAME` gets [`Say::Held`] at once. The daemon refuses a
//! takeover of a session with a terminal, and of a relay of a protocol
//! before 4.
//!
//! [`Up::Held`]: crate::session_relay::Up::Held
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
//!
//! # A new riff
//!
//! `riff update` puts a new riff on disk, and the daemon goes on with
//! the old one. The daemon runs an old riff when its binary was
//! replaced, or when its unit names another riff ([`stale`]). Two steps
//! start it again on the new riff, only when it is idle: it owns no
//! session, no start waits for room, and no job of a wave runs
//! ([`Status::busy`]).
//!
//! - `riff update` ([`after_update`]): when the `ExecStart` of the unit
//!   is not the `riff` on `PATH`, it writes the unit again with that riff
//!   and the same variables (01M517BTDRKDXD3X84P6AEAF7P). When the
//!   daemon runs an old riff and is idle, it starts the unit again. Else
//!   it says that the daemon starts again after its last session
//!   (01M517BTJ9EV4ZGPYRB6HXJJC9).
//! - The daemon ([`Daemon::restart_when_old`]): at the end of each
//!   session and of each job, and each [`OLD_LOOK`], it looks. When it
//!   is idle, runs an old riff, and is the process of the unit, it asks
//!   systemd once to start the unit again (01M517BTP6BAGTRBYWPX9GPVE0).
//!   The stop stops no container (01M4ZDTW00B7N6T9P3MK066KA7).
//!
//! ```mermaid
//! flowchart TD
//!     U["riff update"] --> X{"ExecStart is the riff on PATH?"}
//!     X -- no --> W["write the unit with the riff on PATH,<br/>systemctl --user daemon-reload"]
//!     X -- yes --> O
//!     W --> O{"the daemon runs an old riff?"}
//!     O -- no --> N["nothing"]
//!     O -- yes --> B{"a session, a start that waits,<br/>or a job of a wave?"}
//!     B -- no --> R["systemctl --user --no-block try-restart riff.service"]
//!     B -- yes --> L["the daemon starts again<br/>after its last session"]
//!     L --> E["the end of a session or a job, each minute"]
//!     E --> B
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
use riff_state::store::{Dir, Store};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Notify, mpsc};
use tokio::task::{JoinHandle, JoinSet};

use crate::daemon_key::{Found, Key};
use crate::room::{Limit, Room, Slot};
use crate::run::{Scrub, user_line};
use crate::runtime::{Bind, End, Env, Runtime, SessionSpec, Source};
use crate::session_relay::Up;
use crate::text;
use crate::wave::{Job, Wave, World};

/// The version of the protocol between `riff` and the daemon, and
/// between the relay and the daemon. Version 3 adds the waves
/// ([`Ask::Wave`], [`Ask::WaveStop`]). Version 4 adds the sessions with a
/// terminal, the takeover and the watch of a session.
pub const PROTOCOL: u32 = 4;

/// The first protocol of a relay that takes [`Say::Hold`].
pub const TAKEOVER_PROTOCOL: u32 = 4;

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

/// The quiet limit: after the input of `claude` closed, the longest time
/// with no event of the session. Then the daemon stops its container
/// (01M4ZSZX77AJDGWNQMGBPV2QXG). It is longer than the longest check of
/// a session: a `just ci` can take 15 minutes.
pub const QUIET_LIMIT: Duration = Duration::from_secs(30 * 60);

/// The variable that gives the quiet limit of a `riff daemon` in
/// milliseconds. Only a test sets it.
///
/// ```
/// assert_eq!(riff::daemon::QUIET_VAR, "RIFF_DAEMON_QUIET_MS");
/// ```
pub const QUIET_VAR: &str = "RIFF_DAEMON_QUIET_MS";

/// [`QUIET_LIMIT`], or the milliseconds of [`QUIET_VAR`].
fn quiet_limit() -> Duration {
    std::env::var(QUIET_VAR)
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map_or(QUIET_LIMIT, Duration::from_millis)
}

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

/// How often the daemon looks for a new riff (01M517BTP6BAGTRBYWPX9GPVE0).
/// The end of each session and of each job looks too.
pub const OLD_LOOK: Duration = Duration::from_secs(60);

/// True when a daemon that runs the binary `own` runs an old riff: its
/// binary was `replaced`, or its `unit` names another riff
/// (01M517BTJ9EV4ZGPYRB6HXJJC9).
///
/// ```
/// use std::path::Path;
/// use riff::daemon::stale;
/// use riff::service::Unit;
///
/// let own = Path::new("/home/mike/.cargo/bin/riff");
/// let unit = |exe: &str| Unit { exe: exe.into(), env: vec![] };
/// let cargo = unit("/home/mike/.cargo/bin/riff");
/// assert!(!stale(Some(&cargo), own, false));
/// assert!(stale(Some(&cargo), own, true));
/// let check = unit("/home/mike/.cache/riff/check/target/static/release/riff");
/// assert!(stale(Some(&check), own, false));
/// assert!(!stale(None, own, false));
/// ```
pub fn stale(unit: Option<&crate::service::Unit>, own: &Path, replaced: bool) -> bool {
    replaced || unit.is_some_and(|unit| !crate::binary::same_file(&unit.exe, own))
}

/// True when this daemon runs an old riff ([`stale`]), and the unit of
/// `manager` runs it (01M517BTP6BAGTRBYWPX9GPVE0). The daemon of a
/// `RIFF_HOME` is never the daemon of the unit.
fn old_here(manager: &impl crate::service::ServiceManager) -> bool {
    if crate::home::dir().is_some() {
        return false;
    }
    let Ok(own) = own_binary() else {
        return false;
    };
    let unit = manager.installed().ok().flatten();
    let replaced = crate::binary::replaced(Path::new("/proc/self/exe"));
    stale(unit.as_ref(), &own, replaced)
        && manager.main_pid().ok().flatten() == Some(std::process::id())
}

/// The step of `riff update` for the daemon of this machine, whose
/// folder is `dir`, with the new `riff` on `PATH`: one line for each
/// change.
///
/// When the `ExecStart` of the unit of `manager` is not `riff`, it
/// writes the unit again with `riff` and the same variables
/// (01M517BTDRKDXD3X84P6AEAF7P). When the daemon answers, runs an old
/// riff ([`stale`]) and is idle ([`Status::busy`]), it starts the unit
/// again. When it is not idle, the daemon starts again after its last
/// session ([`Daemon::restart_when_old`], 01M517BTJ9EV4ZGPYRB6HXJJC9).
/// With no unit, it does nothing.
pub async fn after_update(
    manager: &impl crate::service::ServiceManager,
    riff: &Path,
    dir: &Path,
) -> Result<Vec<String>> {
    let Some(old) = manager.installed()? else {
        return Ok(Vec::new());
    };
    let mut lines = Vec::new();
    let unit = crate::service::Unit {
        exe: riff.to_owned(),
        env: old.env.clone(),
    };
    if !crate::binary::same_file(&old.exe, riff) {
        let path = manager.rewrite(&unit)?;
        lines.push(text::daemon_unit_moved(&path, &old.exe, riff));
    }
    let Ok(status) = status(dir).await else {
        return Ok(lines);
    };
    let exe = std::fs::read_link(format!("/proc/{}/exe", status.pid))
        .map(crate::binary::on_disk)
        .unwrap_or(old.exe);
    if !stale(Some(&unit), &exe, crate::binary::runs_replaced(status.pid)) {
        return Ok(lines);
    }
    match status.busy() {
        0 => {
            manager.try_restart()?;
            lines.push(text::DAEMON_STARTS_AGAIN.to_owned());
        }
        busy => lines.push(text::daemon_starts_again_later(busy)),
    }
    Ok(lines)
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
    /// Run the current wave of a repository ([`crate::wave`]).
    Wave {
        /// The wave.
        wave: Wave,
    },
    /// Start no new session for the wave of the repository `repo`.
    WaveStop {
        /// The repository, as `OWNER/REPO`.
        repo: String,
    },
    /// Take over the live session in the container `name`
    /// (01M4Z60B83ZH10HYKW7X1C0SSS).
    Takeover {
        /// The name of its container.
        name: String,
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
    /// True for a session with a terminal: the lead. Its start text is
    /// in the command of the spec.
    #[serde(default)]
    pub terminal: bool,
}

/// A line of `riff` after [`Ask::Start`], [`Ask::Follow`] or
/// [`Ask::Takeover`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "then", rename_all = "snake_case")]
pub enum Then {
    /// Stop the container.
    Stop,
    /// The terminal of the takeover ended: the session goes on.
    GoOn,
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
        /// True when `claude` ended for a takeover, and the relay waits
        /// for [`Say::GoOn`].
        #[serde(default)]
        held: bool,
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
    /// Wait for the next wake of the session: `riff relay watch`.
    Watch {
        /// [`PROTOCOL`] of the relay.
        protocol: u32,
    },
}

/// What a session does now, as `riff view` shows it
/// (01M501PADKZNRHBZD9XPMJGW06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// A turn of `claude` runs.
    #[default]
    Works,
    /// After the result of the turn: `claude` ends, or waits for its
    /// background tasks.
    Ends,
    /// A takeover waits for the end of the turn.
    Holds,
    /// `claude` ended for a takeover: a person has the session.
    Taken,
    /// A session with a terminal: the lead.
    Terminal,
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
    /// What the session does now.
    #[serde(default)]
    pub phase: Phase,
    /// The last step of the session in a few words: the last line that
    /// `riff run` shows of its events ([`crate::run::event_lines`]).
    #[serde(default)]
    pub last: String,
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
    /// What the daemon waits for before it serves, while it opens: its
    /// key in the OS keyring, or its state (01M51JW233MXB72MHHRAX4E4NF).
    #[serde(default)]
    pub waits: Option<String>,
    /// The public key of the daemon (01M519Z2N2PMZJ0CYAPV0A47BZ).
    #[serde(default)]
    pub key: Option<String>,
    /// The ID of the riff of this machine, from the state of the daemon.
    #[serde(default)]
    pub riff: Option<String>,
    /// The user of the owner of the riff of the daemon.
    #[serde(default)]
    pub owner: Option<String>,
    /// The position of the last record of the log of the daemon.
    #[serde(default)]
    pub position: u64,
    /// Its sessions, by name.
    pub sessions: Vec<Live>,
    /// The limit of the host ([`crate::room`]).
    #[serde(default)]
    pub limit: Option<Limit>,
    /// The starts that wait for room, in order.
    #[serde(default)]
    pub waiting: Vec<String>,
    /// The waves that the daemon runs, by repository.
    #[serde(default)]
    pub waves: Vec<WaveShown>,
    /// The jobs of the waves that run: from the pick of the item to the
    /// end of the steps after its session.
    #[serde(default)]
    pub jobs: usize,
}

impl Status {
    /// The work of the daemon: its sessions, the starts that wait for
    /// room, and the jobs of its waves. The daemon starts again on a new
    /// riff only at 0 (01M517BTJ9EV4ZGPYRB6HXJJC9).
    ///
    /// ```
    /// let status: riff::daemon::Status = serde_json::from_value(serde_json::json!({
    ///     "pid": 7, "build": "1.4.0", "since_ms": 0, "dir": "/d", "sessions": [],
    /// }))?;
    /// assert_eq!(status.busy(), 0);
    /// let waits = riff::daemon::Status { waiting: vec!["riff-issue-12".into()], ..status.clone() };
    /// assert_eq!(waits.busy(), 1);
    /// assert_eq!(riff::daemon::Status { jobs: 2, ..status }.busy(), 2);
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    pub fn busy(&self) -> usize {
        self.sessions.len() + self.waiting.len() + self.jobs
    }

    /// The state of the daemon of the process `pid` in the folder `dir`
    /// while it opens: it waits for `waits`, and has no key, no riff and
    /// no session yet (01M51JW233MXB72MHHRAX4E4NF).
    ///
    /// ```
    /// let status = riff::daemon::Status::opening(7, "/d".into(), 0, "its key".into());
    /// assert_eq!(status.waits.as_deref(), Some("its key"));
    /// assert_eq!((status.key, status.riff, status.busy()), (None, None, 0));
    /// ```
    pub fn opening(pid: u32, dir: PathBuf, since_ms: i64, waits: String) -> Status {
        Status {
            pid,
            build: Build::this().to_string(),
            since_ms,
            dir,
            waits: Some(waits),
            key: None,
            riff: None,
            owner: None,
            position: 0,
            sessions: Vec::new(),
            limit: None,
            waiting: Vec::new(),
            waves: Vec::new(),
            jobs: 0,
        }
    }
}

/// A wave that the daemon runs, as `riff daemon status` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveShown {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// What the wave did last, in a few words.
    pub word: String,
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
    /// To the relay: a takeover. Close the stdin of `claude`, and keep
    /// the container at its end.
    Hold,
    /// To the relay: the takeover ended. Start `claude` again.
    GoOn,
    /// To `riff takeover`: `claude` ended its turn. Run the terminal of
    /// the person now.
    Held {
        /// The riff session ID: `claude --resume` takes it.
        session: String,
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
    /// The job of a session that the daemon started for a wave: the
    /// daemon does its steps after the end, also after a restart
    /// ([`crate::wave`]). `None` for a session of `riff run`.
    #[serde(default)]
    pub job: Option<Job>,
    /// True for a session with a terminal: the lead.
    #[serde(default)]
    pub terminal: bool,
    /// True from the ask of a takeover until the daemon gives the
    /// session back.
    #[serde(default)]
    pub takeover: bool,
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
    /// The input of `claude` closed: the quiet limit counts from now.
    closing: Notify,
    /// The task of the socket of the session.
    door: Mutex<Option<JoinHandle<()>>>,
}

/// The state of an [`Owned`] session.
struct SessionState {
    record: Record,
    /// The number of events in `events.jsonl`.
    seq: u64,
    /// The time of the last event, or of the start of the ownership.
    last: tokio::time::Instant,
    log: std::fs::File,
    /// The connections of `riff` that follow the session.
    followers: Vec<Out>,
    /// The connection of the relay.
    relay: Option<Out>,
    /// The protocol of the last relay that connected, or 0.
    relay_protocol: u32,
    /// True when `claude` ended for a takeover, and the relay waits.
    held: bool,
    /// The connection of `riff takeover`.
    taker: Option<Out>,
    /// The last step of the session ([`Live::last`]).
    last_step: String,
    /// True after the end of the ownership: its record is gone.
    ended: bool,
}

impl SessionState {
    /// What the session does now.
    fn phase(&self) -> Phase {
        let record = &self.record;
        if record.terminal {
            Phase::Terminal
        } else if record.takeover && self.held {
            Phase::Taken
        } else if record.takeover {
            Phase::Holds
        } else if record.closed {
            Phase::Ends
        } else {
            Phase::Works
        }
    }

    /// Gives the session back after a takeover, when `claude` ended for
    /// it: the relay starts `claude` again, with one new input.
    fn go_on(&mut self) {
        if !self.held || self.record.takeover {
            return;
        }
        self.held = false;
        let line = user_line(text::TAKEOVER_GO_ON);
        self.record.inputs.push(line.clone());
        self.record.closed = false;
        if let Some(relay) = &self.relay {
            relay.say(Say::GoOn);
            relay.say(Say::Input { line });
        }
    }

    /// The session as `riff daemon status` shows it.
    fn live(&self) -> Live {
        Live {
            phase: self.phase(),
            last: self.last_step.clone(),
            ..self.record.live.clone()
        }
    }
}

impl Owned {
    fn lock(&self) -> MutexGuard<'_, SessionState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Writes the record of `state` in place of the one on disk. After
    /// the end of the ownership, it writes nothing: a record of a
    /// session that ended would make a new daemon do its end again.
    fn save(&self, state: &SessionState) -> Result<()> {
        if state.ended {
            return Ok(());
        }
        let tmp = self.record_path.with_extension("json.new");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&state.record)?)
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
        state.last = tokio::time::Instant::now();
        let event = serde_json::from_str::<Value>(&line).ok();
        if let Some(step) = event
            .as_ref()
            .and_then(|e| crate::run::event_lines(e).pop())
        {
            state.last_step = step;
        }
        if let Some(event) = event
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
                self.closing.notify_one();
            }
            if let Err(e) = self.save(&state) {
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
    /// `protocol`, `inputs`, `closed` and `held`.
    async fn relay(
        &self,
        protocol: u32,
        (inputs, closed, held): (u64, bool, bool),
        mut lines: Lines<BufReader<OwnedReadHalf>>,
        write: OwnedWriteHalf,
    ) {
        let (out, written) = writer(write);
        {
            let mut state = self.lock();
            out.say(Say::Resume { events: state.seq });
            state.relay_protocol = protocol;
            state.held = held;
            // A takeover that ended while the relay was away: go on
            // first, so that the new input reaches the new claude.
            let gone_on = held && !state.record.takeover;
            if gone_on {
                out.say(Say::GoOn);
                state.held = false;
            }
            let given = usize::try_from(inputs).unwrap_or(usize::MAX);
            for line in state.record.inputs.iter().skip(given) {
                out.say(Say::Input { line: line.clone() });
            }
            if gone_on && usize::try_from(inputs).is_ok_and(|n| n >= state.record.inputs.len()) {
                let line = user_line(text::TAKEOVER_GO_ON);
                state.record.inputs.push(line.clone());
                out.say(Say::Input { line });
                state.record.closed = false;
            }
            if state.record.closed && !closed && !gone_on {
                out.say(Say::Close);
            }
            if state.record.takeover && !held {
                out.say(Say::Hold);
            }
            if held
                && state.record.takeover
                && let Some(taker) = &state.taker
            {
                taker.say(Say::Held {
                    session: state.record.live.session.clone(),
                });
            }
            state.relay = Some(out.clone());
            if let Err(e) = self.save(&state) {
                eprintln!("riff daemon: {e:#}");
            }
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
                Ok(Up::Held) => {
                    let mut state = self.lock();
                    state.held = true;
                    if state.record.takeover {
                        if let Some(taker) = &state.taker {
                            taker.say(Say::Held {
                                session: state.record.live.session.clone(),
                            });
                        }
                    } else {
                        // The person left before the end of the turn.
                        state.go_on();
                        if let Err(e) = self.save(&state) {
                            eprintln!("riff daemon: {e:#}");
                        }
                    }
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
    /// True when [`crate::release::DYNAMIC`] lets the daemon bind a
    /// binary that is not static: a test.
    any_relay: bool,
    /// The quiet limit of each session ([`QUIET_LIMIT`]).
    quiet: Duration,
    live: Mutex<BTreeMap<String, Arc<Owned>>>,
    /// The key of the daemon ([`crate::daemon_key`]).
    key: Key,
    /// The user of the owner of its riff.
    owner: String,
    state: Local,
    /// The limit of the host, and the starts that wait for room.
    room: Arc<Room>,
    /// The forge and the riff of the waves ([`crate::wave::Live`]).
    world: Arc<dyn World>,
    /// The waves that run, by repository.
    waves: Mutex<BTreeMap<String, Running>>,
    /// A session ended, or a job of a wave ended: the waves look again.
    changed: Notify,
}

/// A wave that runs in the daemon.
struct Running {
    /// The wave.
    wave: Wave,
    /// The task of its loop.
    task: JoinHandle<()>,
    /// The items of its jobs that the daemon does not own yet.
    taken: Arc<Mutex<std::collections::BTreeSet<String>>>,
    /// What it did last.
    word: Arc<Mutex<String>>,
}

/// The object of the riff of the daemon in its state: the riff ID and
/// the user of the owner ([`Made`]). The record `riff` of Wave B takes
/// its place.
pub const RIFF: &str = "riff.json";

/// The riff that a daemon made: the object [`RIFF`] of its state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Made {
    /// The ID of the riff: the public key of the daemon.
    pub riff: String,
    /// The user of the owner.
    pub owner: String,
}

/// Why a daemon makes a new key and a riff of its own: its key and its
/// state do not go together (01M519Z2G77CT94Y6XPW9BCRK9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lost {
    /// The keyring holds a key, and the state has no riff.
    NoState,
    /// The state has a riff, and the keyring answers with no key.
    NoKey,
    /// The keyring holds a value that is not a key.
    NotAKey,
    /// The riff of the state is not the public key of the keyring.
    OtherKey {
        /// The riff of the state.
        riff: String,
    },
}

/// What the start of a daemon does with the key of its keyring and the
/// riff of its state.
#[derive(Debug)]
pub enum Step {
    /// The riff is the public key: go on with the key.
    Go(Key),
    /// No key and no riff: the first start.
    First,
    /// The key and the state do not go together.
    New(Lost),
}

/// The step of a start with the key `found` of the keyring and the riff
/// `riff` of the state (01M519Z2G77CT94Y6XPW9BCRK9). In a riff of its
/// own, the riff ID is the public key of the daemon.
///
/// ```
/// use riff::daemon::{Lost, Step, step};
/// use riff::daemon_key::{Found, Key};
///
/// let key = || Found::Key(Key::from_bytes(&[1; 32]));
/// let own = Key::from_bytes(&[1; 32]).public().to_string();
/// assert!(matches!(step(key(), Some(&own)), Step::Go(_)));
/// assert!(matches!(step(Found::None, None), Step::First));
/// assert!(matches!(step(key(), None), Step::New(Lost::NoState)));
/// assert!(matches!(step(Found::None, Some(&own)), Step::New(Lost::NoKey)));
/// assert!(matches!(step(Found::NotAKey, None), Step::New(Lost::NotAKey)));
/// let Step::New(Lost::OtherKey { riff }) = step(key(), Some("0a1b")) else { panic!() };
/// assert_eq!(riff, "0a1b");
/// ```
pub fn step(found: Found, riff: Option<&str>) -> Step {
    match (found, riff) {
        (Found::Key(key), Some(riff)) if key.public().to_string() == riff => Step::Go(key),
        (Found::Key(_), Some(riff)) => Step::New(Lost::OtherKey { riff: riff.into() }),
        (Found::Key(_), None) => Step::New(Lost::NoState),
        (Found::None, None) => Step::First,
        (Found::None, Some(_)) => Step::New(Lost::NoKey),
        (Found::NotAKey, _) => Step::New(Lost::NotAKey),
    }
}

/// The user of the daemon: `USER`, in lower case, as riff writes a user.
fn this_user() -> Result<String> {
    let user = std::env::var("USER")
        .ok()
        .filter(|user| !user.is_empty())
        .context(text::DAEMON_NO_USER)?;
    Ok(riff_core::name::sanitize(&user.to_lowercase()))
}

/// Loads the state in the folder `state_dir`, with its store.
async fn load_state(state_dir: &Path) -> Result<(Local, Arc<Dir>)> {
    let store = Arc::new(Dir::new(state_dir));
    let state = Local::open(store.clone(), checkpoint::Settings::default())
        .await
        .with_context(|| text::daemon_no_state(state_dir))?;
    Ok((state, store))
}

/// The riff in the object [`RIFF`] of `store`, when it is there.
async fn made(store: &Dir) -> Result<Option<Made>> {
    let Some(loaded) = store.load(RIFF).await? else {
        return Ok(None);
    };
    let made = serde_json::from_slice(&loaded.bytes)
        .with_context(|| format!("cannot read {}", store.locate(RIFF)))?;
    Ok(Some(made))
}

/// Writes `made` to the object [`RIFF`] of `store`, in place of an older
/// one.
async fn keep_made(store: &Dir, made: &Made) -> Result<()> {
    let known = store.load(RIFF).await?.map(|loaded| loaded.version);
    store.save(RIFF, serde_json::to_vec(made)?, known).await?;
    Ok(())
}

/// Moves the folder `state_dir` of an old key to `state.lost-MS` beside
/// it, or to `state.lost-MS-N` when that name is taken
/// (01M51JW25SPDW564A3DG31D7XM). It gives the new path, or `None` when
/// there is no such folder.
fn put_aside(state_dir: &Path) -> Result<Option<PathBuf>> {
    if !state_dir.exists() {
        return Ok(None);
    }
    let stamp = format!("state.lost-{}", now_ms());
    let lost = (0..)
        .map(|n| match n {
            0 => state_dir.with_file_name(&stamp),
            n => state_dir.with_file_name(format!("{stamp}-{n}")),
        })
        .find(|path| !path.exists())
        .expect("a free name");
    std::fs::rename(state_dir, &lost)
        .with_context(|| format!("cannot move {} to {}", state_dir.display(), lost.display()))?;
    Ok(Some(lost))
}

/// Makes a new key in the secret `name` and a riff of its own in
/// `state`, with the owner `owner`. The key goes to the keyring first.
async fn make_riff(name: &str, state: &Local, store: &Dir, owner: &str) -> Result<Key> {
    let key = Key::generate();
    let name = name.to_owned();
    let key = tokio::task::spawn_blocking(move || crate::daemon_key::keep(&name, &key).map(|()| key))
        .await??;
    let riff = key.public().to_string();
    let make = MakeRiff {
        riff_id: riff.clone(),
    };
    state.run(&Caller::server(), &make).await?;
    let owner = owner.to_owned();
    keep_made(store, &Made { riff, owner }).await?;
    Ok(key)
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
    /// binds its own binary into each session ([`own_binary`]). It reads
    /// its key, and loads its state from [`state_dir`]: the newest
    /// checkpoint, and the log after it. At its first start, it makes its
    /// key and a riff of its own ([`step`]). Only one daemon may open a
    /// folder: call [`listen`] first. Its quiet limit is [`QUIET_LIMIT`],
    /// or the value of [`QUIET_VAR`]. It says each line of its key on
    /// the standard output.
    pub async fn open(runtime: R, dir: PathBuf) -> Result<Self> {
        let say = |line: String| println!("{line}");
        Self::open_with(runtime, dir, crate::secrets::retry(), say).await
    }

    /// [`Daemon::open`] that looks at a keyring that refuses each `retry`
    /// (01M51BFYNMYHSQ62BBJES3CCWT), and gives `say` each line of its key:
    /// the keyring refuses or answers again, a first start, a new key and
    /// why.
    pub async fn open_with(
        runtime: R,
        dir: PathBuf,
        retry: Duration,
        mut say: impl FnMut(String) + Send,
    ) -> Result<Self> {
        private_dir(&dir)?;
        let name = crate::daemon_key::secret_name(&dir);
        let found = crate::daemon_key::wait(&name, retry, &mut say).await?;
        let state_dir = state_dir(&dir);
        let (mut state, mut store) = load_state(&state_dir).await?;
        let riff = state.read(State::riff_id).await;
        let (key, owner) = match step(found, riff.as_deref()) {
            Step::Go(key) => {
                let riff = key.public().to_string();
                let owner = match made(&store).await? {
                    Some(made) if made.riff == riff => made.owner,
                    _ => {
                        let owner = this_user()?;
                        keep_made(&store, &Made { riff, owner: owner.clone() }).await?;
                        owner
                    }
                };
                (key, owner)
            }
            Step::First => {
                let owner = this_user()?;
                let key = make_riff(&name, &state, &store, &owner).await?;
                say(text::daemon_made_riff(&key.public().to_string(), &owner));
                (key, owner)
            }
            Step::New(lost) => {
                let owner = this_user()?;
                drop(state);
                let old = put_aside(&state_dir)?;
                (state, store) = load_state(&state_dir).await?;
                let key = make_riff(&name, &state, &store, &owner).await?;
                let riff = key.public().to_string();
                say(text::daemon_new_riff(&lost, &riff, &owner, old.as_deref()));
                (key, owner)
            }
        };
        let room = Room::new(dir.clone(), Arc::new(crate::room::free_memory));
        Ok(Daemon {
            runtime,
            dir,
            since_ms: now_ms(),
            relay_bin: own_binary().unwrap_or_else(|_| PathBuf::from("riff")),
            any_relay: crate::release::takes_dynamic(),
            quiet: quiet_limit(),
            live: Mutex::new(BTreeMap::new()),
            key,
            owner,
            state,
            room,
            world: Arc::new(crate::wave::Live::default()),
            waves: Mutex::new(BTreeMap::new()),
            changed: Notify::new(),
        })
    }

    /// The daemon with the quiet limit `quiet` in place of
    /// [`QUIET_LIMIT`].
    #[must_use]
    pub fn with_quiet(mut self, quiet: Duration) -> Self {
        self.quiet = quiet;
        self
    }

    /// The daemon with the free memory of `memory`, for a test.
    #[must_use]
    pub fn with_memory(mut self, memory: crate::room::Memory) -> Self {
        self.room = Room::new(self.dir.clone(), memory);
        self
    }

    /// The daemon that binds `riff` into each session in place of its
    /// own binary, for a test.
    #[must_use]
    pub fn with_relay(mut self, riff: PathBuf) -> Self {
        self.relay_bin = riff;
        self
    }

    /// The daemon with the forge and the riff of `world` for its waves,
    /// for a test.
    #[must_use]
    pub fn with_world(mut self, world: Arc<dyn World>) -> Self {
        self.world = world;
        self
    }

    /// The state of riff on this machine. The riff tools of a session
    /// act on riff-server 1.x until peer to peer (Wave B).
    pub fn state(&self) -> &Local {
        &self.state
    }

    /// The folder of the daemon.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The limit of the host and the line of the starts that wait.
    pub fn room(&self) -> &Arc<Room> {
        &self.room
    }

    /// The forge and the riff of the waves.
    pub(crate) fn world(&self) -> &Arc<dyn World> {
        &self.world
    }

    /// The number of sessions that the daemon owns, but the sessions
    /// with a terminal: the lead counts in no limit.
    pub fn live_count(&self) -> usize {
        self.lock()
            .values()
            .filter(|owned| !owned.lock().record.terminal)
            .count()
    }

    /// The record of each session that the daemon owns.
    pub fn records(&self) -> Vec<Record> {
        self.lock()
            .values()
            .map(|owned| owned.lock().record.clone())
            .collect()
    }

    /// Waits until a session ends, or a job of a wave ends.
    pub(crate) async fn changed(&self) {
        self.changed.notified().await;
    }

    /// Tells the waves and the starts that wait: a session or a job
    /// ended.
    pub(crate) fn tell_changed(&self) {
        self.changed.notify_waiters();
        self.room.ended();
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
            waits: None,
            key: Some(self.key.public().to_string()),
            riff,
            owner: Some(self.owner.clone()),
            position,
            sessions: self
                .lock()
                .values()
                .map(|owned| owned.lock().live())
                .collect(),
            limit: Some(self.room.limit()),
            waiting: self.room.waiting(),
            waves: self.waves_shown(),
            jobs: self
                .waves_lock()
                .values()
                .map(|running| {
                    running
                        .taken
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .len()
                })
                .sum(),
        }
    }

    /// Asks for a new start of the daemon with `restart` when it runs an
    /// old riff (`old`) and is idle ([`Status::busy`] is 0,
    /// 01M517BTP6BAGTRBYWPX9GPVE0). It looks at the end of each session
    /// and of each job, and each [`OLD_LOOK`]. It asks once, then ends.
    pub async fn restart_when_old(
        &self,
        old: impl Fn() -> bool,
        restart: impl FnOnce() -> Result<()>,
    ) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.status().await.busy() == 0 && old() {
                println!("{}", text::DAEMON_NEW_RIFF);
                if let Err(e) = restart() {
                    eprintln!(
                        "riff daemon: {}",
                        text::daemon_no_restart(&format!("{e:#}"))
                    );
                }
                return;
            }
            tokio::select! {
                () = &mut changed => {}
                () = tokio::time::sleep(OLD_LOOK) => {}
            }
        }
    }

    /// Each wave that the daemon runs, with what it did last.
    pub fn waves_shown(&self) -> Vec<WaveShown> {
        self.waves_lock()
            .iter()
            .map(|(repo, running)| WaveShown {
                repo: repo.clone(),
                word: running
                    .word
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone(),
            })
            .collect()
    }

    fn waves_lock(&self) -> MutexGuard<'_, BTreeMap<String, Running>> {
        self.waves.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs the wave `wave`: it keeps it in [`crate::wave::WAVES_FILE`],
    /// and starts its loop when it does not run.
    pub fn run_wave(self: &Arc<Self>, wave: Wave) -> Result<()> {
        let mut waves = crate::wave::read_waves(&self.dir)?;
        waves.insert(wave.repo.clone(), wave.clone());
        crate::wave::write_waves(&self.dir, &waves)?;
        self.spawn_wave(wave);
        Ok(())
    }

    /// Starts the loop of `wave`, when it does not run.
    /// A loop that runs with other facts of the wave, for example
    /// another main clone, starts again with the new facts. Its jobs go
    /// on.
    fn spawn_wave(self: &Arc<Self>, wave: Wave) {
        let mut running = self.waves_lock();
        let taken = match running.get(&wave.repo) {
            Some(old) if old.wave == wave && !old.task.is_finished() => return,
            Some(old) => {
                old.task.abort();
                old.taken.clone()
            }
            None => Arc::new(Mutex::new(std::collections::BTreeSet::new())),
        };
        let word = Arc::new(Mutex::new(text::wave_starts()));
        let task = tokio::spawn(crate::wave::run(
            self.clone(),
            wave.clone(),
            taken.clone(),
            word.clone(),
        ));
        running.insert(
            wave.repo.clone(),
            Running {
                wave,
                task,
                taken,
                word,
            },
        );
    }

    /// Starts no new session for the wave of `repo`. The sessions that
    /// run go on. False when the daemon did not run that wave.
    pub fn stop_wave(&self, repo: &str) -> Result<bool> {
        let mut waves = crate::wave::read_waves(&self.dir)?;
        let kept = waves.remove(repo).is_some();
        crate::wave::write_waves(&self.dir, &waves)?;
        let running = self.waves_lock().remove(repo);
        if let Some(running) = &running {
            running.task.abort();
        }
        Ok(kept || running.is_some())
    }

    /// Starts the loop of each wave of [`crate::wave::WAVES_FILE`]. Call
    /// it after [`Daemon::recover`].
    pub fn run_waves(self: &Arc<Self>) {
        match crate::wave::read_waves(&self.dir) {
            Ok(waves) => {
                for wave in waves.into_values() {
                    println!("{}", text::wave_runs(&wave.repo));
                    self.spawn_wave(wave);
                }
            }
            Err(e) => eprintln!("riff daemon: {e:#}"),
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
                Ok(Ask::Wave { wave }) => {
                    let repo = wave.repo.clone();
                    out.say(match self.run_wave(wave) {
                        Ok(()) => {
                            println!("{}", text::wave_runs(&repo));
                            Say::Done
                        }
                        Err(e) => Say::Failed {
                            error: format!("{e:#}"),
                        },
                    });
                }
                Ok(Ask::WaveStop { repo }) => out.say(match self.stop_wave(&repo) {
                    Ok(true) => {
                        println!("{}", text::wave_stops(&repo));
                        Say::Done
                    }
                    Ok(false) => Say::Failed {
                        error: text::wave_not_run(&repo),
                    },
                    Err(e) => Say::Failed {
                        error: format!("{e:#}"),
                    },
                }),
                Ok(Ask::Takeover { name }) => self.takeover(&name, lines, &out).await,
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
        let shown = |text: String| out.say(Say::Line { text });
        out.say(match self.have_image(image, shown).await {
            Ok(()) => Say::Done,
            Err(e) => Say::Failed {
                error: format!("{e:#}"),
            },
        });
    }

    /// Has the image `image` on this machine: it pulls it when it is not
    /// here, and gives `show` the line of the pull.
    pub(crate) async fn have_image(&self, image: &str, show: impl FnOnce(String)) -> Result<()> {
        if !self.runtime.has_image(image).await? {
            show(text::run_pulling(image));
            self.runtime
                .pull(image)
                .await
                .map_err(|e| anyhow!("{}: {e:#}", text::run_no_image(image)))?;
        }
        Ok(())
    }

    /// Refuses to start a session when the binary that the daemon binds
    /// at [`RELAY_BIN`] is not static (01M510NW8YEVMQKD6RWJPW38N3). It
    /// reads the file each time, so a `riff update` needs no restart of
    /// the daemon. [`crate::release::DYNAMIC`] takes each binary.
    pub fn check_relay(&self) -> Result<()> {
        if self.any_relay {
            return Ok(());
        }
        crate::release::check_file(&self.relay_bin)
            .map_err(|why| anyhow!("{}", text::daemon_relay_not_static(&self.relay_bin, &why)))
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
                last: tokio::time::Instant::now(),
                log,
                followers: Vec::new(),
                relay: None,
                relay_protocol: 0,
                held: false,
                taker: None,
                last_step: String::new(),
                ended: false,
            }),
            notes,
            stop: Notify::new(),
            closing: Notify::new(),
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
            .save(&owned.lock())
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

    /// Starts the session of `start` when the host has room
    /// ([`crate::room`]), says its events on `out`, and waits for its
    /// end. A [`Then::Stop`] on `from` stops it. A stop, or the end of
    /// `from`, while the start waits for room starts nothing.
    async fn start(
        self: &Arc<Self>,
        start: Start,
        mut from: Lines<BufReader<OwnedReadHalf>>,
        out: &Out,
    ) {
        let name = start.spec.name.clone();
        // The lead waits for no room: it runs no build of an item.
        if start.terminal {
            self.launch(start, None, None, Some(from), out).await;
            return;
        }
        let slot = {
            let admit = self.room.admit(
                &name,
                || self.live_count(),
                |full| {
                    out.say(Say::Line {
                        text: text::daemon_waits(&name, full),
                    });
                },
            );
            tokio::pin!(admit);
            loop {
                tokio::select! {
                    slot = &mut admit => break slot,
                    line = from.next_line() => match line {
                        Ok(Some(line)) if !matches!(serde_json::from_str::<Then>(&line), Ok(Then::Stop)) => {}
                        _ => {
                            out.say(Say::Failed {
                                error: text::daemon_no_start(&name),
                            });
                            return;
                        }
                    },
                }
            }
        };
        self.launch(start, None, Some(slot), Some(from), out).await;
    }

    /// Starts the session of `start` in the room of `slot`, with the job
    /// `job` of a wave, says its events on `out`, and waits for its end.
    /// A [`Then::Stop`] on `from` stops it. It gives the record of the
    /// session. A session with a terminal takes no slot: it says
    /// [`Say::Done`] when its container runs.
    pub(crate) async fn launch(
        self: &Arc<Self>,
        start: Start,
        job: Option<Job>,
        slot: Option<Slot>,
        from: Option<Lines<BufReader<OwnedReadHalf>>>,
        out: &Out,
    ) -> Record {
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
            terminal,
        } = start;
        let name = spec.name.clone();
        let record = Record {
            live: Live {
                name: name.clone(),
                session: session.clone(),
                item: item.clone(),
                image: spec.image.clone(),
                since_ms: now_ms(),
                phase: Phase::default(),
                last: String::new(),
            },
            folder,
            server,
            me,
            inputs: if terminal {
                Vec::new()
            } else {
                vec![user_line(&start)]
            },
            closed: false,
            failed: false,
            job,
            terminal,
            takeover: false,
        };
        if let Err(e) = self.check(&spec).and_then(|()| self.check_relay()) {
            out.say(Say::Failed {
                error: format!("{e:#}"),
            });
            return record;
        }
        let scrub = Scrub::new(secrets);
        let (owned, notes) = match self.own(record.clone(), scrub.clone(), false) {
            Ok(owned) => owned,
            Err(e) => {
                out.say(Say::Failed {
                    error: format!("{e:#}"),
                });
                return record;
            }
        };
        // The daemon owns the session now: it counts as live, and its
        // slot goes.
        drop(slot);
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
        let relay: &[&str] = if terminal {
            &[RELAY_BIN, "relay", "--terminal", "--"]
        } else {
            &[RELAY_BIN, "relay", "--"]
        };
        let mut command: Vec<String> = relay.iter().map(|a| (*a).to_owned()).collect();
        command.append(&mut spec.command);
        spec.command = command;
        println!("{}", text::daemon_session_starts(&name, &item, &session));
        out.say(Say::Started {
            name: name.clone(),
            socket: owned.socket.clone(),
        });
        if terminal {
            let end = match self.runtime.start_terminal(&spec, &values).await {
                Ok(end) => end,
                Err(e) => {
                    self.close(&owned);
                    println!("{}", text::daemon_session_failed(&name));
                    out.say(Say::Failed {
                        error: format!("{e:#}"),
                    });
                    return record;
                }
            };
            self.world.resume(&record);
            out.say(Say::Done);
            let reader = from.map(|from| stop_on(from, owned.clone()));
            self.clone().watch(owned, notes, Some(end)).await;
            if let Some(reader) = reader {
                reader.abort();
            }
            return record;
        }
        let running = match self.runtime.start(&spec, &values).await {
            Ok(running) => running,
            Err(e) => {
                self.close(&owned);
                println!("{}", text::daemon_session_failed(&name));
                out.say(Say::Failed {
                    error: format!("{e:#}"),
                });
                return record;
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
        let reader = from.map(|from| stop_on(from, owned.clone()));
        // The input of the container stays open until the end: the relay
        // gives claude its input.
        let input = running.input;
        self.clone().watch(owned, notes, Some(running.end)).await;
        drop(input);
        if let Some(reader) = reader {
            reader.abort();
        }
        record
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
    /// end of the relay in a container that does not run. After the
    /// input of `claude` closed, it stops the container when no event
    /// comes for the quiet limit, and says why
    /// (01M4ZSZX77AJDGWNQMGBPV2QXG).
    async fn watch(
        self: Arc<Self>,
        owned: Arc<Owned>,
        mut notes: mpsc::UnboundedReceiver<Note>,
        end: Option<End>,
    ) {
        let mut end = end.map(|end| Box::pin(end.wait()));
        let mut stopped = false;
        let (code, exited) = loop {
            // Each event moves the end of the quiet: a new turn after a
            // task of claude ended is no quiet.
            let quiet_at = {
                let state = owned.lock();
                (state.record.closed && !state.record.takeover).then(|| state.last + self.quiet)
            };
            tokio::select! {
                () = owned.closing.notified() => {}
                () = tokio::time::sleep_until(quiet_at.unwrap_or_else(tokio::time::Instant::now)),
                    if quiet_at.is_some() && !stopped =>
                {
                    let quiet = {
                        let state = owned.lock();
                        state.last.elapsed() >= self.quiet && !state.record.takeover
                    };
                    if quiet {
                        stopped = true;
                        let line = text::daemon_quiet(&owned.name, self.quiet);
                        println!("{line}");
                        Owned::tell(&mut owned.lock(), &Say::Line { text: line });
                        let _ = self.runtime.stop(&owned.name).await;
                    }
                }
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
        let (failed, record) = {
            let state = owned.lock();
            (state.record.failed, state.record.clone())
        };
        let ended = Ended {
            code,
            failed: failed || stopped || !exited,
        };
        println!("{}", text::daemon_session_ends(&owned.name, code));
        if record.terminal {
            let world = self.world.clone();
            tokio::spawn(async move { world.leave(&record).await });
        }
        let followers = self.close(&owned);
        for out in followers {
            out.say(Say::Ended(ended));
        }
    }

    /// Ends the ownership of the session `owned`: its socket, its folder
    /// and its record go. It gives the connections that followed it.
    fn close(&self, owned: &Owned) -> Vec<Out> {
        // From now on, no step writes its record again.
        owned.lock().ended = true;
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
        self.tell_changed();
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
            Ok(SessionAsk::Relay {
                protocol,
                inputs,
                closed,
                held,
            }) => {
                owned
                    .relay(protocol, (inputs, closed, held), lines, write)
                    .await;
            }
            Ok(SessionAsk::Mcp { client, .. }) => self.mcp(&owned, client, lines, write).await,
            Ok(SessionAsk::Watch { .. }) => {
                let (out, written) = writer(write);
                let record = owned.lock().record.clone();
                let wake = self.world.wake(&record);
                // The wait ends when the relay leaves.
                let gone = async { while let Ok(Some(_)) = lines.next_line().await {} };
                tokio::select! {
                    woke = wake => out.say(match woke {
                        Ok(text) => Say::Line { text },
                        Err(e) => Say::Failed { error: format!("{e:#}") },
                    }),
                    () = gone => {}
                }
                drop(out);
                let _ = written.await;
            }
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
    /// worker session of `riff run` or of a wave, which holds the claim
    /// of the item. The session gets only the tools that act on
    /// riff-server, and on no folder of the host and no claim
    /// ([`crate::mcp::CONTAINER_TOOLS`], 01M4ZV3B6T2KB09WJ59F07HXET).
    pub fn tools_of(record: &Record) -> Result<crate::mcp::Tools> {
        let api = crate::api::Api::new(&record.server).signed_in(Some(&record.live.session))?;
        let tools = crate::mcp::Tools::new(api, record.me.clone()).as_worker(!record.terminal);
        Ok(if record.terminal {
            tools.in_lead_container()
        } else {
            tools.in_container()
        })
    }

    /// Takes over the live session `name` for the person of `out`
    /// (01M4Z60B83ZH10HYKW7X1C0SSS): see "A takeover" in the design. It
    /// gives the session back at a [`Then::GoOn`] on `from`, or at the
    /// end of `from`.
    async fn takeover(&self, name: &str, mut from: Lines<BufReader<OwnedReadHalf>>, out: &Out) {
        let Some(owned) = self.lock().get(name).cloned() else {
            out.say(Say::Failed {
                error: text::daemon_no_session(name),
            });
            return;
        };
        {
            let mut state = owned.lock();
            let refused = if state.record.terminal {
                Some(text::takeover_terminal(name))
            } else if state.taker.as_ref().is_some_and(Out::open) {
                Some(text::takeover_runs(name))
            } else if !state.record.takeover
                && (state.relay.is_none() || state.relay_protocol < TAKEOVER_PROTOCOL)
            {
                Some(text::takeover_no_relay(name))
            } else {
                None
            };
            if let Some(error) = refused {
                out.say(Say::Failed { error });
                return;
            }
            state.taker = Some(out.clone());
            state.followers.push(out.clone());
            if state.held && state.record.takeover {
                out.say(Say::Held {
                    session: state.record.live.session.clone(),
                });
            } else {
                if !state.record.takeover {
                    state.record.takeover = true;
                    if let Err(e) = owned.save(&state) {
                        eprintln!("riff daemon: {e:#}");
                    }
                    if let Some(relay) = &state.relay {
                        relay.say(Say::Hold);
                    }
                }
                out.say(Say::Line {
                    text: text::takeover_waits(name),
                });
            }
        }
        println!("{}", text::takeover_asked(name));
        loop {
            match from.next_line().await {
                Ok(Some(line)) if matches!(serde_json::from_str::<Then>(&line), Ok(Then::GoOn)) => {
                    break;
                }
                Ok(Some(_)) => {}
                _ => break,
            }
        }
        let mut state = owned.lock();
        if state.taker.as_ref().is_some_and(|t| t.same(out)) {
            state.taker = None;
        }
        // A session that ended has no record to give back.
        if state.ended {
            return;
        }
        state.record.takeover = false;
        state.go_on();
        if let Err(e) = owned.save(&state) {
            eprintln!("riff daemon: {e:#}");
        }
        println!("{}", text::takeover_ended(name));
        out.say(Say::Done);
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
                if record.terminal {
                    let world = self.world.clone();
                    let record = record.clone();
                    tokio::spawn(async move { world.leave(&record).await });
                }
                // The job of a wave does its steps after the end now:
                // the session ended while no daemon ran.
                if record.job.is_some() {
                    let ended = Ended {
                        code: None,
                        failed: true,
                    };
                    tokio::spawn(crate::wave::finish(self.clone(), record, ended));
                }
                continue;
            }
            match self.own(record.clone(), Scrub::default(), true) {
                Ok((owned, notes)) => {
                    println!("{}", text::daemon_takes_up(&name));
                    // The lead keeps its session on riff-server alive.
                    if record.terminal {
                        self.world.resume(&record);
                    }
                    // The job of a wave goes on: the keep-alive of its
                    // claim, and its steps after the end.
                    if record.job.is_some() {
                        let (out, said) = Out::keep();
                        owned.lock().followers.push(out);
                        self.world.resume(&record);
                        tokio::spawn(crate::wave::after(self.clone(), record, said));
                    }
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

    /// Tells the daemon that the terminal of the takeover of this
    /// connection ended: the session goes on.
    pub async fn go_on(&mut self) -> Result<()> {
        self.line(&Then::GoOn).await
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
pub async fn image(dir: &Path, image: &str, show: impl FnMut(&str)) -> Result<()> {
    let ask = Ask::Image {
        image: image.into(),
    };
    ask_done(dir, ask, show).await
}

/// Sends `ask` to the daemon in the folder `dir`, shows each line of
/// the daemon with `show`, and waits for [`Say::Done`].
pub async fn ask_done(dir: &Path, ask: Ask, mut show: impl FnMut(&str)) -> Result<()> {
    let mut client = Client::connect(dir).await?;
    client.ask(ask).await?;
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
    let mut view = crate::run::View::default();
    loop {
        match client.next().await? {
            Some(Say::Event { line, .. }) => view.show(&line),
            Some(Say::Stderr { line }) => eprintln!("{line}"),
            Some(Say::Line { text }) => println!("{text}"),
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

/// A daemon that opens: it waits for its key, or loads its state. It
/// answers each connection with what it waits for
/// (01M51JW233MXB72MHHRAX4E4NF).
#[cfg(target_os = "linux")]
struct Opening {
    dir: PathBuf,
    since_ms: i64,
    /// The last line of the key of the daemon.
    waits: Mutex<String>,
}

#[cfg(target_os = "linux")]
impl Opening {
    fn new(dir: PathBuf) -> Opening {
        Opening {
            dir,
            since_ms: now_ms(),
            waits: Mutex::new(text::DAEMON_OPENS.into()),
        }
    }

    /// The daemon said `line` of its key.
    fn waits(&self, line: String) {
        *self.waits.lock().unwrap_or_else(PoisonError::into_inner) = line;
    }

    /// Answers each connection on `listener`: [`Ask::Status`] with
    /// [`Status::opening`], each other ask with a failure that says what
    /// the daemon waits for. It does not end.
    async fn answer(self: &Arc<Self>, listener: &UnixListener) {
        loop {
            let Ok((conn, _)) = listener.accept().await else {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            let opening = self.clone();
            tokio::spawn(async move {
                let (read, write) = conn.into_split();
                let mut lines = BufReader::new(read).lines();
                let (out, written) = writer(write);
                if let Ok(Some(first)) = lines.next_line().await {
                    let waits = opening
                        .waits
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone();
                    out.say(match hello(&first) {
                        Ok(Ask::Status) => Say::Status(Status::opening(
                            std::process::id(),
                            opening.dir.clone(),
                            opening.since_ms,
                            waits,
                        )),
                        Ok(_) => Say::Failed { error: waits },
                        Err(error) => Say::Failed { error },
                    });
                }
                drop(out);
                let _ = written.await;
            });
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
        // While it opens, the daemon answers with what it waits for
        // (01M51JW233MXB72MHHRAX4E4NF).
        let opening = Arc::new(Opening::new(dir.clone()));
        let say = {
            let opening = opening.clone();
            move |line: String| {
                println!("{line}");
                opening.waits(line);
            }
        };
        let open = Daemon::open_with(crate::podman::Podman, dir.clone(), crate::secrets::retry(), say);
        let opened = tokio::select! {
            opened = open => Some(opened?),
            () = opening.answer(&listener) => None,
            _ = term.recv() => None,
            _ = int.recv() => None,
        };
        let Some(daemon) = opened else {
            let _ = std::fs::remove_file(socket(&dir));
            println!("{}", text::DAEMON_STOPS);
            return Ok(());
        };
        let daemon = Arc::new(daemon);
        let status = daemon.status().await;
        println!(
            "{}",
            text::daemon_listens(&socket(&dir), status.pid, &status.build)
        );
        daemon.recover().await;
        daemon.run_waves();
        // A new riff on disk (01M517BTP6BAGTRBYWPX9GPVE0).
        if let Ok(manager) = crate::service::Systemd::here() {
            let daemon = daemon.clone();
            tokio::spawn(async move {
                let old = || old_here(&manager);
                let restart = || crate::service::ServiceManager::try_restart(&manager);
                daemon.restart_when_old(old, restart).await;
            });
        }
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

/// `riff daemon uninstall`: stops the daemon, and removes its unit with
/// `manager` (01M501M0T0JBWGMMHCT7HT95T9). It keeps the folder of the
/// daemon and each container. It gives the line for the person.
pub fn uninstall(
    manager: &impl crate::service::ServiceManager,
    unit_path: &Path,
) -> Result<String> {
    if crate::home::dir().is_some() {
        bail!("{}", text::DAEMON_UNINSTALL_RIFF_HOME);
    }
    Ok(match manager.uninstall()? {
        Some(path) => text::daemon_uninstalled(&path),
        None => text::daemon_not_installed(unit_path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::HIDDEN;
    use crate::runtime::{Exit, Fake};
    use crate::service::{ServiceManager, Unit};
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
        quiet: Duration,
        world: Option<Arc<dyn World>>,
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

    /// A static riff beside the folder `dir` of a daemon, for its
    /// sessions: the riff of a test is not static.
    fn static_riff(dir: &Path) -> PathBuf {
        crate::release::fake_riff(&dir.with_file_name("static-riff"), None)
    }

    /// The riff-server of the daemon is `world` when it is given, else
    /// the riff-server of this machine.
    async fn serve_in(
        fake: &Fake,
        dir: &Path,
        quiet: Duration,
        world: Option<Arc<dyn World>>,
    ) -> Served {
        let listener = listen(dir).await.unwrap();
        crate::secrets::mock_store();
        let mut daemon = Daemon::open(fake.clone(), dir.to_owned())
            .await
            .unwrap()
            .with_quiet(quiet)
            .with_relay(static_riff(dir));
        if let Some(world) = world {
            daemon = daemon.with_world(world);
        }
        let daemon = Arc::new(daemon);
        let (stop, stopped) = oneshot::channel::<()>();
        let served = tokio::spawn(daemon.clone().serve(listener, async {
            let _ = stopped.await;
        }));
        (daemon, stop, served)
    }

    impl Here {
        async fn new(fake: Fake) -> Here {
            Here::with_quiet(fake, QUIET_LIMIT).await
        }

        /// A daemon on `fake` with the quiet limit `quiet`.
        async fn with_quiet(fake: Fake, quiet: Duration) -> Here {
            Here::with_world(fake, quiet, None).await
        }

        /// A daemon on `fake` with the quiet limit `quiet` and the
        /// riff-server of `world`.
        async fn with_world(fake: Fake, quiet: Duration, world: Option<Arc<dyn World>>) -> Here {
            let root = tempfile::tempdir().unwrap();
            let dir = dir_in(root.path());
            let (daemon, stop, served) = serve_in(&fake, &dir, quiet, world.clone()).await;
            Here {
                root,
                dir,
                fake,
                quiet,
                world,
                daemon,
                stop: Some(stop),
                served,
            }
        }

        /// Stops the daemon, and starts a new one in the same folder.
        async fn restart(&mut self) {
            self.stop().await;
            let (daemon, stop, served) =
                serve_in(&self.fake, &self.dir, self.quiet, self.world.clone()).await;
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
                terminal: false,
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
            Relay::connect_as(
                dir,
                SessionAsk::Relay {
                    protocol: PROTOCOL,
                    inputs,
                    closed,
                    held: false,
                },
            )
            .await
        }

        /// Connects with the hello `hello`, and gives the resume.
        async fn connect_as(dir: &Path, hello: SessionAsk) -> (Relay, u64) {
            let conn = connect_in(dir, SOCKET).await.unwrap();
            let (read, write) = conn.into_split();
            let mut relay = Relay {
                lines: BufReader::new(read).lines(),
                write,
            };
            relay.send(&hello).await;
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
        // The relay is the entrypoint: the riff of the daemon, with no
        // write, and the command of the spec after it. The riff of the
        // test daemon is a static fake.
        let spec = here.fake.spec(&name).unwrap();
        assert_eq!(spec.command, ["/opt/riff/riff", "relay", "--", "claude"]);
        assert!(spec.binds.contains(&Bind {
            source: Source::Path(static_riff(&here.dir)),
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

    /// The daemon acks no line that it lacks: it closes the connection,
    /// and the relay sends the line again at the next one
    /// (01M4ZHSZMH4Y8CB41FYGE3C3R2).
    #[tokio::test]
    async fn the_daemon_acks_no_line_that_it_lacks_and_the_relay_sends_it_again() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (mut client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let socket = session_dir(&here.dir, &name);
        let (mut relay, _) = Relay::connect(&socket, 0, false).await;
        assert!(matches!(relay.next().await, Say::Input { .. }));
        relay.event(1, TURN[0]).await;

        // An exit before the events 2 and 3.
        relay
            .send(&Up::Exit {
                seq: 4,
                code: Some(0),
            })
            .await;
        assert_eq!(relay.lines.next_line().await.unwrap(), None);
        // An event after a gap.
        let (mut relay, had) = Relay::connect(&socket, 1, false).await;
        assert_eq!(had, 1);
        relay
            .send(&Up::Event {
                seq: 3,
                line: TURN[2].into(),
            })
            .await;
        assert_eq!(relay.lines.next_line().await.unwrap(), None);

        let (mut relay, had) = Relay::connect(&socket, 1, false).await;
        assert_eq!(had, 1);
        relay.event(2, TURN[1]).await;
        relay.event(3, TURN[2]).await;
        relay.exit(4, 0).await;
        let (said, end) = to_the_end(&mut client).await;
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

    /// An event of `claude`: one background task runs.
    const ONE_TASK: &str = r#"{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"b1","task_type":"local_bash","description":"wait for just check"}]}"#;

    /// After the result, no event comes for the quiet limit: the daemon
    /// stops the container, and says why (01M4ZSZX77AJDGWNQMGBPV2QXG).
    #[tokio::test]
    async fn the_daemon_stops_a_session_with_no_event_for_the_quiet_limit_after_the_result() {
        let quiet = Duration::from_millis(500);
        let here = Here::with_quiet(Fake::default().with_image(IMAGE), quiet).await;
        let (mut client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let (mut relay, _) = Relay::connect(&session_dir(&here.dir, &name), 1, false).await;
        // No stop while the input is open: a turn can be quiet for long.
        relay.event(1, TURN[0]).await;
        tokio::time::sleep(quiet * 2).await;
        assert_eq!(here.fake.calls(), [format!("start {name}")]);

        relay.event(2, TURN[2]).await;
        let closed = tokio::time::Instant::now();
        relay.event(3, ONE_TASK).await;
        let (said, end) = to_the_end(&mut client).await;
        assert!(closed.elapsed() >= quiet);
        assert!(
            said.contains(&Say::Line {
                text: text::daemon_quiet(&name, quiet)
            }),
            "{said:?}"
        );
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: None,
                failed: true
            }))
        );
        assert_eq!(
            here.fake.calls(),
            [format!("start {name}"), format!("stop {name}")]
        );
        assert!(!record_path(&here.dir, &name).exists());
    }

    /// Each event after the result moves the end of the quiet: a new
    /// turn after the end of a background task is no quiet.
    #[tokio::test]
    async fn each_event_after_the_result_moves_the_end_of_the_quiet() {
        let quiet = Duration::from_millis(1000);
        let here = Here::with_quiet(Fake::default().with_image(IMAGE), quiet).await;
        let (mut client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let (mut relay, _) = Relay::connect(&session_dir(&here.dir, &name), 1, false).await;
        relay.event(1, TURN[0]).await;
        relay.event(2, ONE_TASK).await;
        relay.event(3, TURN[2]).await;
        // Two times the quiet limit, with an event each fifth of it.
        let mut seq = 3;
        for _ in 0..10 {
            tokio::time::sleep(quiet / 5).await;
            seq += 1;
            relay.event(seq, TURN[0]).await;
        }
        relay.exit(seq + 1, 0).await;
        let (said, end) = to_the_end(&mut client).await;
        assert_eq!(events(&said).len(), 13);
        assert!(
            !said.iter().any(|s| matches!(s, Say::Line { .. })),
            "{said:?}"
        );
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: false
            }))
        );
        assert_eq!(here.fake.calls(), [format!("start {name}")]);
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

    /// 01M510NW8YEVMQKD6RWJPW38N3: the daemon starts no container with a
    /// riff that is not static, and names `riff update`. After the
    /// update, the next start works with no restart of the daemon.
    #[tokio::test]
    async fn the_daemon_starts_no_container_with_a_riff_that_is_not_static() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let riff = static_riff(&here.dir);
        crate::release::fake_riff(&riff, Some("/lib64/ld-linux-x86-64.so.2"));
        let mut client = Client::connect(&here.dir).await.unwrap();
        let start = here.start("riff-issue-1-aaaaaaaa", "issue-1");
        client.ask(Ask::Start(Box::new(start))).await.unwrap();
        match client.next().await.unwrap() {
            Some(Say::Failed { error }) => {
                assert!(
                    error.starts_with(&format!(
                        "The riff daemon starts no session with {}: it needs the shared \
                         libraries of /lib64/ld-linux-x86-64.so.2: it is not static.",
                        riff.display()
                    )),
                    "{error}"
                );
                assert!(error.contains("Run riff update"), "{error}");
            }
            other => panic!("{other:?}"),
        }
        assert!(here.fake.calls().is_empty(), "{:?}", here.fake.calls());
        assert!(here.daemon.status().await.sessions.is_empty());

        // riff update puts the static riff at the same path.
        crate::release::fake_riff(&riff, None);
        let (_client, name) = here
            .started(here.start("riff-issue-1-aaaaaaaa", "issue-1"))
            .await;
        assert_eq!(name, "riff-issue-1-aaaaaaaa");

        // A daemon binds its own binary: the test binary, which is not
        // static with glibc.
        let root = tempfile::tempdir().unwrap();
        crate::secrets::mock_store();
        let own = Daemon::open(Fake::default(), dir_in(root.path()))
            .await
            .unwrap();
        assert_eq!(own.relay_bin, own_binary().unwrap());
        if cfg!(target_env = "gnu") && !crate::release::takes_dynamic() {
            let error = own.check_relay().unwrap_err().to_string();
            assert!(error.contains("Run riff update"), "{error}");
        }
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

    /// When podman cannot say whether the container of a record runs, a
    /// new daemon takes up the session (01M4ZHT01HH88KPA8628M7VBEY).
    #[tokio::test]
    async fn a_new_daemon_takes_up_a_session_when_podman_cannot_say() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let name = "riff-issue-1-aaaaaaaa";
        let folder = root.path().join("sessions/issue-1");
        std::fs::create_dir_all(&folder).unwrap();
        private_dir(&dir.join(LIVE)).unwrap();
        let record = Record {
            live: Live {
                name: name.into(),
                session: format!("{name}-session"),
                item: "issue-1".into(),
                image: IMAGE.into(),
                since_ms: 0,
                phase: Phase::default(),
                last: String::new(),
            },
            folder,
            server: "http://127.0.0.1:1".into(),
            me: me(name),
            inputs: vec![user_line("Do it.")],
            closed: false,
            failed: false,
            job: None,
            terminal: false,
            takeover: false,
        };
        std::fs::write(
            record_path(&dir, name),
            serde_json::to_string(&record).unwrap(),
        )
        .unwrap();
        private_dir(&session_dir(&dir, name)).unwrap();

        let fake = Fake::default().with_image(IMAGE).with_no_answer();
        let listener = listen(&dir).await.unwrap();
        crate::secrets::mock_store();
        let daemon = Arc::new(Daemon::open(fake, dir.clone()).await.unwrap());
        daemon.recover().await;
        tokio::spawn(daemon.serve(listener, std::future::pending()));
        let names: Vec<String> = status(&dir)
            .await
            .unwrap()
            .sessions
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, [name]);
        assert!(record_path(&dir, name).exists());
        let (_relay, had) = Relay::connect(&session_dir(&dir, name), 1, false).await;
        assert_eq!(had, 0);
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
                terminal: false,
            };
            let record = Record {
                live: Live {
                    name: name.into(),
                    session: start.session.clone(),
                    item: item.into(),
                    image: IMAGE.into(),
                    since_ms: 0,
                    phase: Phase::default(),
                    last: String::new(),
                },
                folder: folder.clone(),
                server: start.server.clone(),
                me: start.me.clone(),
                inputs: vec![user_line("Do it.")],
                closed: false,
                failed: false,
                job: None,
                terminal: false,
                takeover: false,
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
        crate::secrets::mock_store();
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
                phase: Phase::default(),
                last: String::new(),
            },
            folder: folder.clone(),
            server: "http://127.0.0.1:1".into(),
            me: me(name),
            inputs: vec![],
            closed: false,
            failed: false,
            job: None,
            terminal: false,
            takeover: false,
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
        crate::secrets::mock_store();
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
            terminal: false,
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
                closed: false,
                held: false,
            })
            .unwrap(),
            r#"{"ask":"relay","protocol":2,"inputs":1,"closed":false,"held":false}"#
        );
        // The hello of a relay of protocol 3 has no held.
        assert_eq!(
            serde_json::from_str::<SessionAsk>(
                r#"{"ask":"relay","protocol":3,"inputs":1,"closed":true}"#
            )
            .unwrap(),
            SessionAsk::Relay {
                protocol: 3,
                inputs: 1,
                closed: true,
                held: false,
            }
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

    /// 01M519Z2JMAKN0T6JZFBPSTKC8: the first start makes the riff; a
    /// stop writes a checkpoint; the next start loads it, with the riff
    /// and the claims.
    #[tokio::test]
    async fn the_daemon_keeps_its_state_across_a_restart() {
        let mut here = Here::new(Fake::default()).await;
        let first = status(&here.dir).await.unwrap();
        let riff = first.riff.clone().unwrap();
        assert_eq!(riff.len(), 64);
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
        crate::secrets::mock_store();
        let daemon = Daemon::open(Fake::default(), dir.clone()).await.unwrap();
        claim(&daemon, "issue-12").await;
        let before = daemon.status().await;
        drop(daemon);

        let store = Dir::new(state_dir(&dir));
        assert!(checkpoint::names(&store).await.unwrap().is_empty());
        crate::secrets::mock_store();
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
        crate::secrets::mock_store();
        Daemon::open(Fake::default(), dir.clone()).await.unwrap();
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        let chunks = std::fs::read_dir(state_dir(&dir).join("log")).unwrap();
        assert_eq!(chunks.count(), 1);
    }

    /// A daemon in `dir` on a fake runtime, with the mock keyring. It
    /// gives the lines of its key.
    async fn open_key(dir: &Path) -> (Daemon<Fake>, Vec<String>) {
        crate::secrets::mock_store();
        let mut lines = Vec::new();
        let say = |line| lines.push(line);
        let daemon = Daemon::open_with(Fake::default(), dir.to_owned(), Duration::from_millis(10), say)
            .await
            .unwrap();
        (daemon, lines)
    }

    /// The public key of the secret of the daemon in `dir`.
    fn kept_key(dir: &Path) -> Option<String> {
        let name = crate::daemon_key::secret_name(dir);
        match crate::daemon_key::read(&name).unwrap() {
            Found::Key(key) => Some(key.public().to_string()),
            _ => None,
        }
    }

    /// The folders `state.lost-MS` of the daemon in `dir`.
    fn lost(dir: &Path) -> Vec<PathBuf> {
        let mut lost: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.to_string_lossy().contains("state.lost-"))
            .collect();
        lost.sort();
        lost
    }

    /// 01M519Z2BB0YJY5CRMB54JVYFA, 01M519Z2JMAKN0T6JZFBPSTKC8,
    /// 01M519Z2N2PMZJ0CYAPV0A47BZ: the first start makes a key in the
    /// keyring (the mock) and a riff whose ID is the public key, of the
    /// owner USER. A second start loads the same key and the same riff.
    #[tokio::test]
    async fn the_first_start_makes_a_key_and_a_riff_of_its_own() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let (daemon, lines) = open_key(&dir).await;
        let first = daemon.status().await;
        let key = first.key.clone().unwrap();
        assert_eq!(key.len(), 64);
        assert_eq!(first.riff.as_deref(), Some(key.as_str()));
        assert_eq!(first.owner, Some(this_user().unwrap()));
        assert_eq!(kept_key(&dir).as_deref(), Some(key.as_str()));
        let owner = first.owner.clone().unwrap();
        assert_eq!(lines, [text::daemon_made_riff(&key, &owner)]);
        let store = Dir::new(state_dir(&dir));
        let made = made(&store).await.unwrap().unwrap();
        assert_eq!(made, Made { riff: key.clone(), owner });
        drop(daemon);

        let (daemon, lines) = open_key(&dir).await;
        let again = daemon.status().await;
        assert!(lines.is_empty(), "{lines:?}");
        assert_eq!((again.key, again.riff, again.owner), (first.key, first.riff, first.owner));
        assert_eq!(again.position, first.position);
        assert!(lost(&dir).is_empty());
    }

    /// 01M519Z2G77CT94Y6XPW9BCRK9, 01M51JW25SPDW564A3DG31D7XM: a key with
    /// no state, a state with no key, another key, and a value that is
    /// not a key each make a new key and a new riff, with a line that
    /// says why. The old state goes to `state.lost-MS`.
    #[tokio::test]
    async fn a_key_and_a_state_that_do_not_go_together_make_a_new_riff() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let name = crate::daemon_key::secret_name(&dir);
        let (daemon, _) = open_key(&dir).await;
        let mut riffs = vec![daemon.status().await.riff.unwrap()];
        drop(daemon);

        // The key, and no state.
        std::fs::remove_dir_all(state_dir(&dir)).unwrap();
        let (daemon, lines) = open_key(&dir).await;
        let status = daemon.status().await;
        let riff = status.riff.clone().unwrap();
        let owner = status.owner.clone().unwrap();
        assert_eq!(lines, [text::daemon_new_riff(&Lost::NoState, &riff, &owner, None)]);
        assert!(lost(&dir).is_empty());
        riffs.push(riff);
        drop(daemon);

        // The state, and no key.
        crate::secrets::delete(&name).unwrap();
        let (daemon, lines) = open_key(&dir).await;
        let riff = daemon.status().await.riff.unwrap();
        let old = lost(&dir);
        assert_eq!(old.len(), 1);
        assert!(old[0].join("log").exists(), "{old:?}");
        assert_eq!(
            lines,
            [text::daemon_new_riff(&Lost::NoKey, &riff, &owner, Some(&old[0]))]
        );
        riffs.push(riff);
        drop(daemon);

        // Another key.
        let other = Key::generate();
        crate::daemon_key::keep(&name, &other).unwrap();
        let (daemon, lines) = open_key(&dir).await;
        let riff = daemon.status().await.riff.unwrap();
        let why = Lost::OtherKey {
            riff: riffs[2].clone(),
        };
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with(&text::daemon_new_riff(&why, &riff, &owner, None)[..80]));
        assert!(lines[0].contains(&format!("the riff {} of its state", riffs[2])));
        assert_ne!(riff, other.public().to_string());
        riffs.push(riff);
        drop(daemon);

        // A value that is not a key.
        crate::secrets::set(&name, "not a key").unwrap();
        let (daemon, lines) = open_key(&dir).await;
        let riff = daemon.status().await.riff.unwrap();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("that is not a key"), "{lines:?}");
        riffs.push(riff);

        // Each new riff is the key in the keyring, and no riff comes back.
        assert_eq!(kept_key(&dir).as_ref(), riffs.last());
        let mut each = riffs.clone();
        each.sort();
        each.dedup();
        assert_eq!(each.len(), riffs.len(), "{riffs:?}");
        assert_eq!(lost(&dir).len(), 3);
    }

    /// 01M51BFYNMYHSQ62BBJES3CCWT: a locked keyring (the mock refuses one
    /// read) makes the daemon wait. It says one line, makes no new key
    /// and no riff, and then loads the same key and the same riff.
    #[tokio::test]
    async fn a_locked_keyring_makes_the_daemon_wait_for_the_same_key() {
        use keyring_core::{Entry, Error, mock};

        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let (daemon, _) = open_key(&dir).await;
        let before = daemon.status().await;
        drop(daemon);

        let name = crate::daemon_key::secret_name(&dir);
        let entry = Entry::new(crate::secrets::SERVICE, &name).unwrap();
        let cred: &mock::Cred = entry.as_any().downcast_ref().unwrap();
        cred.set_error(Error::NoStorageAccess(Box::new(std::io::Error::other(
            "the collection is locked",
        ))));
        let (daemon, lines) = open_key(&dir).await;
        let after = daemon.status().await;
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].starts_with("The riff daemon waits for its key"), "{lines:?}");
        assert!(lines[0].contains("is locked or does not answer"), "{lines:?}");
        assert!(lines[0].contains("It makes no new key"), "{lines:?}");
        assert_eq!(lines[1], text::daemon_keyring_back());
        assert_eq!((after.key, after.riff), (before.key.clone(), before.riff));
        assert_eq!(after.position, before.position);
        assert_eq!(kept_key(&dir), before.key);
        assert!(lost(&dir).is_empty());
    }

    /// The riff-server of the lead, as the test plays it: it notes each
    /// keep-alive and each end, and gives a wake at once. No wave runs.
    #[derive(Default)]
    struct Server {
        said: Mutex<Vec<String>>,
    }

    impl Server {
        fn said(&self) -> Vec<String> {
            self.said
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }

        fn note(&self, line: String) {
            self.said
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(line);
        }
    }

    impl World for Server {
        fn look<'a>(
            &'a self,
            _: &'a Wave,
        ) -> crate::wave::BoxFuture<'a, Result<crate::wave::Look>> {
            Box::pin(async { bail!("no wave in this test") })
        }

        fn image(&self, _: &Wave) -> Result<String> {
            bail!("no wave in this test")
        }

        fn begin<'a>(
            &'a self,
            _: &'a Wave,
            _: &'a crate::wave::Work,
            _: &'a str,
        ) -> crate::wave::BoxFuture<'a, Result<Option<Start>>> {
            Box::pin(async { bail!("no wave in this test") })
        }

        fn resume(&self, record: &Record) {
            self.note(format!("resume {}", record.live.session));
        }

        fn finish<'a>(&'a self, _: &'a Record, _: Ended) -> crate::wave::BoxFuture<'a, Result<()>> {
            Box::pin(async { Ok(()) })
        }

        fn sweep<'a>(
            &'a self,
            _: &'a Wave,
            _: &'a [PathBuf],
        ) -> crate::wave::BoxFuture<'a, Result<()>> {
            Box::pin(async { Ok(()) })
        }

        fn tell<'a>(&'a self, _: &'a Wave, _: &'a str) -> crate::wave::BoxFuture<'a, ()> {
            Box::pin(async {})
        }

        fn leave<'a>(&'a self, record: &'a Record) -> crate::wave::BoxFuture<'a, ()> {
            Box::pin(async move { self.note(format!("leave {}", record.live.session)) })
        }

        fn wake<'a>(&'a self, record: &'a Record) -> crate::wave::BoxFuture<'a, Result<String>> {
            Box::pin(async move { Ok(format!("a wake of {}", record.live.item)) })
        }
    }

    /// Waits until `done` is true, for at most 5 s.
    async fn until(mut done: impl FnMut() -> bool) {
        for _ in 0..500 {
            if done() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("it did not happen in 5 s");
    }

    /// The phase of the session `name` in the status of the daemon.
    async fn phase_of(dir: &Path, name: &str) -> Phase {
        status(dir)
            .await
            .unwrap()
            .sessions
            .into_iter()
            .find(|s| s.name == name)
            .map(|s| s.phase)
            .unwrap()
    }

    /// Asks a takeover of `name`, and gives the client after the first
    /// line of the daemon.
    async fn take_over(dir: &Path, name: &str) -> (Client, Say) {
        let mut taker = Client::connect(dir).await.unwrap();
        taker
            .ask(Ask::Takeover { name: name.into() })
            .await
            .unwrap();
        let first = taker.next().await.unwrap().unwrap();
        (taker, first)
    }

    /// The next line of `taker` that is not an event of the session.
    async fn next_of(taker: &mut Client) -> Option<Say> {
        loop {
            match taker.next().await.unwrap() {
                Some(Say::Event { .. } | Say::Stderr { .. }) => {}
                other => return other,
            }
        }
    }

    /// A worker with its relay, after its start text and its first event.
    async fn a_worker(here: &Here) -> (Client, String, Relay) {
        let start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        let (client, name) = here.started(start).await;
        let socket = session_dir(&here.dir, &name);
        let (mut relay, _) = Relay::connect(&socket, 0, false).await;
        assert!(matches!(relay.next().await, Say::Input { .. }));
        relay.event(1, TURN[0]).await;
        (client, name, relay)
    }

    #[tokio::test]
    async fn the_lead_runs_with_a_terminal_counts_in_no_limit_and_leaves_riff_server_at_its_end() {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        let mut start = here.start("riff-lead-o-r", "lead");
        start.terminal = true;
        let (mut client, name) = here.started(start).await;
        assert_eq!(client.next().await.unwrap(), Some(Say::Done));
        drop(client);

        // The daemon starts it with a terminal: the relay runs claude in
        // its own place.
        assert_eq!(here.fake.calls(), ["terminal riff-lead-o-r"]);
        let spec = here.fake.spec(&name).unwrap();
        assert_eq!(
            spec.command,
            ["/opt/riff/riff", "relay", "--terminal", "--", "claude"]
        );
        // It counts in no limit, and keeps its session alive.
        assert_eq!(here.daemon.live_count(), 0);
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Terminal);
        assert_eq!(server.said(), ["resume riff-lead-o-r-session"]);
        let record: Record =
            serde_json::from_str(&std::fs::read_to_string(record_path(&here.dir, &name)).unwrap())
                .unwrap();
        assert!(record.terminal && record.inputs.is_empty());

        // riff relay watch gets the next wake of the session.
        let socket = session_dir(&here.dir, &name);
        let conn = connect_in(&socket, SOCKET).await.unwrap();
        let (read, mut write) = conn.into_split();
        let ask = serde_json::to_string(&SessionAsk::Watch { protocol: PROTOCOL }).unwrap();
        write
            .write_all(format!("{ask}\n").as_bytes())
            .await
            .unwrap();
        let line = BufReader::new(read)
            .lines()
            .next_line()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Say>(&line).unwrap(),
            Say::Line {
                text: "a wake of lead".into()
            }
        );

        // A takeover of the lead is refused: a person attaches to it.
        let (_, refused) = take_over(&here.dir, &name).await;
        assert_eq!(
            refused,
            Say::Failed {
                error: text::takeover_terminal(&name)
            }
        );

        // At its end, the daemon ends its session on riff-server.
        here.fake.stop(&name).await.unwrap();
        until(|| server.said().len() == 2).await;
        assert_eq!(server.said()[1], "leave riff-lead-o-r-session");
        assert!(status(&here.dir).await.unwrap().sessions.is_empty());
    }

    #[tokio::test]
    async fn a_takeover_holds_the_worker_at_the_end_of_its_turn_and_gives_it_back_with_one_input() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (mut client, name, mut relay) = a_worker(&here).await;
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Works);

        let (mut taker, first) = take_over(&here.dir, &name).await;
        assert_eq!(
            first,
            Say::Line {
                text: text::takeover_waits(&name)
            }
        );
        // The relay closes the stdin of claude: it ends after its turn.
        assert_eq!(relay.next().await, Say::Hold);
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Holds);
        relay.event(2, TURN[1]).await;
        relay.event(3, TURN[2]).await;
        relay.send(&Up::Held).await;
        assert_eq!(
            next_of(&mut taker).await,
            Some(Say::Held {
                session: "riff-issue-12-1a2b3c4d-session".into()
            })
        );
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Taken);

        // The terminal of the person ended: the session goes on.
        taker.go_on().await.unwrap();
        assert_eq!(next_of(&mut taker).await, Some(Say::Done));
        assert_eq!(relay.next().await, Say::GoOn);
        let go_on = user_line(text::TAKEOVER_GO_ON);
        assert_eq!(
            relay.next().await,
            Say::Input {
                line: go_on.clone()
            }
        );
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Works);
        let record: Record =
            serde_json::from_str(&std::fs::read_to_string(record_path(&here.dir, &name)).unwrap())
                .unwrap();
        assert_eq!(record.inputs, [user_line("Do it."), go_on]);
        assert!(!record.takeover && !record.closed);

        // The new turn ends as each turn: the close, then the exit.
        relay.event(4, TURN[2]).await;
        relay.exit(5, 0).await;
        let (said, end) = to_the_end(&mut client).await;
        assert_eq!(events(&said).len(), 4);
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: false
            }))
        );
    }

    #[tokio::test]
    async fn the_quiet_limit_does_not_stop_a_session_in_a_takeover() {
        let quiet = Duration::from_millis(200);
        let here = Here::with_quiet(Fake::default().with_image(IMAGE), quiet).await;
        let (_client, name, mut relay) = a_worker(&here).await;
        let (mut taker, _) = take_over(&here.dir, &name).await;
        assert_eq!(relay.next().await, Say::Hold);
        // The result closes the input, and the quiet limit would count.
        relay.event(2, TURN[2]).await;
        relay.send(&Up::Held).await;
        assert!(matches!(next_of(&mut taker).await, Some(Say::Held { .. })));
        tokio::time::sleep(quiet * 4).await;
        assert!(
            !here.fake.calls().iter().any(|c| c.starts_with("stop")),
            "{:?}",
            here.fake.calls()
        );
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Taken);
    }

    #[tokio::test]
    async fn the_daemon_refuses_a_takeover_with_no_relay_or_with_a_relay_of_protocol_3() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        let (_client, name) = here.started(start).await;
        let refused = Say::Failed {
            error: text::takeover_no_relay(&name),
        };
        // No relay yet.
        assert_eq!(take_over(&here.dir, &name).await.1, refused);
        // A relay of riff 3.0 before the takeover.
        let socket = session_dir(&here.dir, &name);
        let old = SessionAsk::Relay {
            protocol: 3,
            inputs: 0,
            closed: false,
            held: false,
        };
        let (_relay, _) = Relay::connect_as(&socket, old).await;
        assert_eq!(take_over(&here.dir, &name).await.1, refused);
        // A session that the daemon does not own.
        assert_eq!(
            take_over(&here.dir, "riff-other").await.1,
            Say::Failed {
                error: text::daemon_no_session("riff-other")
            }
        );
    }

    #[tokio::test]
    async fn a_person_who_leaves_before_the_end_of_the_turn_gets_no_terminal_and_the_session_goes_on()
     {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (_client, name, mut relay) = a_worker(&here).await;
        let (taker, _) = take_over(&here.dir, &name).await;
        assert_eq!(relay.next().await, Say::Hold);
        // Ctrl-C before the end of the turn: the connection ends.
        drop(taker);
        until(|| {
            std::fs::read_to_string(record_path(&here.dir, &name))
                .ok()
                .and_then(|t| serde_json::from_str::<Record>(&t).ok())
                .is_some_and(|r| !r.takeover)
        })
        .await;
        // claude ends its turn for the hold: the daemon goes on at once.
        relay.event(2, TURN[2]).await;
        relay.send(&Up::Held).await;
        assert_eq!(relay.next().await, Say::GoOn);
        assert_eq!(
            relay.next().await,
            Say::Input {
                line: user_line(text::TAKEOVER_GO_ON)
            }
        );
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Works);
    }

    #[tokio::test]
    async fn a_new_daemon_keeps_a_takeover_and_a_new_riff_takeover_gets_the_terminal_at_once() {
        let mut here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (_client, name, mut relay) = a_worker(&here).await;
        // The first taker stays: its process went with the old daemon.
        let (mut first, _) = take_over(&here.dir, &name).await;
        assert_eq!(relay.next().await, Say::Hold);
        relay.send(&Up::Held).await;
        assert!(matches!(next_of(&mut first).await, Some(Say::Held { .. })));

        here.restart().await;
        here.daemon.recover().await;
        let socket = session_dir(&here.dir, &name);
        let held = SessionAsk::Relay {
            protocol: PROTOCOL,
            inputs: 1,
            closed: true,
            held: true,
        };
        let (mut relay, had) = Relay::connect_as(&socket, held).await;
        assert_eq!(had, 1);
        let (mut taker, first_line) = take_over(&here.dir, &name).await;
        assert_eq!(
            first_line,
            Say::Held {
                session: "riff-issue-12-1a2b3c4d-session".into()
            }
        );
        taker.go_on().await.unwrap();
        assert_eq!(next_of(&mut taker).await, Some(Say::Done));
        assert_eq!(relay.next().await, Say::GoOn);
        assert!(matches!(relay.next().await, Say::Input { .. }));
        drop(first);
    }

    #[tokio::test]
    async fn riff_takeover_runs_the_terminal_of_the_person_and_gives_the_session_back() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (_client, name, mut relay) = a_worker(&here).await;
        let (dir, fake, at) = (here.dir.clone(), here.fake.clone(), name.clone());
        let shown = Arc::new(Mutex::new(Vec::new()));
        let lines = shown.clone();
        let taken = tokio::spawn(async move {
            crate::takeover::takeover(&dir, &fake, &at, |line| {
                lines
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(line.to_owned());
            })
            .await
        });
        assert_eq!(relay.next().await, Say::Hold);
        relay.event(2, TURN[2]).await;
        relay.send(&Up::Held).await;
        assert_eq!(relay.next().await, Say::GoOn);
        assert!(matches!(relay.next().await, Say::Input { .. }));
        assert_eq!(taken.await.unwrap().unwrap(), 0);
        let exec = format!(
            "exec {name} /opt/riff/riff relay --terminal -- claude --resume \
             riff-issue-12-1a2b3c4d-session --permission-mode auto"
        );
        assert!(here.fake.calls().contains(&exec), "{:?}", here.fake.calls());
        let shown = shown.lock().unwrap_or_else(PoisonError::into_inner).clone();
        assert_eq!(
            shown,
            [
                text::takeover_waits(&name),
                text::takeover_held(&name),
                text::takeover_back(&name)
            ]
        );
    }

    #[tokio::test]
    async fn a_session_that_ends_while_a_takeover_waits_leaves_no_record() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (mut client, name, mut relay) = a_worker(&here).await;
        let (mut taker, _) = take_over(&here.dir, &name).await;
        assert_eq!(relay.next().await, Say::Hold);
        // claude ended its turn before the hold: an exit, and no held.
        relay.event(2, TURN[2]).await;
        relay.exit(3, 0).await;
        assert!(matches!(next_of(&mut taker).await, Some(Say::Ended(_))));
        let (_, end) = to_the_end(&mut client).await;
        assert!(matches!(end, Some(Say::Ended(_))));
        // The end of riff takeover writes no record of the session.
        drop(taker);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!record_path(&here.dir, &name).exists());
        assert!(status(&here.dir).await.unwrap().sessions.is_empty());
    }

    /// The path of the unit file of [`Units`].
    const UNIT_FILE: &str = "/home/mike/.config/systemd/user/riff.service";

    /// A service manager with its unit in memory. It logs each change.
    #[derive(Default)]
    struct Units {
        unit: Mutex<Option<Unit>>,
        calls: Mutex<Vec<&'static str>>,
    }

    impl Units {
        /// A unit that runs `exe`, with a `PATH`.
        fn of(exe: &Path) -> Units {
            let unit = Unit {
                exe: exe.to_owned(),
                env: vec![("PATH".into(), "/home/mike/.cargo/bin:/usr/bin".into())],
            };
            Units {
                unit: Mutex::new(Some(unit)),
                calls: Mutex::default(),
            }
        }

        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }

        fn exe(&self) -> Option<PathBuf> {
            self.unit.lock().unwrap().as_ref().map(|u| u.exe.clone())
        }
    }

    impl ServiceManager for Units {
        fn install(&self, _: &Unit) -> Result<PathBuf> {
            bail!("this test installs nothing")
        }

        fn uninstall(&self) -> Result<Option<PathBuf>> {
            bail!("this test removes nothing")
        }

        fn state(&self) -> Result<String> {
            Ok("active".into())
        }

        fn installed(&self) -> Result<Option<Unit>> {
            Ok(self.unit.lock().unwrap().clone())
        }

        fn rewrite(&self, unit: &Unit) -> Result<PathBuf> {
            self.calls.lock().unwrap().push("rewrite");
            *self.unit.lock().unwrap() = Some(unit.clone());
            Ok(UNIT_FILE.into())
        }

        fn try_restart(&self) -> Result<()> {
            self.calls.lock().unwrap().push("try-restart");
            Ok(())
        }

        fn main_pid(&self) -> Result<Option<u32>> {
            Ok(Some(std::process::id()))
        }
    }

    /// The riff of a verify after the release, which a unit can still
    /// run, and the new riff on `PATH`.
    fn check_and_new(here: &Here) -> (PathBuf, PathBuf) {
        let check = PathBuf::from("/home/mike/.cache/riff/check/target/static/release/riff");
        (check, here.root.path().join("bin/riff"))
    }

    /// The unit runs an old riff, and the daemon owns no session: `riff
    /// update` writes the unit with the riff on `PATH`, keeps its
    /// variables, and starts the daemon again at once
    /// (01M517BTDRKDXD3X84P6AEAF7P, 01M517BTJ9EV4ZGPYRB6HXJJC9).
    #[tokio::test]
    async fn after_an_update_an_idle_daemon_runs_the_new_riff_at_once() {
        let here = Here::new(Fake::default()).await;
        let (check, riff) = check_and_new(&here);
        let units = Units::of(&check);
        let lines = after_update(&units, &riff, &here.dir).await.unwrap();
        assert_eq!(
            lines,
            [
                text::daemon_unit_moved(Path::new(UNIT_FILE), &check, &riff),
                text::DAEMON_STARTS_AGAIN.to_owned()
            ]
        );
        assert_eq!(units.calls(), ["rewrite", "try-restart"]);
        let unit = units.installed().unwrap().unwrap();
        assert_eq!(unit.exe, riff);
        assert_eq!(
            unit.env,
            Units::of(&check).installed().unwrap().unwrap().env
        );
    }

    /// While the daemon runs a session, `riff update` writes the unit and
    /// starts nothing. The daemon looks, and asks for its new start only
    /// at the end of its last session (01M517BTJ9EV4ZGPYRB6HXJJC9,
    /// 01M517BTP6BAGTRBYWPX9GPVE0).
    #[tokio::test]
    async fn after_an_update_a_busy_daemon_starts_again_after_its_last_session() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (check, riff) = check_and_new(&here);
        let units = Arc::new(Units::of(&check));
        let (mut client, _, mut relay) = a_worker(&here).await;

        let lines = after_update(&*units, &riff, &here.dir).await.unwrap();
        assert_eq!(
            lines,
            [
                text::daemon_unit_moved(Path::new(UNIT_FILE), &check, &riff),
                text::daemon_starts_again_later(1)
            ]
        );
        assert_eq!(units.calls(), ["rewrite"]);

        // The daemon runs the riff of the test, not the riff of the unit.
        let watch = {
            let (daemon, units) = (here.daemon.clone(), units.clone());
            tokio::spawn(async move {
                let own = own_binary().unwrap();
                let old = || stale(units.installed().unwrap().as_ref(), &own, false);
                let restart = || units.try_restart();
                daemon.restart_when_old(old, restart).await;
            })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(units.calls(), ["rewrite"], "a new start with a session");
        assert!(!watch.is_finished());

        relay.exit(2, 0).await;
        let (_, end) = to_the_end(&mut client).await;
        assert!(matches!(end, Some(Say::Ended(_))));
        tokio::time::timeout(Duration::from_secs(5), watch)
            .await
            .expect("the end of the session looks")
            .unwrap();
        assert_eq!(units.calls(), ["rewrite", "try-restart"]);
    }

    /// The unit runs the riff on `PATH`, and the daemon runs that riff:
    /// `riff update` changes nothing, and the daemon asks nothing. With
    /// no unit, riff changes nothing. With no daemon, riff only writes
    /// the unit.
    #[tokio::test]
    async fn after_an_update_a_daemon_on_the_new_riff_stays() {
        let here = Here::new(Fake::default()).await;
        let own = own_binary().unwrap();
        let units = Arc::new(Units::of(&own));
        assert!(
            after_update(&*units, &own, &here.dir)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(units.calls().is_empty());
        let watch = {
            let (daemon, units) = (here.daemon.clone(), units.clone());
            tokio::spawn(async move {
                let own = own_binary().unwrap();
                let old = || stale(units.installed().unwrap().as_ref(), &own, false);
                let restart = || units.try_restart();
                daemon.restart_when_old(old, restart).await;
            })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!watch.is_finished());
        watch.abort();
        assert!(units.calls().is_empty());

        let none = Units::default();
        assert!(
            after_update(&none, &own, &here.dir)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(none.calls().is_empty());

        let (check, riff) = check_and_new(&here);
        let units = Units::of(&check);
        let away = here.root.path().join("no-daemon");
        let lines = after_update(&units, &riff, &away).await.unwrap();
        assert_eq!(
            lines,
            [text::daemon_unit_moved(Path::new(UNIT_FILE), &check, &riff)]
        );
        assert_eq!(units.calls(), ["rewrite"]);
        assert_eq!(units.exe(), Some(riff));
    }
}
