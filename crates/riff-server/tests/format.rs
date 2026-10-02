//! The format of release 1.0.0: the log, the checkpoint and the kinds
//! (01M3WNQR41K41TV832GRQZ2CQS).
//!
//! The directory `fixtures/1.0.0` holds the format of the release:
//!
//! - `kinds.json`: the name of each record kind and of each command
//!   kind of the release (01M3XM2C3MND6YB24SGZ565353). A name never
//!   goes out of this list.
//! - `log.jsonl`: one chunk with each kind of record of the release
//!   (01M3XM2C60TKF05NETHY8EYP3P). It starts with the records that the
//!   import of go-live writes for the people. A thread gets more
//!   messages than it keeps. Five sessions are forgotten. Two records
//!   have no cause.
//! - `replayed.json`: the state of a full replay of the log, as a
//!   checkpoint. The end of the log keeps a lead, a pause of the whole
//!   riff and a pause of a repository.
//! - `checkpoint.json`: the checkpoint of a live state. It is the full
//!   replay, and then a session reads two threads. So it has read
//!   cursors, which no record gives.
//! - `later.jsonl`: one chunk of a later build. Each record has a value
//!   that this build does not know: a scope and a class of a caller, as
//!   a text and as an object, a reason of a start as a text, an
//!   object, a number and `null`, a kind of a message, and a field of a
//!   selector (01M3XSF90E9JYYTC13D9THY4WE).
//!
//! Never write `log.jsonl`, `replayed.json` or `checkpoint.json` again
//! with a later build.
//! A later release adds a directory of its own.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use riff_core::record::{Change, Line, Record};
use riff_server::checkpoint::{self, Checkpoint};
use riff_server::log;
use riff_server::state::{CommandKind, Riff, State, apply};
use riff_server::store::Memory;
use serde::Deserialize;

fn bytes(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/1.0.0")
        .join(name);
    std::fs::read(path).unwrap()
}

/// The records of a chunk of the fixtures. Each one is of a kind that
/// this build knows.
fn records(name: &str) -> Vec<Record> {
    let (_, lines) = log::decode(&bytes(name)).unwrap();
    lines
        .into_iter()
        .map(|line| match line {
            Line::Record(record) => *record,
            Line::Unknown { position, kind } => panic!("record {position}: the kind {kind}"),
        })
        .collect()
}

/// The names of `kinds.json`.
#[derive(Deserialize)]
struct Kinds {
    records: Vec<String>,
    commands: Vec<String>,
}

fn kinds() -> Kinds {
    serde_json::from_slice(&bytes("kinds.json")).unwrap()
}

fn set<'a>(names: impl IntoIterator<Item = &'a str>) -> BTreeSet<&'a str> {
    names.into_iter().collect()
}

/// The checkpoint of `state`, with the build and the time of `like`.
fn written(state: &State, like: &Checkpoint) -> Vec<u8> {
    let snapshot = state.snapshot(Instant::now(), 0);
    checkpoint::encode(&Checkpoint::new(&like.build, like.written_at_ms, snapshot))
}

/// A name of the list that is gone from the code fails the first
/// check: a kind is never renamed and never removed. A new kind of the
/// code that is not in the list fails the second one: add it to the
/// list of its release.
#[test]
fn the_code_has_each_kind_of_the_list_and_the_list_has_each_kind_of_the_code() {
    let kinds = kinds();
    let listed = set(kinds.records.iter().map(String::as_str));
    let code = set(Change::KINDS.iter().copied());
    assert_eq!(
        listed.difference(&code).collect::<Vec<_>>(),
        [] as [&&str; 0]
    );
    assert_eq!(
        code.difference(&listed).collect::<Vec<_>>(),
        [] as [&&str; 0]
    );
    assert_eq!(
        kinds.records.len(),
        listed.len(),
        "a name is there two times"
    );

    let listed = set(kinds.commands.iter().map(String::as_str));
    let code = set(CommandKind::ALL.iter().map(|kind| kind.as_str()));
    assert_eq!(
        listed.difference(&code).collect::<Vec<_>>(),
        [] as [&&str; 0]
    );
    assert_eq!(
        code.difference(&listed).collect::<Vec<_>>(),
        [] as [&&str; 0]
    );
    assert_eq!(
        kinds.commands.len(),
        listed.len(),
        "a name is there two times"
    );
}

/// The list of 1.0.0 has no kind that only a build of `main` before
/// the release wrote.
#[test]
fn the_list_of_the_release_has_no_riff_state_set() {
    assert!(!kinds().records.iter().any(|kind| kind == "riff_state_set"));
    assert!(!Change::KINDS.contains(&"riff_state_set"));
}

#[test]
fn the_fixture_log_has_a_record_of_each_kind() {
    let records = records("log.jsonl");
    let found = set(records.iter().map(|record| record.change.kind()));
    assert_eq!(found, set(Change::KINDS.iter().copied()));
    // No record of the release has a value that reads as `other`.
    assert!(records.iter().all(|record| record.other().is_none()));
    // A record with no cause reads, and a kind of command that the
    // build does not know reads as text.
    assert!(records.iter().any(|record| record.by.is_none()));
    let import = records[0].command.as_deref();
    assert_eq!(import, Some("import"));
    assert!(
        CommandKind::ALL
            .iter()
            .all(|kind| Some(kind.as_str()) != import)
    );
}

#[test]
fn this_build_writes_the_bytes_of_the_log_and_the_checkpoint_of_the_release() {
    assert_eq!(log::encode(&records("log.jsonl")), bytes("log.jsonl"));
    let replayed = checkpoint::decode(&bytes("replayed.json")).unwrap();
    assert_eq!(checkpoint::encode(&replayed), bytes("replayed.json"));
}

#[test]
fn a_replay_of_the_fixture_log_gives_the_state_of_the_release() {
    let state = State::replay(records("log.jsonl"), Instant::now(), 0);
    let expected = checkpoint::decode(&bytes("replayed.json")).unwrap();
    assert_eq!(state.position(), expected.state.position);
    assert_eq!(written(&state, &expected), bytes("replayed.json"));
}

/// The time of the checkpoint of the live state.
const LIVE_MS: u64 = 1_792_678_500_000;

/// The live state of `checkpoint.json` at `now`: the full replay, and
/// then the session `a1` of ann reads two threads.
fn live(now: Instant) -> State {
    let mut state = State::replay(records("log.jsonl"), now, LIVE_MS);
    let ann = "riff://ann@heron/acme/app?session=a1".parse().unwrap();
    for thread in ["acme/app", "noise"] {
        let unread = state.read(&ann, &thread.parse().unwrap(), false, now);
        assert!(!unread.unwrap().is_empty(), "{thread}");
    }
    state
}

/// The read cursors are in the checkpoint, and a load keeps them: no
/// record gives them again.
#[test]
fn a_checkpoint_of_a_live_state_keeps_the_read_cursors() {
    let expected = checkpoint::decode(&bytes("checkpoint.json")).unwrap();
    assert_eq!(checkpoint::encode(&expected), bytes("checkpoint.json"));
    let encode = |state: &State, now| {
        let snapshot = state.snapshot(now, LIVE_MS);
        checkpoint::encode(&Checkpoint::new(
            &expected.build,
            expected.written_at_ms,
            snapshot,
        ))
    };
    let now = Instant::now();
    let state = live(now);
    assert_eq!(state.position(), expected.state.position);
    assert_eq!(encode(&state, now), bytes("checkpoint.json"));

    let loaded = State::load(Some(expected.state.clone()), [], now, LIVE_MS);
    assert!(loaded.same_log_state(&state));
    assert_eq!(encode(&loaded, now), bytes("checkpoint.json"));
}

/// Each part of the state that the log gives is in the checkpoint. At
/// each position of the fixture, a load of the checkpoint gives the
/// state of a replay up to that position, and a start from it gives the
/// state of a full replay. The checkpoint goes through its bytes.
#[test]
fn a_start_from_a_checkpoint_at_each_position_gives_the_state_of_a_full_replay() {
    let records = records("log.jsonl");
    let now = Instant::now();
    let full = State::replay(records.clone(), now, 0);
    let like = checkpoint::decode(&bytes("replayed.json")).unwrap();
    for at in 0..=records.len() {
        let (before, after) = records.split_at(at);
        let state = State::replay(before.to_vec(), now, 0);
        let saved = checkpoint::decode(&written(&state, &like)).unwrap();
        assert_eq!(saved.state.position, at as u64);
        // The checkpoint alone gives the state at its position. So a
        // part that a checkpoint loses fails at the first position that
        // has it, also when a later record removes the part.
        let alone = State::load(Some(saved.state.clone()), [], now, 0);
        assert!(alone.same_log_state(&state), "the checkpoint at {at}");
        let loaded = State::load(Some(saved.state), after.to_vec(), now, 0);
        assert!(loaded.same_log_state(&full), "a start from {at}");
    }
}

/// `apply` only stores what a record says (01M3WNQQWA7XGK4Y9ET8HJZ8NN):
/// each arm reads only its record and the riff. The riff after each
/// record of the fixture is the same with no presence and no clock
/// (`apply` alone), and in a state with a presence at two times.
#[test]
fn each_apply_arm_reads_only_its_record_and_the_riff() {
    let records = records("log.jsonl");
    let early = Instant::now();
    let late = early + Duration::from_secs(90 * 24 * 60 * 60);
    let mut riff = Riff::default();
    for (n, record) in records.iter().enumerate() {
        apply(&mut riff, record);
        let so_far = records[..=n].to_vec();
        let at_early = State::replay(so_far.clone(), early, 0);
        let at_late = State::replay(so_far, late, 1_900_000_000_000);
        assert!(at_early.same_log_state(&at_late), "record {}", n + 1);
        assert_eq!(*at_early.written_riff(), riff, "record {}", n + 1);
    }
}

/// The values of a later build read as `other`, and each such record
/// counts as a skipped record (01M3XM2C18TT8VSKGD77YPZG53).
#[tokio::test]
async fn a_value_of_a_later_build_reads_as_other_and_counts_as_a_skipped_record() {
    let later = records("later.jsonl");
    let fields: Vec<_> = later.iter().map(Record::other).collect();
    let expected = [
        "scope", "scope", "reason", "reason", "reason", "reason", "by", "by", "kind", "to",
    ]
    .map(Some);
    assert_eq!(fields, expected);
    // This build writes each field of the selector of the later build
    // again.
    let selector = r#""to":[{"user":"bob","wave":"17"}]"#;
    assert!(serde_json::to_string(&later[9]).unwrap().contains(selector));

    let store = Memory::default();
    let known = records("log.jsonl");
    let timing = log::Timing::default();
    log::write(&store, &known, &timing, || true).await.unwrap();
    let first = known.len() as u64 + 1;
    let moved: Vec<Record> = later
        .into_iter()
        .zip(first..)
        .map(|(record, position)| Record { position, ..record })
        .collect();
    log::write(&store, &moved, &timing, || true).await.unwrap();

    let replayed = log::replay(&store).await.unwrap();
    assert_eq!(replayed.records.len(), known.len() + moved.len());
    assert_eq!((replayed.skipped, replayed.skips), (Some(first), 10));
    // A start after the first such record counts the rest.
    let rest = log::replay_after(&store, first).await.unwrap();
    assert_eq!((rest.skipped, rest.skips), (Some(first + 1), 9));

    // The state takes each record. The two messages are in their
    // thread, and a reader gets each one with its text.
    let now = Instant::now();
    let mut state = State::replay(replayed.records, now, 0);
    let bob = "riff://bob@kite/acme/app?session=b1".parse().unwrap();
    state.register(&bob, now);
    let thread = "acme/app".parse().unwrap();
    let messages = state.read(&bob, &thread, true, now).unwrap();
    let texts: Vec<&str> = messages.iter().map(|m| m.body.as_str()).collect();
    assert!(
        texts.ends_with(&["which wave is next?", "the wave starts"]),
        "{texts:?}"
    );
}
