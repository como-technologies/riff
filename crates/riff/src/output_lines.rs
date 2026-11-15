//! The output of a container, in lines of at most [`LINE_MAX`] bytes
//! (01M7G7HJH5MHGRNYP2J9QG8SCQ).
//!
//! # Design
//!
//! The daemon reads the stdout and the stderr of each container that it
//! starts: the notes of the relay and the stderr of `claude`
//! ([`crate::runtime::Running`]). A session can write a long output with
//! no line break. The lines of tokio hold the whole line, so such a
//! session grows the memory of the daemon of the host. The daemon reads
//! with [`Lines`] in their place.
//!
//! [`Lines`] keeps at most [`LINE_MAX`] bytes (64 KiB) of a line. Of a
//! longer line, it gives the first [`LINE_MAX`] bytes at once, as a
//! [`Line`] that riff cut, and the rest of the line goes. A read holds
//! its buffer of [`BUF`] bytes and at most [`LINE_MAX`] bytes of the
//! line ([`Lines::held`]), whatever the session writes. [`Line::clean`]
//! takes each secret out, also the start of a secret at the cut
//! ([`Scrub::cut_end`]), and ends a line that riff cut with
//! [`text::LINE_CUT`].
//!
//! ```mermaid
//! flowchart LR
//!     C[the output of a container] -->|BUF bytes at a time| L[Lines]
//!     L -->|at most LINE_MAX bytes of a line| K[Line]
//!     L -->|the rest of a longer line| G[gone]
//!     K -->|clean: no secret, the mark of a cut| D[Say::Stderr]
//! ```
//!
//! ```
//! use riff::output_lines::{LINE_MAX, Lines};
//! use riff::run::Scrub;
//! use riff::text::LINE_CUT;
//!
//! let output = format!("a note\r\n{}\nthe end", "a".repeat(LINE_MAX + 1));
//! let scrub = Scrub::default();
//! let shown = tokio::runtime::Builder::new_current_thread()
//!     .build()?
//!     .block_on(async {
//!         let mut lines = Lines::new(output.as_bytes());
//!         let mut shown = Vec::new();
//!         while let Some(line) = lines.next_line().await? {
//!             shown.push(line.clean(&scrub));
//!         }
//!         Ok::<_, std::io::Error>(shown)
//!     })?;
//! assert_eq!(
//!     shown,
//!     ["a note".to_owned(), "a".repeat(LINE_MAX) + LINE_CUT, "the end".to_owned()]
//! );
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::borrow::Cow;
use std::io;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use crate::run::Scrub;
use crate::text;

/// The most bytes of one line of the output of a container that the
/// daemon keeps: 64 KiB.
pub const LINE_MAX: usize = 64 * 1024;

/// The buffer of a read of [`Lines`]: 8 KiB.
pub const BUF: usize = 8 * 1024;

/// One line of the output of a container, with no line break.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The kept part of the line: at most [`LINE_MAX`] bytes. Each byte
    /// that is not UTF-8 is U+FFFD.
    pub text: String,
    /// riff cut the line: the rest of it went.
    pub cut: bool,
}

impl Line {
    /// The line for the followers of the session: with no secret of
    /// `scrub`. A line that riff cut loses the start of a secret at its
    /// end ([`Scrub::cut_end`]), and ends with [`text::LINE_CUT`].
    ///
    /// ```
    /// use riff::output_lines::Line;
    /// use riff::run::Scrub;
    /// use riff::text::LINE_CUT;
    ///
    /// let scrub = Scrub::new(["sk-ant-oat01-secret1".to_owned()]);
    /// let line = |text: &str, cut| Line { text: text.into(), cut }.clean(&scrub);
    /// assert_eq!(line("a sk-ant-oat01-secret1 b", false), "a [secret] b");
    /// assert_eq!(line("a sk-ant-oat01-sec", true), format!("a {LINE_CUT}"));
    /// ```
    pub fn clean(self, scrub: &Scrub) -> String {
        let Line { mut text, cut } = self;
        if cut {
            let end = scrub.cut_end(&text).len();
            text.truncate(end);
        }
        if let Cow::Owned(clean) = scrub.clean(&text) {
            text = clean;
        }
        if cut {
            text.reserve_exact(text::LINE_CUT.len());
            text.push_str(text::LINE_CUT);
        }
        text
    }
}

/// Reads the lines of the output of a container. Of a line of more than
/// [`LINE_MAX`] bytes, it keeps the first [`LINE_MAX`] bytes, and the
/// rest goes. A line ends at `\n`, and loses a `\r` before it.
pub struct Lines<R> {
    /// The output, with a buffer of [`BUF`] bytes.
    from: BufReader<R>,
    /// The bytes of the line so far. It grows to [`LINE_MAX`] bytes at
    /// most, and keeps its size for the next line.
    kept: Vec<u8>,
    /// The rest of a line that riff cut is still in `from`.
    skip: bool,
}

impl<R: AsyncRead + Unpin> Lines<R> {
    /// The lines of `from`.
    pub fn new(from: R) -> Self {
        Self {
            from: BufReader::with_capacity(BUF, from),
            kept: Vec::new(),
            skip: false,
        }
    }

    /// The bytes that the read holds: its buffer and the space of the
    /// line. At most [`BUF`] and [`LINE_MAX`] together.
    pub fn held(&self) -> usize {
        BUF + self.kept.capacity()
    }

    /// The next line, or `None` at the end of the output. A line of more
    /// than [`LINE_MAX`] bytes comes at once, as a [`Line::cut`]. The next
    /// call drops the rest of that line first.
    pub async fn next_line(&mut self) -> io::Result<Option<Line>> {
        while self.skip {
            let buf = self.from.fill_buf().await?;
            if buf.is_empty() {
                return Ok(None);
            }
            let n = match buf.iter().position(|&b| b == b'\n') {
                Some(at) => {
                    self.skip = false;
                    at + 1
                }
                None => buf.len(),
            };
            self.from.consume(n);
        }
        loop {
            let buf = self.from.fill_buf().await?;
            if buf.is_empty() {
                if self.kept.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(self.line(false)));
            }
            let room = LINE_MAX - self.kept.len();
            if let Some(at) = buf.iter().position(|&b| b == b'\n') {
                keep(&mut self.kept, &buf[..at.min(room)]);
                self.from.consume(at + 1);
                return Ok(Some(self.line(at > room)));
            }
            let n = buf.len();
            keep(&mut self.kept, &buf[..n.min(room)]);
            self.from.consume(n);
            if n > room {
                self.skip = true;
                return Ok(Some(self.line(true)));
            }
        }
    }

    /// The line of the kept bytes, which go.
    fn line(&mut self, cut: bool) -> Line {
        let mut bytes = &self.kept[..];
        if cut {
            // No part of a character at the cut.
            if let Err(e) = std::str::from_utf8(bytes)
                && e.error_len().is_none()
            {
                bytes = &bytes[..e.valid_up_to()];
            }
        } else if let Some(line) = bytes.strip_suffix(b"\r") {
            bytes = line;
        }
        let text = String::from_utf8_lossy(bytes).into_owned();
        self.kept.clear();
        Line { text, cut }
    }
}

/// Puts `bytes` at the end of `kept`, which grows to [`LINE_MAX`] bytes
/// at most. The caller gives no more than the room.
fn keep(kept: &mut Vec<u8>, bytes: &[u8]) {
    let need = kept.len() + bytes.len();
    if need > kept.capacity() {
        let size = (kept.capacity() * 2).max(need).min(LINE_MAX);
        kept.reserve_exact(size - kept.len());
    }
    kept.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    /// 100 MiB.
    const LONG: u64 = 100 * 1024 * 1024;

    async fn all(output: &[u8]) -> Vec<Line> {
        let mut lines = Lines::new(output);
        let mut all = Vec::new();
        while let Some(line) = lines.next_line().await.unwrap() {
            all.push(line);
        }
        all
    }

    fn line(text: &str, cut: bool) -> Line {
        Line {
            text: text.into(),
            cut,
        }
    }

    #[tokio::test]
    async fn an_output_of_100_mib_with_no_line_break_holds_at_most_the_limit() {
        let mut lines = Lines::new(tokio::io::repeat(b'a').take(LONG));
        let first = lines.next_line().await.unwrap().unwrap();
        assert!(first.cut);
        assert_eq!(first.text.len(), LINE_MAX);
        assert!(first.text.capacity() <= LINE_MAX);
        assert!(first.text.bytes().all(|b| b == b'a'));
        // The rest of the line goes, and the output ends.
        assert_eq!(lines.next_line().await.unwrap(), None);
        // The space of the line only grows, so it is the most that the
        // read held.
        assert!(lines.held() <= BUF + LINE_MAX, "{}", lines.held());
        let shown = first.clean(&Scrub::default());
        assert!(shown.ends_with(text::LINE_CUT));
        assert_eq!(shown.len(), LINE_MAX + text::LINE_CUT.len());
    }

    #[tokio::test]
    async fn the_line_after_a_cut_line_comes_whole() {
        let long = tokio::io::repeat(b'a').take(LONG);
        let rest = &b" end of the long line\nnext\nlast"[..];
        let mut lines = Lines::new(long.chain(rest));
        assert!(lines.next_line().await.unwrap().unwrap().cut);
        assert_eq!(lines.next_line().await.unwrap(), Some(line("next", false)));
        assert_eq!(lines.next_line().await.unwrap(), Some(line("last", false)));
        assert_eq!(lines.next_line().await.unwrap(), None);
        assert!(lines.held() <= BUF + LINE_MAX, "{}", lines.held());
    }

    #[tokio::test]
    async fn a_line_of_the_limit_is_whole_and_one_byte_more_is_cut() {
        let at = "a".repeat(LINE_MAX);
        let over = "b".repeat(LINE_MAX + 1);
        let output = format!("{at}\n{over}\nc\n");
        assert_eq!(
            all(output.as_bytes()).await,
            [
                line(&at, false),
                line(&over[..LINE_MAX], true),
                line("c", false)
            ]
        );
    }

    #[tokio::test]
    async fn short_lines_lose_their_line_breaks() {
        assert_eq!(
            all(b"one\r\ntwo\n\nthree").await,
            [
                line("one", false),
                line("two", false),
                line("", false),
                line("three", false)
            ]
        );
        assert_eq!(all(b"").await, []);
    }

    #[tokio::test]
    async fn a_cut_drops_the_part_of_a_character_and_a_bad_byte_is_u_fffd() {
        // "é" is two bytes: the cut falls between them.
        let output = format!("{}é tail\n", "a".repeat(LINE_MAX - 1));
        let lines = all(output.as_bytes()).await;
        assert_eq!(lines, [line(&"a".repeat(LINE_MAX - 1), true)]);
        assert_eq!(all(b"a\xffb\n").await, [line("a\u{fffd}b", false)]);
    }

    #[test]
    fn the_space_of_a_line_grows_to_the_limit_at_most() {
        let mut kept = Vec::new();
        for _ in 0..LINE_MAX / 100 {
            keep(&mut kept, &[b'a'; 100]);
        }
        let rest = vec![b'a'; LINE_MAX - kept.len()];
        keep(&mut kept, &rest);
        assert_eq!(kept.len(), LINE_MAX);
        assert_eq!(kept.capacity(), LINE_MAX);
    }

    #[test]
    fn a_clean_cut_line_ends_with_the_mark_and_no_start_of_a_secret() {
        let scrub = Scrub::new(["gho_secret1".to_owned()]);
        assert_eq!(
            line("token gho_secret1 and gho_sec", true).clean(&scrub),
            format!("token [secret] and {}", text::LINE_CUT)
        );
        assert_eq!(line("no secret", false).clean(&scrub), "no secret");
    }
}
