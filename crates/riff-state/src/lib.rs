//! `riff-state`: the state machine of riff (01M4Z60B8AFV68ZSAFA540G73C).
//!
//! # Design
//!
//! riff keeps each change that must not be lost as a record in one log
//! (event sourcing). This crate holds the parts that turn the log into
//! a state, and a state into the log. Two programs use it:
//!
//! - `riff-server` (riff 1.x): the shared server of a riff. Its engine
//!   owns a [`state::State`], serves each command over HTTP, and writes
//!   the log to a Cloud Storage bucket or a folder.
//! - the riff daemon (riff 3.0): the state of the sessions of one
//!   machine, with no server. It runs each command through a
//!   [`local::Local`], and keeps the log and the checkpoints in its own
//!   folder ([`store::Dir`]).
//!
//! ```mermaid
//! flowchart LR
//!     S["riff-server: engine, HTTP, sign-in"] --> ST
//!     D["riff daemon: local::Local"] --> ST
//!     subgraph ST["riff-state"]
//!         State["state: check, queue, written, reply"]
//!         Log["log: chunks, replay"]
//!         CP["checkpoint: write, load, prune"]
//!         Store["store: Memory, Dir"]
//!     end
//!     Log --> Store
//!     CP --> Store
//! ```
//!
//! | Module | Holds |
//! |---|---|
//! | [`state`] | [`state::State`]: the two copies of the riff, the presence, each command, and `apply`, the one function that changes a riff for a record. |
//! | [`log`] | The chunks of the log in a store, the write with its retry, and the replay. |
//! | [`checkpoint`] | The checkpoint of a state: its write, the load of the newest one, and the prune. |
//! | [`store`] | The trait [`store::Store`], with [`store::Memory`] and [`store::Dir`]. |
//! | [`local`] | [`local::Local`]: a state with its log and its checkpoints in one store, for one process. |
//! | [`owner`] | The times and the notes of the owner role. |
//! | [`idle`] | The limits and the notes of idle workers. |
//!
//! The crate does no HTTP, and knows no sign-in. The format of the log
//! and of the checkpoint is the format of riff-server: its format tests
//! (`crates/riff-server/tests/format.rs`) check the fixtures of each
//! release against this crate.

pub mod checkpoint;
pub mod idle;
pub mod local;
pub mod log;
pub mod owner;
pub mod state;
pub mod store;

/// A time in milliseconds since the Unix epoch as UTC text, to the
/// second.
///
/// ```
/// assert_eq!(riff_state::utc(1_790_000_000_000), "2026-09-21T14:13:20Z");
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
