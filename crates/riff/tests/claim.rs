//! `riff claim` and `riff release` from the command line, in the
//! repository thread, against a real server.

use std::path::Path;
use std::process::Command as Git;

use assert_cmd::Command;

async fn start_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    format!("http://{addr}")
}

/// A git repository with a GitHub origin, so the default thread is
/// `como-technologies/riff`.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(
        dir.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/como-technologies/riff.git",
        ],
    );
    dir
}

fn git(dir: &Path, args: &[&str]) {
    assert!(
        Git::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success()
    );
}

/// Runs `riff` as one user in `dir`. Returns stdout and the exit code.
async fn riff(server: &str, dir: &Path, user: &str, args: &[&str]) -> (String, i32) {
    let mut cmd = Command::cargo_bin("riff").unwrap();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", user)
        .env("RIFF_HOST", "pangolin");
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    (
        String::from_utf8(out.stdout).unwrap(),
        out.status.code().unwrap(),
    )
}

#[tokio::test]
async fn claim_and_release_work_in_the_repository_thread() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();

    let (out, code) = riff(&server, dir, "mike", &["claim", "issue-12"]).await;
    assert_eq!(out, "You hold issue-12 in como-technologies/riff.\n");
    assert_eq!(code, 0);

    let (out, code) = riff(&server, dir, "brett", &["claim", "issue-12"]).await;
    assert_eq!(
        out,
        "mike@pangolin:riff holds issue-12 in como-technologies/riff.\n"
    );
    assert_eq!(code, 1, "a held item must fail the command");

    let (_, code) = riff(&server, dir, "brett", &["release", "issue-12"]).await;
    assert_ne!(code, 0, "only the holder can release");

    let (out, code) = riff(&server, dir, "mike", &["release", "issue-12"]).await;
    assert_eq!(out, "You released issue-12 in como-technologies/riff.\n");
    assert_eq!(code, 0);

    let (_, code) = riff(&server, dir, "brett", &["claim", "issue-12"]).await;
    assert_eq!(code, 0);
}

#[tokio::test]
async fn claim_takes_a_named_thread() {
    let server = start_server().await;
    let dir = repo();

    let (out, code) = riff(
        &server,
        dir.path(),
        "mike",
        &["claim", "--thread", "api-v2", "issue-12"],
    )
    .await;
    assert_eq!(out, "You hold issue-12 in api-v2.\n");
    assert_eq!(code, 0);
}
