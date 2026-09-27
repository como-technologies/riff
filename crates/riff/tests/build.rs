//! `riff` and a `riff-server` of another build (01M3JEE7KZR5VVJGZQD82AA6NH
//! to 01M3JEE7WT04BKX377VW5GDSPY). A fake server names an older build, a
//! newer build, or no build. A real server has the same build.

use std::path::Path;
use std::process::{Command, Output};

use axum::http::HeaderValue;
use axum::response::Response;
use riff_core::build::{Build, HEADER, VERSION};

/// A build with another commit, at `time`.
fn other(time: &str) -> Build {
    Build {
        commit: "0000deadbeef".into(),
        time: time.into(),
        ..Build::this()
    }
}

/// A fake server that answers each call with 200 and names `build`, or
/// no build.
async fn fake(build: Option<Build>) -> String {
    let stamp = move |mut r: Response| {
        let build = build.clone();
        async move {
            if let Some(b) = build {
                let value = HeaderValue::from_str(&b.to_string()).unwrap();
                r.headers_mut().insert(HEADER, value);
            }
            r
        }
    };
    let router = axum::Router::new()
        .fallback(|| async { "{}" })
        .layer(axum::middleware::map_response(stamp));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

/// A real riff-server of this build.
async fn real() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    format!("http://{addr}")
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
        .env("XDG_RUNTIME_DIR", dir);
    cmd
}

/// Runs `cmd` away from the runtime of the fake server.
async fn run(mut cmd: Command) -> Output {
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// `join`, `post` and `read` each fail, and name both builds and the
/// side to update (01M3JEE7RDTDD3KQMKH41E8D57).
async fn each_call_fails(server: Option<Build>, step: &str) {
    let url = fake(server.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let theirs = server.map_or("a build from before the check".into(), |b| b.to_string());
    for args in [
        &["read"][..],
        &["post", "--thread", "t", "hi"],
        &["read", "--thread", "t"],
    ] {
        let out = run(riff(&url, dir.path(), args)).await;
        let err = text(&out.stderr);
        assert!(!out.status.success(), "{args:?}: {err}");
        assert!(err.contains(VERSION), "{args:?}: {err}");
        assert!(err.contains(&theirs), "{args:?}: {err}");
        assert!(err.contains(step), "{args:?}: {err}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_newer_riff_is_refused_and_names_the_server_to_update() {
    each_call_fails(Some(other("2000-01-01T00:00:00Z")), "Update riff-server").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_older_riff_is_refused_and_names_riff_to_update() {
    each_call_fails(
        Some(other("2999-01-01T00:00:00Z")),
        "Update riff on this machine",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_with_no_build_is_refused() {
    each_call_fails(None, "Update riff-server").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_same_build_works_and_who_and_whoami_show_it() {
    let url = real().await;
    let dir = tempfile::tempdir().unwrap();
    for args in [&["whoami"][..], &["who"]] {
        let out = run(riff(&url, dir.path(), args)).await;
        assert!(out.status.success(), "{args:?}: {}", text(&out.stderr));
        let line = format!("riff and riff-server have the build {VERSION}.");
        assert!(text(&out.stdout).contains(&line), "{}", text(&out.stdout));
    }
}

/// 01M3JEE7TPZMNK7X6JXJ7GWFPP
#[tokio::test(flavor = "multi_thread")]
async fn the_start_hook_tells_the_session_of_the_mismatch() {
    let url = fake(Some(other("2000-01-01T00:00:00Z"))).await;
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = riff(&url, dir.path(), &["hook", "session-start"]);
    cmd.env_remove("RIFF_SESSION")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    let out = tokio::task::spawn_blocking(move || {
        use std::io::Write;
        let mut child = cmd.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(
                br#"{"session_id":"b2","source":"startup","hook_event_name":"SessionStart"}"#,
            )
            .unwrap();
        child.wait_with_output().unwrap()
    })
    .await
    .unwrap();
    let out: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let context = out["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("do not match"), "{context}");
    assert!(context.contains(VERSION), "{context}");
    assert!(context.contains("Tell your user now"), "{context}");
    assert!(!context.contains("riff watch --once"), "{context}");
}

/// The watch prints the mismatch and stops, so that the session wakes.
#[tokio::test(flavor = "multi_thread")]
async fn the_watch_prints_the_mismatch_and_stops() {
    let url = fake(None).await;
    let dir = tempfile::tempdir().unwrap();
    let out = run(riff(&url, dir.path(), &["watch", "--once"])).await;
    assert_eq!(out.status.code(), Some(1));
    let stdout = text(&out.stdout);
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    assert!(stdout.contains("do not match"), "{stdout}");
}

/// The error links to the book part that tells how to update, and the
/// book shows how to see the build.
#[test]
fn the_book_has_the_part_that_the_error_names() {
    let book = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let anchor = riff_core::build::UPDATE_URL
        .rsplit_once('#')
        .unwrap()
        .1
        .replace('-', " ");
    let heading = format!("### {}{}\n", anchor[..1].to_uppercase(), &anchor[1..]);
    assert!(book.contains(&heading), "no {heading:?}");
    assert!(book.contains("```sh\nriff --version\nriff-server --version\n```"));
}
