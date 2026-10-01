//! The tools of the log: `riff-server log`, `riff-server log verify` and
//! `riff-server log cut`.
//!
//! # Design
//!
//! Each tool works on a [`Store`], with no server. So it runs on the
//! bucket from a laptop, and on a directory in a test.
//!
//! | Tool | Function | What it does |
//! |---|---|---|
//! | `riff-server log` | [`print()`] | Prints each record from a position as one line of text (01M3TJWHNYRCA7RTPFNYM5ZNQS). |
//! | `riff-server log verify` | [`verify`] | Reads each checkpoint and each chunk from the oldest kept checkpoint, and names each line that does not read (01M3TJWHRP49M66NYNHWSYD3XP). |
//! | `riff-server log cut --after POSITION` | [`cut`] | Deletes each record and each checkpoint after the position, and names what it removes (01M3TJWHVN730ZWCWHT9ER186R). |
//!
//! To go back to a position, a person stops the server, runs `verify` to
//! find the first bad record, and cuts before it:
//!
//! ```mermaid
//! flowchart LR
//!     S[stop the server] --> V[log verify:<br/>the first bad record is at P + 1]
//!     V --> C[log cut --after P:<br/>names each removed record]
//!     C --> R[start the server:<br/>it replays up to P]
//! ```
//!
//! - `verify` does not stop at the first problem. It reads each line by
//!   itself, so one run names each bad line, each gap and each repeat of
//!   a position.
//! - `cut` refuses a position before the oldest kept checkpoint: the
//!   chunks before that checkpoint are gone, so no start can replay
//!   them.
//! - `cut` deletes the chunks from the end of the log to its start, then
//!   writes the chunk that holds the position again with only its first
//!   records, then deletes the checkpoints. So a cut that stops in the
//!   middle leaves a log with no gap, and a second run ends it.
//! - `cut` keeps the bytes of each kept line. It never encodes a record
//!   again.
//!
//! # Example
//!
//! ```
//! # #[tokio::main] async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use riff_core::record::{Change, Record, RiffStateSet};
//! use riff_core::wire::RiffState;
//! use riff_server::log::{Timing, write};
//! use riff_server::store::Memory;
//! use riff_server::tools::{cut, print, verify};
//!
//! let store = Memory::default();
//! let record = |position| Record {
//!     position,
//!     written_at_ms: 1_790_000_000_000,
//!     change: Change::RiffStateSet(RiffStateSet { state: RiffState::Running }),
//! };
//! write(&store, &[record(1), record(2), record(3)], &Timing::default(), || true).await?;
//!
//! let mut lines = Vec::new();
//! print(&store, 2, &mut |line| lines.push(line)).await?;
//! assert_eq!(lines[0], "2  2026-09-21T14:13:20Z  riff_state_set  running");
//! assert_eq!(lines.len(), 2);
//!
//! assert!(verify(&store).await?.problems.is_empty());
//!
//! let removed = cut(&store, 1).await?;
//! assert_eq!(removed.records.len(), 2);
//! assert_eq!(verify(&store).await?.last, Some(1));
//! # Ok(()) }
//! ```

use std::collections::BTreeSet;
use std::fmt;

use riff_core::record::{Change, Line, Record};
use riff_core::wire::Kind;

use crate::checkpoint;
use crate::log::{self, Header};
use crate::store::{Store, StoreError};

/// The most characters of a message body that [`show`] prints.
const BODY: usize = 60;

/// Why a tool stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolError {
    /// The store did not do a call.
    Store(StoreError),
    /// The tool refuses, or cannot read an object. The text says why.
    Refused(String),
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolError::Store(error) => write!(f, "{error}"),
            ToolError::Refused(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for ToolError {}

impl From<StoreError> for ToolError {
    fn from(error: StoreError) -> Self {
        ToolError::Store(error)
    }
}

/// A time in milliseconds since the Unix epoch as UTC text, to the
/// second.
///
/// ```
/// assert_eq!(riff_server::tools::utc(1_790_000_000_000), "2026-09-21T14:13:20Z");
/// ```
pub fn utc(ms: u64) -> String {
    i64::try_from(ms)
        .ok()
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map_or_else(
            || format!("{ms} ms"),
            |time| time.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        )
}

/// One record as one line of text: the position, the time, the kind of
/// the change, and its facts. A message shows its thread, its number,
/// its sender, its kind and the start of its body.
///
/// ```
/// use riff_core::record::{Change, Claimed, Record};
/// use riff_server::tools::show;
///
/// let record = Record {
///     position: 1234,
///     written_at_ms: 1_790_000_000_000,
///     change: Change::Claimed(Claimed {
///         session: "riff://ann@heron/acme/app?session=s1".parse()?,
///         thread: "acme/app".parse()?,
///         item: "issue-7".into(),
///     }),
/// };
/// assert_eq!(
///     show(&record),
///     "1234  2026-09-21T14:13:20Z  claimed  issue-7 in acme/app by riff://ann@heron/acme/app?session=s1"
/// );
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn show(record: &Record) -> String {
    let facts = match &record.change {
        Change::Posted(posted) => {
            let message = &posted.message;
            let kind = match message.kind {
                Kind::Message => "message",
                Kind::Status => "status",
                Kind::Note => "note",
            };
            let mut body: String = message.body.chars().take(BODY).collect();
            if message.body.chars().count() > BODY {
                body.push('…');
            }
            format!(
                "posted  {} #{} {kind} from {}, woke {}: {body:?}",
                posted.thread,
                message.seq,
                message.from,
                posted.woken.len()
            )
        }
        Change::JoinedThread(m) => format!("joined_thread  {} by {}", m.thread, m.session),
        Change::LeftThread(m) => format!("left_thread  {} by {}", m.thread, m.session),
        Change::Claimed(c) => format!("claimed  {} in {} by {}", c.item, c.thread, c.session),
        Change::Released(c) => format!("released  {} in {} by {}", c.item, c.thread, c.session),
        Change::LeadSet(m) => format!("lead_set  {} by {}", m.thread, m.session),
        Change::RiffStateSet(s) => format!("riff_state_set  {}", s.state),
        Change::SettingChanged(s) => format!(
            "setting_changed  idle workers: {}",
            serde_json::to_string(&s.idle).unwrap_or_default()
        ),
        Change::SessionForgotten(f) => format!("session_forgotten  {}", f.session),
    };
    format!(
        "{}  {}  {facts}",
        record.position,
        utc(record.written_at_ms)
    )
}

/// The thread that a record names, if any.
fn thread_of(record: &Record) -> Option<String> {
    match &record.change {
        Change::Posted(posted) => Some(posted.thread.to_string()),
        Change::JoinedThread(m) | Change::LeftThread(m) | Change::LeadSet(m) => {
            Some(m.thread.to_string())
        }
        Change::Claimed(c) | Change::Released(c) => Some(c.thread.to_string()),
        Change::RiffStateSet(_) | Change::SettingChanged(_) | Change::SessionForgotten(_) => None,
    }
}

/// One line of the log as text: a record with [`show`], or a record of
/// a kind that this build does not know.
fn show_line(line: &Line) -> String {
    match line {
        Line::Record(record) => show(record),
        Line::Unknown { position, kind } => {
            format!("{position}  {kind}  (a kind that this build does not know)")
        }
    }
}

fn position_of(line: &Line) -> u64 {
    match line {
        Line::Record(record) => record.position,
        Line::Unknown { position, .. } => *position,
    }
}

/// The index of the chunk that holds `position`, or 0.
fn chunk_with(chunks: &[(u64, String)], position: u64) -> usize {
    chunks
        .iter()
        .rposition(|(first, _)| *first <= position)
        .unwrap_or(0)
}

/// What [`print()`] printed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Printed {
    /// The number of lines.
    pub records: u64,
}

/// Gives each record of the log from the position `from` to `emit`, as
/// one line of text ([`show`]). It stops at a chunk that does not read:
/// the error names the chunk, and `riff-server log verify`.
pub async fn print(
    store: &dyn Store,
    from: u64,
    emit: &mut dyn FnMut(String),
) -> Result<Printed, ToolError> {
    let chunks = log::chunks(store).await?;
    let mut printed = Printed::default();
    for (_, name) in chunks.iter().skip(chunk_with(&chunks, from)) {
        let unreadable = |why: String| {
            ToolError::Refused(format!(
                "cannot read {}: {why}. To see each problem, run: riff-server log verify",
                store.locate(name)
            ))
        };
        let Some(loaded) = store.load(name).await? else {
            return Err(unreadable("the chunk is gone".into()));
        };
        let (_, lines) = log::decode(&loaded.bytes).map_err(unreadable)?;
        for line in lines.iter().filter(|line| position_of(line) >= from) {
            emit(show_line(line));
            printed.records += 1;
        }
    }
    Ok(printed)
}

/// A line or an object that does not read, or a break in the positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The full name of the chunk or the checkpoint.
    pub object: String,
    /// The line in the chunk. The header is line 1.
    pub line: Option<usize>,
    pub why: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "{} line {line}: {}", self.object, self.why),
            None => write!(f, "{}: {}", self.object, self.why),
        }
    }
}

/// What [`verify`] read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Verified {
    /// The number of chunks that it read.
    pub chunks: usize,
    /// The number of checkpoints that it read.
    pub checkpoints: usize,
    /// The number of lines that read as a record.
    pub records: u64,
    /// The position of the first record that it read.
    pub first: Option<u64>,
    /// The position of the last record that it read.
    pub last: Option<u64>,
    /// The position of the last record before the first problem of the
    /// log. A cut after it removes each bad record. With no good record,
    /// it is the position before the first chunk.
    pub good: Option<u64>,
    /// True when a chunk has a problem. A cut after [`Verified::good`]
    /// then repairs the log.
    pub bad_log: bool,
    /// Each problem, in the order of the log. The checkpoints come
    /// first.
    pub problems: Vec<Problem>,
}

/// Reads each checkpoint, and each chunk from the oldest kept
/// checkpoint. It names each object and each line that does not read,
/// and each gap or repeat of a position. See the module docs.
pub async fn verify(store: &dyn Store) -> Result<Verified, ToolError> {
    let mut verified = Verified::default();
    let checkpoints = checkpoint::names(store).await?;
    for (position, _, name) in &checkpoints {
        verified.checkpoints += 1;
        let read = match store.load(name).await? {
            Some(loaded) => checkpoint::decode(&loaded.bytes),
            None => Err("the checkpoint is gone".into()),
        };
        let why = match read {
            Ok(checkpoint) if checkpoint.state.position != *position => format!(
                "its name says position {position}, and its state is at position {}",
                checkpoint.state.position
            ),
            Ok(_) => continue,
            Err(why) => format!("does not read: {why}"),
        };
        verified.problems.push(Problem {
            object: store.locate(name),
            line: None,
            why,
        });
    }
    let oldest = checkpoints.first().map(|(position, _, _)| *position);
    let newest = checkpoints.last().map(|(position, _, _)| *position);
    let chunks = log::chunks(store).await?;
    let from = chunk_with(&chunks, oldest.map_or(1, |oldest| oldest + 1));
    // The position that the next record must have.
    let mut next: Option<u64> = None;
    let mut chunk_problems = false;
    for (first, name) in &chunks[from..] {
        verified.chunks += 1;
        if verified.good.is_none() && !chunk_problems {
            verified.good = Some(first.saturating_sub(1));
        }
        // Each problem of this chunk: its line, and why.
        let mut bad: Vec<(Option<usize>, String)> = Vec::new();
        let loaded = store.load(name).await?;
        let text = loaded
            .as_ref()
            .and_then(|loaded| std::str::from_utf8(&loaded.bytes).ok());
        let mut position = *first;
        if let Some(text) = text {
            let mut lines = text.lines();
            match lines.next().map(serde_json::from_str::<Header>) {
                Some(Ok(header)) => {
                    if header.format > log::FORMAT {
                        let why = format!("the chunk has the later format {}", header.format);
                        bad.push((Some(1), why));
                    }
                    if header.first != *first {
                        let why = format!(
                            "the header says position {}, and the name says {first}",
                            header.first
                        );
                        bad.push((Some(1), why));
                    }
                }
                Some(Err(error)) => {
                    bad.push((Some(1), format!("the header does not read: {error}")));
                }
                None => bad.push((None, "the chunk is empty".into())),
            }
            let start = match (next, oldest) {
                (Some(next), _) if next != position => Some(format!(
                    "the chunk starts at position {position}, and the log needs {next}"
                )),
                (None, Some(oldest)) if position > oldest + 1 => Some(format!(
                    "the log starts at position {position}, after the oldest checkpoint at {oldest}"
                )),
                (None, None) if position != 1 => Some(format!(
                    "the log starts at position {position}, and the store has no checkpoint"
                )),
                _ => None,
            };
            bad.extend(start.map(|why| (Some(1), why)));
            for (n, line) in lines.enumerate() {
                let why = match Line::parse(line) {
                    Ok(line) if position_of(&line) == position => None,
                    Ok(line) => Some(format!(
                        "a record has position {}, and the log needs {position}",
                        position_of(&line)
                    )),
                    Err(why) => Some(why),
                };
                match why {
                    Some(why) => bad.push((Some(n + 2), why)),
                    None => {
                        verified.records += 1;
                        verified.first.get_or_insert(position);
                        verified.last = Some(position);
                        if !chunk_problems && bad.is_empty() {
                            verified.good = Some(position);
                        }
                    }
                }
                position += 1;
            }
        } else {
            bad.push((None, "the chunk is gone, or is not text".into()));
        }
        chunk_problems |= !bad.is_empty();
        verified
            .problems
            .extend(bad.into_iter().map(|(line, why)| Problem {
                object: store.locate(name),
                line,
                why,
            }));
        next = Some(position);
    }
    verified.bad_log = chunk_problems;
    let end = next.map_or(0, |next| next - 1);
    if let (Some(newest), Some((_, name))) = (newest, chunks.last())
        && end < newest
    {
        verified.problems.push(Problem {
            object: store.locate(name),
            line: None,
            why: format!(
                "the log ends at position {end}, before the newest checkpoint at {newest}"
            ),
        });
    }
    Ok(verified)
}

/// The last lines of `riff-server log verify`: what it read, and for a
/// bad log the first bad position and the cut that removes it.
///
/// ```
/// use riff_server::tools::{Problem, Verified, verified_text};
///
/// let mut verified = Verified {
///     chunks: 3,
///     checkpoints: 1,
///     records: 6,
///     first: Some(1),
///     last: Some(6),
///     good: Some(6),
///     bad_log: false,
///     problems: Vec::new(),
/// };
/// assert_eq!(
///     verified_text(&verified),
///     "The log reads: 3 chunks, 6 records from position 1 to 6, 1 checkpoint."
/// );
/// verified.good = Some(2);
/// verified.bad_log = true;
/// verified.problems.push(Problem { object: "log/3".into(), line: Some(2), why: "x".into() });
/// assert_eq!(
///     verified_text(&verified),
///     "1 problem in 3 chunks and 1 checkpoint. The last good record of the log is at \
///      position 2.\nTo remove each record after it, stop the server and run: \
///      riff-server log cut --after 2"
/// );
/// ```
pub fn verified_text(verified: &Verified) -> String {
    let chunks = count(verified.chunks as u64, "chunk");
    let checkpoints = count(verified.checkpoints as u64, "checkpoint");
    if verified.problems.is_empty() {
        let range = match (verified.first, verified.last) {
            (Some(first), Some(last)) => format!(" from position {first} to {last}"),
            _ => String::new(),
        };
        return format!(
            "The log reads: {chunks}, {}{range}, {checkpoints}.",
            count(verified.records, "record")
        );
    }
    let mut text = format!(
        "{} in {chunks} and {checkpoints}.",
        count(verified.problems.len() as u64, "problem")
    );
    if verified.bad_log {
        let good = verified.good.unwrap_or(0);
        text.push_str(&format!(
            " The last good record of the log is at position {good}.\n\
             To remove each record after it, stop the server and run: \
             riff-server log cut --after {good}"
        ));
    }
    text
}

/// The last line of `riff-server log cut`: how much it removed, and the
/// threads of the removed records.
///
/// ```
/// use riff_server::tools::{Cut, cut_text};
///
/// let mut removed = Cut::default();
/// assert_eq!(cut_text(&removed, 9), "Nothing is after position 9: removed nothing.");
/// removed.records = vec!["10  …".into(), "11  …".into()];
/// removed.threads.insert("acme/app".into());
/// removed.checkpoints.push("checkpoint/x".into());
/// assert_eq!(
///     cut_text(&removed, 9),
///     "Removed 2 records and 1 checkpoint after position 9. Threads: acme/app."
/// );
/// ```
pub fn cut_text(removed: &Cut, after: u64) -> String {
    if removed.records.is_empty() && removed.checkpoints.is_empty() {
        return format!("Nothing is after position {after}: removed nothing.");
    }
    let threads = if removed.threads.is_empty() {
        "none".to_owned()
    } else {
        removed
            .threads
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "Removed {} and {} after position {after}. Threads: {threads}.",
        count(removed.records.len() as u64, "record"),
        count(removed.checkpoints.len() as u64, "checkpoint")
    )
}

/// A number with its noun: `1 chunk`, `3 chunks`.
fn count(n: u64, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// What [`cut`] removed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cut {
    /// Each removed record as one line of text, in log order. A line
    /// that does not read shows its position and why.
    pub records: Vec<String>,
    /// Each thread that a removed record names.
    pub threads: BTreeSet<String>,
    /// The name of each chunk that it deleted, or wrote again with only
    /// its first records.
    pub chunks: Vec<String>,
    /// The name of each checkpoint that it deleted.
    pub checkpoints: Vec<String>,
}

/// Deletes each record and each checkpoint after the position `after`.
/// It refuses a position before the oldest kept checkpoint. See the
/// module docs. Stop each server of the store before a cut.
///
/// In a chunk, it keeps only the first lines whose positions are right:
/// each is the position of the header plus its place, up to `after`. It
/// removes each line after them, also a line with a lower position. So
/// a record that it names as removed never stays. It removes a chunk
/// whose header does not read.
pub async fn cut(store: &dyn Store, after: u64) -> Result<Cut, ToolError> {
    let checkpoints = checkpoint::names(store).await?;
    if let Some((oldest, _, _)) = checkpoints.first()
        && after < *oldest
    {
        return Err(ToolError::Refused(format!(
            "cannot cut after position {after}: the oldest kept checkpoint is at position \
             {oldest}, and the chunks before it are gone. Cut at position {oldest} or later."
        )));
    }
    let chunks = log::chunks(store).await?;
    let from = chunk_with(&chunks, after.saturating_add(1));
    let mut removed = Cut::default();
    // From the end of the log to the cut, so that no gap stays when the
    // cut stops in the middle.
    for (first, name) in chunks[from..].iter().rev() {
        let Some(loaded) = store.load(name).await? else {
            continue;
        };
        let mut lines: Vec<&[u8]> = loaded.bytes.split(|byte| *byte == b'\n').collect();
        if lines.last().is_some_and(|line| line.is_empty()) {
            lines.pop();
        }
        let header = lines
            .first()
            .and_then(|line| serde_json::from_slice::<Header>(line).ok());
        // False from the first line that does not stay.
        let mut keeping = header.is_some_and(|h| h.first == *first && h.format <= log::FORMAT);
        // The number of bytes that stay: the header and each kept line.
        let mut stay = lines.first().map_or(0, |line| line.len() + 1);
        let mut kept = 0;
        let mut gone = Vec::new();
        for (expected, raw) in (*first..).zip(lines.iter().skip(1)) {
            let parsed = std::str::from_utf8(raw)
                .map_err(|e| e.to_string())
                .and_then(Line::parse);
            let right = matches!(&parsed, Ok(line) if position_of(line) == expected);
            if keeping && right && expected <= after {
                kept += 1;
                stay += raw.len() + 1;
                continue;
            }
            keeping = false;
            match &parsed {
                Ok(line) => {
                    if let Line::Record(record) = line {
                        removed.threads.extend(thread_of(record));
                    }
                    gone.push(show_line(line));
                }
                Err(why) => gone.push(format!("{expected}  (a line that does not read: {why})")),
            }
        }
        if gone.is_empty() && kept > 0 {
            continue;
        }
        if kept == 0 {
            store.delete(name).await?;
        } else {
            // The header and the kept lines, with their bytes unchanged.
            let bytes = loaded.bytes[..stay.min(loaded.bytes.len())].to_vec();
            store.save(name, bytes, Some(loaded.version)).await?;
        }
        removed.chunks.push(name.clone());
        gone.append(&mut removed.records);
        removed.records = gone;
    }
    removed.chunks.reverse();
    for (position, _, name) in &checkpoints {
        if *position > after {
            store.delete(name).await?;
            removed.checkpoints.push(name.clone());
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::Checkpoint;
    use crate::state::State;
    use crate::store::Memory;
    use riff_core::record::{Claimed, RiffStateSet};
    use riff_core::wire::RiffState;
    use std::time::Instant;

    fn running(position: u64) -> Record {
        Record {
            position,
            written_at_ms: 0,
            change: Change::RiffStateSet(RiffStateSet {
                state: RiffState::Running,
            }),
        }
    }

    fn claimed(position: u64, item: &str) -> Record {
        Record {
            position,
            written_at_ms: 0,
            change: Change::Claimed(Claimed {
                session: "riff://ann@heron/acme/app?session=s1".parse().unwrap(),
                thread: "acme/app".parse().unwrap(),
                item: item.into(),
            }),
        }
    }

    /// A store with the chunks 1-2, 3-4 and 5-6.
    async fn store() -> Memory {
        let store = Memory::default();
        for first in [1, 3, 5] {
            let records = [running(first), claimed(first + 1, "issue-7")];
            log::write(&store, &records, &log::Timing::default(), || true)
                .await
                .unwrap();
        }
        store
    }

    async fn checkpoint_at(store: &Memory, position: u64) {
        let records = (1..=position).map(running);
        let snapshot = State::replay(records, Instant::now(), 0).snapshot(Instant::now(), 0);
        let checkpoint = Checkpoint::new("0.8.0", position, snapshot);
        checkpoint::write(store, &checkpoint).await.unwrap();
    }

    #[tokio::test]
    async fn print_starts_in_the_chunk_that_holds_the_position() {
        let store = store().await;
        let mut lines = Vec::new();
        let printed = print(&store, 4, &mut |line| lines.push(line))
            .await
            .unwrap();
        assert_eq!(printed.records, 3);
        assert!(lines[0].starts_with("4  "), "{lines:?}");
        assert!(lines[0].contains("claimed  issue-7 in acme/app"));
    }

    #[tokio::test]
    async fn verify_names_a_bad_line_and_reads_the_lines_after_it() {
        let store = store().await;
        let name = log::chunk_name(3);
        let loaded = store.load(&name).await.unwrap().unwrap();
        let text = String::from_utf8(loaded.bytes).unwrap();
        let mut lines: Vec<&str> = text.lines().collect();
        lines[1] = "{not json";
        let bytes = (lines.join("\n") + "\n").into_bytes();
        store
            .save(&name, bytes, Some(loaded.version))
            .await
            .unwrap();

        let verified = verify(&store).await.unwrap();
        assert_eq!(verified.problems.len(), 1, "{:?}", verified.problems);
        let problem = &verified.problems[0];
        assert_eq!((problem.object.as_str(), problem.line), (&*name, Some(2)));
        assert_eq!(verified.good, Some(2));
        assert_eq!((verified.first, verified.last), (Some(1), Some(6)));
        assert_eq!(verified.records, 5);
    }

    #[tokio::test]
    async fn verify_names_a_gap_and_a_log_that_ends_before_a_checkpoint() {
        let store = store().await;
        store.delete(&log::chunk_name(3)).await.unwrap();
        checkpoint_at(&store, 9).await;
        // The oldest checkpoint is at 9, so the read starts at the last
        // chunk: it is before the checkpoint.
        let verified = verify(&store).await.unwrap();
        let whys: Vec<&str> = verified.problems.iter().map(|p| p.why.as_str()).collect();
        assert_eq!(
            whys,
            ["the log ends at position 6, before the newest checkpoint at 9"]
        );

        let store = self::store().await;
        store.delete(&log::chunk_name(3)).await.unwrap();
        let verified = verify(&store).await.unwrap();
        assert_eq!(
            verified.problems[0].why,
            "the chunk starts at position 5, and the log needs 3"
        );
        assert_eq!(verified.good, Some(2));
    }

    #[tokio::test]
    async fn verify_names_a_checkpoint_that_does_not_read() {
        let store = store().await;
        let name = checkpoint::name(2, 5);
        store.save(&name, b"{}".to_vec(), None).await.unwrap();
        let verified = verify(&store).await.unwrap();
        assert_eq!(verified.problems.len(), 1);
        assert_eq!(verified.problems[0].object, name);
        assert!(verified.problems[0].why.starts_with("does not read"));
        assert_eq!(verified.good, Some(6));
    }

    #[tokio::test]
    async fn cut_keeps_the_first_records_of_the_chunk_that_holds_the_position() {
        let store = store().await;
        checkpoint_at(&store, 2).await;
        checkpoint_at(&store, 4).await;
        let before = store.load(&log::chunk_name(3)).await.unwrap().unwrap();

        let removed = cut(&store, 3).await.unwrap();
        let positions: Vec<&str> = removed
            .records
            .iter()
            .map(|line| line.split_once("  ").unwrap().0)
            .collect();
        assert_eq!(positions, ["4", "5", "6"]);
        assert_eq!(removed.threads, BTreeSet::from(["acme/app".to_owned()]));
        assert_eq!(removed.chunks, [log::chunk_name(3), log::chunk_name(5)]);
        assert_eq!(removed.checkpoints, [checkpoint::name(4, 4)]);

        // The kept line has its bytes unchanged.
        let after = store.load(&log::chunk_name(3)).await.unwrap().unwrap();
        assert!(before.bytes.starts_with(&after.bytes));
        let replayed = log::replay(&store).await.unwrap();
        assert_eq!(replayed.last, 3);
        assert!(verify(&store).await.unwrap().problems.is_empty());
        // A second cut removes nothing.
        assert_eq!(cut(&store, 3).await.unwrap(), Cut::default());
    }

    #[tokio::test]
    async fn cut_refuses_a_position_before_the_oldest_kept_checkpoint() {
        let store = store().await;
        checkpoint_at(&store, 4).await;
        let error = cut(&store, 3).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("the oldest kept checkpoint is at position 4"),
            "{error}"
        );
        assert_eq!(log::replay(&store).await.unwrap().last, 6);
        // A cut at the checkpoint is good.
        assert_eq!(cut(&store, 4).await.unwrap().records.len(), 2);
    }

    /// A chunk with the records `positions` after its header, at
    /// `first`.
    async fn chunk(store: &Memory, first: u64, positions: &[u64]) {
        let mut text = format!("{{\"format\":1,\"first\":{first}}}\n");
        for position in positions {
            text.push_str(&serde_json::to_string(&running(*position)).unwrap());
            text.push('\n');
        }
        let name = log::chunk_name(first);
        store.delete(&name).await.unwrap();
        store.save(&name, text.into_bytes(), None).await.unwrap();
    }

    /// The positions that `cut` printed.
    fn printed(removed: &Cut) -> Vec<&str> {
        removed
            .records
            .iter()
            .map(|line| line.split_once("  ").unwrap().0)
            .collect()
    }

    #[tokio::test]
    async fn cut_removes_each_record_that_it_names_also_after_a_lower_position() {
        let store = store().await;
        chunk(&store, 5, &[5, 6, 3]).await;
        let verified = verify(&store).await.unwrap();
        assert_eq!(
            verified.problems[0].why,
            "a record has position 3, and the log needs 7"
        );
        assert_eq!((verified.good, verified.bad_log), (Some(6), true));

        let removed = cut(&store, 4).await.unwrap();
        assert_eq!(printed(&removed), ["5", "6", "3"]);
        // No record after the position stays.
        assert!(store.load(&log::chunk_name(5)).await.unwrap().is_none());
        let verified = verify(&store).await.unwrap();
        assert!(verified.problems.is_empty(), "{:?}", verified.problems);
        assert_eq!(verified.last, Some(4));
        assert_eq!(log::replay(&store).await.unwrap().last, 4);
    }

    #[tokio::test]
    async fn verify_names_the_cut_for_a_lower_position_at_the_end_and_the_cut_repairs_it() {
        let store = store().await;
        chunk(&store, 5, &[5, 2]).await;
        assert!(log::replay(&store).await.is_err());
        let verified = verify(&store).await.unwrap();
        assert_eq!(
            verified.problems[0].why,
            "a record has position 2, and the log needs 6"
        );
        // The bad line is the last line: the last good record is 5.
        assert_eq!((verified.good, verified.bad_log), (Some(5), true));
        assert!(verified_text(&verified).ends_with("riff-server log cut --after 5"));

        let removed = cut(&store, 5).await.unwrap();
        assert_eq!(printed(&removed), ["2"]);
        assert!(verify(&store).await.unwrap().problems.is_empty());
        assert_eq!(log::replay(&store).await.unwrap().last, 5);
    }

    #[tokio::test]
    async fn cut_removes_each_line_after_a_jump_and_a_chunk_with_a_bad_header() {
        let store = store().await;
        chunk(&store, 3, &[3, 9, 5]).await;
        let removed = cut(&store, 3).await.unwrap();
        assert_eq!(printed(&removed), ["9", "5", "5", "6"]);
        assert_eq!(log::replay(&store).await.unwrap().last, 3);

        // A header that does not read: verify names the position before
        // the chunk, and the cut removes the chunk.
        let name = log::chunk_name(3);
        store.delete(&name).await.unwrap();
        let bad = format!(
            "not a header\n{}\n",
            serde_json::to_string(&running(3)).unwrap()
        );
        store.save(&name, bad.into_bytes(), None).await.unwrap();
        let verified = verify(&store).await.unwrap();
        assert_eq!((verified.good, verified.bad_log), (Some(2), true));
        let removed = cut(&store, 2).await.unwrap();
        assert_eq!(printed(&removed), ["3"]);
        assert!(verify(&store).await.unwrap().problems.is_empty());
    }

    #[tokio::test]
    async fn cut_takes_the_largest_position() {
        let store = store().await;
        assert_eq!(cut(&store, u64::MAX).await.unwrap(), Cut::default());
    }

    #[tokio::test]
    async fn cut_removes_a_line_that_does_not_read() {
        let store = store().await;
        let name = log::chunk_name(5);
        let loaded = store.load(&name).await.unwrap().unwrap();
        let mut bytes = loaded.bytes.clone();
        bytes.extend(b"{not json\n");
        store
            .save(&name, bytes, Some(loaded.version))
            .await
            .unwrap();
        let verified = verify(&store).await.unwrap();
        assert_eq!(verified.good, Some(6));

        let removed = cut(&store, 6).await.unwrap();
        assert_eq!(removed.records.len(), 1);
        assert!(removed.records[0].starts_with("7  (a line that does not read"));
        assert!(verify(&store).await.unwrap().problems.is_empty());
    }
}
