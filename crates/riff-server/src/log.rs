//! The log of `riff-server`: chunks in a [`Store`].
//!
//! # Chunks (01M3T4118SDERYGJ25TAT1RGR2)
//!
//! The writer takes each record in the queue into one chunk, and writes
//! it as one new object: a group commit. It writes at most one chunk at
//! a time. While it writes, new records wait in the queue for the next
//! chunk. See [`crate::state`] for the queue and the two copies of the
//! state.
//!
//! - The name of a chunk is its first position, with zeros in front:
//!   `log/00000000000000001234.jsonl` ([`chunk_name`]). So the names sort
//!   in log order.
//! - The first line of a chunk is a [`Header`] with the format version and
//!   the first position. Then each record is one line of JSON (see
//!   [`riff_core::record`]).
//! - Each write sends "only when new" (`ifGenerationMatch=0` in Cloud
//!   Storage), so it never replaces an object.
//!
//! # A failed write (01M3T411BZQB8N4D2S0JFVESMS)
//!
//! ```mermaid
//! flowchart TD
//!     W[write the chunk] -->|done| D[apply to the written state, reply, wake]
//!     W -->|another object has the name| S[stop for good]
//!     W -->|other error| R{10 s since the first try?}
//!     R -->|no| B[wait, then write again] --> W2[write again]
//!     R -->|yes| S
//!     W2 -->|done| D
//!     W2 -->|another object has the name| C{its bytes are the same?}
//!     C -->|yes| D
//!     C -->|no| S
//!     W2 -->|other error| R
//! ```
//!
//! - On the first try, an object with the same name means that another
//!   instance writes the log. The instance stops for good (R141).
//! - Each other error, for example a 429, a 5xx, a timeout or a failed
//!   token, is tried again with a backoff for [`Timing::retry_for`]. A
//!   try that takes more than [`Timing::attempt`] counts as a timeout.
//! - When a later try finds an object with the same name, an earlier try
//!   can have written it. When its bytes are the same, the write is
//!   done.
//! - After [`Timing::retry_for`], the instance stops for good. The next
//!   instance replays without the chunk.
//!
//! # The replay (01M3T411F3K6FD28R3Q3ZE4VCN)
//!
//! [`replay`] reads each chunk in name order, and checks the positions:
//! each record is the last position plus 1. A gap or a repeat, a line
//! that does not read, or a header of a later format stops the load, and
//! the error names the chunk. A record of a kind that this build does not
//! know is skipped, with a warning. It keeps its position: the next
//! record comes after it ([`Replayed::last`]). [`Replayed::skipped`] has
//! the position of the first skipped record: the server writes no
//! checkpoint past it (see [`crate::checkpoint`]).
//!
//! A start from a checkpoint reads only the records after its position
//! ([`replay_after`]): from the chunk that holds the next position.
//!
//! # Example
//!
//! ```
//! # #[tokio::main] async fn main() -> Result<(), riff_server::store::StoreError> {
//! use riff_core::record::{Change, Record, RiffStateSet};
//! use riff_core::wire::RiffState;
//! use riff_server::log::{Timing, replay, write};
//! use riff_server::store::Memory;
//!
//! let store = Memory::default();
//! let record = |position| Record {
//!     position,
//!     written_at_ms: 0,
//!     by: None,
//!     command: None,
//!     change: Change::RiffStateSet(RiffStateSet { state: RiffState::Running }),
//! };
//! let serving = || true;
//! write(&store, &[record(1), record(2)], &Timing::default(), serving).await?;
//! write(&store, &[record(3)], &Timing::default(), serving).await?;
//! let replayed = replay(&store).await?;
//! let positions: Vec<u64> = replayed.records.iter().map(|r| r.position).collect();
//! assert_eq!(positions, [1, 2, 3]);
//! assert_eq!(replayed.last, 3);
//! # Ok(()) }
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use riff_core::record::{Line, Record};
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

/// The start of the name of each chunk.
pub const LOG: &str = "log/";

/// The format of a chunk that this build writes.
pub const FORMAT: u32 = 1;

/// The first line of a chunk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub format: u32,
    /// The position of the first record of the chunk.
    pub first: u64,
}

/// How long the writer tries a chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// The writer tries a chunk again for this long after the first try.
    pub retry_for: Duration,
    /// A try that takes longer than this is a timeout.
    pub attempt: Duration,
    /// The first wait before a new try. Each wait is double the last,
    /// up to 2 seconds.
    pub backoff: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            retry_for: Duration::from_secs(10),
            attempt: Duration::from_secs(3),
            backoff: Duration::from_millis(100),
        }
    }
}

/// The name of the chunk whose first record is at `first`.
///
/// ```
/// assert_eq!(riff_server::log::chunk_name(1234), "log/00000000000000001234.jsonl");
/// ```
pub fn chunk_name(first: u64) -> String {
    format!("{LOG}{first:020}.jsonl")
}

/// The bytes of a chunk: the header, then one line for each record.
///
/// # Panics
///
/// When `records` is empty.
pub fn encode(records: &[Record]) -> Vec<u8> {
    let header = Header {
        format: FORMAT,
        first: records[0].position,
    };
    let mut bytes = to_line(&header);
    for record in records {
        bytes.extend(to_line(record));
    }
    bytes
}

/// Reads the lines of a chunk.
///
/// ```
/// use riff_core::record::{Change, Line, Record, RiffStateSet};
/// use riff_core::wire::RiffState;
/// use riff_server::log::{decode, encode};
///
/// let record = Record {
///     position: 7,
///     written_at_ms: 0,
///     by: None,
///     command: None,
///     change: Change::RiffStateSet(RiffStateSet { state: RiffState::Paused }),
/// };
/// let (header, lines) = decode(&encode(&[record.clone()])).unwrap();
/// assert_eq!((header.format, header.first), (1, 7));
/// assert_eq!(lines, [Line::Record(Box::new(record))]);
/// assert!(decode(b"").is_err());
/// ```
pub fn decode(bytes: &[u8]) -> Result<(Header, Vec<Line>), String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let mut lines = text.lines();
    let header: Header = serde_json::from_str(lines.next().ok_or("the chunk is empty")?)
        .map_err(|e| format!("the header does not read: {e}"))?;
    if header.format > FORMAT {
        return Err(format!(
            "the chunk has format {}, and this build reads format {FORMAT} only",
            header.format
        ));
    }
    let lines = lines
        .enumerate()
        .map(|(n, line)| Line::parse(line).map_err(|e| format!("line {}: {e}", n + 2)))
        .collect::<Result<_, _>>()?;
    Ok((header, lines))
}

/// Writes `records` as one new chunk. See "A failed write" in the module
/// docs. Before each try, it asks `may_write` whether the instance still
/// holds the lease (R155). A try without the lease counts as a failed
/// try. So each try ends at most [`Timing::attempt`] after the lease
/// ends, far less than the wait of a new instance. An error means that
/// the instance must stop for good.
pub async fn write(
    store: &dyn Store,
    records: &[Record],
    timing: &Timing,
    may_write: impl Fn() -> bool,
) -> Result<(), StoreError> {
    write_counted(store, records, timing, may_write, &AtomicU64::new(0)).await
}

/// As [`write()`]. It adds 1 to `failed` for each try that failed, for the
/// facts of `riff server` (01M3TJWJ12WEDCXW3W0529KRP2).
///
/// ```
/// # #[tokio::main] async fn main() -> Result<(), riff_server::store::StoreError> {
/// use std::sync::atomic::{AtomicU64, Ordering};
/// use riff_core::record::{Change, Record, RiffStateSet};
/// use riff_core::wire::RiffState;
/// use riff_server::log::{Timing, write_counted};
/// use riff_server::store::Memory;
///
/// let record = Record {
///     position: 1,
///     written_at_ms: 0,
///     by: None,
///     command: None,
///     change: Change::RiffStateSet(RiffStateSet { state: RiffState::Running }),
/// };
/// let (store, failed) = (Memory::default(), AtomicU64::new(0));
/// write_counted(&store, &[record.clone()], &Timing::default(), || true, &failed).await?;
/// assert_eq!(failed.load(Ordering::SeqCst), 0);
/// // Another object has the name: the first try fails for good.
/// let again = write_counted(&store, &[record], &Timing::default(), || true, &failed).await;
/// assert!(again.is_err());
/// assert_eq!(failed.load(Ordering::SeqCst), 1);
/// # Ok(()) }
/// ```
pub async fn write_counted(
    store: &dyn Store,
    records: &[Record],
    timing: &Timing,
    may_write: impl Fn() -> bool,
    failed: &AtomicU64,
) -> Result<(), StoreError> {
    let name = chunk_name(records[0].position);
    let bytes = encode(records);
    let start = tokio::time::Instant::now();
    let mut backoff = timing.backoff;
    let mut first = true;
    loop {
        let tried = if may_write() {
            tokio::time::timeout(timing.attempt, store.save(&name, bytes.clone(), None)).await
        } else {
            Ok(Err(StoreError::Failed(format!(
                "the instance does not hold the lease, so it does not write {name}"
            ))))
        };
        let error = match tried {
            Ok(Ok(_)) => return Ok(()),
            Ok(Err(error @ StoreError::Conflict(_))) if first => {
                failed.fetch_add(1, Ordering::SeqCst);
                return Err(error);
            }
            Ok(Err(StoreError::Conflict(_))) => {
                return match store.load(&name).await {
                    Ok(Some(loaded)) if loaded.bytes == bytes => Ok(()),
                    Ok(_) => {
                        failed.fetch_add(1, Ordering::SeqCst);
                        Err(StoreError::Conflict(name))
                    }
                    Err(error) => {
                        failed.fetch_add(1, Ordering::SeqCst);
                        Err(error)
                    }
                };
            }
            Ok(Err(error)) => error,
            Err(_) => StoreError::Failed(format!("the write of {name} timed out")),
        };
        first = false;
        failed.fetch_add(1, Ordering::SeqCst);
        if start.elapsed() + backoff > timing.retry_for {
            return Err(error);
        }
        tracing::warn!("the write of {name} failed, and is tried again: {error}");
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(2));
    }
}

/// Reads each record of the log, in order. See "The replay" in the
/// module docs.
pub async fn replay(store: &dyn Store) -> Result<Replayed, StoreError> {
    replay_after(store, 0).await
}

/// The first position of a chunk, from its name.
///
/// ```
/// use riff_server::log::{chunk_name, first_of};
///
/// assert_eq!(first_of(&chunk_name(1234)), Some(1234));
/// assert_eq!(first_of("log/x.jsonl"), None);
/// ```
pub fn first_of(name: &str) -> Option<u64> {
    name.strip_prefix(LOG)?.strip_suffix(".jsonl")?.parse().ok()
}

/// The chunks of the log, with their first positions, in log order.
pub async fn chunks(store: &dyn Store) -> Result<Vec<(u64, String)>, StoreError> {
    let mut chunks: Vec<(u64, String)> = store
        .list(LOG)
        .await?
        .into_iter()
        .filter_map(|name| Some((first_of(&name)?, name)))
        .collect();
    chunks.sort();
    Ok(chunks)
}

/// Reads each record after the position `after`, in order: from the
/// chunk that holds the position `after + 1`. See "The replay" in the
/// module docs.
///
/// ```
/// # #[tokio::main] async fn main() -> Result<(), riff_server::store::StoreError> {
/// use riff_core::record::{Change, Record, RiffStateSet};
/// use riff_core::wire::RiffState;
/// use riff_server::log::{Timing, replay_after, write};
/// use riff_server::store::Memory;
///
/// let store = Memory::default();
/// let record = |position| Record {
///     position,
///     written_at_ms: 0,
///     by: None,
///     command: None,
///     change: Change::RiffStateSet(RiffStateSet { state: RiffState::Running }),
/// };
/// write(&store, &[record(1), record(2)], &Timing::default(), || true).await?;
/// write(&store, &[record(3)], &Timing::default(), || true).await?;
/// let replayed = replay_after(&store, 1).await?;
/// let positions: Vec<u64> = replayed.records.iter().map(|r| r.position).collect();
/// assert_eq!(positions, [2, 3]);
/// assert!(replay_after(&store, 3).await?.records.is_empty());
/// # Ok(()) }
/// ```
pub async fn replay_after(store: &dyn Store, after: u64) -> Result<Replayed, StoreError> {
    let chunks = chunks(store).await?;
    let from = chunks
        .iter()
        .rposition(|(first, _)| *first <= after + 1)
        .unwrap_or(0);
    let mut records = Vec::new();
    let mut skipped = None;
    let mut skips = 0;
    let mut next = None;
    let mut last_name = None;
    for (_, name) in &chunks[from..] {
        let not_valid = |why: String| StoreError::not_valid(store, name, why);
        let Some(loaded) = store.load(name).await? else {
            return Err(not_valid("the chunk is gone".into()));
        };
        let (header, lines) = decode(&loaded.bytes).map_err(not_valid)?;
        // The first chunk holds the position after + 1, or starts the log.
        let expected = next.unwrap_or(if header.first <= after + 1 {
            header.first
        } else {
            after + 1
        });
        if header.first != expected {
            return Err(not_valid(format!(
                "the chunk starts at position {}, and the log needs {expected}",
                header.first
            )));
        }
        let mut position = header.first;
        for line in lines {
            let at = match &line {
                Line::Record(record) => record.position,
                Line::Unknown { position, .. } => *position,
            };
            if at != position {
                return Err(not_valid(format!(
                    "a record has position {at}, and the log needs {position}"
                )));
            }
            position += 1;
            if at <= after {
                continue;
            }
            match line {
                Line::Record(record) => records.push(*record),
                Line::Unknown { position, kind } => {
                    tracing::warn!(position, "skipped a record of the unknown kind {kind}");
                    skipped = skipped.or(Some(position));
                    skips += 1;
                }
            }
        }
        next = Some(position);
        last_name = Some(name);
    }
    let last = next.map_or(after, |next| next - 1);
    if last < after
        && let Some(name) = last_name
    {
        return Err(StoreError::not_valid(
            store,
            name,
            format!("the log ends at position {last}, before the checkpoint at {after}"),
        ));
    }
    Ok(Replayed {
        records,
        last,
        skipped,
        skips,
        chunks: chunks.len() as u64,
    })
}

/// The log as [`replay`] reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replayed {
    /// Each record of a kind that this build knows, in order.
    pub records: Vec<Record>,
    /// The position of the last record, also of a skipped one. The next
    /// record comes after it, so it never takes the position of a skipped
    /// record.
    pub last: u64,
    /// The position of the first record that this build skipped.
    pub skipped: Option<u64>,
    /// The number of records that this build skipped.
    pub skips: u64,
    /// The number of chunks in the store, also the ones that the replay
    /// did not read.
    pub chunks: u64,
}

fn to_line(value: &impl Serialize) -> Vec<u8> {
    // Each key is a string and each value is plain data, so this cannot fail.
    let mut line = serde_json::to_vec(value).expect("a record is valid JSON");
    line.push(b'\n');
    line
}
