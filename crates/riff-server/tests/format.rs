//! The format of release 1.0.0: the log, the checkpoint and the kinds
//! (01M3WNQR41K41TV832GRQZ2CQS).
//!
//! The directory `fixtures/1.0.0` holds the format of the release:
//!
//! - `kinds.json`: the name of each record kind and of each command
//!   kind of the release (01M3XM2C3MND6YB24SGZ565353), and each field
//!   of the envelope of a record. A name never goes out of this list.
//!   The field `call` came after the release
//!   (01M48VFFY5CK9MRXJESV2NHY5F): a record of the release has none,
//!   and a build of the release skips it.
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
//!   object, a number and `null`, a kind of a message, a selector with
//!   a field or a value of a later build and in each other form of
//!   JSON, and a session URI with a query part of a later build
//!   (01M3XSF90E9JYYTC13D9THY4WE).
//!
//! Never write `log.jsonl`, `replayed.json` or `checkpoint.json` again
//! with a later build.
//! A later release adds a directory of its own
//! (01M43GSRSDJMGAH8SR1GD4Z3XF), with the same names:
//!
//! - `fixtures/1.1.0`: the holds. `kinds.json` has only the new kinds
//!   of the release; the test of the kinds takes the union of the lists
//!   of each release. `log.jsonl` has each new kind, and ends with holds
//!   in two repositories, so `replayed.json` has the part `plans`.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use riff_core::record::{By, Change, Envelope, Line, Record};
use riff_server::checkpoint::{self, Checkpoint};
use riff_server::log;
use riff_server::state::{CommandKind, Riff, State, apply};
use riff_server::store::Memory;
use serde::Deserialize;

/// The release of the fixtures of the holds.
const HOLDS: &str = "1.1.0";

/// Each release with a directory of its own and a `kinds.json`.
const RELEASES: [&str; 2] = ["1.0.0", HOLDS];

fn bytes(name: &str) -> Vec<u8> {
    bytes_of("1.0.0", name)
}

fn bytes_of(release: &str, name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(release)
        .join(name);
    std::fs::read(path).unwrap()
}

/// The records of a chunk of the fixtures of 1.0.0. Each one is of a
/// kind that this build knows.
fn records(name: &str) -> Vec<Record> {
    records_of("1.0.0", name)
}

fn records_of(release: &str, name: &str) -> Vec<Record> {
    let (_, lines) = log::decode(&bytes_of(release, name)).unwrap();
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
    #[serde(default)]
    envelope: Vec<String>,
    records: Vec<String>,
    commands: Vec<String>,
}

/// The union of the lists of each release, in the order of the
/// releases.
fn kinds() -> Kinds {
    let mut all = Kinds {
        envelope: Vec::new(),
        records: Vec::new(),
        commands: Vec::new(),
    };
    for release in RELEASES {
        let kinds: Kinds = serde_json::from_slice(&bytes_of(release, "kinds.json")).unwrap();
        all.envelope.extend(kinds.envelope);
        all.records.extend(kinds.records);
        all.commands.extend(kinds.commands);
    }
    all
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

/// The logs of the releases together have a record of each kind. The
/// log of each release has each kind of its own list.
#[test]
fn the_fixture_log_has_a_record_of_each_kind() {
    let holds = records_of(HOLDS, "log.jsonl");
    let new: Kinds = serde_json::from_slice(&bytes_of(HOLDS, "kinds.json")).unwrap();
    let found = set(holds.iter().map(|record| record.change.kind()));
    let listed = set(new.records.iter().map(String::as_str));
    assert!(found.is_superset(&listed), "{found:?}");
    assert!(holds.iter().all(|record| record.other().is_none()));

    let records = records("log.jsonl");
    let found = set(records.iter().map(|record| record.change.kind()));
    let all = found.union(&listed).copied().collect::<BTreeSet<_>>();
    assert_eq!(all, set(Change::KINDS.iter().copied()));
    // No record of the release has a value that reads as `other`.
    assert!(records.iter().all(|record| record.other().is_none()));
    // A record with no cause reads.
    assert!(records.iter().any(|record| record.envelope.by.is_none()));
    // The first records come from the command `import` of go-live.
    let import = records[0].envelope.command.as_deref();
    assert_eq!(import, Some(CommandKind::Import.as_str()));
    // A kind of command that the build does not know reads as text.
    let later = serde_json::to_string(&records[0])
        .unwrap()
        .replace(r#""command":"import""#, r#""command":"merge""#);
    let Line::Record(later) = Line::parse(&later).unwrap() else {
        panic!("a known kind of record");
    };
    let merge = later.envelope.command.as_deref();
    assert_eq!(merge, Some("merge"));
    assert!(
        CommandKind::ALL
            .iter()
            .all(|kind| Some(kind.as_str()) != merge)
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
        "session", "from", "to", "to", "to",
    ]
    .map(Some);
    assert_eq!(fields, expected);
    // This build writes a selector and a session URI of the later build
    // again as they came: the line of the record is the line of the
    // fixture.
    let text = String::from_utf8(bytes("later.jsonl")).unwrap();
    let lines: Vec<&str> = text.lines().skip(1).collect();
    assert_eq!(lines.len(), later.len());
    for at in 9..later.len() {
        assert_eq!(serde_json::to_string(&later[at]).unwrap(), lines[at]);
    }

    let store = Memory::default();
    let known = records("log.jsonl");
    let timing = log::Timing::default();
    log::write(&store, &known, &timing, || true).await.unwrap();
    let first = known.len() as u64 + 1;
    let moved: Vec<Record> = later
        .into_iter()
        .zip(first..)
        .map(|(mut record, position)| {
            record.envelope.position = position;
            record
        })
        .collect();
    log::write(&store, &moved, &timing, || true).await.unwrap();

    let replayed = log::replay(&store).await.unwrap();
    assert_eq!(replayed.records.len(), known.len() + moved.len());
    assert_eq!((replayed.skipped, replayed.skips), (Some(first), 15));
    // A start after the first such record counts the rest.
    let rest = log::replay_after(&store, first).await.unwrap();
    assert_eq!((rest.skipped, rest.skips), (Some(first + 1), 14));

    // The state takes each record. The messages are in their thread,
    // and a reader gets each one with its text.
    let now = Instant::now();
    let mut state = State::replay(replayed.records, now, 0);
    let bob = "riff://bob@kite/acme/app?session=b1".parse().unwrap();
    state.register(&bob, now);
    let thread = "acme/app".parse().unwrap();
    let messages = state.read(&bob, &thread, true, now).unwrap();
    let texts: Vec<&str> = messages.iter().map(|m| m.body.as_str()).collect();
    let expected = [
        "which wave is next?",
        "the wave starts",
        "from a lead of a later build",
        "to all",
        "to the wave",
        "to each form",
    ];
    assert!(texts.ends_with(&expected), "{texts:?}");
    // The session with a URI of a later build is the same session: it
    // holds the claim of the record. The part `lead=maybe` gives no
    // lead.
    let ann: riff_core::name::SessionUri = "riff://ann@heron/acme/app?session=a1".parse().unwrap();
    let uri = state.uri(ann.who(), now);
    assert!(uri.claims().contains(&"issue-17".to_owned()), "{uri}");
    let from = &messages[messages.len() - 4].from;
    assert!(from.is_other() && !from.lead() && from.who() == ann.who());
}

/// The log of 1.1.0 with the holds: this build writes its bytes, a
/// replay gives `replayed.json` with the part `plans`, and a load of
/// the checkpoint at each position gives the state of a replay
/// (01M43GSGVYJW7C09SVRWRAQZDZ).
#[test]
fn the_log_of_the_holds_gives_its_checkpoint_at_each_position() {
    let records = records_of(HOLDS, "log.jsonl");
    assert_eq!(log::encode(&records), bytes_of(HOLDS, "log.jsonl"));
    let expected = checkpoint::decode(&bytes_of(HOLDS, "replayed.json")).unwrap();
    assert_eq!(
        checkpoint::encode(&expected),
        bytes_of(HOLDS, "replayed.json")
    );

    let now = Instant::now();
    let full = State::replay(records.clone(), now, 0);
    assert_eq!(written(&full, &expected), bytes_of(HOLDS, "replayed.json"));
    let thread = "acme/app".parse().unwrap();
    let held = full.plans().hold(&thread, "issue-12").unwrap();
    assert_eq!(held.reason, "waits for the release of 1.1.0");
    assert_eq!(held.by, Some(riff_core::record::By::Person("ann".into())));
    assert!(full.plans().hold(&thread, "issue-13").is_none());
    let lib = "acme/lib".parse().unwrap();
    assert_eq!(full.plans().holds(&lib).count(), 1);

    for at in 0..=records.len() {
        let (before, after) = records.split_at(at);
        let state = State::replay(before.to_vec(), now, 0);
        let saved = checkpoint::decode(&written(&state, &expected)).unwrap();
        let alone = State::load(Some(saved.state.clone()), [], now, 0);
        assert!(alone.same_log_state(&state), "the checkpoint at {at}");
        let loaded = State::load(Some(saved.state), after.to_vec(), now, 0);
        assert!(loaded.same_log_state(&full), "a start from {at}");
    }
}

/// A riff with no hold writes no part `plans`: the log of 1.0.0 gives
/// the checkpoint of 1.0.0 (01M43GSGVYJW7C09SVRWRAQZDZ).
#[test]
fn a_riff_with_no_hold_writes_no_part_plans() {
    let replayed = String::from_utf8(bytes("replayed.json")).unwrap();
    assert!(!replayed.contains("\"plans\""));
    let held = String::from_utf8(bytes_of(HOLDS, "replayed.json")).unwrap();
    assert!(held.contains("\"plans\""));
}

/// The list names each field of the envelope of a record, and the code
/// has each field of the list: a field is never removed and never
/// renamed (01M3T4111PFM0C6KPREWFS9EQQ).
#[test]
fn the_list_has_each_field_of_the_envelope() {
    let Record { envelope, change } = records("log.jsonl")[0].clone();
    let record = Record {
        envelope: Envelope {
            position: envelope.position,
            written_at_ms: envelope.written_at_ms,
            by: Some(By::Server),
            command: Some("import".into()),
            call: Some("c1".into()),
        },
        change,
    };
    let json = serde_json::to_value(&record).unwrap();
    let code = set(json.as_object().unwrap().keys().map(String::as_str));
    let kinds = kinds();
    assert_eq!(code, set(kinds.envelope.iter().map(String::as_str)));
}

/// The envelope of a record of release 1.0.0: a copy of its type,
/// with no field `call`.
#[derive(Debug, Deserialize)]
struct Record100 {
    position: u64,
    written_at_ms: u64,
    #[serde(default)]
    by: Option<By>,
    #[serde(default)]
    command: Option<String>,
    change: Change,
}

/// A record with a call ID loads in a build that does not know the
/// field: the build of 1.0.0 skips it (01M48VFFY5CK9MRXJESV2NHY5F).
/// A record of 1.0.0 has no call ID.
#[test]
fn a_record_with_a_call_id_loads_in_a_build_of_the_release() {
    let records = records("log.jsonl");
    assert!(records.iter().all(|record| record.envelope.call.is_none()));
    for record in records {
        let mut with_call = record.clone();
        with_call.envelope.call = Some("c1".into());
        let line = serde_json::to_string(&with_call).unwrap();
        assert!(line.contains(r#""call":"c1""#), "{line}");
        let old: Record100 = serde_json::from_str(&line).unwrap();
        assert_eq!(old.position, record.envelope.position);
        assert_eq!(old.written_at_ms, record.envelope.written_at_ms);
        assert_eq!(
            (old.by, old.command),
            (record.envelope.by, record.envelope.command)
        );
        assert_eq!(old.change, record.change);
    }
}
