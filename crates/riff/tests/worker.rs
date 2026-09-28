//! How a worker ends, how it waits with no work, and what its lead sees
//! (01M3JQC8ANFYYEXSHBS2DCZYBX, 01M3JQC8ETHRAWSJPHMKA062SQ,
//! 01M3K0AXMCVRST7HYH4DM8B3AN, 01M3K0AXRNA0F2920E9QCSDFQZ). A fake
//! `claude` runs in place of Claude Code.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use riff::api::Api;
use riff_core::name::SessionUri;

const LEAD: &str = "riff://mike@pangolin/como-technologies/riff?session=lead1";

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// A git repository with a GitHub origin, so the repository is
/// `como-technologies/riff`.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q"][..],
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/como-technologies/riff.git",
        ],
    ] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success();
        assert!(ok);
    }
    dir
}

/// A fake `claude` in `dir` that runs `script`, and counts its starts in
/// the file `starts`.
fn fake_claude(dir: &Path, script: &str) -> PathBuf {
    let path = dir.join("claude");
    let body = format!(
        "#!/bin/sh\necho start >> '{}'\n{script}\n",
        dir.join("starts").display()
    );
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// `riff` in `dir` as mike on pangolin, in the worker pane `%5` of the
/// worker session `session`.
fn riff(api: &Api, dir: &Path, session: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_riff"));
    cmd.current_dir(dir)
        .env("RIFF_SERVER", api.base())
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", session)
        .env("RIFF_BIN", env!("CARGO_BIN_EXE_riff"))
        .env("TMUX_PANE", "%5")
        .env("XDG_RUNTIME_DIR", dir)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("RIFF_WORKER");
    cmd
}

/// Registers the lead: the first session of mike in the repository.
async fn lead(api: &Api) -> SessionUri {
    let lead: SessionUri = LEAD.parse().unwrap();
    api.register(&lead).await.unwrap();
    lead
}

/// The unread text of the lead.
async fn lead_reads(api: &Api, lead: &SessionUri) -> String {
    riff::text::inbox(&api.inbox(lead, None, false).await.unwrap(), lead)
}

fn starts(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("starts"))
        .unwrap_or_default()
        .lines()
        .count()
}

/// `claude` exits with status 1. The lead gets a direct message with
/// the pane, the session ID and the exit code. The wrapper does not
/// start `claude` again.
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_that_exits_tells_the_lead() {
    let api = start_server().await;
    let lead = lead(&api).await;
    let dir = repo();
    let claude = fake_claude(dir.path(), "exit 1");
    let out = riff(&api, dir.path(), "w1")
        .args(["workers", "run"])
        .arg(&claude)
        .arg("Join the riff.")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_eq!(starts(dir.path()), 1);

    let read = lead_reads(&api, &lead).await;
    assert!(read.contains("direct with mike@pangolin"), "{read}");
    assert!(
        read.contains("worker stopped: pane %5, session w1, exit code 1."),
        "{read}"
    );
    // The crashed worker does not come back in `riff who`.
    let who = api.who(&lead, false).await.unwrap();
    assert!(
        who.iter().all(|s| s.uri.who().session() != Some("w1")),
        "{who:?}"
    );
}

/// The worker gets `RIFF_WORKER=1`.
#[tokio::test(flavor = "multi_thread")]
async fn the_wrapper_marks_claude_as_a_worker() {
    let api = start_server().await;
    lead(&api).await;
    let dir = repo();
    let seen = dir.path().join("seen");
    let claude = fake_claude(
        dir.path(),
        &format!("echo \"$RIFF_WORKER\" > '{}'", seen.display()),
    );
    let out = riff(&api, dir.path(), "w1")
        .args(["workers", "run"])
        .arg(&claude)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(std::fs::read_to_string(seen).unwrap().trim(), "1");
}

/// The wrapper gives `claude` each argument as it is, also the flag
/// settings that turn off Remote Control (01M3JV0ZNGKDFMRR9ACT0480V9).
#[tokio::test(flavor = "multi_thread")]
async fn the_wrapper_gives_claude_the_flag_settings() {
    let api = start_server().await;
    lead(&api).await;
    let dir = repo();
    let seen = dir.path().join("seen");
    let claude = fake_claude(
        dir.path(),
        &format!("printf '%s\\n' \"$@\" > '{}'", seen.display()),
    );
    let out = riff(&api, dir.path(), "w1")
        .args(["workers", "run"])
        .arg(&claude)
        .args(["--settings", riff::terminal::NO_REMOTE_CONTROL])
        .arg(riff::terminal::JOIN)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        std::fs::read_to_string(seen).unwrap(),
        "--settings\n{\"remoteControlAtStartup\":false}\nJoin the riff.\n"
    );
}

/// A worker with no work sets its status idle and keeps its watch. It
/// stays in `riff who`, and a request of the lead wakes it.
#[tokio::test(flavor = "multi_thread")]
async fn a_request_of_the_lead_wakes_an_idle_worker() {
    let api = start_server().await;
    let lead = lead(&api).await;
    let dir = repo();
    let status = riff(&api, dir.path(), "w2")
        .args(["status", riff::worker::IDLE])
        .env("RIFF_WORKER", "1")
        .output()
        .unwrap();
    assert!(status.status.success(), "{status:?}");
    let watch = riff(&api, dir.path(), "w2")
        .args(["watch", "--once"])
        .env("RIFF_WORKER", "1")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    // The idle worker is live in `riff who`, with its status.
    let begin = Instant::now();
    let idle = loop {
        let who = api.who(&lead, false).await.unwrap();
        if let Some(w2) = who
            .into_iter()
            .find(|s| s.uri.who().session() == Some("w2") && s.live)
        {
            break w2;
        }
        assert!(begin.elapsed() < Duration::from_secs(10), "no live w2");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(idle.status.unwrap().status.step, riff::worker::IDLE);

    api.tell(&lead, "w2", "request: claim issue-12")
        .await
        .unwrap();
    let out = tokio::task::spawn_blocking(move || watch.wait_with_output().unwrap())
        .await
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let line = String::from_utf8_lossy(&out.stdout);
    assert!(
        line.contains("riff: mike@pangolin:riff (lead1) wrote to you in a direct message"),
        "{line}"
    );
}

/// `riff workers stop` closes the pane: tmux sends SIGHUP. The wrapper
/// stops `claude` and sends no message.
#[tokio::test(flavor = "multi_thread")]
async fn a_hangup_stops_the_worker_with_no_message() {
    let api = start_server().await;
    let lead = lead(&api).await;
    let dir = repo();
    let claude = fake_claude(dir.path(), "sleep 30");
    let mut child = riff(&api, dir.path(), "w3")
        .args(["workers", "run"])
        .arg(&claude)
        .spawn()
        .unwrap();
    while starts(dir.path()) == 0 {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let hup = Command::new("kill")
        .args(["-HUP", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(hup.success());
    let begin = Instant::now();
    let status = child.wait().unwrap();
    assert!(begin.elapsed() < Duration::from_secs(10));
    assert_eq!(status.code(), Some(0));
    assert_eq!(lead_reads(&api, &lead).await, "No unread messages.");
}

/// The start hook tells a session with `RIFF_WORKER=1` that it is a
/// worker, to wait idle with no work, and to wait for a verify.
#[tokio::test(flavor = "multi_thread")]
async fn the_start_hook_tells_a_worker() {
    let api = start_server().await;
    let dir = repo();
    let context = |worker: bool| {
        let mut cmd = riff(&api, dir.path(), "w5");
        cmd.args(["hook", "session-start"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        if worker {
            cmd.env("RIFF_WORKER", "1");
        }
        let mut child = cmd.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(br#"{"session_id":"w5","source":"startup"}"#)
            .unwrap();
        String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
    };
    let worker = context(true);
    assert!(
        worker.contains("You are a worker (RIFF_WORKER=1)"),
        "{worker}"
    );
    assert!(!worker.contains("riff workers done"), "{worker}");
    assert!(
        worker.contains("set your status `idle: waits for work`, keep your watch running"),
        "{worker}"
    );
    assert!(
        worker.contains("While you wait for a verify, keep your claim and wait."),
        "{worker}"
    );
    assert!(!context(false).contains("You are a worker"));
}
