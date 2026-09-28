//! A fresh context for a worker after each item
//! (01M3JQCCX22R4R4MN7XZPTS391 to 01M3JQCD16CNWN5FCQBRKHXYMP). A fake
//! `tmux` on `PATH` writes each call to a log.

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use riff::api::Api;
use riff::identity;
use riff_core::name::{SessionUri, Who};
use riff_core::wire::RiffState;

const FAKE_TMUX: &str = "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$(dirname \"$0\")/log\"\n";

struct Worker {
    fake: tempfile::TempDir,
    run: tempfile::TempDir,
    repo: tempfile::TempDir,
    server: String,
}

impl Worker {
    fn new(server: &str) -> Self {
        let fake = tempfile::tempdir().unwrap();
        let tmux = fake.path().join("tmux");
        std::fs::write(&tmux, FAKE_TMUX).unwrap();
        std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755)).unwrap();
        let repo = tempfile::tempdir().unwrap();
        let out = Command::new("git")
            .args(["init", "-q"])
            .arg(repo.path())
            .output()
            .unwrap();
        assert!(out.status.success());
        Worker {
            fake,
            run: tempfile::tempdir().unwrap(),
            repo,
            server: server.into(),
        }
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.fake.path().join("log")).unwrap_or_default()
    }

    /// A riff command of the session `id` in the pane `%3`. `worker`
    /// sets `RIFF_WORKER=1`.
    fn riff(&self, id: &str, worker: bool, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = Isolated::shared().riff();
        cmd.args(args)
            .current_dir(self.repo.path())
            .env("PATH", path)
            .env("RIFF_HOME", self.run.path())
            .env("RIFF_SERVER", &self.server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("RIFF_SESSION", id)
            .env("TMUX", "/tmp/tmux-1000/default,1,0")
            .env("TMUX_PANE", "%3")
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .env_remove("CLAUDE_CODE_SESSION_ID");
        if worker {
            cmd.env("RIFF_WORKER", "1");
        } else {
            cmd.env_remove("RIFF_WORKER");
        }
        cmd
    }

    fn next(&self, id: &str, worker: bool) -> Output {
        self.riff(id, worker, &["workers", "next"])
            .output()
            .unwrap()
    }

    /// The Stop hook of the session `id`.
    fn stop_hook(&self, id: &str) -> Output {
        let mut hook = self
            .riff(id, true, &["hook", "stop"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = format!(r#"{{"session_id":"{id}","hook_event_name":"Stop"}}"#);
        std::io::Write::write_all(hook.stdin.as_mut().unwrap(), input.as_bytes()).unwrap();
        hook.wait_with_output().unwrap()
    }

    fn uri(&self, id: &str) -> SessionUri {
        let place = identity::place_in(self.repo.path(), "pangolin").unwrap();
        SessionUri::new(Who::new("mike", Some(id)).unwrap(), place)
    }
}

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A riff with the lead `l1` and the worker `w1`.
async fn riff_with_a_worker() -> (Api, Worker) {
    let api = start_server().await;
    let w = Worker::new(api.base());
    let lead = w.uri("l1");
    api.register(&lead).await.unwrap();
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    api.register(&w.uri("w1")).await.unwrap();
    (api, w)
}

/// The path of a worker whose item is done: `riff workers next`, then
/// the turn ends, then riff types `/clear` and the start prompt into its
/// pane (01M3JQCCZ5M9VY3RGXWJYJN9Q9).
#[tokio::test(flavor = "multi_thread")]
async fn a_finished_worker_gets_clear_and_the_start_prompt() {
    let (_api, w) = riff_with_a_worker().await;
    let out = w.next("w1", true);
    assert!(out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("End your turn now"),
        "{out:?}"
    );
    assert!(w.run.path().join("state/next-w1").exists() || find_mark(w.run.path()));
    assert_eq!(w.log(), "", "nothing types before the turn ends");

    // The hook returns at once; the keys come after it.
    let start = Instant::now();
    let out = w.stop_hook("w1");
    assert!(out.status.success(), "{out:?}");
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "{:?}",
        start.elapsed()
    );
    assert!(!find_mark(w.run.path()), "the hook took the mark");

    let end = Instant::now() + Duration::from_secs(10);
    while w.log().lines().count() < 4 {
        assert!(Instant::now() < end, "{}", w.log());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        w.log().lines().collect::<Vec<_>>(),
        [
            "send-keys -t %3 -l /clear",
            "send-keys -t %3 Enter",
            "send-keys -t %3 -l Join the riff.",
            "send-keys -t %3 Enter",
        ]
    );

    // A second turn end with no request types nothing.
    let out = w.stop_hook("w1");
    assert!(out.status.success());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(w.log().lines().count(), 4);
}

/// True when a `next-*` file is anywhere under `dir`.
fn find_mark(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let path = e.path();
        if path.is_dir() {
            find_mark(&path)
        } else {
            e.file_name().to_string_lossy().starts_with("next-")
        }
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn only_a_worker_asks_for_a_fresh_context() {
    let (_api, w) = riff_with_a_worker().await;
    let out = w.next("w1", false);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(stderr(&out).contains("only a worker"), "{out:?}");
    assert!(!find_mark(w.run.path()));
}

/// riff never clears the lead: its user works in it.
#[tokio::test(flavor = "multi_thread")]
async fn the_lead_is_never_cleared() {
    let (_api, w) = riff_with_a_worker().await;
    let out = w.next("l1", true);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(stderr(&out).contains("this session is the lead"), "{out:?}");
    assert!(!find_mark(w.run.path()));
    assert!(w.stop_hook("l1").status.success());
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(w.log(), "");
}

/// A worker that still holds its claim is not finished.
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_with_a_claim_is_not_finished() {
    let (api, w) = riff_with_a_worker().await;
    let thread = "como-technologies/riff".parse().unwrap();
    api.claim(&w.uri("w1"), &thread, "issue-12").await.unwrap();
    let out = w.next("w1", true);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(stderr(&out).contains("you still hold issue-12"), "{out:?}");
    assert!(!find_mark(w.run.path()));
}
