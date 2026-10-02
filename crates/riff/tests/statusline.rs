//! `riff statusline` shows the short session ID and the claims of the
//! Claude Code session on stdin (01M3JDWA0WZWKF3JT3NYA2FV5Z). It never
//! fails. It adds a tag when the riff runs a newer release
//! (01M3NT6X22A4GNFTNKRYV8Z4N1, 01M3NJCWDN5APKZ3Z53XQR8P0B). It asks
//! riff-server with `GET /v1/me`, not `who` (01M3T5GFVS8NMA992KHZN4VE17).

use axum::http::HeaderValue;
use axum::response::Response;
use isolated::Isolated;
use riff_core::build::{Build, HEADER};
use std::path::Path;
use std::process::Command as Git;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const ID: &str = "a6cf2205-d54a-4c1e-9b1f-2e3d4c5b6a7f";

async fn start_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    format!("http://{addr}")
}

/// A real riff-server whose replies name the build `build`.
async fn server_of(build: Build) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let value = HeaderValue::from_str(&build.to_string()).unwrap();
    let router =
        riff_server::router().layer(axum::middleware::map_response(move |mut r: Response| {
            r.headers_mut().insert(HEADER, value.clone());
            std::future::ready(r)
        }));
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

/// This build with the version `major.minor.0`.
fn release(major: u64, minor: u64) -> Build {
    Build {
        version: format!("{major}.{minor}.0"),
        ..Build::this()
    }
}

/// The release after this one, on the same major.
fn newer() -> Build {
    let v = Build::this().semver().unwrap();
    release(v.major, v.minor + 1)
}

/// A git repository with a GitHub origin.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q"][..],
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/como-technologies/riff.git",
        ][..],
    ] {
        assert!(
            Git::new("git")
                .args(args)
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
    }
    dir
}

/// `riff ARGS` in `dir` with `stdin`, with no session in the
/// environment unless `session` is given. Returns stdout and the exit
/// code.
async fn riff(
    server: &str,
    dir: &Path,
    session: Option<&str>,
    stdin: &str,
    args: &[&str],
) -> (String, i32) {
    let mut cmd = Isolated::shared().assert_riff();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env("RIFF_HOME", dir)
        .write_stdin(stdin);
    if let Some(id) = session {
        cmd.env("RIFF_SESSION", id);
    }
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    (
        String::from_utf8(out.stdout).unwrap(),
        out.status.code().unwrap(),
    )
}

#[tokio::test]
async fn the_status_line_shows_the_session_and_its_claims() {
    let server = start_server().await;
    let dir = repo();
    // A new riff is paused. The person resumes it.
    let (_, code) = riff(&server, dir.path(), None, "", &["resume", "--riff"]).await;
    assert_eq!(code, 0);
    let (_, code) = riff(&server, dir.path(), Some(ID), "", &["lead"]).await;
    assert_eq!(code, 0);
    let (_, code) = riff(&server, dir.path(), Some(ID), "", &["claim", "issue-82"]).await;
    assert_eq!(code, 0);

    let stdin = format!(r#"{{"session_id":"{ID}","cwd":"/x"}}"#);
    let (out, code) = riff(&server, dir.path(), None, &stdin, &["statusline"]).await;
    // The session is the lead of mike in the repository.
    assert_eq!(out, "riff a6cf2205 lead issue-82\n");
    assert_eq!(code, 0);
}

#[tokio::test]
async fn without_a_server_the_status_line_still_shows_the_session() {
    let dir = repo();
    let stdin = format!(r#"{{"session_id":"{ID}"}}"#);
    let started = std::time::Instant::now();
    let (out, code) = riff(
        "http://127.0.0.1:1",
        dir.path(),
        None,
        &stdin,
        &["statusline"],
    )
    .await;
    assert_eq!(out, "riff a6cf2205 (not in the riff)\n");
    assert_eq!(code, 0);
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[tokio::test]
async fn without_a_session_the_status_line_says_so() {
    let dir = repo();
    let (out, code) = riff("http://127.0.0.1:1", dir.path(), None, "", &["statusline"]).await;
    assert_eq!(out, "riff: no session\n");
    assert_eq!(code, 0);
}

/// The status line of the session [`ID`] of mike at `server`.
async fn line(server: &str, dir: &Path) -> String {
    let stdin = format!(r#"{{"session_id":"{ID}","cwd":"/x"}}"#);
    let (out, code) = riff(server, dir, None, &stdin, &["statusline"]).await;
    assert_eq!(code, 0);
    out
}

/// Resumes the riff at `server`, and the session [`ID`] joins it as
/// the lead.
async fn join(server: &str, dir: &Path) {
    let (_, code) = riff(server, dir, None, "", &["resume", "--riff"]).await;
    assert_eq!(code, 0);
    let (_, code) = riff(server, dir, Some(ID), "", &["lead"]).await;
    assert_eq!(code, 0);
    let (_, code) = riff(server, dir, Some(ID), "", &["status", "work"]).await;
    assert_eq!(code, 0);
}

/// A real riff-server that records the path of each call in `calls`.
async fn counting_server(calls: Arc<Mutex<Vec<String>>>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = riff_server::router().layer(axum::middleware::map_request(
        move |r: axum::extract::Request| {
            calls.lock().unwrap().push(r.uri().path().to_owned());
            std::future::ready(r)
        },
    ));
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

#[tokio::test]
async fn the_status_line_calls_me_and_not_who() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let server = counting_server(calls.clone()).await;
    let dir = repo();
    join(&server, dir.path()).await;
    let (_, code) = riff(&server, dir.path(), Some(ID), "", &["claim", "issue-82"]).await;
    assert_eq!(code, 0);
    calls.lock().unwrap().clear();

    assert_eq!(
        line(&server, dir.path()).await,
        "riff a6cf2205 lead issue-82\n"
    );
    let calls = calls.lock().unwrap().clone();
    assert!(calls.iter().any(|c| c == "/v1/me"), "{calls:?}");
    assert!(!calls.iter().any(|c| c == "/v1/who"), "{calls:?}");
}

#[tokio::test]
async fn a_newer_release_gives_the_tag_and_the_same_release_none() {
    let dir = repo();
    let server = server_of(newer()).await;
    join(&server, dir.path()).await;
    assert_eq!(
        line(&server, dir.path()).await,
        format!(
            "riff a6cf2205 lead update v{}: riff update\n",
            newer().version
        )
    );

    // Another commit of the same release: a dev build or a loopback riff.
    let dev = Build {
        commit: "0123456789ab".into(),
        ..Build::this()
    };
    let dir = repo();
    let server = server_of(dev).await;
    join(&server, dir.path()).await;
    assert_eq!(line(&server, dir.path()).await, "riff a6cf2205 lead\n");
}

#[tokio::test]
async fn a_release_that_riff_cannot_talk_to_still_gives_the_tag() {
    let dir = repo();
    let v = Build::this().semver().unwrap();
    let far = release(v.major, v.minor + 2);
    let server = server_of(far.clone()).await;
    assert_eq!(
        line(&server, dir.path()).await,
        format!(
            "riff a6cf2205 (not in the riff) update v{}: riff update\n",
            far.version
        )
    );
}

#[tokio::test]
async fn with_update_auto_on_the_tag_says_updating_then_installed() {
    let dir = repo();
    let state = dir.path().join("state");
    let server = server_of(newer()).await;
    join(&server, dir.path()).await;

    // The update by itself runs: it holds the update lock. So no riff
    // of this test starts a real update.
    let lock = riff::local::update(&state).unwrap().expect("free");
    std::fs::write(dir.path().join("config.toml"), "[update]\nauto = true\n").unwrap();
    assert_eq!(
        line(&server, dir.path()).await,
        format!("riff a6cf2205 lead updating to v{}\n", newer().version)
    );
    drop(lock);

    // It installed the release of the server. The riff mcp of the
    // session (here: this test, the parent of riff statusline) still
    // runs an older one.
    let v = Build::this().semver().unwrap();
    assert!(v.minor > 0, "this riff has no older release");
    let build_file = state.join(format!("build-{}", std::process::id()));
    let mcp = riff::local::record(&state, std::process::id(), ID)
        .unwrap()
        .expect("free");
    std::fs::write(&build_file, release(v.major, v.minor - 1).to_string()).unwrap();
    let server = server_of(Build::this()).await;
    join(&server, dir.path()).await;
    assert_eq!(
        line(&server, dir.path()).await,
        format!("riff a6cf2205 lead v{} installed\n", Build::this().version)
    );

    // riff mcp runs the new release: no tag.
    std::fs::write(&build_file, Build::this().to_string()).unwrap();
    assert_eq!(line(&server, dir.path()).await, "riff a6cf2205 lead\n");
    drop(mcp);
}

#[tokio::test]
async fn a_server_that_does_not_answer_in_time_gives_no_tag_in_time() {
    let dir = repo();
    // It takes each connection and never answers.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let mut open = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            open.push(socket);
        }
    });
    let started = Instant::now();
    assert_eq!(
        line(&format!("http://{addr}"), dir.path()).await,
        "riff a6cf2205 (not in the riff)\n"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}
