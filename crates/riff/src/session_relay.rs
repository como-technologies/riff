//! `riff relay`: the riff in each session (#717, 01M4Z60B7KEV0H161RBZ1SDRY2).
//!
//! # Design
//!
//! The daemon binds its own binary into each container at
//! [`daemon::RELAY_BIN`], with no write, and runs it as the entrypoint:
//! `RELAY_BIN relay -- COMMAND`. So the riff code in a session is always
//! the riff code of the host. The relay starts `COMMAND` (`claude -p` in
//! stream-json) and holds its stdin and stdout. It talks to the daemon
//! through the socket of its session, [`daemon::SOCKET_VAR`].
//!
//! conmon, the monitor of podman, is the parent of the container, not
//! the daemon. So the relay and `claude` run on when the daemon stops.
//! While the daemon is away, the relay keeps each event of `claude` that
//! the daemon did not take, and tries the socket again each [`RETRY`].
//! When the daemon comes back, the relay sends the kept events by their
//! sequence number. `claude` does not see the break
//! (01M4Z60B7Z24WBD6HV6ER0WN9K).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant C as claude -p
//!     participant R as riff relay
//!     participant D as riff daemon
//!     R->>C: start claude, with the riff tools
//!     R->>D: hello relay: inputs 0
//!     D-->>R: resume: events 0
//!     D-->>R: input: the start text
//!     R->>C: the start text, one line on stdin
//!     C-->>R: event 1, event 2
//!     R->>D: event 1, event 2
//!     D-->>R: ack 2
//!     Note over D: the daemon stops
//!     C-->>R: event 3, event 4 (result)
//!     Note over R: keeps 3 and 4, tries the socket again
//!     Note over D: a new daemon starts
//!     R->>D: hello relay: inputs 1
//!     D-->>R: resume: events 2
//!     R->>D: event 3, event 4
//!     D-->>R: ack 4, close
//!     R->>C: close stdin
//!     C-->>R: claude ends
//!     R->>D: exit 5
//!     D-->>R: ack 5
//!     Note over R: the relay ends with the exit code of claude
//! ```
//!
//! # The protocol
//!
//! JSON, one message on each line, on the socket of the session
//! (01M4ZDTVVV56X964S28GS6K2Z2). The relay opens each connection with
//! [`SessionAsk::Relay`]: its version of the protocol, the number of
//! inputs that it gave `claude`, and whether it closed the stdin of
//! `claude`. The daemon answers [`Say::Resume`] with the number of
//! events that it has. Then:
//!
//! | From | Line | Means |
//! |---|---|---|
//! | relay | [`Up::Event`] | One line of the stdout of `claude`, with its sequence number, with no secret. |
//! | relay | [`Up::Exit`] | `claude` ended, with its exit code. It has the next sequence number. |
//! | relay | [`Up::Held`] | `claude` ended for a takeover. The relay waits for [`Say::GoOn`]. |
//! | daemon | [`Say::Input`] | One line for the stdin of `claude`: a wake. |
//! | daemon | [`Say::Close`] | Close the stdin of `claude`: it ends after its turn. |
//! | daemon | [`Say::Ack`] | The daemon has each line up to this sequence number. |
//! | daemon | [`Say::Hold`] | A takeover: interrupt the turn when it says so, close the stdin of `claude`, and keep the container at its end. |
//! | daemon | [`Say::GoOn`] | The takeover ended: start `claude` again with `--resume`. |
//!
//! The sequence numbers start at 1, and go up by 1 for each line. The
//! event with the number N is line N of `events.jsonl`. At a new
//! connection, the relay drops each kept line that the resume names,
//! and sends each other one again. The daemon gives each input after
//! the number in the hello, and the close when the relay did not close
//! yet. So no event and no input goes twice, and none is lost
//! (01M4ZHSZMH4Y8CB41FYGE3C3R2).
//!
//! After `claude` ends, the relay waits up to [`WAIT`] for the ack of
//! its exit, and then ends with the exit code of `claude`. So the
//! container goes.
//!
//! # A takeover
//!
//! A person can take over a worker (01M4Z60B83ZH10HYKW7X1C0SSS,
//! 01M501PA94CDZDH214GGFMTH5V). The
//! daemon says [`Say::Hold`]: the relay closes the stdin of `claude`, so
//! `claude` ends after its turn. With `interrupt`, the relay first writes
//! [`INTERRUPT`] on the stdin of `claude`: `claude` stops its turn at
//! once (01M51KJ1V0STB4QCN4N5SBYGSP). The hello of the relay says
//! `interrupts`, so the daemon knows that this relay can do it. The end
//! of `claude` is then no exit: the relay says
//! [`Up::Held`], and the container stays. The person runs `claude
//! --resume ID` in the container with a terminal (`podman exec`). After
//! that, the daemon says [`Say::GoOn`] and gives one new input: the relay
//! starts `claude -p --resume ID` again ([`resumed`]), and the sequence
//! numbers of its events go on. While the daemon is away, the hello of
//! the next connection says that the relay waits (`held`).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as riff takeover (the person)
//!     participant D as riff daemon
//!     participant R as riff relay
//!     participant C as claude -p
//!     P->>D: takeover NAME
//!     D->>R: hold, interrupt
//!     R->>C: the interrupt, then close stdin
//!     C-->>R: the control response, the result of the stopped turn, then the end
//!     R->>D: held
//!     D-->>P: held: run the terminal now
//!     Note over P: podman exec -it NAME claude --resume ID
//!     P->>D: go on
//!     D->>R: go on, then an input
//!     R->>C: start claude -p --resume ID, the input
//! ```
//!
//! # A session with a terminal
//!
//! The lead runs `riff relay --terminal -- claude ...`: the relay adds
//! the riff tools, and runs `claude` in its own place with the terminal
//! ([`terminal`], 01M501P9VN47QAKZ6B8P39ZSW4). It relays no event.
//! `riff relay watch` waits for the next wake of the session, through
//! the daemon ([`watch`], 01M501PA04NP79RJQEDP4D7YTM), as `riff watch
//! --once` does on the host. At each prompt of the person, the hook
//! `UserPromptSubmit` runs `riff relay prompt` ([`prompt`]): the daemon
//! ends the block of the lead (01M5GVQQ8NK3RD9CM3VTGXEHZQ).
//!
//! # Versions
//!
//! An update of riff reaches new sessions only: the bind keeps the old
//! binary in a session that runs (01M4Z60B800BH5BHX0H9JVDG6A). So on the
//! socket of a session, the daemon takes a hello of the protocol before
//! its own, of its own, and of the next ([`daemon::compatible`]). A
//! change of the protocol adds a line or a field, and a side skips a
//! line that it does not know (01M4ZHSZPRYK42C20ETXM3HJZ2).
//!
//! # Secrets
//!
//! The relay takes each secret of the session out of each event and of
//! each line of the stderr of `claude` before it sends or writes it
//! (01M4Z60B8KDXD190RWC7D184Q3, 01M4ZHSZRY0P9HHNVSFET8HEEN): the values
//! of [`SECRET_VARS`], and the tokens of the sign-in file of `claude`
//! ([`secrets`]). It writes no file.
//!
//! # The riff tools
//!
//! The relay gives `claude` the riff tools: it adds `--mcp-config` with
//! the MCP server `riff`, which runs `RELAY_BIN relay mcp`
//! ([`with_tools`]), and each MCP server that the person gave the
//! sessions of the host ([`crate::session_mcp`]). [`mcp`] passes each
//! line between `claude` and the daemon, on the socket of the session ([`SessionAsk::Mcp`]). The
//! daemon serves the tools as that session (01M4ZHSZV46RF3XDXDTQXWGXRG):
//! only the tools that talk ([`crate::tools::CONTAINER_TOOLS`],
//! 01M4ZV3B6T2KB09WJ59F07HXET).
//! When the daemon goes away, each call in flight gets an error, and
//! each new line waits for the daemon. At a new connection, `riff relay
//! mcp` gives the daemon the initialize request of `claude`, so the
//! daemon skips the handshake (01M4ZHSZXAY7F7E17B3CMHG562).

use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::daemon::{self, PROTOCOL, Say, SessionAsk};
use crate::run::Scrub;
use crate::text;

/// The time between two tries to reach the daemon.
pub const RETRY: Duration = Duration::from_millis(250);

/// After `claude` ends, the longest wait for the daemon to take the last
/// lines (01M4ZKX0R850RPSS63B001NNJZ).
pub const WAIT: Duration = Duration::from_secs(600);

/// The variables of a session whose values are secrets.
pub const SECRET_VARS: [&str; 3] = ["GH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_API_KEY"];

/// The name of the MCP server of the riff tools: the tools are
/// `mcp__riff__NAME`, as in a worker of riff 1.x.
pub const MCP_SERVER: &str = "riff";

/// The stream-json line that interrupts the turn of `claude -p`, as the
/// Agent SDK sends it (01M51KJ1V0STB4QCN4N5SBYGSP). `claude` stops the
/// turn at once, also in the middle of a tool call. It answers with a
/// `control_response` and ends the turn with a `result` that has an
/// error.
///
/// ```
/// let line: serde_json::Value = serde_json::from_str(riff::session_relay::INTERRUPT).unwrap();
/// assert_eq!(line["type"], "control_request");
/// assert_eq!(line["request"]["subtype"], "interrupt");
/// assert!(!riff::session_relay::INTERRUPT.contains('\n'));
/// ```
pub const INTERRUPT: &str =
    r#"{"type":"control_request","request_id":"riff-takeover","request":{"subtype":"interrupt"}}"#;

/// What the relay sends the daemon after its hello.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "relay", rename_all = "snake_case")]
pub enum Up {
    /// One line of the stdout of `claude`, with no secret.
    Event {
        /// Its sequence number: its line in `events.jsonl`.
        seq: u64,
        /// The line.
        line: String,
    },
    /// `claude` ended.
    Exit {
        /// The next sequence number after the last event.
        seq: u64,
        /// The exit code of `claude`, or none after a signal.
        code: Option<i32>,
    },
    /// `claude` ended for a takeover: the relay waits for [`Say::GoOn`].
    /// The relay keeps no copy of it: the hello of the next connection
    /// says `held`.
    Held,
}

impl Up {
    /// The sequence number of the line.
    ///
    /// ```
    /// use riff::session_relay::Up;
    ///
    /// assert_eq!(Up::Event { seq: 3, line: "{}".into() }.seq(), 3);
    /// assert_eq!(Up::Exit { seq: 4, code: Some(0) }.seq(), 4);
    /// assert_eq!(Up::Held.seq(), 0);
    /// ```
    pub fn seq(&self) -> u64 {
        match self {
            Up::Event { seq, .. } | Up::Exit { seq, .. } => *seq,
            Up::Held => 0,
        }
    }
}

/// The lines that the daemon did not take yet, in order.
///
/// ```
/// use riff::session_relay::{Kept, Up};
///
/// let event = |seq| Up::Event { seq, line: format!("e{seq}") };
/// let mut kept = Kept::default();
/// for seq in 1..=4 {
///     kept.push(event(seq));
/// }
/// // The daemon has each line up to 2.
/// kept.taken(2);
/// let left: Vec<u64> = kept.lines().map(Up::seq).collect();
/// assert_eq!(left, [3, 4]);
/// kept.taken(4);
/// assert!(kept.is_empty());
/// ```
#[derive(Debug, Default)]
pub struct Kept(VecDeque<Up>);

impl Kept {
    /// Keeps `up` until the daemon takes it.
    pub fn push(&mut self, up: Up) {
        self.0.push_back(up);
    }

    /// Drops each line up to the sequence number `seq`: the daemon has it.
    pub fn taken(&mut self, seq: u64) {
        while self.0.front().is_some_and(|up| up.seq() <= seq) {
            self.0.pop_front();
        }
    }

    /// The kept lines, in order.
    pub fn lines(&self) -> impl Iterator<Item = &Up> {
        self.0.iter()
    }

    /// True when the daemon has each line.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The secrets of a session: the value of each variable of
/// [`SECRET_VARS`] that `var` gives, each value of the `env` and the
/// `headers` of the MCP servers in [`crate::session_mcp::VAR`]
/// (01M57840R2HNXGP9B2ENSKTWD3), and the tokens of the sign-in file of
/// `claude`, when `credentials` holds its text.
///
/// ```
/// let servers = r#"{"docs": {"command": "docs", "env": {"DOCS_TOKEN": "t1"}}}"#;
/// let var = |name: &str| match name {
///     "GH_TOKEN" => Some("gho_x1y2".to_owned()),
///     riff::session_mcp::VAR => Some(servers.to_owned()),
///     _ => None,
/// };
/// let file = r#"{"claudeAiOauth":{"accessToken":"sk-a1","refreshToken":"sk-r1","expiresAt":0}}"#;
/// let secrets = riff::session_relay::secrets(var, Some(file));
/// assert_eq!(secrets, ["gho_x1y2", "t1", "sk-a1", "sk-r1"]);
/// assert!(riff::session_relay::secrets(|_| None, Some("no json")).is_empty());
/// ```
pub fn secrets(var: impl Fn(&str) -> Option<String>, credentials: Option<&str>) -> Vec<String> {
    let mut secrets: Vec<String> = SECRET_VARS
        .iter()
        .filter_map(|name| var(name))
        .filter(|v| !v.is_empty())
        .collect();
    let servers = crate::session_mcp::from_var(var(crate::session_mcp::VAR).as_deref());
    secrets.extend(crate::session_mcp::secrets(&servers));
    if let Some(file) = credentials
        && let Ok(sign_in) = crate::run::sign_in(file, 0)
    {
        secrets.extend(sign_in.secrets);
    }
    secrets
}

/// [`secrets`] of this process: its variables, with the MCP servers that
/// the daemon gives it, and the sign-in file in `CLAUDE_CONFIG_DIR`, else
/// in `HOME/.claude`. So a daemon that takes up the session after its
/// restart gets each event scrubbed by the relay.
pub fn secrets_here() -> Vec<String> {
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude")));
    let file = dir.and_then(|d| std::fs::read_to_string(d.join(".credentials.json")).ok());
    secrets(|name| std::env::var(name).ok(), file.as_deref())
}

/// The MCP config that gives `claude` the riff tools: the server
/// [`MCP_SERVER`] runs `bin relay mcp`. Each server of `more`, the MCP
/// servers of the host ([`crate::session_mcp`]), goes beside it. `riff`
/// stays the riff of the relay.
///
/// ```
/// let more = serde_json::json!({"docs": {"type": "http", "url": "https://d"}, "riff": {}});
/// let config = riff::session_relay::mcp_config("/opt/riff/riff".as_ref(), more.as_object().unwrap());
/// assert_eq!(config["mcpServers"]["riff"]["command"], "/opt/riff/riff");
/// assert_eq!(config["mcpServers"]["riff"]["args"], serde_json::json!(["relay", "mcp"]));
/// assert_eq!(config["mcpServers"]["docs"]["url"], "https://d");
/// ```
pub fn mcp_config(bin: &Path, more: &Map<String, Value>) -> Value {
    let mut servers = more.clone();
    servers.insert(
        MCP_SERVER.into(),
        json!({
            "type": "stdio",
            "command": bin.to_string_lossy(),
            "args": ["relay", "mcp"],
        }),
    );
    json!({ "mcpServers": servers })
}

/// `command` with the riff tools and the servers of `more`: the option
/// `--mcp-config` with [`mcp_config`] goes last, because it takes more
/// than one value.
///
/// ```
/// let command = ["claude".to_owned(), "-p".to_owned()];
/// let with = riff::session_relay::with_tools(&command, "/opt/riff/riff".as_ref(), &Default::default());
/// assert_eq!(with[..3], ["claude", "-p", "--mcp-config"]);
/// assert!(with[3].contains("\"relay\",\"mcp\""));
/// ```
pub fn with_tools(command: &[String], bin: &Path, more: &Map<String, Value>) -> Vec<String> {
    let mut with = command.to_vec();
    with.push("--mcp-config".into());
    with.push(mcp_config(bin, more).to_string());
    with
}

/// The MCP servers of the host that the daemon gives this process in
/// [`crate::session_mcp::VAR`].
fn more_here() -> Map<String, Value> {
    crate::session_mcp::from_var(std::env::var(crate::session_mcp::VAR).ok().as_deref())
}

/// The command of `claude` that goes on after a takeover:
/// `--resume ID` in place of `--session-id ID`
/// (01M4Z60B83ZH10HYKW7X1C0SSS).
///
/// ```
/// let command: Vec<String> = ["claude", "-p", "--session-id", "s1", "--verbose"]
///     .map(String::from)
///     .into();
/// assert_eq!(
///     riff::session_relay::resumed(&command),
///     ["claude", "-p", "--resume", "s1", "--verbose"]
/// );
/// ```
pub fn resumed(command: &[String]) -> Vec<String> {
    command
        .iter()
        .map(|arg| {
            if arg == "--session-id" {
                "--resume".to_owned()
            } else {
                arg.clone()
            }
        })
        .collect()
}

/// Starts `claude` again after a takeover, for [`relay`].
pub type Again = Box<dyn FnMut() -> Result<Child> + Send>;

/// The process of `claude`, for [`relay`].
pub struct Child {
    /// Its stdin: the relay writes each input as one line.
    pub stdin: Box<dyn AsyncWrite + Send + Unpin>,
    /// Its stdout: one event on each line.
    pub stdout: Box<dyn AsyncRead + Send + Unpin>,
    /// Its stderr.
    pub stderr: Box<dyn AsyncRead + Send + Unpin>,
    /// Its end: the exit code, or none after a signal.
    pub end: Pin<Box<dyn Future<Output = Option<i32>> + Send>>,
}

/// Writes `value` as one line on `write`.
async fn send(write: &mut OwnedWriteHalf, value: &impl Serialize) -> std::io::Result<()> {
    let mut line = serde_json::to_string(value)?;
    line.push('\n');
    write.write_all(line.as_bytes()).await
}

/// Connects to the socket at `socket`, through a descriptor of its
/// folder, so that a long path works.
async fn connect(socket: &Path) -> std::io::Result<UnixStream> {
    let (Some(dir), Some(name)) = (socket.parent(), socket.file_name()) else {
        return Err(std::io::ErrorKind::InvalidInput.into());
    };
    daemon::connect_in(dir, &name.to_string_lossy()).await
}

/// What the relay knows of `claude` across its connections.
struct Link {
    /// The stdin of `claude`, until the close.
    stdin: Option<Box<dyn AsyncWrite + Send + Unpin>>,
    /// The number of inputs that the relay gave `claude`.
    inputs: u64,
    /// True after the close of the stdin of `claude`.
    closed: bool,
    /// The lines that the daemon did not take.
    kept: Kept,
    /// The sequence number and the code of the exit of `claude`.
    exit: Option<(u64, Option<i32>)>,
    /// The end of the wait for the daemon, after the exit.
    deadline: Option<Instant>,
    /// True when the output of `claude` ended.
    done: bool,
    /// True after [`Say::Hold`]: the end of `claude` is no exit.
    hold: bool,
    /// While `claude` waits for [`Say::GoOn`] after a takeover: the
    /// sequence number of its next event.
    held: Option<u64>,
    /// Starts `claude` again after a takeover.
    again: Again,
    /// The lines of each `claude` go here.
    ups: mpsc::UnboundedSender<Up>,
    scrub: Scrub,
    notes: mpsc::UnboundedSender<String>,
}

/// Sends each line of the stdout of one `claude` to `ups` as an event,
/// with no secret of `scrub`, from the sequence number `first` on. Then
/// it sends the exit, with the next number.
fn read_claude(
    stdout: Box<dyn AsyncRead + Send + Unpin>,
    end: Pin<Box<dyn Future<Output = Option<i32>> + Send>>,
    first: u64,
    scrub: Scrub,
    ups: mpsc::UnboundedSender<Up>,
) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        let mut seq = first;
        while let Ok(Some(line)) = lines.next_line().await {
            let line = scrub.clean(&line).into_owned();
            if ups.send(Up::Event { seq, line }).is_err() {
                return;
            }
            seq += 1;
        }
        let code = end.await;
        let _ = ups.send(Up::Exit { seq, code });
    });
}

/// Sends each line of the stderr of one `claude` to `notes`, with no
/// secret of `scrub`.
fn read_notes(
    stderr: Box<dyn AsyncRead + Send + Unpin>,
    scrub: Scrub,
    notes: mpsc::UnboundedSender<String>,
) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = notes.send(scrub.clean(&line).into_owned());
        }
    });
}

impl Link {
    /// Reads the output of `child`, from the sequence number `first` on.
    /// It gives the stdin of `child`.
    fn read(&mut self, child: Child, first: u64) -> Box<dyn AsyncWrite + Send + Unpin> {
        let Child {
            stdin,
            stdout,
            stderr,
            end,
        } = child;
        read_claude(stdout, end, first, self.scrub.clone(), self.ups.clone());
        read_notes(stderr, self.scrub.clone(), self.notes.clone());
        stdin
    }

    /// A takeover: `claude` ends after its turn, and its end is no exit.
    /// With `interrupt`, the relay first sends `claude` [`INTERRUPT`]:
    /// it ends its turn at once.
    async fn hold(&mut self, interrupt: bool) {
        if self.held.is_none() && self.exit.is_none() {
            self.hold = true;
            if interrupt && let Some(stdin) = self.stdin.as_mut() {
                let _ = stdin.write_all(format!("{INTERRUPT}\n").as_bytes()).await;
                let _ = stdin.flush().await;
            }
            self.close().await;
        }
    }

    /// The takeover ended: starts `claude` again. When it cannot start,
    /// that is the exit of `claude`. It gives the line for the daemon.
    fn go_on(&mut self, wait: Duration) -> Option<Up> {
        let next = self.held.take()?;
        match (self.again)() {
            Ok(child) => {
                let stdin = self.read(child, next);
                self.stdin = Some(stdin);
                self.closed = false;
                self.done = false;
                None
            }
            Err(e) => {
                let _ = self.notes.send(format!("{e:#}"));
                take(
                    self,
                    Some(Up::Exit {
                        seq: next,
                        code: None,
                    }),
                    wait,
                )
            }
        }
    }

    /// Gives `claude` the input `line`.
    async fn input(&mut self, line: &str) {
        if let Some(stdin) = self.stdin.as_mut() {
            let _ = stdin.write_all(format!("{line}\n").as_bytes()).await;
            let _ = stdin.flush().await;
        }
        self.inputs += 1;
    }

    /// Closes the stdin of `claude`: it ends after its turn.
    async fn close(&mut self) {
        if let Some(mut stdin) = self.stdin.take() {
            let _ = stdin.shutdown().await;
        }
        self.closed = true;
    }

    /// The exit code of `claude`, when the daemon has its exit.
    fn acked(&self, seq: u64) -> Option<Option<i32>> {
        self.exit.filter(|(at, _)| *at <= seq).map(|(_, code)| code)
    }
}

/// Runs the relay for `child` on the socket at `socket` until `claude`
/// ends and the daemon has its exit, or [`relay`]'s `wait` after the
/// exit. After a takeover, `again` starts `claude` again. It gives the
/// exit code of `claude`. Each line of the stderr of `claude` and each
/// note of the relay goes to `notes`, with no secret of `scrub`.
pub async fn relay(
    child: Child,
    again: Again,
    socket: &Path,
    scrub: Scrub,
    wait: Duration,
    notes: mpsc::UnboundedSender<String>,
) -> Option<i32> {
    let (ups, mut from_claude) = mpsc::unbounded_channel::<Up>();
    let mut link = Link {
        stdin: None,
        inputs: 0,
        closed: false,
        kept: Kept::default(),
        exit: None,
        deadline: None,
        done: false,
        hold: false,
        held: None,
        again,
        ups,
        scrub,
        notes: notes.clone(),
    };
    let stdin = link.read(child, 1);
    link.stdin = Some(stdin);
    let mut away = false;
    loop {
        if let Ok(conn) = connect(socket).await {
            if away {
                let _ = notes.send(text::RELAY_BACK.into());
                away = false;
            }
            if let Some(code) = connected(conn, &mut link, &mut from_claude, wait, &notes).await {
                return code;
            }
        }
        if !away {
            let _ = notes.send(text::RELAY_AWAY.into());
            away = true;
        }
        // While the daemon is away, take the lines of claude, and try
        // the socket again after a moment.
        let sleep = tokio::time::sleep(RETRY);
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                () = &mut sleep => break,
                up = from_claude.recv(), if !link.done => {
                    take(&mut link, up, wait);
                }
                () = until(link.deadline) => {
                    let _ = notes.send(text::RELAY_GAVE_UP.into());
                    return link.exit.and_then(|(_, code)| code);
                }
            }
        }
    }
}

/// Waits until `deadline`, or for ever with none.
async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Keeps the line `up` of `claude` in `link`, and gives the line for
/// the daemon. The exit starts the wait for the daemon. After
/// [`Say::Hold`], the end of `claude` is [`Up::Held`]: the relay keeps
/// no copy of it.
fn take(link: &mut Link, up: Option<Up>, wait: Duration) -> Option<Up> {
    let Some(up) = up else {
        link.done = true;
        return None;
    };
    if let Up::Exit { seq, code } = up {
        link.done = true;
        if link.hold {
            link.hold = false;
            link.held = Some(seq);
            return Some(Up::Held);
        }
        link.exit = Some((seq, code));
        link.deadline = Some(Instant::now() + wait);
    }
    link.kept.push(up.clone());
    Some(up)
}

/// Relays on one connection `conn`. It gives `Some` with the exit code
/// of `claude` when the relay is done, and `None` when the daemon went
/// away.
async fn connected(
    conn: UnixStream,
    link: &mut Link,
    from_claude: &mut mpsc::UnboundedReceiver<Up>,
    wait: Duration,
    notes: &mpsc::UnboundedSender<String>,
) -> Option<Option<i32>> {
    let (read, mut write) = conn.into_split();
    let mut lines = BufReader::new(read).lines();
    let hello = SessionAsk::Relay {
        protocol: PROTOCOL,
        inputs: link.inputs,
        closed: link.closed,
        held: link.held.is_some(),
        interrupts: true,
    };
    send(&mut write, &hello).await.ok()?;
    match serde_json::from_str::<Say>(&lines.next_line().await.ok()??) {
        Ok(Say::Resume { events }) => link.kept.taken(events),
        Ok(Say::Failed { error }) => {
            let _ = notes.send(error);
            return None;
        }
        _ => return None,
    }
    for up in link.kept.lines() {
        send(&mut write, up).await.ok()?;
    }
    loop {
        tokio::select! {
            up = from_claude.recv(), if !link.done => {
                if let Some(up) = take(link, up, wait) {
                    send(&mut write, &up).await.ok()?;
                }
            }
            line = lines.next_line() => {
                // A line of a newer daemon that this relay does not
                // know: skip it.
                match serde_json::from_str::<Say>(&line.ok()??) {
                    Ok(Say::Input { line }) => link.input(&line).await,
                    Ok(Say::Close) => link.close().await,
                    Ok(Say::Hold { interrupt }) => link.hold(interrupt).await,
                    Ok(Say::GoOn) => {
                        if let Some(up) = link.go_on(wait) {
                            send(&mut write, &up).await.ok()?;
                        }
                    }
                    Ok(Say::Ack { seq }) => {
                        link.kept.taken(seq);
                        if let Some(code) = link.acked(seq) {
                            return Some(code);
                        }
                    }
                    _ => {}
                }
            }
            () = until(link.deadline) => {
                let _ = notes.send(text::RELAY_GAVE_UP.into());
                return Some(link.exit.and_then(|(_, code)| code));
            }
        }
    }
}

/// The socket of the session of this process: [`daemon::SOCKET_VAR`].
pub fn socket_here() -> Result<PathBuf> {
    std::env::var_os(daemon::SOCKET_VAR)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("{}", text::RELAY_NO_SOCKET))
}

/// Starts `command` with pipes for its stdin, stdout and stderr, and
/// keeps its process ID in `pid`.
fn spawn_claude(command: &[String], pid: &Arc<AtomicU32>) -> Result<Child> {
    let (program, args) = command
        .split_first()
        .ok_or_else(|| anyhow!("{}", text::RELAY_NO_COMMAND))?;
    let mut child = tokio::process::Command::new(program)
        .env_remove(crate::session_mcp::VAR)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| text::no_program(program))?;
    pid.store(child.id().unwrap_or(0), Ordering::SeqCst);
    let (stdin, stdout, stderr) = (
        child.stdin.take().context("the stdin of claude")?,
        child.stdout.take().context("the stdout of claude")?,
        child.stderr.take().context("the stderr of claude")?,
    );
    Ok(Child {
        stdin: Box::new(stdin),
        stdout: Box::new(stdout),
        stderr: Box::new(stderr),
        end: Box::pin(async move { child.wait().await.ok().and_then(|s| s.code()) }),
    })
}

/// `riff relay -- COMMAND`: starts `COMMAND` with the riff tools, and
/// relays it on the socket of `RIFF_SOCKET` ([`relay`]). After a
/// takeover, it starts the [`resumed`] command. A SIGTERM or a SIGINT
/// goes to `claude`. It gives the exit code of `claude`, or 1 after a
/// signal.
pub async fn main(command: Vec<String>) -> Result<i32> {
    use tokio::signal::unix::{SignalKind, signal};
    let socket = socket_here()?;
    let bin = std::env::current_exe().context("cannot find the path of riff")?;
    let command = with_tools(&command, &bin, &more_here());
    let again_command = resumed(&command);
    // The handlers come first: a signal before them ends the relay, and
    // not claude.
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    let pid = Arc::new(AtomicU32::new(0));
    let child = spawn_claude(&command, &pid)?;
    {
        let pid = pid.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = term.recv() => {}
                    _ = int.recv() => {}
                }
                let pid = pid.load(Ordering::SeqCst);
                if let Ok(pid) = i32::try_from(pid)
                    && pid > 0
                {
                    let _ = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(pid),
                        nix::sys::signal::Signal::SIGTERM,
                    );
                }
            }
        });
    }
    let (notes, mut lines) = mpsc::unbounded_channel::<String>();
    let written = tokio::spawn(async move {
        // The stderr of the relay can be gone, for example when the
        // daemon that started the container stopped: write and go on.
        let mut stderr = tokio::io::stderr();
        while let Some(line) = lines.recv().await {
            let _ = stderr.write_all(format!("{line}\n").as_bytes()).await;
        }
    });
    let again: Again = Box::new(move || spawn_claude(&again_command, &pid));
    let code = relay(
        child,
        again,
        &socket,
        Scrub::new(secrets_here()),
        WAIT,
        notes,
    )
    .await;
    let _ = written.await;
    Ok(code.unwrap_or(1))
}

/// `riff relay --terminal -- COMMAND`: runs `COMMAND` with the riff
/// tools in the place of this process, so it keeps the terminal: the
/// lead, and the `claude` of a takeover (01M4Z60B81RTZT71PN5HV0GZHX). It
/// gives an error only when the command does not start.
pub fn terminal(command: &[String]) -> anyhow::Error {
    use std::os::unix::process::CommandExt;
    let bin = match std::env::current_exe() {
        Ok(bin) => bin,
        Err(e) => return anyhow!(e).context("cannot find the path of riff"),
    };
    let command = with_tools(command, &bin, &more_here());
    let Some((program, args)) = command.split_first() else {
        return anyhow!("{}", text::RELAY_NO_COMMAND);
    };
    let error = std::process::Command::new(program)
        .args(args)
        .env_remove(crate::session_mcp::VAR)
        .exec();
    anyhow!(error).context(text::no_program(program))
}

/// `riff relay watch`: waits for the next wake of the session through
/// the daemon, on the socket at `socket` ([`SessionAsk::Watch`]), and
/// gives its line. While the daemon is away, it tries the socket again
/// each [`RETRY`].
pub async fn watch(socket: &Path) -> Result<String> {
    loop {
        let Ok(conn) = connect(socket).await else {
            tokio::time::sleep(RETRY).await;
            continue;
        };
        let (read, mut write) = conn.into_split();
        let mut lines = BufReader::new(read).lines();
        if send(&mut write, &SessionAsk::Watch { protocol: PROTOCOL })
            .await
            .is_err()
        {
            continue;
        }
        while let Ok(Some(line)) = lines.next_line().await {
            match serde_json::from_str::<Say>(&line) {
                Ok(Say::Line { text }) => return Ok(text),
                Ok(Say::Failed { error }) => anyhow::bail!("{error}"),
                _ => {}
            }
        }
        // The daemon went away: wait for the next one.
        tokio::time::sleep(RETRY).await;
    }
}

/// `riff relay watch --follow` (01M59235BBXEKPZBRH23C39574): gives the
/// line of each wake of the session to `emit`, as the daemon says it on
/// the socket at `socket` ([`SessionAsk::Follow`]), and never ends. While
/// the daemon is away, it tries the socket again each [`RETRY`].
pub async fn follow(socket: &Path, mut emit: impl FnMut(&str)) -> Result<()> {
    loop {
        let Ok(conn) = connect(socket).await else {
            tokio::time::sleep(RETRY).await;
            continue;
        };
        let (read, mut write) = conn.into_split();
        let mut lines = BufReader::new(read).lines();
        if send(&mut write, &SessionAsk::Follow { protocol: PROTOCOL })
            .await
            .is_err()
        {
            continue;
        }
        while let Ok(Some(line)) = lines.next_line().await {
            match serde_json::from_str::<Say>(&line) {
                Ok(Say::Line { text }) => emit(&text),
                Ok(Say::Failed { error }) => anyhow::bail!("{error}"),
                _ => {}
            }
        }
        // The daemon went away: wait for the next one.
        tokio::time::sleep(RETRY).await;
    }
}

/// `riff relay denied --send` (01M57XY3Q6HEYQG27FE0CEQ7HB): takes each
/// refusal of auto mode of `queue`, and gives them to the daemon on the
/// socket at `socket` ([`SessionAsk::Denied`]). The daemon posts them as
/// the session. Each refusal that the daemon did not post goes back to
/// `queue` (01M57XY42CNR16M5G024PWKANV).
pub async fn send_denials(socket: &Path, queue: &crate::denied::Queue) -> Result<()> {
    let denials = queue.take();
    if denials.is_empty() {
        return Ok(());
    }
    let posted = ask_denied(socket, &denials).await;
    let posted = posted.as_ref().map_or(0, |n| (*n).min(denials.len()));
    queue.keep(&denials[posted..])?;
    if posted < denials.len() {
        anyhow::bail!(
            "the daemon posted {posted} of {} records: riff keeps the rest",
            denials.len()
        );
    }
    Ok(())
}

/// Gives `denials` to the daemon, and gives the number that it posted.
async fn ask_denied(socket: &Path, denials: &[crate::denied::Denial]) -> Result<usize> {
    let conn = connect(socket).await?;
    let (read, mut write) = conn.into_split();
    let mut lines = BufReader::new(read).lines();
    let ask = SessionAsk::Denied {
        protocol: PROTOCOL,
        denials: denials.to_vec(),
    };
    send(&mut write, &ask).await?;
    while let Some(line) = lines.next_line().await? {
        match serde_json::from_str::<Say>(&line) {
            Ok(Say::Posted { count }) => return Ok(count),
            Ok(Say::Failed { error }) => anyhow::bail!("{error}"),
            _ => {}
        }
    }
    anyhow::bail!("the daemon went away")
}

/// `riff relay prompt` (01M5GVQQ8NK3RD9CM3VTGXEHZQ): tells the daemon
/// on the socket at `socket` that the person of the session gave a
/// prompt ([`SessionAsk::Prompt`]). The daemon ends the block of the
/// lead, and says [`Say::Done`].
pub async fn prompt(socket: &Path) -> Result<()> {
    let conn = connect(socket).await?;
    let (read, mut write) = conn.into_split();
    let mut lines = BufReader::new(read).lines();
    send(&mut write, &SessionAsk::Prompt { protocol: PROTOCOL }).await?;
    while let Some(line) = lines.next_line().await? {
        match serde_json::from_str::<Say>(&line) {
            Ok(Say::Done) => return Ok(()),
            Ok(Say::Failed { error }) => anyhow::bail!("{error}"),
            _ => {}
        }
    }
    anyhow::bail!("the daemon went away")
}

/// Asks the daemon on the socket at `socket` who this session is for
/// the release cycle ([`SessionAsk::Caller`]). The answer is
/// [`Say::Caller`].
pub async fn ask_caller(socket: &Path) -> Result<Say> {
    let conn = connect(socket).await?;
    let (read, mut write) = conn.into_split();
    let mut lines = BufReader::new(read).lines();
    send(&mut write, &SessionAsk::Caller { protocol: PROTOCOL }).await?;
    while let Some(line) = lines.next_line().await? {
        match serde_json::from_str::<Say>(&line) {
            Ok(say @ Say::Caller { .. }) => return Ok(say),
            Ok(Say::Failed { error }) => anyhow::bail!("{error}"),
            _ => {}
        }
    }
    anyhow::bail!("the daemon went away")
}

/// Asks the daemon on the socket at `socket` to make `call` on the
/// riff-server of this session ([`SessionAsk::Server`]). The answer is
/// the JSON of [`Say::Served`].
pub async fn ask_server(socket: &Path, call: &daemon::ServerCall) -> Result<Value> {
    let conn = connect(socket).await?;
    let (read, mut write) = conn.into_split();
    let mut lines = BufReader::new(read).lines();
    let ask = SessionAsk::Server {
        protocol: PROTOCOL,
        call: call.clone(),
    };
    send(&mut write, &ask).await?;
    while let Some(line) = lines.next_line().await? {
        match serde_json::from_str::<Say>(&line) {
            Ok(Say::Served { reply }) => return Ok(reply),
            Ok(Say::Failed { error }) => anyhow::bail!("{error}"),
            _ => {}
        }
    }
    anyhow::bail!("the daemon went away")
}

/// The error answer to the request `id` of `claude`: `message`.
///
/// ```
/// let line = riff::session_relay::error_answer(&serde_json::json!(7), "away");
/// let answer: serde_json::Value = serde_json::from_str(&line).unwrap();
/// assert_eq!(
///     answer,
///     serde_json::json!({"jsonrpc": "2.0", "id": 7, "error": {"code": -32603, "message": "away"}})
/// );
/// assert!(!line.contains('\n'));
/// ```
pub fn error_answer(id: &Value, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32603, "message": message}}).to_string()
}

/// The calls of `claude` that the daemon did not answer, by their ID.
#[derive(Debug, Default)]
struct Calls(BTreeMap<String, Value>);

impl Calls {
    /// Notes a line of `claude`. It gives the params of an initialize
    /// request.
    fn of_claude(&mut self, line: &str) -> Option<Value> {
        let message = serde_json::from_str::<Value>(line).ok()?;
        match (message.get("method"), message.get("id")) {
            (Some(method), Some(id)) => {
                self.0.insert(id.to_string(), id.clone());
                (method == "initialize").then(|| message["params"].clone())
            }
            (Some(method), None) if method == "notifications/cancelled" => {
                if let Some(id) = message.pointer("/params/requestId") {
                    self.0.remove(&id.to_string());
                }
                None
            }
            _ => None,
        }
    }

    /// Notes a line of the daemon.
    fn of_daemon(&mut self, line: &str) {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            return;
        };
        if let (None, Some(id)) = (message.get("method"), message.get("id")) {
            self.0.remove(&id.to_string());
        }
    }
}

/// `riff relay mcp`: the riff tools for `claude` on `input` and
/// `output`, through the socket at `socket` ([`SessionAsk::Mcp`]). It
/// ends when `claude` closes `input`.
pub async fn mcp(
    socket: &Path,
    input: impl AsyncRead + Unpin,
    mut output: impl AsyncWrite + Unpin,
) -> Result<()> {
    let mut from_claude = BufReader::new(input).lines();
    let mut calls = Calls::default();
    let mut client: Option<Value> = None;
    let mut waiting: VecDeque<String> = VecDeque::new();
    let mut first = true;
    loop {
        // While the daemon is away, each line of claude waits.
        let conn = loop {
            if let Ok(conn) = connect(socket).await {
                break conn;
            }
            let sleep = tokio::time::sleep(RETRY);
            tokio::pin!(sleep);
            loop {
                tokio::select! {
                    () = &mut sleep => break,
                    line = from_claude.next_line() => match line? {
                        Some(line) => waiting.push_back(line),
                        None => return Ok(()),
                    },
                }
            }
        };
        let (read, mut write) = conn.into_split();
        let mut from_daemon = BufReader::new(read).lines();
        let hello = SessionAsk::Mcp {
            protocol: PROTOCOL,
            client: if first { None } else { client.clone() },
        };
        first = false;
        let mut open = send(&mut write, &hello).await.is_ok();
        while open && let Some(line) = waiting.pop_front() {
            client = calls.of_claude(&line).or(client);
            open = write
                .write_all(format!("{line}\n").as_bytes())
                .await
                .is_ok();
        }
        while open {
            tokio::select! {
                line = from_claude.next_line() => match line? {
                    Some(line) => {
                        client = calls.of_claude(&line).or(client);
                        open = write.write_all(format!("{line}\n").as_bytes()).await.is_ok();
                    }
                    None => return Ok(()),
                },
                line = from_daemon.next_line() => match line {
                    Ok(Some(line)) => {
                        calls.of_daemon(&line);
                        output.write_all(format!("{line}\n").as_bytes()).await?;
                        output.flush().await?;
                    }
                    _ => open = false,
                },
            }
        }
        // The daemon went away: each call in flight gets an error, so
        // that claude does not wait for ever.
        for id in std::mem::take(&mut calls.0).into_values() {
            let line = error_answer(&id, text::RELAY_CALL_LOST);
            output.write_all(format!("{line}\n").as_bytes()).await?;
        }
        output.flush().await?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixListener;
    use tokio::net::unix::OwnedReadHalf;

    #[test]
    fn the_scrub_of_the_relay_hides_the_values_of_the_added_mcp_servers() {
        let servers = json!({
            "docs": {"command": "docs-mcp", "env": {"DOCS_TOKEN": "env-s3cret"}},
            "unifi": {
                "type": "http",
                "url": "https://unifi.example",
                "headers": {"Authorization": "Bearer head-s3cret"},
            },
        })
        .to_string();
        let var = |name: &str| (name == crate::session_mcp::VAR).then(|| servers.clone());
        let scrub = Scrub::new(secrets(var, None));
        let line = r#"{"tool":"docs","env":"env-s3cret","auth":"Bearer head-s3cret","url":"https://unifi.example"}"#;
        assert_eq!(
            scrub.clean(line),
            format!(
                r#"{{"tool":"docs","env":"{0}","auth":"{0}","url":"https://unifi.example"}}"#,
                crate::run::HIDDEN
            )
        );
    }

    /// A fake daemon on a socket in a temp folder.
    struct Daemon {
        _dir: tempfile::TempDir,
        socket: PathBuf,
        listener: Option<UnixListener>,
    }

    impl Daemon {
        fn new() -> Daemon {
            let dir = tempfile::tempdir().unwrap();
            let socket = dir.path().join("riff.sock");
            let listener = Some(UnixListener::bind(&socket).unwrap());
            Daemon {
                _dir: dir,
                socket,
                listener,
            }
        }

        /// Takes the next connection, and reads its hello.
        async fn accept(&self) -> (SessionAsk, Lines, OwnedWriteHalf) {
            let (conn, _) = self.listener.as_ref().unwrap().accept().await.unwrap();
            let (read, write) = conn.into_split();
            let mut lines = BufReader::new(read).lines();
            let hello = lines.next_line().await.unwrap().unwrap();
            (serde_json::from_str(&hello).unwrap(), lines, write)
        }

        /// Stops listening, as a daemon that stops.
        fn stop(&mut self) {
            self.listener = None;
            let _ = std::fs::remove_file(&self.socket);
        }

        /// Listens again, as a new daemon.
        fn start(&mut self) {
            self.listener = Some(UnixListener::bind(&self.socket).unwrap());
        }
    }

    type Lines = tokio::io::Lines<BufReader<OwnedReadHalf>>;

    async fn up(lines: &mut Lines) -> Up {
        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap()
    }

    /// A fake claude: the test writes its stdout and reads its stdin.
    struct Claude {
        stdout: tokio::io::DuplexStream,
        stdin: BufReader<tokio::io::DuplexStream>,
        stderr: tokio::io::DuplexStream,
        end: Option<tokio::sync::oneshot::Sender<Option<i32>>>,
    }

    fn claude() -> (Claude, Child) {
        let (out_claude, out_relay) = tokio::io::duplex(1 << 16);
        let (in_relay, in_claude) = tokio::io::duplex(1 << 16);
        let (err_claude, err_relay) = tokio::io::duplex(1 << 16);
        let (end, ended) = tokio::sync::oneshot::channel();
        (
            Claude {
                stdout: out_claude,
                stdin: BufReader::new(in_claude),
                stderr: err_claude,
                end: Some(end),
            },
            Child {
                stdin: Box::new(in_relay),
                stdout: Box::new(out_relay),
                stderr: Box::new(err_relay),
                end: Box::pin(async move { ended.await.ok().flatten() }),
            },
        )
    }

    impl Claude {
        async fn print(&mut self, line: &str) {
            self.stdout
                .write_all(format!("{line}\n").as_bytes())
                .await
                .unwrap();
        }

        async fn read(&mut self) -> Option<String> {
            let mut line = String::new();
            match self.stdin.read_line(&mut line).await.unwrap() {
                0 => None,
                _ => Some(line.trim_end().to_owned()),
            }
        }

        /// claude ends with `code`: its stdout closes.
        async fn end(mut self, code: i32) {
            self.stdout.shutdown().await.unwrap();
            drop(self.stdout);
            self.end.take().unwrap().send(Some(code)).unwrap();
        }
    }

    async fn say(write: &mut OwnedWriteHalf, say: &Say) {
        send(write, say).await.unwrap();
    }

    /// No second `claude`: a test with no takeover.
    fn no_again() -> Again {
        Box::new(|| Err(anyhow!("no takeover in this test")))
    }

    #[tokio::test]
    async fn a_takeover_interrupts_the_turn_of_claude_and_the_relay_says_held() {
        let daemon = Daemon::new();
        let (mut claude, child) = claude();
        let (notes, _noted) = mpsc::unbounded_channel();
        let socket = daemon.socket.clone();
        let relay = tokio::spawn(async move {
            relay(child, no_again(), &socket, Scrub::default(), WAIT, notes).await
        });
        let (_, mut lines, mut write) = daemon.accept().await;
        say(&mut write, &Say::Resume { events: 0 }).await;
        say(
            &mut write,
            &Say::Input {
                line: "start".into(),
            },
        )
        .await;
        assert_eq!(claude.read().await.as_deref(), Some("start"));
        claude
            .print(r#"{"type":"assistant","message":{"content":[{"type":"tool_use"}]}}"#)
            .await;
        assert_eq!(up(&mut lines).await.seq(), 1);

        // The hold with interrupt: claude gets the interrupt, then the
        // end of its stdin.
        say(&mut write, &Say::Hold { interrupt: true }).await;
        assert_eq!(claude.read().await.as_deref(), Some(INTERRUPT));
        assert_eq!(claude.read().await, None);
        // claude stops its turn with an error, and ends with 1: that is
        // no exit, the relay says held.
        claude
            .print(r#"{"type":"control_response","response":{"subtype":"success","request_id":"riff-takeover"}}"#)
            .await;
        claude
            .print(r#"{"type":"result","subtype":"error_during_execution","is_error":true}"#)
            .await;
        claude.end(1).await;
        assert_eq!(up(&mut lines).await.seq(), 2);
        assert_eq!(up(&mut lines).await.seq(), 3);
        assert_eq!(up(&mut lines).await, Up::Held);
        drop((lines, write));
        relay.abort();
    }

    #[tokio::test]
    async fn a_takeover_at_the_end_ends_claude_after_its_turn_and_a_go_on_starts_it_again() {
        let mut daemon = Daemon::new();
        let (mut first, child) = claude();
        let (mut second, again) = claude();
        let mut again = Some(again);
        let again: Again = Box::new(move || again.take().ok_or_else(|| anyhow!("twice")));
        let (notes, _noted) = mpsc::unbounded_channel();
        let socket = daemon.socket.clone();
        let relay = tokio::spawn(async move {
            relay(child, again, &socket, Scrub::default(), WAIT, notes).await
        });

        let (_, mut lines, mut write) = daemon.accept().await;
        say(&mut write, &Say::Resume { events: 0 }).await;
        say(
            &mut write,
            &Say::Input {
                line: "start".into(),
            },
        )
        .await;
        assert_eq!(first.read().await.as_deref(), Some("start"));
        first.print("e1").await;
        assert_eq!(up(&mut lines).await.seq(), 1);
        // The hold with no interrupt closes the stdin: claude ends after
        // its turn, and its end is no exit.
        say(&mut write, &Say::Hold { interrupt: false }).await;
        assert_eq!(first.read().await, None);
        first.print("e2").await;
        first.end(0).await;
        assert_eq!(up(&mut lines).await.seq(), 2);
        assert_eq!(up(&mut lines).await, Up::Held);

        // The daemon goes away during the takeover: the hello of the new
        // daemon says that the relay waits.
        say(&mut write, &Say::Ack { seq: 2 }).await;
        daemon.stop();
        drop((lines, write));
        daemon.start();
        let (hello, mut lines, mut write) = daemon.accept().await;
        assert_eq!(
            hello,
            SessionAsk::Relay {
                protocol: PROTOCOL,
                inputs: 1,
                closed: true,
                held: true,
                interrupts: true,
            }
        );
        say(&mut write, &Say::Resume { events: 2 }).await;
        // The go on starts claude again; its events go on from 3.
        say(&mut write, &Say::GoOn).await;
        say(
            &mut write,
            &Say::Input {
                line: "go on".into(),
            },
        )
        .await;
        assert_eq!(second.read().await.as_deref(), Some("go on"));
        second.print("e3").await;
        assert_eq!(
            up(&mut lines).await,
            Up::Event {
                seq: 3,
                line: "e3".into()
            }
        );
        say(&mut write, &Say::Close).await;
        assert_eq!(second.read().await, None);
        second.end(0).await;
        assert_eq!(
            up(&mut lines).await,
            Up::Exit {
                seq: 4,
                code: Some(0)
            }
        );
        say(&mut write, &Say::Ack { seq: 4 }).await;
        assert_eq!(relay.await.unwrap(), Some(0));
    }

    #[tokio::test]
    async fn the_relay_keeps_the_events_while_the_daemon_is_away_and_loses_none() {
        let mut daemon = Daemon::new();
        let (mut claude, child) = claude();
        let (notes, mut noted) = mpsc::unbounded_channel();
        let socket = daemon.socket.clone();
        let relay = tokio::spawn(async move {
            relay(child, no_again(), &socket, Scrub::default(), WAIT, notes).await
        });

        // The first daemon: the start text, two events.
        let (hello, mut lines, mut write) = daemon.accept().await;
        assert_eq!(
            hello,
            SessionAsk::Relay {
                protocol: PROTOCOL,
                inputs: 0,
                closed: false,
                held: false,
                interrupts: true,
            }
        );
        say(&mut write, &Say::Resume { events: 0 }).await;
        say(
            &mut write,
            &Say::Input {
                line: "start".into(),
            },
        )
        .await;
        assert_eq!(claude.read().await.as_deref(), Some("start"));
        claude.print("e1").await;
        claude.print("e2").await;
        assert_eq!(
            up(&mut lines).await,
            Up::Event {
                seq: 1,
                line: "e1".into()
            }
        );
        assert_eq!(
            up(&mut lines).await,
            Up::Event {
                seq: 2,
                line: "e2".into()
            }
        );
        // The daemon took only the first, and stops.
        say(&mut write, &Say::Ack { seq: 1 }).await;
        daemon.stop();
        drop((lines, write));

        // claude goes on while the daemon is away.
        claude.print("e3").await;
        assert_eq!(noted.recv().await.unwrap(), text::RELAY_AWAY);

        // A new daemon has the first event only: the relay sends 2 and
        // 3 again, and gives no input twice.
        daemon.start();
        let (hello, mut lines, mut write) = daemon.accept().await;
        assert_eq!(
            hello,
            SessionAsk::Relay {
                protocol: PROTOCOL,
                inputs: 1,
                closed: false,
                held: false,
                interrupts: true,
            }
        );
        say(&mut write, &Say::Resume { events: 1 }).await;
        assert_eq!(
            up(&mut lines).await,
            Up::Event {
                seq: 2,
                line: "e2".into()
            }
        );
        assert_eq!(
            up(&mut lines).await,
            Up::Event {
                seq: 3,
                line: "e3".into()
            }
        );
        say(&mut write, &Say::Close).await;
        assert_eq!(claude.read().await, None);
        claude.end(0).await;
        assert_eq!(
            up(&mut lines).await,
            Up::Exit {
                seq: 4,
                code: Some(0)
            }
        );
        say(&mut write, &Say::Ack { seq: 4 }).await;
        assert_eq!(relay.await.unwrap(), Some(0));
        assert_eq!(noted.recv().await.unwrap(), text::RELAY_BACK);
    }

    #[tokio::test]
    async fn the_relay_sends_no_secret_and_writes_none_to_its_stderr() {
        let daemon = Daemon::new();
        let (mut claude, child) = claude();
        let (notes, mut noted) = mpsc::unbounded_channel();
        let socket = daemon.socket.clone();
        let scrub = Scrub::new(["gho_secret1".to_owned()]);
        let relay =
            tokio::spawn(
                async move { relay(child, no_again(), &socket, scrub, WAIT, notes).await },
            );
        let (_, mut lines, mut write) = daemon.accept().await;
        say(&mut write, &Say::Resume { events: 0 }).await;
        claude
            .print(r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"gho_secret1"}]}}"#)
            .await;
        claude
            .stderr
            .write_all(b"a token gho_secret1 on stderr\n")
            .await
            .unwrap();
        let Up::Event { line, .. } = up(&mut lines).await else {
            panic!()
        };
        assert!(
            line.contains("[secret]") && !line.contains("gho_secret1"),
            "{line}"
        );
        assert_eq!(noted.recv().await.unwrap(), "a token [secret] on stderr");
        claude.end(3).await;
        assert_eq!(
            up(&mut lines).await,
            Up::Exit {
                seq: 2,
                code: Some(3)
            }
        );
        say(&mut write, &Say::Ack { seq: 2 }).await;
        assert_eq!(relay.await.unwrap(), Some(3));
    }

    #[tokio::test(start_paused = true)]
    async fn after_the_end_of_claude_the_relay_waits_for_the_daemon_only_so_long() {
        let daemon = Daemon::new();
        let (claude, child) = claude();
        let (notes, mut noted) = mpsc::unbounded_channel();
        let socket = daemon.socket.clone();
        drop(daemon);
        let wait = Duration::from_secs(60);
        let relay = tokio::spawn(async move {
            relay(child, no_again(), &socket, Scrub::default(), wait, notes).await
        });
        claude.end(0).await;
        assert_eq!(relay.await.unwrap(), Some(0));
        assert_eq!(noted.recv().await.unwrap(), text::RELAY_AWAY);
        assert_eq!(noted.recv().await.unwrap(), text::RELAY_GAVE_UP);
    }

    #[tokio::test]
    async fn the_relay_skips_a_line_of_a_newer_daemon() {
        let daemon = Daemon::new();
        let (claude, child) = claude();
        let (notes, _noted) = mpsc::unbounded_channel();
        let socket = daemon.socket.clone();
        let relay = tokio::spawn(async move {
            relay(child, no_again(), &socket, Scrub::default(), WAIT, notes).await
        });
        let (_, mut lines, mut write) = daemon.accept().await;
        say(&mut write, &Say::Resume { events: 0 }).await;
        write
            .write_all(b"{\"say\":\"from_the_future\",\"x\":1}\n")
            .await
            .unwrap();
        claude.end(0).await;
        assert_eq!(
            up(&mut lines).await,
            Up::Exit {
                seq: 1,
                code: Some(0)
            }
        );
        say(&mut write, &Say::Ack { seq: 1 }).await;
        assert_eq!(relay.await.unwrap(), Some(0));
    }

    /// Reads one line of `read` as JSON.
    async fn json(read: &mut tokio::io::Lines<BufReader<tokio::io::DuplexStream>>) -> Value {
        serde_json::from_str(&read.next_line().await.unwrap().unwrap()).unwrap()
    }

    #[tokio::test]
    async fn riff_relay_mcp_answers_a_lost_call_with_an_error_and_gives_a_new_daemon_the_client() {
        let mut daemon = Daemon::new();
        let (mut to_mcp, mcp_in) = tokio::io::duplex(1 << 16);
        let (mcp_out, from_mcp) = tokio::io::duplex(1 << 16);
        let mut from_mcp = BufReader::new(from_mcp).lines();
        let socket = daemon.socket.clone();
        let shim = tokio::spawn(async move { mcp(&socket, mcp_in, mcp_out).await });
        let init = r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"claude-code","version":"2"}}}"#;
        to_mcp
            .write_all(format!("{init}\n").as_bytes())
            .await
            .unwrap();

        // The first daemon: the handshake goes through.
        let (hello, mut lines, mut write) = daemon.accept().await;
        assert_eq!(
            hello,
            SessionAsk::Mcp {
                protocol: PROTOCOL,
                client: None
            }
        );
        assert_eq!(lines.next_line().await.unwrap().unwrap(), init);
        write
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{}}\n")
            .await
            .unwrap();
        assert_eq!(json(&mut from_mcp).await["id"], 0);

        // A call, and the daemon stops before its answer.
        let call = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"whoami"}}"#;
        to_mcp
            .write_all(format!("{call}\n").as_bytes())
            .await
            .unwrap();
        assert_eq!(lines.next_line().await.unwrap().unwrap(), call);
        daemon.stop();
        drop((lines, write));
        let lost = json(&mut from_mcp).await;
        assert_eq!(lost["id"], 1);
        assert_eq!(lost["error"]["message"], text::RELAY_CALL_LOST);

        // A call while the daemon is away waits for the new daemon,
        // which gets the initialize request of claude in the hello.
        let again = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"whoami"}}"#;
        to_mcp
            .write_all(format!("{again}\n").as_bytes())
            .await
            .unwrap();
        tokio::time::sleep(RETRY * 2).await;
        daemon.start();
        let (hello, mut lines, mut write) = daemon.accept().await;
        let SessionAsk::Mcp { client, .. } = hello else {
            panic!("{hello:?}")
        };
        assert_eq!(client.unwrap()["clientInfo"]["name"], "claude-code");
        assert_eq!(lines.next_line().await.unwrap().unwrap(), again);
        write
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{}}\n")
            .await
            .unwrap();
        assert_eq!(json(&mut from_mcp).await["id"], 2);
        drop(to_mcp);
        shim.await.unwrap().unwrap();
    }
}
