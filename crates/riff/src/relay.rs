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

/// The `tools/list` requests of Claude Code, and the answers of the
/// tools to them (01M5512RKABQ70QPGC48S5F67W). A refusal means that the
/// session has no riff tools.
///
/// ```
/// use riff::relay::Listing;
///
/// let mut listing = Listing::default();
/// listing.from_client(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
/// listing.from_client(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call"}"#);
/// assert_eq!(listing.from_tools(r#"{"jsonrpc":"2.0","id":3,"result":{}}"#), None);
/// let refused = r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32602,"message":"no _meta"}}"#;
/// assert_eq!(listing.from_tools(refused), Some(false));
///
/// listing.from_client(r#"{"jsonrpc":"2.0","id":4,"method":"tools/list"}"#);
/// assert_eq!(listing.from_tools(r#"{"jsonrpc":"2.0","id":4,"result":{"tools":[]}}"#), Some(true));
/// // Each answer counts once.
/// assert_eq!(listing.from_tools(r#"{"jsonrpc":"2.0","id":4,"result":{"tools":[]}}"#), None);
/// ```
#[derive(Debug, Default)]
pub struct Listing(HashSet<String>);

impl Listing {
    /// Notes a line from Claude Code.
    pub fn from_client(&mut self, line: &str) {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            return;
        };
        if let (Some(method), Some(id)) = (message.get("method"), message.get("id"))
            && method == "tools/list"
        {
            self.0.insert(id.to_string());
        }
    }

    /// Notes a line from the tools: `Some(true)` when it lists the
    /// tools, `Some(false)` when it refuses the list, `None` for each
    /// other line.
    pub fn from_tools(&mut self, line: &str) -> Option<bool> {
        let message = serde_json::from_str::<Value>(line).ok()?;
        if message.get("method").is_some() {
            return None;
        }
        let id = message.get("id")?.to_string();
        self.0.remove(&id).then_some(message.get("error").is_none())
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
/// with no request in flight and no part of a line held. It gives each
/// answer to a `tools/list` to `listed`: true when it lists the tools
/// ([`Listing`]). Stdin is blocking again when it returns.
pub async fn run(
    tools: DuplexStream,
    update: impl Future<Output = ()>,
    listed: impl FnMut(bool),
) -> io::Result<Ended> {
    let stdin = AsyncFd::new(Stdin).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("riff mcp reads stdin from a pipe or a terminal, not from a file: {e}"),
        )
    })?;
    nonblocking(io::stdin().as_fd(), true)?;
    let ended = pass(&stdin, tools, update, listed).await;
    nonblocking(io::stdin().as_fd(), false)?;
    ended
}

async fn pass(
    stdin: &AsyncFd<Stdin>,
    tools: DuplexStream,
    update: impl Future<Output = ()>,
    mut listed: impl FnMut(bool),
) -> io::Result<Ended> {
    let (from_tools, mut to_tools) = tokio::io::split(tools);
    let mut from_tools = BufReader::new(from_tools).lines();
    let mut stdout = tokio::io::stdout();
    let mut flight = InFlight::default();
    let mut listing = Listing::default();
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
                            let text = String::from_utf8_lossy(&line);
                            flight.from_client(&text);
                            listing.from_client(&text);
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
                if let Some(list) = listing.from_tools(&line) {
                    listed(list);
                }
                stdout.write_all(line.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
            () = &mut update, if !new => new = true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_with_a_text_id_counts() {
        let mut listing = Listing::default();
        listing.from_client(r#"{"jsonrpc":"2.0","id":"l1","method":"tools/list"}"#);
        // The same id as a number is another request.
        assert_eq!(
            listing.from_tools(r#"{"jsonrpc":"2.0","id":1,"error":{}}"#),
            None
        );
        let refused = r#"{"jsonrpc":"2.0","id":"l1","error":{"code":-32602,"message":"m"}}"#;
        assert_eq!(listing.from_tools(refused), Some(false));
    }

    #[test]
    fn a_request_of_the_tools_is_no_answer() {
        let mut listing = Listing::default();
        listing.from_client(r#"{"jsonrpc":"2.0","id":5,"method":"tools/list"}"#);
        let ping = r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#;
        assert_eq!(listing.from_tools(ping), None);
        assert_eq!(listing.from_tools("not json"), None);
        let listed = r#"{"jsonrpc":"2.0","id":5,"result":{"tools":[]}}"#;
        assert_eq!(listing.from_tools(listed), Some(true));
    }
}
