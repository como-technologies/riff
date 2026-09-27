//! After `/clear`, each riff process of a session uses the session ID of
//! `riff mcp` (R167, R168), and one watch runs for each session (R169).
//!
//! This test process stands in for Claude Code: it is the parent of each
//! riff process. `CLAUDE_CODE_SESSION_ID` is `old` for `riff mcp`, and
//! `new` for each process that starts after `/clear`.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use riff::local;

/// A riff command with its own local directory, and no server.
fn riff(run: &Path, session: &str, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_riff"));
    cmd.args(args)
        .env("XDG_RUNTIME_DIR", run)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SERVER", "http://127.0.0.1:9")
        .env_remove("RIFF_SESSION")
        .env("CLAUDE_CODE_SESSION_ID", session)
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The context of the start hook for `/clear`, after `/clear` gave the
/// session the ID `new`.
fn clear_context(run: &Path) -> String {
    let mut hook = riff(run, "new", &["hook", "session-start"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(
        hook.stdin.as_mut().unwrap(),
        br#"{"session_id":"new","source":"clear","hook_event_name":"SessionStart"}"#,
    )
    .unwrap();
    let out = hook.wait_with_output().unwrap();
    assert!(out.status.success());
    let out: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    out["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Waits until `done` is true, for at most 20 seconds.
fn wait_for(what: &str, done: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < end, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Stops `child` and waits for its end.
fn stop(mut child: Child) {
    child.kill().unwrap();
    child.wait().unwrap();
}

fn files(run: &Path) -> PathBuf {
    run.join("riff")
}

#[test]
fn after_clear_each_process_uses_the_id_of_riff_mcp() {
    let run = tempfile::tempdir().unwrap();
    let mcp = riff(run.path(), "old", &["mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let me = std::process::id();
    wait_for("riff mcp records its session", || {
        local::recorded(&files(run.path()), me).is_some()
    });

    let whoami = riff(run.path(), "new", &["whoami"]).output().unwrap();
    assert!(stdout(&whoami).contains("session=old"), "{whoami:?}");

    let context = clear_context(run.path());
    assert!(context.contains("session=old"), "{context}");
    assert!(!context.contains("session=new"), "{context}");
    assert!(context.contains("claims stay"), "{context}");
    assert!(context.contains("Now run `riff watch`"), "{context}");

    // A watch of the old ID runs from before /clear.
    let watch = local::watch(&files(run.path()), "old").unwrap().unwrap();
    let context = clear_context(run.path());
    assert!(context.contains("Keep it."), "{context}");
    assert!(!context.contains("Now run"), "{context}");

    // A new watch after /clear finds the watch of the old ID.
    let second = riff(run.path(), "new", &["watch"]).output().unwrap();
    assert_eq!(second.status.code(), Some(1));
    assert_eq!(stdout(&second), format!("{}\n", riff::text::WATCH_RUNS));
    drop(watch);

    // Without a live riff mcp, the ID of the agent tool counts.
    stop(mcp);
    let whoami = riff(run.path(), "new", &["whoami"]).output().unwrap();
    assert!(stdout(&whoami).contains("session=new"), "{whoami:?}");
}

#[test]
fn one_watch_runs_for_each_session() {
    let run = tempfile::tempdir().unwrap();
    // No server listens, so the first watch keeps trying to connect.
    let first = riff(run.path(), "a6cf", &["watch"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    wait_for("the first watch takes its lock", || {
        local::watching(&files(run.path()), "a6cf")
    });

    let second = riff(run.path(), "a6cf", &["watch"]).output().unwrap();
    assert_eq!(second.status.code(), Some(1));
    assert_eq!(stdout(&second), format!("{}\n", riff::text::WATCH_RUNS));
    assert!(stdout(&second).contains("Do not start the watch again now."));

    stop(first);
    assert!(!local::watching(&files(run.path()), "a6cf"));
}

#[test]
fn a_hook_with_riff_session_uses_it() {
    let run = tempfile::tempdir().unwrap();
    let context = {
        let mut cmd = riff(run.path(), "new", &["hook", "session-start"]);
        let mut hook = cmd
            .env("RIFF_SESSION", "mine")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        std::io::Write::write_all(
            hook.stdin.as_mut().unwrap(),
            br#"{"session_id":"new","source":"startup"}"#,
        )
        .unwrap();
        stdout(&hook.wait_with_output().unwrap())
    };
    assert!(context.contains("session=mine"), "{context}");
}
