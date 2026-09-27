//! `riff statusline` shows the short session ID and the claims of the
//! Claude Code session on stdin (01M3JDWA0WZWKF3JT3NYA2FV5Z). It never
//! fails.

use std::path::Path;
use std::process::Command as Git;

use assert_cmd::Command;

const ID: &str = "a6cf2205-d54a-4c1e-9b1f-2e3d4c5b6a7f";

async fn start_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    format!("http://{addr}")
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
    let mut cmd = Command::cargo_bin("riff").unwrap();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env("XDG_RUNTIME_DIR", dir)
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
    let (_, code) = riff(&server, dir.path(), None, "", &["resume"]).await;
    assert_eq!(code, 0);
    let (_, code) = riff(&server, dir.path(), Some(ID), "", &["claim", "issue-82"]).await;
    assert_eq!(code, 0);

    let stdin = format!(r#"{{"session_id":"{ID}","cwd":"/x"}}"#);
    let (out, code) = riff(&server, dir.path(), None, &stdin, &["statusline"]).await;
    // The first session of mike in the repository is his lead (R176).
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
