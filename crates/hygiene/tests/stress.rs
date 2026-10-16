//! `just stress`: runs a test many times on a busy machine
//! (01M517P9QE2SQ45SPV3K2SM71B). The tests run `deploy/stress.sh` of
//! this repository with a fake `cargo` first on the `PATH`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A fake `cargo` in `dir`. It logs its arguments to `dir/cargo.log`.
/// A test run counts in `dir/count`, and fails when its number is in
/// `fails`.
fn fake_cargo(dir: &Path, fails: &[u32]) {
    let fails: Vec<String> = fails.iter().map(u32::to_string).collect();
    isolated::write_program(
        dir.join("cargo"),
        format!(
            "#!/bin/sh\nd=$(dirname \"$0\")\necho \"$*\" >> \"$d/cargo.log\"\n\
             case \" $* \" in *' --no-run '*) exit 0 ;; esac\n\
             n=$(($(cat \"$d/count\" 2>/dev/null || echo 0) + 1))\necho $n > \"$d/count\"\n\
             echo \"the output of run $n\"\n\
             case \" {} \" in *\" $n \"*) exit 101 ;; esac\n",
            fails.join(" ")
        ),
    );
}

/// Runs `deploy/stress.sh ARGS` with the fake `cargo` of `dir`, 5 runs
/// and 2 busy loops. The output closes only when each busy loop has
/// ended, so the wait for it checks that the script stops them.
fn stress(dir: &Path, args: &[&str]) -> Output {
    let path = format!("{}:{}", dir.display(), std::env::var("PATH").unwrap());
    let mut child = Command::new(top().join("deploy/stress.sh"))
        .args(args)
        .current_dir(dir)
        .env("PATH", path)
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .env("STRESS_RUNS", "5")
        .env("STRESS_LOAD", "2")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (mut out, mut err) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let (done, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        out.read_to_end(&mut stdout).unwrap();
        err.read_to_end(&mut stderr).unwrap();
        done.send((stdout, stderr)).unwrap();
    });
    let status = child.wait().unwrap();
    let span = isolated::Span::start();
    loop {
        if let Ok((stdout, stderr)) = wait.recv_timeout(Duration::from_millis(100)) {
            return Output {
                status,
                stdout,
                stderr,
            };
        }
        assert!(
            span.within(Duration::from_secs(10)),
            "a busy loop still runs after the end of the script"
        );
    }
}

#[test]
fn stress_runs_the_test_under_load_and_counts_the_fails() {
    let dir = tempfile::tempdir().unwrap();
    fake_cargo(dir.path(), &[2, 4]);
    let out = stress(dir.path(), &["-p", "riff", "--test", "all", "hosts::"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    let logs = dir.path().join("target/stress");
    let log = |n: u32| logs.join(format!("run-{n}.log"));
    let lines: Vec<String> = (1..=5)
        .map(|n| match n {
            2 | 4 => format!("run {n} of 5: FAIL, see {}", log(n).display()),
            _ => format!("run {n} of 5: pass"),
        })
        .chain([
            "stress: 3 of 5 runs passed with 2 busy loops: cargo test -p riff --test all hosts::"
                .to_owned(),
        ])
        .collect();
    assert_eq!(stdout.lines().collect::<Vec<_>>(), lines, "{stdout}");
    // Only the output of a failed run stays.
    assert_eq!(
        std::fs::read_to_string(log(2)).unwrap(),
        "the output of run 2\n"
    );
    assert!(log(4).exists() && !log(1).exists() && !log(5).exists());
    // One build first, then the runs.
    let calls = std::fs::read_to_string(dir.path().join("cargo.log")).unwrap();
    let mut want = vec!["test -q --no-run -p riff --test all hosts::"];
    want.extend(["test -q -p riff --test all hosts::"; 5]);
    assert_eq!(calls.lines().collect::<Vec<_>>(), want);
}

#[test]
fn stress_passes_when_each_run_passes_and_removes_old_logs() {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("target/stress");
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(logs.join("run-9.log"), "old").unwrap();
    fake_cargo(dir.path(), &[]);
    let out = stress(dir.path(), &["chat::"]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.ends_with("stress: 5 of 5 runs passed with 2 busy loops: cargo test chat::\n"),
        "{stdout}"
    );
    assert_eq!(std::fs::read_dir(&logs).unwrap().count(), 0);
}

#[test]
fn stress_with_no_test_shows_the_usage() {
    let dir = tempfile::tempdir().unwrap();
    fake_cargo(dir.path(), &[]);
    let out = stress(dir.path(), &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).starts_with("usage: just stress ARGS"),
        "{}",
        text(&out.stderr)
    );
    assert!(!dir.path().join("cargo.log").exists());
}

/// The recipe runs the script in the environment of the tests.
#[test]
fn the_recipe_runs_the_script_as_just_test_runs_cargo_test() {
    let justfile = std::fs::read_to_string(top().join("justfile")).unwrap();
    let env = "RIFF_SERVER=http://127.0.0.1:9 \
               DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent/riff-test-bus";
    let recipe = format!(
        "\nstress *ARGS:\n    cargo build -q --workspace --bins\n    {env} deploy/stress.sh {{{{ARGS}}}}\n"
    );
    assert!(justfile.contains(&recipe), "{recipe}");
    assert!(justfile.contains(&format!("    {env} cargo test {{{{ARGS}}}}\n")));
}
