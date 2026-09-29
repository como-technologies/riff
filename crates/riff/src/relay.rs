//! The stdio of `riff mcp`: it passes each line between Claude Code and
//! the tools, so that `riff mcp` can run a new binary in place and keep
//! the connection (01M3NT6WZTKAFKGDWGCFKC8TB5).
//!
//! # Design
//!
//! Claude Code talks to `riff mcp` over stdin and stdout, one JSON-RPC
//! message on each line. It does not start the server again when it
//! stops. So after an update, `riff mcp` runs the new binary with
//! `exec`: the process, its stdin and its stdout stay.
//!
//! A line that the old process read but did not answer is lost at the
//! `exec`. So the tools do not read stdin themselves. The relay reads
//! it, with no read ahead that it cannot see, and gives each line to
//! the tools over a pipe in memory. It knows each request in flight: a
//! request from Claude Code that the tools did not answer, or that
//! Claude Code did not cancel. It gives the word for the `exec` only
//! when a new binary is on disk, no request is in flight, and it holds
//! no part of a line.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant C as Claude Code
//!     participant R as relay
//!     participant T as tools (rmcp)
//!     C->>R: request id 7 (stdin)
//!     R->>T: request id 7
//!     T->>R: response id 7
//!     R->>C: response id 7 (stdout)
//!     Note over R: a new binary, nothing in flight
//!     R->>R: exec the new riff mcp, stdin and stdout stay
//!     C->>R: request id 8, to the new process
//! ```
//!
//! The new process skips the handshake: the old process gives it the
//! initialize request of Claude Code in a hidden option, and the tools
//! answer the next request at once.

use std::collections::HashSet;
use std::future::Future;
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd};

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use serde_json::Value;
use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

/// The size of the pipe in memory between the relay and the tools, in
/// each direction.
pub const PIPE: usize = 1 << 20;

/// Why the relay ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
    /// Claude Code closed stdin, or the tools ended.
    Closed,
    /// A new binary is on disk, and nothing is in flight: run it now.
    Update,
}

/// The requests of Claude Code that are in flight.
///
/// ```
/// use riff::relay::InFlight;
///
/// let mut flight = InFlight::default();
/// flight.from_client(r#"{"jsonrpc":"2.0","id":7,"method":"tools/call"}"#);
/// flight.from_client(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
/// assert!(!flight.is_empty());
/// flight.from_tools(r#"{"jsonrpc":"2.0","id":7,"result":{}}"#);
/// assert!(flight.is_empty());
///
/// // A cancel ends a request too: the tools do not answer it.
/// flight.from_client(r#"{"jsonrpc":"2.0","id":"a","method":"tools/call"}"#);
/// flight.from_client(
///     r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"a"}}"#,
/// );
/// assert!(flight.is_empty());
/// ```
#[derive(Debug, Default)]
pub struct InFlight(HashSet<String>);

impl InFlight {
    /// Notes a line from Claude Code.
    pub fn from_client(&mut self, line: &str) {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            return;
        };
        match (message.get("method"), message.get("id")) {
            (Some(_), Some(id)) => {
                self.0.insert(id.to_string());
            }
            (Some(method), None) if method == "notifications/cancelled" => {
                if let Some(id) = message.pointer("/params/requestId") {
                    self.0.remove(&id.to_string());
                }
            }
            _ => {}
        }
    }

    /// Notes a line from the tools.
    pub fn from_tools(&mut self, line: &str) {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            return;
        };
        if let (None, Some(id)) = (message.get("method"), message.get("id")) {
            self.0.remove(&id.to_string());
        }
    }

    /// True when no request is in flight.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Stdin of this process, with no buffer of its own.
struct Stdin;

impl AsRawFd for Stdin {
    fn as_raw_fd(&self) -> RawFd {
        io::stdin().as_raw_fd()
    }
}

/// Sets `O_NONBLOCK` on `fd` on or off. The new process of an `exec`
/// gets stdin as the old one leaves it.
fn nonblocking(fd: BorrowedFd<'_>, on: bool) -> io::Result<()> {
    let flags = OFlag::from_bits_truncate(fcntl(fd, FcntlArg::F_GETFL)?);
    let flags = if on {
        flags | OFlag::O_NONBLOCK
    } else {
        flags - OFlag::O_NONBLOCK
    };
    fcntl(fd, FcntlArg::F_SETFL(flags))?;
    Ok(())
}

/// Passes each line of stdin to `tools`, and each line of `tools` to
/// stdout, until Claude Code closes stdin or the tools end. When
/// `update` is ready, it ends with [`Ended::Update`] at the first moment
/// with no request in flight and no part of a line held. Stdin is
/// blocking again when it returns.
pub async fn run(tools: DuplexStream, update: impl Future<Output = ()>) -> io::Result<Ended> {
    let stdin = AsyncFd::new(Stdin)?;
    nonblocking(io::stdin().as_fd(), true)?;
    let ended = pass(&stdin, tools, update).await;
    nonblocking(io::stdin().as_fd(), false)?;
    ended
}

async fn pass(
    stdin: &AsyncFd<Stdin>,
    tools: DuplexStream,
    update: impl Future<Output = ()>,
) -> io::Result<Ended> {
    let (from_tools, mut to_tools) = tokio::io::split(tools);
    let mut from_tools = BufReader::new(from_tools).lines();
    let mut stdout = tokio::io::stdout();
    let mut flight = InFlight::default();
    let mut held: Vec<u8> = Vec::new();
    let mut open = true;
    let mut new = false;
    tokio::pin!(update);
    loop {
        if new && flight.is_empty() && held.is_empty() {
            return Ok(Ended::Update);
        }
        tokio::select! {
            ready = stdin.readable(), if open => {
                let mut ready = ready?;
                let mut buf = [0u8; 8192];
                match nix::unistd::read(io::stdin().as_fd(), &mut buf) {
                    Ok(0) => {
                        open = false;
                        to_tools.shutdown().await?;
                    }
                    Ok(n) => {
                        held.extend_from_slice(&buf[..n]);
                        while let Some(end) = held.iter().position(|&b| b == b'\n') {
                            let line: Vec<u8> = held.drain(..=end).collect();
                            flight.from_client(&String::from_utf8_lossy(&line));
                            to_tools.write_all(&line).await?;
                        }
                    }
                    Err(nix::errno::Errno::EAGAIN) => ready.clear_ready(),
                    Err(nix::errno::Errno::EINTR) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            line = from_tools.next_line() => {
                let Some(line) = line? else { return Ok(Ended::Closed) };
                flight.from_tools(&line);
                stdout.write_all(line.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
            () = &mut update, if !new => new = true,
        }
    }
}
