//! `riff-state`: the state machine of riff (01M4ZHW1Y1X21ZPV053D0CXWMC).
//!
//! # Design
//!
//! riff keeps each change that must not be lost as a record in one log
//! (event sourcing). This crate holds the parts that turn the log into
//! a state, and a state into the log. Only `riff-server`, the shared
//! server of a riff, uses it. Its engine owns a [`state::State`], serves
//! each command over HTTP, and writes the log to a Cloud Storage bucket
//! or a folder.
//!
//! ```mermaid
//! flowchart LR
//!     S["riff-server: engine, HTTP, sign-in"] --> ST
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
//! | [`owner`] | The times and the notes of the owner role. |
//!
//! The crate does no HTTP, and knows no sign-in. The format of the log
//! and of the checkpoint is the format of riff-server: its format tests
//! (`crates/riff-server/tests/format.rs`) check the fixtures of each
//! release against this crate.

pub mod checkpoint;
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
