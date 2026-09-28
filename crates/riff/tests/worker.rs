//! How a worker ends, and what its lead sees
//! (01M3JQC8ANFYYEXSHBS2DCZYBX to 01M3JQC8GVFWC47NTN4NKE730P). A fake
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
        .env_remove("RIFF_WORKER")
        .env_remove("RIFF_WORKER_PID");
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

/// The worker gets `RIFF_WORKER=1` and the process ID of its wrapper.
#[tokio::test(flavor = "multi_thread")]
async fn the_wrapper_marks_claude_as_a_worker() {
    let api = start_server().await;
    lead(&api).await;
    let dir = repo();
    let seen = dir.path().join("seen");
    let claude = fake_claude(
        dir.path(),
        &format!(
            "echo \"$RIFF_WORKER $RIFF_WORKER_PID\" > '{}'",
            seen.display()
        ),
    );
    let child = riff(&api, dir.path(), "w1")
        .args(["workers", "run"])
        .arg(&claude)
        .spawn()
        .unwrap();
    let pid = child.id();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        std::fs::read_to_string(seen).unwrap().trim(),
        format!("1 {pid}")
    );
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

/// A worker with no work runs `riff workers done`. It tells the lead,
/// then ends: it leaves `riff who` within 10 seconds, and its wrapper
/// exits.
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_with_no_work_tells_the_lead_and_ends() {
    let api = start_server().await;
    let lead = lead(&api).await;
    let dir = repo();
    let claude = fake_claude(
        dir.path(),
        "\"$RIFF_BIN\" status 'looking for work' >/dev/null\n\
         \"$RIFF_BIN\" workers done\n\
         sleep 30",
    );
    let begin = Instant::now();
    let out = riff(&api, dir.path(), "w2")
        .args(["workers", "run"])
        .arg(&claude)
        .output()
        .unwrap();
    assert!(begin.elapsed() < Duration::from_secs(10), "{out:?}");
    assert_eq!(out.status.code(), Some(0), "{out:?}");

    let read = lead_reads(&api, &lead).await;
    assert!(
        read.contains(
            "worker done: I hold no claim and find no free item. I end now, and my pane %5 closes."
        ),
        "{read}"
    );
    // Only the message of done: the wrapper sends no crash message.
    assert!(!read.contains("worker stopped"), "{read}");
    let who = api.who(&lead, false).await.unwrap();
    assert!(
        who.iter().all(|s| s.uri.who().session() != Some("w2")),
        "{who:?}"
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

/// A worker that holds a claim, for example while it waits for a
/// verify, does not end: `riff workers done` refuses and does nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_that_holds_a_claim_does_not_end() {
    let api = start_server().await;
    let lead = lead(&api).await;
    api.set_riff(&lead, riff_core::wire::RiffState::Running)
        .await
        .unwrap();
    lead_reads(&api, &lead).await;
    let dir = repo();
    let d = dir.path().display();
    let claude = fake_claude(
        dir.path(),
        &format!(
            "\"$RIFF_BIN\" claim issue-12 >/dev/null\n\
             \"$RIFF_BIN\" workers done 2> '{d}/done.err'\n\
             echo $? > '{d}/done.code'"
        ),
    );
    let out = riff(&api, dir.path(), "w6")
        .args(["workers", "run"])
        .arg(&claude)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let code = std::fs::read_to_string(dir.path().join("done.code")).unwrap();
    assert_ne!(code.trim(), "0");
    let err = std::fs::read_to_string(dir.path().join("done.err")).unwrap();
    assert!(err.contains("you still hold issue-12"), "{err}");

    // No done message, and the worker is still in `riff who` with its claim.
    let read = lead_reads(&api, &lead).await;
    assert!(!read.contains("worker done"), "{read}");
    let who = api.who(&lead, false).await.unwrap();
    let w6 = who
        .iter()
        .find(|s| s.uri.who().session() == Some("w6"))
        .expect("w6 stays in riff who");
    assert_eq!(w6.uri.claims(), ["issue-12"]);
}

/// `riff workers done` outside a worker refuses and ends nothing.
#[tokio::test(flavor = "multi_thread")]
async fn done_outside_a_worker_refuses() {
    let api = start_server().await;
    lead(&api).await;
    let dir = repo();
    let out = riff(&api, dir.path(), "w4")
        .args(["workers", "done"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not a worker"), "{stderr}");
}

/// The start hook tells a session with `RIFF_WORKER=1` that it is a
/// worker, what to do with no work, and to wait for a verify.
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
    assert!(worker.contains("riff workers done"), "{worker}");
    assert!(
        worker.contains("While you wait for a verify, keep your claim and wait. Do not end."),
        "{worker}"
    );
    assert!(!context(false).contains("You are a worker"));
}
