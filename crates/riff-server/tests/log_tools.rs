//! The tools of the log on a directory store: `riff-server log`
//! (01M3TJWHNYRCA7RTPFNYM5ZNQS), `riff-server log verify`
//! (01M3TJWHRP49M66NYNHWSYD3XP) and `riff-server log cut`
//! (01M3TJWHVN730ZWCWHT9ER186R). Each test runs the real binary.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use isolated::Isolated;
use riff_core::name::Who;
use riff_core::record::{By, Change, Claimed, Record, RiffStateSet};
use riff_core::wire::RiffState;
use riff_server::checkpoint::{self, Checkpoint};
use riff_server::log::{self, chunk_name};
use riff_server::state::State;
use riff_server::store::{Dir, LEASE};

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
    let (ok, stdout, stderr) = tool(dir.path(), &["log", "cut", "--after", "3", "--yes"]);
    assert!(ok, "{stderr}");
    assert!(stdout.contains("4  (a line that does not read"), "{stdout}");
    let (ok, stdout, _) = tool(dir.path(), &["log", "verify"]);
    assert!(ok, "{stdout}");
}

#[tokio::test]
async fn log_cut_removes_the_chunks_after_a_position_and_names_the_records() {
    let dir = store(&[2, 4]).await;
    let (ok, stdout, stderr) = tool(dir.path(), &["log", "cut", "--after", "3", "--yes"]);
    assert!(ok, "{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    let checkpoint = dir.path().join(checkpoint::name(4, 4));
    assert_eq!(
        lines,
        [
            "4  2026-09-21T14:13:20Z  claimed  issue-4 in acme/app by \
             riff://ann@heron/acme/app?session=s1  (claim, the session ann/s1)",
            "5  2026-09-21T14:13:20Z  riff_state_set  running  (cause not known)",
            "6  2026-09-21T14:13:20Z  claimed  issue-6 in acme/app by \
             riff://ann@heron/acme/app?session=s1  (claim, the session ann/s1)",
            &format!("checkpoint  {}", checkpoint.display()),
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
    let (ok, stdout, _) = tool(dir.path(), &["log", "cut", "--after", "3", "--yes"]);
    assert!(ok);
    assert_eq!(stdout, "Nothing is after position 3: removed nothing.\n");
}

#[tokio::test]
async fn log_cut_refuses_a_cut_before_the_oldest_kept_checkpoint() {
    let dir = store(&[4]).await;
    let (ok, stdout, stderr) = tool(dir.path(), &["log", "cut", "--after", "3", "--yes"]);
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
    let (ok, _, stderr) = tool(dir.path(), &["log", "cut", "--after", "4", "--yes"]);
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
    assert!(
        !stdout.contains("--yes"),
        "verify names the dry run: {stdout}"
    );

    let (ok, stdout, stderr) = tool(dir.path(), &["log", "cut", "--after", "4", "--yes"]);
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

/// Each `log cut` command in the `sh` blocks of the part of the
/// Development page from `heading` to `end`.
fn book_cuts(heading: &str, end: &str) -> Vec<String> {
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/src/development.md");
    let page = fs::read_to_string(page).unwrap();
    let part = &page[page.find(heading).unwrap()..];
    let part = &part[..part.find(end).unwrap()];
    let mut in_sh = false;
    let mut cuts = Vec::new();
    for line in part.lines() {
        if line.starts_with("```") {
            in_sh = line == "```sh";
        } else if in_sh && line.contains("log cut") {
            cuts.push(line.to_owned());
        }
    }
    cuts
}

/// The book shows the dry run and the cut with `--yes`, in "Cut the
/// log" and in "Go back to a position". `--help` names the flag.
#[test]
fn the_book_shows_the_dry_run_and_the_cut_with_yes() {
    let parts = [
        ("### Cut the log\n", "### Use the tools on the bucket\n"),
        (
            "### Go back to a position\n",
            "## Set up the cloud project\n",
        ),
    ];
    for (heading, end) in parts {
        let cuts = book_cuts(heading, end);
        assert!(cuts.len() >= 2, "{heading}: {cuts:?}");
        // The dry run comes first.
        assert!(!cuts[0].contains("--yes"), "{heading}: {cuts:?}");
        assert!(
            cuts.iter().any(|cut| cut.contains(" --yes ")),
            "{heading}: {cuts:?}"
        );
        for cut in &cuts {
            assert!(
                cut.starts_with("riff-server log cut --after "),
                "{heading}: {cut}"
            );
        }
    }
    let (ok, help, _) = tool(Path::new("d"), &["log", "cut", "--help"]);
    assert!(ok);
    assert!(help.contains("--yes"), "{help}");
    assert!(help.contains("--after <POSITION>"), "{help}");
}

/// The bytes of each file under `dir`, by its path.
fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut all = BTreeMap::new();
    for part in ["log", "checkpoint"] {
        for name in files(dir, part) {
            let bytes = fs::read(dir.join(&name)).unwrap();
            all.insert(name, bytes);
        }
    }
    all
}

/// 01M3X342G8KF2W06PABGXTERMZ: `log cut` with no `--yes` removes
/// nothing. It prints each record and each checkpoint that a cut
/// removes, and the command with `--yes`.
#[tokio::test]
async fn log_cut_with_no_yes_is_a_dry_run() {
    let dir = store(&[2, 4]).await;
    let before = snapshot(dir.path());
    let (ok, stdout, stderr) = tool(dir.path(), &["log", "cut", "--after", "3"]);
    assert!(ok, "{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    let checkpoint = dir.path().join(checkpoint::name(4, 4));
    assert_eq!(
        lines,
        [
            "4  2026-09-21T14:13:20Z  claimed  issue-4 in acme/app by \
             riff://ann@heron/acme/app?session=s1  (claim, the session ann/s1)",
            "5  2026-09-21T14:13:20Z  riff_state_set  running  (cause not known)",
            "6  2026-09-21T14:13:20Z  claimed  issue-6 in acme/app by \
             riff://ann@heron/acme/app?session=s1  (claim, the session ann/s1)",
            &format!("checkpoint  {}", checkpoint.display()),
            "A cut removes 3 records and 1 checkpoint after position 3. Threads: acme/app.",
            "This run removed nothing. To remove them, stop the server and run: \
             riff-server log cut --after 3 --yes",
        ]
    );
    assert_eq!(snapshot(dir.path()), before, "a dry run changes no file");

    // The whole log: `--after 0` on a store with no checkpoint.
    let dir = store(&[]).await;
    let before = snapshot(dir.path());
    let (ok, stdout, _) = tool(dir.path(), &["log", "cut", "--after", "0"]);
    assert!(ok);
    assert!(stdout.contains("A cut removes 6 records"), "{stdout}");
    assert_eq!(snapshot(dir.path()), before);

    // The same command with `--yes` removes what the dry run printed.
    let (ok, removed, stderr) = tool(dir.path(), &["log", "cut", "--after", "3", "--yes"]);
    assert!(ok, "{stderr}");
    assert!(removed.contains("Removed 3 records"), "{removed}");
    assert_ne!(snapshot(dir.path()), before);
}

/// 01M3X342K007K3Z9G0CYWFKVMA: `log cut --yes` refuses while a server
/// holds the lease, and names the instance. After the shutdown of the
/// server, it cuts.
#[tokio::test]
async fn log_cut_refuses_while_a_server_holds_the_lease() {
    let dir = store(&[]).await;
    let service = common::load_on(Arc::new(Dir::new(dir.path())))
        .await
        .unwrap();
    service.save().await.unwrap();
    let lease = fs::read(dir.path().join(LEASE)).unwrap();
    let lease: serde_json::Value = serde_json::from_slice(&lease).unwrap();
    let id = lease["id"].as_str().unwrap();
    let before = snapshot(dir.path());

    let cut = ["log", "cut", "--after", "4", "--yes"];
    let (ok, stdout, stderr) = tool(dir.path(), &cut);
    assert!(!ok, "{stdout}");
    assert_eq!(stdout, "");
    assert!(
        stderr.contains(&format!(
            "cannot cut: the server instance {id} holds the lease"
        )),
        "{stderr}"
    );
    assert!(stderr.contains("Stop the server first."), "{stderr}");
    assert_eq!(snapshot(dir.path()), before, "a refusal changes no file");

    // A dry run runs, and names the instance too.
    let (ok, stdout, stderr) = tool(dir.path(), &["log", "cut", "--after", "4"]);
    assert!(ok, "{stderr}");
    assert!(
        stdout.contains(&format!("the server instance {id} holds the lease")),
        "{stdout}"
    );
    assert_eq!(snapshot(dir.path()), before);

    // The shutdown ends the lease (01M3X342ARX5Y7R9ZJDT12R9A1).
    service.shutdown().await.unwrap();
    let (ok, stdout, stderr) = tool(dir.path(), &cut);
    assert!(ok, "{stderr}");
    assert!(stdout.contains("after position 4."), "{stdout}");
    let (ok, stdout, _) = tool(dir.path(), &["log", "verify"]);
    assert!(ok, "{stdout}");
    assert!(stdout.contains("from position 1 to 4"), "{stdout}");
}

/// A lease file of the instance `abc123`, with a time `age_ms` before
/// now.
fn lease_file(dir: &Path, age_ms: u64) {
    let at = riff_server::lease::now_ms() - age_ms;
    let json = format!(r#"{{"id":"abc123","renewed_at_ms":{at},"ended":false}}"#);
    fs::write(dir.join(LEASE), json).unwrap();
}

/// 01M3X342DH98YEZ3X5CND43DGD: a lease is live for 90 seconds after its
/// time. A server that stopped with no shutdown leaves such a lease.
#[tokio::test]
async fn log_cut_cuts_when_the_lease_is_90_seconds_old() {
    let dir = store(&[]).await;
    let cut = ["log", "cut", "--after", "4", "--yes"];
    lease_file(dir.path(), 60_000);
    let (ok, _, stderr) = tool(dir.path(), &cut);
    assert!(!ok);
    assert!(
        stderr.contains("cannot cut: the server instance abc123 holds the lease"),
        "{stderr}"
    );
    assert!(
        stderr.contains("or 90 seconds after its last write"),
        "{stderr}"
    );
    assert_eq!(files(dir.path(), "log").len(), 3);

    lease_file(dir.path(), 90_000);
    let (ok, stdout, stderr) = tool(dir.path(), &cut);
    assert!(ok, "{stderr}");
    assert!(
        stdout.contains("Removed 2 records and 0 checkpoints after position 4."),
        "{stdout}"
    );
}

/// The command that `log verify` names.
fn named_cut(stdout: &str) -> Vec<String> {
    let command = stdout.rsplit_once("run: riff-server ").unwrap().1.trim();
    command.split(' ').map(str::to_owned).collect()
}

/// Runs the cut that `log verify` named, with `--yes`.
fn run_named_cut(dir: &Path, verify_stdout: &str) -> String {
    let mut args = named_cut(verify_stdout);
    args.push("--yes".into());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, stdout, stderr) = tool(dir, &args);
    assert!(ok, "{stderr}");
    stdout
}

/// 01M3X342NQWZBJPS0GXV98BQME: a chunk that starts before the end of
/// the chunk before it. The cut that `verify` names repairs the log,
/// and keeps each record before the problem.
#[tokio::test]
async fn the_named_cut_repairs_a_chunk_that_starts_too_early() {
    let dir = tempfile::tempdir().unwrap();
    let line = |position| serde_json::to_string(&running(position)).unwrap() + "\n";
    fs::create_dir_all(dir.path().join("log")).unwrap();
    let first: String = (1..=4).map(line).collect();
    let second: String = (3..=5).map(line).collect();
    let chunk = |n: u64, lines: &str| {
        let text = format!("{{\"format\":1,\"first\":{n}}}\n{lines}");
        fs::write(dir.path().join(chunk_name(n)), text).unwrap();
    };
    chunk(1, &first);
    chunk(3, &second);
    let good = fs::read(dir.path().join(chunk_name(1))).unwrap();

    let (ok, stdout, _) = tool(dir.path(), &["log", "verify"]);
    assert!(!ok);
    assert!(
        stdout.contains("line 1: the chunk starts at position 3, and the log needs 5"),
        "{stdout}"
    );
    assert_eq!(named_cut(&stdout), ["log", "cut", "--after", "4"]);

    let removed = run_named_cut(dir.path(), &stdout);
    let lines: Vec<&str> = removed.lines().collect();
    assert_eq!(lines.len(), 4, "{removed}");
    assert!(
        lines[0].starts_with("3  ")
            && lines[0].ends_with("(a repeat: the record at position 3 stays)"),
        "{removed}"
    );
    assert!(lines[2].starts_with("5  ") && !lines[2].contains("a repeat"));
    assert_eq!(
        lines[3],
        "Removed 3 records and 0 checkpoints after position 4. Threads: none."
    );
    // The chunk before the problem has each of its bytes.
    assert_eq!(files(dir.path(), "log"), [chunk_name(1)]);
    assert_eq!(fs::read(dir.path().join(chunk_name(1))).unwrap(), good);
    let (ok, stdout, _) = tool(dir.path(), &["log", "verify"]);
    assert!(ok, "{stdout}");
    assert_eq!(
        stdout,
        "The log reads: 1 chunk, 4 records from position 1 to 4, 0 checkpoints.\n"
    );
    let replayed = log::replay(&Dir::new(dir.path())).await.unwrap();
    assert_eq!(replayed.last, 4);
}

/// 01M3X342NQWZBJPS0GXV98BQME: a line that is not UTF-8. `verify`
/// names that line only, and the cut that it names keeps each record
/// before the line, with its bytes.
#[tokio::test]
async fn the_named_cut_repairs_a_line_that_is_not_utf8() {
    // One chunk, with the positions 1 to 4. The record 3 is not UTF-8.
    let dir = tempfile::tempdir().unwrap();
    let line = |position| serde_json::to_string(&claimed(position)).unwrap() + "\n";
    let mut bytes = b"{\"format\":1,\"first\":1}\n".to_vec();
    bytes.extend(line(1).bytes());
    bytes.extend(line(2).bytes());
    let good = bytes.clone();
    bytes.extend(b"{\"position\":3,\"bad\":\"\xff\xfe\"}\n");
    bytes.extend(line(4).bytes());
    fs::create_dir_all(dir.path().join("log")).unwrap();
    let chunk = dir.path().join(chunk_name(1));
    fs::write(&chunk, bytes).unwrap();

    let (ok, stdout, _) = tool(dir.path(), &["log", "verify"]);
    assert!(!ok);
    assert!(stdout.contains("line 4: the line is not UTF-8"), "{stdout}");
    assert!(stdout.starts_with(&chunk.display().to_string()), "{stdout}");
    assert!(stdout.contains("1 problem in 1 chunk"), "{stdout}");
    // Not `--after 0`: that is the whole log.
    assert_eq!(named_cut(&stdout), ["log", "cut", "--after", "2"]);

    let removed = run_named_cut(dir.path(), &stdout);
    let lines: Vec<&str> = removed.lines().collect();
    assert_eq!(lines.len(), 3, "{removed}");
    assert!(
        lines[0].starts_with("3  (a line that does not read: the line is not UTF-8"),
        "{removed}"
    );
    assert!(lines[1].starts_with("4  "), "{removed}");
    assert_eq!(
        lines[2],
        "Removed 2 records and 0 checkpoints after position 2. Threads: acme/app."
    );
    assert_eq!(
        fs::read(&chunk).unwrap(),
        good,
        "the kept bytes are the same"
    );
    let (ok, stdout, _) = tool(dir.path(), &["log", "verify"]);
    assert!(ok, "{stdout}");
    let replayed = log::replay(&Dir::new(dir.path())).await.unwrap();
    assert_eq!(replayed.last, 2);
}
