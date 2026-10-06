//! The checkpoints of the log of `riff-server`.
//!
//! # Design
//!
//! A checkpoint is the state that the log gives up to a position, as one
//! object of JSON in the [`Store`] (01M3TBZBMMSMNWP126ZQED13YG). A start
//! loads the newest checkpoint that reads, and replays only the records
//! after its position ([`crate::log::replay_after`]). So the start time
//! depends on the size of the state and the records after the
//! checkpoint, not on the whole history.
//!
//! ```mermaid
//! flowchart LR
//!     W[writer: chunks] --> L[(log/)]
//!     T[timer] -->|each 1,000 records,<br/>or each 60 minutes when records came| S[snapshot of the written state<br/>under the lock]
//!     S -->|encode outside the lock| C[(checkpoint/)]
//!     C --> P[keep the last 3 and one each day for 30 days]
//!     P -->|delete each chunk that no kept checkpoint needs| L
//! ```
//!
//! - The name of a checkpoint is its position, with zeros in front, and
//!   the time of its write: `checkpoint/00000000000000001234-1790000000000.json`
//!   ([`name`]). So the names sort by position.
//! - A [`Checkpoint`] holds the format, the build that wrote it, the time
//!   of its write, and a [`Snapshot`]: the state that the log gives, the
//!   last [`crate::state::KEEP_MESSAGES`] messages of each thread, the
//!   read cursors, and the last call of each session.
//! - A write is "only when new", as a chunk.
//!
//! # The rules (01M3TBZBQDF0ES4KM54FJQF6Z8)
//!
//! - The server writes a checkpoint each [`Settings::every_records`]
//!   records, or each [`Settings::every`] when records came.
//! - A build writes no checkpoint past the first record that it skipped
//!   ([`crate::log::Replayed::skipped`]): a record of a kind that it
//!   does not know, or with a value that it read as `other`
//!   (01M3XM2C18TT8VSKGD77YPZG53). A build writes no checkpoint
//!   while the newest checkpoint comes from a later version, or does not
//!   read ([`Found::blocked`]). So a rollback and a roll forward lose no
//!   record: the newer build replays each record after its own
//!   checkpoint.
//! - The server keeps the last [`Settings::keep_last`] checkpoints, and
//!   the newest checkpoint of each day for [`Settings::keep_days`] days
//!   ([`kept`]). It deletes the others.
//! - It deletes a chunk only when each kept checkpoint is past it
//!   ([`prune`]). No rule deletes chunks by age.
//! - When the newest checkpoint does not read, the start uses the one
//!   before it ([`load`]).
//!
//! # Example
//!
//! ```
//! # #[tokio::main] async fn main() -> Result<(), riff_server::store::StoreError> {
//! use std::time::Instant;
//! use riff_server::checkpoint::{Checkpoint, load, write};
//! use riff_server::state::State;
//! use riff_server::store::Memory;
//!
//! let store = Memory::default();
//! let now = Instant::now();
//! let snapshot = State::replay([], now, 0).snapshot(now, 0);
//! write(&store, &Checkpoint::new("0.8.0", 5, snapshot.clone())).await?;
//! let found = load(&store, "0.8.0").await?;
//! assert_eq!(found.checkpoint.unwrap().state, snapshot);
//! assert_eq!(found.blocked, None);
//! # Ok(()) }
//! ```

use std::collections::BTreeSet;
use std::time::Duration;

use riff_core::build::Build;
use serde::{Deserialize, Serialize};

use crate::log;
use crate::state::Snapshot;
use crate::store::{Store, StoreError};

/// The start of the name of each checkpoint.
pub const CHECKPOINT: &str = "checkpoint/";

/// The format of a checkpoint that this build writes.
pub const FORMAT: u32 = 1;

/// One day in milliseconds.
const DAY_MS: u64 = 24 * 60 * 60 * 1000;

/// When the server writes checkpoints, and which it keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// A checkpoint after this many records.
    pub every_records: u64,
    /// A checkpoint after this time, when records came.
    pub every: Duration,
    /// The timer looks this often.
    pub check_every: Duration,
    /// Keep this many of the newest checkpoints.
    pub keep_last: usize,
    /// Keep the newest checkpoint of each day for this many days.
    pub keep_days: u64,
    /// The build that writes the checkpoints: its crate version.
    pub build: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            every_records: 1000,
            every: Duration::from_secs(60 * 60),
            check_every: Duration::from_secs(10),
            keep_last: 3,
            keep_days: 30,
            build: Build::this().version,
        }
    }
}

/// One checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub format: u32,
    /// The crate version of the build that wrote it.
    pub build: String,
    /// The time of the write, in milliseconds since the Unix epoch.
    pub written_at_ms: u64,
    pub state: Snapshot,
}

impl Checkpoint {
    /// A checkpoint of `state` that the build `build` writes at
    /// `written_at_ms`.
    pub fn new(build: &str, written_at_ms: u64, state: Snapshot) -> Checkpoint {
        Checkpoint {
            format: FORMAT,
            build: build.to_owned(),
            written_at_ms,
            state,
        }
    }
}

/// The name of the checkpoint at `position`, written at `written_at_ms`.
///
/// ```
/// use riff_server::checkpoint::{name, parse};
///
/// let name = name(1234, 1_790_000_000_000);
/// assert_eq!(name, "checkpoint/00000000000000001234-1790000000000.json");
/// assert_eq!(parse(&name), Some((1234, 1_790_000_000_000)));
/// assert_eq!(parse("checkpoint/x.json"), None);
/// ```
pub fn name(position: u64, written_at_ms: u64) -> String {
    format!("{CHECKPOINT}{position:020}-{written_at_ms}.json")
}

/// The position and the time of a checkpoint, from its name.
pub fn parse(name: &str) -> Option<(u64, u64)> {
    let (position, at) = name
        .strip_prefix(CHECKPOINT)?
        .strip_suffix(".json")?
        .split_once('-')?;
    Some((position.parse().ok()?, at.parse().ok()?))
}

/// The bytes of a checkpoint.
pub fn encode(checkpoint: &Checkpoint) -> Vec<u8> {
    // Each key is a string and each value is plain data, so this cannot fail.
    serde_json::to_vec(checkpoint).expect("a checkpoint is valid JSON")
}

/// Reads a checkpoint. A checkpoint of a later format does not read.
pub fn decode(bytes: &[u8]) -> Result<Checkpoint, String> {
    #[derive(Deserialize)]
    struct Format {
        format: u32,
    }
    let Format { format } = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if format > FORMAT {
        return Err(format!(
            "the checkpoint has format {format}, and this build reads format {FORMAT} only"
        ));
    }
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

/// True when the version `theirs` is later than `ours`. A version that is
/// not a semantic version is not later.
///
/// ```
/// use riff_server::checkpoint::later;
///
/// assert!(later("0.9.0", "0.8.1"));
/// assert!(!later("0.8.1", "0.8.1"));
/// assert!(!later("next", "0.8.1"));
/// ```
pub fn later(theirs: &str, ours: &str) -> bool {
    use riff_core::build::Semver;
    match (theirs.parse::<Semver>(), ours.parse::<Semver>()) {
        (Ok(theirs), Ok(ours)) => theirs > ours,
        _ => false,
    }
}

/// What a start finds in `checkpoint/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// The newest checkpoint that reads.
    pub checkpoint: Option<Checkpoint>,
    /// Why this build writes no checkpoint: the newest checkpoint comes
    /// from a later version, or does not read.
    pub blocked: Option<String>,
    /// The name of the newest checkpoint in the store, also when it does
    /// not read. A start compares it with the name after the wait for the
    /// lease (01M3TJWJC08ZR5TWA1Y9CDE0QM).
    pub newest: Option<String>,
}

/// Loads the newest checkpoint that reads. It logs a warning for each
/// newer one that does not read. `build` is the crate version of this
/// build.
pub async fn load(store: &dyn Store, build: &str) -> Result<Found, StoreError> {
    let names = names(store).await?;
    let newest = names.last().map(|(_, _, name)| name.clone());
    let mut blocked = None;
    for (_, _, name) in names.iter().rev() {
        let read = match store.load(name).await? {
            Some(loaded) => decode(&loaded.bytes),
            None => Err("the checkpoint is gone".into()),
        };
        match read {
            Ok(checkpoint) => {
                if blocked.is_none() && later(&checkpoint.build, build) {
                    blocked = Some(format!(
                        "the newest checkpoint {name} comes from the later version {}",
                        checkpoint.build
                    ));
                }
                return Ok(Found {
                    checkpoint: Some(checkpoint),
                    blocked,
                    newest,
                });
            }
            Err(why) => {
                tracing::warn!("the checkpoint {} does not read: {why}", store.locate(name));
                blocked
                    .get_or_insert_with(|| format!("the newest checkpoint {name} does not read"));
            }
        }
    }
    Ok(Found {
        checkpoint: None,
        blocked,
        newest,
    })
}

/// The position, the time and the name of each checkpoint in `store`,
/// from the names, in the order of the positions.
///
/// ```
/// # #[tokio::main] async fn main() -> Result<(), riff_server::store::StoreError> {
/// use riff_server::checkpoint::{name, names};
/// use riff_server::store::{Memory, Store};
///
/// let store = Memory::default();
/// for position in [20, 3] {
///     store.save(&name(position, 5), b"{}".to_vec(), None).await?;
/// }
/// let positions: Vec<u64> = names(&store).await?.iter().map(|n| n.0).collect();
/// assert_eq!(positions, [3, 20]);
/// # Ok(()) }
/// ```
pub async fn names(store: &dyn Store) -> Result<Vec<(u64, u64, String)>, StoreError> {
    let mut names: Vec<(u64, u64, String)> = store
        .list(CHECKPOINT)
        .await?
        .into_iter()
        .filter_map(|name| parse(&name).map(|(position, at)| (position, at, name)))
        .collect();
    names.sort();
    Ok(names)
}

/// Writes a checkpoint as a new object. Gives its name.
pub async fn write(store: &dyn Store, checkpoint: &Checkpoint) -> Result<String, StoreError> {
    let name = name(checkpoint.state.position, checkpoint.written_at_ms);
    store.save(&name, encode(checkpoint), None).await?;
    Ok(name)
}

/// The checkpoints to keep, from their positions and times: the last
/// `keep_last`, and the newest of each day of the last `keep_days` days
/// before `now_ms`.
///
/// ```
/// use std::collections::BTreeSet;
/// use riff_server::checkpoint::kept;
///
/// const DAY: u64 = 24 * 60 * 60 * 1000;
/// let now = 40 * DAY;
/// // Four checkpoints today, one on each of two days before, and one
/// // from 35 days ago.
/// let all = [(10, 5 * DAY), (20, 38 * DAY), (30, 39 * DAY), (40, now), (50, now + 1), (60, now + 2), (70, now + 3)];
/// assert_eq!(kept(&all, now + 3, 3, 30), BTreeSet::from([20, 30, 50, 60, 70]));
/// ```
pub fn kept(all: &[(u64, u64)], now_ms: u64, keep_last: usize, keep_days: u64) -> BTreeSet<u64> {
    let mut sorted = all.to_vec();
    sorted.sort();
    let mut kept: BTreeSet<u64> = sorted
        .iter()
        .rev()
        .take(keep_last)
        .map(|(position, _)| *position)
        .collect();
    let today = now_ms / DAY_MS;
    let mut days = BTreeSet::new();
    for (position, at) in sorted.iter().rev() {
        let day = at / DAY_MS;
        if today.saturating_sub(day) < keep_days && days.insert(day) {
            kept.insert(*position);
        }
    }
    kept
}

/// What [`prune`] deleted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pruned {
    pub checkpoints: Vec<String>,
    pub chunks: Vec<String>,
}

/// Deletes each checkpoint that [`kept`] does not keep, then each chunk
/// whose records are all at or before the oldest kept checkpoint. It
/// never deletes the last chunk.
pub async fn prune(
    store: &dyn Store,
    settings: &Settings,
    now_ms: u64,
) -> Result<Pruned, StoreError> {
    let checkpoints = names(store).await?;
    let times: Vec<(u64, u64)> = checkpoints.iter().map(|(p, at, _)| (*p, *at)).collect();
    let keep = kept(&times, now_ms, settings.keep_last, settings.keep_days);
    let mut pruned = Pruned::default();
    for (position, _, name) in &checkpoints {
        if !keep.contains(position) {
            store.delete(name).await?;
            pruned.checkpoints.push(name.clone());
        }
    }
    let Some(oldest) = keep.first() else {
        return Ok(pruned);
    };
    let chunks = log::chunks(store).await?;
    for pair in chunks.windows(2) {
        let [(_, name), (next, _)] = pair else {
            continue;
        };
        if *next <= oldest + 1 {
            store.delete(name).await?;
            pruned.chunks.push(name.clone());
        }
    }
    Ok(pruned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Memory;
    use riff_core::record::{Change, Envelope, PauseSet, Record, Scope};
    use riff_core::wire::RiffState;
    use std::time::Instant;

    fn record(position: u64) -> Record {
        Record {
            envelope: Envelope {
                position,
                written_at_ms: 0,
                by: None,
                command: None,
                call: None,
            },
            change: Change::PauseSet(PauseSet {
                scope: Scope::Riff,
                state: RiffState::Running,
            }),
        }
    }

    fn snapshot(position: u64) -> Snapshot {
        let records = (1..=position).map(record);
        State::replay(records, Instant::now(), 0).snapshot(Instant::now(), 0)
    }

    use crate::state::State;

    #[tokio::test]
    async fn the_delete_keeps_each_chunk_that_a_kept_checkpoint_needs() {
        let store = Memory::default();
        let timing = log::Timing::default();
        // Chunks at 1-2, 3-4, 5-6, 7-8.
        for first in [1, 3, 5, 7] {
            let records = [record(first), record(first + 1)];
            log::write(&store, &records, &timing, || true)
                .await
                .unwrap();
        }
        let settings = Settings {
            keep_last: 2,
            keep_days: 0,
            ..Settings::default()
        };
        // Checkpoints at 2, 4 and 5. The last 2 are kept: 4 and 5.
        for position in [2, 4, 5] {
            let checkpoint = Checkpoint::new("0.8.0", position, snapshot(position));
            write(&store, &checkpoint).await.unwrap();
        }
        let pruned = prune(&store, &settings, 10).await.unwrap();
        assert_eq!(pruned.checkpoints, [name(2, 2)]);
        // The chunks 1-2 and 3-4 are at or before 4. The chunk 5-6 holds
        // position 5 and 6, so the checkpoint at 4 needs it.
        assert_eq!(pruned.chunks, [log::chunk_name(1), log::chunk_name(3)]);
        let left = log::chunks(&store).await.unwrap();
        assert_eq!(
            left.iter().map(|(first, _)| *first).collect::<Vec<_>>(),
            [5, 7]
        );
        // A start from each kept checkpoint still reads.
        for position in [4, 5] {
            let replayed = log::replay_after(&store, position).await.unwrap();
            assert_eq!(replayed.last, 8);
            assert_eq!(replayed.records[0].envelope.position, position + 1);
        }
    }

    #[tokio::test]
    async fn the_delete_never_deletes_the_last_chunk() {
        let store = Memory::default();
        log::write(&store, &[record(1)], &log::Timing::default(), || true)
            .await
            .unwrap();
        write(&store, &Checkpoint::new("0.8.0", 1, snapshot(1)))
            .await
            .unwrap();
        let pruned = prune(&store, &Settings::default(), 1).await.unwrap();
        assert_eq!(pruned, Pruned::default());
    }

    #[tokio::test]
    async fn a_start_uses_the_checkpoint_before_one_that_does_not_read() {
        let store = Memory::default();
        write(&store, &Checkpoint::new("0.8.0", 1, snapshot(1)))
            .await
            .unwrap();
        store
            .save(&name(2, 2), b"{\"format\":1}".to_vec(), None)
            .await
            .unwrap();
        let found = load(&store, "0.8.0").await.unwrap();
        assert_eq!(found.checkpoint.unwrap().state.position, 1);
        assert!(found.blocked.unwrap().contains("does not read"));
    }

    #[tokio::test]
    async fn a_checkpoint_of_a_later_version_blocks_the_writes() {
        let store = Memory::default();
        write(&store, &Checkpoint::new("0.9.0", 1, snapshot(1)))
            .await
            .unwrap();
        let found = load(&store, "0.8.0").await.unwrap();
        assert!(found.checkpoint.is_some());
        assert!(found.blocked.unwrap().contains("later version 0.9.0"));
        assert_eq!(load(&store, "0.9.0").await.unwrap().blocked, None);
    }

    #[test]
    fn a_checkpoint_of_a_later_format_does_not_read() {
        let error = decode(br#"{"format":2}"#).unwrap_err();
        assert!(error.contains("format 2"), "{error}");
    }
}
