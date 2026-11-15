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
//! at most [`HELD`] bytes ([`Lines::held`]), whatever the session
//! writes. Each sequence of bytes that is not UTF-8 becomes the
//! replacement character `�`, and the text of a line stays at most
//! [`LINE_MAX`] bytes. [`Line::clean`] takes each secret out, also the
//! start of a secret at the cut ([`Scrub::clean_cut`]), and ends a line
//! that riff cut with [`text::LINE_CUT`].
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

/// The most bytes of a line that a read of [`Lines`] keeps: [`LINE_MAX`],
/// and the `\r` of a `\r\n`.
const KEEP: usize = LINE_MAX + 1;

/// The most bytes that a read of [`Lines`] holds: its buffer, and the
/// kept bytes of a line.
pub const HELD: usize = BUF + KEEP;

/// One line of the output of a container, with no line break.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The kept part of the line: at most [`LINE_MAX`] bytes. Each
    /// sequence of bytes that is not UTF-8 is the replacement character
    /// `�`.
    pub text: String,
    /// riff cut the line: the rest of it went.
    pub cut: bool,
}

impl Line {
    /// The line for the followers of the session: with no secret of
    /// `scrub`. A line that riff cut loses the start of a secret at its
    /// end ([`Scrub::clean_cut`]), and ends with [`text::LINE_CUT`].
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
            text = scrub.clean_cut(&text);
            text.reserve_exact(text::LINE_CUT.len());
            text.push_str(text::LINE_CUT);
        } else if let Cow::Owned(clean) = scrub.clean(&text) {
            text = clean;
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
    /// The bytes of the line so far. It grows to [`KEEP`] bytes at most,
    /// and keeps its size for the next line.
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
    /// line. At most [`HELD`].
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
            let room = KEEP - self.kept.len();
            if let Some(at) = buf.iter().position(|&b| b == b'\n') {
                keep(&mut self.kept, &buf[..at.min(room)]);
                self.from.consume(at + 1);
                return Ok(Some(self.line(at <= room)));
            }
            let n = buf.len();
            keep(&mut self.kept, &buf[..n.min(room)]);
            self.from.consume(n);
            if n > room {
                self.skip = true;
                return Ok(Some(self.line(false)));
            }
        }
    }

    /// The line of the kept bytes, which go. With `broke`, the line
    /// ended at a line break, and `kept` holds each byte of it.
    fn line(&mut self, broke: bool) -> Line {
        let mut bytes = &self.kept[..];
        if broke && let Some(line) = bytes.strip_suffix(b"\r") {
            bytes = line;
        }
        let mut cut = bytes.len() > LINE_MAX;
        if cut {
            bytes = whole_chars(&bytes[..LINE_MAX]);
        }
        let mut text = String::from_utf8_lossy(bytes).into_owned();
        // A replacement character has more bytes than a bad byte.
        if text.len() > LINE_MAX {
            text.truncate(text.floor_char_boundary(LINE_MAX));
            cut = true;
        }
        self.kept.clear();
        Line { text, cut }
    }
}

/// `bytes` with no part of a character at the end: a cut can split one.
fn whole_chars(bytes: &[u8]) -> &[u8] {
    let part = bytes.utf8_chunks().last().map_or(0, |chunk| {
        let bad = chunk.invalid();
        // Only a part of a character is bad at the end and at no place
        // before it.
        match std::str::from_utf8(bad) {
            Err(e) if e.error_len().is_none() => bad.len(),
            _ => 0,
        }
    });
    &bytes[..bytes.len() - part]
}

/// Puts `bytes` at the end of `kept`, which grows to [`KEEP`] bytes at
/// most. The caller gives no more than the room.
fn keep(kept: &mut Vec<u8>, bytes: &[u8]) {
    let need = kept.len() + bytes.len();
    if need > kept.capacity() {
        let size = (kept.capacity() * 2).max(need).min(KEEP);
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

    async fn all(output: impl AsyncRead + Unpin) -> Vec<Line> {
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
        assert!(lines.held() <= HELD, "{}", lines.held());
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
        assert!(lines.held() <= HELD, "{}", lines.held());
    }

    #[tokio::test]
    async fn a_line_of_the_limit_is_whole_and_one_byte_more_is_cut() {
        let at = "a".repeat(LINE_MAX);
        let over = "b".repeat(LINE_MAX + 1);
        let output = format!("{at}\n{over}\nc\n{over}");
        assert_eq!(
            all(output.as_bytes()).await,
            [
                line(&at, false),
                line(&over[..LINE_MAX], true),
                line("c", false),
                line(&over[..LINE_MAX], true)
            ]
        );
    }

    #[tokio::test]
    async fn a_line_of_the_limit_and_a_carriage_return_is_whole() {
        let at = "a".repeat(LINE_MAX);
        let whole = [line(&at, false), line("next", false)];
        // The `\r\n` in the read of the end of the line.
        let output = format!("x\n{at}\r\nnext\n");
        assert_eq!(all(output.as_bytes()).await[1..], whole);
        // The `\r\n` in the next read, and the `\r` and the `\n` in two
        // reads.
        assert_eq!(all(at.as_bytes().chain(&b"\r\nnext\n"[..])).await, whole);
        let parts = at.as_bytes().chain(&b"\r"[..]).chain(&b"\nnext\n"[..]);
        assert_eq!(all(parts).await, whole);
        // A `\r` that ends no line counts.
        let output = format!("{at}\rb\n");
        assert_eq!(all(output.as_bytes()).await, [line(&at, true)]);
    }

    #[tokio::test]
    async fn short_lines_lose_their_line_breaks() {
        assert_eq!(
            all(&b"one\r\ntwo\n\nthree"[..]).await,
            [
                line("one", false),
                line("two", false),
                line("", false),
                line("three", false)
            ]
        );
        assert_eq!(all(&b""[..]).await, []);
    }

    #[tokio::test]
    async fn a_cut_drops_the_part_of_a_character_and_a_bad_byte_is_the_replacement_character() {
        // "é" is two bytes: the cut falls between them.
        let output = format!("{}é tail\n", "a".repeat(LINE_MAX - 1));
        let lines = all(output.as_bytes()).await;
        assert_eq!(lines, [line(&"a".repeat(LINE_MAX - 1), true)]);
        assert_eq!(all(&b"a\xffb\n"[..]).await, [line("a\u{fffd}b", false)]);
    }

    #[tokio::test]
    async fn a_bad_byte_before_a_split_character_leaves_no_replacement_character_at_the_cut() {
        let mut output = b"\xff".to_vec();
        output.extend("a".repeat(LINE_MAX - 2).bytes());
        output.extend("é tail\n".bytes());
        let lines = all(&output[..]).await;
        assert_eq!(lines.len(), 1);
        let Line { text, cut } = &lines[0];
        assert!(cut);
        assert!(text.len() <= LINE_MAX, "{}", text.len());
        assert!(text.starts_with('\u{fffd}'));
        assert!(text.ends_with('a'));
    }

    #[tokio::test]
    async fn a_long_line_of_bad_bytes_has_at_most_the_limit() {
        let output = [&[0xff; LINE_MAX + 10][..], b"\nnext\n"].concat();
        let lines = all(&output[..]).await;
        assert_eq!(lines.len(), 2);
        assert!(lines[0].cut);
        assert!(lines[0].text.len() <= LINE_MAX, "{}", lines[0].text.len());
        assert!(lines[0].text.chars().all(|c| c == '\u{fffd}'));
        assert_eq!(lines[1], line("next", false));
        // A line of the limit grows with its replacement character: riff
        // cuts it.
        let output = [&b"a".repeat(LINE_MAX - 1)[..], b"\xff\n"].concat();
        let lines = all(&output[..]).await;
        assert_eq!(lines, [line(&"a".repeat(LINE_MAX - 1), true)]);
    }

    #[test]
    fn the_space_of_a_line_grows_to_the_limit_at_most() {
        let mut kept = Vec::new();
        for _ in 0..KEEP / 100 {
            keep(&mut kept, &[b'a'; 100]);
        }
        let rest = vec![b'a'; KEEP - kept.len()];
        keep(&mut kept, &rest);
        assert_eq!(kept.len(), KEEP);
        assert_eq!(kept.capacity(), KEEP);
    }

    #[test]
    fn a_clean_cut_line_ends_with_the_mark_and_no_start_of_a_secret() {
        let scrub = Scrub::new(["sk-ant-oat01-secret1".to_owned()]);
        assert_eq!(
            line("token sk-ant-oat01-secret1 and sk-ant-oat01-sec", true).clean(&scrub),
            format!("token [secret] and {}", text::LINE_CUT)
        );
        assert_eq!(line("no secret", false).clean(&scrub), "no secret");
        // A token of GitHub at the cut is a token.
        let scrub = Scrub::default();
        assert_eq!(
            line("token gho_sec", true).clean(&scrub),
            format!("token [secret]{}", text::LINE_CUT)
        );
        // The end of one secret goes, and then the start of the other.
        let scrub = Scrub::new(["sk-ant-oat01-ABCDEF".into(), "zzq_TOKEN".into()]);
        assert_eq!(
            line("x sk-ant-oat01-ABCzz", true).clean(&scrub),
            format!("x {}", text::LINE_CUT)
        );
    }

    #[tokio::test]
    async fn a_whole_token_at_the_cut_goes_whole() {
        // The token ends with its own first byte, and the cut falls just
        // after it.
        let token = "sk-ant-oat01-secrets";
        let a = "a".repeat(LINE_MAX - 1 - token.len());
        let output = format!("{a} {token}MORE BYTES\n");
        let mut lines = Lines::new(output.as_bytes());
        let cut = lines.next_line().await.unwrap().unwrap();
        assert_eq!(cut, line(&format!("{a} {token}"), true));
        let scrub = Scrub::new([token.to_owned()]);
        assert_eq!(cut.clean(&scrub), format!("{a} [secret]{}", text::LINE_CUT));
    }
}
