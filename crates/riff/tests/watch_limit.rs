//! `riff watch --once` ends by itself when no wake came in the time of
//! `watch.limit` (01M3Z64J08GW6N1H42AR2FZQZ4), against a real server.
//! The session starts the watch again, loses no wake, and stays live in
//! `riff who`.

use isolated::Isolated;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const THREAD: &str = "como-technologies/riff";

/// The longest time that a test waits for a command.
const WAIT: Duration = Duration::from_secs(20);

async fn start_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    format!("http://{addr}")
}

/// A git repository with a GitHub origin, so the place of each session
/// is `como-technologies/riff`.
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
        let status = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status();
        assert!(status.unwrap().success());
    }
    dir
}

/// `riff ARGS` in `dir`, as the agent session `session` of mike. The
/// settings of riff are in `dir`.
fn riff(server: &str, dir: &Path, session: &str, args: &[&str]) -> Command {
    let mut cmd = Isolated::shared().riff();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", session)
        .env("RIFF_HOME", dir)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    cmd
}

/// The output of `cmd`. The command must end in [`WAIT`].
async fn run(mut cmd: Command) -> Output {
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap());
    tokio::time::timeout(WAIT, out)
        .await
        .expect("the command did not end")
        .unwrap()
}

/// The stdout of `cmd`, which must exit with status 0.
async fn stdout(cmd: Command) -> String {
    let out = run(cmd).await;
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// The stdout of `child` after its end. The child must end in [`WAIT`]
/// with status 0.
async fn end(child: Child) -> String {
    let out = tokio::task::spawn_blocking(move || child.wait_with_output().unwrap());
    let out = tokio::time::timeout(WAIT, out)
        .await
        .expect("the watch did not end")
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// True when `child` still runs after `time`.
async fn runs_after(child: &mut Child, time: Duration) -> bool {
    tokio::time::sleep(time).await;
    child.try_wait().unwrap().is_none()
}

fn no_wake(secs: u64) -> String {
    format!("{}\n", riff::text::watch_no_wake(Duration::from_secs(secs)))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_watch_that_ends_with_no_wake_loses_no_wake() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let post = |body: &'static str| {
        let args = ["post", "-t", THREAD, "--to", "session=b2", body];
        stdout(riff(&server, dir, "a1", &args))
    };

    let set = stdout(riff(&server, dir, "b2", &["watch", "limit", "1"])).await;
    assert!(set.starts_with("watch.limit  1  ("), "{set}");

    // No wake comes: the watch ends by itself with one line and status 0.
    let start = Instant::now();
    let first = stdout(riff(&server, dir, "b2", &["watch", "--once"])).await;
    assert_eq!(first, no_wake(1));
    assert!(start.elapsed() >= Duration::from_secs(1));

    // A message that comes while no watch runs wakes the new watch.
    post("in the gap").await;
    let second = stdout(riff(&server, dir, "b2", &["watch", "--once"])).await;
    assert_eq!(second.lines().count(), 1, "{second}");
    assert!(second.contains("(message 1)"), "{second}");
    stdout(riff(&server, dir, "b2", &["read"])).await;

    // The new watch runs: the session is live, and the next wake comes.
    stdout(riff(&server, dir, "b2", &["watch", "limit", "600"])).await;
    let mut third = riff(&server, dir, "b2", &["watch", "--once"])
        .spawn()
        .unwrap();
    assert!(runs_after(&mut third, Duration::from_millis(1500)).await);
    let who = stdout(riff(&server, dir, "a1", &["who"])).await;
    let row = who
        .lines()
        .find(|line| line.contains("(b2)"))
        .unwrap_or_else(|| panic!("no row of b2: {who}"));
    assert!(!row.contains("offline"), "{who}");
    post("after the new start").await;
    let third = end(third).await;
    assert_eq!(third.lines().count(), 1, "{third}");
    assert!(third.contains("(message 2)"), "{third}");
}

/// No server listens, so each watch keeps trying to connect.
const NO_SERVER: &str = "http://127.0.0.1:9";

#[tokio::test(flavor = "multi_thread")]
async fn the_limit_is_only_for_a_watch_with_once() {
    let dir = repo();
    let dir = dir.path();
    stdout(riff(NO_SERVER, dir, "b2", &["watch", "limit", "1"])).await;

    let mut watch = riff(NO_SERVER, dir, "b2", &["watch"]).spawn().unwrap();

    assert!(runs_after(&mut watch, Duration::from_millis(2500)).await);
    watch.kill().unwrap();
    watch.wait().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_limit_0_is_no_limit() {
    let dir = repo();
    let dir = dir.path();
    let set = stdout(riff(NO_SERVER, dir, "b2", &["watch", "limit", "0"])).await;
    assert!(set.contains("waits with no limit"), "{set}");

    let mut watch = riff(NO_SERVER, dir, "b2", &["watch", "--once"])
        .spawn()
        .unwrap();

    assert!(runs_after(&mut watch, Duration::from_millis(1500)).await);
    watch.kill().unwrap();
    watch.wait().unwrap();
}

/// The watch that an update starts gets the end of the wait of the old
/// watch in `--until`: the update does not start the time again.
#[tokio::test(flavor = "multi_thread")]
async fn a_watch_after_an_update_keeps_the_end_of_the_wait() {
    let dir = repo();
    let dir = dir.path();
    stdout(riff(NO_SERVER, dir, "b2", &["watch", "limit", "600"])).await;

    let args = ["watch", "--once", "--until", "1"];
    let out = stdout(riff(NO_SERVER, dir, "b2", &args)).await;

    assert_eq!(out, no_wake(600));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_machine_has_the_default_limit() {
    let dir = repo();
    let dir = dir.path();

    let shown = stdout(riff(NO_SERVER, dir, "b2", &["watch", "limit"])).await;

    assert!(shown.starts_with("watch.limit  6000  ("), "{shown}");
    assert!(shown.contains("riff watch limit SECONDS"), "{shown}");
}
