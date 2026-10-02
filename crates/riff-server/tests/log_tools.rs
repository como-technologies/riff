//! The tools of the log on a directory store: `riff-server log`
//! (01M3TJWHNYRCA7RTPFNYM5ZNQS), `riff-server log verify`
//! (01M3TJWHRP49M66NYNHWSYD3XP) and `riff-server log cut`
//! (01M3TJWHVN730ZWCWHT9ER186R). Each test runs the real binary.

use std::fs;
use std::path::Path;
use std::time::Instant;

use isolated::Isolated;
use riff_core::name::Who;
use riff_core::record::{By, Change, Claimed, Record, RiffStateSet};
use riff_core::wire::RiffState;
use riff_server::checkpoint::{self, Checkpoint};
use riff_server::log::{self, chunk_name};
use riff_server::state::State;
use riff_server::store::Dir;

/// A record from before the cause.
fn running(position: u64) -> Record {
    Record {
        position,
        written_at_ms: 1_790_000_000_000,
        by: None,
        command: None,
        change: Change::RiffStateSet(RiffStateSet {
            state: RiffState::Running,
        }),
    }
}

/// A record with its cause: a claim of the session.
fn claimed(position: u64) -> Record {
    Record {
        position,
        written_at_ms: 1_790_000_000_000,
        by: Some(By::Session(Who::new("ann", Some("s1")).unwrap())),
        command: Some("claim".into()),
        change: Change::Claimed(Claimed {
            session: "riff://ann@heron/acme/app?session=s1".parse().unwrap(),
            thread: "acme/app".parse().unwrap(),
            item: format!("issue-{position}"),
        }),
    }
}

/// A directory store with the chunks 1-2, 3-4 and 5-6, and a checkpoint
/// at each position of `checkpoints`.
async fn store(checkpoints: &[u64]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let store = Dir::new(dir.path());
    for first in [1, 3, 5] {
        let records = [running(first), claimed(first + 1)];
        log::write(&store, &records, &log::Timing::default(), || true)
            .await
            .unwrap();
    }
    for position in checkpoints {
        let records = (1..=*position).map(running);
        let snapshot = State::replay(records, Instant::now(), 0).snapshot(Instant::now(), 0);
        let checkpoint = Checkpoint::new("0.8.0", *position, snapshot);
        checkpoint::write(&store, &checkpoint).await.unwrap();
    }
    dir
}

/// The result of `riff-server ARGS --dir DIR`: success, stdout and
/// stderr.
fn tool(dir: &Path, args: &[&str]) -> (bool, String, String) {
    let out = Isolated::shared()
        .riff_server()
        .args(args)
        .arg("--dir")
        .arg(dir)
        .output()
        .unwrap();
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    (out.status.success(), text(&out.stdout), text(&out.stderr))
}

/// The files of `dir` under `part`, in name order.
fn files(dir: &Path, part: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir.join(part))
        .map(|entries| {
            entries
                .map(|entry| format!("{part}/{}", entry.unwrap().file_name().to_string_lossy()))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[tokio::test]
async fn log_prints_the_records_as_text() {
    let dir = store(&[]).await;
    let (ok, stdout, stderr) = tool(dir.path(), &["log"]);
    assert!(ok, "{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 6, "{stdout}");
    assert_eq!(
        lines[0],
        "1  2026-09-21T14:13:20Z  riff_state_set  running  (cause not known)"
    );
    assert_eq!(
        lines[1],
        "2  2026-09-21T14:13:20Z  claimed  issue-2 in acme/app by \
         riff://ann@heron/acme/app?session=s1  (claim, the session ann/s1)"
    );

    // From a position.
    let (ok, stdout, _) = tool(dir.path(), &["log", "--from", "5"]);
    assert!(ok);
    let positions: Vec<&str> = stdout
        .lines()
        .map(|line| line.split_once("  ").unwrap().0)
        .collect();
    assert_eq!(positions, ["5", "6"]);
}

#[tokio::test]
async fn log_needs_a_store() {
    let out = Isolated::shared()
        .riff_server()
        .arg("log")
        .env_remove("RIFF_DIR")
        .env_remove("RIFF_BUCKET")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--bucket BUCKET or --dir DIR"), "{stderr}");
}

#[tokio::test]
async fn log_verify_reads_a_good_log() {
    let dir = store(&[4]).await;
    let (ok, stdout, stderr) = tool(dir.path(), &["log", "verify"]);
    assert!(ok, "{stdout}{stderr}");
    // It reads from the oldest kept checkpoint, at position 4.
    assert_eq!(
        stdout,
        "The log reads: 1 chunk, 2 records from position 5 to 6, 1 checkpoint.\n"
    );
}

#[tokio::test]
async fn log_verify_names_a_bad_line() {
    let dir = store(&[]).await;
    let chunk = dir.path().join(chunk_name(3));
    let text = fs::read_to_string(&chunk).unwrap();
    let mut lines: Vec<&str> = text.lines().collect();
    lines[2] = "{\"position\":4,\"written_at_ms\":";
    fs::write(&chunk, lines.join("\n") + "\n").unwrap();

    let (ok, stdout, _) = tool(dir.path(), &["log", "verify"]);
    assert!(!ok, "a bad log exits with a failure");
    let first = stdout.lines().next().unwrap();
    assert!(
        first.contains("log/00000000000000000003.jsonl line 3: "),
        "{stdout}"
    );
    assert!(
        stdout.contains("The last good record of the log is at position 3."),
        "{stdout}"
    );
    assert!(stdout.contains("riff-server log cut --after 3"), "{stdout}");

    // The cut that it names removes the bad line, and the log reads.
    let (ok, stdout, stderr) = tool(dir.path(), &["log", "cut", "--after", "3"]);
    assert!(ok, "{stderr}");
    assert!(stdout.contains("4  (a line that does not read"), "{stdout}");
    let (ok, stdout, _) = tool(dir.path(), &["log", "verify"]);
    assert!(ok, "{stdout}");
}

#[tokio::test]
async fn log_cut_removes_the_chunks_after_a_position_and_names_the_records() {
    let dir = store(&[2, 4]).await;
    let (ok, stdout, stderr) = tool(dir.path(), &["log", "cut", "--after", "3"]);
    assert!(ok, "{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        [
            "4  2026-09-21T14:13:20Z  claimed  issue-4 in acme/app by \
             riff://ann@heron/acme/app?session=s1  (claim, the session ann/s1)",
            "5  2026-09-21T14:13:20Z  riff_state_set  running  (cause not known)",
            "6  2026-09-21T14:13:20Z  claimed  issue-6 in acme/app by \
             riff://ann@heron/acme/app?session=s1  (claim, the session ann/s1)",
            "Removed 3 records and 1 checkpoint after position 3. Threads: acme/app.",
        ]
    );
    assert_eq!(
        files(dir.path(), "log"),
        [chunk_name(1), chunk_name(3)],
        "the chunk after the position is gone"
    );
    assert_eq!(files(dir.path(), "checkpoint"), [checkpoint::name(2, 2)]);

    // The log now ends at the position.
    let (ok, stdout, _) = tool(dir.path(), &["log"]);
    assert!(ok);
    assert_eq!(stdout.lines().count(), 3);
    let (ok, stdout, _) = tool(dir.path(), &["log", "verify"]);
    assert!(ok, "{stdout}");
    // A second cut removes nothing.
    let (ok, stdout, _) = tool(dir.path(), &["log", "cut", "--after", "3"]);
    assert!(ok);
    assert_eq!(stdout, "Nothing is after position 3: removed nothing.\n");
}

#[tokio::test]
async fn log_cut_refuses_a_cut_before_the_oldest_kept_checkpoint() {
    let dir = store(&[4]).await;
    let (ok, stdout, stderr) = tool(dir.path(), &["log", "cut", "--after", "3"]);
    assert!(!ok);
    assert_eq!(stdout, "");
    assert!(
        stderr.contains("cannot cut after position 3: the oldest kept checkpoint is at position 4"),
        "{stderr}"
    );
    assert_eq!(files(dir.path(), "log").len(), 3, "the cut removed nothing");
    assert_eq!(files(dir.path(), "checkpoint").len(), 1);
}

/// A server starts from the log after a cut: the restore of the book.
#[tokio::test]
async fn a_server_loads_the_log_after_a_cut() {
    let dir = store(&[2]).await;
    let (ok, _, stderr) = tool(dir.path(), &["log", "cut", "--after", "4"]);
    assert!(ok, "{stderr}");
    let store = Dir::new(dir.path());
    let found = checkpoint::load(&store, "0.8.0").await.unwrap();
    assert_eq!(found.checkpoint.unwrap().state.position, 2);
    let replayed = log::replay_after(&store, 2).await.unwrap();
    assert_eq!(replayed.last, 4);
    let positions: Vec<u64> = replayed.records.iter().map(|r| r.position).collect();
    assert_eq!(positions, [3, 4]);
}

/// A record with a position lower than the record before it: `verify`
/// names the cut, and the cut removes each record that it names.
#[tokio::test]
async fn log_cut_repairs_a_chunk_with_a_record_of_a_lower_position() {
    let dir = store(&[]).await;
    let chunk = dir.path().join(chunk_name(5));
    let line = |record: &Record| serde_json::to_string(record).unwrap();
    let text = format!(
        "{{\"format\":1,\"first\":5}}\n{}\n{}\n{}\n",
        line(&running(5)),
        line(&claimed(6)),
        line(&running(3))
    );
    fs::write(&chunk, text).unwrap();

    let (ok, stdout, _) = tool(dir.path(), &["log", "verify"]);
    assert!(!ok);
    assert!(
        stdout.contains("line 4: a record has position 3, and the log needs 7"),
        "{stdout}"
    );
    assert!(stdout.contains("riff-server log cut --after 6"), "{stdout}");

    let (ok, stdout, stderr) = tool(dir.path(), &["log", "cut", "--after", "4"]);
    assert!(ok, "{stderr}");
    assert!(
        stdout.contains("Removed 3 records and 0 checkpoints after position 4."),
        "{stdout}"
    );
    assert!(!chunk.exists(), "no record after the position stays");
    let (ok, stdout, _) = tool(dir.path(), &["log", "verify"]);
    assert!(ok, "{stdout}");
    assert_eq!(
        stdout,
        "The log reads: 2 chunks, 4 records from position 1 to 4, 0 checkpoints.\n"
    );
}
