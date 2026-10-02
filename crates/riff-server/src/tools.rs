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
//! | `riff-server log cut --after POSITION` | [`cut`] | Names each record and each checkpoint after the position. With `--yes`, it deletes them (01M3TJWHVN730ZWCWHT9ER186R). |
//!
//! To go back to a position, a person stops the server, runs `verify` to
//! find the first bad record, and cuts before it:
//!
//! ```mermaid
//! flowchart LR
//!     S[stop the server] --> V[log verify:<br/>the last good record is at P]
//!     V --> D[log cut --after P:<br/>a dry run, names each record]
//!     D --> C[log cut --after P --yes:<br/>removes them]
//!     C --> R[start the server:<br/>it replays up to P]
//! ```
//!
//! - `verify` does not stop at the first problem. It reads each line by
//!   its bytes, so one run names each bad line, each gap and each repeat
//!   of a position. A line that is not UTF-8 is one bad line.
//! - `verify` and `cut` read the log with the same walk. The good part
//!   of the log is its first records that read and have the right
//!   positions. `cut` keeps only that part, up to the position: it
//!   removes each line after the first problem, also in a later chunk
//!   (01M3X342NQWZBJPS0GXV98BQME). So the cut that `verify` names
//!   repairs the log, and removes no record before the problem.
//! - `cut` removes nothing without `--yes` ([`Mode::DryRun`],
//!   01M3X342G8KF2W06PABGXTERMZ). It names what a cut removes.
//! - `cut --yes` refuses while a server holds the lease
//!   (01M3X342K007K3Z9G0CYWFKVMA). See "A live lease" in
//!   [`crate::lease`].
//! - `cut` refuses a position before the oldest kept checkpoint: the
//!   chunks before that checkpoint are gone, so no start can replay
//!   them. It refuses also when the first problem is before that
//!   checkpoint: no cut repairs such a log.
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
//! use riff_server::tools::{Mode, cut, print, verify};
//!
//! let store = Memory::default();
//! let record = |position| Record {
//!     position,
//!     written_at_ms: 1_790_000_000_000,
//!     by: None,
//!     command: None,
//!     change: Change::RiffStateSet(RiffStateSet { state: RiffState::Running }),
//! };
//! write(&store, &[record(1), record(2), record(3)], &Timing::default(), || true).await?;
//!
//! let mut lines = Vec::new();
//! print(&store, 2, &mut |line| lines.push(line)).await?;
//! assert_eq!(lines[0], "2  2026-09-21T14:13:20Z  riff_state_set  running  (cause not known)");
//! assert_eq!(lines.len(), 2);
//!
//! assert!(verify(&store).await?.problems.is_empty());
//!
//! // A dry run names the records, and removes nothing.
//! let named = cut(&store, 1, Mode::DryRun).await?;
//! assert_eq!(named.records.len(), 2);
//! assert_eq!(verify(&store).await?.last, Some(3));
//!
//! let removed = cut(&store, 1, Mode::Remove).await?;
//! assert_eq!(removed, named);
//! assert_eq!(verify(&store).await?.last, Some(1));
//! # Ok(()) }
//! ```

use std::collections::BTreeSet;
use std::fmt;

use riff_core::record::{Change, Line, Record};
use riff_core::wire::Kind;

use crate::checkpoint;
use crate::lease::{self, Held};
use crate::log::{self, Header};
use crate::store::{Loaded, Store, StoreError};

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
/// the change, its facts, and its cause: the kind of the command and
/// the caller (01M3X4Z60G1FXQTDC5XDJ05BAX). A message shows its thread, its number, its
/// sender, its kind and the start of its body.
///
/// ```
/// use riff_core::name::Who;
/// use riff_core::record::{By, Change, Claimed, Record};
/// use riff_server::tools::show;
///
/// let mut record = Record {
///     position: 1234,
///     written_at_ms: 1_790_000_000_000,
///     by: Some(By::Session(Who::new("ann", Some("s1"))?)),
///     command: Some("claim".into()),
///     change: Change::Claimed(Claimed {
///         session: "riff://ann@heron/acme/app?session=s1".parse()?,
///         thread: "acme/app".parse()?,
///         item: "issue-7".into(),
///     }),
/// };
/// assert_eq!(
///     show(&record),
///     "1234  2026-09-21T14:13:20Z  claimed  issue-7 in acme/app by riff://ann@heron/acme/app?session=s1  (claim, the session ann/s1)"
/// );
/// // A record from before the cause.
/// (record.by, record.command) = (None, None);
/// assert!(show(&record).ends_with("  (cause not known)"));
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
        "{}  {}  {facts}  ({})",
        record.position,
        utc(record.written_at_ms),
        cause(record)
    )
}

/// The cause of a record as text: the kind of its command and its
/// caller (01M3X4Z60G1FXQTDC5XDJ05BAX). A record from before the cause has none.
fn cause(record: &Record) -> String {
    match (&record.command, &record.by) {
        (Some(command), Some(by)) => format!("{command}, {by}"),
        (Some(command), None) => command.clone(),
        (None, Some(by)) => by.to_string(),
        (None, None) => "cause not known".to_owned(),
    }
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

/// One line of a chunk after its header.
struct Row {
    /// The position that the log needs at this line.
    expected: u64,
    /// The number of bytes of the chunk up to the end of this line.
    end: usize,
    /// The line, or why it does not read.
    line: Result<Line, String>,
}

impl Row {
    /// True when the line is the record that the log needs here.
    fn right(&self) -> bool {
        matches!(&self.line, Ok(line) if position_of(line) == self.expected)
    }

    /// Why the line is not the record that the log needs here.
    fn problem(&self) -> Option<String> {
        match &self.line {
            Ok(line) if position_of(line) == self.expected => None,
            Ok(line) => Some(format!(
                "a record has position {}, and the log needs {}",
                position_of(line),
                self.expected
            )),
            Err(why) => Some(why.clone()),
        }
    }

    /// The line as text, for a cut that removes it. `last` is the
    /// position of the last record that stays.
    fn gone(&self, last: u64) -> String {
        match &self.line {
            Ok(line) if position_of(line) <= last => format!(
                "{}  (a repeat: the record at position {} stays)",
                show_line(line),
                position_of(line)
            ),
            Ok(line) => show_line(line),
            Err(why) => format!("{}  (a line that does not read: {why})", self.expected),
        }
    }
}

/// One chunk as [`verify`] and [`cut`] read it.
struct Chunk {
    /// The first position, from the name.
    first: u64,
    name: String,
    /// `None` when the chunk is gone.
    loaded: Option<Loaded>,
    /// Each problem of the header and of the start of the chunk: its
    /// line, and why.
    head: Vec<(Option<usize>, String)>,
    rows: Vec<Row>,
}

/// Each line of `bytes`, with the number of bytes up to its end. It
/// splits bytes, so a line that is not UTF-8 is one line.
fn lines(bytes: &[u8]) -> Vec<(usize, &[u8])> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (at, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            lines.push((at + 1, &bytes[start..at]));
            start = at + 1;
        }
    }
    if start < bytes.len() {
        lines.push((bytes.len(), &bytes[start..]));
    }
    lines
}

/// The chunks of the log from the oldest kept checkpoint, as [`verify`]
/// and [`cut`] read them. Both tools use this one walk, so the cut that
/// `verify` names keeps what `verify` read as good
/// (01M3X342NQWZBJPS0GXV98BQME).
struct Walk {
    /// The position after the oldest kept checkpoint, or 1.
    start: u64,
    chunks: Vec<Chunk>,
}

impl Walk {
    /// Reads each chunk from the chunk that holds the position after
    /// `oldest`, the oldest kept checkpoint. It reads each line by its
    /// bytes.
    async fn read(store: &dyn Store, oldest: Option<u64>) -> Result<Walk, ToolError> {
        let names = log::chunks(store).await?;
        let start = oldest.map_or(1, |oldest| oldest + 1);
        let from = chunk_with(&names, start);
        // The position that the next record must have.
        let mut next: Option<u64> = None;
        let mut chunks = Vec::new();
        for (first, name) in &names[from..] {
            let loaded = store.load(name).await?;
            let mut head = Vec::new();
            let mut rows = Vec::new();
            if let Some(loaded) = &loaded {
                let mut lines = lines(&loaded.bytes).into_iter();
                let header = lines.next();
                match header.map(|(_, line)| serde_json::from_slice::<Header>(line)) {
                    Some(Ok(header)) => {
                        if header.format > log::FORMAT {
                            let why = format!("the chunk has the later format {}", header.format);
                            head.push((Some(1), why));
                        }
                        if header.first != *first {
                            let why = format!(
                                "the header says position {}, and the name says {first}",
                                header.first
                            );
                            head.push((Some(1), why));
                        }
                    }
                    Some(Err(error)) => {
                        head.push((Some(1), format!("the header does not read: {error}")));
                    }
                    None => head.push((None, "the chunk is empty".into())),
                }
                let begins = match (next, oldest) {
                    (Some(next), _) if next != *first => Some(format!(
                        "the chunk starts at position {first}, and the log needs {next}"
                    )),
                    (None, Some(oldest)) if *first > oldest + 1 => Some(format!(
                        "the log starts at position {first}, after the oldest checkpoint at {oldest}"
                    )),
                    (None, None) if *first != 1 => Some(format!(
                        "the log starts at position {first}, and the store has no checkpoint"
                    )),
                    _ => None,
                };
                head.extend(begins.map(|why| (Some(1), why)));
                for (expected, (end, raw)) in (*first..).zip(lines) {
                    let line = std::str::from_utf8(raw)
                        .map_err(|error| format!("the line is not UTF-8: {error}"))
                        .and_then(Line::parse);
                    rows.push(Row {
                        expected,
                        end,
                        line,
                    });
                }
            } else {
                head.push((None, "the chunk is gone".into()));
            }
            next = Some(first + rows.len() as u64);
            chunks.push(Chunk {
                first: *first,
                name: name.clone(),
                loaded,
                head,
                rows,
            });
        }
        Ok(Walk { start, chunks })
    }

    /// What stays in a cut after the position `after`: the first records
    /// of the log that read and have the right positions, up to `after`.
    /// Each line after the first problem goes, also in a later chunk.
    fn keep(&self, after: u64) -> Kept {
        // The last position before the first chunk of the walk.
        let mut last = match self.chunks.first() {
            Some(chunk) if chunk.first <= self.start => chunk.first.saturating_sub(1),
            _ => self.start - 1,
        };
        let mut keeping = true;
        let mut rows = Vec::new();
        for chunk in &self.chunks {
            keeping &= chunk.head.is_empty();
            let mut stay = 0;
            for row in &chunk.rows {
                keeping &= row.right() && row.expected <= after;
                if !keeping {
                    break;
                }
                stay += 1;
                last = row.expected;
            }
            rows.push(stay);
        }
        Kept { last, rows }
    }
}

/// What stays of the chunks of a [`Walk`].
struct Kept {
    /// The position of the last record that stays.
    last: u64,
    /// For each chunk, the number of its first lines that stay.
    rows: Vec<usize>,
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
    /// log. A cut after it removes each bad record, and no record before
    /// the problem. With no good record, it is the position before the
    /// first record that the log needs.
    pub good: Option<u64>,
    /// True when a chunk has a problem. A cut after [`Verified::good`]
    /// then repairs the log.
    pub bad_log: bool,
    /// The position of the oldest kept checkpoint. A cut before it is
    /// refused.
    pub oldest: Option<u64>,
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
    verified.oldest = checkpoints.first().map(|(position, _, _)| *position);
    let newest = checkpoints.last().map(|(position, _, _)| *position);
    let walk = Walk::read(store, verified.oldest).await?;
    for chunk in &walk.chunks {
        verified.chunks += 1;
        // Each problem of this chunk: its line, and why.
        let mut bad = chunk.head.clone();
        for (n, row) in chunk.rows.iter().enumerate() {
            match row.problem() {
                Some(why) => bad.push((Some(n + 2), why)),
                None => {
                    verified.records += 1;
                    verified.first.get_or_insert(row.expected);
                    verified.last = Some(row.expected);
                }
            }
        }
        verified.bad_log |= !bad.is_empty();
        verified
            .problems
            .extend(bad.into_iter().map(|(line, why)| Problem {
                object: store.locate(&chunk.name),
                line,
                why,
            }));
    }
    if let Some(chunk) = walk.chunks.last() {
        verified.good = Some(walk.keep(u64::MAX).last);
        let end = (chunk.first + chunk.rows.len() as u64).saturating_sub(1);
        if let Some(newest) = newest
            && end < newest
        {
            verified.problems.push(Problem {
                object: store.locate(&chunk.name),
                line: None,
                why: format!(
                    "the log ends at position {end}, before the newest checkpoint at {newest}"
                ),
            });
        }
    }
    Ok(verified)
}

/// The last lines of `riff-server log verify`: what it read, and for a
/// bad log the last good position and the cut that removes each record
/// after it. That command is a dry run: it prints the records, and the
/// command that removes them.
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
///     oldest: Some(1),
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
///      position 2.\nTo see each record after it, and the command that removes them, run: \
///      riff-server log cut --after 2"
/// );
/// // A problem before the oldest kept checkpoint: no cut repairs the log.
/// verified.good = Some(0);
/// assert_eq!(
///     verified_text(&verified),
///     "1 problem in 3 chunks and 1 checkpoint. The first problem of the log is before the \
///      oldest kept checkpoint at position 1, so no cut repairs the log. Get back an older \
///      version of the chunk."
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
        match verified.oldest {
            Some(oldest) if good < oldest => text.push_str(&format!(
                " The first problem of the log is before the oldest kept checkpoint at \
                 position {oldest}, so no cut repairs the log. Get back an older version of \
                 the chunk."
            )),
            _ => text.push_str(&format!(
                " The last good record of the log is at position {good}.\n\
                 To see each record after it, and the command that removes them, run: \
                 riff-server log cut --after {good}"
            )),
        }
    }
    text
}

/// A live lease as text, for a cut.
fn held_text(held: &Held) -> String {
    format!(
        "the server instance {} holds the lease, and wrote it at {}",
        held.id,
        utc(held.renewed_at_ms)
    )
}

/// The last lines of `riff-server log cut`: how much it removed, and the
/// threads of the removed records. `after` is the position that the
/// person named. A dry run also gives the command that removes.
///
/// ```
/// use riff_server::tools::{Cut, Mode, cut_text};
///
/// let mut removed = Cut { last: 9, ..Cut::default() };
/// assert_eq!(
///     cut_text(&removed, 9, Mode::Remove),
///     "Nothing is after position 9: removed nothing."
/// );
/// removed.records = vec!["10  …".into(), "11  …".into()];
/// removed.threads.insert("acme/app".into());
/// removed.checkpoints.push("checkpoint/x".into());
/// assert_eq!(
///     cut_text(&removed, 9, Mode::Remove),
///     "Removed 2 records and 1 checkpoint after position 9. Threads: acme/app."
/// );
/// assert_eq!(
///     cut_text(&removed, 9, Mode::DryRun),
///     "A cut removes 2 records and 1 checkpoint after position 9. Threads: acme/app.\n\
///      This run removed nothing. To remove them, stop the server and run: \
///      riff-server log cut --after 9 --yes"
/// );
/// ```
pub fn cut_text(removed: &Cut, after: u64, mode: Mode) -> String {
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
    let what = format!(
        "{} and {} after position {}. Threads: {threads}.",
        count(removed.records.len() as u64, "record"),
        count(removed.checkpoints.len() as u64, "checkpoint"),
        removed.last
    );
    match mode {
        Mode::Remove => format!("Removed {what}"),
        Mode::DryRun => {
            let held = removed.held.as_ref().map_or_else(String::new, |held| {
                format!("\nNow {}: `--yes` refuses.", held_text(held))
            });
            format!(
                "A cut removes {what}\nThis run removed nothing. To remove them, stop the \
                 server and run: riff-server log cut --after {after} --yes{held}"
            )
        }
    }
}

/// A number with its noun: `1 chunk`, `3 chunks`.
fn count(n: u64, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// What [`cut`] does with the records that it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// It removes nothing (01M3X342G8KF2W06PABGXTERMZ).
    DryRun,
    /// It removes them: `--yes`.
    Remove,
}

/// What [`cut`] removed, or what it removes with [`Mode::Remove`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cut {
    /// Each removed line as one line of text, in log order. A line that
    /// does not read shows its position and why. A line whose position
    /// stays in the log says so.
    pub records: Vec<String>,
    /// Each thread that a removed record names.
    pub threads: BTreeSet<String>,
    /// The name of each chunk that it deleted, or wrote again with only
    /// its first records.
    pub chunks: Vec<String>,
    /// The name of each checkpoint that it deleted.
    pub checkpoints: Vec<String>,
    /// The position of the last record that stays: the position of the
    /// cut, or the last good record of the log before it.
    pub last: u64,
    /// The instance that holds the lease, in a dry run.
    pub held: Option<Held>,
}

/// Names each record and each checkpoint after the position `after`,
/// and deletes them with [`Mode::Remove`]. See the module docs.
///
/// - It refuses a position before the oldest kept checkpoint.
/// - With [`Mode::Remove`], it refuses while an instance holds the
///   lease, and names the instance (01M3X342K007K3Z9G0CYWFKVMA). See
///   [`crate::lease::holder`].
/// - It keeps only the first records of the log that read and have the
///   right positions, up to `after`. It removes each line after them,
///   also a line with a lower position, and each later chunk. So the
///   log reads after the cut, and a line that it names as removed never
///   stays (01M3X342NQWZBJPS0GXV98BQME).
pub async fn cut(store: &dyn Store, after: u64, mode: Mode) -> Result<Cut, ToolError> {
    let checkpoints = checkpoint::names(store).await?;
    let oldest = checkpoints.first().map(|(position, _, _)| *position);
    if let Some(oldest) = oldest
        && after < oldest
    {
        return Err(ToolError::Refused(format!(
            "cannot cut after position {after}: the oldest kept checkpoint is at position \
             {oldest}, and the chunks before it are gone. Cut at position {oldest} or later."
        )));
    }
    let timing = lease::Timing::default();
    let held = lease::holder(store, lease::now_ms(), &timing).await?;
    if let (Mode::Remove, Some(held)) = (mode, &held) {
        return Err(ToolError::Refused(format!(
            "cannot cut: {}. Stop the server first. A lease ends when its server shuts \
             down, or {} seconds after its last write.",
            held_text(held),
            timing.ends_after.as_secs()
        )));
    }
    let walk = Walk::read(store, oldest).await?;
    let kept = walk.keep(after);
    if let Some(oldest) = oldest
        && kept.last < oldest
    {
        return Err(ToolError::Refused(format!(
            "cannot cut: the log does not read at position {}, before the oldest kept \
             checkpoint at position {oldest}. No cut repairs the log. To see each problem, \
             run: riff-server log verify",
            kept.last + 1
        )));
    }
    let mut removed = Cut {
        last: kept.last,
        held,
        ..Cut::default()
    };
    // From the end of the log to the cut, so that no gap stays when the
    // cut stops in the middle.
    for (chunk, stay) in walk.chunks.iter().zip(&kept.rows).rev() {
        let Some(loaded) = &chunk.loaded else {
            continue;
        };
        let rows = &chunk.rows[*stay..];
        if rows.is_empty() && *stay > 0 {
            continue;
        }
        if mode == Mode::Remove {
            match stay.checked_sub(1) {
                None => store.delete(&chunk.name).await?,
                Some(end) => {
                    // The header and the kept lines, with their bytes
                    // unchanged.
                    let bytes = loaded.bytes[..chunk.rows[end].end].to_vec();
                    store.save(&chunk.name, bytes, Some(loaded.version)).await?;
                }
            }
        }
        removed.chunks.push(chunk.name.clone());
        for row in rows {
            if let Ok(Line::Record(record)) = &row.line {
                removed.threads.extend(thread_of(record));
            }
        }
        let mut gone: Vec<String> = rows.iter().map(|row| row.gone(kept.last)).collect();
        gone.append(&mut removed.records);
        removed.records = gone;
    }
    removed.chunks.reverse();
    for (position, _, name) in &checkpoints {
        if *position > kept.last {
            if mode == Mode::Remove {
                store.delete(name).await?;
            }
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
            by: None,
            command: None,
            change: Change::RiffStateSet(RiffStateSet {
                state: RiffState::Running,
            }),
        }
    }

    fn claimed(position: u64, item: &str) -> Record {
        Record {
            position,
            written_at_ms: 0,
            by: None,
            command: None,
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

        let removed = cut(&store, 3, Mode::Remove).await.unwrap();
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
        let nothing = Cut {
            last: 3,
            ..Cut::default()
        };
        assert_eq!(cut(&store, 3, Mode::Remove).await.unwrap(), nothing);
    }

    /// Each object of the store, with its bytes.
    async fn objects(store: &Memory) -> Vec<(String, Vec<u8>)> {
        let mut objects = Vec::new();
        for name in store.list("").await.unwrap() {
            let bytes = store.load(&name).await.unwrap().unwrap().bytes;
            objects.push((name, bytes));
        }
        objects
    }

    /// 01M3X342G8KF2W06PABGXTERMZ: a dry run names what the cut removes,
    /// and changes no object.
    #[tokio::test]
    async fn a_dry_run_names_each_record_and_each_checkpoint_and_removes_nothing() {
        let store = store().await;
        checkpoint_at(&store, 2).await;
        checkpoint_at(&store, 4).await;
        let before = objects(&store).await;

        let named = cut(&store, 3, Mode::DryRun).await.unwrap();
        assert_eq!(printed(&named), ["4", "5", "6"]);
        assert_eq!(named.checkpoints, [checkpoint::name(4, 4)]);
        assert_eq!(objects(&store).await, before, "a dry run changes nothing");
        let text = cut_text(&named, 3, Mode::DryRun);
        assert!(
            text.starts_with("A cut removes 3 records and 1 checkpoint after position 3."),
            "{text}"
        );
        assert!(
            text.ends_with("riff-server log cut --after 3 --yes"),
            "{text}"
        );

        // The cut removes what the dry run named.
        assert_eq!(cut(&store, 3, Mode::Remove).await.unwrap(), named);
        assert_ne!(objects(&store).await, before);
    }

    /// The lease of the instance `id`, with a time `age_ms` before now.
    async fn lease_of(store: &Memory, id: &str, age_ms: u64, ended: bool) {
        let at = lease::now_ms() - age_ms;
        let json = format!(r#"{{"id":"{id}","renewed_at_ms":{at},"ended":{ended}}}"#);
        store.delete(crate::store::LEASE).await.unwrap();
        let saved = store.save(crate::store::LEASE, json.into_bytes(), None);
        saved.await.unwrap();
    }

    /// 01M3X342K007K3Z9G0CYWFKVMA: a cut refuses while an instance holds
    /// the lease, and names the instance.
    #[tokio::test]
    async fn cut_refuses_a_live_lease_and_cuts_after_a_lease_that_ended() {
        let store = store().await;
        lease_of(&store, "abc123", 1_000, false).await;
        let before = objects(&store).await;
        let error = cut(&store, 4, Mode::Remove).await.unwrap_err().to_string();
        assert!(
            error.starts_with("cannot cut: the server instance abc123 holds the lease"),
            "{error}"
        );
        assert!(error.contains("Stop the server first."), "{error}");
        assert_eq!(objects(&store).await, before, "a refusal changes nothing");

        // A dry run names the records and the instance.
        let named = cut(&store, 4, Mode::DryRun).await.unwrap();
        assert_eq!(printed(&named), ["5", "6"]);
        assert_eq!(named.held.as_ref().unwrap().id, "abc123");
        let text = cut_text(&named, 4, Mode::DryRun);
        assert!(
            text.contains("the server instance abc123 holds the lease"),
            "{text}"
        );

        // The lease ended: 90 seconds with no new time.
        lease_of(&store, "abc123", 90_000, false).await;
        assert_eq!(cut(&store, 5, Mode::Remove).await.unwrap().records.len(), 1);
        // The lease ended: the instance shut down.
        lease_of(&store, "abc123", 0, true).await;
        assert_eq!(cut(&store, 4, Mode::Remove).await.unwrap().records.len(), 1);
        assert_eq!(log::replay(&store).await.unwrap().last, 4);
    }

    /// 01M3X342NQWZBJPS0GXV98BQME: a chunk that starts before the end of
    /// the chunk before it. The cut that `verify` names removes that
    /// chunk, and keeps each record of the chunk before it.
    #[tokio::test]
    async fn the_cut_that_verify_names_repairs_a_chunk_that_starts_too_early() {
        let store = Memory::default();
        chunk(&store, 1, &[1, 2, 3, 4]).await;
        chunk(&store, 3, &[3, 4, 5]).await;
        assert!(log::replay(&store).await.is_err());
        let verified = verify(&store).await.unwrap();
        assert_eq!(
            verified.problems[0].why,
            "the chunk starts at position 3, and the log needs 5"
        );
        assert_eq!((verified.good, verified.bad_log), (Some(4), true));

        let removed = cut(&store, 4, Mode::Remove).await.unwrap();
        assert_eq!(printed(&removed), ["3", "4", "5"]);
        // A removed line whose position stays in the log says so.
        assert!(
            removed.records[0].ends_with("(a repeat: the record at position 3 stays)"),
            "{:?}",
            removed.records
        );
        assert!(!removed.records[2].contains("a repeat"));
        assert_eq!(removed.chunks, [log::chunk_name(3)]);
        assert_eq!(removed.last, 4);
        let verified = verify(&store).await.unwrap();
        assert!(verified.problems.is_empty(), "{:?}", verified.problems);
        assert_eq!((verified.first, verified.last), (Some(1), Some(4)));
        assert_eq!(log::replay(&store).await.unwrap().last, 4);
    }

    /// A cut before the chunk that starts too early removes the records
    /// of both chunks, and names each line that it removes.
    #[tokio::test]
    async fn a_cut_before_a_chunk_that_starts_too_early_names_each_removed_line() {
        let store = Memory::default();
        chunk(&store, 1, &[1, 2, 3, 4]).await;
        chunk(&store, 3, &[3, 4, 5]).await;
        let removed = cut(&store, 2, Mode::Remove).await.unwrap();
        assert_eq!(printed(&removed), ["3", "4", "3", "4", "5"]);
        assert!(
            removed
                .records
                .iter()
                .all(|line| !line.contains("a repeat"))
        );
        assert_eq!(
            removed.chunks,
            [log::chunk_name(1), log::chunk_name(3)],
            "the cut wrote the first chunk again, and deleted the second"
        );
        let verified = verify(&store).await.unwrap();
        assert!(verified.problems.is_empty(), "{:?}", verified.problems);
        assert_eq!(verified.last, Some(2));
    }

    /// The chunk `first` of `store`, with the bytes `bad` and a newline
    /// in the place of its line `line`. The header is line 1.
    async fn replace_line(store: &Memory, first: u64, line: usize, bad: &[u8]) {
        let name = log::chunk_name(first);
        let loaded = store.load(&name).await.unwrap().unwrap();
        let mut lines: Vec<&[u8]> = loaded.bytes.split(|byte| *byte == b'\n').collect();
        lines.pop();
        lines[line - 1] = bad;
        let mut bytes = lines.join(&b'\n');
        bytes.push(b'\n');
        let saved = store.save(&name, bytes, Some(loaded.version));
        saved.await.unwrap();
    }

    /// 01M3X342NQWZBJPS0GXV98BQME: a line that is not UTF-8 is one bad
    /// line. The cut that `verify` names keeps each record before it.
    #[tokio::test]
    async fn the_cut_that_verify_names_repairs_a_line_that_is_not_utf8() {
        let store = store().await;
        replace_line(&store, 3, 3, b"\xff\xfe not text").await;
        let before = store.load(&log::chunk_name(3)).await.unwrap().unwrap();
        assert!(log::replay(&store).await.is_err());

        let verified = verify(&store).await.unwrap();
        assert_eq!(verified.problems.len(), 1, "{:?}", verified.problems);
        let problem = &verified.problems[0];
        assert_eq!(problem.line, Some(3));
        assert!(
            problem.why.starts_with("the line is not UTF-8"),
            "{problem}"
        );
        // The record at position 3, before the bad line, is good.
        assert_eq!((verified.good, verified.bad_log), (Some(3), true));
        assert_eq!(verified.records, 5);

        let removed = cut(&store, 3, Mode::Remove).await.unwrap();
        assert_eq!(printed(&removed), ["4", "5", "6"]);
        assert!(
            removed.records[0].starts_with("4  (a line that does not read: the line is not UTF-8"),
            "{:?}",
            removed.records
        );
        // The kept lines have their bytes unchanged.
        let after = store.load(&log::chunk_name(3)).await.unwrap().unwrap();
        assert!(before.bytes.starts_with(&after.bytes));
        assert!(verify(&store).await.unwrap().problems.is_empty());
        assert_eq!(log::replay(&store).await.unwrap().last, 3);
    }

    /// A log of one chunk with a line that is not UTF-8: the cut that
    /// `verify` names is not the whole log.
    #[tokio::test]
    async fn a_bad_line_in_the_only_chunk_does_not_cut_the_whole_log() {
        let store = Memory::default();
        chunk(&store, 1, &[1, 2, 3, 4]).await;
        replace_line(&store, 1, 4, b"\xc3\x28").await;
        let verified = verify(&store).await.unwrap();
        assert_eq!(verified.good, Some(2));
        let removed = cut(&store, 2, Mode::Remove).await.unwrap();
        assert_eq!(printed(&removed), ["3", "4"]);
        assert_eq!(log::replay(&store).await.unwrap().last, 2);
    }

    /// A cut after a position that comes after a problem keeps only the
    /// good part of the log, and says where it ends.
    #[tokio::test]
    async fn a_cut_after_a_problem_removes_each_line_from_the_problem() {
        let store = store().await;
        replace_line(&store, 3, 3, b"{not json").await;
        let removed = cut(&store, 5, Mode::Remove).await.unwrap();
        assert_eq!(printed(&removed), ["4", "5", "6"]);
        assert_eq!(removed.last, 3);
        assert_eq!(
            cut_text(&removed, 5, Mode::Remove),
            "Removed 3 records and 0 checkpoints after position 3. Threads: acme/app."
        );
        assert!(verify(&store).await.unwrap().problems.is_empty());
    }

    /// A problem before the oldest kept checkpoint: `verify` names no
    /// cut, and a cut refuses and changes nothing.
    #[tokio::test]
    async fn no_cut_repairs_a_problem_before_the_oldest_kept_checkpoint() {
        let store = store().await;
        // The chunk 3 holds the positions 3 and 4: the checkpoint needs
        // the chunk for the position 4.
        checkpoint_at(&store, 3).await;
        replace_line(&store, 3, 2, b"{not json").await;
        let before = objects(&store).await;
        let verified = verify(&store).await.unwrap();
        assert_eq!((verified.good, verified.oldest), (Some(2), Some(3)));
        let text = verified_text(&verified);
        assert!(text.contains("no cut repairs the log"), "{text}");
        assert!(!text.contains("log cut"), "{text}");

        let error = cut(&store, 4, Mode::Remove).await.unwrap_err().to_string();
        assert!(
            error.starts_with("cannot cut: the log does not read at position 3"),
            "{error}"
        );
        assert_eq!(objects(&store).await, before);
    }

    #[tokio::test]
    async fn cut_refuses_a_position_before_the_oldest_kept_checkpoint() {
        let store = store().await;
        checkpoint_at(&store, 4).await;
        let error = cut(&store, 3, Mode::Remove).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("the oldest kept checkpoint is at position 4"),
            "{error}"
        );
        assert_eq!(log::replay(&store).await.unwrap().last, 6);
        // A cut at the checkpoint is good.
        assert_eq!(cut(&store, 4, Mode::Remove).await.unwrap().records.len(), 2);
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

        let removed = cut(&store, 4, Mode::Remove).await.unwrap();
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

        let removed = cut(&store, 5, Mode::Remove).await.unwrap();
        assert_eq!(printed(&removed), ["2"]);
        assert!(verify(&store).await.unwrap().problems.is_empty());
        assert_eq!(log::replay(&store).await.unwrap().last, 5);
    }

    #[tokio::test]
    async fn cut_removes_each_line_after_a_jump_and_a_chunk_with_a_bad_header() {
        let store = store().await;
        chunk(&store, 3, &[3, 9, 5]).await;
        let removed = cut(&store, 3, Mode::Remove).await.unwrap();
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
        let removed = cut(&store, 2, Mode::Remove).await.unwrap();
        assert_eq!(printed(&removed), ["3"]);
        assert!(verify(&store).await.unwrap().problems.is_empty());
    }

    #[tokio::test]
    async fn cut_takes_the_largest_position() {
        let store = store().await;
        let nothing = Cut {
            last: 6,
            ..Cut::default()
        };
        let removed = cut(&store, u64::MAX, Mode::Remove).await.unwrap();
        assert_eq!(removed, nothing);
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

        let removed = cut(&store, 6, Mode::Remove).await.unwrap();
        assert_eq!(removed.records.len(), 1);
        assert!(removed.records[0].starts_with("7  (a line that does not read"));
        assert!(verify(&store).await.unwrap().problems.is_empty());
    }
}
