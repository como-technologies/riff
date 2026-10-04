//! The lock of `just ci` and `just ci-full`: one run at a time in a
//! worktree (01M43DKYVAX0TJ2F5YYGYFSZ4G). The tests run the real
//! justfile of this repository in a temporary directory, so the lock is
//! in that directory and not in the `target` of this worktree.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The start time of process `pid`, in the form of `ci-lock.sh`.
fn start(pid: u32) -> String {
    let out = Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    text(&out.stdout)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Writes the lock of `pid` in `dir/target`, as a run that started
/// `ago` seconds before.
fn lock(dir: &Path, pid: u32, start: &str, ago: u64) -> PathBuf {
    let target = dir.join("target");
    std::fs::create_dir_all(&target).unwrap();
    let lock = target.join(".riff-ci.lock");
    std::fs::write(&lock, format!("{pid}\n{start}\n{}\n", now() - ago)).unwrap();
    lock
}

/// A process that runs, as the first `just ci`.
fn first_run() -> Child {
    Command::new("sleep").arg("60").spawn().unwrap()
}

/// `just RECIPE` of this repository, with `dir` as its directory.
fn just(dir: &Path, recipe: &str) -> Output {
    Command::new("just")
        .arg("--justfile")
        .arg(top().join("justfile"))
        .arg("--working-directory")
        .arg(dir)
        .arg(recipe)
        .env_remove("RIFF_CI_LOCK")
        .output()
        .unwrap()
}

/// Runs `script` in bash in `dir`, after it sources `ci-lock.sh`.
fn bash(dir: &Path, script: &str) -> Output {
    let lib = top().join("crates/hygiene/ci-lock.sh");
    Command::new("bash")
        .arg("-c")
        .arg(format!(
            "set -euo pipefail; . '{}'; {script}",
            lib.display()
        ))
        .current_dir(dir)
        .env_remove("RIFF_CI_LOCK")
        .output()
        .unwrap()
}

#[test]
fn a_second_run_in_a_worktree_with_a_live_lock_stops_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut first = first_run();
    let pid = first.id();
    let lock = lock(dir.path(), pid, &start(pid), 3 * 60);
    let held = std::fs::read_to_string(&lock).unwrap();
    let line = format!(
        "a just ci runs in this worktree already (pid {pid}, started 3 min ago); \
         wait for it, or stop it"
    );
    for recipe in ["ci", "ci-full"] {
        let at = Instant::now();
        let out = just(dir.path(), recipe);
        let took = at.elapsed();
        let stderr = text(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{recipe}: {stderr}");
        assert_eq!(stderr.lines().next(), Some(line.as_str()), "{recipe}");
        assert!(took < Duration::from_secs(10), "{recipe} took {took:?}");
        assert_eq!(std::fs::read_to_string(&lock).unwrap(), held, "{recipe}");
    }
    first.kill().unwrap();
    first.wait().unwrap();
}

#[test]
fn a_lock_of_a_dead_pid_lets_the_run_go_on() {
    let dir = tempfile::tempdir().unwrap();
    let mut gone = Command::new("true").spawn().unwrap();
    let pid = gone.id();
    let start = start(pid);
    gone.wait().unwrap();
    lock(dir.path(), pid, &start, 60);
    let out = bash(
        dir.path(),
        "ci_lock \"$PWD/target\"; echo \"$$\"; cat target/.riff-ci.lock",
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], lines[1], "the run holds the lock: {stdout}");
    assert!(
        !dir.path().join("target/.riff-ci.lock").exists(),
        "the run removes its lock at the exit"
    );
}

#[test]
fn a_lock_of_a_pid_that_another_process_got_again_does_not_count() {
    let dir = tempfile::tempdir().unwrap();
    let mut other = first_run();
    lock(dir.path(), other.id(), "Thu Jan 1 00:00:00 1970", 60);
    let out = bash(dir.path(), "ci_lock \"$PWD/target\"; echo took");
    other.kill().ok();
    other.wait().ok();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout).trim(), "took");
}

#[test]
fn a_run_that_the_holder_starts_takes_no_lock() {
    let dir = tempfile::tempdir().unwrap();
    let out = bash(
        dir.path(),
        "ci_lock \"$PWD/target\"; echo \"$RIFF_CI_LOCK\"; \
         ( ci_lock \"$PWD/target\"; echo inner ); echo \"$$\"",
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, [lines[2], "inner", lines[2]], "{stdout}");
}
