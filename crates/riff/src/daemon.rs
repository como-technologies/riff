//! The riff daemon (#716, 01M4Z60B7FCREZDCTVR73WWRY1): it owns the
//! sessions of riff 3.0 on this machine. A systemd user unit runs it
//! ([`crate::service`]).
//!
//! # Design
//!
//! The daemon starts and stops each session through a [`Runtime`]: on
//! Linux, rootless podman. It gives each session a Unix socket of its
//! own (01M4Z60B7W6F3CC5XJE0VS7Y86). `riff` talks to the daemon on the
//! control socket.
//!
//! ```mermaid
//! flowchart LR
//!     R["riff run issue-12"] -- "control socket" --> D["riff daemon<br/>systemd user unit"]
//!     D -- "Runtime: podman" --> C1
//!     D -- "Runtime: podman" --> C2
//!     subgraph C1["container riff-issue-12-ID8"]
//!         S1["/run/riff/riff.sock"]
//!     end
//!     subgraph C2["container riff-issue-9-ID8"]
//!         S2["/run/riff/riff.sock"]
//!     end
//!     S1 -- "its own socket" --> D
//!     S2 -- "its own socket" --> D
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
//!     S -->|"a command (#717, #719)"| R["Local::run: the records<br/>in the log, then the reply"]
//!     S -->|"the stop"| C["Local::checkpoint"]
//! ```
//!
//! - [`Daemon::open`] loads the state. At the first start, it makes the
//!   riff of the machine with a random ID.
//! - Each change goes through [`Daemon::state`]: one command at a time,
//!   with its records in the log before its reply
//!   (01M4ZHW22F8HE2E1WDSTNDJW4H). The riff tools of a session act on it
//!   (#717, #719).
//! - At the stop, [`Daemon::serve`] writes a checkpoint. A start after a
//!   crash replays the log after the last checkpoint.
//! - `riff daemon status` shows the ID of the riff and the position of
//!   the log ([`Status::riff`], [`Status::position`],
//!   01M4ZHW24JZZXM3JJXG3PRJS7N).
//!
//! # The protocol
//!
//! JSON, one message on each line (01M4ZDTVVV56X964S28GS6K2Z2). The
//! first line of each connection is a [`Hello`]: the version
//! [`PROTOCOL`] and one [`Ask`]. The daemon refuses another version, and
//! says to start it again with this riff. The daemon answers with
//! [`Say`] lines.
//!
//! | Ask | The daemon says |
//! |---|---|
//! | [`Ask::Status`] | [`Say::Status`] |
//! | [`Ask::Image`] | [`Say::Line`] for a pull, then [`Say::Done`] or [`Say::Failed`] |
//! | [`Ask::Start`] | [`Say::Started`], each [`Say::Event`] and [`Say::Stderr`], then [`Say::Ended`] or [`Say::Failed`] |
//!
//! After [`Ask::Start`], `riff` can send [`Then::Stop`]: the daemon stops
//! the container. When `riff` closes the connection, the session goes
//! on: the daemon owns it.
//!
//! On the socket of a session, a [`SessionAsk::Hello`] gets
//! [`Say::Session`]: the session ID, the item and the name of the
//! container. `riff relay` (#717) adds to this protocol.
//!
//! # A session
//!
//! [`drive`] starts the session of a spec, gives `claude` the start text
//! on one stream-json line, and takes each event. It takes each secret
//! out of each line ([`Scrub`]), writes it to `events.jsonl` in the
//! folder of the session, and sends it to `riff`. At the `result` of
//! the turn, it closes the input: `claude` ends.
//!
//! # A stop of the daemon
//!
//! A stop or a restart of the daemon stops no container
//! (01M4ZDTW00B7N6T9P3MK066KA7). The daemon stops a container only on
//! [`Then::Stop`]. conmon holds each container in a scope of its own
//! ([`crate::podman`]). Until `riff relay` (#717), the input and the
//! events of a session go through the `podman run` of the daemon. So at
//! a stop of the daemon, they end: `claude` ends after its turn, and
//! its container goes (01M4ZDTW25YV0GMMVGWN6JTJC5).

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use riff_core::build::Build;
use riff_state::checkpoint;
use riff_state::local::Local;
use riff_state::state::{Caller, MakeRiff, State};
use riff_state::store::Dir;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};

use crate::run::{Scrub, user_line};
use crate::runtime::{Bind, Env, Exit, Running, Runtime, SessionSpec, Source};
use crate::text;

/// The version of the protocol between `riff` and the daemon.
pub const PROTOCOL: u32 = 1;

/// The name of each socket of the daemon.
pub const SOCKET: &str = "riff.sock";

/// The folder of its socket in a session.
pub const IN_SESSION: &str = "/run/riff";

/// The variable that names the socket in a session.
pub const SOCKET_VAR: &str = "RIFF_SOCKET";

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
async fn connect_in(dir: &Path, name: &str) -> std::io::Result<UnixStream> {
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
    /// The spec. The daemon adds the bind of the socket.
    pub spec: SessionSpec,
    /// The start text: the first message to `claude`.
    pub start: String,
    /// The value of each [`Env::Host`] of the spec.
    pub values: Vec<(String, String)>,
    /// The secrets that no event shows ([`Scrub`]).
    pub secrets: Vec<String>,
}

/// A line of `riff` after [`Ask::Start`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "then", rename_all = "snake_case")]
pub enum Then {
    /// Stop the container.
    Stop,
}

/// What a session asks on its own socket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "ask", rename_all = "snake_case")]
pub enum SessionAsk {
    /// Who am I?
    Hello {
        /// [`PROTOCOL`] of the session.
        protocol: u32,
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
    /// The exit code of the command of the session, or none after a
    /// signal.
    pub code: Option<i32>,
    /// True when the turn of `claude` ended with an error, or a stop
    /// ended the session.
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
    /// One line of the output of the session: an event of `claude`,
    /// with no secret.
    Event {
        /// The line.
        line: String,
    },
    /// One line of the errors of the session, with no secret.
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

/// The daemon: the sessions of this machine, on the runtime `R`, and
/// the state of riff on this machine.
pub struct Daemon<R> {
    runtime: R,
    dir: PathBuf,
    since_ms: i64,
    live: Mutex<BTreeMap<String, Live>>,
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
            live: Mutex::new(BTreeMap::new()),
            state,
        })
    }

    /// The state of riff on this machine. The riff tools of a session
    /// act on it (#717, #719).
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
            sessions: self.lock().values().cloned().collect(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Live>> {
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
    /// in it, or a bind at [`IN_SESSION`] (01M4ZDTVQEHD87WD5TSN93XFSS).
    pub fn check(&self, spec: &SessionSpec) -> Result<()> {
        let ours = resolved(&self.dir);
        for bind in &spec.binds {
            if bind.target.starts_with(IN_SESSION) {
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

    /// Starts the session of `start`, says its events on `out`, and
    /// waits for its end. A [`Then::Stop`] on `from` stops it.
    async fn start(
        self: &Arc<Self>,
        start: Start,
        mut from: Lines<BufReader<OwnedReadHalf>>,
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
        } = start;
        let name = spec.name.clone();
        if let Err(e) = self.check(&spec) {
            out.say(Say::Failed {
                error: format!("{e:#}"),
            });
            return;
        }
        let live = Live {
            name: name.clone(),
            session: session.clone(),
            item: item.clone(),
            image: spec.image.clone(),
            since_ms: now_ms(),
        };
        {
            let mut sessions = self.lock();
            if sessions.contains_key(&name) {
                drop(sessions);
                out.say(Say::Failed {
                    error: text::daemon_runs_session(&name),
                });
                return;
            }
            sessions.insert(name.clone(), live);
        }
        let socket_dir = session_dir(&self.dir, &name);
        let ended = match self.door(&socket_dir, &session, &item, &name) {
            Ok(door) => {
                spec.binds.push(Bind {
                    source: Source::Path(socket_dir.clone()),
                    target: IN_SESSION.into(),
                    write: true,
                });
                spec.env.push(Env::Value(
                    SOCKET_VAR.into(),
                    in_session().to_string_lossy().into_owned(),
                ));
                println!("{}", text::daemon_session_starts(&name, &item, &session));
                out.say(Say::Started {
                    name: name.clone(),
                    socket: socket_dir.clone(),
                });
                let (stopped, stop) = oneshot::channel();
                let reader = tokio::spawn(async move {
                    while let Ok(Some(line)) = from.next_line().await {
                        if matches!(serde_json::from_str::<Then>(&line), Ok(Then::Stop)) {
                            let _ = stopped.send(());
                            return;
                        }
                    }
                    // `riff` left with no stop: the session goes on.
                    std::future::pending::<()>().await;
                });
                let stop = async move {
                    if stop.await.is_err() {
                        std::future::pending::<()>().await;
                    }
                };
                let ended = drive(
                    &self.runtime,
                    &spec,
                    &start,
                    &folder.join("events.jsonl"),
                    &Scrub::new(secrets),
                    &values,
                    stop,
                    out,
                )
                .await;
                reader.abort();
                door.abort();
                ended
            }
            Err(e) => Err(e),
        };
        let _ = std::fs::remove_dir_all(&socket_dir);
        self.lock().remove(&name);
        match ended {
            Ok(ended) => {
                println!("{}", text::daemon_session_ends(&name, ended.code));
                out.say(Say::Ended(ended));
            }
            Err(e) => {
                println!("{}", text::daemon_session_failed(&name));
                out.say(Say::Failed {
                    error: format!("{e:#}"),
                });
            }
        }
    }

    /// Makes the folder of the socket of the session `name`, and answers
    /// each connection on its socket with who the session is. The task
    /// ends at its abort, with each of its connections.
    fn door(&self, dir: &Path, session: &str, item: &str, name: &str) -> Result<JoinHandle<()>> {
        let _ = std::fs::remove_dir_all(dir);
        private_dir(dir)?;
        let listener = bind_in(dir, SOCKET)?;
        let who = Say::Session {
            session: session.to_owned(),
            item: item.to_owned(),
            name: name.to_owned(),
        };
        Ok(tokio::spawn(async move {
            let mut connections = JoinSet::new();
            while let Ok((conn, _)) = listener.accept().await {
                while connections.try_join_next().is_some() {}
                connections.spawn(answer_session(conn, who.clone()));
            }
        }))
    }
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

/// Answers one connection on the socket of a session with `who`.
async fn answer_session(conn: UnixStream, who: Say) {
    let (read, write) = conn.into_split();
    let mut lines = BufReader::new(read).lines();
    let (out, written) = writer(write);
    while let Ok(Some(line)) = lines.next_line().await {
        match serde_json::from_str::<SessionAsk>(&line) {
            Ok(SessionAsk::Hello { protocol }) if protocol == PROTOCOL => out.say(who.clone()),
            Ok(SessionAsk::Hello { protocol }) => out.say(Say::Failed {
                error: text::daemon_protocol(protocol, PROTOCOL),
            }),
            Err(_) => out.say(Say::Failed {
                error: text::DAEMON_NO_HELLO.into(),
            }),
        }
    }
    drop(out);
    let _ = written.await;
}

/// Starts the session of `spec` with the `values` of the host, gives
/// `claude` the start text, and says each event on `out`, with no
/// secret. It writes each event to `events`. At the `result` of the
/// turn, it closes the input: `claude` ends. When `stop` ends first, it
/// stops the container.
#[allow(clippy::too_many_arguments)]
pub async fn drive(
    runtime: &impl Runtime,
    spec: &SessionSpec,
    start: &str,
    events: &Path,
    scrub: &Scrub,
    values: &[(String, String)],
    stop: impl Future<Output = ()>,
    out: &Out,
) -> Result<Ended> {
    // The log comes first: a session that cannot log starts no
    // container.
    let mut log = tokio::fs::File::create(events)
        .await
        .with_context(|| format!("cannot write {}", events.display()))?;
    let Running {
        name,
        input,
        events: output,
        errors,
        end,
    } = runtime.start(spec, values).await?;
    let mut input = Some(input);
    if let Some(input) = input.as_mut() {
        input
            .write_all(format!("{}\n", user_line(start)).as_bytes())
            .await?;
        input.flush().await?;
    }
    let errors = {
        let (scrub, out) = (scrub.clone(), out.clone());
        tokio::spawn(async move {
            let mut lines = BufReader::new(errors).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                out.say(Say::Stderr {
                    line: scrub.clean(&line).into_owned(),
                });
            }
        })
    };
    let mut lines = BufReader::new(output).lines();
    let mut stopped = false;
    let mut failed = false;
    tokio::pin!(stop);
    loop {
        let line = tokio::select! {
            line = lines.next_line() => line?,
            () = &mut stop, if !stopped => {
                stopped = true;
                println!("{}", text::daemon_stops(&name));
                let _ = runtime.stop(&name).await;
                continue;
            }
        };
        let Some(line) = line else { break };
        let line = scrub.clean(&line).into_owned();
        log.write_all(format!("{line}\n").as_bytes()).await?;
        if let Ok(event) = serde_json::from_str::<Value>(&line)
            && event["type"] == "result"
        {
            failed |= event["is_error"] == true;
            // The end of the one turn: claude ends when its input
            // closes.
            input = None;
        }
        out.say(Say::Event { line });
    }
    drop(input);
    log.flush().await?;
    let Exit { code } = end.wait().await?;
    let _ = errors.await;
    Ok(Ended {
        code,
        failed: failed || stopped,
    })
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

/// `riff daemon`: runs the daemon of this machine until a SIGTERM or a
/// SIGINT. The stop stops no session (01M4ZDTVH57VE77YT20G65A4BQ).
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
    use crate::runtime::Fake;

    const IMAGE: &str = "localhost/riff-session:latest";

    /// The events of one turn of a fake `claude`, with a token in a
    /// tool result.
    const TURN: [&str; 3] = [
        r#"{"type":"system","subtype":"init","model":"m","permissionMode":"auto"}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true,"content":"gho_secret1"}]}}"#,
        r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1}"#,
    ];

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

    /// Asks the socket of a session in the folder `dir` who it is.
    async fn who_in(dir: &Path) -> Say {
        let conn = connect_in(dir, SOCKET).await.unwrap();
        let (read, mut write) = conn.into_split();
        let hello = serde_json::to_string(&SessionAsk::Hello { protocol: PROTOCOL }).unwrap();
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

    #[tokio::test]
    async fn a_session_runs_through_the_daemon_and_ends_after_its_turn() {
        let here = Here::new(Fake::default().with_image(IMAGE).with_events(TURN)).await;
        let start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        let events = start.folder.join("events.jsonl");
        let (mut client, name) = here.started(start).await;
        let (said, end) = to_the_end(&mut client).await;
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: false
            }))
        );
        // Each event, with no secret, to riff and to the log.
        let lines: Vec<String> = said
            .iter()
            .filter_map(|s| match s {
                Say::Event { line } => Some(line.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 3, "{said:?}");
        assert!(lines[1].contains(HIDDEN) && !lines[1].contains("gho_secret1"));
        let log = std::fs::read_to_string(&events).unwrap();
        assert_eq!(log, lines.join("\n") + "\n");
        // One stream-json line, then the input closed after the result.
        assert_eq!(here.fake.input(&name), format!("{}\n", user_line("Do it.")));
        assert_eq!(here.fake.exit(&name), Some(Exit::SUCCESS));
        // The token goes to the runtime beside the spec.
        assert_eq!(
            here.fake.values(&name),
            [("GH_TOKEN".to_owned(), "gho_secret1".to_owned())]
        );
        // The daemon owns the session no more, and its socket is gone.
        assert!(status(&here.dir).await.unwrap().sessions.is_empty());
        assert!(!session_dir(&here.dir, &name).exists());
    }

    #[tokio::test]
    async fn a_turn_that_ends_with_an_error_fails() {
        let error = r#"{"type":"result","subtype":"error_during_execution","is_error":true}"#;
        let here = Here::new(Fake::default().with_image(IMAGE).with_events([error])).await;
        let (mut client, _) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
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
        let here = Here::new(Fake::default().with_events(TURN)).await;
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
    }

    #[tokio::test]
    async fn each_session_gets_its_own_socket_and_no_other() {
        // claude starts, and never ends its turn.
        let here = Here::new(Fake::default().with_image(IMAGE).with_events([TURN[0]])).await;
        let (mut one, a) = here
            .started(here.start("riff-issue-1-aaaaaaaa", "issue-1"))
            .await;
        let (mut two, b) = here
            .started(here.start("riff-issue-2-bbbbbbbb", "issue-2"))
            .await;
        // The first event comes after the start of the container.
        for client in [&mut one, &mut two] {
            assert_eq!(
                client.next().await.unwrap(),
                Some(Say::Event {
                    line: TURN[0].into()
                })
            );
        }

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
            // Its socket tells the daemon which session asks.
            assert_eq!(
                who_in(&own).await,
                Say::Session {
                    session: format!("{name}-session"),
                    item: item.into(),
                    name: name.clone(),
                }
            );
        }
        let mode = |p: &Path| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(p).unwrap().permissions().mode() & 0o777
        };
        assert_eq!(mode(&here.dir), 0o700);
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
        let here = Here::new(Fake::default().with_image(IMAGE).with_events(TURN)).await;
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
        // A bind at the place of the socket in the session.
        let mut start = here.start("riff-issue-2-bbbbbbbb", "issue-2");
        start.spec.binds[0].target = "/run/riff".into();
        let mut client = Client::connect(&here.dir).await.unwrap();
        client.ask(Ask::Start(Box::new(start))).await.unwrap();
        assert!(matches!(
            client.next().await.unwrap(),
            Some(Say::Failed { .. })
        ));
        assert!(here.fake.calls().is_empty(), "{:?}", here.fake.calls());
    }

    #[tokio::test]
    async fn a_stop_of_the_daemon_stops_no_session() {
        let mut here = Here::new(Fake::default().with_image(IMAGE).with_events([TURN[0]])).await;
        let (mut client, name) = here
            .started(here.start("riff-issue-1-aaaaaaaa", "issue-1"))
            .await;
        assert_eq!(
            client.next().await.unwrap(),
            Some(Say::Event {
                line: TURN[0].into()
            })
        );
        here.stop().await;
        // No daemon answers, and the daemon stopped no container. A
        // connect alone can still work: a child that another test forks
        // holds the listener until its exec.
        assert!(status(&here.dir).await.is_err());
        assert_eq!(here.fake.calls(), [format!("start {name}")]);
        assert_eq!(here.fake.exit(&name), None);
    }

    #[tokio::test]
    async fn a_client_that_leaves_with_no_stop_leaves_the_session_running() {
        let here = Here::new(Fake::default().with_image(IMAGE).with_events([TURN[0]])).await;
        let (mut client, name) = here
            .started(here.start("riff-issue-1-aaaaaaaa", "issue-1"))
            .await;
        // The first event comes after the start of the container.
        assert_eq!(
            client.next().await.unwrap(),
            Some(Say::Event {
                line: TURN[0].into()
            })
        );
        drop(client);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(here.fake.calls(), [format!("start {name}")]);
        let sessions = status(&here.dir).await.unwrap().sessions;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].item, "issue-1");
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
