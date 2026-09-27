//! `riff watch` and `riff tail` connect again when the server ends a
//! stream (R131). `riff post` tries again while the server replies 503
//! (R132). `riff watch --once` exits after one wake (R170). A fake
//! server ends each stream after one event.

use std::convert::Infallible;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures::Stream;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{Kind, Message, Posted, Tailed, Wake};

const WAIT: Duration = Duration::from_secs(10);

/// How many times the fake server got each call.
#[derive(Default)]
struct Calls {
    watch: AtomicU64,
    tail: AtomicU64,
    post: AtomicU64,
}

type Shared = Arc<Calls>;

fn mike() -> SessionUri {
    "riff://mike@pangolin/como-technologies/riff?session=a1"
        .parse()
        .unwrap()
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

/// One event, then the end of the stream.
fn once(event: impl serde::Serialize) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let event = Event::default().json_data(event).unwrap();
    Sse::new(futures::stream::iter([Ok(event)]))
}

async fn watch(State(calls): State<Shared>) -> impl IntoResponse {
    let seq = calls.watch.fetch_add(1, Ordering::SeqCst) + 1;
    once(Wake {
        thread: repo(),
        seq,
        from: mike(),
        kind: Kind::Message,
    })
}

async fn tail(State(calls): State<Shared>) -> impl IntoResponse {
    let seq = calls.tail.fetch_add(1, Ordering::SeqCst) + 1;
    let message = Message {
        seq,
        from: mike(),
        to: Vec::new(),
        body: format!("stream {seq}"),
        at_ms: 0,
        kind: Kind::Message,
        sig: None,
    };
    once(Tailed {
        thread: repo(),
        message,
        keys: Default::default(),
    })
}

/// 503 twice, then the post goes through.
async fn post_busy(State(calls): State<Shared>) -> Response {
    if calls.post.fetch_add(1, Ordering::SeqCst) < 2 {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Json(Posted {
        thread: repo(),
        seq: 7,
        woken: Vec::new(),
        unmatched: Vec::new(),
    })
    .into_response()
}

async fn start_fake() -> (String, Shared) {
    let calls = Shared::default();
    let router = axum::Router::new()
        .route("/v1/watch", get(watch))
        .route("/v1/tail", get(tail))
        .route("/v1/post", post(post_busy))
        .with_state(calls.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{addr}"), calls)
}

fn riff(server: &str, dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin("riff"));
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "brett")
        .env("RIFF_HOST", "heron")
        .env("RIFF_SESSION", "b2")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        // Keep the watch lock away from other test runs (R169).
        .env("XDG_RUNTIME_DIR", dir);
    cmd
}

/// Runs a command that does not stop, and gives its first `n` lines on
/// stdout.
async fn first_lines(mut cmd: Command, n: usize) -> Vec<String> {
    tokio::task::spawn_blocking(move || {
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let lines: Result<Vec<String>, _> = (0..n).map(|_| rx.recv_timeout(WAIT)).collect();
        child.kill().unwrap();
        child.wait().unwrap();
        lines.expect("the command stopped too soon")
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn watch_connects_again_when_the_server_ends_the_stream() {
    let (server, calls) = start_fake().await;
    let dir = tempfile::tempdir().unwrap();

    let lines = first_lines(riff(&server, dir.path(), &["watch"]), 3).await;

    for (line, seq) in lines.iter().zip(1..) {
        assert!(line.contains(&format!("(message {seq})")), "{line}");
    }
    assert!(calls.watch.load(Ordering::SeqCst) >= 3);
}

#[tokio::test]
async fn watch_once_exits_after_the_first_wake() {
    let (server, calls) = start_fake().await;
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = riff(&server, dir.path(), &["watch", "--once"]);

    let out = tokio::time::timeout(
        WAIT,
        tokio::task::spawn_blocking(move || cmd.output().unwrap()),
    )
    .await
    .expect("riff watch --once did not exit")
    .unwrap();

    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    assert!(stdout.contains("(message 1)"), "{stdout}");
    assert_eq!(calls.watch.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn tail_connects_again_when_the_server_ends_the_stream() {
    let (server, calls) = start_fake().await;
    let dir = tempfile::tempdir().unwrap();
    let args = ["tail", "como-technologies/riff"];

    let lines = first_lines(riff(&server, dir.path(), &args), 3).await;

    for (line, seq) in lines.iter().zip(1..) {
        assert!(line.ends_with(&format!("stream {seq}")), "{line}");
    }
    assert!(calls.tail.load(Ordering::SeqCst) >= 3);
}

#[tokio::test]
async fn post_tries_again_while_the_server_replies_503() {
    let (server, calls) = start_fake().await;
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = riff(
        &server,
        dir.path(),
        &["post", "--thread", "como-technologies/riff", "hello"],
    );

    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();

    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Posted message 7"), "{stdout}");
    assert_eq!(calls.post.load(Ordering::SeqCst), 3);
}
