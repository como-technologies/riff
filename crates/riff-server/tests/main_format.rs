//! A log and a checkpoint that an earlier build wrote read with no
//! change (01M3WNQR41K41TV832GRQZ2CQS).
//!
//! The directory `fixtures/main-a2e9c98` holds what `main` wrote at the
//! commit a2e9c98, before the engine build:
//!
//! - `log.jsonl`: one chunk with each kind of record. A thread gets more
//!   messages than it keeps. Five sessions are forgotten.
//! - `checkpoint.json`: the checkpoint of the live state at the position
//!   246, with read cursors.
//! - `replayed.json`: the state of a full replay of the log, as a
//!   checkpoint.
//! - `loaded.json`: the state of a start from the checkpoint and the
//!   records after it, as a checkpoint.
//!
//! Never write these files again with a later build.

use std::path::PathBuf;
use std::time::Instant;

use riff_core::record::{Line, Record};
use riff_server::checkpoint::{self, Checkpoint};
use riff_server::log;
use riff_server::state::State;

fn bytes(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/main-a2e9c98")
        .join(name);
    std::fs::read(path).unwrap()
}

fn records() -> Vec<Record> {
    let (_, lines) = log::decode(&bytes("log.jsonl")).unwrap();
    lines
        .into_iter()
        .map(|line| match line {
            Line::Record(record) => *record,
            Line::Unknown { position, kind } => panic!("record {position}: the kind {kind}"),
        })
        .collect()
}

fn checkpoint(name: &str) -> Checkpoint {
    checkpoint::decode(&bytes(name)).unwrap()
}

/// The checkpoint of `state`, with the build and the time of `like`.
fn written(state: &State, like: &Checkpoint) -> Vec<u8> {
    let snapshot = state.snapshot(Instant::now(), 0);
    checkpoint::encode(&Checkpoint::new(&like.build, like.written_at_ms, snapshot))
}

#[test]
fn this_build_writes_the_bytes_of_the_log_and_the_checkpoint_of_main() {
    assert_eq!(log::encode(&records()), bytes("log.jsonl"));
    for name in ["checkpoint.json", "replayed.json", "loaded.json"] {
        assert_eq!(checkpoint::encode(&checkpoint(name)), bytes(name), "{name}");
    }
}

#[test]
fn a_replay_of_the_log_of_main_gives_the_state_of_main() {
    let state = State::replay(records(), Instant::now(), 0);
    let expected = checkpoint("replayed.json");
    assert_eq!(state.position(), expected.state.position);
    assert_eq!(written(&state, &expected), bytes("replayed.json"));
}

#[test]
fn a_start_from_the_checkpoint_of_main_gives_the_state_of_main() {
    let from = checkpoint("checkpoint.json");
    let rest: Vec<Record> = records()
        .into_iter()
        .filter(|record| record.position > from.state.position)
        .collect();
    assert!(!rest.is_empty());
    let now = Instant::now();
    let state = State::load(Some(from.state), rest, now, 0);
    let expected = checkpoint("loaded.json");
    assert_eq!(written(&state, &expected), bytes("loaded.json"));
    assert!(state.same_log_state(&State::replay(records(), now, 0)));
}
