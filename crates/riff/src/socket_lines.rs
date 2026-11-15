//! The lines of the socket of a session, of at most [`LINE_MAX`] bytes
//! (01M7GEMCQQQT6TN8ABFGWRQBWR).
//!
//! # Design
//!
//! The folder of the socket of each session is a bind with write in its
//! container (01M4ZDTVSP0T1YENKAWGKA114D). So a process in a container
//! can connect to the socket of its session ([`crate::daemon::SOCKET`])
//! and send bytes with no line break. The lines of tokio hold the whole
//! line, so such a process grows the memory of the daemon of the host.
//!
//! The daemon reads each connection on that socket through [`Limited`].
//! It gives the bytes on, and counts the bytes of the line. A line of
//! more than [`LINE_MAX`] bytes (1 MiB) ends the connection: after the
//! first [`LINE_MAX`] bytes of the line, each read fails with
//! [`text::socket_line_too_long`], which names the session. The daemon
//! writes the error into its log at once. On a connection of the
//! protocol of riff, it also sends the error as [`Say::Failed`].
//!
//! The daemon reads the lines with [`Lines`]: a read holds at most
//! [`HELD`] bytes, its buffer and the bytes of one line, whatever the
//! process sends. The riff tools speak MCP on their connection
//! ([`SessionAsk::Mcp`]), and get no line of the error. rmcp reads their
//! lines from [`Limited`] itself, in a buffer of its own that grows by
//! doubling: it holds about twice [`LINE_MAX`] at most.
//!
//! The relay sends each event of `claude` in a line of at most
//! [`LINE_MAX`] bytes ([`crate::session_relay::Up::event`]). The relay of
//! the riff tools sends no longer call ([`crate::session_relay::mcp`]),
//! and the send of the refusals of auto mode sends them in parts
//! ([`crate::session_relay::send_denials`]). So the relay never ends its
//! own connection. A relay of the version before sends each event whole.
//! At a longer event, the daemon keeps [`crate::session_relay::CUT_EVENT`]
//! in its place, and ends the connection: at its next connection, the
//! relay goes on after that event ([`Lines::too_long`],
//! [`crate::session_relay::Up::long_event`], 01M7GPV2HGN527YPAFYK8CNEQY).
//!
//! ```mermaid
//! flowchart LR
//!     P[a process in the container] -->|the socket of the session| L[Limited]
//!     L -->|lines of at most LINE_MAX bytes| R[Lines]
//!     R --> A[the answer of the daemon]
//!     L -->|a longer line| E[the error in the log and in Say::Failed]
//!     E --> X[the end of the connection]
//! ```
//!
//! ```
//! use riff::socket_lines::{LINE_MAX, Lines};
//!
//! let input = format!("a hello\n{}", "a".repeat(LINE_MAX + 1));
//! let name = "riff-issue-12-1a2b3c4d";
//! tokio::runtime::Builder::new_current_thread()
//!     .build()?
//!     .block_on(async {
//!         let mut lines = Lines::new(input.as_bytes(), name);
//!         assert_eq!(lines.next_line().await?, Some("a hello".to_owned()));
//!         let error = lines.next_line().await.unwrap_err();
//!         assert_eq!(error.to_string(), riff::text::socket_line_too_long(name, LINE_MAX));
//!         assert_eq!(lines.too_long().map(<[u8]>::len), Some(LINE_MAX));
//!         Ok::<_, std::io::Error>(())
//!     })?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! [`Say::Failed`]: crate::daemon::Say::Failed
//! [`SessionAsk::Mcp`]: crate::daemon::SessionAsk::Mcp

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader, ReadBuf};

use crate::output_lines::keep;
use crate::text;

/// The most bytes of one line on the socket of a session, with no line
/// break: 1 MiB.
pub const LINE_MAX: usize = 1024 * 1024;

/// The buffer of a read of [`Lines`]: 8 KiB.
pub const BUF: usize = 8 * 1024;

/// The most bytes that a read of [`Lines`] holds: its buffer, and the
/// bytes of one line.
pub const HELD: usize = BUF + LINE_MAX;

/// Reads the lines of a connection on the socket of a session, through
/// [`Limited`]. A line ends at `\n`, and loses a `\r` before it, as in
/// the lines of tokio. A read holds at most [`HELD`] bytes
/// ([`Lines::held`]).
pub struct Lines<R> {
    /// The connection, with a buffer of [`BUF`] bytes.
    from: BufReader<Limited<R>>,
    /// The bytes of the line so far: at most [`LINE_MAX`], because
    /// [`Limited`] gives no more bytes of one line.
    kept: Vec<u8>,
}

impl<R: AsyncRead + Unpin> Lines<R> {
    /// The lines of the connection `from` on the socket of the session
    /// `name`.
    pub fn new(from: R, name: &str) -> Self {
        Self {
            from: BufReader::with_capacity(BUF, Limited::new(from, name)),
            kept: Vec::new(),
        }
    }

    /// The next line, or `None` at the end of the connection. A line of
    /// more than [`LINE_MAX`] bytes fails with
    /// [`text::socket_line_too_long`], and so does each later call. A
    /// line that is not UTF-8 fails.
    pub async fn next_line(&mut self) -> io::Result<Option<String>> {
        loop {
            let buf = self.from.fill_buf().await?;
            if buf.is_empty() {
                if self.kept.is_empty() {
                    return Ok(None);
                }
                return self.line().map(Some);
            }
            let end = buf.iter().position(|&b| b == b'\n');
            let n = end.unwrap_or(buf.len());
            keep(&mut self.kept, &buf[..n], LINE_MAX);
            self.from.consume(n + usize::from(end.is_some()));
            if end.is_some() {
                return self.line().map(Some);
            }
        }
    }

    /// The line of the kept bytes, which go.
    fn line(&mut self) -> io::Result<String> {
        let mut bytes = std::mem::take(&mut self.kept);
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// The bytes that the read holds: its buffer and the space of the
    /// line. At most [`HELD`].
    pub fn held(&self) -> usize {
        BUF + self.kept.capacity()
    }

    /// After a line of more than [`LINE_MAX`] bytes: its first
    /// [`LINE_MAX`] bytes. `None` before such a line.
    pub fn too_long(&self) -> Option<&[u8]> {
        self.from.get_ref().over.then_some(&self.kept[..])
    }

    /// The connection, with each byte after the last line, for a reader
    /// of its own: rmcp reads the riff tools.
    pub fn into_inner(self) -> BufReader<Limited<R>> {
        self.from
    }
}

/// Reads a connection on the socket of a session, and fails at a line of
/// more than [`LINE_MAX`] bytes. It gives each byte up to the limit, and
/// then the error [`text::socket_line_too_long`] at each read.
pub struct Limited<R> {
    /// The connection.
    from: R,
    /// The session of the socket, for the error.
    name: String,
    /// The bytes of the line so far, with no line break.
    line: usize,
    /// A line passed the limit: each read fails.
    over: bool,
}

impl<R> Limited<R> {
    /// The connection `from` on the socket of the session `name`.
    pub fn new(from: R, name: &str) -> Self {
        Self {
            from,
            name: name.to_owned(),
            line: 0,
            over: false,
        }
    }

    /// The error of a line that passed the limit.
    fn error(&self) -> io::Error {
        io::Error::new(
            io::ErrorKind::InvalidData,
            text::socket_line_too_long(&self.name, LINE_MAX),
        )
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for Limited<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let me = self.get_mut();
        if me.over {
            return Poll::Ready(Err(me.error()));
        }
        let before = buf.filled().len();
        ready!(Pin::new(&mut me.from).poll_read(cx, buf))?;
        let mut line = me.line;
        for (at, &byte) in buf.filled()[before..].iter().enumerate() {
            line = if byte == b'\n' { 0 } else { line + 1 };
            if line > LINE_MAX {
                me.over = true;
                eprintln!("riff daemon: {}", me.error());
                // The bytes before the one over the limit go on. With
                // none, the read fails now: no byte is the end of the
                // connection.
                buf.set_filled(before + at);
                return Poll::Ready(if at == 0 { Err(me.error()) } else { Ok(()) });
            }
        }
        me.line = line;
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, BufReader};

    const NAME: &str = "riff-issue-12-1a2b3c4d";

    /// 100 MiB.
    const LONG: u64 = 100 * 1024 * 1024;

    #[tokio::test]
    async fn a_line_of_100_mib_with_no_line_break_fails_and_the_read_holds_at_most_the_limit() {
        // After a short line too: its space does not stay.
        let from = (&b"x\n"[..]).chain(tokio::io::repeat(b'a').take(LONG));
        let mut lines = Lines::new(from, NAME);
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("x"));
        let error = lines.next_line().await.unwrap_err();
        assert_eq!(
            error.to_string(),
            text::socket_line_too_long(NAME, LINE_MAX)
        );
        assert!(lines.held() <= HELD, "{}", lines.held());
        assert_eq!(lines.too_long(), Some(&[b'a'; LINE_MAX][..]));
        // Each later read fails too.
        assert!(lines.next_line().await.is_err());
        // The read took the line up to the limit, and one buffer more at
        // most.
        let taken = LONG - lines.into_inner().into_inner().from.into_inner().1.limit();
        assert!(taken <= (LINE_MAX + BUF) as u64, "{taken}");
    }

    #[tokio::test]
    async fn a_line_of_the_limit_passes_and_the_lines_before_a_long_line_come() {
        let at = "a".repeat(LINE_MAX);
        let input = format!("x\r\n{at}\nnext\n{at}b\n");
        // One read with each byte, and reads of 8 KiB.
        for size in [4 * LINE_MAX, 8 * 1024] {
            let from = BufReader::with_capacity(size, input.as_bytes());
            let mut lines = Lines::new(from, NAME);
            assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("x"));
            assert_eq!(lines.too_long(), None);
            assert_eq!(lines.next_line().await.unwrap(), Some(at.clone()));
            assert!(lines.held() <= HELD, "{}", lines.held());
            assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("next"));
            assert!(lines.next_line().await.is_err());
            assert_eq!(lines.too_long(), Some(at.as_bytes()));
        }
    }

    #[tokio::test]
    async fn the_last_line_needs_no_line_break_and_a_line_that_is_not_utf_8_fails() {
        let mut lines = Lines::new(&b"one\n\ntwo"[..], NAME);
        for line in ["one", "", "two"] {
            assert_eq!(lines.next_line().await.unwrap().as_deref(), Some(line));
        }
        assert_eq!(lines.next_line().await.unwrap(), None);
        let mut lines = Lines::new(&b"a\xffb\nnext\n"[..], NAME);
        let error = lines.next_line().await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(lines.too_long(), None);
    }

    #[tokio::test]
    async fn the_connection_after_a_line_keeps_the_bytes_that_the_read_holds() {
        let mut lines = Lines::new(&b"hello\nnext\n"[..], NAME);
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("hello"));
        let mut rest = String::new();
        lines.into_inner().read_to_string(&mut rest).await.unwrap();
        assert_eq!(rest, "next\n");
    }

    #[tokio::test]
    async fn a_long_line_at_the_start_of_a_read_fails_the_read_at_once() {
        // The first read ends at the limit, so the next byte opens a read.
        let input = "a".repeat(LINE_MAX + 1);
        let mut limited = Limited::new(input.as_bytes(), NAME);
        let mut buf = vec![0; LINE_MAX];
        assert_eq!(limited.read(&mut buf).await.unwrap(), LINE_MAX);
        assert!(limited.read(&mut buf).await.is_err());
        assert!(limited.read(&mut buf).await.is_err());
    }
}
