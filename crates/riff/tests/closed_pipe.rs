//! A command of `riff` ends quietly when the reader of its stdout closed
//! the pipe, as in `riff daemon status | head -1`
//! (01M5MRG35BYRGZYD4VQVV8ZTYX, #1264).

use std::process::{Command, Output};

use isolated::Isolated;

/// Runs `cmd` with a stdout pipe whose reader is closed before the start.
fn into_a_closed_pipe(mut cmd: Command) -> Output {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    cmd.stdout(writer).output().unwrap()
}

/// `riff usage` prints its line into the closed pipe: it ends with the
/// exit 0 and prints no panic.
#[test]
fn riff_usage_into_a_closed_pipe_ends_quietly() {
    let env = Isolated::new();
    let mut cmd = env.riff();
    cmd.arg("usage");
    let out = into_a_closed_pipe(cmd);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{:?} {stderr}", out.status);
    assert!(!stderr.contains("panicked"), "{stderr}");
}

/// `riff daemon status` with no daemon prints the state of the unit. Into
/// the closed pipe, it ends at its first line with the exit 0, and
/// prints no panic.
#[test]
fn riff_daemon_status_into_a_closed_pipe_ends_quietly() {
    let env = Isolated::new();
    let mut cmd = env.riff();
    cmd.args(["daemon", "status"]);
    let out = into_a_closed_pipe(cmd);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{:?} {stderr}", out.status);
    assert!(!stderr.contains("panicked"), "{stderr}");
}

/// A panic that is not a closed pipe still shows: the hook passes it on.
/// `riff usage` with a stdout that is full is such a case: `/dev/full`
/// fails each write with ENOSPC.
#[test]
fn another_error_of_a_print_still_panics() {
    let env = Isolated::new();
    let full = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .unwrap();
    let out = env.riff().arg("usage").stdout(full).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{:?} {stderr}", out.status);
    assert!(stderr.contains("panicked"), "{stderr}");
    assert!(stderr.contains("No space left on device"), "{stderr}");
}
