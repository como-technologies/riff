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
//! protocol of riff, it also sends the error as [`Say::Failed`]. The
//! riff tools speak MCP on their connection ([`SessionAsk::Mcp`]), and
//! get no such line. A read holds the [`LINE_MAX`] bytes of the line and
//! the buffer of its reader at most, whatever the process sends.
//!
//! The relay sends each event of `claude` in a line of at most
//! [`LINE_MAX`] bytes ([`crate::session_relay::Up::event`]). So the relay
//! never ends its own connection.
//!
//! ```mermaid
//! flowchart LR
//!     P[a process in the container] -->|the socket of the session| L[Limited]
//!     L -->|lines of at most LINE_MAX bytes| A[the answer of the daemon]
//!     L -->|a longer line| E[the error in the log and in Say::Failed]
//!     E --> X[the end of the connection]
//! ```
//!
//! ```
//! use riff::socket_lines::{LINE_MAX, Limited};
//! use tokio::io::{AsyncBufReadExt, BufReader};
//!
//! let input = format!("a hello\n{}", "a".repeat(LINE_MAX + 1));
//! let name = "riff-issue-12-1a2b3c4d";
//! tokio::runtime::Builder::new_current_thread()
//!     .build()?
//!     .block_on(async {
//!         let mut lines = BufReader::new(Limited::new(input.as_bytes(), name)).lines();
//!         assert_eq!(lines.next_line().await?, Some("a hello".to_owned()));
//!         let error = lines.next_line().await.unwrap_err();
//!         assert_eq!(error.to_string(), riff::text::socket_line_too_long(name, LINE_MAX));
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

use tokio::io::{AsyncRead, ReadBuf};

use crate::text;

/// The most bytes of one line on the socket of a session, with no line
/// break: 1 MiB.
pub const LINE_MAX: usize = 1024 * 1024;

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
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

    const NAME: &str = "riff-issue-12-1a2b3c4d";

    /// 100 MiB.
    const LONG: u64 = 100 * 1024 * 1024;

    #[tokio::test]
    async fn a_line_of_100_mib_with_no_line_break_fails_and_the_read_holds_at_most_the_limit() {
        let from = tokio::io::repeat(b'a').take(LONG);
        let mut read = BufReader::new(Limited::new(from, NAME));
        let mut line = String::new();
        let error = read.read_line(&mut line).await.unwrap_err();
        assert_eq!(
            error.to_string(),
            text::socket_line_too_long(NAME, LINE_MAX)
        );
        // The read gave the first LINE_MAX bytes, and its string grows
        // by doubling.
        assert_eq!(line.len(), LINE_MAX);
        assert!(line.capacity() <= 2 * LINE_MAX, "{}", line.capacity());
        // Each later read fails too.
        assert!(read.read_line(&mut String::new()).await.is_err());
        // The read took the line up to the limit, and one buffer more at
        // most.
        let taken = LONG - read.into_inner().from.limit();
        assert!(taken <= (LINE_MAX + 8 * 1024) as u64, "{taken}");
    }

    #[tokio::test]
    async fn a_line_of_the_limit_passes_and_the_lines_before_a_long_line_come() {
        let at = "a".repeat(LINE_MAX);
        let input = format!("x\n{at}\nnext\n{at}b\n");
        // One read with each byte, and reads of 8 KiB.
        for size in [4 * LINE_MAX, 8 * 1024] {
            let mut lines =
                BufReader::with_capacity(size, Limited::new(input.as_bytes(), NAME)).lines();
            assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("x"));
            assert_eq!(lines.next_line().await.unwrap(), Some(at.clone()));
            assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("next"));
            // The read fails. The lines of tokio take no read after a fault.
            assert!(lines.next_line().await.is_err());
        }
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
