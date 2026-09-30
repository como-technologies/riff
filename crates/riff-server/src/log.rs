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
//! record comes after it ([`Replayed::last`]).
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
            Ok(Err(error @ StoreError::Conflict(_))) if first => return Err(error),
            Ok(Err(StoreError::Conflict(_))) => {
                return match store.load(&name).await {
                    Ok(Some(loaded)) if loaded.bytes == bytes => Ok(()),
                    Ok(_) => Err(StoreError::Conflict(name)),
                    Err(error) => Err(error),
                };
            }
            Ok(Err(error)) => error,
            Err(_) => StoreError::Failed(format!("the write of {name} timed out")),
        };
        first = false;
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
    let mut names = store.list(LOG).await?;
    names.sort();
    let mut records = Vec::new();
    let mut next = 1;
    for name in names {
        let not_valid = |why: String| StoreError::not_valid(store, &name, why);
        let Some(loaded) = store.load(&name).await? else {
            return Err(not_valid("the chunk is gone".into()));
        };
        let (header, lines) = decode(&loaded.bytes).map_err(not_valid)?;
        if header.first != next {
            return Err(not_valid(format!(
                "the chunk starts at position {}, and the log needs {next}",
                header.first
            )));
        }
        for line in lines {
            let position = match &line {
                Line::Record(record) => record.position,
                Line::Unknown { position, .. } => *position,
            };
            if position != next {
                return Err(not_valid(format!(
                    "a record has position {position}, and the log needs {next}"
                )));
            }
            next += 1;
            match line {
                Line::Record(record) => records.push(*record),
                Line::Unknown { position, kind } => {
                    tracing::warn!(position, "skipped a record of the unknown kind {kind}");
                }
            }
        }
    }
    Ok(Replayed {
        records,
        last: next - 1,
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
}

fn to_line(value: &impl Serialize) -> Vec<u8> {
    // Each key is a string and each value is plain data, so this cannot fail.
    let mut line = serde_json::to_vec(value).expect("a record is valid JSON");
    line.push(b'\n');
    line
}
