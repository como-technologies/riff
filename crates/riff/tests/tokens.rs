//! `riff tokens` on a hand-made transcript fixture. The session `a6cf`
//! starts in the folder of the main worktree and goes on in the folder
//! of a worktree. The #60 script gives the same numbers for it.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/projects")
}

fn main_folder() -> PathBuf {
    fixtures().join("-home-mike-src-owner-repo")
}

fn a6cf() -> [PathBuf; 2] {
    [
        main_folder().join("a6cf.jsonl"),
        fixtures().join("-home-mike-src-owner-repo--claude-worktrees-issue-1/a6cf.jsonl"),
    ]
}

fn riff() -> Command {
    Command::cargo_bin("riff").unwrap()
}

fn json(cmd: &mut Command) -> Value {
    let out = cmd
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&out).unwrap()
}

/// Each number of 01M3JCFE67QMVGZHPWRGJ9Y83D to 01M3JCFEGZWQ1SC76VF7DA6HX9.
#[test]
fn each_number_of_a_session_in_two_folders() {
    let report = json(riff().arg("tokens").args(a6cf()));
    let sessions = report["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1, "the two files are one session");
    let m = &sessions[0];
    let n = |k: &str| m[k].as_u64().unwrap();
    assert_eq!(m["session"], "a6cf");
    assert_eq!(n("requests"), 6);
    assert_eq!(n("input"), 80_700);
    assert_eq!(n("output"), 120);
    assert_eq!(n("mean_context"), 13_450);
    assert_eq!((n("only_riff_requests"), n("only_riff_input")), (2, 26_500));
    assert_eq!(
        (n("riff_and_other_requests"), n("riff_and_other_input")),
        (1, 13_000)
    );
    assert_eq!(n("riff_in_context"), 1_693);
    assert_eq!(n("skill"), 122);
    assert_eq!(n("wakes"), 2);
    assert_eq!(n("wakes_that_start_a_turn"), 1);
    assert_eq!(n("wakes_in_a_turn"), 1);
    assert_eq!((n("wake_turn_requests"), n("wake_turn_input")), (2, 29_600));
    assert_eq!(n("compactions"), 0);
    assert_eq!(
        m["riff_calls"],
        serde_json::json!({"claim": 1, "read": 2, "riff watch": 1})
    );
    assert_eq!(
        m["riff_results"],
        serde_json::json!({
            "claim": {"calls": 1, "tokens": 7},
            "read": {"calls": 2, "tokens": 193},
            "watch": {"calls": 1, "tokens": 10},
        })
    );
}

/// The text shows the same numbers as the JSON (01M3JCFEK6Q4YXXJ0ERMJ2KS65).
#[test]
fn the_text_shows_the_numbers() {
    let out = riff()
        .arg("tokens")
        .args(a6cf())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).unwrap();
    for line in [
        "== a6cf\n",
        "requests 6, input 80,700 tokens (mean context 13,450), output 120\n",
        "requests that call only riff: 2, 26,500 input tokens (32.8% of input)\n",
        "requests that call riff and other tools: 1, 13,000 input tokens (16.1% of input)\n",
        "riff in context: 1,693 input tokens (2.1% of input)\n",
        "riff skill: 122 tokens\n",
        "wakes 2 (1 started a turn, 1 came in a turn); the turns that a wake started: \
             2 requests, 29,600 input tokens (36.7% of input)\n",
        "riff calls: claim 1, read 2, riff watch 1\n",
        "riff tool results: 210 tokens: claim 1 (7 tokens), read 2 (193 tokens), watch 1 (10 tokens)\n",
    ] {
        assert!(out.contains(line), "{line}\nnot in:\n{out}");
    }
    assert!(!out.contains("== total"), "one session has no total");
}

/// Only the requests in the window count (01M3JCFENCPP20TJRYS505YER6).
#[test]
fn since_and_until_limit_the_count() {
    let report = json(riff().arg("tokens").args(a6cf()).args([
        "--since",
        "2026-09-27T11:00:00Z",
        "--until",
        "2026-09-27T11:15:00Z",
    ]));
    let m = &report["sessions"][0];
    assert_eq!(m["requests"], 2, "r4 and r5");
    assert_eq!(m["input"], 29_600);
    assert_eq!(m["wakes"], 2);
    assert_eq!(m["riff_calls"], serde_json::json!({"read": 1}));
    // The riff text from before the window is still in the context.
    assert!(m["riff_in_context"].as_u64().unwrap() > 400);
}

#[test]
fn a_bad_time_is_refused() {
    riff()
        .args(["tokens", "--since", "yesterday"])
        .args(a6cf())
        .assert()
        .failure();
}

/// With no file, the sessions of the repository: the folder of the main
/// worktree and of each of its worktrees (01M3JCFE44P47XAZ1STAM5M6JV).
#[test]
fn with_no_file_it_reads_each_session_of_the_repository() {
    let home = tempfile::tempdir().unwrap();
    let repo = home.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let init = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status()
        .unwrap();
    assert!(init.success());
    let repo = repo.canonicalize().unwrap();
    let projects = home.path().join("projects");
    let base = riff::tokens::project_name(&repo);
    let copy = |folder: &str, from: &Path| {
        let dir = projects.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(from, dir.join(from.file_name().unwrap())).unwrap();
    };
    let [a_main, a_worktree] = a6cf();
    copy(&base, &a_main);
    copy(&base, &main_folder().join("b7d0.jsonl"));
    copy(&format!("{base}--claude-worktrees-issue-1"), &a_worktree);
    // A repository whose name starts with the same name is not in it.
    copy(
        &format!("{base}-server"),
        &fixtures().join("-home-mike-src-owner-other/c8e1.jsonl"),
    );
    let mut cmd = riff();
    cmd.current_dir(&repo)
        .arg("tokens")
        .arg("--projects")
        .arg(&projects);
    let report = json(&mut cmd);
    let ids: Vec<&str> = report["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["session"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["b7d0", "a6cf"], "in the order that they started");
    assert_eq!(report["total"]["requests"], 7);
    assert_eq!(report["total"]["input"], 81_100);

    let mut cmd = riff();
    cmd.current_dir(&repo)
        .args(["tokens", "--session", "a6"])
        .arg("--projects")
        .arg(&projects);
    let report = json(&mut cmd);
    assert_eq!(report["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(report["sessions"][0]["session"], "a6cf");
}
